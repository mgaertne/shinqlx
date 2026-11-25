//! Runs the scan of the installed maps in a thread of its own and publishes the result.
//!
//! The engine's search path list is frozen once, on the main thread, with [`configure`]. Every later scan, from
//! `late_init` or from a reload, reads the archives and directories of that list itself and needs nothing from the
//! engine. A workshop item that the engine mounts later is therefore seen after the next server start.

use alloc::sync::Arc;
use core::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use std::{thread::JoinHandle, time::Instant};

use arc_swap::ArcSwapOption;
use derive_more::Display;
use log::{error, info, warn};

use super::{
    archive::ArchiveFileSystem,
    scan::{Snapshot, scan, scan_parallel},
    sources::SourceIndex,
    store,
};

const MAX_WORKERS: usize = 4;
const PROBLEMS_LOGGED: usize = 50;
const IDLE_POLL: Duration = Duration::from_millis(10);
#[cfg(target_os = "linux")]
const LOWEST_PRIORITY: libc::c_int = 19;

#[derive(Debug)]
struct ScanSetup {
    index: SourceIndex,
    homepath: String,
}

static SETUP: ArcSwapOption<ScanSetup> = ArcSwapOption::const_empty();
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Why a scan could not be started.
#[derive(Debug, Display, PartialEq, Eq)]
pub(crate) enum StartError {
    /// [`configure`] has not been called.
    #[display("the search paths of the engine are not known yet")]
    NotConfigured,
    /// The previous scan has not finished.
    #[display("a scan is already running")]
    AlreadyRunning,
    /// The operating system refused the thread.
    #[display("cannot start the scan thread: {_0}")]
    Thread(String),
}

/// Remembers the frozen search path list and the home path for all later scans.
pub(crate) fn configure(index: SourceIndex, homepath: &str) {
    SETUP.store(Some(Arc::new(ScanSetup {
        index,
        homepath: homepath.to_owned(),
    })));
}

/// Whether [`configure`] has been called.
pub(crate) fn is_configured() -> bool {
    SETUP.load().is_some()
}

/// Whether a scan is running.
pub(crate) fn is_running() -> bool {
    RUNNING.load(Ordering::Acquire)
}

fn worker_count() -> usize {
    std::thread::available_parallelism().map_or(1, |cores| (cores.get() / 2).clamp(1, MAX_WORKERS))
}

/// Gives the calling thread the lowest priority, so the scan only uses the CPU time the game leaves over. This
/// matters on a server with a single core.
#[cfg(target_os = "linux")]
fn lower_priority_of_this_thread() {
    // SAFETY: `gettid` takes no arguments and cannot fail.
    let thread_id = unsafe { libc::syscall(libc::SYS_gettid) };
    let Ok(thread_id) = libc::id_t::try_from(thread_id) else {
        return;
    };
    // SAFETY: plain system call; a thread id selects exactly that thread on Linux, and a failure is harmless.
    unsafe { libc::setpriority(libc::PRIO_PROCESS, thread_id, LOWEST_PRIORITY) };
}

#[cfg(not(target_os = "linux"))]
fn lower_priority_of_this_thread() {}

fn run(setup: &ScanSetup) -> Snapshot {
    let workers = worker_count();
    let reader = || ArchiveFileSystem::new(setup.index.clone());
    if workers == 1 {
        return scan(&reader(), &setup.homepath);
    }
    match rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .start_handler(|_| lower_priority_of_this_thread())
        .build()
    {
        Ok(pool) => scan_parallel(&reader(), &setup.homepath, &pool, reader),
        Err(error) => {
            warn!(target: "shinqlx", func = "run"; "No pool of {workers} threads ({error}), scanning in this thread");
            scan(&reader(), &setup.homepath)
        }
    }
}

struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::Release);
    }
}

/// Starts a scan in a thread of its own; its result replaces the stored snapshot when it is complete.
pub(crate) fn start_scan() -> Result<JoinHandle<()>, StartError> {
    let setup = SETUP.load_full().ok_or(StartError::NotConfigured)?;
    RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| StartError::AlreadyRunning)?;
    std::thread::Builder::new()
        .name("shinqlx-maps".to_owned())
        .spawn(move || {
            let _running = RunningGuard;
            lower_priority_of_this_thread();
            let started = Instant::now();
            let snapshot = run(&setup);
            info!(
                target: "shinqlx", func = "start_scan";
                "Scanned {} maps and {} factories in {} ms",
                snapshot.maps.len(), snapshot.factories.len(), started.elapsed().as_millis()
            );
            for problem in snapshot.problems.iter().take(PROBLEMS_LOGGED) {
                warn!(target: "shinqlx", func = "start_scan"; "{problem}");
            }
            store::publish(snapshot);
        })
        .map_err(|error| {
            RUNNING.store(false, Ordering::Release);
            error!(target: "shinqlx", func = "start_scan"; "Cannot start the scan thread: {error}");
            StartError::Thread(error.to_string())
        })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serial_test::serial;

    use super::*;
    use crate::maps::{bsp::mini_bsp, sources::SearchEntry};

    fn index_with_one_loose_map(root: &std::path::Path) -> SourceIndex {
        std::fs::create_dir_all(root.join("baseq3/maps")).expect("dirs");
        std::fs::write(
            root.join("baseq3/maps/sample.bsp"),
            mini_bsp(r#"{ "classname" "worldspawn" "message" "Sample" }"#),
        )
        .expect("bsp");
        SourceIndex::new(vec![SearchEntry::Directory {
            path: root.to_string_lossy().into_owned(),
            gamedir: "baseq3".into(),
        }])
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore = "starts threads and reads real files")]
    fn a_scan_in_its_thread_publishes_the_snapshot_and_frees_the_slot() {
        let dir = tempfile::tempdir().expect("temp dir");
        configure(index_with_one_loose_map(dir.path()), "/nowhere");

        let handle = start_scan().expect("the scan starts");
        handle.join().expect("the scan thread ends");

        let snapshot = store::current().expect("a snapshot was published");
        assert_eq!(snapshot.maps["sample"].name, "sample");
        assert_eq!(snapshot.maps["sample"].longname, "Sample");
        assert!(!is_running());
        assert!(is_configured());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[cfg_attr(miri, ignore = "makes system calls")]
    fn the_scan_thread_gets_the_lowest_priority_and_the_caller_keeps_its_own() {
        let own = |thread_id: libc::id_t| {
            // SAFETY: plain system call without pointers.
            unsafe { libc::getpriority(libc::PRIO_PROCESS, thread_id) }
        };
        let caller = own(0);

        let lowered = std::thread::spawn(move || {
            lower_priority_of_this_thread();
            own(0)
        })
        .join()
        .expect("the thread ends");

        assert_eq!(lowered, LOWEST_PRIORITY);
        assert_eq!(own(0), caller);
    }

    #[test]
    #[serial]
    fn a_second_scan_is_refused_while_one_is_running() {
        RUNNING.store(true, Ordering::Release);
        configure(SourceIndex::default(), "/nowhere");

        let result = start_scan();
        RUNNING.store(false, Ordering::Release);

        assert_eq!(result.err(), Some(StartError::AlreadyRunning));
    }
}
