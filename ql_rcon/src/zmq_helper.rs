use std::{io, path::PathBuf};

use anyhow::Result;
use arzmq::{
    prelude::{
        DealerBuilder, DealerSocket, Message, MonitorReceiver, MonitorSocket, Receiver, SendFlags,
        Sender, Socket, SocketBuilder,
    },
    security::SecurityMechanism,
};
use ql_zmq::{LinkEvent, LinkState, POLL_INTERVAL, Reconnect, context, monitor_flags};
use tokio::{
    select,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::{MissedTickBehavior, interval},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    cmd_line::CommandLineOptions,
    fetch::{FETCH_COMMANDS, FLAG_LEGEND, Feed, FetchState, Fetched},
    guide::local_help,
    learn::{Learned, Learner, explained},
    line_editor::{SessionFiles, remember, save_completions},
};

/// What the console needs from the connection to the server, so that the loop does not depend on ZMQ.
trait Transport: Reconnect {
    async fn send(&self, line: &str);
    async fn receive(&self) -> Option<String>;
    async fn next_link_event(&self) -> Option<LinkEvent>;
}

/// Where what the console learns about the server is kept.
trait CompletionStore {
    fn remember(&self, learned: &Learned) -> io::Result<()>;
    fn save_completions(&self, fetched: &Fetched) -> io::Result<PathBuf>;
}

impl CompletionStore for SessionFiles {
    fn remember(&self, learned: &Learned) -> io::Result<()> {
        remember(self, learned)
    }

    fn save_completions(&self, fetched: &Fetched) -> io::Result<PathBuf> {
        save_completions(self, fetched)
    }
}

struct MonitoredDealer {
    dealer: DealerSocket,
    monitor: MonitorSocket,
}

impl MonitoredDealer {
    fn new() -> Result<Self> {
        let context = context(2)?;

        let dealer = Socket::from_context(&context)?;
        let monitor = dealer.monitor(monitor_flags(true))?;

        Ok(Self { dealer, monitor })
    }

    fn configure(&self, password: &str, identity: &str) -> Result<()> {
        let identity_str = if identity.is_empty() {
            let identity = Uuid::new_v4();
            identity.to_string().replace("-", "")
        } else {
            identity.to_string()
        };

        let socket_builder = SocketBuilder::default()
            .immediate(true)
            .receive_timeout(0)
            .receive_highwater_mark(0)
            .send_timeout(0)
            .send_highwater_mark(0)
            .heartbeat_interval(600_000)
            .heartbeat_timeout(600_000);
        let socket_builder = if password.is_empty() {
            socket_builder
        } else {
            socket_builder
                .zap_domain("rcon")
                .security_mechanism(SecurityMechanism::Plain {
                    username: "rcon".into(),
                    password: password.into(),
                })
        };

        let dealer_config = DealerBuilder::default()
            .socket_builder(socket_builder)
            .routing_id(identity_str)
            .hello_message("register");

        dealer_config.apply(&self.dealer)?;

        Ok(())
    }

    fn disconnect(&self) -> Result<()> {
        let last_endpoint = self.dealer.last_endpoint()?;
        self.dealer.disconnect(&last_endpoint)?;

        Ok(())
    }
}

impl Transport for MonitoredDealer {
    async fn send(&self, line: &str) {
        self.dealer
            .send_msg_async(Message::from(line), SendFlags::DONT_WAIT)
            .await;
    }

    async fn receive(&self) -> Option<String> {
        self.dealer
            .recv_msg_async()
            .await
            .map(|message| message.to_string())
    }

    async fn next_link_event(&self) -> Option<LinkEvent> {
        self.monitor
            .recv_monitor_event_async()
            .await
            .map(LinkEvent::from)
    }
}

impl Reconnect for MonitoredDealer {
    fn connect(&self, endpoint: &str) -> Result<()> {
        self.dealer.connect(endpoint)?;

        Ok(())
    }
}

fn trim_ql_msg(msg: &str) -> String {
    msg.replace("\n", "")
        .replace("\\n", "")
        .replace('\u{0019}', "")
}

/// Handles all link events that are waiting and asks for the completion lists after a handshake.
async fn check_link<T: Transport>(
    transport: &T,
    sender: &UnboundedSender<String>,
    endpoint: &str,
    fetch: &mut FetchState,
    link: &mut LinkState,
    running: &CancellationToken,
) -> Result<()> {
    while let Some(event) = transport.next_link_event().await {
        if link.handle(event, transport, sender, endpoint, running)? && fetch.handshake_succeeded()
        {
            for command in FETCH_COMMANDS {
                transport.send(command).await;
            }
        }
    }

    Ok(())
}

fn store_fetched_names<S: CompletionStore>(
    store: &S,
    fetched: &Fetched,
    sender: &UnboundedSender<String>,
) -> Result<()> {
    let message = match store.save_completions(fetched) {
        Ok(file) => format!(
            "ZMQ fetched {} names for the completion into {}, \
            they are used for the completion now.",
            fetched.names.len() + fetched.maps.len(),
            file.display()
        ),
        Err(err) => format!("could not keep the fetched names: {err}"),
    };
    sender.send(message)?;

    Ok(())
}

/// Shows a line of the server, keeps what it teaches the console and what the completion lists fetch.
fn handle_reply<S: CompletionStore>(
    text: String,
    learner: &mut Learner,
    fetch: &mut FetchState,
    store: &S,
    sender: &UnboundedSender<String>,
) -> Result<()> {
    if let Some(learned) = learner.feed(&text) {
        store.remember(&learned).ok();
    }
    match fetch.feed(&text) {
        Feed::Show => {
            let explanation = explained(&text);
            sender.send(text)?;
            if let Some(explanation) = explanation {
                sender.send(explanation)?;
            }
        }
        Feed::Hide => (),
        Feed::Replace(line) => sender.send(line)?,
        Feed::Done(fetched) => store_fetched_names(store, &fetched, sender)?,
        Feed::ShowAndDone(fetched) => {
            sender.send(text)?;
            sender.send(FLAG_LEGEND.to_string())?;
            store_fetched_names(store, &fetched, sender)?;
        }
    }

    Ok(())
}

/// Sends what the user typed to the server, unless the console answers it itself.
async fn handle_user_line<T: Transport>(
    line: &str,
    fetch: &mut FetchState,
    transport: &T,
    sender: &UnboundedSender<String>,
) -> Result<()> {
    if let Some(help) = local_help(line) {
        for text in help {
            sender.send((*text).to_string())?;
        }
    } else {
        fetch.user_sent(line);
        transport.send(line).await;
    }

    Ok(())
}

async fn run_console<T: Transport, S: CompletionStore>(
    transport: &T,
    store: &S,
    endpoint: &str,
    fetch_completions: bool,
    zmq_receiver: &mut UnboundedReceiver<String>,
    display_sender: &UnboundedSender<String>,
    running: &CancellationToken,
) -> Result<()> {
    let mut fetch = FetchState::new(fetch_completions);
    let mut learner = Learner::default();
    let mut link = LinkState::default();

    let mut poll = interval(POLL_INTERVAL);
    poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

    while !running.is_cancelled() && !zmq_receiver.is_closed() {
        select!(
            biased;

            () = running.cancelled() => break,

            Some(line) = zmq_receiver.recv() => {
                handle_user_line(&line, &mut fetch, transport, display_sender).await?;
            },

            _ = poll.tick() => {
                while let Some(message) = transport.receive().await {
                    handle_reply(
                        trim_ql_msg(&message),
                        &mut learner,
                        &mut fetch,
                        store,
                        display_sender,
                    )?;
                }

                if let Some(fetched) = fetch.expired() {
                    store_fetched_names(store, &fetched, display_sender)?;
                }

                check_link(transport, display_sender, endpoint, &mut fetch, &mut link, running).await?;
            }
        );
    }

    Ok(())
}

pub(crate) async fn run_zmq(
    args: CommandLineOptions,
    mut zmq_receiver: UnboundedReceiver<String>,
    display_sender: UnboundedSender<String>,
    running: &CancellationToken,
) -> Result<()> {
    display_sender.send(format!("ZMQ connecting to {}...", args.host))?;

    let monitored_dealer = MonitoredDealer::new()?;
    monitored_dealer.configure(&args.password, &args.identity)?;
    monitored_dealer.connect(&args.host)?;

    let result = run_console(
        &monitored_dealer,
        &SessionFiles::from(&args),
        &args.host,
        args.fetch_completions,
        &mut zmq_receiver,
        &display_sender,
        running,
    )
    .await;

    drop(zmq_receiver);

    monitored_dealer.disconnect()?;

    drop(display_sender);

    result
}

#[cfg(test)]
mod zmq_helper_tests {
    use alloc::collections::VecDeque;
    use core::time::Duration;
    use std::{
        io,
        path::PathBuf,
        sync::{Mutex, MutexGuard},
    };

    use anyhow::Result;
    use arzmq::prelude::{ContextBuilder, RouterSocket, Socket};
    use ql_zmq::ColorChoice;
    use tokio::{
        sync::mpsc::{UnboundedReceiver, unbounded_channel},
        time::{sleep, timeout},
    };
    use tokio_util::sync::CancellationToken;

    use super::{
        CommandLineOptions, CompletionStore, FETCH_COMMANDS, FLAG_LEGEND, FetchState, LinkEvent,
        LinkState, MonitoredDealer, Reconnect, SessionFiles, Transport, check_link, handle_reply,
        handle_user_line, run_console, run_zmq, store_fetched_names, trim_ql_msg,
    };
    use crate::{
        fetch::Fetched,
        learn::{Learned, Learner},
    };

    fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex.lock().expect("the recorded values are not poisoned")
    }

    #[derive(Default)]
    struct FakeTransport {
        sent: Mutex<Vec<String>>,
        connected: Mutex<Vec<String>>,
        events: Mutex<VecDeque<LinkEvent>>,
        replies: Mutex<VecDeque<String>>,
    }

    impl Transport for FakeTransport {
        async fn send(&self, line: &str) {
            locked(&self.sent).push(line.to_string());
        }

        async fn receive(&self) -> Option<String> {
            locked(&self.replies).pop_front()
        }

        async fn next_link_event(&self) -> Option<LinkEvent> {
            locked(&self.events).pop_front()
        }
    }

    impl Reconnect for FakeTransport {
        fn connect(&self, endpoint: &str) -> Result<()> {
            locked(&self.connected).push(endpoint.to_string());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeStore {
        remembered: Mutex<Vec<String>>,
        saved: Mutex<Vec<Fetched>>,
    }

    impl CompletionStore for FakeStore {
        fn remember(&self, learned: &Learned) -> io::Result<()> {
            locked(&self.remembered).push(format!("{learned:?}"));
            Ok(())
        }

        fn save_completions(&self, fetched: &Fetched) -> io::Result<PathBuf> {
            locked(&self.saved).push(Fetched {
                names: fetched.names.clone(),
                maps: fetched.maps.clone(),
                plugin_commands: fetched.plugin_commands.clone(),
                flags: fetched.flags.clone(),
            });
            Ok(PathBuf::from("completions.txt"))
        }
    }

    struct FailingStore;

    impl CompletionStore for FailingStore {
        fn remember(&self, _learned: &Learned) -> io::Result<()> {
            Err(io::Error::other("no space left"))
        }

        fn save_completions(&self, _fetched: &Fetched) -> io::Result<PathBuf> {
            Err(io::Error::other("no space left"))
        }
    }

    fn drained(receiver: &mut UnboundedReceiver<String>) -> Vec<String> {
        let mut lines = Vec::new();
        while let Ok(line) = receiver.try_recv() {
            lines.push(line);
        }
        lines
    }

    #[test]
    fn trim_ql_msg_removes_line_breaks_and_the_quake_marker() {
        assert_eq!(trim_ql_msg("a\nb"), "ab");
        assert_eq!(trim_ql_msg("a\\nb"), "ab");
        assert_eq!(trim_ql_msg("\u{19}hello"), "hello");
        assert_eq!(trim_ql_msg("unchanged"), "unchanged");
    }

    #[test]
    fn a_reply_is_shown_and_what_it_teaches_is_remembered() {
        let store = FakeStore::default();
        let (sender, mut receiver) = unbounded_channel();

        handle_reply(
            "usage: dir <directory> [extension]".to_string(),
            &mut Learner::default(),
            &mut FetchState::new(false),
            &store,
            &sender,
        )
        .expect("the reply can be handled");

        assert_eq!(
            drained(&mut receiver),
            ["usage: dir <directory> [extension]"]
        );
        assert_eq!(locked(&store.remembered).len(), 1);
        assert!(locked(&store.saved).is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn help_cvarflags_is_answered_without_the_server() {
        let transport = FakeTransport::default();
        let (display_sender, mut display_receiver) = unbounded_channel();

        handle_user_line(
            "help cvarflags",
            &mut FetchState::new(false),
            &transport,
            &display_sender,
        )
        .await
        .expect("the line can be handled");

        assert!(locked(&transport.sent).is_empty());
        assert!(
            drained(&mut display_receiver)
                .iter()
                .any(|line| line.contains("R  read-only"))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_first_handshake_requests_the_lists_and_later_ones_do_not() {
        let transport = FakeTransport::default();
        locked(&transport.events)
            .extend([LinkEvent::HandshakeSucceeded, LinkEvent::HandshakeSucceeded]);
        let (sender, mut receiver) = unbounded_channel();

        check_link(
            &transport,
            &sender,
            "tcp://127.0.0.1:27961",
            &mut FetchState::new(true),
            &mut LinkState::default(),
            &CancellationToken::new(),
        )
        .await
        .expect("the events can be handled");

        assert_eq!(locked(&transport.sent).as_slice(), FETCH_COMMANDS);
        assert_eq!(
            drained(&mut receiver),
            ["ZMQ connected to tcp://127.0.0.1:27961."; 2]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_lost_connection_is_connected_again() {
        let transport = FakeTransport::default();
        locked(&transport.events).push_back(LinkEvent::Lost);
        let (sender, mut receiver) = unbounded_channel();

        check_link(
            &transport,
            &sender,
            "tcp://127.0.0.1:27961",
            &mut FetchState::new(false),
            &mut LinkState::default(),
            &CancellationToken::new(),
        )
        .await
        .expect("the events can be handled");

        assert_eq!(drained(&mut receiver), ["Reconnecting ZMQ..."]);
        assert_eq!(locked(&transport.connected).len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_typed_line_goes_to_the_server() {
        let transport = FakeTransport::default();
        let (display_sender, mut display_receiver) = unbounded_channel();

        handle_user_line(
            "map campgrounds",
            &mut FetchState::new(false),
            &transport,
            &display_sender,
        )
        .await
        .expect("the line can be handled");

        assert_eq!(locked(&transport.sent).as_slice(), ["map campgrounds"]);
        assert!(drained(&mut display_receiver).is_empty());
    }

    #[test]
    fn a_failing_store_is_reported_instead_of_stopping_the_console() {
        let (sender, mut receiver) = unbounded_channel();
        let fetched = Fetched {
            names: vec!["quad".to_string()],
            maps: Vec::new(),
            plugin_commands: Vec::new(),
            flags: Vec::new(),
        };

        store_fetched_names(&FailingStore, &fetched, &sender).expect("the report can be sent");
        store_fetched_names(&FakeStore::default(), &fetched, &sender)
            .expect("the report can be sent");

        let lines = drained(&mut receiver);
        assert_eq!(lines[0], "could not keep the fetched names: no space left");
        assert_eq!(
            lines[1],
            "ZMQ fetched 1 names for the completion into completions.txt, \
            they are used for the completion now."
        );
    }

    #[test]
    fn a_cvar_answer_is_followed_by_its_explanation() {
        let (sender, mut receiver) = unbounded_channel();
        let answer = "\"g_gametype\" is:\"4^7\" default:\"0^7\"";

        handle_reply(
            answer.to_string(),
            &mut Learner::default(),
            &mut FetchState::new(false),
            &FailingStore,
            &sender,
        )
        .expect("the reply can be handled");

        assert_eq!(drained(&mut receiver), [answer, "  = Clan Arena"]);
    }

    #[test]
    fn the_end_of_a_requested_list_stores_it_and_a_typed_list_command_shows_it() {
        let hidden_store = FakeStore::default();
        let (sender, mut receiver) = unbounded_channel();
        let mut hidden = FetchState::new(true);
        assert!(hidden.handshake_succeeded());
        for line in ["campgrounds.bsp", "3 cvar indexes"] {
            handle_reply(
                line.to_string(),
                &mut Learner::default(),
                &mut hidden,
                &hidden_store,
                &sender,
            )
            .expect("the reply can be handled");
        }
        assert_eq!(locked(&hidden_store.saved).len(), 1);
        assert_eq!(drained(&mut receiver).len(), 1);

        let shown_store = FakeStore::default();
        let mut shown = FetchState::new(false);
        shown.user_sent("cvarlist");
        for line in ["sv_fps \"40\"", "3 cvar indexes"] {
            handle_reply(
                line.to_string(),
                &mut Learner::default(),
                &mut shown,
                &shown_store,
                &sender,
            )
            .expect("the reply can be handled");
        }

        let lines = drained(&mut receiver);
        assert_eq!(lines[0], "sv_fps \"40\"");
        assert_eq!(lines[1], "3 cvar indexes");
        assert_eq!(lines[2], FLAG_LEGEND);
        assert_eq!(locked(&shown_store.saved).len(), 1);
    }

    #[test]
    fn the_session_files_keep_what_the_console_learns() {
        let session = SessionFiles::default();

        assert!(
            session
                .remember(&Learned::Latched("g_gametype".to_string()))
                .is_ok()
        );
        let error = session
            .save_completions(&Fetched::default())
            .expect_err("a session without host has no file for the names");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_console_sends_typed_lines_and_shows_the_replies_of_the_server() {
        let transport = FakeTransport::default();
        locked(&transport.replies)
            .push_back("\u{19}usage: dir <directory> [extension]\n".to_string());
        let store = FakeStore::default();
        let running = CancellationToken::new();
        let (zmq_sender, mut zmq_receiver) = unbounded_channel();
        let (display_sender, mut display_receiver) = unbounded_channel();
        zmq_sender
            .send("map campgrounds".to_string())
            .expect("the console is listening");

        let (result, ()) = tokio::join!(
            run_console(
                &transport,
                &store,
                "tcp://127.0.0.1:27961",
                false,
                &mut zmq_receiver,
                &display_sender,
                &running,
            ),
            async {
                sleep(Duration::from_millis(150)).await;
                running.cancel();
            }
        );

        result.expect("the console ends cleanly");
        assert_eq!(locked(&transport.sent).as_slice(), ["map campgrounds"]);
        assert_eq!(
            drained(&mut display_receiver),
            ["usage: dir <directory> [extension]"]
        );
        assert_eq!(locked(&store.remembered).len(), 1);
    }

    #[test]
    fn an_empty_identity_gets_a_random_one() {
        let dealer = MonitoredDealer::new().expect("the sockets can be created");

        assert!(dealer.configure("", "").is_ok());
    }

    #[test]
    fn a_server_without_password_is_configured_without_plain_security() {
        let dealer = MonitoredDealer::new().expect("the sockets can be created");

        assert!(dealer.configure("secret", "").is_ok());
        assert!(dealer.configure("secret", "tester").is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_zmq_stops_when_the_server_does_not_speak_the_configured_security() {
        let context = ContextBuilder::default()
            .build()
            .expect("a context can be created");
        let router: RouterSocket = Socket::from_context(&context).expect("a router can be created");
        router
            .bind("tcp://127.0.0.1:*")
            .expect("a free port can be bound");
        let endpoint = router.last_endpoint().expect("the endpoint is known");

        let (_zmq_sender, zmq_receiver) = unbounded_channel();
        let (display_sender, mut display_receiver) = unbounded_channel();
        let running = CancellationToken::new();
        let task = run_zmq(
            CommandLineOptions {
                host: endpoint.clone(),
                password: "secret".to_string(),
                identity: "tester".to_string(),
                color: ColorChoice::Never,
                fetch_completions: false,
                timestamp: false,
                profile: None,
            },
            zmq_receiver,
            display_sender,
            &running,
        );

        let result = timeout(Duration::from_secs(10), task).await;

        assert!(
            matches!(result, Ok(Ok(()))),
            "run_zmq did not end cleanly: {result:?}"
        );
        assert!(running.is_cancelled());
        let lines = drained(&mut display_receiver);
        assert_eq!(
            lines.first(),
            Some(&format!("ZMQ connecting to {endpoint}..."))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("ZMQ socket error")),
            "{lines:?}"
        );
    }
}
