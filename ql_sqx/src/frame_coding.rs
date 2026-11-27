//! The payload of the frame records: the player states first, as the entities are predicted from them.

use alloc::{collections::BTreeMap, vec::Vec};

use super::{
    bits::{BitReader, BitWriter},
    capture::{CapturedEntity, Frame, Visibility},
    delta,
    error::DemoError,
    fields::{ENTITY_FIELDS, Field, PLAYER_FIELDS},
    message::{check_order, read_entities, write_entities},
    player_coding::RecordingCoding,
    prediction::predicted_entity,
    state::{ENTITY_NUMBER_BITS, ENTITY_NUMBER_END, EntityState, PlayerState},
};

const VISIBLE_TO_EVERYONE: u32 = 0;
const VISIBLE_TO_ONE: u32 = 1;
const VISIBLE_TO_ALL_BUT_ONE: u32 = 2;

/// The state later frames are deltas of.
#[derive(Debug, Default)]
pub(super) struct Previous {
    entities: Vec<CapturedEntity>,
    players: BTreeMap<u8, PlayerState>,
}

pub(super) fn encode_frame(previous: &mut Previous, frame: &Frame) -> Result<Vec<u8>, DemoError> {
    check_order(&states(&frame.entities))?;
    let frame = as_carried(frame);
    let mut writer = BitWriter::default();
    writer.write_long(frame.server_time);

    writer.write_byte(u8::try_from(frame.players.len()).map_err(|_| DemoError::PlayerCount)?);
    let coding = RecordingCoding {
        server_time: frame.server_time,
    };
    let no_player = PlayerState::default();
    for (client, player) in &frame.players {
        writer.write_byte(*client);
        let from = previous.players.get(client).unwrap_or(&no_player);
        delta::write_player(&mut writer, &PLAYER_FIELDS, &coding, from, player);
    }

    let expected = Expected::new(&previous.entities, &frame.players);
    write_entities(
        &mut writer,
        &ENTITY_FIELDS,
        &expected.new_entities,
        &expected.known_entities,
        &states(&frame.entities),
    );
    for entity in &frame.entities {
        let number = entity.state.number;
        if visibility_of(&previous.entities, number) == entity.visibility {
            continue;
        }
        writer.write(u32::from(number), ENTITY_NUMBER_BITS);
        let (kind, client) = match entity.visibility {
            Visibility::Everyone => (VISIBLE_TO_EVERYONE, None),
            Visibility::Only(client) => (VISIBLE_TO_ONE, Some(client)),
            Visibility::AllBut(client) => (VISIBLE_TO_ALL_BUT_ONE, Some(client)),
        };
        writer.write(kind, 2);
        if let Some(client) = client {
            writer.write_long(client);
        }
    }
    writer.write(u32::from(ENTITY_NUMBER_END), ENTITY_NUMBER_BITS);

    previous.remember(&frame);
    Ok(writer.into_message())
}

pub(super) fn decode_frame(previous: &mut Previous, payload: &[u8]) -> Result<Frame, DemoError> {
    let mut reader = BitReader::new(payload);
    let server_time = reader.read_long()?;

    let coding = RecordingCoding { server_time };
    let no_player = PlayerState::default();
    let players = (0..reader.read_byte()?)
        .map(|_| {
            let client = reader.read_byte()?;
            let from = previous.players.get(&client).unwrap_or(&no_player);
            let player = delta::read_player(&mut reader, &PLAYER_FIELDS, &coding, from)?;
            Ok((client, player))
        })
        .collect::<Result<Vec<_>, DemoError>>()?;

    let expected = Expected::new(&previous.entities, &players);
    let mut entities: Vec<CapturedEntity> = read_entities(
        &mut reader,
        &ENTITY_FIELDS,
        &expected.new_entities,
        &expected.known_entities,
    )?
    .into_iter()
    .map(|state| CapturedEntity {
        visibility: visibility_of(&previous.entities, state.number),
        state,
    })
    .collect();
    loop {
        let number = reader.read(ENTITY_NUMBER_BITS)? as u16;
        if number == ENTITY_NUMBER_END {
            break;
        }
        let visibility = match reader.read(2)? {
            VISIBLE_TO_EVERYONE => Visibility::Everyone,
            VISIBLE_TO_ONE => Visibility::Only(reader.read_long()?),
            _ => Visibility::AllBut(reader.read_long()?),
        };
        let index = entities
            .binary_search_by_key(&number, |entity| entity.state.number)
            .map_err(|_| DemoError::EntityNumber(number))?;
        entities[index].visibility = visibility;
    }

    let frame = Frame {
        server_time,
        entities,
        players,
    };
    previous.remember(&frame);
    Ok(frame)
}

impl Previous {
    fn remember(&mut self, frame: &Frame) {
        self.entities.clone_from(&frame.entities);
        self.players = frame.players.iter().copied().collect();
    }
}

/// What the entities of a frame are deltas of: the entities as the player states predict them.
struct Expected {
    known_entities: Vec<EntityState>,
    new_entities: Vec<EntityState>,
}

impl Expected {
    fn new(previous: &[CapturedEntity], players: &[(u8, PlayerState)]) -> Self {
        let mut known_entities = states(previous);
        let mut new_entities = vec![EntityState::default(); usize::from(ENTITY_NUMBER_END)];
        for (client, player) in players {
            let number = u16::from(*client);
            new_entities[usize::from(number)] = predicted_entity(EntityState::default(), player);
            if let Ok(index) = known_entities.binary_search_by_key(&number, |entity| entity.number)
            {
                known_entities[index] = predicted_entity(known_entities[index], player);
            }
        }
        Self {
            known_entities,
            new_entities,
        }
    }
}

/// The frame as a reader gets it back, which writer and reader have to predict from alike.
fn as_carried(frame: &Frame) -> Frame {
    let mut frame = frame.clone();
    for entity in &mut frame.entities {
        carry(&mut entity.state.fields, &ENTITY_FIELDS);
    }
    for (_, player) in &mut frame.players {
        carry(&mut player.fields, &PLAYER_FIELDS);
    }
    frame
}

fn carry(values: &mut [u32], table: &[Field]) {
    for (value, field) in values.iter_mut().zip(table) {
        *value = delta::carried(field.kind, *value);
    }
}

fn states(entities: &[CapturedEntity]) -> Vec<EntityState> {
    entities.iter().map(|entity| entity.state).collect()
}

fn visibility_of(entities: &[CapturedEntity], number: u16) -> Visibility {
    entities
        .binary_search_by_key(&number, |entity| entity.state.number)
        .map_or(Visibility::Everyone, |index| entities[index].visibility)
}

#[cfg(test)]
mod frame_coding_tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use super::*;
    use crate::fields::{entity_field, player_field};

    /// A frame with one player and the entity of this player, from arbitrary bits.
    fn frame() -> impl Strategy<Value = Frame> {
        let sparse = || prop_oneof![3 => Just(0u32), 1 => any::<u32>()];
        (
            any::<i32>(),
            prop::collection::vec(sparse(), 58),
            prop::collection::vec(sparse(), 58),
            any::<[i16; 16]>(),
            any::<bool>(),
        )
            .prop_map(|(server_time, entity, player, stats, has_entity)| {
                let entity = CapturedEntity {
                    state: EntityState {
                        number: 3,
                        fields: core::array::from_fn(|index| entity[index]),
                    },
                    visibility: Visibility::Everyone,
                };
                let player = PlayerState {
                    fields: core::array::from_fn(|index| player[index]),
                    stats,
                    ..PlayerState::default()
                };
                Frame {
                    server_time,
                    entities: has_entity.then_some(entity).into_iter().collect(),
                    players: vec![(3, player)],
                }
            })
    }

    proptest! {
        #[test]
        #[cfg_attr(miri, ignore)]
        fn frames_are_read_back_as_carried(frames in prop::collection::vec(frame(), 1..5)) {
            let (mut written, mut read) = (Previous::default(), Previous::default());

            for frame in frames {
                let payload = encode_frame(&mut written, &frame).expect("encodable");
                let decoded = decode_frame(&mut read, &payload);

                prop_assert_eq!(decoded, Ok(as_carried(&frame)));
            }
        }
    }

    #[test]
    fn an_entity_following_its_player_costs_nothing() {
        let frame = |server_time, x: f32, with_entity: bool| {
            let mut player = PlayerState::default();
            player.fields[player_field("origin[0]")] = x.to_bits();
            let mut entity = EntityState::default();
            entity.fields[entity_field("pos.trBase[0]")] = x.trunc().to_bits();
            let entity = CapturedEntity {
                state: entity,
                visibility: Visibility::Everyone,
            };
            Frame {
                server_time,
                entities: with_entity.then_some(entity).into_iter().collect(),
                players: vec![(0, player)],
            }
        };
        let second_frame_length = |with_entity| {
            let mut previous = Previous::default();
            encode_frame(&mut previous, &frame(25, 10.5, with_entity)).expect("encodable");
            encode_frame(&mut previous, &frame(50, 20.5, with_entity))
                .expect("encodable")
                .len()
        };

        assert_eq!(second_frame_length(true), second_frame_length(false));
    }
}
