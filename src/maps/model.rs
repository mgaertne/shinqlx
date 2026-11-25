//! The data the scan produces; plain values without any Python or engine type.

use alloc::collections::BTreeMap;

use super::{block::Block, factory::Factory, gametype::parse_type, name::map_key};
use crate::gametypes::GameTypes;

/// The Steam app id of Quake Live, which names the workshop directory.
const WORKSHOP_APP_ID: &str = "282440";

/// Where a file comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceKind {
    /// The game's own directory (`pak00.pk3`, other archives and loose files next to them).
    Base,
    /// The home directory of the server (loose scripts only).
    Home,
    /// A workshop item.
    Workshop,
}

/// The archive or directory a file was found in; the full path is not part of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MapSource {
    pub(crate) kind: SourceKind,
    /// The last path component, e.g. `map-charon3dm12.pk3` or `baseq3`.
    pub(crate) file_name: String,
    pub(crate) workshop_id: Option<u64>,
    /// Whether `maps/<name>.aas` comes from this source; only filled in for the sources of a map.
    pub(crate) has_aas: bool,
    /// Whether the levelshot picture of the map comes from this source.
    pub(crate) has_levelshot: bool,
    /// Whether the picture for the map-end vote comes from this source.
    pub(crate) has_preview: bool,
}

impl MapSource {
    /// Whether both name the same archive or directory, whatever they hold.
    pub(crate) fn is_same_place(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.file_name == other.file_name
            && self.workshop_id == other.workshop_id
    }

    /// Derives the source from the path the engine reports for a file; `homepath` is the server's `fs_homepath`.
    pub(crate) fn from_found_in(path: &str, homepath: &str) -> Self {
        let path = path.trim_end_matches('/');
        let file_name = path.rsplit('/').next().unwrap_or(path).to_owned();
        let workshop_id = workshop_id(path);
        let kind = if workshop_id.is_some() {
            SourceKind::Workshop
        } else if !homepath.is_empty() && path.starts_with(homepath.trim_end_matches('/')) {
            SourceKind::Home
        } else {
            SourceKind::Base
        };
        Self {
            kind,
            file_name,
            workshop_id,
            has_aas: false,
            has_levelshot: false,
            has_preview: false,
        }
    }
}

fn workshop_id(path: &str) -> Option<u64> {
    let mut parts = path.split('/').skip_while(|part| *part != WORKSHOP_APP_ID);
    parts.next()?;
    parts.next()?.parse().ok()
}

/// One arena block of a map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArenaInfo {
    pub(crate) map: String,
    pub(crate) longname: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) author2: Option<String>,
    /// The game types the engine understands in the `type` value, without repeats.
    pub(crate) gametypes: Vec<GameTypes>,
    /// All tokens of the `type` value as written, including the ones the engine ignores.
    pub(crate) type_tokens: Vec<String>,
    pub(crate) fraglimit: Option<i32>,
    pub(crate) timelimit: Option<i32>,
    pub(crate) capturelimit: Option<i32>,
    /// Every key of the block, as written, the last of equal keys winning.
    pub(crate) raw: BTreeMap<String, String>,
    pub(crate) source: Option<MapSource>,
}

impl ArenaInfo {
    /// The arena of a block; `None` when the block names no map.
    pub(crate) fn from_block(block: &Block, source: Option<MapSource>) -> Option<Self> {
        let map = block.get("map").filter(|map| !map.is_empty())?.to_owned();
        let text = |key: &str| block.get(key).map(str::to_owned);
        let (gametypes, type_tokens) = parse_type(block);
        let number = |key: &str| block.get(key).and_then(|value| value.trim().parse().ok());
        Some(Self {
            map,
            longname: text("longname").filter(|name| !name.is_empty()),
            author: text("author").filter(|name| !name.is_empty()),
            author2: text("author2").filter(|name| !name.is_empty()),
            gametypes,
            type_tokens,
            fraglimit: number("fraglimit"),
            timelimit: number("timelimit"),
            capturelimit: number("capturelimit"),
            raw: block
                .entries
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            source,
        })
    }
}

/// The values of the `worldspawn` entity of a BSP that matter here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WorldSpawn {
    pub(crate) message: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) author2: Option<String>,
}

/// A factory together with the file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FactoryInfo {
    pub(crate) factory: Factory,
    pub(crate) source: Option<MapSource>,
}

/// Everything known about one map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MapInfo {
    /// The name as the archive spells it.
    pub(crate) name: String,
    pub(crate) longname: String,
    pub(crate) author: Option<String>,
    pub(crate) author2: Option<String>,
    /// All arena blocks for the map, the most important first.
    pub(crate) arenas: Vec<ArenaInfo>,
    /// The archives that contain the map's BSP, the most important first.
    pub(crate) sources: Vec<MapSource>,
    /// Whether a levelshot picture exists for the map.
    pub(crate) has_levelshot: bool,
    /// Whether a picture for the map-end vote exists for the map.
    pub(crate) has_preview: bool,
    /// Whether `maps/<name>.aas` exists; without it the map has no bot support.
    pub(crate) has_aas: bool,
}

impl MapInfo {
    /// Combines the findings for one map, `arenas` most important first. The BSP's `worldspawn` is more binding than
    /// an arena block. A text the BSP lacks comes from the first arena block that has it, the game types from the
    /// first block alone, and a map without an arena block runs in every game type.
    pub(crate) fn assemble(
        name: &str,
        worldspawn: Option<WorldSpawn>,
        arenas: Vec<ArenaInfo>,
        sources: Vec<MapSource>,
    ) -> Self {
        let worldspawn = worldspawn.unwrap_or_default();
        let pick = |from_bsp: Option<String>, from_arena: fn(&ArenaInfo) -> &Option<String>| {
            from_bsp
                .filter(|value| !value.is_empty())
                .or_else(|| arenas.iter().find_map(|arena| from_arena(arena).clone()))
        };
        Self {
            name: name.to_owned(),
            longname: pick(worldspawn.message, |arena| &arena.longname)
                .unwrap_or_else(|| name.to_owned()),
            author: pick(worldspawn.author, |arena| &arena.author),
            author2: pick(worldspawn.author2, |arena| &arena.author2),
            arenas,
            sources,
            has_levelshot: false,
            has_preview: false,
            has_aas: false,
        }
    }

    /// The game types the map runs in: those of the first arena block, every game type without a block.
    pub(crate) fn gametypes(&self) -> &[GameTypes] {
        self.arenas
            .first()
            .map_or(&GameTypes::ALL[..], |arena| &arena.gametypes)
    }

    /// The `type` tokens of the first arena block as written; none without a block.
    pub(crate) fn type_tokens(&self) -> &[String] {
        self.arenas
            .first()
            .map_or(&[][..], |arena| &arena.type_tokens)
    }

    /// The key under which the map is looked up.
    pub(crate) fn key(&self) -> String {
        map_key(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;
    use crate::maps::block::parse_blocks;

    fn arena(text: &str) -> ArenaInfo {
        ArenaInfo::from_block(&parse_blocks(text).blocks[0], None).unwrap()
    }

    #[rstest]
    #[case(
        "/srv/qlds/baseq3/pak00.pk3",
        "/home/ql/.quakelive/27960",
        SourceKind::Base,
        "pak00.pk3",
        None
    )]
    #[case(
        "/srv/qlds/baseq3",
        "/home/ql/.quakelive/27960",
        SourceKind::Base,
        "baseq3",
        None
    )]
    #[case(
        "/home/ql/.quakelive/27960/baseq3",
        "/home/ql/.quakelive/27960",
        SourceKind::Home,
        "baseq3",
        None
    )]
    #[case(
        "/srv/qlds/steamapps/workshop/content/282440/957375836/map-charon3dm12.pk3",
        "/home/ql/.quakelive/27960",
        SourceKind::Workshop,
        "map-charon3dm12.pk3",
        Some(957_375_836)
    )]
    #[case(
        "/srv/qlds/steamapps/workshop/content/282440/945192253",
        "",
        SourceKind::Workshop,
        "945192253",
        Some(945_192_253)
    )]
    fn source_follows_the_reported_path(
        #[case] path: &str,
        #[case] homepath: &str,
        #[case] kind: SourceKind,
        #[case] file_name: &str,
        #[case] workshop_id: Option<u64>,
    ) {
        assert_eq!(
            MapSource::from_found_in(path, homepath),
            MapSource {
                kind,
                file_name: file_name.to_owned(),
                workshop_id,
                has_aas: false,
                has_levelshot: false,
                has_preview: false,
            }
        );
    }

    #[test]
    fn arena_reads_known_keys_and_keeps_all_in_raw() {
        let arena = arena(
            r#"{ map a longname "A" author "X" type "ca" fraglimit 30 timelimit "x" bots "slash" }"#,
        );

        assert_eq!(arena.map, "a");
        assert_eq!(arena.longname.as_deref(), Some("A"));
        assert_eq!(arena.fraglimit, Some(30));
        assert_eq!(arena.timelimit, None);
        assert_eq!(arena.raw.get("timelimit").map(String::as_str), Some("x"));
        assert_eq!(arena.raw.get("bots").map(String::as_str), Some("slash"));
        assert_eq!(arena.gametypes, vec![GameTypes::ClanArena]);
    }

    #[test]
    fn block_without_a_map_gives_no_arena() {
        let block = &parse_blocks(r#"{ longname "x" } { map "" }"#).blocks;

        assert!(
            block
                .iter()
                .all(|block| ArenaInfo::from_block(block, None).is_none())
        );
    }

    #[test]
    fn worldspawn_beats_the_arena_and_missing_values_fall_back() {
        let arenas = vec![arena(
            r#"{ map a longname "Arena Name" author "Arena Author" type "ca" }"#,
        )];
        let worldspawn = WorldSpawn {
            message: Some("BSP Name".into()),
            author: None,
            author2: Some("Second".into()),
        };

        let info = MapInfo::assemble("a", Some(worldspawn), arenas, Vec::new());

        assert_eq!(info.longname, "BSP Name");
        assert_eq!(info.author.as_deref(), Some("Arena Author"));
        assert_eq!(info.author2.as_deref(), Some("Second"));
        assert_eq!(info.gametypes(), vec![GameTypes::ClanArena]);
    }

    #[test]
    fn map_without_any_information_uses_its_name_and_runs_everywhere() {
        let info = MapInfo::assemble("q3dmp15", None, Vec::new(), Vec::new());

        assert_eq!(info.longname, "q3dmp15");
        assert_eq!(info.author, None);
        assert!(info.arenas.is_empty());
        assert!(info.type_tokens().is_empty());
        assert_eq!(info.gametypes().len(), GameTypes::ALL.len());
    }

    #[test]
    fn first_block_decides_game_types_and_later_blocks_fill_missing_texts() {
        let arenas = vec![
            arena(r#"{ map a type "ffa" }"#),
            arena(r#"{ map a type "ca" longname "Other" }"#),
        ];

        let info = MapInfo::assemble("A", None, arenas, Vec::new());

        assert_eq!(info.gametypes(), vec![GameTypes::FreeForAll]);
        assert_eq!(info.longname, "Other");
        assert_eq!(info.arenas.len(), 2);
        assert_eq!(info.key(), "a");
    }
}
