use super::{
    bits::{BitReader, BitWriter},
    error::DemoError,
    fields::{Field, FieldKind},
    state::{ARRAY_LENGTH, ENTITY_NUMBER_BITS, EntityState, PlayerState, float_bits},
};

/// Whole floats within this many bits are sent as integers.
const SMALL_FLOAT_BITS: u8 = 13;
const SMALL_FLOAT_BIAS: i32 = 1 << (SMALL_FLOAT_BITS - 1);

pub(super) fn write_entity_removal(writer: &mut BitWriter, number: u16) {
    writer.write(u32::from(number), ENTITY_NUMBER_BITS);
    writer.write_flag(true);
}

/// Writes the changes from `from` to `to`; without changes only a forced entity is written at all.
pub(super) fn write_entity(
    writer: &mut BitWriter,
    table: &[Field],
    from: &EntityState,
    to: &EntityState,
    force: bool,
) {
    let changed = changed_field_count(&from.fields, &to.fields, table.len());
    if changed == 0 && !force {
        return;
    }
    writer.write(u32::from(to.number), ENTITY_NUMBER_BITS);
    writer.write_flag(false);
    writer.write_flag(changed > 0);
    if changed == 0 {
        return;
    }
    writer.write_byte(changed as u8);
    for (index, field) in table.iter().enumerate().take(changed) {
        let value = to.fields[index];
        let differs = from.fields[index] != value;
        writer.write_flag(differs);
        if !differs {
            continue;
        }
        let is_zero = match field.kind {
            FieldKind::Float => f32::from_bits(value) == 0.0,
            FieldKind::Unsigned(_) | FieldKind::Signed(_) => value == 0,
        };
        writer.write_flag(!is_zero);
        if !is_zero {
            write_value(writer, field.kind, value);
        }
    }
}

/// Reads what follows the number of an entity, returning `None` for a removed entity.
pub(super) fn read_entity(
    reader: &mut BitReader<'_>,
    table: &[Field],
    from: &EntityState,
    number: u16,
) -> Result<Option<EntityState>, DemoError> {
    if reader.read_flag()? {
        return Ok(None);
    }
    let mut entity = EntityState { number, ..*from };
    if !reader.read_flag()? {
        return Ok(Some(entity));
    }
    let changed = read_changed_field_count(reader, table.len())?;
    for (index, field) in table.iter().enumerate().take(changed) {
        if !reader.read_flag()? {
            continue;
        }
        entity.fields[index] = if reader.read_flag()? {
            read_value(reader, field.kind)?
        } else {
            0
        };
    }
    Ok(Some(entity))
}

/// How the value of a changed player field is carried.
pub(super) trait ValueCoding {
    fn write(&self, writer: &mut BitWriter, index: usize, kind: FieldKind, from: u32, to: u32);

    fn read(
        &self,
        reader: &mut BitReader<'_>,
        index: usize,
        kind: FieldKind,
        from: u32,
    ) -> Result<u32, DemoError>;
}

/// The coding of the engine, as demos carry it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Netfields;

impl ValueCoding for Netfields {
    fn write(&self, writer: &mut BitWriter, _index: usize, kind: FieldKind, _from: u32, to: u32) {
        write_value(writer, kind, to);
    }

    fn read(
        &self,
        reader: &mut BitReader<'_>,
        _index: usize,
        kind: FieldKind,
        _from: u32,
    ) -> Result<u32, DemoError> {
        read_value(reader, kind)
    }
}

/// What a reader gets back for a value written as a netfield.
pub(super) fn carried(kind: FieldKind, value: u32) -> u32 {
    match kind {
        FieldKind::Float => float_bits(f32::from_bits(value)),
        FieldKind::Unsigned(width) => value & (u32::MAX >> (32 - u32::from(width))),
        FieldKind::Signed(width) => sign_extend(value, width),
    }
}

pub(super) fn write_player(
    writer: &mut BitWriter,
    table: &[Field],
    coding: &impl ValueCoding,
    from: &PlayerState,
    to: &PlayerState,
) {
    let changed = changed_field_count(&from.fields, &to.fields, table.len());
    writer.write_byte(changed as u8);
    for (index, field) in table.iter().enumerate().take(changed) {
        let differs = from.fields[index] != to.fields[index];
        writer.write_flag(differs);
        if differs {
            coding.write(
                writer,
                index,
                field.kind,
                from.fields[index],
                to.fields[index],
            );
        }
    }

    let short_arrays = [
        (&from.stats, &to.stats),
        (&from.persistant, &to.persistant),
        (&from.ammo, &to.ammo),
    ];
    let short_masks = short_arrays.map(|(from, to)| change_mask(from, to));
    let powerup_mask = change_mask(&from.powerups, &to.powerups);
    let any_array_changed = short_masks.iter().any(|&mask| mask != 0) || powerup_mask != 0;
    writer.write_flag(any_array_changed);
    if !any_array_changed {
        return;
    }
    for ((_, values), mask) in short_arrays.into_iter().zip(short_masks) {
        write_array(writer, values, mask, |writer, value| {
            writer.write_short(value);
        });
    }
    write_array(writer, &to.powerups, powerup_mask, |writer, value| {
        writer.write_long(value);
    });
}

pub(super) fn read_player(
    reader: &mut BitReader<'_>,
    table: &[Field],
    coding: &impl ValueCoding,
    from: &PlayerState,
) -> Result<PlayerState, DemoError> {
    let mut player = *from;
    let changed = read_changed_field_count(reader, table.len())?;
    for (index, field) in table.iter().enumerate().take(changed) {
        if reader.read_flag()? {
            player.fields[index] = coding.read(reader, index, field.kind, player.fields[index])?;
        }
    }
    if reader.read_flag()? {
        read_array(reader, &mut player.stats, BitReader::read_short)?;
        read_array(reader, &mut player.persistant, BitReader::read_short)?;
        read_array(reader, &mut player.ammo, BitReader::read_short)?;
        read_array(reader, &mut player.powerups, BitReader::read_long)?;
    }
    Ok(player)
}

/// The number of fields up to and including the last changed one.
fn changed_field_count(from: &[u32], to: &[u32], count: usize) -> usize {
    (0..count)
        .rev()
        .find(|&index| from[index] != to[index])
        .map_or(0, |index| index + 1)
}

fn read_changed_field_count(reader: &mut BitReader<'_>, count: usize) -> Result<usize, DemoError> {
    let changed = reader.read_byte()?;
    if usize::from(changed) > count {
        return Err(DemoError::FieldCount(changed));
    }
    Ok(usize::from(changed))
}

fn sign_extend(value: u32, width: u8) -> u32 {
    let unused = 32 - u32::from(width);
    ((value << unused).cast_signed() >> unused).cast_unsigned()
}

pub(super) fn write_value(writer: &mut BitWriter, kind: FieldKind, value: u32) {
    match kind {
        FieldKind::Float => write_float(writer, value),
        FieldKind::Unsigned(width) | FieldKind::Signed(width) => writer.write(value, width),
    }
}

fn write_float(writer: &mut BitWriter, bits: u32) {
    let value = f32::from_bits(bits);
    let whole = value as i32;
    let is_small = whole as f32 == value && (-SMALL_FLOAT_BIAS..SMALL_FLOAT_BIAS).contains(&whole);
    writer.write_flag(!is_small);
    if is_small {
        writer.write((whole + SMALL_FLOAT_BIAS).cast_unsigned(), SMALL_FLOAT_BITS);
    } else {
        writer.write(bits, 32);
    }
}

pub(super) fn read_value(reader: &mut BitReader<'_>, kind: FieldKind) -> Result<u32, DemoError> {
    match kind {
        FieldKind::Float => {
            if reader.read_flag()? {
                reader.read(32)
            } else {
                let whole = reader.read(SMALL_FLOAT_BITS)?.cast_signed() - SMALL_FLOAT_BIAS;
                Ok((whole as f32).to_bits())
            }
        }
        FieldKind::Unsigned(width) => reader.read(width),
        FieldKind::Signed(width) => Ok(sign_extend(reader.read(width)?, width)),
    }
}

fn change_mask<T: PartialEq>(from: &[T; ARRAY_LENGTH], to: &[T; ARRAY_LENGTH]) -> u16 {
    from.iter()
        .zip(to)
        .enumerate()
        .filter(|(_, (from, to))| from != to)
        .fold(0, |mask, (index, _)| mask | 1 << index)
}

fn write_array<T: Copy>(
    writer: &mut BitWriter,
    values: &[T; ARRAY_LENGTH],
    mask: u16,
    write: impl Fn(&mut BitWriter, T),
) {
    writer.write_flag(mask != 0);
    if mask == 0 {
        return;
    }
    writer.write(u32::from(mask), 16);
    values
        .iter()
        .enumerate()
        .filter(|(index, _)| mask & (1 << index) != 0)
        .for_each(|(_, &value)| write(writer, value));
}

fn read_array<'a, T>(
    reader: &mut BitReader<'a>,
    values: &mut [T; ARRAY_LENGTH],
    read: impl Fn(&mut BitReader<'a>) -> Result<T, DemoError>,
) -> Result<(), DemoError> {
    if !reader.read_flag()? {
        return Ok(());
    }
    let mask = reader.read(16)?;
    for (index, value) in values.iter_mut().enumerate() {
        if mask & (1 << index) != 0 {
            *value = read(reader)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod delta_tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use super::*;
    use crate::fields::{ENTITY_FIELD_COUNT, ENTITY_FIELDS, PLAYER_FIELD_COUNT, PLAYER_FIELDS};

    /// Maps arbitrary bits to a value the field can carry over the network without loss.
    fn representable(kind: FieldKind, raw: u32) -> u32 {
        match kind {
            FieldKind::Float => match raw % 4 {
                0 => 0,
                1 => (((raw >> 2) % 8192) as f32 - 4096.0).to_bits(),
                2 => ((raw >> 2) as f32 / 8.0 + 0.125).to_bits(),
                _ => (-(((raw >> 2) % 100_000) as f32) - 0.5).to_bits(),
            },
            FieldKind::Unsigned(width) => (u64::from(raw) & ((1u64 << width) - 1)) as u32,
            FieldKind::Signed(width) => {
                let unused = 32 - u32::from(width);
                ((raw << unused).cast_signed() >> unused).cast_unsigned()
            }
        }
    }

    fn sparse() -> impl Strategy<Value = u32> {
        prop_oneof![3 => Just(0u32), 1 => any::<u32>()]
    }

    fn entity() -> impl Strategy<Value = EntityState> {
        prop::collection::vec(sparse(), ENTITY_FIELD_COUNT).prop_map(|raw| EntityState {
            number: 17,
            fields: core::array::from_fn(|index| {
                representable(ENTITY_FIELDS[index].kind, raw[index])
            }),
        })
    }

    fn player() -> impl Strategy<Value = PlayerState> {
        prop::collection::vec(sparse(), PLAYER_FIELD_COUNT + 4 * ARRAY_LENGTH).prop_map(|raw| {
            let (fields, arrays) = raw.split_at(PLAYER_FIELD_COUNT);
            let short = |array: usize, index: usize| arrays[array * ARRAY_LENGTH + index] as i16;
            PlayerState {
                fields: core::array::from_fn(|index| {
                    representable(PLAYER_FIELDS[index].kind, fields[index])
                }),
                stats: core::array::from_fn(|index| short(0, index)),
                persistant: core::array::from_fn(|index| short(1, index)),
                ammo: core::array::from_fn(|index| short(2, index)),
                powerups: core::array::from_fn(|index| {
                    arrays[3 * ARRAY_LENGTH + index].cast_signed()
                }),
            }
        })
    }

    #[test]
    fn an_unchanged_entity_is_only_written_when_forced() {
        let entity = EntityState::default();
        let mut writer = BitWriter::default();

        write_entity(&mut writer, &ENTITY_FIELDS, &entity, &entity, false);

        assert_eq!(writer.into_message(), [0]);
    }

    #[test]
    fn too_many_changed_fields_are_an_error() {
        let mut writer = BitWriter::default();
        writer.write_byte(59);
        let message = writer.into_message();

        let result = read_player(
            &mut BitReader::new(&message),
            &PLAYER_FIELDS,
            &Netfields,
            &PlayerState::default(),
        );

        assert_eq!(result, Err(DemoError::FieldCount(59)));
    }

    proptest! {
        #[test]
        #[cfg_attr(miri, ignore)]
        fn entities_survive_the_round_trip(from in entity(), to in entity()) {
            let mut writer = BitWriter::default();
            write_entity(&mut writer, &ENTITY_FIELDS, &from, &to, true);
            let message = writer.into_message();
            let mut reader = BitReader::new(&message);

            let number = reader.read(ENTITY_NUMBER_BITS);
            let read = read_entity(&mut reader, &ENTITY_FIELDS, &from, 17);

            prop_assert_eq!(number, Ok(17));
            prop_assert_eq!(read, Ok(Some(to)));
        }

        #[test]
        #[cfg_attr(miri, ignore)]
        fn players_survive_the_round_trip(from in player(), to in player()) {
            let mut writer = BitWriter::default();
            write_player(&mut writer, &PLAYER_FIELDS, &Netfields, &from, &to);
            let message = writer.into_message();

            let read = read_player(&mut BitReader::new(&message), &PLAYER_FIELDS, &Netfields, &from);

            prop_assert_eq!(read, Ok(to));
        }
    }
}
