//! What the server and its plugins know about the recordings. The game thread keeps it up to date,
//! plugins ask it from any thread. Nothing here calls the engine or Python, so that neither can
//! wait for the other while the catalog is locked.

use alloc::{string::String, vec::Vec};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{LazyLock, OnceLock},
    thread::{self, ThreadId},
};

use chrono::NaiveDateTime;
use derive_more::Display;
use parking_lot::{Mutex, MutexGuard};
use ql_sqx::{DemoFileError, PrivateMessages, Stay, without_colors, write_demo_files};

use super::{
    sidecar::{self, PlayerEntry, Request, Sidecar},
    storage::{self, is_recording_name},
};

static CATALOG: LazyLock<Mutex<Catalog>> = LazyLock::new(Mutex::default);
static GAME_THREAD: OnceLock<ThreadId> = OnceLock::new();

pub(crate) fn catalog() -> MutexGuard<'static, Catalog> {
    CATALOG.lock()
}

/// To be called from the game thread, which must not be kept busy.
pub(crate) fn mark_game_thread() {
    GAME_THREAD.get_or_init(|| thread::current().id());
}

pub(crate) fn is_game_thread() -> bool {
    GAME_THREAD.get() == Some(&thread::current().id())
}

/// One recording, as it is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordingInfo {
    /// The file name without its extension, which identifies the recording.
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) sidecar: Sidecar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecorderState {
    Disabled,
    Waiting,
    Recording,
    DiskFull,
}

#[derive(Debug)]
struct Current {
    path: PathBuf,
    sidecar: Sidecar,
}

#[derive(Debug, Default)]
pub(crate) struct Catalog {
    directory: Option<PathBuf>,
    enabled: bool,
    disk_full: bool,
    countdown: bool,
    current: Option<Current>,
    finished: Vec<PathBuf>,
}

impl Catalog {
    pub(crate) fn configure(&mut self, directory: &Path, enabled: bool, has_room: bool) {
        if self.directory.as_deref() != Some(directory) {
            self.directory = Some(directory.into());
        }
        self.enabled = enabled;
        self.disk_full = !has_room;
    }

    pub(crate) fn set_countdown(&mut self, countdown: bool) {
        self.countdown = countdown;
    }

    /// A recording started.
    pub(crate) fn begin(&mut self, path: PathBuf, sidecar: Sidecar) -> io::Result<()> {
        let written = sidecar::write(&path, &sidecar);
        self.current = Some(Current { path, sidecar });
        written
    }

    /// Changes the sidecar of the running recording and writes it if `change` says it did.
    pub(crate) fn update(&mut self, change: impl FnOnce(&mut Sidecar) -> bool) -> io::Result<()> {
        let Some(current) = self.current.as_mut() else {
            return Ok(());
        };
        if !change(&mut current.sidecar) {
            return Ok(());
        }
        sidecar::write(&current.path, &current.sidecar)
    }

    /// The running recording ended; its file may still be written.
    pub(crate) fn end(&mut self, ended: u64) -> io::Result<()> {
        let written = self.update(|sidecar| {
            sidecar.ended = Some(ended);
            true
        });
        self.current = None;
        written
    }

    /// The file of a recording is complete.
    pub(crate) fn finished(&mut self, path: PathBuf) {
        self.finished.push(path);
    }

    /// The recordings whose files were completed since the last call.
    pub(crate) fn take_finished(&mut self) -> Vec<RecordingInfo> {
        core::mem::take(&mut self.finished)
            .into_iter()
            .filter_map(|path| self.describe(path))
            .collect()
    }

    pub(crate) fn status(&self) -> (RecorderState, Option<RecordingInfo>) {
        let current = self
            .current
            .as_ref()
            .and_then(|current| self.describe(current.path.clone()));
        let state = if self.current.is_some() {
            RecorderState::Recording
        } else if !self.enabled {
            RecorderState::Disabled
        } else if self.disk_full {
            RecorderState::DiskFull
        } else {
            RecorderState::Waiting
        };
        (state, current)
    }

    /// All recordings, the oldest first.
    pub(crate) fn recordings(&self) -> io::Result<Vec<RecordingInfo>> {
        let Some(directory) = &self.directory else {
            return Ok(Vec::new());
        };
        Ok(storage::recording_paths(directory)?
            .into_iter()
            .filter_map(|path| self.describe(path))
            .collect())
    }

    pub(crate) fn find(&self, name: &str) -> Option<RecordingInfo> {
        let current = self
            .current
            .as_ref()
            .filter(|current| name_of(&current.path) == name);
        let path = match current {
            Some(current) => current.path.clone(),
            None => {
                let file = format!("{name}.dm_sqx");
                self.directory
                    .as_ref()
                    .filter(|_| is_recording_name(&file))?
                    .join(file)
            }
        };
        self.describe(path)
    }

    /// Asks to keep the recording `name`. Without a name this is the running recording if it holds
    /// a match, otherwise the last one that does, otherwise the running one. A later request of the
    /// same requester replaces the earlier one. `None` if there is no such recording.
    pub(crate) fn request(
        &mut self,
        name: Option<&str>,
        request: Request,
    ) -> io::Result<Option<RecordingInfo>> {
        let target = match name {
            Some(name) => self.find(name),
            None => self.meant_by_default()?,
        };
        let Some(target) = target else {
            return Ok(None);
        };
        let add = |sidecar: &mut Sidecar| {
            sidecar
                .requests
                .retain(|earlier| earlier.requested_by != request.requested_by);
            sidecar.requests.push(request.clone());
            true
        };
        if self
            .current
            .as_ref()
            .is_some_and(|current| current.path == target.path)
        {
            self.update(add)?;
        } else {
            // An unreadable sidecar is not replaced, it may hold other requests.
            let mut sidecar =
                sidecar::read(&target.path)?.unwrap_or_else(|| target.sidecar.clone());
            add(&mut sidecar);
            sidecar::write(&target.path, &sidecar)?;
        }
        Ok(self.describe(target.path))
    }

    fn meant_by_default(&self) -> io::Result<Option<RecordingInfo>> {
        let (_, current) = self.status();
        if let Some(current) = &current
            && (current.sidecar.match_played || self.countdown)
        {
            return Ok(Some(current.clone()));
        }
        let previous = self
            .recordings()?
            .into_iter()
            .rev()
            .find(|recording| recording.sidecar.match_played);
        Ok(previous.or(current))
    }

    /// `None` if the file is gone.
    fn describe(&self, path: PathBuf) -> Option<RecordingInfo> {
        let size = fs::metadata(&path).ok()?.len();
        let name = name_of(&path);
        let sidecar = self
            .current
            .as_ref()
            .filter(|current| current.path == path)
            .map(|current| current.sidecar.clone())
            .or_else(|| sidecar::read(&path).ok().flatten())
            .unwrap_or_else(|| sidecar_from_name(&name));
        Some(RecordingInfo {
            name,
            path,
            size,
            sidecar,
        })
    }
}

fn name_of(recording: &Path) -> String {
    recording
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// What the name `YYYYMMDD_HHmmSS_mapname` tells about a recording without a sidecar.
fn sidecar_from_name(name: &str) -> Sidecar {
    let (started, map) = NaiveDateTime::parse_and_remainder(name, "%Y%m%d_%H%M%S_").map_or(
        (0, name),
        |(time, map)| {
            let seconds = time.and_utc().timestamp();
            (u64::try_from(seconds).unwrap_or_default(), map)
        },
    );
    Sidecar {
        map: map.into(),
        started,
        ..Sidecar::default()
    }
}

/// Why a recording could not be exported.
#[derive(Debug, Display)]
pub(crate) enum ExportFailure {
    #[display("{} exists already", _0.display())]
    Exists(PathBuf),
    #[display("cannot create {}: {_1}", _0.display())]
    Directory(PathBuf, io::Error),
    #[display("cannot remove {}: {_1}", _0.display())]
    Cleanup(PathBuf, io::Error),
    #[display("{_0}")]
    Demo(DemoFileError),
}

/// Writes the demos of the players with these SteamIDs, of everyone without a choice. Either all
/// of them are new files or none is written.
pub(crate) fn export(
    recording: &RecordingInfo,
    directory: &Path,
    private_messages: PrivateMessages,
    steam_ids: Option<&[u64]>,
) -> Result<Vec<(PathBuf, PlayerEntry)>, ExportFailure> {
    fs::create_dir_all(directory)
        .map_err(|error| ExportFailure::Directory(directory.into(), error))?;
    let players = &recording.sidecar.players;
    let demos = write_demo_files(&recording.path, directory, private_messages, |stay| {
        steam_ids.is_none_or(|wanted| {
            listed(players, stay)
                .map(|player| player.steam_id)
                .or(stay.steam_id)
                .is_some_and(|steam_id| wanted.contains(&steam_id))
        })
    })
    .map_err(ExportFailure::Demo)?;
    if let Some(existing) = demos.kept.into_iter().next() {
        for demo in demos.written {
            fs::remove_file(&demo.path)
                .map_err(|error| ExportFailure::Cleanup(demo.path, error))?;
        }
        return Err(ExportFailure::Exists(existing));
    }
    Ok(demos
        .written
        .into_iter()
        .map(|demo| {
            let player = listed(players, &demo.stay)
                .cloned()
                .unwrap_or_else(|| PlayerEntry {
                    steam_id: demo.stay.steam_id.unwrap_or_default(),
                    name: demo.stay.name.unwrap_or_default(),
                    client_id: demo.stay.client,
                });
            (demo.path, player)
        })
        .collect())
}

/// The entry of the sidecar for a stay. The game tells no SteamID for bots, so they are known by
/// client number and name.
fn listed<'a>(players: &'a [PlayerEntry], stay: &Stay) -> Option<&'a PlayerEntry> {
    players.iter().find(|player| {
        player.client_id == stay.client
            && match stay.steam_id {
                Some(steam_id) => player.steam_id == steam_id,
                None => stay.name.as_deref() == Some(&without_colors(&player.name)),
            }
    })
}

#[cfg(test)]
mod catalog_tests {
    use pretty_assertions::assert_eq;
    use ql_sqx::{ArchiveWriter, Event, Frame, PlayerState};
    use rstest::rstest;
    use tempfile::{TempDir, tempdir};

    use super::*;

    const FIRST: &str = "20260101_100000_campgrounds";
    const SECOND: &str = "20260101_110000_bloodrun";
    const RUNNING: &str = "20260101_120000_aerowalk";

    fn path(directory: &TempDir, name: &str) -> PathBuf {
        directory.path().join(format!("{name}.dm_sqx"))
    }

    fn sidecar(match_played: bool) -> Sidecar {
        Sidecar {
            map: "map".into(),
            match_played,
            ..Sidecar::default()
        }
    }

    fn request(requested_by: u64, reason: &str) -> Request {
        Request {
            requested_by: Some(requested_by),
            reason: reason.into(),
            time: 1,
        }
    }

    /// Two finished recordings, the older one with a match, and a running one.
    fn catalog_with(running_has_match: bool) -> (TempDir, Catalog) {
        let directory = tempdir().expect("a directory");
        for (name, match_played) in [(FIRST, true), (SECOND, false)] {
            fs::write(path(&directory, name), b"content").expect("written");
            sidecar::write(&path(&directory, name), &sidecar(match_played)).expect("written");
        }
        fs::write(path(&directory, RUNNING), b"").expect("written");
        let mut catalog = Catalog::default();
        catalog.configure(directory.path(), true, true);
        catalog
            .begin(path(&directory, RUNNING), sidecar(running_has_match))
            .expect("begun");
        (directory, catalog)
    }

    fn requested(catalog: &Catalog) -> Vec<String> {
        catalog
            .recordings()
            .expect("readable")
            .into_iter()
            .filter(|recording| recording.sidecar.is_requested())
            .map(|recording| recording.name)
            .collect()
    }

    #[rstest]
    #[case::running_match(true, false, RUNNING)]
    #[case::countdown(false, true, RUNNING)]
    #[case::warmup_after_a_match(false, false, FIRST)]
    #[cfg_attr(miri, ignore)]
    fn without_a_name_the_match_is_meant(
        #[case] running_has_match: bool,
        #[case] countdown: bool,
        #[case] meant: &str,
    ) {
        let (_directory, mut catalog) = catalog_with(running_has_match);
        catalog.set_countdown(countdown);

        let target = catalog.request(None, request(1, "")).expect("requested");

        assert_eq!(target.map(|target| target.name), Some(meant.into()));
        assert_eq!(requested(&catalog), [meant]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn without_any_match_the_running_recording_is_meant() {
        let (directory, mut catalog) = catalog_with(false);
        fs::remove_file(path(&directory, FIRST)).expect("removed");

        let target = catalog.request(None, request(1, "")).expect("requested");

        assert_eq!(target.map(|target| target.name), Some(RUNNING.into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_request_stays_when_the_running_recording_ends() {
        let (directory, mut catalog) = catalog_with(true);
        catalog
            .request(None, request(1, "check"))
            .expect("requested");
        catalog
            .update(|sidecar| sidecar.note_match())
            .expect("updated");
        catalog.end(42).expect("ended");

        let written = sidecar::read(&path(&directory, RUNNING))
            .expect("readable")
            .expect("a sidecar");

        assert_eq!(written.ended, Some(42));
        assert_eq!(written.requests, [request(1, "check")]);
        assert_eq!(catalog.status(), (RecorderState::Waiting, None));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn each_requester_has_one_request() {
        let (_directory, mut catalog) = catalog_with(false);
        for added in [
            request(1, "first"),
            request(2, "other"),
            request(1, "again"),
        ] {
            catalog.request(Some(SECOND), added).expect("requested");
        }

        let requests = catalog.find(SECOND).expect("found").sidecar.requests;

        assert_eq!(requests, [request(2, "other"), request(1, "again")]);
    }

    #[rstest]
    #[case::unknown("20260101_130000_unknown")]
    #[case::outside("../20260101_100000_campgrounds")]
    #[case::no_recording("notes")]
    #[cfg_attr(miri, ignore)]
    fn only_recordings_of_the_directory_are_found(#[case] name: &str) {
        let (_directory, mut catalog) = catalog_with(false);

        assert_eq!(catalog.find(name), None);
        assert_eq!(
            catalog
                .request(Some(name), request(1, ""))
                .expect("answered"),
            None
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn a_recording_without_sidecar_is_described_by_its_name() {
        let (directory, catalog) = catalog_with(false);
        fs::remove_file(sidecar::path_of(&path(&directory, SECOND))).expect("removed");

        let recording = catalog.find(SECOND).expect("found");

        assert_eq!(recording.sidecar.map, "bloodrun");
        assert_eq!(recording.sidecar.started, 1_767_265_200);
        assert_eq!(recording.size, 7);
    }

    #[rstest]
    #[case::disabled(false, true, RecorderState::Disabled)]
    #[case::full(true, false, RecorderState::DiskFull)]
    #[case::waiting(true, true, RecorderState::Waiting)]
    fn the_state_tells_why_nothing_is_recorded(
        #[case] enabled: bool,
        #[case] has_room: bool,
        #[case] state: RecorderState,
    ) {
        let mut catalog = Catalog::default();
        catalog.configure(Path::new("/nowhere"), enabled, has_room);

        assert_eq!(catalog.status(), (state, None));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn finished_files_are_reported_once() {
        let (directory, mut catalog) = catalog_with(false);
        catalog.finished(path(&directory, FIRST));
        catalog.finished(path(&directory, "20260101_130000_gone"));

        let names: Vec<_> = catalog
            .take_finished()
            .into_iter()
            .map(|recording| recording.name)
            .collect();

        assert_eq!(names, [FIRST]);
        assert!(catalog.take_finished().is_empty());
    }

    fn exportable(directory: &TempDir) -> RecordingInfo {
        let file = fs::File::create(path(directory, FIRST)).expect("created");
        let mut writer = ArchiveWriter::start(file, &[], &[]).expect("started");
        let players = [
            (529, b"n\\Human\\st\\76561198000000001".to_vec()),
            (530, b"n\\Bot".to_vec()),
        ];
        for (index, value) in players {
            writer
                .event(&Event::Configstring { index, value })
                .expect("written");
        }
        writer
            .event(&Event::Frame(Frame {
                server_time: 25,
                entities: Vec::new(),
                players: vec![(0, PlayerState::default()), (1, PlayerState::default())],
            }))
            .expect("written");
        writer.finish().expect("finished");
        let mut catalog = Catalog::default();
        catalog.configure(directory.path(), true, true);
        let mut recording = catalog.find(FIRST).expect("found");
        recording.sidecar.players = vec![
            PlayerEntry {
                steam_id: 76_561_198_000_000_001,
                name: "^1Human".into(),
                client_id: 0,
            },
            PlayerEntry {
                steam_id: 90_071_996_842_377_216,
                name: "Bot".into(),
                client_id: 1,
            },
        ];
        recording
    }

    #[rstest]
    #[case::everyone(None, &["^1Human", "Bot"])]
    #[case::human(Some(vec![76_561_198_000_000_001]), &["^1Human"])]
    #[case::bot(Some(vec![90_071_996_842_377_216]), &["Bot"])]
    #[case::nobody(Some(vec![5]), &[])]
    #[cfg_attr(miri, ignore)]
    fn demos_are_exported_by_steam_id(
        #[case] steam_ids: Option<Vec<u64>>,
        #[case] expected: &[&str],
    ) {
        let directory = tempdir().expect("a directory");
        let recording = exportable(&directory);

        let demos = export(
            &recording,
            &directory.path().join("demos"),
            PrivateMessages::Include,
            steam_ids.as_deref(),
        )
        .expect("exported");

        assert!(demos.iter().all(|(path, _)| path.is_file()));
        let names: Vec<_> = demos.iter().map(|(_, player)| &player.name).collect();
        assert_eq!(names, expected);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn an_existing_demo_stops_the_whole_export() {
        let directory = tempdir().expect("a directory");
        let recording = exportable(&directory);
        let target = directory.path().join("demos");
        let run = |steam_ids: Option<&[u64]>| {
            export(&recording, &target, PrivateMessages::Omit, steam_ids)
        };
        run(Some(&[90_071_996_842_377_216])).expect("exported");

        let result = run(None);

        assert!(matches!(result, Err(ExportFailure::Exists(_))));
        assert_eq!(fs::read_dir(&target).expect("readable").count(), 1);
    }
}
