use pyo3::prelude::*;

use super::{
    create_cvar, register_command,
    target::{stop_all, usage},
};
use crate::python::{
    AbstractChannel, Player, PlayerMethods,
    demos::{Recording, all_recordings, pyshinqlx_demo_status, pyshinqlx_request_demo},
    get_cvar,
    plugin::{Plugin, cvar_int},
    string_enums::DemoStates,
};

const LISTED_BY_DEFAULT: usize = 10;
const MOST_LISTED: usize = 30;
const LISTING_PERMISSION: i32 = 4;
const UNAVAILABLE: &str = "This server cannot record demos.";

/// Lets players ask to keep the recording of a match, and admins see what is recorded.
#[pyclass(name = "demos", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Demos;

#[pymethods]
impl Demos {
    fn cmd_request(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let [_, subject, reason @ ..] = msg.as_slice() else {
            return usage();
        };
        if !subject.eq_ignore_ascii_case("demo") {
            return usage();
        }
        let py = player.py();
        let status = pyshinqlx_demo_status(py);
        if DemoStates::from_name(status.state) == Some(DemoStates::Unavailable) {
            return tell_privately(player, UNAVAILABLE);
        }
        let requested = pyshinqlx_request_demo(
            py,
            None,
            requester(player.get_steam_id()),
            &reason.join(" "),
        )?;
        let message = match requested {
            Some(recording) => {
                let running = status
                    .current
                    .is_some_and(|current| current.name == recording.name);
                kept_notice(&recording.name, running, keep_days()?)
            }
            None => "There is no recording to keep.".to_owned(),
        };
        tell_privately(player, &message)
    }

    fn cmd_demos(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let Some(limit) = listing_limit(msg.get(1)) else {
            return usage();
        };
        let py = player.py();
        let status = pyshinqlx_demo_status(py);
        if DemoStates::from_name(status.state) == Some(DemoStates::Unavailable) {
            return tell_privately(player, UNAVAILABLE);
        }
        let current = status.current.map(|current| current.name);
        let listing = listing(&all_recordings(py)?, current.as_deref(), limit);
        tell_privately(player, &listing)
    }

    fn cmd_demostatus(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let py = player.py();
        let status = pyshinqlx_demo_status(py);
        let state = match DemoStates::from_name(status.state) {
            None | Some(DemoStates::Unavailable) => {
                return tell_privately(player, UNAVAILABLE);
            }
            Some(DemoStates::Disabled) => {
                "Demo recording is switched off (qlx_demoRecord 0).".to_owned()
            }
            Some(DemoStates::Waiting) => {
                "Demo recording is waiting for something to record.".to_owned()
            }
            Some(DemoStates::DiskFull) => "Demo recording is paused, the disk is full.".to_owned(),
            Some(DemoStates::Recording) => status.current.as_ref().map_or_else(
                || "Demo recording is running.".to_owned(),
                |current| {
                    format!(
                        "Recording ^6{}^7 ({}, {} players).",
                        current.name,
                        megabytes(current.size),
                        current.players.len()
                    )
                },
            ),
        };
        let totals = totals(&all_recordings(py)?);
        tell_privately(player, &format!("{state}\n{totals}"))
    }
}

/// Replies to the caller only and keeps the command out of the chat.
fn tell_privately(player: &Bound<'_, Player>, message: &str) -> PyResult<Option<i32>> {
    player.tell(message, None)?;
    stop_all()
}

/// A console without an owner has no SteamID.
fn requester(steam_id: i64) -> Option<u64> {
    u64::try_from(steam_id)
        .ok()
        .filter(|&steam_id| steam_id != 0)
}

fn keep_days() -> PyResult<i64> {
    Ok(match get_cvar("qlx_demoKeepRequestedDays")? {
        Some(_) => cvar_int("qlx_demoKeepRequestedDays")?,
        None => 30,
    })
}

fn kept_notice(name: &str, running: bool, days: i64) -> String {
    let which = if running {
        "The running recording"
    } else {
        "Recording"
    };
    let duration = match days {
        ..=0 => String::new(),
        1 => " for ^61^7 day".to_owned(),
        days => format!(" for ^6{days}^7 days"),
    };
    format!("{which} ^6{name}^7 will be kept{duration}.")
}

/// `None` if the argument is not a positive number.
fn listing_limit(argument: Option<&String>) -> Option<usize> {
    argument.map_or(Some(LISTED_BY_DEFAULT), |text| {
        text.parse::<usize>()
            .ok()
            .filter(|&limit| limit > 0)
            .map(|limit| limit.min(MOST_LISTED))
    })
}

fn megabytes(bytes: u64) -> String {
    let tenths = bytes.saturating_mul(10).saturating_add(1 << 19) >> 20;
    format!("{}.{} MB", tenths / 10, tenths % 10)
}

/// The newest recordings first; `recordings` has the oldest first.
fn listing(recordings: &[Recording], current: Option<&str>, limit: usize) -> String {
    if recordings.is_empty() {
        return "There are no recordings.".to_owned();
    }
    let header = format!(
        "Showing {} of {} recordings, the newest first:",
        limit.min(recordings.len()),
        recordings.len()
    );
    core::iter::once(header)
        .chain(
            recordings
                .iter()
                .rev()
                .take(limit)
                .map(|recording| line(recording, current == Some(recording.name.as_str()))),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

fn line(recording: &Recording, running: bool) -> String {
    let mut line = format!("^6{}^7 {}", recording.name, megabytes(recording.size));
    if running {
        line.push_str(", running");
    }
    if !recording.match_played {
        line.push_str(", no match");
    }
    if !recording.requests.is_empty() {
        line.push_str(", ^3requested^7");
        let reasons = recording
            .requests
            .iter()
            .map(|request| request.reason.as_str())
            .filter(|reason| !reason.is_empty())
            .collect::<Vec<_>>();
        if !reasons.is_empty() {
            line.push_str(&format!(": {}^7", reasons.join("; ")));
        }
    }
    line
}

fn totals(recordings: &[Recording]) -> String {
    let size = recordings.iter().map(|recording| recording.size).sum();
    let requested = recordings
        .iter()
        .filter(|recording| !recording.requests.is_empty())
        .count();
    format!(
        "Recordings: {}, {} in total, {requested} requested.",
        recordings.len(),
        megabytes(size)
    )
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, Plugin>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Demos),
    )?;
    create_cvar(py, "qlx_demoRequestPermission", "1")?;
    let request_permission = i32::try_from(cvar_int("qlx_demoRequestPermission")?).unwrap_or(5);
    register_command(
        plugin.as_super(),
        &["request"],
        "cmd_request",
        request_permission,
        "demo [reason]",
        None,
    )?;
    register_command(
        plugin.as_super(),
        &["demos"],
        "cmd_demos",
        LISTING_PERMISSION,
        "[count]",
        None,
    )?;
    register_command(
        plugin.as_super(),
        &["demostatus"],
        "cmd_demostatus",
        LISTING_PERMISSION,
        "",
        None,
    )?;
    Ok(plugin.into_super())
}

#[cfg(test)]
mod demos_plugin_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;
    use crate::python::demos::DemoRequest;

    fn recording(name: &str, size: u64, match_played: bool, reasons: &[&str]) -> Recording {
        Recording {
            name: name.into(),
            size,
            match_played,
            requests: reasons
                .iter()
                .map(|&reason| DemoRequest {
                    requested_by: None,
                    reason: reason.into(),
                    time: 0,
                })
                .collect(),
            ..Recording::default()
        }
    }

    #[rstest]
    #[case(false, 30, "Recording ^6a^7 will be kept for ^630^7 days.")]
    #[case(true, 1, "The running recording ^6a^7 will be kept for ^61^7 day.")]
    #[case(false, 0, "Recording ^6a^7 will be kept.")]
    fn the_notice_tells_how_long_a_recording_is_kept(
        #[case] running: bool,
        #[case] days: i64,
        #[case] expected: &str,
    ) {
        assert_eq!(kept_notice("a", running, days), expected);
    }

    #[rstest]
    #[case(None, Some(10))]
    #[case(Some("3"), Some(3))]
    #[case(Some("500"), Some(30))]
    #[case(Some("0"), None)]
    #[case(Some("all"), None)]
    fn the_number_of_listed_recordings_is_limited(
        #[case] argument: Option<&str>,
        #[case] limit: Option<usize>,
    ) {
        assert_eq!(listing_limit(argument.map(String::from).as_ref()), limit);
    }

    #[rstest]
    #[case(0, "0.0 MB")]
    #[case(1_048_576, "1.0 MB")]
    #[case(1_572_864, "1.5 MB")]
    fn sizes_are_shown_in_megabytes(#[case] bytes: u64, #[case] expected: &str) {
        assert_eq!(megabytes(bytes), expected);
    }

    #[rstest]
    #[case(0, None)]
    #[case(-1, None)]
    #[case(7, Some(7))]
    fn only_players_are_named_as_requesters(#[case] steam_id: i64, #[case] named: Option<u64>) {
        assert_eq!(requester(steam_id), named);
    }

    #[test]
    fn the_listing_starts_with_the_newest_recording() {
        let recordings = [
            recording("old", 1_048_576, true, &["aim", ""]),
            recording("middle", 0, true, &[""]),
            recording("new", 0, false, &[]),
        ];

        assert_eq!(
            listing(&recordings, Some("new"), 10),
            "Showing 3 of 3 recordings, the newest first:\n\
            ^6new^7 0.0 MB, running, no match\n\
            ^6middle^7 0.0 MB, ^3requested^7\n\
            ^6old^7 1.0 MB, ^3requested^7: aim^7"
        );
        assert_eq!(
            listing(&recordings, None, 1),
            "Showing 1 of 3 recordings, the newest first:\n^6new^7 0.0 MB, no match"
        );
        assert_eq!(listing(&[], None, 10), "There are no recordings.");
        assert_eq!(
            totals(&recordings),
            "Recordings: 3, 1.0 MB in total, 2 requested."
        );
    }
}

#[cfg(test)]
mod demos_command_tests {
    use pyo3::types::PyDict;
    use rstest::rstest;

    use super::*;
    use crate::{prelude::*, python::pyshinqlx_setup_fixture::pyshinqlx_setup};

    const CALLER: &core::ffi::CStr = cr#"
import shinqlx


class Caller(shinqlx.AbstractDummyPlayer):
    def __init__(self):
        super().__init__("Caller")
        self.told = []

    def tell(self, msg, **kwargs):
        self.told.append(msg)


caller = Caller()
"#;

    /// Runs the commands with a caller that remembers what it is told, and returns that.
    fn told(commands: &[&str]) -> Vec<String> {
        Python::attach(|py| {
            let globals = PyDict::new(py);
            py.run(CALLER, Some(&globals), None).expect("a caller");
            let caller = globals
                .get_item("caller")
                .expect("readable")
                .expect("defined")
                .cast_into::<Player>()
                .expect("a player");
            let plugin = Bound::new(
                py,
                PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Demos),
            )
            .expect("a plugin");
            let channel = py
                .get_type::<AbstractChannel>()
                .call1(("chat",))
                .expect("a channel")
                .cast_into::<AbstractChannel>()
                .expect("a channel");
            for command in commands {
                let msg: Vec<String> = command.split(' ').map(Into::into).collect();
                let handler = match msg[0].as_str() {
                    "request" => Demos::cmd_request,
                    "demos" => Demos::cmd_demos,
                    _ => Demos::cmd_demostatus,
                };
                handler(&plugin, &caller, msg, &channel).expect("handled");
            }
            caller
                .getattr("told")
                .and_then(|told| told.extract())
                .expect("messages")
        })
    }

    #[cfg(not(feature = "demo-recording"))]
    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn without_the_feature_every_command_says_so(_pyshinqlx_setup: ()) {
        assert_eq!(
            told(&["request demo", "demos", "demostatus"]),
            [UNAVAILABLE; 3]
        );
    }

    #[cfg(feature = "demo-recording")]
    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn recordings_are_requested_and_listed(_pyshinqlx_setup: ()) {
        use core::borrow::BorrowMut;

        use crate::{
            demo_capture::catalog::catalog,
            engine::prelude::{CVar, CVarBuilder, cvar_t},
        };

        let directory = tempfile::tempdir().expect("a directory");
        catalog().configure(directory.path(), false, true);
        let mut raw_cvar = CVarBuilder::default()
            .string(c"7".as_ptr().cast_mut())
            .integer(7)
            .build()
            .expect("a cvar");
        MockEngineBuilder::default()
            .with_find_cvar(
                |name| name == "qlx_demoKeepRequestedDays",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                0..,
            )
            .run(|| {
                assert_eq!(
                    told(&["request demo", "demos"]),
                    ["There is no recording to keep.", "There are no recordings."]
                );
                std::fs::write(
                    directory.path().join("20260101_100000_aerowalk.dm_sqx"),
                    b"x",
                )
                .expect("written");

                assert_eq!(
                    told(&["demostatus"]),
                    ["Demo recording is switched off (qlx_demoRecord 0).\n\
                    Recordings: 1, 0.0 MB in total, 0 requested."]
                );
                // Without a match in any recording nothing is meant by default.
                assert_eq!(
                    told(&["request demo", "request map", "demos x"]),
                    ["There is no recording to keep."]
                );
            });
    }
}
