use core::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use anyhow::Result;
use arzmq::{
    prelude::{
        ContextBuilder, Message, MonitorFlags, MonitorReceiver, MonitorSocket, MonitorSocketEvent,
        Receiver, Socket, SocketBuilder, SubscribeBuilder, SubscribeSocket,
    },
    security::SecurityMechanism,
};
use serde_json::Value;
use tokio::{
    sync::mpsc::UnboundedSender,
    time::{MissedTickBehavior, interval},
};

use crate::{CONTINUE_RUNNING, cmd_line::CommandLineOptions};

/// One poll per server frame at 40 frames per second keeps the output close to real time without a busy loop.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

struct MonitoredSubscriber {
    subscriber: SubscribeSocket,
    monitor: MonitorSocket,
}

impl MonitoredSubscriber {
    fn new() -> Result<Self> {
        let context = ContextBuilder::default()
            .blocky(false)
            .max_sockets(10)
            .io_threads(1)
            .build()?;

        let subscriber = Socket::from_context(&context)?;
        let monitor = subscriber.monitor(
            MonitorFlags::HandshakeSucceeded
                | MonitorFlags::HandshakeFailedAuth
                | MonitorFlags::HandshakeFailedProtocol
                | MonitorFlags::HandshakeFailedNoDetail
                | MonitorFlags::MonitorStopped
                | MonitorFlags::Disconnected
                | MonitorFlags::Closed,
        )?;

        Ok(Self {
            subscriber,
            monitor,
        })
    }

    fn configure(&self, password: &str) -> Result<()> {
        let socket_builder = SocketBuilder::default()
            .receive_timeout(0)
            .receive_highwater_mark(0)
            .send_timeout(0)
            .send_highwater_mark(0);
        let socket_builder = if password.is_empty() {
            socket_builder
        } else {
            socket_builder
                .zap_domain("stats")
                .security_mechanism(SecurityMechanism::Plain {
                    username: "stats".into(),
                    password: password.into(),
                })
        };

        let config = SubscribeBuilder::default()
            .socket_builder(socket_builder)
            .subscribe("");

        config.apply(&self.subscriber)?;

        Ok(())
    }

    fn connect(&self, address: &str) -> Result<()> {
        self.subscriber.connect(address)?;

        Ok(())
    }

    fn disconnect(&self) -> Result<()> {
        let last_endpoint = self.subscriber.last_endpoint()?;
        self.subscriber.disconnect(&last_endpoint)?;

        Ok(())
    }

    async fn recv_msg(&self) -> Option<Message> {
        self.subscriber.recv_msg_async().await
    }

    async fn check_monitor(&self) -> Option<MonitorSocketEvent> {
        self.monitor.recv_monitor_event_async().await
    }
}

fn format_ql_json(msg: &str, args: &CommandLineOptions) -> String {
    serde_json::from_str::<Value>(msg)
        .and_then(|parsed_json| {
            if args.pretty_print {
                serde_json::to_string_pretty(&parsed_json)
            } else {
                serde_json::to_string(&parsed_json)
            }
        })
        .unwrap_or(msg.to_string())
}

static FIRST_TIME: AtomicBool = AtomicBool::new(true);

fn handle_monitor_event(
    event: MonitorSocketEvent,
    monitored_subscriber: &MonitoredSubscriber,
    sender: &UnboundedSender<String>,
    endpoint: &str,
) -> Result<()> {
    match event {
        MonitorSocketEvent::HandshakeSucceeded => {
            FIRST_TIME.store(true, Ordering::Release);
            sender.send(format!("ZMQ connected to {}.", endpoint))?;
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
            if let Err(e) = monitored_subscriber.connect(endpoint) {
                sender.send(format!("error reconnecting: {e:?}."))?;
            }
        }

        MonitorSocketEvent::Connected
        | MonitorSocketEvent::ConnectDelayed
        | MonitorSocketEvent::ConnectRetried(_) => (),

        event => {
            sender.send(format!("ZMQ socket error: {event:?}",))?;
        }
    };

    Ok(())
}

pub(crate) async fn run_zmq(
    args: CommandLineOptions,
    display_sender: UnboundedSender<String>,
) -> Result<()> {
    display_sender.send(format!("ZMQ connecting to {}...", args.host))?;

    let monitored_subscriber = MonitoredSubscriber::new()?;
    monitored_subscriber.configure(&args.password)?;

    monitored_subscriber.connect(&args.host)?;

    let mut poll = interval(POLL_INTERVAL);
    poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

    while CONTINUE_RUNNING.load(Ordering::Acquire) {
        poll.tick().await;

        while let Some(zmq_msg) = monitored_subscriber.recv_msg().await {
            let zmq_str = zmq_msg.to_string();
            display_sender.send(format_ql_json(&zmq_str, &args))?;
        }

        while let Some(event) = monitored_subscriber.check_monitor().await {
            handle_monitor_event(event, &monitored_subscriber, &display_sender, &args.host)?;
        }
    }

    monitored_subscriber.disconnect()?;

    drop(display_sender);

    Ok(())
}

#[cfg(test)]
mod zmq_helper_tests {
    use core::{sync::atomic::Ordering, time::Duration};

    use arzmq::prelude::{ContextBuilder, PublishSocket, Socket};
    use tokio::{sync::mpsc::unbounded_channel, time::timeout};

    use super::{CONTINUE_RUNNING, CommandLineOptions, format_ql_json, run_zmq};

    fn options(pretty_print: bool) -> CommandLineOptions {
        CommandLineOptions {
            host: String::new(),
            password: String::new(),
            pretty_print,
        }
    }

    #[test]
    fn format_ql_json_compacts_valid_json() {
        let formatted = format_ql_json(
            "{ \"TYPE\" : \"PLAYER_KILL\",\n \"DATA\": {} }",
            &options(false),
        );

        assert_eq!(formatted, "{\"DATA\":{},\"TYPE\":\"PLAYER_KILL\"}");
    }

    #[test]
    fn format_ql_json_pretty_prints_on_request() {
        let formatted = format_ql_json("{\"a\":1}", &options(true));

        assert_eq!(formatted, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn format_ql_json_passes_other_text_through() {
        assert_eq!(format_ql_json("no json", &options(true)), "no json");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn run_zmq_stops_when_the_server_does_not_speak_the_configured_security() {
        let context = ContextBuilder::default()
            .build()
            .expect("a context can be created");
        let publisher: PublishSocket =
            Socket::from_context(&context).expect("a publisher can be created");
        publisher
            .bind("tcp://127.0.0.1:*")
            .expect("a free port can be bound");
        let endpoint = publisher.last_endpoint().expect("the endpoint is known");

        let (sender, mut receiver) = unbounded_channel();
        let task = run_zmq(
            CommandLineOptions {
                host: endpoint.clone(),
                password: "secret".to_string(),
                pretty_print: false,
            },
            sender,
        );

        let result = timeout(Duration::from_secs(10), task).await;

        assert!(
            matches!(result, Ok(Ok(()))),
            "run_zmq did not end cleanly: {result:?}"
        );
        assert!(!CONTINUE_RUNNING.load(Ordering::Acquire));
        let mut lines = Vec::new();
        while let Ok(line) = receiver.try_recv() {
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
