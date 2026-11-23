use core::sync::atomic::Ordering;

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
    sync::mpsc::UnboundedSender,
    time::{MissedTickBehavior, interval},
};

use crate::{CONTINUE_RUNNING, cmd_line::CommandLineOptions, format::Renderer};

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
) -> Result<()> {
    display_sender.send(format!("ZMQ connecting to {}...", args.host))?;

    let monitored_subscriber = MonitoredSubscriber::new()?;
    monitored_subscriber.configure(&args.password)?;

    monitored_subscriber.connect(&args.host)?;

    let renderer = Renderer::new(&args, args.color.enabled());
    let mut link = LinkState::default();
    let mut poll = interval(POLL_INTERVAL);
    poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

    while CONTINUE_RUNNING.load(Ordering::Acquire) {
        poll.tick().await;

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
                &args.host,
                &CONTINUE_RUNNING,
            )?;
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
    use clap::Parser;
    use tokio::{sync::mpsc::unbounded_channel, time::timeout};

    use super::{CONTINUE_RUNNING, CommandLineOptions, run_zmq, with_timestamp};

    #[test]
    fn with_timestamp_puts_the_time_before_the_line() {
        let stamped = with_timestamp("line".to_string(), true);

        assert!(stamped.ends_with(" line"));
        assert_eq!(stamped.len(), "00:00:00 line".len());
        assert_eq!(with_timestamp("line".to_string(), false), "line");
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
        let task = run_zmq(options, sender);

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
