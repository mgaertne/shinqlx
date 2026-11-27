//! The Python interface to the recordings of the matches (`.dm_sqx`).
//!
//! The names exist in every build; without the cargo feature `demo-recording` nothing is recorded
//! and there is nothing to find. The classes only hold plain Rust data.

use std::path::PathBuf;

use pyo3::types::PyTuple;

use super::{
    map_info::{enum_member, repr_option, tuple_of},
    prelude::*,
    string_enums::DemoStates,
};

/// Someone who was in the game during a recording.
#[pyclass(module = "_shinqlx", name = "DemoPlayer", frozen, eq, get_all)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DemoPlayer {
    /// Made up by the engine for bots.
    steam_id: u64,
    /// The name with its color codes.
    name: String,
    client_id: u8,
}

#[pymethods]
impl DemoPlayer {
    fn __repr__(&self) -> String {
        format!(
            "DemoPlayer(steam_id={}, name={:?}, client_id={})",
            self.steam_id, self.name, self.client_id
        )
    }
}

/// The wish to keep a recording.
#[pyclass(module = "_shinqlx", name = "DemoRequest", frozen, eq, get_all)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DemoRequest {
    /// The SteamID of who asked, if a player did.
    pub(super) requested_by: Option<u64>,
    pub(super) reason: String,
    /// Seconds since the Unix epoch.
    pub(super) time: u64,
}

#[pymethods]
impl DemoRequest {
    fn __repr__(&self) -> String {
        format!(
            "DemoRequest(requested_by={}, reason={:?}, time={})",
            repr_option(self.requested_by.as_ref()),
            self.reason,
            self.time
        )
    }
}

/// One recording, as it was when asked for.
#[pyclass(module = "_shinqlx", name = "Recording", frozen, eq)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Recording {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) map: String,
    pub(super) factory: String,
    pub(super) started: u64,
    pub(super) ended: Option<u64>,
    pub(super) size: u64,
    pub(super) match_played: bool,
    pub(super) players: Vec<DemoPlayer>,
    pub(super) requests: Vec<DemoRequest>,
}

#[pymethods]
impl Recording {
    /// The file name without its extension, which identifies the recording.
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[getter]
    fn path(&self) -> &str {
        &self.path
    }

    #[getter]
    fn map(&self) -> &str {
        &self.map
    }

    #[getter]
    fn factory(&self) -> &str {
        &self.factory
    }

    /// Seconds since the Unix epoch.
    #[getter]
    fn started(&self) -> u64 {
        self.started
    }

    /// `None` while the recording runs, and if the server stopped without ending it.
    #[getter]
    fn ended(&self) -> Option<u64> {
        self.ended
    }

    /// The size of the file in bytes.
    #[getter]
    fn size(&self) -> u64 {
        self.size
    }

    /// Whether a match was played during the recording.
    #[getter]
    fn match_played(&self) -> bool {
        self.match_played
    }

    /// Everyone who was in the game, spectators included, as a tuple of `DemoPlayer`.
    #[getter]
    fn players<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        tuple_of(py, self.players.iter().cloned())
    }

    /// The requests to keep the recording as a tuple of `DemoRequest`.
    #[getter]
    fn requests<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        tuple_of(py, self.requests.iter().cloned())
    }

    fn __repr__(&self) -> String {
        format!(
            "Recording(name={:?}, map={:?}, started={}, ended={}, size={})",
            self.name,
            self.map,
            self.started,
            repr_option(self.ended.as_ref()),
            self.size
        )
    }
}

/// What the recorder is doing.
#[pyclass(module = "_shinqlx", name = "DemoStatus", frozen, eq)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DemoStatus {
    pub(super) state: &'static str,
    pub(super) current: Option<Recording>,
}

#[pymethods]
impl DemoStatus {
    /// A member of `DemoStates`.
    #[getter]
    fn state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        enum_member(py, "DemoStates", self.state)
    }

    /// The running recording, if there is one.
    #[getter]
    fn current(&self) -> Option<Recording> {
        self.current.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "DemoStatus(state={:?}, current={})",
            self.state,
            repr_option(self.current.as_ref().map(|current| &current.name))
        )
    }
}

/// A demo that was written from a recording.
#[pyclass(module = "_shinqlx", name = "ExportedDemo", frozen, eq, get_all)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportedDemo {
    pub(super) path: String,
    /// Whose view the demo shows.
    player: DemoPlayer,
}

#[pymethods]
impl ExportedDemo {
    fn __repr__(&self) -> String {
        format!(
            "ExportedDemo(path={:?}, player={})",
            self.path,
            self.player.__repr__()
        )
    }
}

/// What the recorder is doing.
#[pyfunction]
#[pyo3(name = "demo_status")]
pub(crate) fn pyshinqlx_demo_status(py: Python<'_>) -> DemoStatus {
    library::status(py)
}

/// All recordings, the oldest first.
pub(super) fn all_recordings(py: Python<'_>) -> PyResult<Vec<Recording>> {
    library::recordings(py)
}

/// All recordings as a tuple of `Recording`, the oldest first.
#[pyfunction]
#[pyo3(name = "recordings")]
pub(crate) fn pyshinqlx_recordings(py: Python<'_>) -> PyResult<Bound<'_, PyTuple>> {
    tuple_of(py, all_recordings(py)?)
}

/// The recording with this name, `None` if there is none.
#[pyfunction]
#[pyo3(name = "recording", signature = (name), text_signature = "(name)")]
pub(crate) fn pyshinqlx_recording(py: Python<'_>, name: &str) -> Option<Recording> {
    library::recording(py, name)
}

/// Asks to keep a recording. Without a name this is the running recording if it holds a match,
/// otherwise the last one that does, otherwise the running one. Returns `None` if there is no such
/// recording.
#[pyfunction]
#[pyo3(
    name = "request_demo",
    signature = (name = None, *, requested_by = None, reason = ""),
    text_signature = "(name = None, *, requested_by = None, reason = \"\")"
)]
pub(crate) fn pyshinqlx_request_demo(
    py: Python<'_>,
    name: Option<&str>,
    requested_by: Option<u64>,
    reason: &str,
) -> PyResult<Option<Recording>> {
    library::request(py, name, requested_by, reason)
}

/// Writes the demos (`.dm_91`) of a recording into a directory, by default one for every player.
/// This takes seconds, so it must not be called from the game thread. Existing files are never
/// overwritten. Returns a tuple of `ExportedDemo`.
#[pyfunction]
#[pyo3(
    name = "export_demo",
    signature = (name, directory, *, steam_ids = None, include_private = true),
    text_signature = "(name, directory, *, steam_ids = None, include_private = True)"
)]
pub(crate) fn pyshinqlx_export_demo<'py>(
    py: Python<'py>,
    name: &str,
    directory: PathBuf,
    steam_ids: Option<&Bound<'py, PyAny>>,
    include_private: bool,
) -> PyResult<Bound<'py, PyTuple>> {
    let steam_ids = steam_ids
        .map(|steam_ids| {
            steam_ids
                .try_iter()?
                .map(|steam_id| steam_id?.extract::<u64>())
                .collect::<PyResult<Vec<_>>>()
        })
        .transpose()?;
    tuple_of(
        py,
        library::export(py, name, &directory, steam_ids, include_private)?,
    )
}

#[cfg(not(feature = "demo-recording"))]
mod library {
    use std::path::Path;

    use pyo3::exceptions::PyRuntimeError;

    use super::{DemoStates, DemoStatus, ExportedDemo, Recording};
    use crate::python::prelude::*;

    pub(super) fn status(_py: Python<'_>) -> DemoStatus {
        DemoStatus {
            state: DemoStates::Unavailable.as_str(),
            current: None,
        }
    }

    pub(super) fn recordings(_py: Python<'_>) -> PyResult<Vec<Recording>> {
        Ok(Vec::new())
    }

    pub(super) fn recording(_py: Python<'_>, _name: &str) -> Option<Recording> {
        None
    }

    pub(super) fn request(
        _py: Python<'_>,
        _name: Option<&str>,
        _requested_by: Option<u64>,
        _reason: &str,
    ) -> PyResult<Option<Recording>> {
        Ok(None)
    }

    pub(super) fn export(
        _py: Python<'_>,
        _name: &str,
        _directory: &Path,
        _steam_ids: Option<Vec<u64>>,
        _include_private: bool,
    ) -> PyResult<Vec<ExportedDemo>> {
        Err(PyRuntimeError::new_err(
            "this build of shinqlx cannot record demos",
        ))
    }
}

#[cfg(all(feature = "demo-recording", not(test)))]
pub(crate) use library::demo_finished_dispatcher;

#[cfg(feature = "demo-recording")]
mod library {
    use std::{
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    use pyo3::{
        exceptions::{PyFileExistsError, PyFileNotFoundError, PyOSError, PyRuntimeError},
        intern,
    };
    use ql_sqx::PrivateMessages;

    use super::{DemoPlayer, DemoRequest, DemoStates, DemoStatus, ExportedDemo, Recording};
    use crate::{
        demo_capture::{
            catalog::{self, ExportFailure, RecorderState, RecordingInfo, catalog},
            sidecar::{PlayerEntry, Request},
        },
        python::{
            event_dispatcher,
            events::{DemoFinishedDispatcherMethods, event_wanted},
            events_enum::EventName,
            log_exception,
            prelude::*,
            pyshinqlx_is_initialized,
        },
    };

    /// Tells the plugins that the file of a recording is written.
    pub(crate) fn demo_finished_dispatcher(recording: &RecordingInfo) {
        if !event_wanted(EventName::DemoFinished) || !pyshinqlx_is_initialized() {
            return;
        }
        Python::attach(|py| {
            if let Err(error) = dispatch_finished(py, Recording::from(recording)) {
                log_exception(py, &error);
            }
        });
    }

    fn dispatch_finished(py: Python<'_>, recording: Recording) -> PyResult<()> {
        let dispatcher = event_dispatcher(
            py,
            intern!(py, EventName::DemoFinished.as_str()),
            "could not get access to demo finished dispatcher",
        )?;
        DemoFinishedDispatcherMethods::dispatch(dispatcher.cast()?, &Bound::new(py, recording)?)?;
        Ok(())
    }

    pub(super) fn status(py: Python<'_>) -> DemoStatus {
        let (state, current) = py.detach(|| catalog().status());
        let state = match state {
            RecorderState::Disabled => DemoStates::Disabled,
            RecorderState::Waiting => DemoStates::Waiting,
            RecorderState::Recording => DemoStates::Recording,
            RecorderState::DiskFull => DemoStates::DiskFull,
        };
        DemoStatus {
            state: state.as_str(),
            current: current.as_ref().map(Recording::from),
        }
    }

    pub(super) fn recordings(py: Python<'_>) -> PyResult<Vec<Recording>> {
        let recordings = py
            .detach(|| catalog().recordings().map_err(|error| error.to_string()))
            .map_err(PyOSError::new_err)?;
        Ok(recordings.iter().map(Recording::from).collect())
    }

    pub(super) fn recording(py: Python<'_>, name: &str) -> Option<Recording> {
        py.detach(|| catalog().find(name))
            .as_ref()
            .map(Recording::from)
    }

    pub(super) fn request(
        py: Python<'_>,
        name: Option<&str>,
        requested_by: Option<u64>,
        reason: &str,
    ) -> PyResult<Option<Recording>> {
        let request = Request {
            requested_by,
            reason: reason.into(),
            time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let recording = py
            .detach(|| {
                catalog()
                    .request(name, request)
                    .map_err(|error| error.to_string())
            })
            .map_err(PyOSError::new_err)?;
        Ok(recording.as_ref().map(Recording::from))
    }

    pub(super) fn export(
        py: Python<'_>,
        name: &str,
        directory: &Path,
        steam_ids: Option<Vec<u64>>,
        include_private: bool,
    ) -> PyResult<Vec<ExportedDemo>> {
        if catalog::is_game_thread() {
            return Err(PyRuntimeError::new_err(
                "export_demo takes too long for the game thread, call it from a function \
                 decorated with @shinqlx.thread",
            ));
        }
        let private_messages = if include_private {
            PrivateMessages::Include
        } else {
            PrivateMessages::Omit
        };
        let demos = py
            .detach(|| {
                let recording = catalog().find(name)?;
                let demos = catalog::export(
                    &recording,
                    directory,
                    private_messages,
                    steam_ids.as_deref(),
                );
                Some(demos.map_err(|failure| (exception_for(&failure), failure.to_string())))
            })
            .ok_or_else(|| PyFileNotFoundError::new_err(format!("no recording {name:?}")))?
            .map_err(|(exception, message)| exception(message))?;
        Ok(demos
            .into_iter()
            .map(|(path, player)| ExportedDemo {
                path: path.to_string_lossy().into_owned(),
                player: DemoPlayer::from(&player),
            })
            .collect())
    }

    fn exception_for(failure: &ExportFailure) -> fn(String) -> PyErr {
        match failure {
            ExportFailure::Exists(_) => PyFileExistsError::new_err,
            ExportFailure::Directory(..) | ExportFailure::Cleanup(..) => PyOSError::new_err,
            ExportFailure::Demo(_) => PyRuntimeError::new_err,
        }
    }

    impl From<&PlayerEntry> for DemoPlayer {
        fn from(player: &PlayerEntry) -> Self {
            Self {
                steam_id: player.steam_id,
                name: player.name.clone(),
                client_id: player.client_id,
            }
        }
    }

    impl From<&Request> for DemoRequest {
        fn from(request: &Request) -> Self {
            Self {
                requested_by: request.requested_by,
                reason: request.reason.clone(),
                time: request.time,
            }
        }
    }

    impl From<&RecordingInfo> for Recording {
        fn from(recording: &RecordingInfo) -> Self {
            let sidecar = &recording.sidecar;
            Self {
                name: recording.name.clone(),
                path: recording.path.to_string_lossy().into_owned(),
                map: sidecar.map.clone(),
                factory: sidecar.factory.clone(),
                started: sidecar.started,
                ended: sidecar.ended,
                size: recording.size,
                match_played: sidecar.match_played,
                players: sidecar.players.iter().map(DemoPlayer::from).collect(),
                requests: sidecar.requests.iter().map(DemoRequest::from).collect(),
            }
        }
    }
}

#[cfg(test)]
mod demos_tests {
    use pyo3::{ffi::c_str, types::PyDict};
    use rstest::rstest;

    use super::*;
    use crate::{prelude::*, python::pyshinqlx_setup_fixture::pyshinqlx_setup};

    fn run(py: Python<'_>, code: &core::ffi::CStr, directory: &str) {
        let globals = PyDict::new(py);
        globals
            .set_item("shinqlx", py.import("shinqlx").expect("importable"))
            .expect("set");
        globals.set_item("directory", directory).expect("set");
        py.run(code, Some(&globals), None).expect("passes");
    }

    #[cfg(not(feature = "demo-recording"))]
    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn without_the_feature_there_is_nothing(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            run(
                py,
                c_str!(
                    r#"
assert shinqlx.demo_status().state == shinqlx.DemoStates.UNAVAILABLE
assert shinqlx.demo_status().current is None
assert shinqlx.recordings() == ()
assert shinqlx.recording("20260101_100000_aerowalk") is None
assert shinqlx.request_demo(requested_by=1, reason="check") is None
try:
    shinqlx.export_demo("20260101_100000_aerowalk", directory)
except RuntimeError:
    pass
else:
    raise AssertionError("exported")
"#
                ),
                "/nowhere",
            );
        });
    }

    #[cfg(feature = "demo-recording")]
    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn hooked_plugins_hear_of_finished_recordings(_pyshinqlx_setup: ()) {
        use core::{borrow::BorrowMut, sync::atomic::Ordering};

        use crate::{
            demo_capture::{catalog::RecordingInfo, sidecar::Sidecar},
            engine::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                EVENT_DISPATCHERS, PYSHINQLX_INITIALIZED,
                commands::CommandPriorities,
                events::{
                    DemoFinishedDispatcher, EventDispatcher, EventDispatcherManager,
                    EventDispatcherManagerMethods, EventDispatcherMethods,
                },
            },
        };

        let mut raw_cvar = CVarBuilder::default()
            .string(c"1".as_ptr().cast_mut())
            .build()
            .expect("a cvar");
        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                0..,
            )
            .run(|| {
                Python::attach(|py| {
                    let manager = Bound::new(py, EventDispatcherManager::default())
                        .expect("a dispatcher manager");
                    manager
                        .add_dispatcher(&py.get_type::<DemoFinishedDispatcher>())
                        .expect("added");
                    EVENT_DISPATCHERS.store(Some(manager.clone().unbind().into()));
                    let globals = PyDict::new(py);
                    py.run(
                        c"received = []\ndef handler(recording):\n    received.append(recording.name)",
                        Some(&globals),
                        None,
                    )
                    .expect("defined");
                    let handler = globals.get_item("handler").expect("readable");
                    manager
                        .get_item("demo_finished")
                        .expect("registered")
                        .cast::<EventDispatcher>()
                        .expect("a dispatcher")
                        .add_hook(
                            "test_plugin",
                            &handler.expect("a function"),
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("hooked");

                    let initialized = PYSHINQLX_INITIALIZED.swap(true, Ordering::AcqRel);
                    library::demo_finished_dispatcher(&RecordingInfo {
                        name: "finished".into(),
                        path: "/nowhere/finished.dm_sqx".into(),
                        size: 1,
                        sidecar: Sidecar::default(),
                    });
                    PYSHINQLX_INITIALIZED.store(initialized, Ordering::Release);

                    py.run(c"assert received == [\"finished\"]", Some(&globals), None)
                        .expect("received");
                });
            });
    }

    #[cfg(feature = "demo-recording")]
    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn recordings_are_listed_requested_and_exported(_pyshinqlx_setup: ()) {
        use ql_sqx::{ArchiveWriter, Event, Frame, PlayerState};

        use crate::demo_capture::catalog::catalog;

        let directory = tempfile::tempdir().expect("a directory");
        let file = std::fs::File::create(directory.path().join("20260101_100000_aerowalk.dm_sqx"))
            .expect("created");
        let mut writer = ArchiveWriter::start(file, &[], &[]).expect("started");
        writer
            .event(&Event::Frame(Frame {
                server_time: 25,
                entities: Vec::new(),
                players: vec![(0, PlayerState::default())],
            }))
            .expect("written");
        writer.finish().expect("finished");
        catalog().configure(directory.path(), true, true);

        Python::attach(|py| {
            run(
                py,
                c_str!(
                    r#"
name = "20260101_100000_aerowalk"
assert shinqlx.demo_status().state == shinqlx.DemoStates.WAITING
(recording,) = shinqlx.recordings()
assert recording == shinqlx.recording(name)
assert (recording.name, recording.map, recording.ended) == (name, "aerowalk", None)
assert recording.players == () and recording.requests == ()
assert shinqlx.recording("unknown") is None

requested = shinqlx.request_demo(name, requested_by=7, reason="check")
((request,),) = (requested.requests,)
assert (request.requested_by, request.reason) == (7, "check")
assert shinqlx.request_demo("unknown") is None

(demo,) = shinqlx.export_demo(name, directory + "/demos", include_private=False)
assert demo.path.endswith("_client00.dm_91") and demo.player.client_id == 0
assert shinqlx.export_demo(name, directory + "/other", steam_ids={5}) == ()
for failing, error in (
    (lambda: shinqlx.export_demo(name, directory + "/demos"), FileExistsError),
    (lambda: shinqlx.export_demo("unknown", directory), FileNotFoundError),
    (lambda: shinqlx.export_demo(name, directory, steam_ids=["x"]), TypeError),
):
    try:
        failing()
    except error:
        pass
    else:
        raise AssertionError(error)
"#
                ),
                directory.path().to_str().expect("utf-8"),
            );
        });
    }
}
