#![allow(missing_docs)]

use git2::{Repository, StatusOptions, StatusShow};

const COMMIT_ID_SHORT_HASH_LENGTH: usize = 8;

fn main() {
    let python_config = pyo3_build_config::get();

    if let Some(lib_dir) = &python_config.lib_dir {
        println!("cargo::rustc-link-search={lib_dir}");
    }

    if let Some(library) = &python_config.lib_name {
        println!("cargo::rustc-link-lib={library}");
    }

    let shinqlx_version = gather_shinqlx_version();
    println!("cargo::rustc-env=SHINQLX_VERSION={shinqlx_version}");
}

fn gather_shinqlx_version() -> String {
    let pkg_version = env!("CARGO_PKG_VERSION");
    match shinqlx_version_suffix() {
        None => pkg_version.into(),
        Some(suffix) => format!("{pkg_version}+{suffix}"),
    }
}

fn shinqlx_version_suffix() -> Option<String> {
    let repository = Repository::discover(env!("CARGO_MANIFEST_DIR")).ok()?;

    if let Some(workdir) = repository.workdir() {
        println!("cargo::rerun-if-changed={}", workdir.display());
    }

    if repository
        .statuses(Some(
            StatusOptions::default()
                .show(StatusShow::IndexAndWorkdir)
                .include_untracked(false)
                .include_ignored(false)
                .include_unmodified(false)
                .exclude_submodules(false),
        ))
        .ok()?
        .iter()
        .any(|status| ![git2::Status::CURRENT, git2::Status::IGNORED].contains(&status.status()))
    {
        return Some("modified".into());
    }

    let head_commit = repository.head().ok()?.peel_to_commit().ok()?;
    let head_commit_id_str =
        head_commit.id().to_string()[..COMMIT_ID_SHORT_HASH_LENGTH].to_string();
    Some(format!("dev{head_commit_id_str}"))
}
