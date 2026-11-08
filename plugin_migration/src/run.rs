use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::{
    ThreadPoolBuilder,
    iter::{IntoParallelRefIterator, ParallelIterator},
};

use crate::{
    migration::{Migration, migrate},
    rules::{Rule, Warning, default_rules},
    walk::list_files,
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Copied,
    Migrated { edits: usize },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FileReport {
    pub(crate) path: PathBuf,
    pub(crate) outcome: Outcome,
    pub(crate) warnings: Vec<Warning>,
}

struct Job<'a> {
    input: &'a Path,
    output: &'a Path,
    dry_run: bool,
    rules: &'a [Box<dyn Rule>],
}

fn is_python(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "py")
}

fn migrate_python(
    job: &Job<'_>,
    relative: &Path,
) -> anyhow::Result<Option<(Migration, Vec<Warning>)>> {
    let text = match fs::read_to_string(job.input.join(relative)) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::InvalidData => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", relative.display()));
        }
    };
    let result = migrate(&text, job.rules);
    Ok(Some((result.migration, result.warnings)))
}

fn process_file(job: &Job<'_>, relative: &Path) -> anyhow::Result<FileReport> {
    let python = if is_python(relative) {
        migrate_python(job, relative)?
    } else {
        None
    };
    let mut warnings = Vec::new();
    if is_python(relative) && python.is_none() {
        warnings.push(Warning::new(
            0,
            "the file is not valid UTF-8, it was copied unchanged",
        ));
    }

    let target = job.output.join(relative);
    let (outcome, rewritten) = match python {
        Some((Migration::Rewritten { text, edits }, found)) => {
            warnings.extend(found);
            (Outcome::Migrated { edits }, Some(text))
        }
        Some((Migration::Unchanged, found)) => {
            warnings.extend(found);
            (Outcome::Copied, None)
        }
        None => (Outcome::Copied, None),
    };

    if !job.dry_run {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        match rewritten {
            Some(text) => fs::write(&target, text),
            None => fs::copy(job.input.join(relative), &target).map(|_| ()),
        }
        .with_context(|| format!("could not write {}", target.display()))?;
    }

    Ok(FileReport {
        path: relative.to_path_buf(),
        outcome,
        warnings,
    })
}

fn ensure_usable_output(input: &Path, output: &Path) -> anyhow::Result<()> {
    let absolute_output = std::path::absolute(output)
        .with_context(|| format!("could not resolve {}", output.display()))?;
    if absolute_output.starts_with(input) {
        bail!("the output folder must not be the input folder or inside of it");
    }
    let is_empty = match fs::read_dir(&absolute_output) {
        Ok(mut entries) => entries.next().is_none(),
        Err(error) if error.kind() == ErrorKind::NotFound => true,
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", output.display()));
        }
    };
    if !is_empty {
        bail!("the output folder {} is not empty", output.display());
    }
    Ok(())
}

/// Migrates all files below `input` into `output` and returns one report per file, sorted by path.
pub(crate) fn run(
    input: &Path,
    output: &Path,
    dry_run: bool,
    jobs: Option<usize>,
) -> anyhow::Result<Vec<FileReport>> {
    let input = input
        .canonicalize()
        .with_context(|| format!("could not open {}", input.display()))?;
    if !input.is_dir() {
        bail!("{} is not a folder", input.display());
    }
    ensure_usable_output(&input, output)?;

    let files = list_files(&input)
        .with_context(|| format!("could not list the files of {}", input.display()))?;
    let rules = default_rules();
    let job = Job {
        input: &input,
        output,
        dry_run,
        rules: &rules,
    };

    let progress = ProgressBar::new(files.len() as u64).with_style(
        ProgressStyle::with_template("{bar:40} {pos}/{len} {msg}")
            .context("invalid progress template")?,
    );
    let pool = ThreadPoolBuilder::new()
        .num_threads(jobs.unwrap_or(0))
        .build()
        .context("could not start the worker threads")?;
    let reports = pool.install(|| {
        files
            .par_iter()
            .progress_with(progress)
            .map(|relative| process_file(&job, relative))
            .collect::<anyhow::Result<Vec<_>>>()
    })?;
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq;

    use super::{Outcome, run};

    fn write(dir: &std::path::Path, name: &str, content: &str) {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().expect("every test path has a parent"))
            .expect("could not create directory");
        fs::write(path, content).expect("could not write file");
    }

    #[test]
    fn migrates_changed_plugins_and_copies_the_rest() {
        let input = tempfile::tempdir().expect("could not create temporary directory");
        let output = tempfile::tempdir().expect("could not create temporary directory");
        write(input.path(), "changed.py", "import minqlx\n");
        write(input.path(), "same.py", "import os\n");
        write(input.path(), "data/words.txt", "minqlx\n");
        write(input.path(), "broken.py", "def broken(:\n");

        let reports = run(input.path(), output.path(), false, Some(2)).expect("run failed");

        let outcomes = reports
            .iter()
            .map(|report| (report.path.to_string_lossy().into_owned(), &report.outcome))
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes,
            vec![
                ("broken.py".to_string(), &Outcome::Copied),
                ("changed.py".to_string(), &Outcome::Migrated { edits: 1 }),
                ("data/words.txt".to_string(), &Outcome::Copied),
                ("same.py".to_string(), &Outcome::Copied),
            ]
        );
        let read =
            |name: &str| fs::read_to_string(output.path().join(name)).expect("missing output");
        assert_eq!(read("changed.py"), "import shinqlx\n");
        assert_eq!(read("same.py"), "import os\n");
        assert_eq!(read("data/words.txt"), "minqlx\n");
        assert_eq!(read("broken.py"), "def broken(:\n");
    }

    #[test]
    fn dry_run_writes_nothing() {
        let input = tempfile::tempdir().expect("could not create temporary directory");
        let output = tempfile::tempdir().expect("could not create temporary directory");
        write(input.path(), "changed.py", "import minqlx\n");

        let reports = run(input.path(), output.path(), true, Some(1)).expect("run failed");

        assert_eq!(reports.len(), 1);
        assert!(!output.path().join("changed.py").exists());
    }

    #[test]
    fn refuses_a_non_empty_output_folder() {
        let input = tempfile::tempdir().expect("could not create temporary directory");
        let output = tempfile::tempdir().expect("could not create temporary directory");
        write(output.path(), "existing.py", "");

        assert!(run(input.path(), output.path(), false, Some(1)).is_err());
    }

    #[test]
    fn refuses_an_output_folder_inside_the_input_folder() {
        let input = tempfile::tempdir().expect("could not create temporary directory");

        assert!(run(input.path(), &input.path().join("out"), false, Some(1)).is_err());
    }
}
