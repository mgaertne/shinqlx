use core::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use anyhow::Result;
use arzmq::{
    prelude::{
        ContextBuilder, DealerBuilder, DealerSocket, Message, MonitorFlags, MonitorReceiver,
        MonitorSocket, MonitorSocketEvent, Receiver, SendFlags, Sender, Socket, SocketBuilder,
    },
    security::SecurityMechanism,
};
use tokio::{
    select,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    time::{MissedTickBehavior, interval},
};
use uuid::Uuid;

use crate::{
    CONTINUE_RUNNING,
    cmd_line::CommandLineOptions,
    fetch::{FETCH_COMMANDS, Feed, FetchState, Fetched},
    learn::learn_line,
    linefeed_helper::{remember, save_completions},
};

/// One poll per server frame at 40 frames per second keeps the console close to real time without a busy loop.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

struct MonitoredDealer {
    dealer: DealerSocket,
    monitor: MonitorSocket,
}

impl MonitoredDealer {
    fn new() -> Result<Self> {
        let context = ContextBuilder::default()
            .blocky(false)
            .max_sockets(10)
            .io_threads(2)
            .build()?;

        let dealer = Socket::from_context(&context)?;
        let monitor = dealer.monitor(
            MonitorFlags::Connected
                | MonitorFlags::HandshakeSucceeded
                | MonitorFlags::HandshakeFailedAuth
                | MonitorFlags::HandshakeFailedProtocol
                | MonitorFlags::HandshakeFailedNoDetail
                | MonitorFlags::MonitorStopped
                | MonitorFlags::Disconnected
                | MonitorFlags::Closed,
        )?;

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
            .security_mechanism(SecurityMechanism::Plain {
                username: "rcon".into(),
                password: password.into(),
            })
            .immediate(true)
            .receive_timeout(0)
            .receive_highwater_mark(0)
            .send_timeout(0)
            .send_highwater_mark(0)
            .heartbeat_interval(600_000)
            .heartbeat_timeout(600_000)
            .zap_domain("rcon");

        let dealer_config = DealerBuilder::default()
            .socket_builder(socket_builder)
            .routing_id(identity_str)
            .hello_message("register");

        dealer_config.apply(&self.dealer)?;

        Ok(())
    }

    fn connect(&self, address: &str) -> Result<()> {
        self.dealer.connect(address)?;

        Ok(())
    }

    fn disconnect(&self) -> Result<()> {
        let last_endpoint = self.dealer.last_endpoint()?;
        self.dealer.disconnect(&last_endpoint)?;

        Ok(())
    }

    async fn send<M: Into<Message>, F: Into<SendFlags>>(&self, msg: M, flags: F) -> Option<()> {
        self.dealer.send_msg_async(msg.into(), flags.into()).await?;

        Some(())
    }

    async fn recv_msg(&self) -> Option<Message> {
        self.dealer.recv_msg_async().await
    }

    async fn check_monitor(&self) -> Option<MonitorSocketEvent> {
        self.monitor.recv_monitor_event_async().await
    }
}

fn trim_ql_msg(msg: &str) -> String {
    msg.replace("\n", "")
        .replace("\\n", "")
        .replace('\u{0019}', "")
}

static FIRST_TIME: AtomicBool = AtomicBool::new(true);

async fn handle_monitor_event(
    event: MonitorSocketEvent,
    monitored_dealer: &MonitoredDealer,
    sender: &UnboundedSender<String>,
    endpoint: &str,
    fetch: &mut FetchState,
) -> Result<()> {
    match event {
        MonitorSocketEvent::Connected => {
            if FIRST_TIME.load(Ordering::Acquire) {
                FIRST_TIME.store(false, Ordering::Release);
                sender.send("ZMQ registering with the server.".to_string())?;
            }
        }

        MonitorSocketEvent::HandshakeSucceeded => {
            FIRST_TIME.store(true, Ordering::Release);
            sender.send(format!("ZMQ connected to {}.", endpoint))?;
            if fetch.handshake_succeeded() {
                for command in FETCH_COMMANDS {
                    monitored_dealer.send(command, SendFlags::DONT_WAIT).await;
                }
            }
        }

        MonitorSocketEvent::HandshakeFailedAuth(_)
        | MonitorSocketEvent::HandshakeFailedProtocol(_)
        | MonitorSocketEvent::HandshakeFailedNoDetail(_)
        | MonitorSocketEvent::MonitorStopped => {
            sender.send(format!("ZMQ socket error: {event:?}"))?;
            CONTINUE_RUNNING.store(false, Ordering::Release);
        }

        MonitorSocketEvent::Disconnected | MonitorSocketEvent::Closed => {
            if FIRST_TIME.load(Ordering::Acquire) {
                FIRST_TIME.store(false, Ordering::Release);
                sender.send("Reconnecting ZMQ...".to_string())?;
            }
            if let Err(e) = monitored_dealer.connect(endpoint) {
                sender.send(format!("error reconnecting: {e:?}."))?;
            }
        }

        event => {
            sender.send(format!("ZMQ socket error: {event:?}",))?;
        }
    };

    Ok(())
}

/// Handles all monitor events that are waiting.
async fn check_monitor(
    monitored_dealer: &MonitoredDealer,
    sender: &UnboundedSender<String>,
    endpoint: &str,
    fetch: &mut FetchState,
) -> Result<()> {
    while let Some(event) = monitored_dealer.check_monitor().await {
        handle_monitor_event(event, monitored_dealer, sender, endpoint, fetch).await?;
    }

    Ok(())
}

fn store_fetched_names(
    args: &CommandLineOptions,
    fetched: &Fetched,
    sender: &UnboundedSender<String>,
) -> Result<()> {
    let message = match save_completions(args, fetched) {
        Ok(file) => format!(
            "ZMQ fetched {} names for the completion into {}, they are used for the completion now.",
            fetched.names.len() + fetched.maps.len(),
            file.display()
        ),
        Err(err) => format!("could not keep the fetched names: {err}"),
    };
    sender.send(message)?;

    Ok(())
}

pub(crate) async fn run_zmq(
    args: CommandLineOptions,
    mut zmq_receiver: UnboundedReceiver<String>,
    display_sender: UnboundedSender<String>,
) -> Result<()> {
    display_sender.send(format!("ZMQ connecting to {}...", args.host))?;

    let monitored_dealer = MonitoredDealer::new()?;
    monitored_dealer.configure(&args.password, &args.identity)?;

    monitored_dealer.connect(&args.host)?;

    let mut fetch = FetchState::new(args.fetch_completions);

    let mut poll = interval(POLL_INTERVAL);
    poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

    while CONTINUE_RUNNING.load(Ordering::Acquire) && !zmq_receiver.is_closed() {
        select!(
            biased;

            Some(line) = zmq_receiver.recv() => {
                monitored_dealer.send(&line, SendFlags::DONT_WAIT).await;
            },

            _ = poll.tick() => {
                while let Some(zmq_msg) = monitored_dealer.recv_msg().await {
                    let zmq_str = trim_ql_msg(&zmq_msg.to_string());
                    if let Some(learned) = learn_line(&zmq_str) {
                        remember(&args, &learned).ok();
                    }
                    match fetch.feed(&zmq_str) {
                        Feed::Show => display_sender.send(zmq_str)?,
                        Feed::Hide => (),
                        Feed::Done(fetched) => store_fetched_names(&args, &fetched, &display_sender)?,
                    }
                }

                if let Some(fetched) = fetch.expired() {
                    store_fetched_names(&args, &fetched, &display_sender)?;
                }

                check_monitor(&monitored_dealer, &display_sender, &args.host, &mut fetch).await?;
            }
        );
    }

    drop(zmq_receiver);

    monitored_dealer.disconnect()?;

    drop(display_sender);

    Ok(())
}

#[cfg(test)]
mod zmq_helper_tests {
    use core::{sync::atomic::Ordering, time::Duration};

    use arzmq::prelude::{ContextBuilder, RouterSocket, Socket};
    use termcolor::ColorChoice;
    use tokio::{sync::mpsc::unbounded_channel, time::timeout};

    use super::{CONTINUE_RUNNING, CommandLineOptions, run_zmq, trim_ql_msg};

    #[test]
    fn trim_ql_msg_removes_line_breaks_and_the_quake_marker() {
        assert_eq!(trim_ql_msg("a\nb"), "ab");
        assert_eq!(trim_ql_msg("a\\nb"), "ab");
        assert_eq!(trim_ql_msg("\u{19}hello"), "hello");
        assert_eq!(trim_ql_msg("unchanged"), "unchanged");
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
        let task = run_zmq(
            CommandLineOptions {
                host: endpoint.clone(),
                password: "secret".to_string(),
                identity: "tester".to_string(),
                color: ColorChoice::Never,
                fetch_completions: false,
            },
            zmq_receiver,
            display_sender,
        );

        let result = timeout(Duration::from_secs(10), task).await;

        assert!(
            matches!(result, Ok(Ok(()))),
            "run_zmq did not end cleanly: {result:?}"
        );
        assert!(!CONTINUE_RUNNING.load(Ordering::Acquire));
        let mut lines = Vec::new();
        while let Ok(line) = display_receiver.try_recv() {
            lines.push(line);
        }
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
