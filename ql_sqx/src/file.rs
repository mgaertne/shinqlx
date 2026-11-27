use std::io::{self, Write};

use super::error::DemoError;

/// The longest message a client accepts.
const MAX_MESSAGE_LENGTH: usize = 32768;
const END_MARKER: i32 = -1;

/// Appends one message to a demo.
pub(crate) fn write_block(
    output: &mut impl Write,
    sequence: i32,
    message: &[u8],
) -> io::Result<()> {
    let length = i32::try_from(message.len()).map_err(io::Error::other)?;
    if message.len() > MAX_MESSAGE_LENGTH {
        return Err(io::Error::other(DemoError::BlockLength(length)));
    }
    output.write_all(&sequence.to_le_bytes())?;
    output.write_all(&length.to_le_bytes())?;
    output.write_all(message)
}

/// Appends what marks the end of a demo.
pub(crate) fn write_end(output: &mut impl Write) -> io::Result<()> {
    output.write_all(&END_MARKER.to_le_bytes())?;
    output.write_all(&END_MARKER.to_le_bytes())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Block<'a> {
    pub(crate) sequence: i32,
    pub(crate) message: &'a [u8],
}

/// Takes the next message from the front of `demo`, returning `None` at the end of the demo.
pub(crate) fn read_block<'a>(demo: &mut &'a [u8]) -> Result<Option<Block<'a>>, DemoError> {
    if demo.is_empty() {
        return Ok(None);
    }
    let sequence = read_number(demo)?;
    let length = read_number(demo)?;
    if length == END_MARKER {
        return Ok(None);
    }
    let size = usize::try_from(length)
        .ok()
        .filter(|&size| size <= MAX_MESSAGE_LENGTH)
        .ok_or(DemoError::BlockLength(length))?;
    let (message, rest) = demo
        .split_at_checked(size)
        .ok_or(DemoError::UnexpectedEnd)?;
    *demo = rest;
    Ok(Some(Block { sequence, message }))
}

fn read_number(demo: &mut &[u8]) -> Result<i32, DemoError> {
    let (number, rest) = demo.split_first_chunk().ok_or(DemoError::UnexpectedEnd)?;
    *demo = rest;
    Ok(i32::from_le_bytes(*number))
}

#[cfg(test)]
mod file_tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::message::{Decoder, Encoder, Operation};

    #[test]
    fn written_blocks_are_read_back() {
        let mut demo = Vec::new();
        write_block(&mut demo, 4, b"abc").expect("writable");
        write_end(&mut demo).expect("writable");
        let mut rest = demo.as_slice();

        assert_eq!(
            read_block(&mut rest),
            Ok(Some(Block {
                sequence: 4,
                message: b"abc",
            }))
        );
        assert_eq!(read_block(&mut rest), Ok(None));
    }

    #[test]
    fn a_truncated_block_is_an_error() {
        let mut demo = Vec::new();
        write_block(&mut demo, 4, b"abc").expect("writable");
        demo.pop();

        assert_eq!(
            read_block(&mut demo.as_slice()),
            Err(DemoError::UnexpectedEnd)
        );
    }

    #[test]
    fn overlong_messages_are_not_written() {
        let message = vec![0; MAX_MESSAGE_LENGTH + 1];

        assert!(write_block(&mut Vec::new(), 1, &message).is_err());
    }

    /// The engine leaves the last byte of a message undefined when no bits fall into it.
    fn equal_up_to_padding(encoded: &[u8], recorded: &[u8]) -> bool {
        encoded.len() == recorded.len()
            && encoded.split_last().map(|(_, head)| head)
                == recorded.split_last().map(|(_, head)| head)
    }

    #[test]
    #[ignore = "needs a protocol 91 demo named in SHINQLX_DM91_DEMO"]
    fn a_recorded_protocol_91_demo_is_decoded_completely() {
        let path = std::env::var("SHINQLX_DM91_DEMO").expect("SHINQLX_DM91_DEMO is set");
        let content = std::fs::read(path).expect("the demo is readable");
        let mut demo = content.as_slice();
        let mut decoder = Decoder::default();
        let mut snapshots = 0;
        let mut server_time = None;
        let mut client = 0;
        let (mut encoder, mut checker) = (Encoder::default(), Decoder::default());
        let (mut messages, mut identical) = (0, 0);

        while let Some(block) = read_block(&mut demo).expect("well formed blocks") {
            let message = decoder
                .decode(block.sequence, block.message)
                .expect("decodable");
            let encoded = encoder.encode(block.sequence, &message).expect("encodable");
            let reread = checker.decode(block.sequence, &encoded);
            assert_eq!(reread.as_ref(), Ok(&message));
            messages += 1;
            identical += usize::from(equal_up_to_padding(&encoded, block.message));
            for operation in message.operations {
                match operation {
                    Operation::Gamestate(gamestate) => {
                        client = gamestate.client_number;
                        let info = String::from_utf8_lossy(&gamestate.configstrings[0].1);
                        assert!(info.contains("\\protocol\\91"), "{info}");
                        let characters: usize = gamestate
                            .configstrings
                            .iter()
                            .map(|(_, value)| value.len() + 1)
                            .sum();
                        println!(
                            "client {client}, {} configstrings, {characters} characters",
                            gamestate.configstrings.len()
                        );
                    }
                    Operation::ServerCommand { sequence, command } => {
                        println!(
                            "command {sequence}: {:.100}",
                            String::from_utf8_lossy(&command)
                        );
                    }
                    Operation::Snapshot(snapshot) => {
                        assert!(server_time < Some(snapshot.server_time));
                        server_time = Some(snapshot.server_time);
                        if snapshots % 400 == 0 {
                            let player = &snapshot.player;
                            let position =
                                [1, 2, 9].map(|field| f32::from_bits(player.fields[field]));
                            let types = snapshot.entities.iter().fold(
                                alloc::collections::BTreeMap::new(),
                                |mut types, entity| {
                                    *types.entry(entity.fields[12]).or_insert(0) += 1;
                                    types
                                },
                            );
                            println!(
                                "time {}, player {} at {position:?}, health {}, entity types {types:?}",
                                snapshot.server_time, player.fields[40], player.stats[0]
                            );
                        }
                        snapshots += 1;
                    }
                }
            }
        }

        assert!(demo.len() < 8, "data after the end marker");
        println!("{snapshots} snapshots of client {client}");
        println!("{identical} of {messages} messages encoded like the original");
        assert!(snapshots > 100);
    }

    #[test]
    #[ignore = "needs a protocol 73 demo named in SHINQLX_DM73_DEMO"]
    fn a_recorded_protocol_73_demo_is_decoded_completely() {
        let path = std::env::var("SHINQLX_DM73_DEMO").expect("SHINQLX_DM73_DEMO is set");
        let content = std::fs::read(path).expect("the demo is readable");
        let mut demo = content.as_slice();
        let mut decoder = Decoder::protocol_73();
        let (mut encoder, mut checker) = (Encoder::protocol_73(), Decoder::protocol_73());
        let mut identical = 0;
        let (mut messages, mut snapshots, mut commands, mut skipped) = (0, 0, 0, 0);
        let mut entities = 0;

        while let Some(block) = read_block(&mut demo).expect("well formed blocks") {
            messages += 1;
            match decoder.decode(block.sequence, block.message) {
                Ok(message) => {
                    let encoded = encoder.encode(block.sequence, &message).expect("encodable");
                    identical += usize::from(equal_up_to_padding(&encoded, block.message));
                    let reread = checker.decode(block.sequence, &encoded);
                    assert_eq!(reread.as_ref(), Ok(&message), "message {messages}");
                    for operation in message.operations {
                        match operation {
                            Operation::Gamestate(gamestate) => {
                                let info = String::from_utf8_lossy(&gamestate.configstrings[0].1)
                                    .into_owned();
                                assert!(info.contains("\\protocol\\73"), "{info}");
                                assert!(gamestate.baselines.len() > 1);
                            }
                            Operation::ServerCommand { .. } => commands += 1,
                            Operation::Snapshot(snapshot) => {
                                snapshots += 1;
                                entities = entities.max(snapshot.entities.len());
                            }
                        }
                    }
                }
                Err(DemoError::MissingDeltaBase(_)) => skipped += 1,
                Err(error) => panic!("message {messages}: {error}"),
            }
        }

        println!(
            "{messages} messages, {snapshots} snapshots, {commands} commands, {skipped} skipped, \
             up to {entities} entities, {identical} encoded like the engine"
        );
        assert!(snapshots > 100);
    }
}
