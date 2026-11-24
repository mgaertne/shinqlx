use anyhow::Result;
use arzmq::{
    prelude::{
        Message, MonitorReceiver, MonitorSocket, Receiver, Socket, SocketBuilder, SubscribeBuilder,
        SubscribeSocket,
    },
    security::SecurityMechanism,
};
use chrono::Local;
use ql_zmq::{LinkEvent, LinkState, POLL_INTERVAL, Reconnect, context, monitor_flags};
use tokio::{
    select,
    sync::mpsc::UnboundedSender,
    time::{MissedTickBehavior, interval},
};
use tokio_util::sync::CancellationToken;

use crate::{cmd_line::CommandLineOptions, format::Renderer};

struct MonitoredSubscriber {
    subscriber: SubscribeSocket,
    monitor: MonitorSocket,
}

impl MonitoredSubscriber {
    fn new() -> Result<Self> {
        let context = context(1)?;

        let subscriber = Socket::from_context(&context)?;
        let monitor = subscriber.monitor(monitor_flags(false))?;

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

    fn disconnect(&self) -> Result<()> {
        let last_endpoint = self.subscriber.last_endpoint()?;
        self.subscriber.disconnect(&last_endpoint)?;

        Ok(())
    }

    async fn recv_msg(&self) -> Option<Message> {
        self.subscriber.recv_msg_async().await
    }

    async fn next_link_event(&self) -> Option<LinkEvent> {
        self.monitor
            .recv_monitor_event_async()
            .await
            .map(LinkEvent::from)
    }
}

impl Reconnect for MonitoredSubscriber {
    fn connect(&self, endpoint: &str) -> Result<()> {
        self.subscriber.connect(endpoint)?;

        Ok(())
    }
}

fn with_timestamp(line: String, timestamp: bool) -> String {
    if timestamp {
        format!("{} {line}", Local::now().format("%H:%M:%S"))
    } else {
        line
    }
}

pub(crate) async fn run_zmq(
    args: CommandLineOptions,
    display_sender: UnboundedSender<String>,
    running: &CancellationToken,
) -> Result<()> {
    display_sender.send(format!("ZMQ connecting to {}...", args.host()))?;

    let monitored_subscriber = MonitoredSubscriber::new()?;
    monitored_subscriber.configure(args.password())?;

    monitored_subscriber.connect(args.host())?;

    let renderer = Renderer::new(&args, args.color.enabled());
    let mut link = LinkState::default();
    let mut poll = interval(POLL_INTERVAL);
    poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        select! {
            biased;
            () = running.cancelled() => break,
            _ = poll.tick() => {}
        }

        while let Some(zmq_msg) = monitored_subscriber.recv_msg().await {
            let zmq_str = zmq_msg.to_string();
            if let Some(line) = renderer.render(&zmq_str) {
                display_sender.send(with_timestamp(line, args.timestamp))?;
            }
        }

        while let Some(event) = monitored_subscriber.next_link_event().await {
            link.handle(
                event,
                &monitored_subscriber,
                &display_sender,
                args.host(),
                running,
            )?;
        }
    }

    monitored_subscriber.disconnect()?;

    drop(display_sender);

    Ok(())
}

#[cfg(test)]
mod zmq_helper_tests {
    use core::time::Duration;

    use arzmq::prelude::{ContextBuilder, Message, PublishSocket, SendFlags, Sender, Socket};
    use clap::Parser;
    use tokio::{
        sync::mpsc::unbounded_channel,
        time::{sleep, timeout},
    };
    use tokio_util::sync::CancellationToken;

    use super::{CommandLineOptions, MonitoredSubscriber, run_zmq, with_timestamp};

    #[tokio::test(flavor = "current_thread")]
    async fn run_zmq_prints_the_messages_of_a_server_without_password() {
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
        let options = CommandLineOptions::try_parse_from([
            "ql_stats", "--host", &endpoint, "--format", "summary", "--color", "never",
        ])
        .expect("the options are valid");
        let running = CancellationToken::new();
        let message = r#"{"TYPE":"PLAYER_CONNECT","DATA":{"NAME":"Alice"}}"#;

        let drive = async {
            let mut lines = Vec::new();
            while lines
                .iter()
                .all(|line: &String| !line.ends_with("Alice connected"))
            {
                publisher
                    .send_msg(Message::from(message), SendFlags::DONT_WAIT)
                    .expect("the message can be published");
                sleep(Duration::from_millis(50)).await;
                while let Ok(line) = receiver.try_recv() {
                    lines.push(line);
                }
            }
            running.cancel();
            lines
        };
        let (result, lines) = timeout(Duration::from_secs(10), async {
            tokio::join!(run_zmq(options, sender, &running), drive)
        })
        .await
        .expect("the run ends in time");

        assert!(result.is_ok(), "{result:?}");
        assert!(
            lines
                .iter()
                .any(|line| line == "PLAYER_CONNECT     Alice connected"),
            "{lines:?}"
        );
    }

    #[test]
    fn with_timestamp_puts_the_time_before_the_line() {
        let stamped = with_timestamp("line".to_string(), true);

        assert!(stamped.ends_with(" line"));
        assert_eq!(stamped.len(), "00:00:00 line".len());
        assert_eq!(with_timestamp("line".to_string(), false), "line");
    }

    #[test]
    fn the_subscriber_is_configured_with_and_without_a_password() {
        let subscriber = MonitoredSubscriber::new().expect("a subscriber can be created");
        let open_subscriber = MonitoredSubscriber::new().expect("a subscriber can be created");

        assert!(subscriber.configure("secret").is_ok());
        assert!(open_subscriber.configure("").is_ok());
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
        let options = CommandLineOptions::try_parse_from([
            "ql_stats",
            "--host",
            &endpoint,
            "--password",
            "secret",
        ])
        .expect("the options are valid");
        let running = CancellationToken::new();
        let task = run_zmq(options, sender, &running);

        let result = timeout(Duration::from_secs(10), task).await;

        assert!(
            matches!(result, Ok(Ok(()))),
            "run_zmq did not end cleanly: {result:?}"
        );
        assert!(running.is_cancelled());
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
