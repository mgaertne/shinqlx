//! Runs the binary the way a user does.

use std::{fs, process::Command};

fn plugin_migration() -> Command {
    Command::new(env!("CARGO_BIN_EXE_plugin_migration"))
}

#[test]
fn migrates_a_folder_and_lists_the_files() {
    let input = tempfile::tempdir().expect("could not create temporary directory");
    let output = tempfile::tempdir().expect("could not create temporary directory");
    fs::write(input.path().join("changed.py"), "import minqlx\n").expect("could not write file");

    let result = plugin_migration()
        .arg(input.path())
        .arg(output.path())
        .output()
        .expect("could not start plugin_migration");

    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("changed.py"));
    assert_eq!(
        fs::read_to_string(output.path().join("changed.py")).expect("missing output"),
        "import shinqlx\n"
    );
}

#[test]
fn a_dry_run_writes_nothing() {
    let input = tempfile::tempdir().expect("could not create temporary directory");
    let output = tempfile::tempdir().expect("could not create temporary directory");
    fs::write(input.path().join("changed.py"), "import minqlx\n").expect("could not write file");

    let result = plugin_migration()
        .args(["--dry-run", "--jobs", "1"])
        .arg(input.path())
        .arg(output.path())
        .output()
        .expect("could not start plugin_migration");

    assert!(result.status.success());
    assert!(!output.path().join("changed.py").exists());
}

#[test]
fn a_missing_input_folder_ends_with_an_error() {
    let output = tempfile::tempdir().expect("could not create temporary directory");

    let result = plugin_migration()
        .arg(output.path().join("missing"))
        .arg(output.path())
        .output()
        .expect("could not start plugin_migration");

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("could not open"));
}
