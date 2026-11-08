use std::{
    fs, io,
    path::{Path, PathBuf},
};

const SKIPPED_DIRECTORIES: [&str; 3] = [".git", "__pycache__", ".idea"];

/// All regular files below `root` as paths relative to `root`, sorted. Symbolic links are skipped.
pub(crate) fn list_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect(root, Path::new(""), &mut files)?;
    files.sort();
    Ok(files)
}

fn collect(root: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let entry_path = relative.join(entry.file_name());
        if file_type.is_dir() {
            if !SKIPPED_DIRECTORIES
                .iter()
                .any(|name| entry.file_name() == *name)
            {
                collect(root, &entry_path, files)?;
            }
        } else if file_type.is_file() {
            files.push(entry_path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use pretty_assertions::assert_eq;

    use super::list_files;

    #[test]
    fn lists_files_recursively_and_skips_caches() {
        let dir = tempfile::tempdir().expect("could not create temporary directory");
        fs::create_dir_all(dir.path().join("pkg/__pycache__")).expect("could not create directory");
        fs::create_dir_all(dir.path().join(".git")).expect("could not create directory");
        for name in ["b.py", "pkg/a.py", "pkg/__pycache__/a.pyc", ".git/config"] {
            fs::write(dir.path().join(name), "").expect("could not write file");
        }

        let files = list_files(dir.path()).expect("could not list files");
        assert_eq!(
            files,
            vec![PathBuf::from("b.py"), PathBuf::from("pkg/a.py")]
        );
    }
}
