//! The snapshot of the installed maps, kept for the life of the process.
//!
//! It lives in a static, so it survives `pyrestart`, which runs the Python start-up again.

use alloc::sync::Arc;

use arc_swap::ArcSwapOption;

use super::scan::Snapshot;

/// The cvar that switches the scan at start-up off with `0`.
pub(crate) const SCAN_CVAR: &str = "qlx_mapsScan";

static SNAPSHOT: ArcSwapOption<Snapshot> = ArcSwapOption::const_empty();

/// Whether a scan has been stored.
pub(crate) fn has_snapshot() -> bool {
    SNAPSHOT.load().is_some()
}

/// The stored snapshot, if there is one.
pub(crate) fn current() -> Option<Arc<Snapshot>> {
    SNAPSHOT.load_full()
}

/// Replaces the stored snapshot; readers that hold the old one keep it until they let go.
pub(crate) fn publish(snapshot: Snapshot) {
    SNAPSHOT.store(Some(Arc::new(snapshot)));
}

/// Whether the start-up scan is switched on by the value of [`SCAN_CVAR`].
pub(crate) fn scan_enabled(cvar_value: &str) -> bool {
    cvar_value.trim() != "0"
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serial_test::serial;

    use super::*;

    #[rstest]
    #[case("1", true)]
    #[case("0", false)]
    #[case(" 0 ", false)]
    #[case("", true)]
    #[case("yes", true)]
    fn cvar_value_decides_whether_the_scan_runs(#[case] value: &str, #[case] enabled: bool) {
        assert_eq!(scan_enabled(value), enabled);
    }

    #[test]
    #[serial]
    fn published_snapshot_replaces_the_old_one_without_disturbing_holders() {
        publish(Snapshot::default());
        let first = current().expect("a snapshot was published");

        let mut other = Snapshot::default();
        other.problems.push("marker".to_owned());
        publish(other);

        assert!(has_snapshot());
        assert!(first.problems.is_empty());
        assert_eq!(
            current().expect("a snapshot").problems,
            vec!["marker".to_owned()]
        );
    }
}
