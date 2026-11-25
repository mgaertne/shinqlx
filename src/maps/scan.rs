//! Collects the maps, arena blocks and factories of a virtual file system into one snapshot.

use alloc::collections::BTreeMap;
use core::cmp::Reverse;

use super::{
    block::parse_blocks,
    bsp::parse_worldspawn,
    factory::parse_factories,
    filesystem::{FsError, MapFileSystem},
    model::{ArenaInfo, FactoryInfo, MapInfo, MapSource, SourceKind, WorldSpawn},
    name::{map_key, map_name_from_bsp_path},
    strip_control,
};

const SCRIPT_LIMIT: u64 = 1 << 20;
const BSP_LIMIT: u64 = 128 << 20;
const PICTURE_EXTENSIONS: [&str; 4] = [".jpg", ".jpeg", ".tga", ".png"];

/// What a scan found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// The maps by their lookup key (the lowercase name).
    pub(crate) maps: BTreeMap<String, MapInfo>,
    /// The factories, the most important source first.
    pub(crate) factories: Vec<FactoryInfo>,
    /// Files that could not be used, one line each, free of control characters.
    pub(crate) problems: Vec<String>,
}

/// Scans `fs`. A map exists when `maps/<name>.bsp` exists; arena blocks without a BSP are dropped.
pub(crate) fn scan(fs: &dyn MapFileSystem, homepath: &str) -> Snapshot {
    scan_with(fs, homepath, |paths| {
        paths.iter().map(|path| read_worldspawn(fs, path)).collect()
    })
}

/// Like [`scan`], but reads the BSPs on the threads of `pool`. Every worker reads through a reader of its own from
/// `make_reader`; `fs` serves everything else.
pub(crate) fn scan_parallel<R: MapFileSystem>(
    fs: &dyn MapFileSystem,
    homepath: &str,
    pool: &rayon::ThreadPool,
    make_reader: impl Fn() -> R + Sync + Send,
) -> Snapshot {
    use rayon::prelude::*;

    scan_with(fs, homepath, |paths| {
        pool.install(|| {
            paths
                .par_iter()
                .map_init(&make_reader, |reader, path| read_worldspawn(reader, path))
                .collect()
        })
    })
}

/// The worldspawn of a BSP, or the reason it could not be read.
type WorldspawnResult = (Option<WorldSpawn>, Option<String>);

fn scan_with(
    fs: &dyn MapFileSystem,
    homepath: &str,
    read_worldspawns: impl FnOnce(&[String]) -> Vec<WorldspawnResult>,
) -> Snapshot {
    let mut snapshot = Snapshot::default();
    let source_of = |path: &str| {
        fs.source_of(path)
            .map(|found_in| MapSource::from_found_in(&found_in, homepath))
    };

    let mut arenas = collect_arenas(fs, &source_of, &mut snapshot.problems);
    let levelshots = find_files(fs, "levelshots", &PICTURE_EXTENSIONS);
    let previews = find_files(fs, "levelshots/preview", &PICTURE_EXTENSIONS);
    let bot_files = find_files(fs, "maps", &[".aas"]);

    let mut paths = Vec::new();
    for file in fs.list_files("maps", ".bsp") {
        let path = format!("maps/{file}");
        if map_name_from_bsp_path(&path).is_some() {
            paths.push(path);
        } else {
            snapshot
                .problems
                .push(format!("ignored map file {}", strip_control(&path)));
        }
    }
    let worldspawns = read_worldspawns(&paths);

    for (path, (worldspawn, problem)) in paths.into_iter().zip(worldspawns) {
        let Some(name) = map_name_from_bsp_path(&path) else {
            continue;
        };
        snapshot.problems.extend(problem);
        let key = map_key(name);
        let mut map_arenas = arenas.remove(&key).unwrap_or_default();
        map_arenas.sort_by_key(|arena| rank(arena.source.as_ref()));
        let mut sources = source_of(&path).into_iter().collect::<Vec<_>>();
        let comes_from = |file: Option<&String>, source: &MapSource| {
            file.and_then(|file| source_of(file))
                .is_some_and(|found| found.is_same_place(source))
        };
        for source in &mut sources {
            source.has_aas = comes_from(bot_files.get(&key), source);
            source.has_levelshot = comes_from(levelshots.get(&key), source);
            source.has_preview = comes_from(previews.get(&key), source);
        }
        let mut info = MapInfo::assemble(name, worldspawn, map_arenas, sources);
        info.has_levelshot = levelshots.contains_key(&key);
        info.has_preview = previews.contains_key(&key);
        info.has_aas = bot_files.contains_key(&key);
        snapshot.maps.insert(key, info);
    }

    snapshot.factories = collect_factories(fs, &source_of, &mut snapshot.problems);
    snapshot
}

/// Orders sources the way the engine searches them: home scripts, base archives, base loose files, then workshop
/// items from the highest id down, in each archives before loose files and later names first.
fn rank(source: Option<&MapSource>) -> (u8, Reverse<Option<u64>>, bool, Reverse<String>) {
    let Some(source) = source else {
        return (3, Reverse(None), true, Reverse(String::new()));
    };
    let kind = match source.kind {
        SourceKind::Home => 0,
        SourceKind::Base => 1,
        SourceKind::Workshop => 2,
    };
    let loose = !source.file_name.to_ascii_lowercase().ends_with(".pk3");
    (
        kind,
        Reverse(source.workshop_id),
        loose,
        Reverse(source.file_name.clone()),
    )
}

fn script_files(fs: &dyn MapFileSystem, single: &str, extension: &str) -> Vec<String> {
    core::iter::once(single.to_owned())
        .chain(
            fs.list_files("scripts", extension)
                .into_iter()
                .map(|name| format!("scripts/{name}")),
        )
        .collect()
}

fn read_script(fs: &dyn MapFileSystem, path: &str, problems: &mut Vec<String>) -> Option<String> {
    match fs.read_file(path, SCRIPT_LIMIT) {
        Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        Err(FsError::NotFound) => None,
        Err(error) => {
            problems.push(format!("cannot read {}: {error}", strip_control(path)));
            None
        }
    }
}

fn read_worldspawn(fs: &dyn MapFileSystem, path: &str) -> WorldspawnResult {
    let reason = match fs.read_file(path, BSP_LIMIT) {
        Ok(bytes) => match parse_worldspawn(&bytes) {
            Ok(worldspawn) => return (Some(worldspawn), None),
            Err(error) => error.to_string(),
        },
        Err(FsError::NotFound) => return (None, None),
        Err(error) => error.to_string(),
    };
    (None, Some(format!("{}: {reason}", strip_control(path))))
}

fn collect_arenas(
    fs: &dyn MapFileSystem,
    source_of: &dyn Fn(&str) -> Option<MapSource>,
    problems: &mut Vec<String>,
) -> BTreeMap<String, Vec<ArenaInfo>> {
    let mut arenas = BTreeMap::<String, Vec<ArenaInfo>>::new();
    for path in script_files(fs, "scripts/arenas.txt", ".arena") {
        let Some(text) = read_script(fs, &path, problems) else {
            continue;
        };
        let parsed = parse_blocks(&text);
        if let Some(error) = parsed.error {
            problems.push(format!(
                "{}: {error:?}, the rest of the file is ignored",
                strip_control(&path)
            ));
        }
        let source = source_of(&path);
        for arena in parsed
            .blocks
            .iter()
            .filter_map(|block| ArenaInfo::from_block(block, source.clone()))
        {
            arenas.entry(map_key(&arena.map)).or_default().push(arena);
        }
    }
    arenas
}

fn collect_factories(
    fs: &dyn MapFileSystem,
    source_of: &dyn Fn(&str) -> Option<MapSource>,
    problems: &mut Vec<String>,
) -> Vec<FactoryInfo> {
    let mut factories = Vec::new();
    for path in script_files(fs, "scripts/factories.txt", ".factories") {
        let Some(text) = read_script(fs, &path, problems) else {
            continue;
        };
        match parse_factories(&text) {
            Ok(parsed) => {
                if parsed.skipped > 0 {
                    problems.push(format!(
                        "{}: {} entries skipped",
                        strip_control(&path),
                        parsed.skipped
                    ));
                }
                let source = source_of(&path);
                factories.extend(parsed.factories.into_iter().map(|factory| FactoryInfo {
                    factory,
                    source: source.clone(),
                }));
            }
            Err(error) => problems.push(format!(
                "{}: {}",
                strip_control(&path),
                strip_control(&error.0)
            )),
        }
    }
    factories.sort_by_key(|info| rank(info.source.as_ref()));
    factories
}

/// The picture paths in `dir` by map key; for several extensions the order of `PICTURE_EXTENSIONS` decides.
/// The first file of `dir` for every map name, trying the extensions in the order given.
fn find_files(fs: &dyn MapFileSystem, dir: &str, extensions: &[&str]) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for extension in extensions {
        for file in fs.list_files(dir, extension) {
            let stem = &file[..file.len() - extension.len()];
            files
                .entry(map_key(stem))
                .or_insert_with(|| format!("{dir}/{file}"));
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        gametypes::GameTypes,
        maps::{bsp::mini_bsp, filesystem::MemoryFileSystem},
    };

    const HOME: &str = "/home/ql/.quakelive/27960";
    const PAK: &str = "/srv/qlds/baseq3/pak00.pk3";
    const ITEM_LOW: &str = "/srv/qlds/steamapps/workshop/content/282440/100/map-a.pk3";
    const ITEM_HIGH: &str = "/srv/qlds/steamapps/workshop/content/282440/200/map-b.pk3";

    fn plain_bsp() -> Vec<u8> {
        mini_bsp(r#"{ "classname" "worldspawn" }"#)
    }

    fn named_bsp() -> Vec<u8> {
        mini_bsp(r#"{ "classname" "worldspawn" "message" "Simplicity CA" "author2" "Second" }"#)
    }

    fn fixture() -> MemoryFileSystem {
        MemoryFileSystem::default()
            .with_sourced_file("maps/campgrounds.bsp", plain_bsp(), PAK)
            .with_sourced_file("maps/Geit3DM4.bsp", named_bsp(), ITEM_LOW)
            .with_sourced_file("maps/bare.bsp", plain_bsp(), ITEM_LOW)
            .with_file("maps/two words.bsp", "")
            .with_sourced_file(
                "scripts/arenas.txt",
                r#"{ map campgrounds longname "Campgrounds" type "ffa ca" }
                   { map ghost type "ffa" }"#,
                PAK,
            )
            .with_sourced_file(
                "scripts/geit3dm4.arena",
                r#"{ map geit3dm4 longname "Simplicity" author "Geit" type "ca" }"#,
                ITEM_LOW,
            )
            .with_sourced_file(
                "scripts/extra.arena",
                r#"{ map GEIT3DM4 type "ffa" }"#,
                ITEM_HIGH,
            )
            .with_sourced_file(
                "scripts/factories.txt",
                r#"[{"id": "ca", "title": "Clan Arena", "basegt": "ca"}]"#,
                PAK,
            )
            .with_sourced_file(
                "scripts/local.factories",
                r#"[{"id": "ca2", "basegt": "ca"}, {"title": "broken"}]"#,
                &format!("{HOME}/baseq3"),
            )
            .with_sourced_file("maps/Campgrounds.aas", "", PAK)
            .with_file("levelshots/campgrounds.tga", "")
            .with_sourced_file("levelshots/campgrounds.jpg", "", PAK)
            .with_file("levelshots/preview/campgrounds.jpg", "")
    }

    #[test]
    #[cfg_attr(miri, ignore = "starts threads")]
    fn parallel_scan_finds_exactly_what_the_sequential_scan_finds() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(3)
            .build()
            .expect("a small pool can be built");

        let parallel = scan_parallel(&fixture(), HOME, &pool, fixture);

        assert_eq!(parallel, scan(&fixture(), HOME));
    }

    #[test]
    fn maps_exist_where_a_bsp_exists_and_blocks_without_bsp_are_dropped() {
        let snapshot = scan(&fixture(), HOME);

        let names = snapshot
            .maps
            .values()
            .map(|map| map.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["bare", "campgrounds", "Geit3DM4"]);
        assert!(
            snapshot
                .problems
                .iter()
                .any(|problem| problem.contains("two words"))
        );
    }

    #[test]
    fn arena_data_reaches_the_map_and_the_source_is_derived() {
        let snapshot = scan(&fixture(), HOME);

        let camp = &snapshot.maps["campgrounds"];
        assert_eq!(camp.longname, "Campgrounds");
        assert_eq!(
            camp.gametypes(),
            vec![GameTypes::FreeForAll, GameTypes::ClanArena]
        );
        assert_eq!(camp.sources[0].file_name, "pak00.pk3");
        assert_eq!(camp.sources[0].kind, SourceKind::Base);
    }

    #[test]
    fn a_script_over_the_limit_is_a_problem_and_a_source_less_file_ranks_last() {
        let big = vec![b' '; usize::try_from(SCRIPT_LIMIT).expect("fits") + 1];
        let fs = MemoryFileSystem::default().with_file("scripts/huge.arena", big);

        let snapshot = scan(&fs, HOME);

        assert!(
            snapshot
                .problems
                .iter()
                .any(|problem| problem.contains("cannot read scripts/huge.arena"))
        );
        let sourced = MapSource::from_found_in("/base/baseq3/pak00.pk3", HOME);
        assert!(rank(Some(&sourced)) < rank(None));
    }

    #[test]
    #[cfg_attr(miri, ignore = "allocates more than 128 MiB")]
    fn a_bsp_that_cannot_be_read_is_reported_but_the_map_stays() {
        let big = vec![0_u8; usize::try_from(BSP_LIMIT).expect("fits") + 1];
        let fs = MemoryFileSystem::default().with_file("maps/huge.bsp", big);

        let snapshot = scan(&fs, HOME);

        assert!(snapshot.maps.contains_key("huge"));
        assert!(
            snapshot
                .problems
                .iter()
                .any(|problem| problem.contains("huge"))
        );
    }

    #[test]
    fn map_without_block_is_playable_everywhere() {
        let bare = &scan(&fixture(), HOME).maps["bare"];

        assert!(bare.arenas.is_empty());
        assert_eq!(bare.gametypes().len(), GameTypes::ALL.len());
        assert_eq!(bare.sources[0].workshop_id, Some(100));
    }

    #[test]
    fn blocks_are_matched_ignoring_case_and_ranked_by_source() {
        let geit = &scan(&fixture(), HOME).maps["geit3dm4"];

        let sources = geit
            .arenas
            .iter()
            .map(|arena| arena.source.as_ref().and_then(|source| source.workshop_id))
            .collect::<Vec<_>>();
        assert_eq!(sources, vec![Some(200), Some(100)]);
        assert_eq!(geit.gametypes(), vec![GameTypes::FreeForAll]);
        assert_eq!(geit.arenas.len(), 2);
    }

    #[test]
    fn worldspawn_of_the_bsp_beats_the_arena_block() {
        let geit = &scan(&fixture(), HOME).maps["geit3dm4"];

        assert_eq!(geit.longname, "Simplicity CA");
        assert_eq!(geit.author.as_deref(), Some("Geit"));
        assert_eq!(geit.author2.as_deref(), Some("Second"));
    }

    #[test]
    fn a_map_with_a_damaged_bsp_is_kept_and_reported() {
        let fs = fixture().with_sourced_file("maps/broken.bsp", "not a bsp", PAK);

        let snapshot = scan(&fs, HOME);

        assert!(snapshot.maps.contains_key("broken"));
        assert!(
            snapshot
                .problems
                .iter()
                .any(|problem| problem.contains("maps/broken.bsp: not a BSP file"))
        );
    }

    #[test]
    fn pictures_and_bot_files_are_found_by_map_name() {
        let camp = &scan(&fixture(), HOME).maps["campgrounds"];

        assert!(camp.has_levelshot);
        assert!(camp.has_preview);
        assert!(!scan(&fixture(), HOME).maps["bare"].has_levelshot);
        assert!(!scan(&fixture(), HOME).maps["bare"].has_preview);
        assert!(camp.has_aas);
        let source = &camp.sources[0];
        assert!(source.has_aas && source.has_levelshot && !source.has_preview);
        assert!(!scan(&fixture(), HOME).maps["bare"].has_aas);
    }

    #[test]
    fn factories_come_home_first_and_bad_entries_are_reported() {
        let snapshot = scan(&fixture(), HOME);

        let ids = snapshot
            .factories
            .iter()
            .map(|info| {
                (
                    info.factory.id.as_str(),
                    info.source.as_ref().map(|source| source.kind),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                ("ca2", Some(SourceKind::Home)),
                ("ca", Some(SourceKind::Base))
            ]
        );
        assert!(
            snapshot
                .problems
                .iter()
                .any(|problem| problem.contains("1 entries skipped"))
        );
    }

    #[test]
    fn a_broken_file_is_reported_and_does_not_stop_the_scan() {
        let fs = fixture()
            .with_sourced_file("scripts/bad.arena", "{ map x", PAK)
            .with_sourced_file("scripts/bad.factories", "[{", PAK);

        let snapshot = scan(&fs, HOME);

        assert!(
            snapshot
                .problems
                .iter()
                .any(|problem| problem.contains("scripts/bad.arena"))
        );
        assert!(
            snapshot
                .problems
                .iter()
                .any(|problem| problem.contains("scripts/bad.factories"))
        );
        assert_eq!(snapshot.maps.len(), 3);
    }

    #[test]
    fn an_empty_file_system_gives_an_empty_snapshot() {
        assert_eq!(
            scan(&MemoryFileSystem::default(), HOME),
            Snapshot::default()
        );
    }
}
