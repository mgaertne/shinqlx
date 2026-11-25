//! Finds out which archive or directory a file comes from, from the search path list of the engine.
//!
//! The engine searches its sources in a fixed order and takes the first one that has a file. This module keeps that
//! order, so the answer is the one the engine itself gets.

use alloc::collections::BTreeMap;

/// One entry of the engine's search path list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SearchEntry {
    /// An archive and its files, by lowercase name, with the name as the archive spells it.
    Pack {
        path: String,
        names: BTreeMap<String, String>,
    },
    /// A directory; its files are found under `<path>/<gamedir>/`.
    Directory { path: String, gamedir: String },
}

/// Where the engine reads a file from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Located {
    /// The archive at `path`, where the file is called `name`.
    Pack { path: String, name: String },
    /// The directory `root` (`<path>/<gamedir>`), where the file is found under its own name.
    Directory { root: String },
}

impl Located {
    /// The path of the archive or directory.
    pub(crate) fn source(&self) -> &str {
        match self {
            Self::Pack { path, .. } => path,
            Self::Directory { root } => root,
        }
    }
}

/// The search path list in the order of the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourceIndex {
    entries: Vec<SearchEntry>,
}

impl SourceIndex {
    pub(crate) fn new(entries: Vec<SearchEntry>) -> Self {
        Self { entries }
    }

    /// The directories of the list, as `<path>/<gamedir>`, in order.
    pub(crate) fn directory_roots(&self) -> impl Iterator<Item = String> + '_ {
        self.entries.iter().filter_map(|entry| match entry {
            SearchEntry::Directory { path, gamedir } => Some(format!("{path}/{gamedir}")),
            SearchEntry::Pack { .. } => None,
        })
    }

    /// The names (without directory) of the archive files in `dir` that end with `extension`, ignoring case.
    pub(crate) fn pack_file_names(&self, dir: &str, extension: &str) -> Vec<String> {
        let prefix = format!("{}/", dir.trim_end_matches('/').to_ascii_lowercase());
        let extension = extension.to_ascii_lowercase();
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                SearchEntry::Pack { names, .. } => Some(names),
                SearchEntry::Directory { .. } => None,
            })
            .flat_map(|names| {
                let first = format!("{prefix}\0");
                let last = format!("{prefix}\u{10ffff}");
                names
                    .range(first..=last)
                    .filter(|(lowercase, _)| {
                        let rest = &lowercase[prefix.len()..];
                        !rest.contains('/')
                            && rest.len() > extension.len()
                            && rest.ends_with(&extension)
                    })
                    .map(|(_, original)| original[prefix.len()..].to_owned())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The first source in engine order that has `file`. `file_exists` tells whether a file of a directory exists,
    /// given its complete path.
    pub(crate) fn locate(&self, file: &str, file_exists: &dyn Fn(&str) -> bool) -> Option<Located> {
        let lowercase = file.to_ascii_lowercase();
        self.entries.iter().find_map(|entry| match entry {
            SearchEntry::Pack { path, names } => names.get(&lowercase).map(|name| Located::Pack {
                path: path.clone(),
                name: name.clone(),
            }),
            SearchEntry::Directory { path, gamedir } => {
                let root = format!("{path}/{gamedir}");
                file_exists(&format!("{root}/{file}")).then_some(Located::Directory { root })
            }
        })
    }

    /// The path of the archive, or `<path>/<gamedir>` of the directory, that the engine reads `file` from.
    pub(crate) fn source_of(
        &self,
        file: &str,
        file_exists: &dyn Fn(&str) -> bool,
    ) -> Option<String> {
        self.locate(file, file_exists)
            .map(|located| located.source().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn pack(path: &str, names: &[&str]) -> SearchEntry {
        SearchEntry::Pack {
            path: path.to_owned(),
            names: names
                .iter()
                .map(|name| (name.to_ascii_lowercase(), (*name).to_owned()))
                .collect(),
        }
    }

    fn directory(path: &str) -> SearchEntry {
        SearchEntry::Directory {
            path: path.to_owned(),
            gamedir: "baseq3".to_owned(),
        }
    }

    fn index() -> SourceIndex {
        SourceIndex::new(vec![
            directory("/home"),
            pack(
                "/base/baseq3/pak00.pk3",
                &["maps/a.bsp", "scripts/arenas.txt"],
            ),
            directory("/base"),
            pack("/ws/282440/100/map.pk3", &["maps/a.bsp", "Maps/B.BSP"]),
        ])
    }

    #[test]
    fn first_entry_in_engine_order_wins() {
        let nothing_loose = |_: &str| false;

        assert_eq!(
            index().source_of("maps/a.bsp", &nothing_loose).as_deref(),
            Some("/base/baseq3/pak00.pk3")
        );
        assert_eq!(
            index().source_of("maps/b.bsp", &nothing_loose).as_deref(),
            Some("/ws/282440/100/map.pk3")
        );
    }

    #[test]
    fn loose_file_in_an_earlier_directory_beats_an_archive() {
        let loose = |path: &str| path == "/home/baseq3/scripts/arenas.txt";

        assert_eq!(
            index().source_of("scripts/arenas.txt", &loose).as_deref(),
            Some("/home/baseq3")
        );
    }

    #[test]
    fn names_are_compared_ignoring_case_and_located_with_their_own_spelling() {
        assert_eq!(
            index().locate("MAPS/b.bsp", &|_| false),
            Some(Located::Pack {
                path: "/ws/282440/100/map.pk3".into(),
                name: "Maps/B.BSP".into()
            })
        );
    }

    #[test]
    fn unknown_file_has_no_source() {
        assert_eq!(index().source_of("maps/none.bsp", &|_| false), None);
        assert_eq!(
            SourceIndex::default().source_of("maps/a.bsp", &|_| true),
            None
        );
    }

    #[test]
    fn directory_asks_for_the_complete_path() {
        let asked = core::cell::RefCell::new(Vec::new());

        index().source_of("maps/none.bsp", &|path| {
            asked.borrow_mut().push(path.to_owned());
            false
        });

        assert_eq!(
            asked.into_inner(),
            vec!["/home/baseq3/maps/none.bsp", "/base/baseq3/maps/none.bsp"]
        );
    }

    #[test]
    fn archive_listing_keeps_the_spelling_and_skips_deeper_directories() {
        let index = SourceIndex::new(vec![pack(
            "/p.pk3",
            &[
                "maps/a.bsp",
                "Maps/B.BSP",
                "maps/sub/c.bsp",
                "maps/a.aas",
                "mapsx/d.bsp",
            ],
        )]);

        let mut names = index.pack_file_names("maps", ".bsp");
        names.sort();

        assert_eq!(names, vec!["B.BSP", "a.bsp"]);
    }

    #[test]
    fn directory_roots_come_in_order() {
        assert_eq!(
            index().directory_roots().collect::<Vec<_>>(),
            vec!["/home/baseq3", "/base/baseq3"]
        );
    }
}
