//! The file system the scan reads from, as a trait so that the scan runs against memory in tests and under Miri.
//!
//! Paths are `/` separated and relative to the root of the virtual file system, like the engine sees them
//! (`maps/aerowalk.bsp`). Only [`RealFileSystem`] touches `std`.

use alloc::collections::BTreeMap;

use derive_more::Display;

/// Why a file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Display)]
pub(crate) enum FsError {
    #[display("not found")]
    NotFound,
    #[display("larger than {limit} bytes")]
    TooLarge { limit: u64 },
    #[display("{_0}")]
    Io(String),
}

/// Read access to a virtual file system that overlays several archives and directories.
pub(crate) trait MapFileSystem {
    /// The names (without directory) of the files in `dir` that end with `extension` (with its dot, ignoring case),
    /// from all sources, sorted and without repeats.
    fn list_files(&self, dir: &str, extension: &str) -> Vec<String>;

    /// Reads at most `limit` bytes of `path` from the first source that has it; a larger file is
    /// [`FsError::TooLarge`].
    fn read_file(&self, path: &str, limit: u64) -> Result<Vec<u8>, FsError>;

    /// The archive or directory (a full path) that `path` is read from, if known.
    fn source_of(&self, path: &str) -> Option<String>;
}

/// Files held in memory, for tests.
#[derive(Debug, Default, Clone)]
pub(crate) struct MemoryFileSystem {
    files: BTreeMap<String, (Vec<u8>, Option<String>)>,
}

impl MemoryFileSystem {
    /// Adds a file without a known source.
    pub(crate) fn with_file(self, path: &str, content: impl Into<Vec<u8>>) -> Self {
        self.insert(path, content.into(), None)
    }

    /// Adds a file that is read from `source`.
    pub(crate) fn with_sourced_file(
        self,
        path: &str,
        content: impl Into<Vec<u8>>,
        source: &str,
    ) -> Self {
        self.insert(path, content.into(), Some(source.to_owned()))
    }

    fn insert(mut self, path: &str, content: Vec<u8>, source: Option<String>) -> Self {
        self.files.insert(path.to_owned(), (content, source));
        self
    }
}

impl MapFileSystem for MemoryFileSystem {
    fn list_files(&self, dir: &str, extension: &str) -> Vec<String> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        self.files
            .keys()
            .filter_map(|path| path.strip_prefix(&prefix))
            .filter(|name| !name.contains('/') && has_extension(name, extension))
            .map(str::to_owned)
            .collect()
    }

    fn read_file(&self, path: &str, limit: u64) -> Result<Vec<u8>, FsError> {
        let (content, _) = self.files.get(path).ok_or(FsError::NotFound)?;
        if content.len() as u64 > limit {
            return Err(FsError::TooLarge { limit });
        }
        Ok(content.clone())
    }

    fn source_of(&self, path: &str) -> Option<String> {
        self.files.get(path)?.1.clone()
    }
}

pub(crate) fn has_extension(name: &str, extension: &str) -> bool {
    name.len() > extension.len()
        && name
            .get(name.len() - extension.len()..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(extension))
}

/// A directory of the machine as the root of the virtual file system.
#[derive(Debug, Clone)]
pub(crate) struct RealFileSystem {
    root: String,
}

impl RealFileSystem {
    pub(crate) fn new(root: &str) -> Self {
        Self {
            root: root.trim_end_matches('/').to_owned(),
        }
    }

    fn full_path(&self, path: &str) -> String {
        format!("{}/{}", self.root, path)
    }
}

impl From<std::io::Error> for FsError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound,
            _ => Self::Io(error.to_string()),
        }
    }
}

impl MapFileSystem for RealFileSystem {
    fn list_files(&self, dir: &str, extension: &str) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.full_path(dir)) else {
            return Vec::new();
        };
        let mut names = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_string_lossy().into_owned();
                (entry.metadata().ok()?.is_file() && has_extension(&name, extension))
                    .then_some(name)
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn read_file(&self, path: &str, limit: u64) -> Result<Vec<u8>, FsError> {
        use std::io::Read;

        let mut content = Vec::new();
        std::fs::File::open(self.full_path(path))?
            .take(limit.saturating_add(1))
            .read_to_end(&mut content)?;
        if content.len() as u64 > limit {
            return Err(FsError::TooLarge { limit });
        }
        Ok(content)
    }

    fn source_of(&self, path: &str) -> Option<String> {
        std::path::Path::new(&self.full_path(path))
            .is_file()
            .then(|| self.root.clone())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn memory() -> MemoryFileSystem {
        MemoryFileSystem::default()
            .with_sourced_file("maps/b.bsp", b"bsp".to_vec(), "/base/baseq3/pak00.pk3")
            .with_file("maps/A.BSP", b"bsp".to_vec())
            .with_file("maps/a.aas", b"aas".to_vec())
            .with_file("maps/sub/c.bsp", b"bsp".to_vec())
            .with_file("mapsx/d.bsp", b"bsp".to_vec())
    }

    #[test]
    fn an_io_error_keeps_not_found_apart_from_the_rest() {
        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

        assert_eq!(FsError::from(missing), FsError::NotFound);
        assert!(matches!(FsError::from(denied), FsError::Io(_)));
    }

    #[test]
    fn memory_lists_by_extension_ignoring_case_and_not_below() {
        assert_eq!(memory().list_files("maps", ".bsp"), vec!["A.BSP", "b.bsp"]);
        assert_eq!(memory().list_files("maps/", ".aas"), vec!["a.aas"]);
        assert!(memory().list_files("nowhere", ".bsp").is_empty());
    }

    #[test]
    fn memory_reads_within_limit_and_rejects_larger() {
        let fs = memory();

        assert_eq!(fs.read_file("maps/a.aas", 3).unwrap(), b"aas");
        assert_eq!(
            fs.read_file("maps/a.aas", 2),
            Err(FsError::TooLarge { limit: 2 })
        );
        assert_eq!(fs.read_file("maps/none.bsp", 2), Err(FsError::NotFound));
    }

    #[test]
    fn memory_knows_the_source_only_when_given() {
        let fs = memory();

        assert_eq!(
            fs.source_of("maps/b.bsp").as_deref(),
            Some("/base/baseq3/pak00.pk3")
        );
        assert_eq!(fs.source_of("maps/a.aas"), None);
        assert_eq!(fs.source_of("maps/none.bsp"), None);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn real_lists_reads_and_names_the_root_as_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::create_dir(dir.path().join("scripts")).unwrap();
        std::fs::write(dir.path().join("scripts/b.arena"), b"12345").unwrap();
        std::fs::write(dir.path().join("scripts/a.ARENA"), b"1").unwrap();
        std::fs::write(dir.path().join("scripts/c.txt"), b"1").unwrap();
        std::fs::create_dir(dir.path().join("scripts/d.arena")).unwrap();
        let fs = RealFileSystem::new(root);

        assert_eq!(
            fs.list_files("scripts", ".arena"),
            vec!["a.ARENA", "b.arena"]
        );
        assert_eq!(fs.read_file("scripts/b.arena", 5).unwrap(), b"12345");
        assert_eq!(
            fs.read_file("scripts/b.arena", 4),
            Err(FsError::TooLarge { limit: 4 })
        );
        assert_eq!(fs.read_file("scripts/none", 4), Err(FsError::NotFound));
        assert_eq!(fs.source_of("scripts/b.arena").as_deref(), Some(root));
        assert_eq!(fs.source_of("scripts/none"), None);
    }
}
