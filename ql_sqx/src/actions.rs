//! What players do and what happens to them, as the server sees it between two frames.

use alloc::{collections::BTreeMap, vec::Vec};

use super::{
    bits::{BitReader, BitWriter},
    error::DemoError,
    view::{MeansOfDeath, Weapon},
};

/// One command of a client: what was pressed and where the view pointed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Input {
    /// The client number.
    pub client: u8,
    /// The time the client made the command for, in milliseconds.
    pub server_time: i32,
    /// The view angles as the client sends them, in steps of 360/65536 degrees and without the
    /// offset `delta_angles` of the player state.
    pub angles: [i32; 3],
    /// The pressed buttons as bits; the lowest one is attack.
    pub buttons: i32,
    /// The selected weapon.
    pub weapon: u8,
    /// The primary weapon of the loadout.
    pub weapon_primary: u8,
    /// The field of view.
    pub fov: u8,
    /// Positive forward, negative back.
    pub forward: i8,
    /// Positive right, negative left.
    pub right: i8,
    /// Positive jump, negative crouch.
    pub up: i8,
}

/// A weapon being fired.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shot {
    /// The client number.
    pub client: u8,
    /// The number of the weapon, see [`Shot::weapon`].
    pub weapon_number: u32,
    /// The time of the command that fired, in milliseconds.
    pub command_time: i32,
    /// The bits of the position at the time of the shot, see [`Shot::origin`].
    pub origin_bits: [u32; 3],
    /// The bits of the view angles at the time of the shot, see [`Shot::view_angles`].
    pub view_angle_bits: [u32; 3],
}

impl Shot {
    /// The weapon, `None` for one that is not known.
    pub fn weapon(&self) -> Option<Weapon> {
        Weapon::from_number(self.weapon_number)
    }

    /// The position of the player when firing.
    pub fn origin(&self) -> [f32; 3] {
        self.origin_bits.map(f32::from_bits)
    }

    /// Where the player looked when firing, as pitch, yaw and roll in degrees.
    pub fn view_angles(&self) -> [f32; 3] {
        self.view_angle_bits.map(f32::from_bits)
    }
}

impl Damage {
    /// How the damage was done, `None` for a way that is not known.
    pub fn means(&self) -> Option<MeansOfDeath> {
        MeansOfDeath::from_number(self.means_number)
    }
}

/// Damage the game was asked to do to a living player.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Damage {
    /// The client number of who was hit.
    pub target: u8,
    /// The number of the entity responsible, which is the client number for players.
    pub attacker: Option<u16>,
    /// The damage asked for, before armor, protection, handicap and power-ups.
    pub amount: i32,
    /// The damage flags of the game.
    pub flags: u32,
    /// The number of the means of death, see [`Damage::means`].
    pub means_number: u32,
    /// The health the player lost.
    pub health_lost: i32,
    /// The armor the player lost.
    pub armor_lost: i32,
    /// Whether this killed the player.
    pub fatal: bool,
}

const SHORT_TIME_STEP: u32 = u8::MAX as u32;

/// Every command is coded as its changes to the command of the same client before.
pub(super) fn encode_inputs(
    previous: &mut BTreeMap<u8, Input>,
    inputs: &[Input],
) -> Result<Vec<u8>, DemoError> {
    let count = u16::try_from(inputs.len()).map_err(|_| DemoError::TooManyInputs)?;
    let mut writer = BitWriter::default();
    writer.write(u32::from(count), 16);
    for input in inputs {
        writer.write_byte(input.client);
        let before = previous.insert(input.client, *input).unwrap_or_default();

        let step = input
            .server_time
            .wrapping_sub(before.server_time)
            .cast_unsigned();
        writer.write_flag(step <= SHORT_TIME_STEP);
        if step <= SHORT_TIME_STEP {
            writer.write(step, 8);
        } else {
            writer.write_long(input.server_time);
        }

        for (value, old) in input.angles.iter().zip(before.angles) {
            write_changed(&mut writer, *value, old);
        }
        write_changed(&mut writer, input.buttons, before.buttons);

        let rest = rest_of(input);
        let changed = rest != rest_of(&before);
        writer.write_flag(changed);
        if changed {
            rest.into_iter().for_each(|byte| writer.write_byte(byte));
        }
    }
    Ok(writer.into_message())
}

pub(super) fn decode_inputs(
    previous: &mut BTreeMap<u8, Input>,
    payload: &[u8],
) -> Result<Vec<Input>, DemoError> {
    let mut reader = BitReader::new(payload);
    let count = reader.read(16)?;
    (0..count)
        .map(|_| {
            let client = reader.read_byte()?;
            let mut input = previous.get(&client).copied().unwrap_or_default();
            input.client = client;
            input.server_time = if reader.read_flag()? {
                input.server_time.wrapping_add_unsigned(reader.read(8)?)
            } else {
                reader.read_long()?
            };
            for angle in &mut input.angles {
                read_changed(&mut reader, angle)?;
            }
            read_changed(&mut reader, &mut input.buttons)?;
            if reader.read_flag()? {
                input.weapon = reader.read_byte()?;
                input.weapon_primary = reader.read_byte()?;
                input.fov = reader.read_byte()?;
                input.forward = reader.read_byte()?.cast_signed();
                input.right = reader.read_byte()?.cast_signed();
                input.up = reader.read_byte()?.cast_signed();
            }
            previous.insert(client, input);
            Ok(input)
        })
        .collect()
}

fn rest_of(input: &Input) -> [u8; 6] {
    [
        input.weapon,
        input.weapon_primary,
        input.fov,
        input.forward.cast_unsigned(),
        input.right.cast_unsigned(),
        input.up.cast_unsigned(),
    ]
}

fn write_changed(writer: &mut BitWriter, value: i32, before: i32) {
    writer.write_flag(value != before);
    if value != before {
        writer.write_long(value);
    }
}

fn read_changed(reader: &mut BitReader<'_>, value: &mut i32) -> Result<(), DemoError> {
    if reader.read_flag()? {
        *value = reader.read_long()?;
    }
    Ok(())
}

#[cfg(test)]
mod actions_tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use super::*;

    fn input() -> impl Strategy<Value = Input> {
        (
            0u8..3,
            prop_oneof![0i32..600, any::<i32>()],
            any::<[i32; 3]>(),
            any::<i32>(),
            any::<(u8, u8, u8, i8, i8, i8)>(),
        )
            .prop_map(|(client, server_time, angles, buttons, rest)| Input {
                client,
                server_time,
                angles,
                buttons,
                weapon: rest.0,
                weapon_primary: rest.1,
                fov: rest.2,
                forward: rest.3,
                right: rest.4,
                up: rest.5,
            })
    }

    proptest! {
        #[test]
        #[cfg_attr(miri, ignore)]
        fn inputs_survive_the_round_trip(
            batches in prop::collection::vec(prop::collection::vec(input(), 0..6), 1..4)
        ) {
            let mut written = BTreeMap::new();
            let mut read = BTreeMap::new();
            for batch in batches {
                let payload = encode_inputs(&mut written, &batch).expect("encoded");

                prop_assert_eq!(decode_inputs(&mut read, &payload).expect("decoded"), batch);
            }
        }
    }

    #[test]
    fn a_command_that_only_moves_the_time_costs_a_few_bytes() {
        let first = Input {
            server_time: 1000,
            angles: [123_456; 3],
            ..Input::default()
        };
        let second = Input {
            server_time: 1008,
            ..first
        };
        let mut previous = BTreeMap::new();
        encode_inputs(&mut previous, &[first]).expect("encoded");

        let payload = encode_inputs(&mut previous, &[second]).expect("encoded");

        assert!(payload.len() <= 8, "{} bytes", payload.len());
    }

    #[test]
    fn a_cut_off_record_is_an_error() {
        let mut previous = BTreeMap::new();

        let result = decode_inputs(&mut previous, &[0xff]);

        assert_eq!(result, Err(DemoError::UnexpectedEnd));
    }
}
