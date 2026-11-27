//! The recording format `.dm_sqx`: a header followed by records `{u8 kind, u32 length, payload}`.

use alloc::{
    collections::{BTreeMap, VecDeque},
    string::{String, ToString},
    vec::Vec,
};
use std::io::{self, Read, Write};

use derive_more::Display;

use super::{
    actions::{Damage, Input, Shot, decode_inputs, encode_inputs},
    capture::Frame,
    error::DemoError,
    frame_coding::{Previous, decode_frame, encode_frame},
    writer::Event,
};

const MAGIC: &[u8; 8] = b"dm_sqx\r\n";
const VERSION: u16 = 2;
const MAX_RECORD_LENGTH: usize = 1 << 24;
/// A full frame this often limits what a damaged file loses.
const KEY_FRAME_INTERVAL: i32 = 10_000;
const EVERY_CLIENT: u8 = u8::MAX;

const KIND_START: u8 = 1;
const KIND_CONFIGSTRING: u8 = 2;
const KIND_KEY_FRAME: u8 = 3;
const KIND_DELTA_FRAME: u8 = 4;
const KIND_COMMAND: u8 = 5;
const KIND_PLAYER_LEFT: u8 = 6;
const KIND_END: u8 = 7;
const KIND_STATS: u8 = 8;
const KIND_INPUTS: u8 = 9;
const KIND_SHOT: u8 = 10;
const KIND_DAMAGE: u8 = 11;
const NO_ATTACKER: u16 = u16::MAX;

/// Why a recording cannot be read or written.
#[derive(Debug, Display)]
pub enum ArchiveError {
    /// The file does not start like a recording.
    #[display("this is not a dm_sqx recording")]
    NotARecording,
    /// The recording was written by a newer version.
    #[display("version {_0} of the format is not supported")]
    UnsupportedVersion(u16),
    /// A record does not contain what its kind promises.
    #[display("damaged record: {_0}")]
    Damaged(DemoError),
    /// The file cannot be read or written.
    #[display("{_0}")]
    Io(io::Error),
}

impl core::error::Error for ArchiveError {}

impl From<DemoError> for ArchiveError {
    fn from(error: DemoError) -> Self {
        Self::Damaged(error)
    }
}

impl From<io::Error> for ArchiveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// One entry of a recording.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Record {
    /// What was valid when the recording started.
    Start {
        /// Descriptions like the map, as key and value.
        meta: Vec<(String, String)>,
        /// All configstrings that were set.
        configstrings: Vec<(u16, Vec<u8>)>,
    },
    /// Something that happened during the recording.
    Event(Event),
}

/// A command and everyone it was sent to one after another.
#[derive(Debug)]
struct SentCommand {
    recipients: Vec<u8>,
    command: Vec<u8>,
}

/// Writes a recording front to back, without ever seeking.
#[derive(Debug)]
pub struct ArchiveWriter<W: Write> {
    output: W,
    previous: Previous,
    previous_inputs: BTreeMap<u8, Input>,
    unwritten_command: Option<SentCommand>,
    key_frame_time: Option<i32>,
    written: u64,
}

impl<W: Write> ArchiveWriter<W> {
    /// Writes the header and what is valid now.
    pub fn start(
        output: W,
        meta: &[(String, String)],
        configstrings: &[(u16, Vec<u8>)],
    ) -> Result<Self, ArchiveError> {
        let mut writer = Self {
            output,
            previous: Previous::default(),
            previous_inputs: BTreeMap::new(),
            unwritten_command: None,
            key_frame_time: None,
            written: 0,
        };
        writer.output.write_all(MAGIC)?;
        writer.output.write_all(&VERSION.to_le_bytes())?;
        writer.written += (MAGIC.len() + size_of::<u16>()) as u64;

        let mut payload = Vec::new();
        push_length(&mut payload, meta.len())?;
        for (key, value) in meta {
            push_bytes(&mut payload, key.as_bytes())?;
            push_bytes(&mut payload, value.as_bytes())?;
        }
        push_length(&mut payload, configstrings.len())?;
        for (index, value) in configstrings {
            payload.extend_from_slice(&index.to_le_bytes());
            push_bytes(&mut payload, value)?;
        }
        writer.record(KIND_START, &payload)?;
        writer.output.flush()?;
        Ok(writer)
    }

    /// Appends one event.
    ///
    /// A command is held back until the next other event, to store it once for all recipients.
    pub fn event(&mut self, event: &Event) -> Result<(), ArchiveError> {
        if let Event::Command { recipient, command } = event {
            return self.command(recipient.unwrap_or(EVERY_CLIENT), command);
        }
        self.write_command()?;
        match event {
            Event::Frame(frame) => self.frame(frame),
            Event::Configstring { index, value } => {
                let payload = [&index.to_le_bytes()[..], value].concat();
                self.record(KIND_CONFIGSTRING, &payload)
            }
            Event::Command { .. } => Ok(()),
            Event::PlayerLeft { client } => self.record(KIND_PLAYER_LEFT, &[*client]),
            Event::End { reason } => self.record(KIND_END, reason.as_bytes()),
            Event::Stats { message } => self.record(KIND_STATS, message.as_bytes()),
            Event::Inputs(inputs) => {
                let payload = encode_inputs(&mut self.previous_inputs, inputs)?;
                self.record(KIND_INPUTS, &payload)
            }
            Event::Shot(shot) => self.record(KIND_SHOT, &shot_payload(shot)),
            Event::Damage(damage) => self.record(KIND_DAMAGE, &damage_payload(damage)),
        }
    }

    /// Writes and flushes everything, returning the number of bytes written and the output.
    pub fn finish(mut self) -> Result<(u64, W), ArchiveError> {
        self.write_command()?;
        self.output.flush()?;
        Ok((self.written, self.output))
    }

    fn command(&mut self, recipient: u8, command: &[u8]) -> Result<(), ArchiveError> {
        if let Some(sent) = self.unwritten_command.as_mut()
            && sent.command == command
            && sent.recipients.len() < usize::from(u8::MAX)
        {
            sent.recipients.push(recipient);
            return Ok(());
        }
        self.write_command()?;
        self.unwritten_command = Some(SentCommand {
            recipients: vec![recipient],
            command: command.to_vec(),
        });
        Ok(())
    }

    fn write_command(&mut self) -> Result<(), ArchiveError> {
        let Some(sent) = self.unwritten_command.take() else {
            return Ok(());
        };
        let count = u8::try_from(sent.recipients.len()).map_err(io::Error::other)?;
        let payload = [&[count][..], &sent.recipients, &sent.command].concat();
        self.record(KIND_COMMAND, &payload)
    }

    fn frame(&mut self, frame: &Frame) -> Result<(), ArchiveError> {
        let is_key_frame = self.key_frame_time.is_none_or(|time| {
            frame.server_time.saturating_sub(time) >= KEY_FRAME_INTERVAL || frame.server_time < time
        });
        if is_key_frame {
            self.previous = Previous::default();
            self.previous_inputs.clear();
            self.key_frame_time = Some(frame.server_time);
        }
        let payload = encode_frame(&mut self.previous, frame)?;
        let kind = if is_key_frame {
            KIND_KEY_FRAME
        } else {
            KIND_DELTA_FRAME
        };
        self.record(kind, &payload)?;
        if is_key_frame {
            self.output.flush()?;
        }
        Ok(())
    }

    fn record(&mut self, kind: u8, payload: &[u8]) -> Result<(), ArchiveError> {
        let length = u32::try_from(payload.len()).map_err(io::Error::other)?;
        self.output.write_all(&[kind])?;
        self.output.write_all(&length.to_le_bytes())?;
        self.output.write_all(payload)?;
        self.written += 5 + u64::from(length);
        Ok(())
    }
}

/// Reads a recording front to back.
#[derive(Debug)]
pub struct ArchiveReader<R: Read> {
    input: R,
    previous: Previous,
    previous_inputs: BTreeMap<u8, Input>,
    /// What is left of a record that stands for several events.
    queued: VecDeque<Record>,
    truncated: bool,
}

impl<R: Read> ArchiveReader<R> {
    /// Checks the header.
    pub fn new(mut input: R) -> Result<Self, ArchiveError> {
        let mut header = [0; 10];
        input.read_exact(&mut header).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                ArchiveError::NotARecording
            } else {
                ArchiveError::Io(error)
            }
        })?;
        let (magic, version) = header.split_at(MAGIC.len());
        if magic != MAGIC {
            return Err(ArchiveError::NotARecording);
        }
        let version = u16::from_le_bytes([version[0], version[1]]);
        if version != VERSION {
            return Err(ArchiveError::UnsupportedVersion(version));
        }
        Ok(Self {
            input,
            previous: Previous::default(),
            previous_inputs: BTreeMap::new(),
            queued: VecDeque::new(),
            truncated: false,
        })
    }

    /// Whether the file ended in the middle of a record.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// The next record, skipping unknown kinds; `None` after the last complete one.
    pub fn next_record(&mut self) -> Result<Option<Record>, ArchiveError> {
        loop {
            if let Some(record) = self.queued.pop_front() {
                return Ok(Some(record));
            }
            let Some((kind, payload)) = self.read_raw()? else {
                return Ok(None);
            };
            let mut rest = payload.as_slice();
            let record = match kind {
                KIND_START => read_start(&mut rest)?,
                KIND_CONFIGSTRING => {
                    let index = u16::from_le_bytes(take(&mut rest)?);
                    Record::Event(Event::Configstring {
                        index,
                        value: rest.to_vec(),
                    })
                }
                KIND_KEY_FRAME | KIND_DELTA_FRAME => {
                    if kind == KIND_KEY_FRAME {
                        self.previous = Previous::default();
                        self.previous_inputs.clear();
                    }
                    Record::Event(Event::Frame(decode_frame(&mut self.previous, rest)?))
                }
                KIND_COMMAND => {
                    self.queued = read_commands(&mut rest)?;
                    continue;
                }
                KIND_PLAYER_LEFT => {
                    let [client] = take(&mut rest)?;
                    Record::Event(Event::PlayerLeft { client })
                }
                KIND_END => Record::Event(Event::End {
                    reason: String::from_utf8_lossy(rest).to_string(),
                }),
                KIND_STATS => Record::Event(Event::Stats {
                    message: String::from_utf8_lossy(rest).to_string(),
                }),
                KIND_INPUTS => Record::Event(Event::Inputs(decode_inputs(
                    &mut self.previous_inputs,
                    rest,
                )?)),
                KIND_SHOT => Record::Event(Event::Shot(read_shot(&mut rest)?)),
                KIND_DAMAGE => Record::Event(Event::Damage(read_damage(&mut rest)?)),
                _ => continue,
            };
            return Ok(Some(record));
        }
    }

    fn read_raw(&mut self) -> Result<Option<(u8, Vec<u8>)>, ArchiveError> {
        let mut header = Vec::new();
        self.input.by_ref().take(5).read_to_end(&mut header)?;
        let Ok([kind, a, b, c, d]) = <[u8; 5]>::try_from(header.as_slice()) else {
            self.truncated |= !header.is_empty();
            return Ok(None);
        };
        let length = u32::from_le_bytes([a, b, c, d]) as usize;
        if length > MAX_RECORD_LENGTH {
            return Err(DemoError::BlockLength(i32::try_from(length).unwrap_or(i32::MAX)).into());
        }
        let mut payload = Vec::new();
        self.input
            .by_ref()
            .take(length as u64)
            .read_to_end(&mut payload)?;
        if payload.len() < length {
            self.truncated = true;
            return Ok(None);
        }
        Ok(Some((kind, payload)))
    }
}

fn push_length(payload: &mut Vec<u8>, length: usize) -> Result<(), ArchiveError> {
    let length = u32::try_from(length).map_err(io::Error::other)?;
    payload.extend_from_slice(&length.to_le_bytes());
    Ok(())
}

fn push_bytes(payload: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ArchiveError> {
    push_length(payload, bytes.len())?;
    payload.extend_from_slice(bytes);
    Ok(())
}

fn take<const N: usize>(rest: &mut &[u8]) -> Result<[u8; N], DemoError> {
    let (taken, remaining) = rest.split_first_chunk().ok_or(DemoError::UnexpectedEnd)?;
    *rest = remaining;
    Ok(*taken)
}

fn take_length(rest: &mut &[u8]) -> Result<usize, DemoError> {
    Ok(u32::from_le_bytes(take(rest)?) as usize)
}

fn take_bytes(rest: &mut &[u8]) -> Result<Vec<u8>, DemoError> {
    let length = take_length(rest)?;
    let (bytes, remaining) = rest
        .split_at_checked(length)
        .ok_or(DemoError::UnexpectedEnd)?;
    *rest = remaining;
    Ok(bytes.to_vec())
}

/// One command event for every recipient of the record.
fn shot_payload(shot: &Shot) -> Vec<u8> {
    let mut payload = vec![shot.client];
    payload.extend_from_slice(&shot.weapon_number.to_le_bytes());
    payload.extend_from_slice(&shot.command_time.to_le_bytes());
    for bits in shot.origin_bits.iter().chain(&shot.view_angle_bits) {
        payload.extend_from_slice(&bits.to_le_bytes());
    }
    payload
}

fn read_shot(rest: &mut &[u8]) -> Result<Shot, DemoError> {
    let [client] = take(rest)?;
    let weapon = u32::from_le_bytes(take(rest)?);
    let command_time = i32::from_le_bytes(take(rest)?);
    let mut vector = || -> Result<[u32; 3], DemoError> {
        let mut bits = [0; 3];
        for value in &mut bits {
            *value = u32::from_le_bytes(take(rest)?);
        }
        Ok(bits)
    };
    Ok(Shot {
        client,
        weapon_number: weapon,
        command_time,
        origin_bits: vector()?,
        view_angle_bits: vector()?,
    })
}

fn damage_payload(damage: &Damage) -> Vec<u8> {
    let mut payload = vec![damage.target, u8::from(damage.fatal)];
    payload.extend_from_slice(&damage.attacker.unwrap_or(NO_ATTACKER).to_le_bytes());
    payload.extend_from_slice(&damage.amount.to_le_bytes());
    payload.extend_from_slice(&damage.flags.to_le_bytes());
    payload.extend_from_slice(&damage.means_number.to_le_bytes());
    payload.extend_from_slice(&damage.health_lost.to_le_bytes());
    payload.extend_from_slice(&damage.armor_lost.to_le_bytes());
    payload
}

fn read_damage(rest: &mut &[u8]) -> Result<Damage, DemoError> {
    let [target, fatal] = take(rest)?;
    let attacker = u16::from_le_bytes(take(rest)?);
    Ok(Damage {
        target,
        fatal: fatal != 0,
        attacker: (attacker != NO_ATTACKER).then_some(attacker),
        amount: i32::from_le_bytes(take(rest)?),
        flags: u32::from_le_bytes(take(rest)?),
        means_number: u32::from_le_bytes(take(rest)?),
        health_lost: i32::from_le_bytes(take(rest)?),
        armor_lost: i32::from_le_bytes(take(rest)?),
    })
}

fn read_commands(rest: &mut &[u8]) -> Result<VecDeque<Record>, DemoError> {
    let [count] = take(rest)?;
    let (recipients, command) = rest
        .split_at_checked(usize::from(count))
        .ok_or(DemoError::UnexpectedEnd)?;
    Ok(recipients
        .iter()
        .map(|&recipient| {
            Record::Event(Event::Command {
                recipient: Some(recipient).filter(|&client| client != EVERY_CLIENT),
                command: command.to_vec(),
            })
        })
        .collect())
}

fn read_start(rest: &mut &[u8]) -> Result<Record, DemoError> {
    let text = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned();
    let meta = (0..take_length(rest)?)
        .map(|_| Ok((text(take_bytes(rest)?), text(take_bytes(rest)?))))
        .collect::<Result<_, DemoError>>()?;
    let configstrings = (0..take_length(rest)?)
        .map(|_| Ok((u16::from_le_bytes(take(rest)?), take_bytes(rest)?)))
        .collect::<Result<_, DemoError>>()?;
    Ok(Record::Start {
        meta,
        configstrings,
    })
}

#[cfg(test)]
mod archive_tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        capture::{CapturedEntity, Visibility},
        state::{EntityState, PlayerState},
    };

    fn entity(number: u16, health: u32, visibility: Visibility) -> CapturedEntity {
        let mut state = EntityState {
            number,
            ..EntityState::default()
        };
        state.fields[55] = health;
        CapturedEntity { state, visibility }
    }

    fn player(health: i16) -> PlayerState {
        let mut player = PlayerState::default();
        player.stats[0] = health;
        player
    }

    fn frame(server_time: i32, entities: Vec<CapturedEntity>, players: &[(u8, i16)]) -> Event {
        Event::Frame(Frame {
            server_time,
            entities,
            players: players
                .iter()
                .map(|&(client, health)| (client, player(health)))
                .collect(),
        })
    }

    fn events() -> Vec<Event> {
        vec![
            frame(
                25,
                vec![
                    entity(0, 100, Visibility::Everyone),
                    entity(70, 0, Visibility::Only(1)),
                ],
                &[(0, 100), (1, 125)],
            ),
            Event::Command {
                recipient: Some(1),
                command: b"chat \"hi\"".to_vec(),
            },
            Event::Command {
                recipient: None,
                command: b"print x".to_vec(),
            },
            Event::Configstring {
                index: 529,
                value: b"n\\Someone".to_vec(),
            },
            frame(
                50,
                vec![
                    entity(0, 80, Visibility::AllBut(3)),
                    entity(70, 0, Visibility::Everyone),
                    entity(900, 1, Visibility::Everyone),
                ],
                &[(1, 120)],
            ),
            Event::PlayerLeft { client: 0 },
            Event::Stats {
                message: r#"{"DATA":{},"TYPE":"ROUND_OVER"}"#.into(),
            },
            Event::Inputs(vec![input(25, 100), input(33, -200)]),
            Event::Shot(Shot {
                client: 1,
                weapon_number: 7,
                command_time: 33,
                origin_bits: [1.5f32.to_bits(), 0, (-8.0f32).to_bits()],
                view_angle_bits: [0, 90.0f32.to_bits(), 0],
            }),
            Event::Damage(Damage {
                target: 0,
                attacker: Some(1),
                amount: 80,
                flags: 2,
                means_number: 10,
                health_lost: 27,
                armor_lost: 53,
                fatal: true,
            }),
            Event::Damage(Damage {
                attacker: None,
                ..Damage::default()
            }),
            frame(
                20_000,
                vec![entity(900, 1, Visibility::Only(0))],
                &[(0, 5), (1, 120)],
            ),
            Event::End {
                reason: "the map ended".into(),
            },
        ]
    }

    fn input(server_time: i32, yaw: i32) -> Input {
        Input {
            client: 1,
            server_time,
            angles: [0, yaw, 0],
            buttons: 1,
            weapon: 7,
            forward: 127,
            right: -127,
            ..Input::default()
        }
    }

    fn recording() -> Vec<u8> {
        let meta = [("map".to_string(), "campgrounds".to_string())];
        let mut writer =
            ArchiveWriter::start(Vec::new(), &meta, &[(0, b"info".to_vec())]).expect("started");
        for event in events() {
            writer.event(&event).expect("written");
        }
        let (written, data) = writer.finish().expect("finished");
        assert_eq!(written, data.len() as u64);
        data
    }

    fn read_all(data: &[u8]) -> (Vec<Record>, bool) {
        let mut reader = ArchiveReader::new(data).expect("a recording");
        let records = core::iter::from_fn(|| reader.next_record().expect("readable")).collect();
        (records, reader.truncated())
    }

    #[test]
    fn a_recording_is_read_back() {
        let (records, truncated) = read_all(&recording());

        let expected: Vec<Record> = core::iter::once(Record::Start {
            meta: vec![("map".into(), "campgrounds".into())],
            configstrings: vec![(0, b"info".to_vec())],
        })
        .chain(events().into_iter().map(Record::Event))
        .collect();
        assert_eq!(records, expected);
        assert!(!truncated);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_truncated_recording_is_readable_up_to_the_last_complete_record() {
        let data = recording();
        let (complete, _) = read_all(&data);

        for length in 10..data.len() {
            let (records, _) = read_all(&data[..length]);

            assert_eq!(records, complete[..records.len()]);
        }
        let (records, truncated) = read_all(&data[..data.len() - 1]);
        assert_eq!(records.len(), complete.len() - 1);
        assert!(truncated);
    }

    #[test]
    fn a_command_for_several_recipients_is_stored_once() {
        let command = |recipient| Event::Command {
            recipient: Some(recipient),
            command: b"chat \"to the whole server\"".to_vec(),
        };
        let size_with = |recipients: &[u8]| {
            let mut writer = ArchiveWriter::start(Vec::new(), &[], &[]).expect("started");
            for &recipient in recipients {
                writer.event(&command(recipient)).expect("written");
            }
            writer.finish().expect("finished").1
        };

        let (one, three) = (size_with(&[4]), size_with(&[4, 2, 4]));

        assert_eq!(three.len(), one.len() + 2);
        let (records, _) = read_all(&three);
        assert_eq!(
            records[1..],
            [4, 2, 4].map(|recipient| Record::Event(command(recipient)))
        );
    }

    #[test]
    fn unknown_kinds_are_skipped() {
        let mut data = recording();
        data.extend_from_slice(&[200, 1, 0, 0, 0, 42]);

        let (records, truncated) = read_all(&data);

        assert_eq!(records.len(), events().len() + 1);
        assert!(!truncated);
    }

    #[test]
    fn other_files_are_rejected() {
        assert!(matches!(
            ArchiveReader::new(&b"0123456789abc"[..]),
            Err(ArchiveError::NotARecording)
        ));
        assert!(matches!(
            ArchiveReader::new(&b"dm_sqx\r\n\x63\x00"[..]),
            Err(ArchiveError::UnsupportedVersion(99))
        ));
    }
}
