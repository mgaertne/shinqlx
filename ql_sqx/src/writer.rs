use alloc::{collections::BTreeMap, string::String, vec::Vec};
use std::{
    io::{self, Write},
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread::{self, JoinHandle},
};

use super::{
    archive::{ArchiveError, ArchiveWriter},
    capture::Frame,
};

/// Events the game thread may be ahead of the writer, a few seconds of a match.
const QUEUE_LENGTH: usize = 1024;

/// Something that happens while a match is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// The state at the end of a server frame.
    Frame(Frame),
    /// A command the server sends to one client, or to every client.
    Command {
        /// The client number, or `None` for every client.
        recipient: Option<u8>,
        /// The text of the command.
        command: Vec<u8>,
    },
    /// A configstring got a new value; an empty one removes it.
    Configstring {
        /// The number of the configstring.
        index: u16,
        /// The new value.
        value: Vec<u8>,
    },
    /// A client left the server.
    PlayerLeft {
        /// The client number.
        client: u8,
    },
    /// A stats message of the game.
    Stats {
        /// The JSON object `{"DATA":{…},"TYPE":"…"}`, as the stats socket of the server carries it.
        message: String,
    },
    /// The recording was ended on purpose.
    End {
        /// Why it ended.
        reason: String,
    },
}

/// What a finished recording contains.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArchiveStats {
    /// Recorded frames.
    pub frames: u64,
    /// The size of the recording.
    pub bytes: u64,
}

/// Writes a recording on its own thread, so the game thread never waits for the disk.
#[derive(Debug)]
pub struct DemoWriter {
    sender: SyncSender<Event>,
    thread: JoinHandle<()>,
}

impl DemoWriter {
    /// Starts the writer thread, which reports the outcome to `done` once the recording ends.
    pub fn start<W>(
        archive: ArchiveWriter<W>,
        configstrings: Vec<(u16, Vec<u8>)>,
        done: impl FnOnce(Result<ArchiveStats, ArchiveError>) + Send + 'static,
    ) -> io::Result<Self>
    where
        W: Write + Send + 'static,
    {
        let (sender, receiver) = sync_channel(QUEUE_LENGTH);
        let thread = thread::Builder::new()
            .name("shinqlx-demo".into())
            .spawn(move || done(record(archive, configstrings, &receiver)))?;
        Ok(Self { sender, thread })
    }

    /// Hands over an event without waiting, returning `false` if the writer cannot take it.
    pub fn send(&self, event: Event) -> bool {
        self.sender.try_send(event).is_ok()
    }

    /// Ends the recording; the returned handle finishes once everything is written.
    pub fn finish(self) -> JoinHandle<()> {
        drop(self.sender);
        self.thread
    }
}

fn record<W: Write>(
    mut archive: ArchiveWriter<W>,
    configstrings: Vec<(u16, Vec<u8>)>,
    events: &Receiver<Event>,
) -> Result<ArchiveStats, ArchiveError> {
    let mut configstrings: BTreeMap<u16, Vec<u8>> = configstrings.into_iter().collect();
    let mut frames = 0;
    for event in events {
        match &event {
            // The game sets some configstrings again in every frame.
            Event::Configstring { index, value } => {
                let known = configstrings.get(index).map_or(&[][..], Vec::as_slice);
                if known == value {
                    continue;
                }
                configstrings.insert(*index, value.clone());
            }
            Event::Frame(_) => frames += 1,
            _ => {}
        }
        archive.event(&event)?;
    }
    let (bytes, _) = archive.finish()?;
    Ok(ArchiveStats { frames, bytes })
}

#[cfg(test)]
mod writer_tests {
    use std::sync::mpsc::channel;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn finishing_writes_everything_but_unchanged_configstrings() {
        let configstrings = vec![(3, b"same".to_vec())];
        let archive =
            ArchiveWriter::start(Vec::new(), &[], &configstrings).expect("header written");
        let (report, outcome) = channel();
        let writer = DemoWriter::start(archive, configstrings, move |result| {
            report.send(result.expect("recorded")).expect("received");
        })
        .expect("thread started");
        let header = ArchiveWriter::start(Vec::new(), &[], &[(3, b"same".to_vec())])
            .and_then(ArchiveWriter::finish)
            .expect("header written")
            .0;

        assert!(writer.send(Event::Configstring {
            index: 3,
            value: b"same".to_vec(),
        }));
        assert!(writer.send(Event::Frame(Frame {
            server_time: 25,
            ..Frame::default()
        })));
        writer.finish().join().expect("thread ended");

        let stats = outcome.recv().expect("reported");
        assert_eq!(stats.frames, 1);
        assert!(stats.bytes > header);
        assert!(stats.bytes < header + 20);
    }
}
