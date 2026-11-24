//! The maps and factories that the game can load, read from the `pk3` files of the search paths.

use alloc::{borrow::Cow, collections::BTreeMap};
use core::time::Duration;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Instant, SystemTime},
};

use itertools::Itertools;
use serde_json::Value;
use zip::ZipArchive;

/// The largest declaration file that is read from a `pk3`.
const MAX_DECLARATION_BYTES: u64 = 4 * 1024 * 1024;

/// How long the result of a scan is used before the directories are looked at again.
const RESCAN_AFTER: Duration = Duration::from_secs(30);

/// A `pk3` that holds a map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MapSource {
    pub(crate) pk3: PathBuf,
    /// The id of the workshop item the `pk3` belongs to.
    pub(crate) workshop_id: Option<u64>,
}

/// One declaration of a map in an `.arena` file or in `arenas.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArenaEntry {
    pub(crate) map: String,
    pub(crate) longname: Option<String>,
    pub(crate) gametypes: Vec<String>,
}

/// A map that the game can load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MapRecord {
    pub(crate) name: String,
    pub(crate) longname: Option<String>,
    /// The gametypes the map declares, `None` if no declaration exists.
    pub(crate) gametypes: Option<Vec<String>>,
    pub(crate) sources: Vec<MapSource>,
}

/// What a single `pk3` contributes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PackContent {
    maps: Vec<String>,
    consolidated_arenas: Vec<ArenaEntry>,
    single_arenas: Vec<ArenaEntry>,
    factories: Vec<String>,
}

/// All maps and factories of the search paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MapCatalog {
    maps: BTreeMap<String, MapRecord>,
    factories: Vec<String>,
}

impl MapCatalog {
    /// The maps in alphabetical order.
    pub(crate) fn maps(&self) -> impl Iterator<Item = &MapRecord> {
        self.maps.values()
    }

    pub(crate) fn map(&self, name: &str) -> Option<&MapRecord> {
        self.maps.get(&name.to_lowercase())
    }

    /// The ids of the factories without duplicates, in the order they are declared.
    pub(crate) fn factories(&self) -> &[String] {
        &self.factories
    }

    /// Whether a map declares a gametype: `None` if the map declares none at all.
    pub(crate) fn supports(&self, name: &str, gametype: &str) -> Option<bool> {
        let gametype = gametype.to_lowercase();
        self.map(name)?
            .gametypes
            .as_ref()
            .map(|gametypes| gametypes.contains(&gametype))
    }
}

fn push_unique(mut items: Vec<String>, item: String) -> Vec<String> {
    if !items.contains(&item) {
        items.push(item);
    }
    items
}

fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    let mut rest = text;
    core::iter::from_fn(move || {
        rest = rest.trim_start();
        let first = rest.chars().next()?;
        if first == '"' {
            let body = &rest[1..];
            let end = body.find('"').unwrap_or(body.len());
            let token = body[..end].to_string();
            rest = body.get(end + 1..).unwrap_or("");
            Some(token)
        } else {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '"')
                .unwrap_or(rest.len())
                .max(first.len_utf8());
            let token = rest[..end].to_string();
            rest = &rest[end..];
            Some(token)
        }
    })
}

/// Parses `.arena` files and `arenas.txt`: blocks of `{ key "value" ... }`.
pub(crate) fn parse_arenas(text: &str) -> Vec<ArenaEntry> {
    let mut entries = Vec::new();
    let mut current: Option<BTreeMap<String, String>> = None;
    let mut tokens = tokens(text);
    while let Some(token) = tokens.next() {
        match (token.as_str(), current.as_mut()) {
            ("{", None) => current = Some(BTreeMap::new()),
            ("}", Some(_)) => {
                if let Some(block) = current.take()
                    && let Some(map) = block.get("map")
                {
                    entries.push(ArenaEntry {
                        map: map.to_lowercase(),
                        longname: block.get("longname").cloned(),
                        gametypes: block
                            .get("type")
                            .map(|types| {
                                types
                                    .split_whitespace()
                                    .map(str::to_lowercase)
                                    .fold(Vec::new(), push_unique)
                            })
                            .unwrap_or_default(),
                    });
                }
            }
            (key, Some(block)) => {
                if let Some(value) = tokens.next() {
                    block.insert(key.to_lowercase(), value);
                }
            }
            _ => {}
        }
    }
    entries
}

/// Parses the ids out of a factory file, which is a JSON array of objects with an `id`.
pub(crate) fn parse_factories(text: &str) -> Vec<String> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|factory| factory.get("id")?.as_str().map(str::to_owned))
        .fold(Vec::new(), push_unique)
}

fn read_text(file: &mut impl Read) -> Option<String> {
    let mut buffer = Vec::new();
    file.take(MAX_DECLARATION_BYTES)
        .read_to_end(&mut buffer)
        .ok()?;
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

/// Reads the maps, arena declarations and factories of a `pk3`.
pub(crate) fn read_pack(path: &Path) -> Option<PackContent> {
    let mut archive = ZipArchive::new(File::open(path).ok()?).ok()?;
    let names = archive
        .file_names()
        .filter_map(Result::ok)
        .map(Cow::into_owned)
        .collect::<Vec<String>>();
    let mut content = PackContent::default();
    for name in names {
        let lower = name.to_lowercase();
        let Some(file_name) = lower.rsplit('/').next() else {
            continue;
        };
        let in_folder = |folder: &str| {
            lower
                .strip_prefix(folder)
                .is_some_and(|rest| rest == file_name)
        };
        if in_folder("maps/")
            && let Some(map) = file_name.strip_suffix(".bsp")
        {
            content.maps.push(map.to_owned());
        } else if in_folder("scripts/") {
            if file_name == "arenas.txt" {
                content.consolidated_arenas.extend(
                    read_entry(&mut archive, &name)
                        .map(|text| parse_arenas(&text))
                        .unwrap_or_default(),
                );
            } else if file_name.ends_with(".arena") {
                content.single_arenas.extend(
                    read_entry(&mut archive, &name)
                        .map(|text| parse_arenas(&text))
                        .unwrap_or_default(),
                );
            } else if file_name == "factories.txt" || file_name.ends_with(".factories") {
                content.factories.extend(
                    read_entry(&mut archive, &name)
                        .map(|text| parse_factories(&text))
                        .unwrap_or_default(),
                );
            }
        }
    }
    Some(content)
}

fn read_entry(archive: &mut ZipArchive<File>, name: &str) -> Option<String> {
    archive
        .by_name(name)
        .ok()
        .and_then(|mut file| read_text(&mut file))
}

fn packs_in(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pk3"))
        })
        .sorted()
        .collect()
}

/// The `pk3` files of the search paths in the order the engine loads them, later files win.
///
/// The workshop directory holds one numeric folder per item.
fn search_order(base: &Path, workshop: Option<&Path>, home: &Path) -> Vec<(PathBuf, Option<u64>)> {
    let workshop_packs = workshop
        .into_iter()
        .flat_map(|workshop| {
            std::fs::read_dir(workshop)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|entry| {
                    let id = entry.file_name().to_str()?.parse::<u64>().ok()?;
                    Some((id, entry.path()))
                })
                .sorted()
                .flat_map(|(id, folder)| {
                    packs_in(&folder)
                        .into_iter()
                        .map(move |pack| (pack, Some(id)))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    packs_in(&base.join("baseq3"))
        .into_iter()
        .map(|pack| (pack, None))
        .chain(workshop_packs)
        .chain(
            packs_in(&home.join("baseq3"))
                .into_iter()
                .map(|pack| (pack, None)),
        )
        .collect()
}

fn build_catalog(packs: &[(PathBuf, Option<u64>, PackContent)]) -> MapCatalog {
    let mut declarations: BTreeMap<String, ArenaEntry> = BTreeMap::new();
    let mut maps: BTreeMap<String, MapRecord> = BTreeMap::new();
    let mut factories = Vec::new();

    for (pk3, workshop_id, content) in packs {
        content
            .consolidated_arenas
            .iter()
            .chain(content.single_arenas.iter())
            .for_each(|entry| {
                declarations.insert(entry.map.clone(), entry.clone());
            });
        factories.extend(content.factories.iter().cloned());
        for map in &content.maps {
            maps.entry(map.clone())
                .or_insert_with(|| MapRecord {
                    name: map.clone(),
                    longname: None,
                    gametypes: None,
                    sources: Vec::new(),
                })
                .sources
                .push(MapSource {
                    pk3: pk3.clone(),
                    workshop_id: *workshop_id,
                });
        }
    }

    maps.values_mut().for_each(|record| {
        if let Some(entry) = declarations.get(&record.name) {
            record.longname = entry.longname.clone();
            record.gametypes = Some(entry.gametypes.clone());
        }
    });

    MapCatalog {
        maps,
        factories: factories.into_iter().fold(Vec::new(), push_unique),
    }
}

type PackKey = (u64, Option<SystemTime>);

#[derive(Default)]
struct ScanCache {
    packs: BTreeMap<PathBuf, (PackKey, PackContent)>,
    catalog: Option<(Instant, MapCatalog)>,
}

static SCAN_CACHE: Mutex<ScanCache> = Mutex::new(ScanCache {
    packs: BTreeMap::new(),
    catalog: None,
});

fn pack_key(path: &Path) -> Option<PackKey> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.len(), metadata.modified().ok()))
}

/// Scans the search paths, and reads only the `pk3` files that changed since the last scan.
pub(crate) fn scan(base: &Path, workshop: Option<&Path>, home: &Path) -> MapCatalog {
    let mut cache = SCAN_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let found = search_order(base, workshop, home);
    let contents = found
        .into_iter()
        .filter_map(|(path, workshop_id)| {
            let key = pack_key(&path)?;
            let content = match cache.packs.get(&path) {
                Some((cached_key, content)) if *cached_key == key => content.clone(),
                _ => {
                    let content = read_pack(&path)?;
                    cache.packs.insert(path.clone(), (key, content.clone()));
                    content
                }
            };
            Some((path, workshop_id, content))
        })
        .collect::<Vec<_>>();

    let catalog = build_catalog(&contents);
    cache.catalog = Some((Instant::now(), catalog.clone()));
    catalog
}

/// Makes the next lookup scan the directories again.
#[cfg(test)]
pub(crate) fn forget_scan() {
    SCAN_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .catalog = None;
}

/// The catalog of the last scan if it is not older than 30 seconds.
pub(crate) fn recent_catalog() -> Option<MapCatalog> {
    SCAN_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .catalog
        .as_ref()
        .filter(|(scanned, _)| scanned.elapsed() < RESCAN_AFTER)
        .map(|(_, catalog)| catalog.clone())
}

/// Writes a `pk3` with the given files for the tests.
#[cfg(test)]
pub(crate) fn write_pack(path: &Path, files: &[(&str, &str)]) {
    use std::io::Write;

    use zip::{ZipWriter, write::SimpleFileOptions};

    let mut writer = ZipWriter::new(File::create(path).expect("could not create pk3"));
    files.iter().for_each(|(name, content)| {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("could not start file");
        writer
            .write_all(content.as_bytes())
            .expect("could not write file");
    });
    writer.finish().expect("could not finish pk3");
}

#[cfg(test)]
mod map_catalog_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::{
        ArenaEntry, MapSource, parse_arenas, parse_factories, read_pack, recent_catalog, scan,
        write_pack,
    };
    use crate::prelude::serial;

    #[test]
    fn parse_arenas_reads_every_block() {
        let entries = parse_arenas(
            "{\nmap\t\"AeroWalk\"\nlongname\t\"Aerowalk\"\ntype\t\"ffa duel ca ca\"\n}\n{\nmap \"x\"\n}",
        );

        assert_eq!(
            entries,
            vec![
                ArenaEntry {
                    map: "aerowalk".to_string(),
                    longname: Some("Aerowalk".to_string()),
                    gametypes: vec!["ffa".to_string(), "duel".to_string(), "ca".to_string()],
                },
                ArenaEntry {
                    map: "x".to_string(),
                    longname: None,
                    gametypes: vec![],
                },
            ]
        );
    }

    #[rstest]
    #[case("")]
    #[case("{ map")]
    #[case("} } {")]
    #[case("{ \"unterminated")]
    fn parse_arenas_survives_broken_input(#[case] text: &str) {
        let _ = parse_arenas(text);
    }

    #[test]
    fn parse_factories_reads_the_ids_once() {
        let ids = parse_factories(
            r#"[{"id": "ctf", "title": "CTF"}, {"id": "ca"}, {"id": "ctf"}, {"title": "x"}]"#,
        );

        assert_eq!(ids, vec!["ctf".to_string(), "ca".to_string()]);
    }

    #[rstest]
    #[case("not json")]
    #[case("{}")]
    #[case("")]
    fn parse_factories_of_garbage_is_empty(#[case] text: &str) {
        assert!(parse_factories(text).is_empty());
    }

    #[test]
    fn read_pack_of_something_that_is_no_pk3_is_none() {
        let directory = tempfile::tempdir().expect("could not create directory");
        let path = directory.path().join("broken.pk3");
        std::fs::write(&path, "no zip").expect("could not write file");

        assert!(read_pack(&path).is_none());
    }

    #[test]
    #[serial]
    fn scan_combines_the_search_paths() {
        let base = tempfile::tempdir().expect("could not create directory");
        let workshop = tempfile::tempdir().expect("could not create directory");
        let home = tempfile::tempdir().expect("could not create directory");
        std::fs::create_dir_all(base.path().join("baseq3")).expect("could not create directory");
        std::fs::create_dir_all(home.path().join("baseq3")).expect("could not create directory");
        std::fs::create_dir_all(workshop.path().join("1234")).expect("could not create directory");
        write_pack(
            &base.path().join("baseq3/pak00.pk3"),
            &[
                ("maps/aerowalk.bsp", ""),
                ("maps/overload.bsp", ""),
                (
                    "scripts/arenas.txt",
                    "{\nmap \"aerowalk\"\nlongname \"Aerowalk\"\ntype \"ffa duel\"\n}",
                ),
                ("scripts/factories.txt", r#"[{"id": "ffa"}, {"id": "ca"}]"#),
            ],
        );
        write_pack(
            &workshop.path().join("1234/custom.pk3"),
            &[
                ("maps/custom.bsp", ""),
                (
                    "scripts/custom.arena",
                    "{\nmap \"custom\"\nlongname \"Custom Map\"\ntype \"ca ctf\"\n}",
                ),
                (
                    "scripts/more.factories",
                    r#"[{"id": "ca"}, {"id": "mine"}]"#,
                ),
            ],
        );
        write_pack(
            &home.path().join("baseq3/zz.pk3"),
            &[(
                "scripts/zz.arena",
                "{\nmap \"aerowalk\"\nlongname \"Aerowalk Redux\"\ntype \"ca\"\n}",
            )],
        );

        let catalog = scan(base.path(), Some(workshop.path()), home.path());

        assert_eq!(
            catalog
                .maps()
                .map(|record| record.name.as_str())
                .collect::<Vec<_>>(),
            vec!["aerowalk", "custom", "overload"]
        );
        let custom = catalog.map("Custom").expect("custom is not installed");
        assert_eq!(custom.longname.as_deref(), Some("Custom Map"));
        assert_eq!(
            custom.sources,
            vec![MapSource {
                pk3: workshop.path().join("1234/custom.pk3"),
                workshop_id: Some(1234),
            }]
        );
        let aerowalk = catalog.map("aerowalk").expect("aerowalk is not installed");
        assert_eq!(aerowalk.longname.as_deref(), Some("Aerowalk Redux"));
        assert_eq!(catalog.supports("aerowalk", "ca"), Some(true));
        assert_eq!(catalog.supports("aerowalk", "ffa"), Some(false));
        assert_eq!(catalog.supports("overload", "ffa"), None);
        assert_eq!(catalog.supports("missing", "ffa"), None);
        assert_eq!(
            catalog.factories(),
            ["ffa".to_string(), "ca".to_string(), "mine".to_string()]
        );
        assert!(recent_catalog().is_some());
    }
}
