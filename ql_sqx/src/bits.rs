use alloc::vec::Vec;

use super::{
    error::DemoError,
    huffman::{self, CODES, MAX_CODE_LENGTH},
};

/// Writes values like the engine does: the bits beyond whole bytes as they are, whole bytes Huffman coded.
#[derive(Debug, Default)]
pub(super) struct BitWriter {
    data: Vec<u8>,
    bit: usize,
}

impl BitWriter {
    /// Writes the lowest `width` bits (1 to 32) of `value`.
    pub(super) fn write(&mut self, value: u32, width: u8) {
        let mut rest = u64::from(value) & ((1u64 << width) - 1);
        for _ in 0..width % 8 {
            self.push_bit(rest & 1 == 1);
            rest >>= 1;
        }
        for _ in 0..width / 8 {
            let (code, length) = CODES[(rest & 0xff) as usize];
            for position in 0..length {
                self.push_bit(code >> position & 1 == 1);
            }
            rest >>= 8;
        }
    }

    /// Writes the lowest `width` bits of `value` as they are, which the engine never does for bytes.
    pub(super) fn write_raw(&mut self, value: u32, width: u8) {
        for position in 0..width {
            self.push_bit(value >> position & 1 == 1);
        }
    }

    pub(super) fn write_flag(&mut self, flag: bool) {
        self.write(u32::from(flag), 1);
    }

    pub(super) fn write_byte(&mut self, value: u8) {
        self.write(u32::from(value), 8);
    }

    pub(super) fn write_short(&mut self, value: i16) {
        self.write(u32::from(value.cast_unsigned()), 16);
    }

    pub(super) fn write_long(&mut self, value: i32) {
        self.write(value.cast_unsigned(), 32);
    }

    /// Returns the message, which like in the engine is always one byte longer than its full bytes.
    pub(super) fn into_message(mut self) -> Vec<u8> {
        self.data.resize(self.bit / 8 + 1, 0);
        self.data
    }

    fn push_bit(&mut self, set: bool) {
        if self.bit % 8 == 0 {
            self.data.push(0);
        }
        if set && let Some(last) = self.data.last_mut() {
            *last |= 1 << (self.bit % 8);
        }
        self.bit += 1;
    }
}

#[derive(Debug)]
pub(super) struct BitReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    /// Reads an unsigned value of `width` bits (1 to 32).
    pub(super) fn read(&mut self, width: u8) -> Result<u32, DemoError> {
        let raw_bits = width % 8;
        let mut value = 0u32;
        for position in 0..raw_bits {
            value |= self.peek(1) << position;
            self.advance(1)?;
        }
        for position in 0..width / 8 {
            let (byte, length) =
                huffman::decode(self.peek(MAX_CODE_LENGTH) as u16).ok_or(DemoError::InvalidCode)?;
            self.advance(length)?;
            value |= u32::from(byte) << (raw_bits + 8 * position);
        }
        Ok(value)
    }

    /// Reads `width` bits (up to 32) written as they are.
    pub(super) fn read_raw(&mut self, width: u8) -> Result<u32, DemoError> {
        (0..width).try_fold(0, |value, position| {
            let bit = self.peek(1);
            self.advance(1)?;
            Ok(value | bit << position)
        })
    }

    pub(super) fn read_flag(&mut self) -> Result<bool, DemoError> {
        Ok(self.read(1)? == 1)
    }

    pub(super) fn read_byte(&mut self) -> Result<u8, DemoError> {
        Ok(self.read(8)? as u8)
    }

    pub(super) fn read_short(&mut self) -> Result<i16, DemoError> {
        Ok((self.read(16)? as u16).cast_signed())
    }

    pub(super) fn read_long(&mut self) -> Result<i32, DemoError> {
        Ok(self.read(32)?.cast_signed())
    }

    fn peek(&self, width: u8) -> u32 {
        (0..usize::from(width)).fold(0, |value, position| {
            let at = self.bit + position;
            let set = self
                .data
                .get(at / 8)
                .is_some_and(|byte| byte >> (at % 8) & 1 == 1);
            value | u32::from(set) << position
        })
    }

    fn advance(&mut self, width: u8) -> Result<(), DemoError> {
        let next = self.bit + usize::from(width);
        if next > self.data.len() * 8 {
            return Err(DemoError::UnexpectedEnd);
        }
        self.bit = next;
        Ok(())
    }
}

#[cfg(test)]
mod bits_tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn reading_beyond_the_data_is_an_error() {
        let mut reader = BitReader::new(&[]);

        assert_eq!(reader.read(1), Err(DemoError::UnexpectedEnd));
    }

    #[test]
    fn a_message_is_one_byte_longer_than_its_full_bytes() {
        let mut writer = BitWriter::default();
        writer.write(0, 7);
        writer.write(1, 1);

        assert_eq!(writer.into_message(), [0x80, 0]);
    }

    proptest! {
        #[test]
        #[cfg_attr(miri, ignore)]
        fn written_values_are_read_back(values in prop::collection::vec((any::<u32>(), 1u8..=32), 0..200)) {
            let mut writer = BitWriter::default();
            for &(value, width) in &values {
                writer.write(value, width);
            }
            let message = writer.into_message();
            let mut reader = BitReader::new(&message);

            for &(value, width) in &values {
                let expected = (u64::from(value) & ((1u64 << width) - 1)) as u32;
                prop_assert_eq!(reader.read(width), Ok(expected));
            }
        }

        #[test]
        #[cfg_attr(miri, ignore)]
        fn raw_values_are_read_back(values in prop::collection::vec((any::<u32>(), 0u8..=32), 0..200)) {
            let mut writer = BitWriter::default();
            for &(value, width) in &values {
                writer.write_raw(value, width);
            }
            let message = writer.into_message();
            let mut reader = BitReader::new(&message);

            for &(value, width) in &values {
                let expected = (u64::from(value) & ((1u64 << width) - 1)) as u32;
                prop_assert_eq!(reader.read_raw(width), Ok(expected));
            }
        }

        #[test]
        #[cfg_attr(miri, ignore)]
        fn arbitrary_data_never_panics(data in prop::collection::vec(any::<u8>(), 0..64)) {
            let mut reader = BitReader::new(&data);
            while reader.read(32).is_ok() {}
        }
    }
}
