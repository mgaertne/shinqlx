//! A file system that reads the archives and directories of a frozen [`SourceIndex`] by itself.
//!
//! It needs nothing from the engine, so it can run in a thread of its own while the engine goes on.

use alloc::collections::BTreeMap;
use std::{fs::File, io::Read, path::Path};

use parking_lot::Mutex;
use zip::{ZipArchive, result::ZipError};

use super::{
    filesystem::{FsError, MapFileSystem, has_extension},
    sources::{Located, SourceIndex},
};

/// An opened archive with its entry names by lowercase name: the engine keeps the names of an archive in lowercase,
/// the archive itself may spell them otherwise.
#[derive(Debug)]
struct OpenArchive {
    zip: ZipArchive<File>,
    names: BTreeMap<String, String>,
}

/// Reads from the archives and directories listed in a [`SourceIndex`], in the order of the index.
#[derive(Debug)]
pub(crate) struct ArchiveFileSystem {
    index: SourceIndex,
    open_archives: Mutex<BTreeMap<String, OpenArchive>>,
}

impl ArchiveFileSystem {
    pub(crate) fn new(index: SourceIndex) -> Self {
        Self {
            index,
            open_archives: Mutex::new(BTreeMap::new()),
        }
    }

    fn read_from_archive(
        path: &str,
        archive: &mut ZipArchive<File>,
        name: &str,
        limit: u64,
    ) -> Result<Vec<u8>, FsError> {
        let entry = archive.by_name(name).map_err(|error| match error {
            ZipError::FileNotFound => FsError::NotFound,
            other => FsError::Io(format!("{path}: {other}")),
        })?;
        if entry.size() > limit {
            return Err(FsError::TooLarge { limit });
        }
        let mut content = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        entry
            .take(limit.saturating_add(1))
            .read_to_end(&mut content)
            .map_err(|error| FsError::Io(format!("{path}: {error}")))?;
        if content.len() as u64 > limit {
            return Err(FsError::TooLarge { limit });
        }
        Ok(content)
    }
}

fn read_loose(full_path: &str, limit: u64) -> Result<Vec<u8>, FsError> {
    let mut content = Vec::new();
    File::open(full_path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut content)?;
    if content.len() as u64 > limit {
        return Err(FsError::TooLarge { limit });
    }
    Ok(content)
}

impl MapFileSystem for ArchiveFileSystem {
    fn list_files(&self, dir: &str, extension: &str) -> Vec<String> {
        let mut names = self.index.pack_file_names(dir, extension);
        for root in self.index.directory_roots() {
            let Ok(entries) = std::fs::read_dir(format!("{root}/{dir}")) else {
                continue;
            };
            names.extend(entries.filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_string_lossy().into_owned();
                (entry.metadata().ok()?.is_file() && has_extension(&name, extension))
                    .then_some(name)
            }));
        }
        names.sort();
        names.dedup();
        names
    }

    fn read_file(&self, path: &str, limit: u64) -> Result<Vec<u8>, FsError> {
        match self
            .index
            .locate(path, &|full_path| Path::new(full_path).is_file())
            .ok_or(FsError::NotFound)?
        {
            Located::Directory { root } => read_loose(&format!("{root}/{path}"), limit),
            Located::Pack {
                path: archive_path,
                name,
            } => {
                let mut open = self.open_archives.lock();
                if !open.contains_key(&archive_path) {
                    let zip = ZipArchive::new(File::open(&archive_path)?)
                        .map_err(|error| FsError::Io(format!("{archive_path}: {error}")))?;
                    let names = zip
                        .file_names()
                        .filter_map(Result::ok)
                        .map(|name| (name.to_ascii_lowercase(), name.into_owned()))
                        .collect();
                    open.insert(archive_path.clone(), OpenArchive { zip, names });
                }
                let archive = open.get_mut(&archive_path).ok_or(FsError::NotFound)?;
                let spelled = archive
                    .names
                    .get(&name.to_ascii_lowercase())
                    .ok_or(FsError::NotFound)?;
                Self::read_from_archive(&archive_path, &mut archive.zip, spelled, limit)
            }
        }
    }

    fn source_of(&self, path: &str) -> Option<String> {
        self.index
            .source_of(path, &|full_path| Path::new(full_path).is_file())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use pretty_assertions::assert_eq;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    use super::*;
    use crate::maps::sources::SearchEntry;

    fn write_pack(path: &Path, files: &[(&str, &[u8], CompressionMethod)]) {
        let mut writer = ZipWriter::new(File::create(path).expect("pack can be created"));
        for (name, content, method) in files {
            writer
                .start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(*method),
                )
                .expect("entry can start");
            writer.write_all(content).expect("entry can be written");
        }
        writer.finish().expect("pack can be finished");
    }

    fn pack_entry(path: &Path, names: &[&str]) -> SearchEntry {
        SearchEntry::Pack {
            path: path.to_string_lossy().into_owned(),
            names: names
                .iter()
                .map(|name| (name.to_ascii_lowercase(), (*name).to_owned()))
                .collect(),
        }
    }

    fn fixture() -> (tempfile::TempDir, ArchiveFileSystem) {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = dir.path().join("base");
        std::fs::create_dir_all(base.join("baseq3/maps")).expect("dirs");
        std::fs::write(base.join("baseq3/maps/loose.bsp"), b"loose").expect("loose file");
        std::fs::write(base.join("baseq3/maps/a.bsp"), b"loose a").expect("loose a");
        let pack = dir.path().join("pak.pk3");
        write_pack(
            &pack,
            &[
                ("Maps/A.bsp", b"packed a", CompressionMethod::Deflated),
                ("maps/b.bsp", b"packed b", CompressionMethod::Stored),
            ],
        );
        let index = SourceIndex::new(vec![
            pack_entry(&pack, &["Maps/A.bsp", "maps/b.bsp"]),
            SearchEntry::Directory {
                path: base.to_string_lossy().into_owned(),
                gamedir: "baseq3".into(),
            },
        ]);
        (dir, ArchiveFileSystem::new(index))
    }

    #[test]
    fn reads_entries_that_the_archive_spells_differently_from_the_engine_list() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pack = dir.path().join("workshop.pk3");
        write_pack(
            &pack,
            &[(
                "SCRIPTS/Mixed.ARENA",
                b"{ map x }",
                CompressionMethod::Deflated,
            )],
        );
        let fs = ArchiveFileSystem::new(SourceIndex::new(vec![pack_entry(
            &pack,
            &["scripts/mixed.arena"],
        )]));

        assert_eq!(fs.list_files("scripts", ".arena"), vec!["mixed.arena"]);
        assert_eq!(
            fs.read_file("scripts/mixed.arena", 100).expect("found"),
            b"{ map x }"
        );
    }

    #[test]
    fn a_packed_entry_over_the_limit_is_refused() {
        let (_dir, fs) = fixture();

        assert_eq!(
            fs.read_file("maps/b.bsp", 3),
            Err(FsError::TooLarge { limit: 3 })
        );
    }

    #[test]
    fn a_pack_that_is_no_zip_file_is_an_io_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pack = dir.path().join("broken.pk3");
        std::fs::write(&pack, b"this is not a zip file").expect("file");
        let fs = ArchiveFileSystem::new(SourceIndex::new(vec![pack_entry(&pack, &["maps/x.bsp"])]));

        assert!(matches!(
            fs.read_file("maps/x.bsp", 100),
            Err(FsError::Io(_))
        ));
    }

    #[test]
    fn an_entry_that_the_index_knows_but_the_pack_lacks_is_not_found() {
        let dir = tempfile::tempdir().expect("temp dir");
        let pack = dir.path().join("short.pk3");
        write_pack(&pack, &[("maps/a.bsp", b"a", CompressionMethod::Stored)]);
        let fs = ArchiveFileSystem::new(SourceIndex::new(vec![pack_entry(
            &pack,
            &["maps/a.bsp", "maps/ghost.bsp"],
        )]));

        assert_eq!(fs.read_file("maps/ghost.bsp", 100), Err(FsError::NotFound));
    }

    #[test]
    fn lists_packed_and_loose_files_once() {
        let (_dir, fs) = fixture();

        assert_eq!(
            fs.list_files("maps", ".bsp"),
            vec!["A.bsp", "a.bsp", "b.bsp", "loose.bsp"]
        );
    }

    #[test]
    fn reads_deflated_stored_and_loose_files_in_engine_order() {
        let (_dir, fs) = fixture();

        assert_eq!(
            fs.read_file("maps/a.bsp", 100).expect("packed"),
            b"packed a"
        );
        assert_eq!(
            fs.read_file("maps/b.bsp", 100).expect("stored"),
            b"packed b"
        );
        assert_eq!(
            fs.read_file("maps/loose.bsp", 100).expect("loose"),
            b"loose"
        );
        assert_eq!(fs.read_file("maps/none.bsp", 100), Err(FsError::NotFound));
    }

    #[test]
    fn refuses_files_over_the_limit() {
        let (_dir, fs) = fixture();

        assert_eq!(
            fs.read_file("maps/a.bsp", 3),
            Err(FsError::TooLarge { limit: 3 })
        );
        assert_eq!(
            fs.read_file("maps/loose.bsp", 3),
            Err(FsError::TooLarge { limit: 3 })
        );
    }

    #[test]
    fn names_the_source_the_engine_would_read_from() {
        let (dir, fs) = fixture();

        assert_eq!(
            fs.source_of("maps/a.bsp"),
            Some(dir.path().join("pak.pk3").to_string_lossy().into_owned())
        );
        assert_eq!(
            fs.source_of("maps/loose.bsp"),
            Some(format!("{}/baseq3", dir.path().join("base").display()))
        );
    }
}
