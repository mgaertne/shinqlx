//! The watching of the ZMQ connection that `ql_rcon` and `ql_stats` have in common.

pub mod quake_colors;
pub mod servers;

use core::time::Duration;

use anyhow::Result;
use arzmq::{
    context::Context,
    prelude::{ContextBuilder, MonitorFlags, MonitorSocketEvent},
};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

/// One poll per server frame at 40 frames per second keeps the output close to real time without a busy loop.
pub const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// A context for one monitored socket that never blocks on shutdown.
pub fn context(io_threads: i32) -> Result<Context> {
    Ok(ContextBuilder::default()
        .blocky(false)
        .max_sockets(10)
        .io_threads(io_threads)
        .build()?)
}

/// The events of a connection that are worth a message; `connected` adds the first connect of the socket.
pub fn monitor_flags(connected: bool) -> MonitorFlags {
    let flags = MonitorFlags::HandshakeSucceeded
        | MonitorFlags::HandshakeFailedAuth
        | MonitorFlags::HandshakeFailedProtocol
        | MonitorFlags::HandshakeFailedNoDetail
        | MonitorFlags::MonitorStopped
        | MonitorFlags::Disconnected
        | MonitorFlags::Closed;
    if connected {
        flags | MonitorFlags::Connected
    } else {
        flags
    }
}

/// What happened to the connection to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkEvent {
    /// The socket connected, the handshake is still to come.
    Connected,
    /// The server accepted the handshake.
    HandshakeSucceeded,
    /// The handshake failed or the monitor stopped, the connection is not usable.
    Failed(String),
    /// The connection was closed and has to be opened again.
    Lost,
    /// Progress of the connection that needs no message.
    Ignored,
    /// An event that the console does not expect.
    Other(String),
}

impl From<MonitorSocketEvent> for LinkEvent {
    fn from(event: MonitorSocketEvent) -> Self {
        match event {
            MonitorSocketEvent::Connected => Self::Connected,
            MonitorSocketEvent::HandshakeSucceeded => Self::HandshakeSucceeded,
            MonitorSocketEvent::HandshakeFailedAuth(_)
            | MonitorSocketEvent::HandshakeFailedProtocol(_)
            | MonitorSocketEvent::HandshakeFailedNoDetail(_)
            | MonitorSocketEvent::MonitorStopped => Self::Failed(format!("{event:?}")),
            MonitorSocketEvent::Disconnected | MonitorSocketEvent::Closed => Self::Lost,
            MonitorSocketEvent::ConnectDelayed | MonitorSocketEvent::ConnectRetried(_) => {
                Self::Ignored
            }
            event => Self::Other(format!("{event:?}")),
        }
    }
}

/// A connection that can be opened again after it was lost.
pub trait Reconnect {
    /// Connects to the endpoint once more.
    fn connect(&self, endpoint: &str) -> Result<()>;
}

/// The messages about the connection, which tell the first loss of a connection only once until it was up again.
#[derive(Debug, Clone, Copy)]
pub struct LinkState {
    announce_next_loss: bool,
}

impl Default for LinkState {
    fn default() -> Self {
        Self {
            announce_next_loss: true,
        }
    }
}

impl LinkState {
    /// Reports the event to `sender`, reconnects a lost connection and cancels `running` when it failed.
    ///
    /// Returns whether the event was a successful handshake, which the caller may answer with its own requests.
    pub fn handle<L: Reconnect>(
        &mut self,
        event: LinkEvent,
        link: &L,
        sender: &UnboundedSender<String>,
        endpoint: &str,
        running: &CancellationToken,
    ) -> Result<bool> {
        match event {
            LinkEvent::Connected => {
                if core::mem::take(&mut self.announce_next_loss) {
                    sender.send("ZMQ registering with the server.".to_string())?;
                }
            }

            LinkEvent::HandshakeSucceeded => {
                self.announce_next_loss = true;
                sender.send(format!("ZMQ connected to {endpoint}."))?;
                return Ok(true);
            }

            LinkEvent::Failed(description) => {
                sender.send(format!("ZMQ socket error: {description}"))?;
                running.cancel();
            }

            LinkEvent::Lost => {
                if core::mem::take(&mut self.announce_next_loss) {
                    sender.send("Reconnecting ZMQ...".to_string())?;
                }
                if let Err(e) = link.connect(endpoint) {
                    sender.send(format!("error reconnecting: {e:?}."))?;
                }
            }

            LinkEvent::Ignored => (),

            LinkEvent::Other(description) => {
                sender.send(format!("ZMQ socket error: {description}"))?;
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use anyhow::{Result, anyhow};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
    use tokio_util::sync::CancellationToken;

    use super::{LinkEvent, LinkState, Reconnect};

    #[derive(Default)]
    struct FakeLink {
        connected: Mutex<Vec<String>>,
        failing: bool,
    }

    impl Reconnect for FakeLink {
        fn connect(&self, endpoint: &str) -> Result<()> {
            self.connected
                .lock()
                .expect("the endpoints are not poisoned")
                .push(endpoint.to_string());
            if self.failing {
                return Err(anyhow!("refused"));
            }
            Ok(())
        }
    }

    fn drained(receiver: &mut UnboundedReceiver<String>) -> Vec<String> {
        let mut lines = Vec::new();
        while let Ok(line) = receiver.try_recv() {
            lines.push(line);
        }
        lines
    }

    fn handle_all(
        events: impl IntoIterator<Item = LinkEvent>,
        link: &FakeLink,
        running: &CancellationToken,
    ) -> (Vec<String>, Vec<bool>) {
        let (sender, mut receiver) = unbounded_channel();
        let mut state = LinkState::default();
        let handshakes = events
            .into_iter()
            .map(|event| {
                state
                    .handle(event, link, &sender, "tcp://127.0.0.1:27961", running)
                    .expect("the event can be handled")
            })
            .collect();

        (drained(&mut receiver), handshakes)
    }

    #[test]
    fn a_handshake_is_reported_to_the_caller() {
        let running = CancellationToken::new();

        let (lines, handshakes) = handle_all(
            [LinkEvent::HandshakeSucceeded, LinkEvent::Ignored],
            &FakeLink::default(),
            &running,
        );

        assert_eq!(lines, ["ZMQ connected to tcp://127.0.0.1:27961."]);
        assert_eq!(handshakes, [true, false]);
        assert!(!running.is_cancelled());
    }

    #[test]
    fn the_first_connect_and_loss_are_announced_once_until_the_handshake_succeeded() {
        let link = FakeLink::default();

        let (lines, _) = handle_all(
            [
                LinkEvent::Connected,
                LinkEvent::Connected,
                LinkEvent::Lost,
                LinkEvent::Lost,
                LinkEvent::HandshakeSucceeded,
                LinkEvent::Lost,
            ],
            &link,
            &CancellationToken::new(),
        );

        assert_eq!(
            lines,
            [
                "ZMQ registering with the server.",
                "ZMQ connected to tcp://127.0.0.1:27961.",
                "Reconnecting ZMQ...",
            ]
        );
        assert_eq!(
            link.connected
                .lock()
                .expect("the endpoints are not poisoned")
                .len(),
            3
        );
    }

    #[test]
    fn a_failed_reconnect_is_reported() {
        let link = FakeLink {
            failing: true,
            ..FakeLink::default()
        };

        let (lines, _) = handle_all([LinkEvent::Lost], &link, &CancellationToken::new());

        assert_eq!(
            lines,
            ["Reconnecting ZMQ...", "error reconnecting: refused."]
        );
    }

    #[test]
    fn a_failed_handshake_stops_the_run() {
        let running = CancellationToken::new();

        let (lines, _) = handle_all(
            [LinkEvent::Failed("MonitorStopped".to_string())],
            &FakeLink::default(),
            &running,
        );

        assert_eq!(lines, ["ZMQ socket error: MonitorStopped"]);
        assert!(running.is_cancelled());
    }

    #[test]
    fn other_events_are_reported_without_stopping() {
        let running = CancellationToken::new();

        let (lines, _) = handle_all(
            [LinkEvent::Other("Listening".to_string())],
            &FakeLink::default(),
            &running,
        );

        assert_eq!(lines, ["ZMQ socket error: Listening"]);
        assert!(!running.is_cancelled());
    }
}
