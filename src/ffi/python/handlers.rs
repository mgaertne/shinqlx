use super::prelude::*;
use crate::ffi::c::prelude::*;
use crate::{quake_live_engine::GetConfigstring, MAIN_ENGINE};

use itertools::Itertools;
use once_cell::sync::Lazy;
use pyo3::types::{IntoPyDict, PyDict};
use regex::{Regex, RegexBuilder};

fn try_log_exception(py: Python<'_>, exception: PyErr) -> PyResult<()> {
    let logging_module = py.import("logging")?;
    let traceback_module = py.import("traceback")?;

    let py_logger = logging_module.call_method1("getLogger", ("shinqlx",))?;

    let formatted_traceback: Vec<String> = traceback_module
        .call_method1(
            "format_exception",
            (
                exception.get_type(py),
                exception.value(py),
                exception.traceback(py),
            ),
        )?
        .extract()?;

    formatted_traceback.iter().for_each(|line| {
        let _ = py_logger.call_method1("error", (line.trim_end(),));
    });

    Ok(())
}

fn log_exception(py: Python<'_>, exception: PyErr) {
    let _ = try_log_exception(py, exception);
}

fn try_handle_rcon(py: Python<'_>, cmd: String) -> PyResult<Option<bool>> {
    let rcon_dummy_player = Py::new(py, RconDummyPlayer::py_new())?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_console_channel = shinqlx_module.getattr("CONSOLE_CHANNEL")?;

    let shinqlx_commands = shinqlx_module.getattr("COMMANDS")?;
    let _ = shinqlx_commands.call_method1(
        "handle_input",
        (rcon_dummy_player, cmd, shinqlx_console_channel),
    )?;
    Ok(None)
}

/// Console commands that are to be processed as regular pyshinqlx
/// commands as if the owner executes it. This allows the owner to
/// interact with the Python part of shinqlx without having to connect.
#[pyfunction]
pub(crate) fn handle_rcon(py: Python<'_>, cmd: String) -> Option<bool> {
    try_handle_rcon(py, cmd).unwrap_or_else(|e| {
        log_exception(py, e);
        Some(true)
    })
}

static RE_SAY: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^say +"?(?P<msg>.+)"?$"#)
        .case_insensitive(true)
        .build()
        .unwrap()
});
static RE_SAY_TEAM: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^say_team +"?(?P<msg>.+)"?$"#)
        .case_insensitive(true)
        .build()
        .unwrap()
});
static RE_CALLVOTE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^(?:cv|callvote) +(?P<cmd>[^ ]+)(?: "?(?P<args>.+?)"?)?$"#)
        .case_insensitive(true)
        .build()
        .unwrap()
});
static RE_VOTE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r"^vote +(?P<arg>.)")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static RE_TEAM: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r"^team +(?P<arg>.)")
        .case_insensitive(true)
        .build()
        .unwrap()
});
static RE_USERINFO: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^userinfo "(?P<vars>.+)"$"#).unwrap());

fn is_vote_active() -> bool {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return false;
    };
    !main_engine
        .get_configstring(CS_VOTE_STRING as u16)
        .is_empty()
}

fn try_handle_client_command(py: Python<'_>, client_id: i32, cmd: String) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let server_command_dispatcher = shinqlx_event_dispatchers.get_item("client_command")?;

    let return_value =
        server_command_dispatcher.call_method1("dispatch", (player.clone(), cmd.clone()))?;
    if return_value.extract::<bool>().is_ok_and(|value| !value) {
        return Ok(false.into_py(py));
    };

    let updated_cmd = match return_value.extract::<String>() {
        Ok(extracted_string) => extracted_string,
        _ => cmd.clone(),
    };

    if let Some(captures) = RE_SAY.captures(&updated_cmd) {
        if let Some(msg) = captures.name("msg") {
            let reformatted_msg = msg.as_str().replace('"', "");
            let channel = shinqlx_module.getattr("CHAT_CHANNEL")?;
            let chat_dispatcher = shinqlx_event_dispatchers.get_item("chat")?;
            let result = chat_dispatcher
                .call_method1("dispatch", (player.clone(), reformatted_msg, channel))?;
            if result.extract::<bool>().is_ok_and(|value| !value) {
                return Ok(false.into_py(py));
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(captures) = RE_SAY_TEAM.captures(&updated_cmd) {
        if let Some(msg) = captures.name("msg") {
            let reformatted_msg = msg.as_str().replace('"', "");
            let channel = match player.get_team(py)?.as_str() {
                "free" => shinqlx_module.getattr("FREE_CHAT_CHANNEL")?,
                "red" => shinqlx_module.getattr("RED_CHAT_CHANNEL")?,
                "blue" => shinqlx_module.getattr("BLUE_CHAT_CHANNEL")?,
                _ => shinqlx_module.getattr("SPECTATOR_CHAT_CHANNEL")?,
            };
            let chat_dispatcher = shinqlx_event_dispatchers.get_item("chat")?;
            let result = chat_dispatcher
                .call_method1("dispatch", (player.clone(), reformatted_msg, channel))?;
            if result.extract::<bool>().is_ok_and(|value| !value) {
                return Ok(false.into_py(py));
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(captures) = RE_CALLVOTE.captures(&updated_cmd) {
        if !is_vote_active() {
            if let Some(vote) = captures.name("cmd") {
                let args = captures
                    .name("args")
                    .map(|matched| matched.as_str())
                    .unwrap_or("");
                let vote_started_dispatcher = shinqlx_event_dispatchers.get_item("vote_started")?;
                let result = vote_started_dispatcher
                    .call_method1("dispatch", (player.clone(), vote.as_str(), args))?;
                if result.extract::<bool>().is_ok_and(|value| !value) {
                    return Ok(false.into_py(py));
                }
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(captures) = RE_VOTE.captures(&updated_cmd) {
        if is_vote_active() {
            if let Some(arg) = captures.name("arg") {
                if ["y", "Y", "1", "n", "N", "2"].contains(&arg.as_str()) {
                    let vote = ["y", "Y", "1"].contains(&arg.as_str());
                    let vote_dispatcher = shinqlx_event_dispatchers.get_item("vote_started")?;
                    let result =
                        vote_dispatcher.call_method1("dispatch", (player.clone(), vote))?;
                    if result.extract::<bool>().is_ok_and(|value| !value) {
                        return Ok(false.into_py(py));
                    }
                }
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(captures) = RE_TEAM.captures(&updated_cmd) {
        if let Some(arg) = captures.name("arg") {
            let current_team = player.get_team(py)?;
            if !["f", "r", "b", "s", "a"].contains(&arg.as_str())
                || current_team.starts_with(arg.as_str())
            {
                return Ok(updated_cmd.into_py(py));
            }

            let target_team = match arg.as_str() {
                "f" => "free",
                "r" => "red",
                "b" => "blue",
                "s" => "spectator",
                _ => "any",
            };
            let team_switch_attempt_dispatcher =
                shinqlx_event_dispatchers.get_item("team_switch_attempt")?;
            let result = team_switch_attempt_dispatcher
                .call_method1("dispatch", (player.clone(), current_team, target_team))?;
            if result.extract::<bool>().is_ok_and(|value| !value) {
                return Ok(false.into_py(py));
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(captures) = RE_USERINFO.captures(&updated_cmd) {
        if let Some(vars) = captures.name("vars") {
            let new_info = parse_variables(vars.as_str().into());
            let old_info = parse_variables(player.user_info.clone());

            let changed: Vec<&(String, String)> = new_info
                .items
                .iter()
                .filter(|(key, new_value)| {
                    let opt_old_value = old_info.get(key);
                    opt_old_value.is_none()
                        || opt_old_value.is_some_and(|old_value| old_value != *new_value)
                })
                .collect();

            if !changed.is_empty() {
                let userinfo_dispatcher = shinqlx_event_dispatchers.get_item("userinfo")?;
                let result = userinfo_dispatcher
                    .call_method1("dispatch", (player.clone(), changed.into_py_dict(py)))?;
                if result.extract::<bool>().is_ok_and(|value| !value) {
                    return Ok(false.into_py(py));
                }
                if let Ok(changed_values) = result.extract::<&PyDict>() {
                    let updated_info = new_info.into_py_dict(py);
                    updated_info.update(changed_values.as_mapping())?;
                    let formatted_key_values = updated_info
                        .iter()
                        .map(|(key, value)| format!(r"\{key}\{value}"))
                        .join("");

                    return Ok(format!(r#"userinfo "{formatted_key_values}""#).into_py(py));
                }
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    Ok(updated_cmd.into_py(py))
}

/// Client commands are commands such as "say", "say_team", "scores",
/// "disconnect" and so on. This function parses those and passes it
/// on to the event dispatcher.
#[pyfunction]
pub(crate) fn handle_client_command(py: Python<'_>, client_id: i32, cmd: String) -> PyObject {
    try_handle_client_command(py, client_id, cmd).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

static RE_VOTE_ENDED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^print "Vote (?P<result>passed|failed).\n"$"#).unwrap());

fn try_handle_server_command(py: Python<'_>, client_id: i32, cmd: String) -> PyResult<PyObject> {
    let Some(player) = (if (0..MAX_CLIENTS as i32).contains(&client_id) {
        Player::py_new(client_id, None)
            .map(|player| player.into_py(py))
            .ok()
    } else {
        Some(py.None())
    }) else {
        return Ok(true.into_py(py));
    };

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let server_command_dispatcher = shinqlx_event_dispatchers.get_item("server_command")?;

    let return_value = server_command_dispatcher.call_method1("dispatch", (player, cmd.clone()))?;
    if return_value.extract::<bool>().is_ok_and(|value| !value) {
        return Ok(false.into_py(py));
    };

    let updated_cmd = match return_value.extract::<String>() {
        Ok(extracted_string) => extracted_string,
        _ => cmd.clone(),
    };

    if let Some(captures) = RE_VOTE_ENDED.captures(&updated_cmd) {
        let vote_passed = captures
            .name("result")
            .is_some_and(|value| value.as_str() == "passed");
        let vote_ended_dispatcher = shinqlx_event_dispatchers.get_item("vote_ended")?;
        let _ = vote_ended_dispatcher.call_method1("dispatch", (vote_passed,))?;
    }

    Ok(updated_cmd.into_py(py))
}

#[pyfunction]
pub(crate) fn handle_server_command(py: Python<'_>, client_id: i32, cmd: String) -> PyObject {
    try_handle_server_command(py, client_id, cmd).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_run_frame_tasks(py: Python<'_>) -> PyResult<()> {
    let shinqlx_module = py.import("shinqlx")?;
    let frame_tasks = shinqlx_module.getattr("frame_tasks")?;
    frame_tasks.call_method("run", (), Some([("blocking", false)].into_py_dict(py)))?;

    Ok(())
}

fn try_handle_frame(py: Python<'_>) -> PyResult<()> {
    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let frame_dispatcher = shinqlx_event_dispatchers.get_item("frame")?;
    frame_dispatcher.call_method0("dispatch")?;

    Ok(())
}

fn run_next_frame_tasks(py: Python<'_>) {
    match PyModule::from_code(
        py,
        r#"
from shinqlx import next_frame_tasks, frame_tasks

while not next_frame_tasks.empty():
    func, args, kwargs = next_frame_tasks.get_nowait()
    frame_tasks.enter(0, 1, func, args, kwargs)
"#,
        "",
        "",
    ) {
        Err(e) => log_exception(py, e),
        Ok(next_frame_tasks_runner) => {
            if let Err(e) = next_frame_tasks_runner.call0() {
                log_exception(py, e);
            }
        }
    }
}

/// This will be called every frame. To allow threads to call stuff from the
/// main thread, tasks can be scheduled using the :func:`shinqlx.next_frame` decorator
/// and have it be executed here.
#[pyfunction]
pub(crate) fn handle_frame(py: Python<'_>) -> Option<bool> {
    while let Err(e) = try_run_frame_tasks(py) {
        log_exception(py, e);
    }

    if let Err(e) = try_handle_frame(py) {
        log_exception(py, e);
        return Some(true);
    }

    run_next_frame_tasks(py);

    None
}

fn try_handle_player_connect(py: Python<'_>, client_id: i32, _is_bot: bool) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let player_connect_dispatcher = shinqlx_event_dispatchers.get_item("player_connect")?;
    player_connect_dispatcher
        .call_method1("dispatch", (player,))
        .map(|value| value.into_py(py))
}

/// This will be called whenever a player tries to connect. If the dispatcher
/// returns False, it will not allow the player to connect and instead show them
/// a message explaining why. The default message is "You are banned from this
/// server.", but it can be set with :func:`shinqlx.set_ban_message`.
#[pyfunction]
pub(crate) fn handle_player_connect(py: Python<'_>, client_id: i32, is_bot: bool) -> PyObject {
    try_handle_player_connect(py, client_id, is_bot).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_player_loaded(py: Python<'_>, client_id: i32) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let player_loaded_dispatcher = shinqlx_event_dispatchers.get_item("player_loaded")?;
    player_loaded_dispatcher
        .call_method1("dispatch", (player,))
        .map(|value| value.into_py(py))
}

/// This will be called whenever a player has connected and finished loading,
/// meaning it'll go off a bit later than the usual "X connected" messages.
/// This will not trigger on bots.his will be called whenever a player tries to connect. If the dispatcher
#[pyfunction]
pub(crate) fn handle_player_loaded(py: Python<'_>, client_id: i32) -> PyObject {
    try_handle_player_loaded(py, client_id).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_player_disconnect(
    py: Python<'_>,
    client_id: i32,
    reason: Option<String>,
) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let player_disconnect_dispatcher = shinqlx_event_dispatchers.get_item("player_disconnect")?;
    player_disconnect_dispatcher
        .call_method1("dispatch", (player, reason))
        .map(|value| value.into_py(py))
}

/// This will be called whenever a player disconnects.
#[pyfunction]
pub(crate) fn handle_player_disconnect(
    py: Python<'_>,
    client_id: i32,
    reason: Option<String>,
) -> PyObject {
    try_handle_player_disconnect(py, client_id, reason).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_player_spawn(py: Python<'_>, client_id: i32) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let player_spawn_dispatcher = shinqlx_event_dispatchers.get_item("player_spawn")?;
    player_spawn_dispatcher
        .call_method1("dispatch", (player,))
        .map(|value| value.into_py(py))
}

/// Called when a player spawns. Note that a spectator going in free spectate mode
/// makes the client spawn, so you'll want to check for that if you only want "actual"
/// spawns.
#[pyfunction]
pub(crate) fn handle_player_spawn(py: Python<'_>, client_id: i32) -> PyObject {
    try_handle_player_spawn(py, client_id).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_kamikaze_use(py: Python<'_>, client_id: i32) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let kamikaze_use_dispatcher = shinqlx_event_dispatchers.get_item("kamikaze_use")?;
    kamikaze_use_dispatcher
        .call_method1("dispatch", (player,))
        .map(|value| value.into_py(py))
}

/// This will be called whenever player uses kamikaze item.
#[pyfunction]
pub(crate) fn handle_kamikaze_use(py: Python<'_>, client_id: i32) -> PyObject {
    try_handle_kamikaze_use(py, client_id).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_kamikaze_explode(
    py: Python<'_>,
    client_id: i32,
    is_used_on_demand: bool,
) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let kamikaze_explode_dispatcher = shinqlx_event_dispatchers.get_item("kamikaze_explode")?;
    kamikaze_explode_dispatcher
        .call_method1("dispatch", (player, is_used_on_demand))
        .map(|value| value.into_py(py))
}

/// This will be called whenever kamikaze explodes.
#[pyfunction]
pub(crate) fn handle_kamikaze_explode(
    py: Python<'_>,
    client_id: i32,
    is_used_on_demand: bool,
) -> PyObject {
    try_handle_kamikaze_explode(py, client_id, is_used_on_demand).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_damage(
    py: Python<'_>,
    target_id: i32,
    attacker_id: Option<i32>,
    damage: i32,
    dflags: i32,
    means_of_death: i32,
) -> PyResult<Option<bool>> {
    let target_player = if (0..MAX_CLIENTS as i32).contains(&target_id) {
        Player::py_new(target_id, None)?.into_py(py)
    } else {
        target_id.into_py(py)
    };

    let attacker_player = attacker_id.and_then(|attacker_id| {
        if (0..MAX_CLIENTS as i32).contains(&attacker_id) {
            Player::py_new(attacker_id, None)
                .ok()
                .map(|player| player.into_py(py))
        } else {
            Some(attacker_id.into_py(py))
        }
    });

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let damage_dispatcher = shinqlx_event_dispatchers.get_item("damage")?;
    let _ = damage_dispatcher.call_method1(
        "dispatch",
        (
            target_player,
            attacker_player,
            damage,
            dflags,
            means_of_death,
        ),
    )?;
    Ok(None)
}

#[pyfunction]
#[pyo3(signature = (target_id, attacker_id, damage, dflags, means_of_death))]
pub(crate) fn handle_damage(
    py: Python<'_>,
    target_id: i32,
    attacker_id: Option<i32>,
    damage: i32,
    dflags: i32,
    means_of_death: i32,
) -> Option<bool> {
    try_handle_damage(py, target_id, attacker_id, damage, dflags, means_of_death).unwrap_or_else(
        |e| {
            log_exception(py, e);
            Some(true)
        },
    )
}

#[cfg(test)]
#[cfg_attr(test, mockall::automock)]
#[allow(dead_code)]
#[allow(clippy::module_inception)]
pub(crate) mod handlers {
    use pyo3::prelude::*;

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_rcon<'a>(_py: Python<'a>, _cmd: String) -> Option<bool> {
        None
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_client_command<'a>(
        py: Python<'a>,
        _client_id: i32,
        _cmd: String,
    ) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_server_command<'a>(
        py: Python<'a>,
        _client_id: i32,
        _cmd: String,
    ) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_frame<'a>(_py: Python<'a>) -> Option<bool> {
        None
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_player_connect<'a>(
        py: Python<'a>,
        _client_id: i32,
        _is_bot: bool,
    ) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_player_loaded<'a>(py: Python<'a>, _client_id: i32) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_player_disconnect<'a>(
        py: Python<'a>,
        _client_id: i32,
        _reason: Option<String>,
    ) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_player_spawn<'a>(py: Python<'a>, _client_id: i32) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_kamikaze_use<'a>(py: Python<'a>, _client_id: i32) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_kamikaze_explode<'a>(
        py: Python<'a>,
        _client_id: i32,
        _is_used_on_demand: bool,
    ) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_damage<'a>(
        _py: Python<'a>,
        _target_id: i32,
        _attacker_id: Option<i32>,
        _damage: i32,
        _dflags: i32,
        _means_of_death: i32,
    ) -> Option<bool> {
        None
    }
}
