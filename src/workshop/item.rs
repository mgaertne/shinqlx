//! The files of the installed workshop items and of the lists that name them.

use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use super::manifest;

const CONTENT_DIR: &str = "steamapps/workshop/content/282440";
const MANIFEST_FILE: &str = "appworkshop_282440.acf";

/// A `.pk3` of an item with the maps it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pk3Maps {
    pub(crate) name: String,
    pub(crate) maps: Vec<String>,
}

/// What a purge removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PurgeReport {
    pub(crate) removed_directory: bool,
    pub(crate) manifest_blocks: usize,
}

/// The files that hold the list of a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListFiles {
    /// The file the engine reads: the first of the override, the home path and the base path that exists.
    pub(crate) read: Option<PathBuf>,
    /// The file the plugin writes: the override, otherwise the one in the home path.
    pub(crate) write: PathBuf,
}

/// Finds the list of a server. A non-empty `override_path` is the only file; otherwise the list in the home path wins
/// over the one in the base path, as it does for the engine, and is the one that gets written.
pub(crate) fn locate_list(
    override_path: &str,
    homepath: &Path,
    basepath: &Path,
    gamedir: &str,
    file_name: &str,
) -> ListFiles {
    if !override_path.is_empty() {
        let path = PathBuf::from(override_path);
        return ListFiles {
            read: path.is_file().then(|| path.clone()),
            write: path,
        };
    }

    let home = homepath.join(gamedir).join(file_name);
    let base = basepath.join(gamedir).join(file_name);
    let read = [&home, &base]
        .into_iter()
        .find(|path| path.is_file())
        .cloned();
    ListFiles { read, write: home }
}

/// The directory of the workshop items below the first of the install directories that has one.
pub(crate) fn content_root(install_dirs: &[&Path]) -> Option<PathBuf> {
    install_dirs
        .iter()
        .map(|dir| dir.join(CONTENT_DIR))
        .find(|root| root.is_dir())
}

fn item_dir(root: &Path, id: u64) -> PathBuf {
    root.join(id.to_string())
}

fn pk3_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pk3"))
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

/// The number of `.pk3` files of the item, 0 if it is not installed.
pub(crate) fn pk3_count(root: &Path, id: u64) -> usize {
    pk3_files(&item_dir(root, id)).map_or(0, |files| files.len())
}

fn maps_of(pk3: &Path) -> io::Result<Vec<String>> {
    let archive = zip::ZipArchive::new(File::open(pk3)?).map_err(io::Error::other)?;
    let mut maps = archive
        .file_names()
        .filter_map(Result::ok)
        .filter_map(|name| {
            let name = name.to_ascii_lowercase();
            let stem = name.strip_prefix("maps/")?.strip_suffix(".bsp")?;
            (!stem.contains('/')).then(|| stem.to_owned())
        })
        .collect::<Vec<_>>();
    maps.sort();
    Ok(maps)
}

/// The `.pk3` files of the installed item with their maps.
pub(crate) fn pk3_maps(root: &Path, id: u64) -> io::Result<Vec<Pk3Maps>> {
    pk3_files(&item_dir(root, id))?
        .into_iter()
        .map(|path| {
            Ok(Pk3Maps {
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                maps: maps_of(&path)?,
            })
        })
        .collect()
}

/// The comment of a list line: `a.pk3 (map1, map2), b.pk3 (map3)`.
pub(crate) fn describe(pk3s: &[Pk3Maps]) -> String {
    pk3s.iter()
        .map(|pk3| format!("{} ({})", pk3.name, pk3.maps.join(", ")))
        .collect::<Vec<_>>()
        .join(", ")
}

fn manifest_path(root: &Path) -> Option<PathBuf> {
    root.ancestors()
        .nth(2)
        .map(|workshop| workshop.join(MANIFEST_FILE))
}

/// Deletes the directory of the item and its blocks in the manifest of Steam.
///
/// The root must be a workshop content directory of the game, so that no other directory can be removed this way.
pub(crate) fn purge(root: &Path, id: u64) -> io::Result<PurgeReport> {
    if !root.ends_with("workshop/content/282440") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is no workshop content directory", root.display()),
        ));
    }

    let dir = item_dir(root, id);
    let removed_directory = dir.is_dir();
    if removed_directory {
        fs::remove_dir_all(&dir)?;
    }

    let mut manifest_blocks = 0;
    if let Some(path) = manifest_path(root).filter(|path| path.is_file()) {
        let (text, removed) = manifest::without_item(&fs::read_to_string(&path)?, id);
        if removed > 0 {
            let temporary = path.with_extension("acf.tmp");
            fs::write(&temporary, text)?;
            fs::rename(&temporary, &path)?;
        }
        manifest_blocks = removed;
    }

    Ok(PurgeReport {
        removed_directory,
        manifest_blocks,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use pretty_assertions::assert_eq;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    fn write_pk3(path: &Path, names: &[&str]) {
        let mut writer = ZipWriter::new(File::create(path).expect("pk3 can be created"));
        for name in names {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .expect("entry can start");
            writer.write_all(b"x").expect("entry can be written");
        }
        writer.finish().expect("pk3 can be finished");
    }

    fn layout() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join(CONTENT_DIR);
        fs::create_dir_all(root.join("42")).expect("item dir");
        write_pk3(
            &root.join("42/Second.pk3"),
            &[
                "maps/Beta.bsp",
                "maps/alpha.bsp",
                "maps/sub/x.bsp",
                "scripts/a.arena",
            ],
        );
        write_pk3(&root.join("42/first.pk3"), &["maps/gamma.bsp"]);
        fs::write(root.join("42/levelshot.jpg"), b"x").expect("other file");
        fs::write(
            dir.path().join("steamapps/workshop").join(MANIFEST_FILE),
            "\"AppWorkshop\"\n{\n\t\"42\"\n\t{\n\t\t\"size\"\t\"1\"\n\t}\n\t\"43\"\n\t{\n\t}\n}\n",
        )
        .expect("manifest");
        (dir, root)
    }

    #[test]
    fn lists_the_pk3_files_with_their_maps_sorted() {
        let (_dir, root) = layout();

        let pk3s = pk3_maps(&root, 42).expect("readable");

        assert_eq!(
            pk3s,
            vec![
                Pk3Maps {
                    name: "Second.pk3".into(),
                    maps: vec!["alpha".into(), "beta".into()]
                },
                Pk3Maps {
                    name: "first.pk3".into(),
                    maps: vec!["gamma".into()]
                },
            ]
        );
        assert_eq!(
            describe(&pk3s),
            "Second.pk3 (alpha, beta), first.pk3 (gamma)"
        );
        assert_eq!(pk3_count(&root, 42), 2);
        assert_eq!(pk3_count(&root, 7), 0);
    }

    #[test]
    fn a_pk3_that_is_no_archive_is_an_error() {
        let (_dir, root) = layout();
        fs::write(root.join("42/broken.pk3"), b"no zip").expect("file");

        assert!(pk3_maps(&root, 42).is_err());
    }

    #[test]
    fn purge_removes_the_directory_and_the_manifest_blocks_of_the_item_only() {
        let (dir, root) = layout();

        let report = purge(&root, 42).expect("purged");

        assert_eq!(
            report,
            PurgeReport {
                removed_directory: true,
                manifest_blocks: 1
            }
        );
        assert!(!root.join("42").exists());
        let manifest =
            fs::read_to_string(dir.path().join("steamapps/workshop").join(MANIFEST_FILE))
                .expect("manifest");
        assert!(!manifest.contains("\"42\""));
        assert!(manifest.contains("\"43\""));
        assert_eq!(
            purge(&root, 42).expect("again"),
            PurgeReport {
                removed_directory: false,
                manifest_blocks: 0
            }
        );
    }

    #[test]
    fn purge_refuses_a_directory_that_is_no_workshop_content_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir_all(dir.path().join("precious/42")).expect("dirs");

        let result = purge(&dir.path().join("precious"), 42);

        assert!(result.is_err_and(|error| error.kind() == io::ErrorKind::InvalidInput));
        assert!(dir.path().join("precious/42").is_dir());
    }

    #[test]
    fn content_root_is_the_first_install_directory_that_has_one() {
        let (dir, root) = layout();
        let other = tempfile::tempdir().expect("temp dir");

        assert_eq!(content_root(&[other.path(), dir.path()]), Some(root));
        assert_eq!(content_root(&[other.path()]), None);
    }

    #[test]
    fn the_list_of_the_home_path_wins_and_is_the_one_to_write() {
        let home = tempfile::tempdir().expect("temp dir");
        let base = tempfile::tempdir().expect("temp dir");
        fs::create_dir_all(home.path().join("baseq3")).expect("dirs");
        fs::create_dir_all(base.path().join("baseq3")).expect("dirs");
        fs::write(base.path().join("baseq3/workshop.txt"), "1\n").expect("file");

        let only_base = locate_list("", home.path(), base.path(), "baseq3", "workshop.txt");
        assert_eq!(
            only_base.read,
            Some(base.path().join("baseq3/workshop.txt"))
        );
        assert_eq!(only_base.write, home.path().join("baseq3/workshop.txt"));

        fs::write(home.path().join("baseq3/workshop.txt"), "2\n").expect("file");
        let both = locate_list("", home.path(), base.path(), "baseq3", "workshop.txt");
        assert_eq!(both.read, Some(home.path().join("baseq3/workshop.txt")));
    }

    #[test]
    fn an_override_is_the_only_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let shared = dir.path().join("shared.txt");

        let missing = locate_list(
            &shared.to_string_lossy(),
            dir.path(),
            dir.path(),
            "baseq3",
            "workshop.txt",
        );
        assert_eq!(
            missing,
            ListFiles {
                read: None,
                write: shared.clone()
            }
        );

        fs::write(&shared, "1\n").expect("file");
        let present = locate_list(
            &shared.to_string_lossy(),
            dir.path(),
            dir.path(),
            "baseq3",
            "workshop.txt",
        );
        assert_eq!(present.read, Some(shared));
    }
}
