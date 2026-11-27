//! The directory of the recordings: `YYYYMMDD_HHmmSS_mapname.dm_sqx` in UTC, so that names sort by
//! age, each with its sidecar. Only files named like this are ever deleted.

use alloc::{format, string::String, vec::Vec};
use core::time::Duration;
use std::{
    fs::{self, File, Metadata},
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use derive_more::Display;

use super::sidecar;

const EXTENSION: &str = ".dm_sqx";
const HIGHEST_SUFFIX: u32 = 99;
/// A recording that is being written changes more often than this and is left alone.
const IN_USE: Duration = Duration::from_secs(60);

/// How long recordings are kept; `None` sets no limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// The age of a recording nobody asked for, counted from its end.
    pub max_age: Option<Duration>,
    /// The age of a requested recording, counted from its end.
    pub max_requested_age: Option<Duration>,
    /// The size of all recordings together.
    pub max_total_bytes: Option<u64>,
    /// The space to leave on the disk.
    pub min_free_bytes: Option<u64>,
}

/// Why a recording was deleted.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub enum DeletionCause {
    /// It was older than allowed.
    #[display("expired")]
    Expired,
    /// All recordings together were too large.
    #[display("over the size limit")]
    Quota,
    /// The disk ran out of space.
    #[display("the disk is running full")]
    DiskSpace,
}

/// What enforcing the limits did.
#[derive(Debug, Default)]
pub struct Sweep {
    /// The deleted recordings.
    pub deleted: Vec<Deleted>,
    /// Files that should have been deleted but could not.
    pub failed: Vec<(PathBuf, io::Error)>,
    /// Whether the disk has the space to be left free.
    pub has_room: bool,
}

/// A recording that is gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deleted {
    /// The file of the recording.
    pub path: PathBuf,
    /// Why it was deleted.
    pub cause: DeletionCause,
    /// Whether someone had asked to keep it.
    pub requested: bool,
}

#[derive(Debug)]
struct Stored {
    path: PathBuf,
    bytes: u64,
    age: Duration,
    requested: bool,
}

/// Creates the file of a new recording, along with the directory, never overwriting anything.
pub fn create_recording(
    directory: &Path,
    started: SystemTime,
    map: &str,
) -> io::Result<(File, PathBuf)> {
    fs::create_dir_all(directory)?;
    let stem = stem(started, map);
    let mut suffix = 1;
    loop {
        let name = match suffix {
            1 => format!("{stem}{EXTENSION}"),
            _ => format!("{stem}_{suffix}{EXTENSION}"),
        };
        let path = directory.join(name);
        match File::create_new(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error)
                if error.kind() == io::ErrorKind::AlreadyExists && suffix < HIGHEST_SUFFIX =>
            {
                suffix += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Deletes expired recordings, then the oldest ones until the size limit holds and the disk has room,
/// requested ones only after all others.
pub fn sweep(
    directory: &Path,
    limits: &Limits,
    now: SystemTime,
    free_space: impl Fn() -> io::Result<u64>,
) -> io::Result<Sweep> {
    let recordings = recordings(directory, now)?;
    let mut total: u64 = recordings.iter().map(|recording| recording.bytes).sum();
    let mut outcome = Sweep::default();
    let is_short_of_space = || -> io::Result<bool> {
        match limits.min_free_bytes {
            Some(minimum) => Ok(free_space()? < minimum),
            None => Ok(false),
        }
    };

    let (expired, mut kept): (Vec<_>, Vec<_>) = recordings
        .into_iter()
        .filter(|recording| recording.age >= IN_USE)
        .partition(|recording| {
            let limit = if recording.requested {
                limits.max_requested_age
            } else {
                limits.max_age
            };
            limit.is_some_and(|age| recording.age > age)
        });
    for recording in expired {
        total -= outcome.delete(recording, DeletionCause::Expired);
    }
    kept.sort_by_key(|recording| recording.requested);
    kept.reverse();
    while !kept.is_empty() {
        let cause = if limits.max_total_bytes.is_some_and(|limit| total > limit) {
            DeletionCause::Quota
        } else if is_short_of_space()? {
            DeletionCause::DiskSpace
        } else {
            break;
        };
        total -= kept.pop().map_or(0, |oldest| outcome.delete(oldest, cause));
    }
    outcome.delete_orphaned_sidecars(directory, now)?;
    outcome.has_room = !is_short_of_space()?;
    Ok(outcome)
}

impl Sweep {
    /// Returns the bytes that are gone.
    fn delete(&mut self, recording: Stored, cause: DeletionCause) -> u64 {
        match fs::remove_file(&recording.path) {
            Ok(()) => {
                self.remove_sidecar(&recording.path);
                self.deleted.push(Deleted {
                    path: recording.path,
                    cause,
                    requested: recording.requested,
                });
                recording.bytes
            }
            Err(error) => {
                self.failed.push((recording.path, error));
                0
            }
        }
    }

    fn remove_sidecar(&mut self, recording: &Path) {
        let path = sidecar::path_of(recording);
        match fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                self.failed.push((path, error));
            }
            _ => {}
        }
    }

    /// Removes the sidecars of recordings that someone else deleted.
    fn delete_orphaned_sidecars(&mut self, directory: &Path, now: SystemTime) -> io::Result<()> {
        for (path, metadata) in files(directory)? {
            let recording = path.with_extension(&EXTENSION[1..]);
            let is_orphan = path == sidecar::path_of(&recording)
                && recording
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(is_recording_name)
                && !recording.exists()
                && age(&metadata, now)? >= IN_USE
                && sidecar::read(&recording).is_ok_and(|content| content.is_some());
            if is_orphan {
                self.remove_sidecar(&recording);
            }
        }
        Ok(())
    }
}

/// The regular files of a directory; links are not followed.
fn files(directory: &Path) -> io::Result<Vec<(PathBuf, Metadata)>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        // A file may vanish while the directory is read.
        if let Ok(metadata) = entry.metadata()
            && metadata.is_file()
        {
            files.push((entry.path(), metadata));
        }
    }
    Ok(files)
}

fn age(metadata: &Metadata, now: SystemTime) -> io::Result<Duration> {
    Ok(now.duration_since(metadata.modified()?).unwrap_or_default())
}

/// The recording files of a directory, the oldest first.
pub(super) fn recording_paths(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths: Vec<_> = recording_files(directory)?.map(|(path, _)| path).collect();
    paths.sort();
    Ok(paths)
}

fn recording_files(directory: &Path) -> io::Result<impl Iterator<Item = (PathBuf, Metadata)>> {
    Ok(files(directory)?.into_iter().filter(|(path, _)| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_recording_name)
    }))
}

/// The recordings of a directory, the oldest first.
fn recordings(directory: &Path, now: SystemTime) -> io::Result<Vec<Stored>> {
    let mut recordings = Vec::new();
    for (path, metadata) in recording_files(directory)? {
        // An unreadable sidecar may hold requests.
        let requested = sidecar::read(&path)
            .map_or(true, |content| content.is_some_and(|it| it.is_requested()));
        recordings.push(Stored {
            bytes: metadata.len(),
            age: age(&metadata, now)?,
            requested,
            path,
        });
    }
    recordings.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(recordings)
}

fn stem(started: SystemTime, map: &str) -> String {
    let map: String = map
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let time = DateTime::<Utc>::from(started).format("%Y%m%d_%H%M%S");
    format!("{time}_{map}")
}

pub(super) fn is_recording_name(name: &str) -> bool {
    const TIME_PATTERN: &[u8] = b"00000000_000000_";

    let Some(stem) = name.strip_suffix(EXTENSION) else {
        return false;
    };
    stem.len() > TIME_PATTERN.len()
        && stem
            .bytes()
            .zip(TIME_PATTERN)
            .all(|(byte, &pattern)| match pattern {
                b'0' => byte.is_ascii_digit(),
                separator => byte == separator,
            })
        && stem
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[cfg(test)]
mod storage_tests {
    use core::cell::Cell;
    use std::time::UNIX_EPOCH;

    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::{
        super::sidecar::{Request, Sidecar},
        *,
    };

    const HOUR: Duration = Duration::from_secs(3600);

    struct Directory(PathBuf);

    impl Directory {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("shinqlx_demo_{name}_{}", std::process::id()));
            fs::create_dir_all(&path).expect("created");
            Self(path)
        }

        fn add(&self, name: &str, bytes: usize, age: Duration) {
            let path = self.0.join(name);
            fs::write(&path, vec![0; bytes]).expect("written");
            self.age(name, age);
        }

        fn age(&self, name: &str, age: Duration) {
            File::options()
                .write(true)
                .open(self.0.join(name))
                .and_then(|file| file.set_modified(SystemTime::now() - age))
                .expect("dated");
        }

        fn names(&self) -> Vec<String> {
            let mut names: Vec<_> = fs::read_dir(&self.0)
                .expect("readable")
                .map(|entry| {
                    entry
                        .expect("entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            names.sort();
            names
        }

        fn sweep(&self, limits: Limits, free: impl Fn() -> u64) -> Sweep {
            sweep(&self.0, &limits, SystemTime::now(), || Ok(free())).expect("swept")
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const OLD: &str = "20260101_100000_campgrounds.dm_sqx";
    const MIDDLE: &str = "20260102_100000_bloodrun.dm_sqx";
    const NEW: &str = "20260103_100000_aerowalk.dm_sqx";

    fn three(name: &str) -> Directory {
        let directory = Directory::new(name);
        directory.add(OLD, 100, 30 * HOUR);
        directory.add(MIDDLE, 100, 20 * HOUR);
        directory.add(NEW, 100, 10 * HOUR);
        directory
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn new_recordings_are_named_after_start_and_map() {
        let directory = Directory::new("names");
        let started = UNIX_EPOCH + Duration::from_secs(1_795_728_623);

        let names = [0; 3].map(|_| {
            let (_, path) = create_recording(&directory.0, started, "../my map").expect("created");
            path.file_name()
                .expect("a file")
                .to_string_lossy()
                .into_owned()
        });

        assert_eq!(
            names,
            [
                "20261126_213023____my_map.dm_sqx",
                "20261126_213023____my_map_2.dm_sqx",
                "20261126_213023____my_map_3.dm_sqx",
            ]
        );
        assert!(names.iter().all(|name| is_recording_name(name)));
    }

    #[rstest]
    #[case::other_extension("20260101_100000_campgrounds.dm_91")]
    #[case::no_time("campgrounds.dm_sqx")]
    #[case::short_time("2026011_100000_campgrounds.dm_sqx")]
    #[case::no_map("20260101_100000_.dm_sqx")]
    #[case::renamed("20260101_100000_campgrounds (keep).dm_sqx")]
    fn other_names_are_no_recordings(#[case] name: &str) {
        assert!(!is_recording_name(name));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn without_limits_everything_stays() {
        let directory = three("unlimited");

        let outcome = directory.sweep(Limits::default(), || 0);

        assert!(outcome.has_room);
        assert_eq!(directory.names(), [OLD, MIDDLE, NEW]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn expired_recordings_are_deleted_but_no_other_files() {
        let directory = three("expired");
        directory.add("notes.txt", 1, 100 * HOUR);

        let outcome = directory.sweep(
            Limits {
                max_age: Some(15 * HOUR),
                ..Limits::default()
            },
            || 0,
        );

        assert_eq!(outcome.deleted.len(), 2);
        assert_eq!(directory.names(), [NEW, "notes.txt"]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn the_oldest_make_room_under_the_size_limit() {
        let directory = three("quota");

        directory.sweep(
            Limits {
                max_total_bytes: Some(200),
                ..Limits::default()
            },
            || 0,
        );

        assert_eq!(directory.names(), [MIDDLE, NEW]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn the_oldest_are_deleted_until_the_disk_has_room() {
        let directory = three("space");
        let free = Cell::new(0);

        let outcome = directory.sweep(
            Limits {
                min_free_bytes: Some(2),
                ..Limits::default()
            },
            || free.replace(free.get() + 1),
        );

        assert!(outcome.has_room);
        assert_eq!(directory.names(), [NEW]);
    }

    fn request(directory: &Directory, name: &str) {
        let sidecar = Sidecar {
            map: "map".into(),
            requests: vec![Request {
                requested_by: Some(1),
                reason: String::new(),
                time: 0,
            }],
            ..Sidecar::default()
        };
        sidecar::write(&directory.0.join(name), &sidecar).expect("written");
    }

    const OLD_SIDECAR: &str = "20260101_100000_campgrounds.json";

    #[test]
    #[cfg_attr(miri, ignore)]
    fn requested_recordings_expire_later() {
        let directory = three("requested_age");
        request(&directory, OLD);

        let limits = |days| Limits {
            max_age: Some(15 * HOUR),
            max_requested_age: Some(days * HOUR),
            ..Limits::default()
        };
        directory.sweep(limits(40), || 0);
        assert_eq!(directory.names(), [OLD, OLD_SIDECAR, NEW]);

        directory.sweep(limits(25), || 0);
        assert_eq!(directory.names(), [NEW]);
    }

    #[rstest]
    #[case::one_too_many(200, &[OLD, OLD_SIDECAR, NEW])]
    #[case::only_room_for_one(100, &[OLD, OLD_SIDECAR])]
    #[case::no_room(50, &[])]
    #[cfg_attr(miri, ignore)]
    fn requested_recordings_are_the_last_to_make_room(
        #[case] max_total_bytes: u64,
        #[case] left: &[&str],
    ) {
        let directory = three(&format!("requested_quota_{max_total_bytes}"));
        request(&directory, OLD);

        let outcome = directory.sweep(
            Limits {
                max_total_bytes: Some(max_total_bytes),
                ..Limits::default()
            },
            || 0,
        );

        assert_eq!(directory.names(), left);
        assert_eq!(
            outcome.deleted.iter().any(|deleted| deleted.requested),
            left.is_empty()
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_unreadable_sidecar_counts_as_a_request() {
        let directory = three("unreadable");
        directory.add(OLD_SIDECAR, 1, 30 * HOUR);

        directory.sweep(
            Limits {
                max_total_bytes: Some(100),
                ..Limits::default()
            },
            || 0,
        );

        assert_eq!(directory.names(), [OLD, OLD_SIDECAR]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sidecars_without_recording_are_removed_but_no_other_json_files() {
        let directory = Directory::new("orphans");
        request(&directory, OLD);
        directory.age(OLD_SIDECAR, HOUR);
        directory.add("20260102_100000_bloodrun.json", 1, HOUR);
        directory.add("settings.json", 1, HOUR);

        directory.sweep(Limits::default(), || 0);

        assert_eq!(
            directory.names(),
            ["20260102_100000_bloodrun.json", "settings.json"]
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn recordings_being_written_are_left_alone() {
        let directory = Directory::new("in_use");
        directory.add(NEW, 100, Duration::ZERO);

        let outcome = directory.sweep(
            Limits {
                max_total_bytes: Some(1),
                min_free_bytes: Some(1),
                ..Limits::default()
            },
            || 0,
        );

        assert!(!outcome.has_room);
        assert_eq!(directory.names(), [NEW]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_missing_directory_only_needs_room() {
        let directory = Directory::new("missing");

        let outcome = sweep(
            &directory.0.join("not there"),
            &Limits::default(),
            SystemTime::now(),
            || Ok(0),
        )
        .expect("swept");

        assert!(outcome.has_room);
    }
}
