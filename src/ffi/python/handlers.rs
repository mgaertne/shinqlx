use super::prelude::*;
use crate::ffi::c::prelude::*;

use super::{
    late_init, log_exception, pyshinqlx_get_logger, set_map_subtitles, BLUE_TEAM_CHAT_CHANNEL,
    CHAT_CHANNEL, COMMANDS, CONSOLE_CHANNEL, EVENT_DISPATCHERS, FREE_CHAT_CHANNEL,
    RED_TEAM_CHAT_CHANNEL, SPECTATOR_CHAT_CHANNEL,
};
use crate::{quake_live_engine::GetConfigstring, MAIN_ENGINE};

use alloc::sync::Arc;
use arc_swap::ArcSwapOption;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use itertools::Itertools;
use once_cell::sync::Lazy;
use pyo3::{
    exceptions::{PyEnvironmentError, PyValueError},
    intern,
    types::{IntoPyDict, PyDict},
    PyTraverseError, PyVisit,
};
use regex::{Regex, RegexBuilder};

fn try_handle_rcon(py: Python<'_>, cmd: &str) -> PyResult<Option<bool>> {
    let rcon_dummy_player = Py::new(py, RconDummyPlayer::py_new())?;
    let player = rcon_dummy_player.borrow(py).into_super().into_super();

    let shinqlx_console_channel = CONSOLE_CHANNEL
        .load()
        .as_ref()
        .map_or(py.None(), |channel| channel.bind(py).into_py(py));

    if let Some(ref commands) = *COMMANDS.load() {
        commands
            .borrow(py)
            .handle_input(py, &player, cmd, shinqlx_console_channel)?;
    }
    Ok(None)
}

/// Console commands that are to be processed as regular pyshinqlx
/// commands as if the owner executes it. This allows the owner to
/// interact with the Python part of shinqlx without having to connect.
#[pyfunction]
pub(crate) fn handle_rcon(py: Python<'_>, cmd: &str) -> Option<bool> {
    try_handle_rcon(py, cmd).unwrap_or_else(|e| {
        log_exception(py, &e);
        Some(true)
    })
}

static RE_SAY: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^say +"?(?P<msg>.+)"?$"#)
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_SAY_TEAM: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^say_team +"?(?P<msg>.+)"?$"#)
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_CALLVOTE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^(?:cv|callvote) +(?P<cmd>[^ ]+)(?: "?(?P<args>.+?)"?)?$"#)
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_VOTE: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r"^vote +(?P<arg>.)")
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_TEAM: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r"^team +(?P<arg>.)")
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_USERINFO: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^userinfo "(?P<vars>.+)"$"#)
        .multi_line(true)
        .build()
        .unwrap()
});

fn is_vote_active() -> bool {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return false;
    };
    !main_engine
        .get_configstring(CS_VOTE_STRING as u16)
        .is_empty()
}

fn try_handle_client_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let Some(server_command_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "client_command"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to client command dispatcher",
        ));
    };

    let return_value =
        server_command_dispatcher.call_method1(intern!(py, "dispatch"), (player.clone(), cmd))?;
    if return_value.extract::<bool>().is_ok_and(|value| !value) {
        return Ok(false.into_py(py));
    };

    let updated_cmd = match return_value.extract::<String>() {
        Ok(extracted_string) => extracted_string,
        _ => cmd.to_string(),
    };

    if let Some(captures) = RE_SAY.captures(&updated_cmd) {
        if let Some(msg) = captures.name("msg") {
            let reformatted_msg = msg.as_str().replace('"', "");
            if let Some(ref main_chat_channel) = *CHAT_CHANNEL.load() {
                let Some(chat_dispatcher) =
                    EVENT_DISPATCHERS
                        .load()
                        .as_ref()
                        .and_then(|event_dispatchers| {
                            event_dispatchers
                                .bind(py)
                                .get_item(intern!(py, "chat"))
                                .ok()
                        })
                else {
                    return Err(PyEnvironmentError::new_err(
                        "could not get access to chat dispatcher",
                    ));
                };
                let result = chat_dispatcher.call_method1(
                    intern!(py, "dispatch"),
                    (player.clone(), reformatted_msg, main_chat_channel.as_ref()),
                )?;
                if result.extract::<bool>().is_ok_and(|value| !value) {
                    return Ok(false.into_py(py));
                }
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(captures) = RE_SAY_TEAM.captures(&updated_cmd) {
        if let Some(msg) = captures.name("msg") {
            let reformatted_msg = msg.as_str().replace('"', "");
            let channel = match player.get_team(py)?.as_str() {
                "free" => &FREE_CHAT_CHANNEL,
                "red" => &RED_TEAM_CHAT_CHANNEL,
                "blue" => &BLUE_TEAM_CHAT_CHANNEL,
                _ => &SPECTATOR_CHAT_CHANNEL,
            };
            let Some(chat_dispatcher) =
                EVENT_DISPATCHERS
                    .load()
                    .as_ref()
                    .and_then(|event_dispatchers| {
                        event_dispatchers
                            .bind(py)
                            .get_item(intern!(py, "chat"))
                            .ok()
                    })
            else {
                return Err(PyEnvironmentError::new_err(
                    "could not get access to chat dispatcher",
                ));
            };
            let Some(ref chat_channel) = *channel.load() else {
                return Err(PyEnvironmentError::new_err(
                    "could not get access to team chat channel",
                ));
            };
            let result = chat_dispatcher.call_method1(
                intern!(py, "dispatch"),
                (player.clone(), reformatted_msg, chat_channel.bind(py)),
            )?;
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
                let Some(vote_started_dispatcher) =
                    EVENT_DISPATCHERS
                        .load()
                        .as_ref()
                        .and_then(|event_dispatchers| {
                            event_dispatchers
                                .bind(py)
                                .get_item(intern!(py, "vote_started"))
                                .ok()
                        })
                else {
                    return Err(PyEnvironmentError::new_err(
                        "could not get access to vote started dispatcher",
                    ));
                };
                vote_started_dispatcher.call_method1(intern!(py, "caller"), (player.clone(),))?;
                let Some(vote_called_dispatcher) =
                    EVENT_DISPATCHERS
                        .load()
                        .as_ref()
                        .and_then(|event_dispatchers| {
                            event_dispatchers
                                .bind(py)
                                .get_item(intern!(py, "vote_called"))
                                .ok()
                        })
                else {
                    return Err(PyEnvironmentError::new_err(
                        "could not get access to vote called dispatcher",
                    ));
                };
                let result = vote_called_dispatcher.call_method1(
                    intern!(py, "dispatch"),
                    (player.clone(), vote.as_str(), args),
                )?;
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
                    let Some(vote_dispatcher) =
                        EVENT_DISPATCHERS
                            .load()
                            .as_ref()
                            .and_then(|event_dispatchers| {
                                event_dispatchers
                                    .bind(py)
                                    .get_item(intern!(py, "vote"))
                                    .ok()
                            })
                    else {
                        return Err(PyEnvironmentError::new_err(
                            "could not get access to vote dispatcher",
                        ));
                    };
                    let result = vote_dispatcher
                        .call_method1(intern!(py, "dispatch"), (player.clone(), vote))?;
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
            let Some(team_switch_attempt_dispatcher) =
                EVENT_DISPATCHERS
                    .load()
                    .as_ref()
                    .and_then(|event_dispatchers| {
                        event_dispatchers
                            .bind(py)
                            .get_item(intern!(py, "team_switch_attempt"))
                            .ok()
                    })
            else {
                return Err(PyEnvironmentError::new_err(
                    "could not get access to team switch attempt dispatcher",
                ));
            };
            let result = team_switch_attempt_dispatcher.call_method1(
                intern!(py, "dispatch"),
                (player.clone(), current_team, target_team),
            )?;
            if result.extract::<bool>().is_ok_and(|value| !value) {
                return Ok(false.into_py(py));
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(captures) = RE_USERINFO.captures(&updated_cmd) {
        if let Some(vars) = captures.name("vars") {
            let new_info = parse_variables(vars.as_str());
            let old_info = parse_variables(&player.user_info);

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
                let Some(userinfo_dispatcher) =
                    EVENT_DISPATCHERS
                        .load()
                        .as_ref()
                        .and_then(|event_dispatchers| {
                            event_dispatchers
                                .bind(py)
                                .get_item(intern!(py, "userinfo"))
                                .ok()
                        })
                else {
                    return Err(PyEnvironmentError::new_err(
                        "could not get access to userinfo dispatcher",
                    ));
                };
                let result = userinfo_dispatcher.call_method1(
                    intern!(py, "dispatch"),
                    (player.clone(), &changed.into_py_dict_bound(py)),
                )?;
                if result.extract::<bool>().is_ok_and(|value| !value) {
                    return Ok(false.into_py(py));
                }
                if let Ok(changed_values) = result.extract::<Bound<'_, PyDict>>() {
                    let updated_info = new_info.into_py_dict_bound(py);
                    updated_info.update(changed_values.to_owned().as_mapping())?;
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
pub(crate) fn handle_client_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyObject {
    try_handle_client_command(py, client_id, cmd).unwrap_or_else(|e| {
        log_exception(py, &e);
        true.into_py(py)
    })
}

static RE_VOTE_ENDED: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new("^print \"Vote (?P<result>passed|failed)\\.\"\n$")
        .multi_line(true)
        .build()
        .unwrap()
});

fn try_handle_server_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyResult<PyObject> {
    let Some(player) = (if (0..MAX_CLIENTS as i32).contains(&client_id) {
        Player::py_new(client_id, None)
            .map(|player| player.into_py(py))
            .ok()
    } else {
        Some(py.None())
    }) else {
        return Ok(true.into_py(py));
    };

    let Some(server_command_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "server_command"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to server command dispatcher",
        ));
    };

    let return_value =
        server_command_dispatcher.call_method1(intern!(py, "dispatch"), (player, cmd))?;
    if return_value.extract::<bool>().is_ok_and(|value| !value) {
        return Ok(false.into_py(py));
    };

    let updated_cmd = match return_value.extract::<String>() {
        Ok(extracted_string) => extracted_string,
        _ => cmd.to_string(),
    };

    if let Some(captures) = RE_VOTE_ENDED.captures(&updated_cmd) {
        let vote_passed = captures
            .name("result")
            .is_some_and(|value| value.as_str() == "passed");
        let Some(vote_ended_dispatcher) =
            EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers
                        .bind(py)
                        .get_item(intern!(py, "vote_ended"))
                        .ok()
                })
        else {
            return Err(PyEnvironmentError::new_err(
                "could not get access to vote ended dispatcher",
            ));
        };

        let _ = vote_ended_dispatcher.call_method1(intern!(py, "dispatch"), (vote_passed,))?;
    }

    Ok(updated_cmd.into_py(py))
}

#[pyfunction]
pub(crate) fn handle_server_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyObject {
    try_handle_server_command(py, client_id, cmd).unwrap_or_else(|e| {
        log_exception(py, &e);
        true.into_py(py)
    })
}

fn try_run_frame_tasks(py: Python<'_>) -> PyResult<()> {
    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    let frame_tasks = shinqlx_module.getattr(intern!(py, "frame_tasks"))?;
    frame_tasks.call_method(
        intern!(py, "run"),
        (),
        Some(&[(intern!(py, "blocking"), false)].into_py_dict_bound(py)),
    )?;

    Ok(())
}

fn try_handle_frame(py: Python<'_>) -> PyResult<()> {
    let Some(frame_dispatcher) = EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "frame"))
                .ok()
        })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to frame dispatcher",
        ));
    };
    frame_dispatcher.call_method0(intern!(py, "dispatch"))?;

    Ok(())
}

fn run_next_frame_tasks(py: Python<'_>) {
    match PyModule::from_code_bound(
        py,
        r#"
from shinqlx import next_frame_tasks, frame_tasks

def next_frame_tasks_runner():
    while not next_frame_tasks.empty():
        func, args, kwargs = next_frame_tasks.get_nowait()
        frame_tasks.enter(0, 1, func, args, kwargs)
"#,
        "",
        "",
    ) {
        Err(e) => log_exception(py, &e),
        Ok(next_frame_tasks_runner) => {
            if let Err(e) = next_frame_tasks_runner.call_method0("next_frame_tasks_runner") {
                log_exception(py, &e);
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
        log_exception(py, &e);
    }

    if let Err(e) = try_handle_frame(py) {
        log_exception(py, &e);
        return Some(true);
    }

    run_next_frame_tasks(py);

    None
}

static ZMQ_WARNING_ISSUED: AtomicBool = AtomicBool::new(false);
static IS_FIRST_GAME: AtomicBool = AtomicBool::new(true);

fn try_handle_new_game(py: Python<'_>, is_restart: bool) -> PyResult<()> {
    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    if IS_FIRST_GAME.load(Ordering::SeqCst) {
        late_init(&shinqlx_module, py)?;
        IS_FIRST_GAME.store(false, Ordering::SeqCst);

        let zmq_enabled_cvar = pyshinqlx_get_cvar(py, "zmq_stats_enable")?;
        let zmq_enabled = zmq_enabled_cvar.is_some_and(|value| value != "0");
        if !zmq_enabled && !ZMQ_WARNING_ISSUED.load(Ordering::SeqCst) {
            let logger = pyshinqlx_get_logger(py, None)?;
            let logging_module = py.import_bound(intern!(py, "logging"))?;
            let warning_level = logging_module.getattr(intern!(py, "WARNING"))?;
            let log_record = logger.call_method(
                intern!(py, "makeRecord"),
                (
                    intern!(py, "shinqlx"),
                    warning_level,
                    intern!(py, ""),
                    -1,
                    intern!(py, r#"Some events will not work because ZMQ stats is not enabled. Launch the server with "zmq_stats_enable 1""#),
                    py.None(),
                    py.None(),
                ),
                Some(
                    &[(intern!(py, "func"), intern!(py, "handle_new_game"))].into_py_dict_bound(py),
                ),
            )?;
            logger.call_method1(intern!(py, "handle"), (log_record,))?;

            ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);
        }
    }

    set_map_subtitles(&shinqlx_module)?;

    if !is_restart {
        let map_name = pyshinqlx_get_cvar(py, "mapname")?;
        let factory_name = pyshinqlx_get_cvar(py, "g_factory")?;
        let Some(map_dispatcher) =
            EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers.bind(py).get_item(intern!(py, "map")).ok()
                })
        else {
            return Err(PyEnvironmentError::new_err(
                "could not get access to map dispatcher",
            ));
        };
        map_dispatcher.call_method1(intern!(py, "dispatch"), (map_name, factory_name))?;
    }

    let Some(new_game_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "new_game"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to new game dispatcher",
        ));
    };
    new_game_dispatcher.call_method0(intern!(py, "dispatch"))?;

    Ok(())
}

#[pyfunction]
pub(crate) fn handle_new_game(py: Python<'_>, is_restart: bool) -> Option<bool> {
    if let Err(e) = try_handle_new_game(py, is_restart) {
        log_exception(py, &e);
        return Some(true);
    }

    None
}

static AD_ROUND_NUMBER: AtomicI32 = AtomicI32::new(0);

fn try_handle_set_configstring(py: Python<'_>, index: u32, value: &str) -> PyResult<PyObject> {
    let Some(set_configstring_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "set_configstring"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to set configstring dispatcher",
        ));
    };
    let result = set_configstring_dispatcher
        .call_method1(intern!(py, "dispatch"), (index.into_py(py), value))?;

    if result
        .extract::<bool>()
        .is_ok_and(|result_value| !result_value)
    {
        return Ok(false.into_py(py));
    }

    let configstring_value = result.extract::<String>().unwrap_or(value.to_string());
    match index {
        CS_VOTE_STRING => {
            if !configstring_value.is_empty() {
                let (vote, args) = configstring_value
                    .split_once(' ')
                    .unwrap_or((configstring_value.as_str(), ""));
                let Some(vote_started_dispatcher) =
                    EVENT_DISPATCHERS
                        .load()
                        .as_ref()
                        .and_then(|event_dispatchers| {
                            event_dispatchers
                                .bind(py)
                                .get_item(intern!(py, "vote_started"))
                                .ok()
                        })
                else {
                    return Err(PyEnvironmentError::new_err(
                        "could not get access to vote started dispatcher",
                    ));
                };
                vote_started_dispatcher.call_method1(intern!(py, "dispatch"), (vote, args))?;
                Ok(py.None())
            } else {
                Ok(configstring_value.into_py(py))
            }
        }
        CS_SERVERINFO => {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Ok(py.None());
            };
            let old_configstring = main_engine.get_configstring(CS_SERVERINFO as u16);
            if old_configstring.is_empty() {
                return Ok(py.None());
            }
            let old_cvars = parse_variables(&old_configstring);
            let opt_old_state = old_cvars.get("g_gameState");
            let old_state = opt_old_state.as_deref().unwrap_or("");

            let new_cvars = parse_variables(&configstring_value);
            let opt_new_state = new_cvars.get("g_gameState");
            let new_state = opt_new_state.as_deref().unwrap_or("");

            if old_state == new_state {
                return Ok(configstring_value.into_py(py));
            }
            match (old_state, new_state) {
                ("PRE_GAME", "IN_PROGRESS") => {}
                ("PRE_GAME", "COUNT_DOWN") => {
                    AD_ROUND_NUMBER.store(1, Ordering::SeqCst);
                    let Some(game_countdown_dispatcher) = EVENT_DISPATCHERS
                        .load()
                        .as_ref()
                        .and_then(|event_dispatchers| {
                            event_dispatchers
                                .bind(py)
                                .get_item(intern!(py, "game_countdown"))
                                .ok()
                        })
                    else {
                        return Err(PyEnvironmentError::new_err(
                            "could not get access to game countdown dispatcher",
                        ));
                    };
                    game_countdown_dispatcher.call_method0(intern!(py, "dispatch"))?;
                }
                ("COUNT_DOWN", "IN_PROGRESS") => {}
                ("IN_PROGRESS", "PRE_GAME") => {}
                ("COUNT_DOWN", "PRE_GAME") => {}
                _ => {
                    let logger = pyshinqlx_get_logger(py, None)?;
                    let warning = format!("UNKNOWN GAME STATES: {old_state} - {new_state}");
                    let logging_module = py.import_bound(intern!(py, "logging"))?;
                    let warning_level = logging_module.getattr(intern!(py, "WARNING"))?;
                    let log_record = logger.call_method(
                        intern!(py, "makeRecord"),
                        (
                            intern!(py, "shinqlx"),
                            warning_level,
                            intern!(py, ""),
                            -1,
                            warning,
                            py.None(),
                            py.None(),
                        ),
                        Some(
                            &[(intern!(py, "func"), intern!(py, "handle_set_configstring"))]
                                .into_py_dict_bound(py),
                        ),
                    )?;
                    logger.call_method1(intern!(py, "handle"), (log_record,))?;
                }
            }
            Ok(configstring_value.into_py(py))
        }
        CS_ROUND_STATUS => {
            let cvars = parse_variables(&configstring_value);
            if cvars.is_empty() {
                return Ok(configstring_value.into_py(py));
            }

            let opt_round = cvars
                .get("round")
                .and_then(|value| value.parse::<i32>().ok());
            let opt_turn = cvars
                .get("turn")
                .and_then(|value| value.parse::<i32>().ok());
            let opt_time = cvars.get("time");

            let opt_round_number = if opt_turn.is_some() {
                if cvars
                    .get("state")
                    .is_some_and(|value| value.parse::<i32>().is_ok_and(|value| value == 0))
                {
                    return Ok(py.None());
                }

                if opt_round.is_some() {
                    let ad_round_number =
                        opt_round.unwrap_or_default() * 2 + 1 + opt_turn.unwrap_or_default();
                    AD_ROUND_NUMBER.store(ad_round_number, Ordering::SeqCst);
                }
                Some(AD_ROUND_NUMBER.load(Ordering::SeqCst))
            } else {
                opt_round
            };

            if opt_round_number.is_some() {
                let round_number = opt_round_number.unwrap();
                let event = match opt_time {
                    Some(_) => intern!(py, "round_countdown"),
                    None => intern!(py, "round_start"),
                };

                let Some(round_discpatcher) = EVENT_DISPATCHERS
                    .load()
                    .as_ref()
                    .and_then(|event_dispatchers| event_dispatchers.bind(py).get_item(event).ok())
                else {
                    return Err(PyEnvironmentError::new_err(
                        "could not get access to round countdown/start dispatcher",
                    ));
                };
                round_discpatcher.call_method1(intern!(py, "dispatch"), (round_number,))?;
                return Ok(py.None());
            }

            Ok(configstring_value.into_py(py))
        }
        _ => Ok(configstring_value.into_py(py)),
    }
}

/// Called whenever the server tries to set a configstring. Can return
/// False to stop the event.
#[pyfunction]
pub(crate) fn handle_set_configstring(py: Python<'_>, index: u32, value: &str) -> PyObject {
    try_handle_set_configstring(py, index, value).unwrap_or_else(|e| {
        log_exception(py, &e);
        true.into_py(py)
    })
}

fn try_handle_player_connect(py: Python<'_>, client_id: i32, _is_bot: bool) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let Some(player_connect_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "player_connect"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to player connect dispatcher",
        ));
    };
    player_connect_dispatcher
        .call_method1(intern!(py, "dispatch"), (player,))
        .map(|value| value.into_py(py))
}

/// This will be called whenever a player tries to connect. If the dispatcher
/// returns False, it will not allow the player to connect and instead show them
/// a message explaining why. The default message is "You are banned from this
/// server.", but it can be set with :func:`shinqlx.set_ban_message`.
#[pyfunction]
pub(crate) fn handle_player_connect(py: Python<'_>, client_id: i32, is_bot: bool) -> PyObject {
    try_handle_player_connect(py, client_id, is_bot).unwrap_or_else(|e| {
        log_exception(py, &e);
        true.into_py(py)
    })
}

fn try_handle_player_loaded(py: Python<'_>, client_id: i32) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let Some(player_loaded_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "player_loaded"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to player loaded dispatcher",
        ));
    };
    player_loaded_dispatcher
        .call_method1(intern!(py, "dispatch"), (player,))
        .map(|value| value.into_py(py))
}

/// This will be called whenever a player has connected and finished loading,
/// meaning it'll go off a bit later than the usual "X connected" messages.
/// This will not trigger on bots.his will be called whenever a player tries to connect. If the dispatcher
#[pyfunction]
pub(crate) fn handle_player_loaded(py: Python<'_>, client_id: i32) -> PyObject {
    try_handle_player_loaded(py, client_id).unwrap_or_else(|e| {
        log_exception(py, &e);
        true.into_py(py)
    })
}

fn try_handle_player_disconnect(
    py: Python<'_>,
    client_id: i32,
    reason: Option<String>,
) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let Some(player_disconnect_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "player_disconnect"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to player disconnect dispatcher",
        ));
    };
    player_disconnect_dispatcher
        .call_method1(intern!(py, "dispatch"), (player, reason))
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
        log_exception(py, &e);
        true.into_py(py)
    })
}

fn try_handle_player_spawn(py: Python<'_>, client_id: i32) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let Some(player_spawn_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "player_spawn"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to player spawn dispatcher",
        ));
    };
    player_spawn_dispatcher
        .call_method1(intern!(py, "dispatch"), (player,))
        .map(|value| value.into_py(py))
}

/// Called when a player spawns. Note that a spectator going in free spectate mode
/// makes the client spawn, so you'll want to check for that if you only want "actual"
/// spawns.
#[pyfunction]
pub(crate) fn handle_player_spawn(py: Python<'_>, client_id: i32) -> PyObject {
    try_handle_player_spawn(py, client_id).unwrap_or_else(|e| {
        log_exception(py, &e);
        true.into_py(py)
    })
}

fn try_handle_kamikaze_use(py: Python<'_>, client_id: i32) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let Some(kamikaze_use_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "kamikaze_use"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to kamikaze use dispatcher",
        ));
    };
    kamikaze_use_dispatcher
        .call_method1(intern!(py, "dispatch"), (player,))
        .map(|value| value.into_py(py))
}

/// This will be called whenever player uses kamikaze item.
#[pyfunction]
pub(crate) fn handle_kamikaze_use(py: Python<'_>, client_id: i32) -> PyObject {
    try_handle_kamikaze_use(py, client_id).unwrap_or_else(|e| {
        log_exception(py, &e);
        true.into_py(py)
    })
}

fn try_handle_kamikaze_explode(
    py: Python<'_>,
    client_id: i32,
    is_used_on_demand: bool,
) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let Some(kamikaze_explode_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "kamikaze_explode"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to kamikaze explode dispatcher",
        ));
    };
    kamikaze_explode_dispatcher
        .call_method1(intern!(py, "dispatch"), (player, is_used_on_demand))
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
        log_exception(py, &e);
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

    let Some(damage_dispatcher) = EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "damage"))
                .ok()
        })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to damage dispatcher",
        ));
    };
    let _ = damage_dispatcher.call_method1(
        intern!(py, "dispatch"),
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
            log_exception(py, &e);
            Some(true)
        },
    )
}

static PRINT_REDIRECTION: Lazy<ArcSwapOption<Py<PyAny>>> = Lazy::new(ArcSwapOption::empty);

fn try_handle_console_print(py: Python<'_>, text: &str) -> PyResult<PyObject> {
    let logger = pyshinqlx_get_logger(py, None)?;
    let console_text = text;
    let logging_module = py.import_bound(intern!(py, "logging"))?;
    let debug_level = logging_module.getattr(intern!(py, "DEBUG"))?;
    let log_record = logger.call_method(
        intern!(py, "makeRecord"),
        (
            intern!(py, "shinqlx"),
            debug_level,
            intern!(py, ""),
            -1,
            console_text.trim_end_matches('\n'),
            py.None(),
            py.None(),
        ),
        Some(&[(intern!(py, "func"), intern!(py, "handle_console_print"))].into_py_dict_bound(py)),
    )?;
    logger.call_method1(intern!(py, "handle"), (log_record,))?;

    let Some(console_print_dispatcher) =
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "console_print"))
                    .ok()
            })
    else {
        return Err(PyEnvironmentError::new_err(
            "could not get access to console print dispatcher",
        ));
    };
    let result = console_print_dispatcher.call_method1(intern!(py, "dispatch"), (console_text,))?;
    if result.extract::<bool>().is_ok_and(|value| !value) {
        return Ok(false.into_py(py));
    }

    PRINT_REDIRECTION
        .load()
        .iter()
        .for_each(|print_redirector| {
            if let Err(e) = print_redirector.call_method1(py, intern!(py, "append"), (text,)) {
                log_exception(py, &e);
            }
        });

    let returned = result.extract::<String>().unwrap_or(text.to_string());
    Ok(returned.into_py(py))
}

/// Called whenever the server prints something to the console and when rcon is used.
#[pyfunction]
pub(crate) fn handle_console_print(py: Python<'_>, text: &str) -> PyObject {
    if text.is_empty() {
        return py.None();
    }

    try_handle_console_print(py, text).unwrap_or_else(|e| {
        log_exception(py, &e);
        true.into_py(py)
    })
}

#[pyclass(module = "_handlers", name = "PrintRedirector")]
pub(crate) struct PrintRedirector {
    channel: PyObject,
    print_buffer: parking_lot::RwLock<String>,
}

#[pymethods]
impl PrintRedirector {
    #[new]
    fn py_new(py: Python<'_>, channel: PyObject) -> PyResult<PrintRedirector> {
        if !channel.bind(py).is_instance_of::<AbstractChannel>() {
            return Err(PyValueError::new_err(
                "The redirection channel must be an instance of shinqlx.AbstractChannel.",
            ));
        }

        Ok(PrintRedirector {
            channel,
            print_buffer: parking_lot::RwLock::new(String::with_capacity(MAX_MSG_LENGTH as usize)),
        })
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.channel)?;
        Ok(())
    }

    #[pyo3(name = "__enter__")]
    fn context_manager_enter(slf: PyRef<'_, Self>, py: Python<'_>) {
        PRINT_REDIRECTION.store(Some(Arc::new(slf.into_py(py))));
    }

    #[pyo3(name = "__exit__")]
    #[allow(unused_variables)]
    fn context_manager_exit(
        &self,
        py: Python<'_>,
        exc_type: Py<PyAny>,
        exc_value: Py<PyAny>,
        exc_traceback: Py<PyAny>,
    ) -> PyResult<()> {
        self.flush(py)?;
        PRINT_REDIRECTION.store(None);
        Ok(())
    }

    fn flush(&self, py: Python<'_>) -> PyResult<()> {
        let mut print_buffer_guard = self.print_buffer.write();
        let print_buffer_contents = print_buffer_guard.clone();
        print_buffer_guard.clear();

        let _ = self
            .channel
            .call_method1(py, intern!(py, "reply"), (print_buffer_contents,))?;

        Ok(())
    }

    fn append(&self, text: &str) {
        let mut print_buffer_guard = self.print_buffer.write();
        (*print_buffer_guard).push_str(text);
    }
}

/// Redirects print output to a channel. Useful for commands that execute console commands
/// and want to redirect the output to the channel instead of letting it go to the console.
///
/// To use it, use the return value with the "with" statement.
///
/// .. code-block:: python
///     def cmd_echo(self, player, msg, channel):
///         with shinqlx.redirect_print(channel):
///             shinqlx.console_command("echo {}".format(" ".join(msg)))
#[pyfunction]
pub(crate) fn redirect_print(py: Python<'_>, channel: PyObject) -> PyResult<PrintRedirector> {
    PrintRedirector::py_new(py, channel)
}

#[pyfunction]
pub(crate) fn register_handlers() {}

#[cfg(test)]
#[cfg_attr(test, mockall::automock)]
#[allow(dead_code)]
#[allow(clippy::module_inception)]
pub(crate) mod handlers {
    use pyo3::prelude::*;

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_rcon<'a>(_py: Python<'a>, _cmd: &str) -> Option<bool> {
        None
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_client_command<'a>(
        py: Python<'a>,
        _client_id: i32,
        _cmd: &str,
    ) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_server_command<'a>(
        py: Python<'a>,
        _client_id: i32,
        _cmd: &str,
    ) -> PyObject {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_frame<'a>(_py: Python<'a>) -> Option<bool> {
        None
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_new_game<'a>(_py: Python<'a>, _is_restart: bool) -> Option<bool> {
        None
    }

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_set_configstring<'a>(
        py: Python<'a>,
        _index: u32,
        _value: &str,
    ) -> PyObject {
        py.None()
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

    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn handle_console_print<'a>(py: Python<'a>, _text: &str) -> PyObject {
        py.None()
    }

    pub(crate) fn register_handlers() {}
}
