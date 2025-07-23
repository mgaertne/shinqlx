use core::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use arzmq::{
    context::ContextBuilder,
    message::Message,
    security::SecurityMechanism,
    socket::{
        MonitorFlags, MonitorReceiver, MonitorSocket, MonitorSocketEvent, Receiver, Socket,
        SocketBuilder, SubscribeBuilder, SubscribeSocket,
    },
};
use serde_json::Value;
use tokio::{
    select,
    sync::{RwLock, mpsc::UnboundedSender},
};

use crate::{CONTINUE_RUNNING, cmd_line::CommandLineOptions};

struct MonitoredSubscriber {
    subscriber: RwLock<SubscribeSocket>,
    monitor: RwLock<MonitorSocket>,
}

unsafe impl Send for MonitoredSubscriber {}
unsafe impl Sync for MonitoredSubscriber {}

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
            subscriber: subscriber.into(),
            monitor: monitor.into(),
        })
    }

    async fn configure(&self, password: &str) -> Result<()> {
        let socket_config = SocketBuilder::default()
            .security_mechanism(SecurityMechanism::PlainClient {
                username: "stats".into(),
                password: password.into(),
            })
            .receive_timeout(0)
            .receive_highwater_mark(0)
            .send_timeout(0)
            .receive_highwater_mark(0)
            .zap_domain("stats");

        let config = SubscribeBuilder::default()
            .socket_config(socket_config)
            .subscribe("");

        let subscriber = self.subscriber.read().await;
        config.apply(&subscriber)?;

        Ok(())
    }

    async fn connect(&self, address: &str) -> Result<()> {
        let socket = self.subscriber.read().await;
        socket.connect(address)?;

        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        let subscriber = self.subscriber.read().await;
        let last_endpoint = subscriber.last_endpoint()?;
        subscriber.disconnect(&last_endpoint)?;

        Ok(())
    }

    async fn recv_msg(&self) -> Option<Message> {
        let subscriber = self.subscriber.read().await;
        subscriber.recv_msg_async().await
    }

    async fn check_monitor(&self) -> Option<MonitorSocketEvent> {
        let monitor = self.monitor.read().await;
        monitor.recv_monitor_event_async().await
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

async fn check_monitor(
    monitored_dealer: &MonitoredSubscriber,
    sender: &UnboundedSender<String>,
    endpoint: &str,
) -> Result<()> {
    match monitored_dealer.check_monitor().await {
        Some(MonitorSocketEvent::HandshakeSucceeded) => {
            FIRST_TIME.store(true, Ordering::Release);
            sender.send(format!("ZMQ connected to {}.", &endpoint))?;
        }

        Some(
            event @ (MonitorSocketEvent::HandshakeFailedAuth(_)
            | MonitorSocketEvent::HandshakeFailedProtocol(_)
            | MonitorSocketEvent::HandshakeFailedNoDetail(_)
            | MonitorSocketEvent::MonitorStopped),
        ) => {
            sender.send(format!("ZMQ socket error: {event:?}"))?;
            CONTINUE_RUNNING.store(false, Ordering::Release);
        }

        Some(MonitorSocketEvent::Disconnected | MonitorSocketEvent::Closed) => {
            if FIRST_TIME.load(Ordering::Acquire) {
                FIRST_TIME.store(false, Ordering::Release);
                sender.send("Reconnecting ZMQ...".to_string())?;
            }
            if let Err(e) = monitored_dealer.connect(endpoint).await {
                sender.send(format!("error reconnecting: {e:?}."))?;
            }
        }

        Some(
            MonitorSocketEvent::Connected
            | MonitorSocketEvent::ConnectDelayed
            | MonitorSocketEvent::ConnectRetried(_),
        ) => (),

        Some(event) => {
            sender.send(format!("ZMQ socket error: {event:?}",))?;
        }

        _ => (),
    };

    Ok(())
}

pub(crate) async fn run_zmq(
    args: CommandLineOptions,
    display_sender: UnboundedSender<String>,
) -> Result<()> {
    display_sender.send(format!("ZMQ connecting to {}...", &args.host))?;

    let monitored_dealer = MonitoredSubscriber::new()?;
    monitored_dealer.configure(&args.password).await?;

    monitored_dealer.connect(&args.host).await?;

    while CONTINUE_RUNNING.load(Ordering::Acquire) {
        select!(
            biased;

            Some(zmq_msg) = monitored_dealer.recv_msg() => {
                let zmq_str = zmq_msg.to_string();
                display_sender.send(format_ql_json(&zmq_str, &args))?;
            }

            Ok(()) = check_monitor(&monitored_dealer, &display_sender, &args.host) => (),

            else => ()
        );
    }

    monitored_dealer.disconnect().await?;

    drop(display_sender);

    Ok(())
}
