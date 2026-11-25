//! Reads the `worldspawn` entity out of a BSP file (Quake 3 style `IBSP`, versions 46 and 47).

use derive_more::Display;

use super::{block::parse_first_block, model::WorldSpawn};

const MAGIC: &[u8; 4] = b"IBSP";
const LUMP_TABLE_START: usize = 8;
const ENTITY_LUMP: usize = 0;
/// The largest entity lump that is read; real maps stay below 120 KB.
pub(crate) const ENTITY_LUMP_LIMIT: usize = 4 << 20;

/// Why a BSP could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub(crate) enum BspError {
    #[display("not a BSP file")]
    NotABsp,
    #[display("unsupported BSP version {_0}")]
    UnsupportedVersion(i32),
    #[display("entity lump outside the file")]
    LumpOutOfRange,
    #[display("entity lump larger than 4 MiB")]
    LumpTooLarge,
    #[display("no worldspawn entity")]
    NoWorldspawn,
}

fn read_i32(data: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The entity lump of a BSP as bytes.
fn entity_lump(bsp: &[u8]) -> Result<&[u8], BspError> {
    if bsp.get(..4) != Some(MAGIC.as_slice()) {
        return Err(BspError::NotABsp);
    }
    let version = read_i32(bsp, 4).ok_or(BspError::NotABsp)?;
    if !matches!(version, 46 | 47) {
        return Err(BspError::UnsupportedVersion(version));
    }
    let entry = LUMP_TABLE_START + ENTITY_LUMP * 8;
    let (offset, length) = read_i32(bsp, entry)
        .zip(read_i32(bsp, entry + 4))
        .and_then(|(offset, length)| {
            Some((usize::try_from(offset).ok()?, usize::try_from(length).ok()?))
        })
        .ok_or(BspError::LumpOutOfRange)?;
    if length > ENTITY_LUMP_LIMIT {
        return Err(BspError::LumpTooLarge);
    }
    offset
        .checked_add(length)
        .and_then(|end| bsp.get(offset..end))
        .ok_or(BspError::LumpOutOfRange)
}

/// The `message`, `author` and `author2` of the first entity, which must be the `worldspawn`.
pub(crate) fn parse_worldspawn(bsp: &[u8]) -> Result<WorldSpawn, BspError> {
    let lump = entity_lump(bsp)?;
    let text = String::from_utf8_lossy(lump);
    let block = parse_first_block(text.trim_end_matches('\0'))
        .filter(|block| {
            block
                .get("classname")
                .is_some_and(|class| class.eq_ignore_ascii_case("worldspawn"))
        })
        .ok_or(BspError::NoWorldspawn)?;
    let value = |key: &str| {
        block
            .get(key)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    Ok(WorldSpawn {
        message: value("message"),
        author: value("author"),
        author2: value("author2"),
    })
}

/// A BSP file with the given entity text, for tests.
#[cfg(test)]
pub(crate) fn mini_bsp(entities: &str) -> Vec<u8> {
    let header_len = LUMP_TABLE_START + 17 * 8;
    let mut bsp = Vec::from(*MAGIC);
    bsp.extend(47_i32.to_le_bytes());
    bsp.extend(i32::try_from(header_len).unwrap().to_le_bytes());
    bsp.extend(i32::try_from(entities.len()).unwrap().to_le_bytes());
    bsp.resize(header_len, 0);
    bsp.extend(entities.as_bytes());
    bsp
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const WORLD: &str = "{\n\"classname\" \"worldspawn\"\n\"message\" \"Simplicity CA\"\n\"author\" \"Geit\"\n\"author2\" \"\"\n\"music\" \"x\"\n}\n{\n\"classname\" \"info_player_deathmatch\"\n\"message\" \"other\"\n}\n\0";

    #[test]
    fn reads_the_worldspawn_values_and_ignores_later_entities() {
        let worldspawn = parse_worldspawn(&mini_bsp(WORLD)).unwrap();

        assert_eq!(
            worldspawn,
            WorldSpawn {
                message: Some("Simplicity CA".into()),
                author: Some("Geit".into()),
                author2: None,
            }
        );
    }

    #[test]
    fn colour_codes_stay_and_control_characters_go() {
        let bsp = mini_bsp("{ \"classname\" \"worldspawn\" \"message\" \"^1Red\u{7} ^7Map\" }");

        assert_eq!(
            parse_worldspawn(&bsp).unwrap().message.as_deref(),
            Some("^1Red ^7Map")
        );
    }

    #[test]
    fn rejects_files_that_are_not_a_supported_bsp() {
        let mut other_version = mini_bsp(WORLD);
        other_version[4] = 38;

        assert_eq!(parse_worldspawn(b""), Err(BspError::NotABsp));
        assert_eq!(parse_worldspawn(b"RIFF....."), Err(BspError::NotABsp));
        assert_eq!(parse_worldspawn(b"IBSP"), Err(BspError::NotABsp));
        assert_eq!(
            parse_worldspawn(&other_version),
            Err(BspError::UnsupportedVersion(38))
        );
    }

    #[test]
    fn rejects_a_lump_outside_the_file_or_too_large() {
        let mut outside = mini_bsp(WORLD);
        outside.truncate(outside.len() - 5);
        let mut negative = mini_bsp(WORLD);
        negative[8..12].copy_from_slice(&(-1_i32).to_le_bytes());
        let mut huge = mini_bsp(WORLD);
        huge[12..16].copy_from_slice(&i32::MAX.to_le_bytes());
        let mut overflow = mini_bsp(WORLD);
        overflow[8..12].copy_from_slice(&i32::MAX.to_le_bytes());

        assert_eq!(parse_worldspawn(&outside), Err(BspError::LumpOutOfRange));
        assert_eq!(parse_worldspawn(&negative), Err(BspError::LumpOutOfRange));
        assert_eq!(parse_worldspawn(&huge), Err(BspError::LumpTooLarge));
        assert_eq!(parse_worldspawn(&overflow), Err(BspError::LumpOutOfRange));
    }

    #[test]
    fn first_entity_must_be_the_worldspawn() {
        assert_eq!(
            parse_worldspawn(&mini_bsp("{ \"classname\" \"item_armor\" }")),
            Err(BspError::NoWorldspawn)
        );
        assert_eq!(parse_worldspawn(&mini_bsp("")), Err(BspError::NoWorldspawn));
        assert_eq!(
            parse_worldspawn(&mini_bsp("{ \"classname\" \"worldspawn\"")),
            Err(BspError::NoWorldspawn)
        );
    }
}
