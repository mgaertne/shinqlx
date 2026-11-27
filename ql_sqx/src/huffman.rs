/// The static code of the engine as `(code, length)` per byte, the first bit in the lowest position.
pub(super) const CODES: [(u16, u8); 256] = include!("huffman_codes.in");

pub(super) const MAX_CODE_LENGTH: u8 = 11;
const LOOKUP_SIZE: usize = 1 << MAX_CODE_LENGTH;

static LOOKUP: [(u8, u8); LOOKUP_SIZE] = lookup_table();

const fn lookup_table() -> [(u8, u8); LOOKUP_SIZE] {
    let mut table = [(0, 0); LOOKUP_SIZE];
    let mut symbol = 0;
    while symbol < CODES.len() {
        let (code, length) = CODES[symbol];
        let mut index = code as usize;
        while index < LOOKUP_SIZE {
            table[index] = (symbol as u8, length);
            index += 1 << length;
        }
        symbol += 1;
    }
    table
}

/// Decodes the byte whose code starts in the lowest bit of `bits`, returning it with its code length.
pub(super) fn decode(bits: u16) -> Option<(u8, u8)> {
    let (symbol, length) = LOOKUP[usize::from(bits) % LOOKUP_SIZE];
    (length > 0).then_some((symbol, length))
}

#[cfg(test)]
mod huffman_tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn every_byte_decodes_to_itself() {
        for (symbol, &(code, length)) in CODES.iter().enumerate() {
            assert_eq!(decode(code), Some((symbol as u8, length)));
        }
    }

    #[test]
    fn only_the_unused_code_of_the_engine_is_left() {
        let unused = LOOKUP.iter().filter(|&&(_, length)| length == 0).count();

        assert_eq!(unused, 1);
    }
}
