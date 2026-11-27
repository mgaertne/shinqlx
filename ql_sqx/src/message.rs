use alloc::{boxed::Box, collections::VecDeque, vec, vec::Vec};
use core::cmp::Ordering;

use super::{
    bits::{BitReader, BitWriter},
    delta::{self, Netfields},
    error::DemoError,
    fields::{ENTITY_FIELDS, Field, PLAYER_FIELDS},
    state::{ENTITY_NUMBER_BITS, ENTITY_NUMBER_END, EntityState, PlayerState},
};

const OP_NOP: u8 = 1;
const OP_GAMESTATE: u8 = 2;
const OP_CONFIGSTRING: u8 = 3;
const OP_BASELINE: u8 = 4;
const OP_SERVER_COMMAND: u8 = 5;
const OP_SNAPSHOT: u8 = 7;
const OP_END: u8 = 8;

/// The client keeps this many messages to delta a snapshot from.
const PACKET_BACKUP: i32 = 32;
const MAX_AREA_MASK_LENGTH: usize = 32;
pub(super) const MAX_COMMAND_LENGTH: usize = 1024;
const MAX_CONFIGSTRING_LENGTH: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Gamestate {
    pub(crate) command_sequence: i32,
    pub(crate) configstrings: Vec<(u16, Vec<u8>)>,
    pub(crate) baselines: Vec<EntityState>,
    pub(crate) client_number: i32,
    pub(crate) checksum_feed: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) server_time: i32,
    pub(crate) flags: u8,
    pub(crate) area_mask: Vec<u8>,
    pub(crate) player: PlayerState,
    /// In strictly increasing order of their numbers.
    pub(crate) entities: Vec<EntityState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Operation {
    Gamestate(Gamestate),
    ServerCommand { sequence: i32, command: Vec<u8> },
    Snapshot(Box<Snapshot>),
}

/// One message from the server to a client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) acknowledge: i32,
    pub(crate) operations: Vec<Operation>,
}

#[derive(Debug, Clone, Copy)]
struct Tables {
    entity: &'static [Field],
    player: &'static [Field],
}

const PROTOCOL_91: Tables = Tables {
    entity: &ENTITY_FIELDS,
    player: &PLAYER_FIELDS,
};

#[cfg(test)]
fn protocol_73_tables() -> Tables {
    let mut player = [&PLAYER_FIELDS[..42], &PLAYER_FIELDS[43..49]].concat();
    for field in player.iter_mut().filter(|field| field.name == "pm_flags") {
        field.kind = super::fields::FieldKind::Unsigned(16);
    }
    Tables {
        entity: &ENTITY_FIELDS[..53],
        player: player.leak(),
    }
}

#[cfg(test)]
impl Encoder {
    pub(super) fn protocol_73() -> Self {
        Self {
            tables: protocol_73_tables(),
            ..Self::default()
        }
    }
}

/// Turns the messages of one client into bytes, keeping what later messages are deltas of.
#[derive(Debug)]
pub(crate) struct Encoder {
    tables: Tables,
    baselines: Vec<EntityState>,
    previous: Option<(i32, Snapshot)>,
}

impl Default for Encoder {
    fn default() -> Self {
        Self {
            tables: PROTOCOL_91,
            baselines: empty_baselines(),
            previous: None,
        }
    }
}

impl Encoder {
    /// Encodes the message with the number `sequence`, which has to increase from message to message.
    pub(crate) fn encode(
        &mut self,
        sequence: i32,
        message: &Message,
    ) -> Result<Vec<u8>, DemoError> {
        let mut writer = BitWriter::default();
        writer.write_long(message.acknowledge);
        for operation in &message.operations {
            match operation {
                Operation::Gamestate(gamestate) => self.write_gamestate(&mut writer, gamestate)?,
                Operation::ServerCommand { sequence, command } => {
                    writer.write_byte(OP_SERVER_COMMAND);
                    writer.write_long(*sequence);
                    write_string(&mut writer, command, MAX_COMMAND_LENGTH)?;
                }
                Operation::Snapshot(snapshot) => {
                    self.write_snapshot(&mut writer, sequence, snapshot)?;
                }
            }
        }
        writer.write_byte(OP_END);
        Ok(writer.into_message())
    }

    fn write_gamestate(
        &mut self,
        writer: &mut BitWriter,
        gamestate: &Gamestate,
    ) -> Result<(), DemoError> {
        self.baselines = empty_baselines();
        self.previous = None;

        writer.write_byte(OP_GAMESTATE);
        writer.write_long(gamestate.command_sequence);
        for (index, value) in &gamestate.configstrings {
            writer.write_byte(OP_CONFIGSTRING);
            writer.write(u32::from(*index), 16);
            write_string(writer, value, MAX_CONFIGSTRING_LENGTH)?;
        }
        for baseline in &gamestate.baselines {
            let slot = checked_number(baseline.number)?;
            writer.write_byte(OP_BASELINE);
            delta::write_entity(
                writer,
                self.tables.entity,
                &EntityState::default(),
                baseline,
                true,
            );
            self.baselines[slot] = *baseline;
        }
        writer.write_byte(OP_END);
        writer.write_long(gamestate.client_number);
        writer.write_long(gamestate.checksum_feed);
        Ok(())
    }

    fn write_snapshot(
        &mut self,
        writer: &mut BitWriter,
        sequence: i32,
        snapshot: &Snapshot,
    ) -> Result<(), DemoError> {
        if snapshot.area_mask.len() > MAX_AREA_MASK_LENGTH {
            return Err(DemoError::AreaMaskLength(snapshot.area_mask.len()));
        }
        check_order(&snapshot.entities)?;

        let base = self.previous.as_ref().and_then(|(base, previous)| {
            let distance = sequence.wrapping_sub(*base);
            (1..PACKET_BACKUP)
                .contains(&distance)
                .then_some((distance as u8, previous))
        });

        writer.write_byte(OP_SNAPSHOT);
        writer.write_long(snapshot.server_time);
        writer.write_byte(base.map_or(0, |(distance, _)| distance));
        writer.write_byte(snapshot.flags);
        writer.write_byte(snapshot.area_mask.len() as u8);
        for &byte in &snapshot.area_mask {
            writer.write_byte(byte);
        }
        let no_player = PlayerState::default();
        let previous_player = base.map_or(&no_player, |(_, previous)| &previous.player);
        delta::write_player(
            writer,
            self.tables.player,
            &Netfields,
            previous_player,
            &snapshot.player,
        );

        let previous_entities = base.map_or(&[][..], |(_, previous)| &previous.entities);
        write_entities(
            writer,
            self.tables.entity,
            &self.baselines,
            previous_entities,
            &snapshot.entities,
        );

        self.previous = Some((sequence, snapshot.clone()));
        Ok(())
    }
}

/// Turns the bytes of one client's messages back into messages.
#[derive(Debug)]
pub(crate) struct Decoder {
    tables: Tables,
    baselines: Vec<EntityState>,
    snapshots: VecDeque<(i32, Snapshot)>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            tables: PROTOCOL_91,
            baselines: empty_baselines(),
            snapshots: VecDeque::new(),
        }
    }
}

impl Decoder {
    /// Reads the older protocol 73, whose tables are shorter, to check the framing against recorded demos.
    #[cfg(test)]
    pub(super) fn protocol_73() -> Self {
        Self {
            tables: protocol_73_tables(),
            ..Self::default()
        }
    }

    pub(crate) fn decode(&mut self, sequence: i32, data: &[u8]) -> Result<Message, DemoError> {
        let mut reader = BitReader::new(data);
        let acknowledge = reader.read_long()?;
        let mut operations = Vec::new();
        loop {
            match reader.read_byte()? {
                OP_END => break,
                OP_NOP => {}
                OP_GAMESTATE => {
                    operations.push(Operation::Gamestate(self.read_gamestate(&mut reader)?));
                }
                OP_SERVER_COMMAND => operations.push(Operation::ServerCommand {
                    sequence: reader.read_long()?,
                    command: read_string(&mut reader)?,
                }),
                OP_SNAPSHOT => {
                    let snapshot = self.read_snapshot(&mut reader, sequence)?;
                    operations.push(Operation::Snapshot(Box::new(snapshot)));
                }
                unknown => return Err(DemoError::UnknownOperation(unknown)),
            }
        }
        Ok(Message {
            acknowledge,
            operations,
        })
    }

    fn read_gamestate(&mut self, reader: &mut BitReader<'_>) -> Result<Gamestate, DemoError> {
        self.baselines = empty_baselines();
        self.snapshots.clear();

        let command_sequence = reader.read_long()?;
        let mut configstrings = Vec::new();
        let mut baselines = Vec::new();
        loop {
            match reader.read_byte()? {
                OP_END => break,
                OP_CONFIGSTRING => {
                    let index = reader.read(16)? as u16;
                    configstrings.push((index, read_string(reader)?));
                }
                OP_BASELINE => {
                    let number = reader.read(ENTITY_NUMBER_BITS)? as u16;
                    let slot = checked_number(number)?;
                    if let Some(baseline) = delta::read_entity(
                        reader,
                        self.tables.entity,
                        &EntityState::default(),
                        number,
                    )? {
                        self.baselines[slot] = baseline;
                        baselines.push(baseline);
                    }
                }
                unknown => return Err(DemoError::UnknownOperation(unknown)),
            }
        }
        Ok(Gamestate {
            command_sequence,
            configstrings,
            baselines,
            client_number: reader.read_long()?,
            checksum_feed: reader.read_long()?,
        })
    }

    fn read_snapshot(
        &mut self,
        reader: &mut BitReader<'_>,
        sequence: i32,
    ) -> Result<Snapshot, DemoError> {
        let server_time = reader.read_long()?;
        let distance = reader.read_byte()?;
        let flags = reader.read_byte()?;
        let area_mask_length = usize::from(reader.read_byte()?);
        if area_mask_length > MAX_AREA_MASK_LENGTH {
            return Err(DemoError::AreaMaskLength(area_mask_length));
        }
        let area_mask = (0..area_mask_length)
            .map(|_| reader.read_byte())
            .collect::<Result<Vec<_>, _>>()?;

        let base = match distance {
            0 => None,
            _ => {
                let base_sequence = sequence.wrapping_sub(i32::from(distance));
                let (_, snapshot) = self
                    .snapshots
                    .iter()
                    .find(|(stored, _)| *stored == base_sequence)
                    .ok_or(DemoError::MissingDeltaBase(distance))?;
                Some(snapshot)
            }
        };
        let no_player = PlayerState::default();
        let previous_player = base.map_or(&no_player, |previous| &previous.player);
        let player = delta::read_player(reader, self.tables.player, &Netfields, previous_player)?;
        let entities = read_entities(
            reader,
            self.tables.entity,
            &self.baselines,
            base.map_or(&[][..], |previous| &previous.entities),
        )?;

        let snapshot = Snapshot {
            server_time,
            flags,
            area_mask,
            player,
            entities,
        };
        if self.snapshots.len() == PACKET_BACKUP as usize {
            self.snapshots.pop_front();
        }
        self.snapshots.push_back((sequence, snapshot.clone()));
        Ok(snapshot)
    }
}

fn empty_baselines() -> Vec<EntityState> {
    vec![EntityState::default(); usize::from(ENTITY_NUMBER_END)]
}

fn checked_number(number: u16) -> Result<usize, DemoError> {
    if number >= ENTITY_NUMBER_END {
        return Err(DemoError::EntityNumber(number));
    }
    Ok(usize::from(number))
}

pub(super) fn check_order(entities: &[EntityState]) -> Result<(), DemoError> {
    let mut lowest_allowed = 0;
    for entity in entities {
        checked_number(entity.number)?;
        if entity.number < lowest_allowed {
            return Err(DemoError::EntityNumber(entity.number));
        }
        lowest_allowed = entity.number + 1;
    }
    Ok(())
}

pub(super) fn write_entities(
    writer: &mut BitWriter,
    table: &[Field],
    baselines: &[EntityState],
    previous: &[EntityState],
    current: &[EntityState],
) {
    let mut previous = previous.iter().peekable();
    let mut current = current.iter().peekable();
    loop {
        let old_number = previous.peek().map_or(ENTITY_NUMBER_END, |old| old.number);
        let new_number = current.peek().map_or(ENTITY_NUMBER_END, |new| new.number);
        match old_number.cmp(&new_number) {
            Ordering::Less => {
                delta::write_entity_removal(writer, old_number);
                previous.next();
            }
            Ordering::Greater => {
                if let Some(new) = current.next() {
                    let baseline = &baselines[usize::from(new.number)];
                    delta::write_entity(writer, table, baseline, new, true);
                }
            }
            Ordering::Equal => {
                let Some((old, new)) = previous.next().zip(current.next()) else {
                    break;
                };
                delta::write_entity(writer, table, old, new, false);
            }
        }
    }
    writer.write(u32::from(ENTITY_NUMBER_END), ENTITY_NUMBER_BITS);
}

pub(super) fn read_entities(
    reader: &mut BitReader<'_>,
    table: &[Field],
    baselines: &[EntityState],
    previous: &[EntityState],
) -> Result<Vec<EntityState>, DemoError> {
    let mut previous = previous.iter().peekable();
    let mut entities = Vec::new();
    loop {
        let number = reader.read(ENTITY_NUMBER_BITS)? as u16;
        if number == ENTITY_NUMBER_END {
            break;
        }
        while let Some(unchanged) = previous.next_if(|old| old.number < number) {
            entities.push(*unchanged);
        }
        let from = previous
            .next_if(|old| old.number == number)
            .unwrap_or(&baselines[usize::from(number)]);
        if let Some(entity) = delta::read_entity(reader, table, from, number)? {
            entities.push(entity);
        }
    }
    entities.extend(previous.copied());
    Ok(entities)
}

fn write_string(writer: &mut BitWriter, value: &[u8], limit: usize) -> Result<(), DemoError> {
    let text = value.split(|&byte| byte == 0).next().unwrap_or_default();
    if text.len() >= limit {
        return Err(DemoError::StringTooLong(text.len()));
    }
    for &byte in text {
        writer.write_byte(if byte == b'%' { b'.' } else { byte });
    }
    writer.write_byte(0);
    Ok(())
}

fn read_string(reader: &mut BitReader<'_>) -> Result<Vec<u8>, DemoError> {
    let mut text = Vec::new();
    loop {
        match reader.read_byte()? {
            0 => return Ok(text),
            byte if text.len() < MAX_CONFIGSTRING_LENGTH => text.push(byte),
            _ => return Err(DemoError::StringTooLong(text.len())),
        }
    }
}

#[cfg(test)]
mod message_tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn entity(number: u16, health: u32) -> EntityState {
        let mut entity = EntityState {
            number,
            ..EntityState::default()
        };
        entity.fields[55] = health;
        entity
    }

    fn snapshot(server_time: i32, entities: Vec<EntityState>) -> Message {
        let mut player = PlayerState::default();
        player.fields[0] = server_time.cast_unsigned();
        player.stats[0] = 100;
        Message {
            acknowledge: 0,
            operations: vec![Operation::Snapshot(Box::new(Snapshot {
                server_time,
                flags: 4,
                area_mask: vec![0; 4],
                player,
                entities,
            }))],
        }
    }

    fn gamestate() -> Message {
        Message {
            acknowledge: 7,
            operations: vec![Operation::Gamestate(Gamestate {
                command_sequence: 3,
                configstrings: vec![
                    (0, b"\\protocol\\91".to_vec()),
                    (3, b"M\xc3\xbcller".to_vec()),
                ],
                baselines: vec![entity(5, 125), entity(900, 0)],
                client_number: 2,
                checksum_feed: -1,
            })],
        }
    }

    #[test]
    fn a_whole_recording_is_read_back() {
        let command = Message {
            acknowledge: 7,
            operations: vec![Operation::ServerCommand {
                sequence: 4,
                command: b"chat \"hello\"".to_vec(),
            }],
        };
        let messages = [
            gamestate(),
            snapshot(25, vec![entity(1, 100), entity(5, 125), entity(8, 0)]),
            command,
            snapshot(50, vec![entity(1, 80), entity(8, 0), entity(900, 1)]),
            snapshot(75, vec![]),
            snapshot(100, vec![entity(5, 125)]),
        ];
        let mut encoder = Encoder::default();
        let mut decoder = Decoder::default();

        for (sequence, message) in (1..).zip(&messages) {
            let data = encoder.encode(sequence, message).expect("encodable");

            assert_eq!(decoder.decode(sequence, &data).as_ref(), Ok(message));
        }
    }

    #[test]
    fn a_snapshot_too_far_from_the_previous_one_is_sent_in_full() {
        let mut encoder = Encoder::default();
        let first = snapshot(25, vec![entity(1, 100)]);
        let second = snapshot(50, vec![entity(1, 100)]);
        encoder.encode(1, &first).expect("encodable");

        let data = encoder.encode(33, &second).expect("encodable");

        assert_eq!(Decoder::default().decode(33, &data), Ok(second));
    }

    #[test]
    fn a_delta_without_its_base_is_an_error() {
        let mut encoder = Encoder::default();
        encoder.encode(1, &snapshot(25, vec![])).expect("encodable");
        let data = encoder.encode(2, &snapshot(50, vec![])).expect("encodable");

        assert_eq!(
            Decoder::default().decode(2, &data),
            Err(DemoError::MissingDeltaBase(1))
        );
    }

    #[test]
    fn entities_out_of_order_are_rejected() {
        let message = snapshot(25, vec![entity(8, 0), entity(8, 0)]);

        assert_eq!(
            Encoder::default().encode(1, &message),
            Err(DemoError::EntityNumber(8))
        );
    }

    #[test]
    fn percent_signs_are_replaced_like_in_the_engine() {
        let message = Message {
            acknowledge: 0,
            operations: vec![Operation::ServerCommand {
                sequence: 1,
                command: b"print 100%".to_vec(),
            }],
        };
        let data = Encoder::default().encode(1, &message).expect("encodable");

        let read = Decoder::default().decode(1, &data).expect("decodable");

        assert_eq!(
            read.operations,
            [Operation::ServerCommand {
                sequence: 1,
                command: b"print 100.".to_vec(),
            }]
        );
    }
}
