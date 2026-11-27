//! Turns the views of the players in a recording into demos.

use alloc::{collections::BTreeMap, vec::Vec};
use std::io::{self, Read, Write};

use derive_more::Display;

use super::{
    archive::{ArchiveError, ArchiveReader, Record},
    capture::Frame,
    players::{Presence, Stay},
    recorder::{PovRecorder, RecordingError, RecordingStats},
    writer::Event,
};

/// Why no demo can be exported.
#[derive(Debug, Display)]
pub enum ExportError {
    /// The recording cannot be read.
    #[display("{_0}")]
    Archive(ArchiveError),
    /// The demo cannot be written.
    #[display("{_0}")]
    Demo(RecordingError),
}

impl core::error::Error for ExportError {}

impl From<ArchiveError> for ExportError {
    fn from(error: ArchiveError) -> Self {
        Self::Archive(error)
    }
}

impl From<RecordingError> for ExportError {
    fn from(error: RecordingError) -> Self {
        Self::Demo(error)
    }
}

/// Whether demos keep the messages that were not meant for everyone: team chat and tells.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrivateMessages {
    /// The demo shows what the player got.
    #[default]
    Include,
    /// Team chat and tells are left out.
    Omit,
}

impl PrivateMessages {
    fn allows(self, command: &[u8]) -> bool {
        self == Self::Include || !(is_team_chat(command) || is_tell(command))
    }
}

fn is_team_chat(command: &[u8]) -> bool {
    command.starts_with(b"tchat ")
}

/// A tell is a `chat` like public messages, but its text begins with `\x19[` after the client number.
fn is_tell(command: &[u8]) -> bool {
    command.strip_prefix(b"chat \"").is_some_and(|text| {
        text.iter()
            .skip_while(|byte| byte.is_ascii_digit() || **byte == b' ')
            .take(2)
            .eq(b"\x19[")
    })
}

/// The demo of one stay.
#[derive(Debug)]
pub struct Demo<W> {
    /// Whose view the demo shows.
    pub stay: Stay,
    /// What the demo contains.
    pub stats: RecordingStats,
    /// What the demo was written to.
    pub output: W,
}

struct Exporter<W: Write> {
    private_messages: PrivateMessages,
    configstrings: BTreeMap<u16, Vec<u8>>,
    /// What a demo beginning with the next frame starts with.
    before_frame: BTreeMap<u16, Vec<u8>>,
    changed: bool,
    commands_since_frame: Vec<(Option<u8>, Vec<u8>)>,
    presence: Presence,
    running: BTreeMap<u8, (Stay, PovRecorder<W>)>,
    finished: Vec<Demo<W>>,
}

impl<W: Write> Default for Exporter<W> {
    fn default() -> Self {
        Self {
            private_messages: PrivateMessages::default(),
            configstrings: BTreeMap::new(),
            before_frame: BTreeMap::new(),
            changed: false,
            commands_since_frame: Vec::new(),
            presence: Presence::default(),
            running: BTreeMap::new(),
            finished: Vec::new(),
        }
    }
}

impl<W: Write> Exporter<W> {
    fn configstring(&mut self, index: u16, value: Vec<u8>) {
        if value.is_empty() {
            self.configstrings.remove(&index);
        } else {
            self.configstrings.insert(index, value);
        }
        self.changed = true;
    }

    fn command(&mut self, recipient: Option<u8>, command: Vec<u8>) -> Result<(), ExportError> {
        if !self.private_messages.allows(&command) {
            return Ok(());
        }
        for (_, recorder) in self.running.values_mut() {
            recorder.command(recipient, &command)?;
        }
        self.commands_since_frame.push((recipient, command));
        Ok(())
    }

    fn frame(
        &mut self,
        frame: &Frame,
        select: &mut impl FnMut(&Stay) -> io::Result<Option<W>>,
    ) -> Result<(), ExportError> {
        for &(client, _) in &frame.players {
            let Some(stay) = self.presence.arrive(client, &self.configstrings) else {
                continue;
            };
            let Some(output) = select(&stay).map_err(RecordingError::from)? else {
                continue;
            };
            let configstrings = self.before_frame.clone().into_iter().collect();
            let mut recorder = PovRecorder::start(output, client, configstrings)?;
            for (recipient, command) in &self.commands_since_frame {
                recorder.command(*recipient, command)?;
            }
            self.running.insert(client, (stay, recorder));
        }
        for (_, recorder) in self.running.values_mut() {
            recorder.frame(frame)?;
        }
        self.commands_since_frame.clear();
        if core::mem::take(&mut self.changed) {
            self.before_frame.clone_from(&self.configstrings);
        }
        Ok(())
    }

    fn player_left(&mut self, client: u8) -> Result<(), ExportError> {
        self.presence.leave(client);
        // Whoever takes the number next must not get what was meant for this player.
        self.commands_since_frame
            .retain(|(recipient, _)| *recipient != Some(client));
        if let Some((stay, recorder)) = self.running.remove(&client) {
            self.finish(stay, recorder)?;
        }
        Ok(())
    }

    fn finish(&mut self, stay: Stay, recorder: PovRecorder<W>) -> Result<(), ExportError> {
        let (stats, output) = recorder.finish()?;
        self.finished.push(Demo {
            stay,
            stats,
            output,
        });
        Ok(())
    }
}

/// Writes demos in one pass over the recording. `select` is asked at the begin of every stay for
/// where to write its demo, `None` leaves the stay out. A demo runs from the arrival of the player
/// until the player leaves. The demos are returned in the order of client and stay.
pub fn export_dm91<R: Read, W: Write>(
    mut recording: ArchiveReader<R>,
    private_messages: PrivateMessages,
    mut select: impl FnMut(&Stay) -> io::Result<Option<W>>,
) -> Result<Vec<Demo<W>>, ExportError> {
    let mut exporter = Exporter {
        private_messages,
        ..Exporter::default()
    };
    while let Some(record) = recording.next_record()? {
        match record {
            Record::Start { configstrings, .. } => {
                exporter.configstrings = configstrings.into_iter().collect();
                exporter.before_frame.clone_from(&exporter.configstrings);
            }
            Record::Event(Event::Configstring { index, value }) => {
                exporter.configstring(index, value);
            }
            Record::Event(Event::Command { recipient, command }) => {
                exporter.command(recipient, command)?;
            }
            Record::Event(Event::Frame(frame)) => exporter.frame(&frame, &mut select)?,
            Record::Event(Event::PlayerLeft { client }) => exporter.player_left(client)?,
            Record::Event(Event::End { .. }) => break,
        }
    }
    for (stay, recorder) in core::mem::take(&mut exporter.running).into_values() {
        exporter.finish(stay, recorder)?;
    }
    let mut demos = exporter.finished;
    demos.sort_by_key(|demo| (demo.stay.client, demo.stay.number));
    Ok(demos)
}

#[cfg(test)]
mod export_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;
    use crate::{
        archive::ArchiveWriter,
        capture::{CapturedEntity, Frame, Visibility},
        file::read_block,
        message::{Decoder, Operation},
        state::{EntityState, PlayerState},
    };

    fn frame(server_time: i32, clients: &[u8]) -> Event {
        let entity = |number: u16| {
            let mut state = EntityState {
                number,
                ..EntityState::default()
            };
            state.fields[1] = (server_time as f32 / 7.0).to_bits();
            CapturedEntity {
                state,
                visibility: Visibility::Everyone,
            }
        };
        Event::Frame(Frame {
            server_time,
            entities: vec![entity(0), entity(1), entity(80)],
            players: clients
                .iter()
                .map(|&client| {
                    let mut player = PlayerState::default();
                    player.fields[0] = server_time.cast_unsigned();
                    player.fields[40] = u32::from(client);
                    (client, player)
                })
                .collect(),
        })
    }

    fn command(recipient: Option<u8>, text: &str) -> Event {
        Event::Command {
            recipient,
            command: text.as_bytes().to_vec(),
        }
    }

    fn configstring(index: u16, value: &str) -> Event {
        Event::Configstring {
            index,
            value: value.as_bytes().to_vec(),
        }
    }

    fn recording(configstrings: &[(u16, Vec<u8>)], events: &[Event]) -> Vec<u8> {
        let mut writer = ArchiveWriter::start(Vec::new(), &[], configstrings).expect("started");
        for event in events {
            writer.event(event).expect("written");
        }
        writer.finish().expect("finished").1
    }

    fn export_all(recording: &[u8], client: u8) -> Vec<Demo<Vec<u8>>> {
        let reader = ArchiveReader::new(recording).expect("a recording");
        export_dm91(reader, PrivateMessages::Include, |stay| {
            Ok((stay.client == client).then(Vec::new))
        })
        .expect("exported")
    }

    fn export(recording: &[u8], client: u8) -> Vec<u8> {
        export_all(recording, client).swap_remove(0).output
    }

    fn operations(demo: &[u8]) -> Vec<Operation> {
        let mut rest = demo;
        let mut decoder = Decoder::default();
        core::iter::from_fn(|| read_block(&mut rest).expect("well formed"))
            .flat_map(|block| {
                decoder
                    .decode(block.sequence, block.message)
                    .expect("decodable")
                    .operations
            })
            .collect()
    }

    #[test]
    fn an_exported_demo_equals_a_directly_recorded_one() {
        let configstrings = vec![(0, b"info".to_vec())];
        let events = [
            command(None, "print before the first frame"),
            frame(25, &[0, 1]),
            command(Some(1), "print own"),
            command(Some(0), "print other"),
            frame(50, &[0, 1]),
            command(None, "map_restart\n"),
            frame(75, &[0]),
            frame(20_000, &[0, 1]),
        ];
        let mut direct = PovRecorder::start(Vec::new(), 1, configstrings.clone()).expect("started");
        for event in &events {
            match event {
                Event::Command { recipient, command } => {
                    direct.command(*recipient, command).expect("recorded");
                }
                Event::Frame(frame) => direct.frame(frame).expect("recorded"),
                _ => {}
            }
        }
        let (_, direct) = direct.finish().expect("finished");

        let exported = export(&recording(&configstrings, &events), 1);

        assert_eq!(exported, direct);
    }

    #[test]
    fn a_later_player_starts_with_what_is_valid_when_joining() {
        let events = [
            frame(25, &[0]),
            configstring(3, "new"),
            configstring(0, ""),
            command(None, "print too early"),
            frame(50, &[0]),
            command(Some(2), "print welcome"),
            frame(75, &[0, 2]),
            Event::PlayerLeft { client: 2 },
            frame(100, &[0, 2]),
        ];
        let demo = export(&recording(&[(0, b"info".to_vec())], &events), 2);

        let operations = operations(&demo);
        let [
            Operation::Gamestate(gamestate),
            Operation::ServerCommand { command, .. },
            Operation::Snapshot(snapshot),
        ] = operations.as_slice()
        else {
            panic!("unexpected operations: {operations:?}");
        };
        assert_eq!(gamestate.configstrings, [(3, b"new".to_vec())]);
        assert_eq!(gamestate.client_number, 2);
        assert_eq!(command, b"print welcome");
        assert_eq!(snapshot.server_time, 75);
    }

    #[test]
    fn a_client_that_never_played_has_no_demo() {
        assert!(export_all(&recording(&[], &[frame(25, &[0])]), 5).is_empty());
    }

    #[test]
    fn everyone_taking_a_client_number_gets_an_own_demo() {
        let events = [
            frame(25, &[0, 2]),
            command(Some(2), "print for the first"),
            Event::PlayerLeft { client: 2 },
            frame(50, &[0]),
            configstring(531, "n\\Second"),
            command(Some(2), "print for the second"),
            frame(75, &[0, 2]),
        ];

        let demos = export_all(&recording(&[(531, b"n\\First".to_vec())], &events), 2);

        let contents: Vec<_> = demos
            .iter()
            .map(|demo| {
                let commands: Vec<_> = operations(&demo.output)
                    .into_iter()
                    .filter_map(|operation| match operation {
                        Operation::ServerCommand { command, .. } => Some(command),
                        _ => None,
                    })
                    .collect();
                (demo.stay.name.clone(), demo.stats.snapshots, commands)
            })
            .collect();
        assert_eq!(
            contents,
            [
                (
                    Some("First".into()),
                    1,
                    vec![b"print for the first".to_vec()]
                ),
                (
                    Some("Second".into()),
                    1,
                    vec![b"print for the second".to_vec()]
                ),
            ]
        );
    }

    #[rstest]
    #[case::public("chat \"00 Name^7\x19: ^2hi\"", true)]
    #[case::public_with_brackets("chat \"00 [x] Name^7\x19: ^2hi\"", true)]
    #[case::print("print \"\x19[not a chat\"", true)]
    #[case::team("tchat \"00 \x19(Name^7\x19) (Location^7)\x19: ^5hi\"", false)]
    #[case::tell("chat \"00 \x19[Name^7\x19]\x19: ^6hi\"", false)]
    #[case::tell_to_a_teammate("chat \"12 \x19[Name^7\x19] (Location^7)\x19: ^6hi\"", false)]
    #[case::tell_without_number("chat \"\x19[Name^7\x19]\x19: ^6hi\"", false)]
    fn team_chat_and_tells_can_be_left_out(#[case] text: &str, #[case] public: bool) {
        let events = [
            command(Some(0), text),
            frame(25, &[0]),
            command(Some(0), text),
        ];
        let recording = recording(&[], &events);
        let received = |private_messages| {
            let reader = ArchiveReader::new(recording.as_slice()).expect("a recording");
            let demos =
                export_dm91(reader, private_messages, |_| Ok(Some(Vec::new()))).expect("exported");
            operations(&demos[0].output)
                .iter()
                .filter(|operation| matches!(operation, Operation::ServerCommand { .. }))
                .count()
        };

        assert_eq!(received(PrivateMessages::Include), 2);
        assert_eq!(received(PrivateMessages::Omit), if public { 2 } else { 0 });
    }

    #[test]
    fn demos_written_together_equal_demos_written_alone() {
        let events = [
            frame(25, &[0, 1]),
            command(None, "print all"),
            command(Some(1), "print own"),
            frame(50, &[0, 1]),
        ];
        let recording = recording(&[(0, b"info".to_vec())], &events);

        let reader = ArchiveReader::new(recording.as_slice()).expect("a recording");
        let together = export_dm91(reader, PrivateMessages::Include, |_| Ok(Some(Vec::new())))
            .expect("exported");

        let outputs: Vec<_> = together.into_iter().map(|demo| demo.output).collect();
        assert_eq!(outputs, [export(&recording, 0), export(&recording, 1)]);
    }
}
