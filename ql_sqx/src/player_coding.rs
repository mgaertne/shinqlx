//! How recordings carry the player fields that change in nearly every frame.

use super::{
    bits::{BitReader, BitWriter},
    delta::{self, ValueCoding},
    error::DemoError,
    fields::{FieldKind, player_field},
};

const COMMAND_TIME: usize = player_field("commandTime");
const VIEW_ANGLES: [usize; 3] = [
    player_field("viewangles[0]"),
    player_field("viewangles[1]"),
    player_field("viewangles[2]"),
];
const MOVEMENT: [usize; 6] = [
    player_field("origin[0]"),
    player_field("origin[1]"),
    player_field("origin[2]"),
    player_field("velocity[0]"),
    player_field("velocity[1]"),
    player_field("velocity[2]"),
];
/// The degrees of one step of the 16 bit angles the clients send.
const ANGLE_STEP: f32 = 360.0 / 65536.0;
const HIGHEST_BIT_BITS: u8 = 5;

#[derive(Debug, Clone, Copy)]
pub(super) struct RecordingCoding {
    pub(super) server_time: i32,
}

impl ValueCoding for RecordingCoding {
    fn write(&self, writer: &mut BitWriter, index: usize, kind: FieldKind, from: u32, to: u32) {
        if index == COMMAND_TIME {
            self.write_command_time(writer, to);
        } else if VIEW_ANGLES.contains(&index) {
            write_angle(writer, kind, to);
        } else if MOVEMENT.contains(&index) {
            write_difference(writer, from, to);
        } else {
            delta::write_value(writer, kind, to);
        }
    }

    fn read(
        &self,
        reader: &mut BitReader<'_>,
        index: usize,
        kind: FieldKind,
        from: u32,
    ) -> Result<u32, DemoError> {
        if index == COMMAND_TIME {
            self.read_command_time(reader)
        } else if VIEW_ANGLES.contains(&index) {
            read_angle(reader, kind)
        } else if MOVEMENT.contains(&index) {
            read_difference(reader, from)
        } else {
            delta::read_value(reader, kind)
        }
    }
}

impl RecordingCoding {
    /// A command is usually just a few milliseconds older than the frame.
    fn write_command_time(self, writer: &mut BitWriter, time: u32) {
        let age = self.server_time.wrapping_sub(time.cast_signed());
        let short_age = i8::try_from(age);
        writer.write_flag(short_age.is_ok());
        match short_age {
            Ok(age) => writer.write_byte(age.cast_unsigned()),
            Err(_) => writer.write(time, 32),
        }
    }

    fn read_command_time(self, reader: &mut BitReader<'_>) -> Result<u32, DemoError> {
        if reader.read_flag()? {
            let age = reader.read_byte()?.cast_signed();
            Ok(self
                .server_time
                .wrapping_sub(i32::from(age))
                .cast_unsigned())
        } else {
            reader.read(32)
        }
    }
}

fn angle_of(step: i16) -> u32 {
    (f32::from(step) * ANGLE_STEP).to_bits()
}

fn write_angle(writer: &mut BitWriter, kind: FieldKind, bits: u32) {
    let step = (f32::from_bits(bits) / ANGLE_STEP) as i16;
    let is_step = angle_of(step) == bits;
    writer.write_flag(is_step);
    if is_step {
        writer.write_raw(u32::from(step.cast_unsigned()), 16);
    } else {
        delta::write_value(writer, kind, bits);
    }
}

fn read_angle(reader: &mut BitReader<'_>, kind: FieldKind) -> Result<u32, DemoError> {
    if reader.read_flag()? {
        Ok(angle_of((reader.read_raw(16)? as u16).cast_signed()))
    } else {
        delta::read_value(reader, kind)
    }
}

/// Close values of a float share their upper bits, so only the bits below the highest changed one
/// are written. `from` and `to` have to differ.
fn write_difference(writer: &mut BitWriter, from: u32, to: u32) {
    let difference = from ^ to;
    let highest = difference.checked_ilog2().unwrap_or_default() as u8;
    writer.write_raw(u32::from(highest), HIGHEST_BIT_BITS);
    writer.write_raw(difference, highest);
}

fn read_difference(reader: &mut BitReader<'_>, from: u32) -> Result<u32, DemoError> {
    let highest = reader.read_raw(HIGHEST_BIT_BITS)? as u8;
    Ok(from ^ (1 << highest | reader.read_raw(highest)?))
}

#[cfg(test)]
mod player_coding_tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    const CODING: RecordingCoding = RecordingCoding {
        server_time: 100_000,
    };

    fn round_trip(index: usize, kind: FieldKind, from: u32, to: u32) -> (u32, usize) {
        let mut writer = BitWriter::default();
        CODING.write(&mut writer, index, kind, from, to);
        let message = writer.into_message();
        let read = CODING
            .read(&mut BitReader::new(&message), index, kind, from)
            .expect("complete");
        (read, message.len())
    }

    #[rstest]
    #[case::of_this_frame(100_000)]
    #[case::oldest_short_one(99_873)]
    #[case::newest_short_one(100_128)]
    #[case::too_old(99_872)]
    #[case::too_new(100_129)]
    #[case::unrelated(-5)]
    fn command_times_are_read_back(#[case] time: i32) {
        let time = time.cast_unsigned();

        let (read, _) = round_trip(COMMAND_TIME, FieldKind::Unsigned(32), 0, time);

        assert_eq!(read, time);
    }

    #[rstest]
    #[case::step(-8192.0 * ANGLE_STEP, 3)]
    #[case::between_steps(0.001, 8)]
    #[case::beyond_the_steps(270.0, 3)]
    fn view_angles_are_read_back(#[case] angle: f32, #[case] longest: usize) {
        let (read, length) = round_trip(VIEW_ANGLES[1], FieldKind::Float, 0, angle.to_bits());

        assert_eq!(read, angle.to_bits());
        assert!(length <= longest, "{length}");
    }

    proptest! {
        #[test]
        #[cfg_attr(miri, ignore)]
        fn any_value_of_a_changed_field_is_read_back(from: u32, to: u32, time: u32) {
            prop_assume!(from != to);
            for (index, kind) in [
                (COMMAND_TIME, FieldKind::Unsigned(32)),
                (VIEW_ANGLES[0], FieldKind::Float),
                (MOVEMENT[0], FieldKind::Float),
            ] {
                let to = if index == COMMAND_TIME { time } else { to };
                // The engine's coding of floats knows no negative zero.
                prop_assume!(index != VIEW_ANGLES[0] || to != (-0.0f32).to_bits());

                prop_assert_eq!(round_trip(index, kind, from, to).0, to);
            }
        }
    }
}
