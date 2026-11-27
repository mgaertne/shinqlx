//! Enforces the limits of the recordings away from the game thread.

use alloc::ffi::CString;
use core::{
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
};
use std::{io, os::unix::ffi::OsStrExt, path::Path, thread, time::SystemTime};

use super::{
    settings::Settings,
    storage::{Deleted, DeletionCause, Limits, sweep},
};

static SWEEPING: AtomicBool = AtomicBool::new(false);
static SWEEP_DUE: AtomicBool = AtomicBool::new(false);
static HAS_ROOM: AtomicBool = AtomicBool::new(true);

/// Whether the last sweep found the space to be left free.
pub(super) fn has_room() -> bool {
    HAS_ROOM.load(Ordering::Relaxed)
}

/// Asks for a sweep soon, from any thread.
pub(super) fn request_sweep() {
    SWEEP_DUE.store(true, Ordering::Relaxed);
}

pub(super) fn take_sweep_request() -> bool {
    SWEEP_DUE.swap(false, Ordering::Relaxed)
}

/// Starts a sweep in a thread of its own, unless one is running.
pub(super) fn start_sweep(settings: &Settings) {
    if SWEEPING.swap(true, Ordering::AcqRel) {
        return;
    }
    let (directory, limits) = (settings.directory.clone(), settings.limits);
    let spawned = thread::Builder::new()
        .name("shinqlx-demo-sweep".into())
        .spawn(move || {
            run(&directory, &limits);
            SWEEPING.store(false, Ordering::Release);
        });
    if let Err(error) = spawned {
        SWEEPING.store(false, Ordering::Release);
        log::error!(target: "shinqlx", func = "demo"; "Could not clean up recordings: {error}");
    }
}

fn run(directory: &Path, limits: &Limits) {
    let outcome = sweep(directory, limits, SystemTime::now(), || {
        free_space(directory)
    });
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            log::error!(target: "shinqlx", func = "demo"; "Could not clean up {}: {error}", directory.display());
            return;
        }
    };
    for Deleted {
        path,
        cause,
        requested,
    } in &outcome.deleted
    {
        if *requested && *cause != DeletionCause::Expired {
            log::warn!(target: "shinqlx", func = "demo"; "Deleted the requested recording {} ({cause})", path.display());
        } else {
            log::info!(target: "shinqlx", func = "demo"; "Deleted {} ({cause})", path.display());
        }
    }
    for (path, error) in &outcome.failed {
        log::error!(target: "shinqlx", func = "demo"; "Could not delete {}: {error}", path.display());
    }
    if HAS_ROOM.swap(outcome.has_room, Ordering::Relaxed) && !outcome.has_room {
        log::warn!(target: "shinqlx", func = "demo"; "Not enough free disk space to record demos in {}", directory.display());
    }
}

/// The bytes available on the disk of the directory, which does not have to exist yet.
fn free_space(directory: &Path) -> io::Result<u64> {
    let existing = directory
        .ancestors()
        .find(|ancestor| ancestor.exists())
        .unwrap_or(Path::new("."));
    let path = CString::new(existing.as_os_str().as_bytes())?;
    let mut stats = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: the path is NUL-terminated and the buffer has the size of the structure.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a successful statvfs fills the whole structure.
    let stats = unsafe { stats.assume_init() };
    Ok(stats.f_bavail.saturating_mul(stats.f_frsize))
}
