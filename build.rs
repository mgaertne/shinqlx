use git2::{Repository, StatusOptions, StatusShow};

const COMMIT_ID_SHORT_HASH_LENGTH: usize = 8;

fn main() {
    let python_config = pyo3_build_config::get();

    if let Some(lib_dir) = &python_config.lib_dir {
        println!("cargo:rustc-link-search={}", lib_dir);
    }

    if let Some(library) = &python_config.lib_name {
        println!("cargo:rustc-link-lib={}", library);
    }

    let shinqlx_version = gather_shinqlx_version();
    println!("cargo:rustc-env=SHINQLX_VERSION={}", shinqlx_version);
}

fn gather_shinqlx_version() -> String {
    match Repository::discover(env!("CARGO_MANIFEST_DIR")) {
        Err(_) => format!(
            "\"v{}-{}\"",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_NAME")
        ),
        Ok(repository) => {
            println!(
                "cargo:rerun-if-changed={}",
                repository.workdir().unwrap().display()
            );

            let modified = {
                let statuses = repository
                    .statuses(Some(
                        StatusOptions::default()
                            .show(StatusShow::IndexAndWorkdir)
                            .include_untracked(false)
                            .include_ignored(false)
                            .include_unmodified(false)
                            .exclude_submodules(false),
                    ))
                    .unwrap();
                statuses.iter().any(|status| {
                    status.status() != git2::Status::CURRENT
                        && status.status() != git2::Status::IGNORED
                })
            };

            if modified {
                format!(
                    "\"v{}-{}-modified\"",
                    env!("CARGO_PKG_VERSION"),
                    env!("CARGO_PKG_NAME")
                )
            } else {
                let head_commit = repository.head().unwrap().peel_to_commit().unwrap();
                let head_commit_id_str =
                    head_commit.id().to_string()[..COMMIT_ID_SHORT_HASH_LENGTH].to_string();
                format!(
                    "\"v{}-{}-dev{}\"",
                    env!("CARGO_PKG_VERSION"),
                    env!("CARGO_PKG_NAME"),
                    head_commit_id_str
                )
            }
        }
    }
}
