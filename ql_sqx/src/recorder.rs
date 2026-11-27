use alloc::{boxed::Box, vec, vec::Vec};
use std::io::{self, Write};

use derive_more::Display;

use super::{
    capture::Frame,
    error::DemoError,
    file::{write_block, write_end},
    message::{Encoder, Gamestate, MAX_COMMAND_LENGTH, Message, Operation, Snapshot},
};

/// A client remembers 64 commands, so no message may carry more than that.
const COMMANDS_PER_MESSAGE: usize = 60;
/// The server toggles this snapshot flag whenever the map restarts.
const SNAPFLAG_SERVERCOUNT: u8 = 4;
const AREA_MASK_LENGTH: usize = 1;

/// Why a demo cannot be written.
#[derive(Debug, Display)]
pub enum RecordingError {
    /// The content cannot be encoded.
    #[display("{_0}")]
    Demo(DemoError),
    /// The output cannot be written.
    #[display("{_0}")]
    Io(io::Error),
}

impl core::error::Error for RecordingError {}

impl From<DemoError> for RecordingError {
    fn from(error: DemoError) -> Self {
        Self::Demo(error)
    }
}

impl From<io::Error> for RecordingError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// What a recording contains, to judge it without playing it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordingStats {
    /// Written snapshots.
    pub snapshots: usize,
    /// Written server commands.
    pub commands: usize,
    /// Commands too long for a client.
    pub skipped_commands: usize,
    /// The most entities in one snapshot.
    pub max_entities: usize,
    /// The length of the longest message.
    pub max_message_length: usize,
    /// The length of the first message.
    pub gamestate_length: usize,
    /// The length of all messages.
    pub bytes: usize,
}

/// Writes the demo of one player.
#[derive(Debug)]
pub struct PovRecorder<W: Write> {
    output: W,
    client: u8,
    encoder: Encoder,
    message_sequence: i32,
    command_sequence: i32,
    pending: Vec<Operation>,
    snapshot_flags: u8,
    stats: RecordingStats,
}

impl<W: Write> PovRecorder<W> {
    /// Starts the demo with the configstrings that are valid now.
    pub fn start(
        output: W,
        client: u8,
        configstrings: Vec<(u16, Vec<u8>)>,
    ) -> Result<Self, RecordingError> {
        let mut recorder = Self {
            output,
            client,
            encoder: Encoder::default(),
            message_sequence: 0,
            command_sequence: 0,
            pending: Vec::new(),
            snapshot_flags: 0,
            stats: RecordingStats::default(),
        };
        let gamestate = Operation::Gamestate(Gamestate {
            command_sequence: 0,
            configstrings,
            baselines: vec![],
            client_number: i32::from(client),
            checksum_feed: 0,
        });
        recorder.stats.gamestate_length = recorder.write(vec![gamestate])?;
        Ok(recorder)
    }

    /// Queues a command the server sends to `recipient`, or to everyone without one.
    pub fn command(&mut self, recipient: Option<u8>, command: &[u8]) -> Result<(), RecordingError> {
        if recipient.is_some_and(|recipient| recipient != self.client) {
            return Ok(());
        }
        if command.len() >= MAX_COMMAND_LENGTH {
            self.stats.skipped_commands += 1;
            return Ok(());
        }
        if command.starts_with(b"map_restart") {
            self.snapshot_flags ^= SNAPFLAG_SERVERCOUNT;
        }
        self.command_sequence += 1;
        self.stats.commands += 1;
        self.pending.push(Operation::ServerCommand {
            sequence: self.command_sequence,
            command: command.to_vec(),
        });
        if self.pending.len() >= COMMANDS_PER_MESSAGE {
            self.flush()?;
        }
        Ok(())
    }

    /// Adds the snapshot of a frame; frames without the player are left out.
    pub fn frame(&mut self, frame: &Frame) -> Result<(), RecordingError> {
        let Some(&player) = frame.player(self.client) else {
            return Ok(());
        };
        let entities = frame.entities_for(player.client_number());
        self.stats.max_entities = self.stats.max_entities.max(entities.len());
        self.stats.snapshots += 1;
        self.pending.push(Operation::Snapshot(Box::new(Snapshot {
            server_time: frame.server_time,
            flags: self.snapshot_flags,
            area_mask: vec![0; AREA_MASK_LENGTH],
            player,
            entities,
        })));
        self.flush()
    }

    /// Ends the demo.
    pub fn finish(mut self) -> Result<(RecordingStats, W), RecordingError> {
        if !self.pending.is_empty() {
            self.flush()?;
        }
        write_end(&mut self.output)?;
        self.output.flush()?;
        Ok((self.stats, self.output))
    }

    fn flush(&mut self) -> Result<(), RecordingError> {
        let operations = core::mem::take(&mut self.pending);
        self.write(operations)?;
        Ok(())
    }

    fn write(&mut self, operations: Vec<Operation>) -> Result<usize, RecordingError> {
        self.message_sequence += 1;
        let message = Message {
            acknowledge: 0,
            operations,
        };
        let data = self.encoder.encode(self.message_sequence, &message)?;
        write_block(&mut self.output, self.message_sequence, &data)?;
        self.stats.max_message_length = self.stats.max_message_length.max(data.len());
        self.stats.bytes += data.len();
        Ok(data.len())
    }
}

#[cfg(test)]
mod recorder_tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        capture::{CapturedEntity, Visibility},
        file::read_block,
        message::Decoder,
        state::{EntityState, PlayerState},
    };

    fn entity(number: u16, visibility: Visibility) -> CapturedEntity {
        CapturedEntity {
            state: EntityState {
                number,
                ..EntityState::default()
            },
            visibility,
        }
    }

    fn decode(demo: &[u8]) -> Vec<Operation> {
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

    fn snapshots(operations: &[Operation]) -> Vec<&Snapshot> {
        operations
            .iter()
            .filter_map(|operation| match operation {
                Operation::Snapshot(snapshot) => Some(snapshot.as_ref()),
                _ => None,
            })
            .collect()
    }

    fn frame() -> Frame {
        Frame {
            server_time: 100,
            entities: vec![
                entity(0, Visibility::Everyone),
                entity(1, Visibility::Everyone),
                entity(70, Visibility::Only(1)),
                entity(71, Visibility::Only(0)),
                entity(72, Visibility::AllBut(1)),
            ],
            players: vec![(1, viewing(1))],
        }
    }

    fn viewing(client_number: u32) -> PlayerState {
        let mut player = PlayerState::default();
        player.fields[40] = client_number;
        player
    }

    #[test]
    fn a_spectator_gets_what_the_followed_player_sees() {
        let following = Frame {
            players: vec![(1, viewing(0))],
            ..frame()
        };

        let numbers: Vec<u16> = following
            .entities_for(viewing(0).client_number())
            .iter()
            .map(|entity| entity.number)
            .collect();

        assert_eq!(numbers, [1, 71, 72]);
    }

    #[test]
    fn a_player_gets_what_the_game_shows_him_without_his_own_entity() {
        let mut recorder = PovRecorder::start(Vec::new(), 1, vec![]).expect("started");
        recorder.frame(&frame()).expect("recorded");
        let (stats, demo) = recorder.finish().expect("finished");

        let operations = decode(&demo);
        let numbers: Vec<u16> = snapshots(&operations)[0]
            .entities
            .iter()
            .map(|entity| entity.number)
            .collect();

        assert_eq!(numbers, [0, 70]);
        assert_eq!(stats.snapshots, 1);
        assert_eq!(stats.max_entities, 2);
    }

    #[test]
    fn only_commands_for_the_player_are_recorded_in_order() {
        let mut recorder = PovRecorder::start(Vec::new(), 1, vec![]).expect("started");
        recorder.command(Some(0), b"print other").expect("recorded");
        recorder.command(None, b"print all").expect("recorded");
        recorder.command(Some(1), b"print own").expect("recorded");
        recorder.command(None, &[b'a'; 1024]).expect("recorded");
        let (stats, demo) = recorder.finish().expect("finished");

        let commands: Vec<_> = decode(&demo)
            .into_iter()
            .filter_map(|operation| match operation {
                Operation::ServerCommand { sequence, command } => Some((sequence, command)),
                _ => None,
            })
            .collect();

        assert_eq!(
            commands,
            [(1, b"print all".to_vec()), (2, b"print own".to_vec())]
        );
        assert_eq!(stats.skipped_commands, 1);
    }

    #[test]
    fn a_map_restart_toggles_the_snapshot_flag() {
        let mut recorder = PovRecorder::start(Vec::new(), 1, vec![]).expect("started");
        recorder.frame(&frame()).expect("recorded");
        recorder.command(None, b"map_restart\n").expect("recorded");
        recorder.frame(&frame()).expect("recorded");
        let (_, demo) = recorder.finish().expect("finished");

        let operations = decode(&demo);
        let flags: Vec<u8> = snapshots(&operations)
            .iter()
            .map(|snapshot| snapshot.flags)
            .collect();

        assert_eq!(flags, [0, SNAPFLAG_SERVERCOUNT]);
    }

    #[test]
    fn many_commands_are_spread_over_several_messages() {
        let mut recorder = PovRecorder::start(Vec::new(), 1, vec![]).expect("started");
        for _ in 0..150 {
            recorder.command(None, b"print x").expect("recorded");
        }
        let (_, demo) = recorder.finish().expect("finished");
        let mut rest = demo.as_slice();

        let messages = core::iter::from_fn(|| read_block(&mut rest).expect("well formed")).count();

        assert_eq!(messages, 4);
    }
}
