use crate::ffi::c::prelude::*;

use super::prelude::{
    parse_variables, pyshinqlx_get_cvar, AbstractChannel, Player, RconDummyPlayer, MAX_MSG_LENGTH,
};
use super::{
    is_vote_active, late_init, log_exception, pyshinqlx_get_logger, set_map_subtitles,
    BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, COMMANDS, CONSOLE_CHANNEL, EVENT_DISPATCHERS,
    FREE_CHAT_CHANNEL, RED_TEAM_CHAT_CHANNEL, SPECTATOR_CHAT_CHANNEL,
};
use crate::{quake_live_engine::GetConfigstring, MAIN_ENGINE};

use pyo3::prelude::*;
use pyo3::{
    exceptions::{PyEnvironmentError, PyValueError},
    intern,
    types::{IntoPyDict, PyDict},
    PyTraverseError, PyVisit,
};

use alloc::sync::Arc;
use arc_swap::ArcSwapOption;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};

fn try_handle_rcon(py: Python<'_>, cmd: &str) -> PyResult<Option<bool>> {
    COMMANDS.load().as_ref().map_or(Ok(None), |commands| {
        let rcon_dummy_player = Py::new(py, RconDummyPlayer::py_new())?;
        let player = rcon_dummy_player.borrow(py).into_super().into_super();

        let shinqlx_console_channel = CONSOLE_CHANNEL
            .load()
            .as_ref()
            .map_or(py.None(), |channel| channel.bind(py).into_py(py));

        commands
            .borrow(py)
            .handle_input(py, &player, cmd, shinqlx_console_channel)
            .map(|_| None)
    })
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

#[cfg(test)]
mod handle_rcon_tests {
    use super::handler_test_support::*;
    use super::{handle_rcon, try_handle_rcon};

    use crate::ffi::python::prelude::*;
    use crate::ffi::python::{
        commands::{Command, CommandPriorities},
        COMMANDS, EVENT_DISPATCHERS,
    };

    use crate::MAIN_ENGINE;

    use pyo3::prelude::*;

    use crate::prelude::serial;

    pub(super) fn failing_test_handler_module(py: Python<'_>) -> Bound<'_, PyModule> {
        PyModule::from_code_bound(
            py,
            r#"
called = False

def handler(*args):
    global called
    called = True
    raise Exception("please ignore this")
        "#,
            "",
            "",
        )
        .expect("could create test handler module")
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_rcon_with_no_commands() {
        COMMANDS.store(None);
        EVENT_DISPATCHERS.store(None);

        Python::with_gil(|py| {
            let result = try_handle_rcon(py, "asdf");
            assert!(result.is_ok_and(|value| value.is_none()));
        })
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_rcon_with_command_invoker_in_place() {
        MAIN_ENGINE.store(None);

        let command_invoker = CommandInvoker::py_new();

        Python::with_gil(|py| {
            let plugin = test_plugin(py);
            let capturing_hook = capturing_hook(py);
            let cmd_handler = capturing_hook
                .getattr("hook")
                .expect("could not get handler from test module");
            let command = Command::py_new(
                py,
                plugin.unbind(),
                "asdf".into_py(py),
                cmd_handler.unbind(),
                0,
                py.None(),
                py.None(),
                false,
                0,
                false,
                "",
            )
            .expect("could not create command");
            command_invoker
                .add_command(py, command, CommandPriorities::PRI_NORMAL as usize)
                .expect("could not add command to command invoker");
            COMMANDS.store(Some(
                Py::new(py, command_invoker)
                    .expect("could not create CommandInvoker in Python")
                    .into(),
            ));
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                .expect("could not add command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_rcon(py, "asdf");
            assert!(result.is_ok_and(|value| value.is_none()));
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", ["asdf"], "_"))
                .is_ok());
        })
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_rcon_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        EVENT_DISPATCHERS.store(None);

        let command_invoker = CommandInvoker::py_new();

        Python::with_gil(|py| {
            let plugin = test_plugin(py);
            let cmd_handler_module = failing_test_handler_module(py);
            let cmd_handler = cmd_handler_module
                .getattr("handler")
                .expect("could not get handler from test module");
            let command = Command::py_new(
                py,
                plugin.unbind(),
                "asdf".into_py(py),
                cmd_handler.unbind(),
                0,
                py.None(),
                py.None(),
                false,
                0,
                false,
                "",
            )
            .expect("could not create command");
            command_invoker
                .add_command(py, command, CommandPriorities::PRI_NORMAL as usize)
                .expect("could not add command to command invoker");
            COMMANDS.store(Some(
                Py::new(py, command_invoker)
                    .expect("could not create CommandInvoker in Python")
                    .into(),
            ));

            let result = handle_rcon(py, "asdf");
            assert!(result.is_some_and(|value| value));
        });
    }
}

static RE_SAY: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^say +"?(?P<msg>.+)"$"#)
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_SAY_TEAM: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^say_team +"?(?P<msg>.+)"$"#)
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

fn try_handle_client_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let return_value = EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "client_command"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to client command dispatcher",
            )),
            |client_command_dispatcher| {
                client_command_dispatcher
                    .call_method1(intern!(py, "dispatch"), (player.clone(), cmd))
            },
        )?;
    if return_value.extract::<bool>().is_ok_and(|value| !value) {
        return Ok(false.into_py(py));
    };

    let updated_cmd = match return_value.extract::<String>() {
        Ok(extracted_string) => extracted_string,
        _ => cmd.to_string(),
    };

    if let Some(captures) = RE_SAY.captures(&updated_cmd) {
        if let Some(msg) = captures.name("msg") {
            let reformatted_msg = msg.as_str().replace('"', "'").replace('%', "％");
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
                    (player.clone(), &reformatted_msg, main_chat_channel.as_ref()),
                )?;
                if result.extract::<bool>().is_ok_and(|value| !value) {
                    return Ok(false.into_py(py));
                }
            }
            let forwarded_cmd = format!("say \"{reformatted_msg}\"");
            return Ok(forwarded_cmd.into_py(py));
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(captures) = RE_SAY_TEAM.captures(&updated_cmd) {
        if let Some(msg) = captures.name("msg") {
            let reformatted_msg = msg.as_str().replace('"', "'").replace('%', "％");
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
                (player.clone(), &reformatted_msg, chat_channel.bind(py)),
            )?;
            if result.extract::<bool>().is_ok_and(|value| !value) {
                return Ok(false.into_py(py));
            }
            let forwarded_cmd = format!("say_team \"{reformatted_msg}\"");
            return Ok(forwarded_cmd.into_py(py));
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

#[cfg(test)]
mod handle_client_command_tests {
    use super::handler_test_support::{
        capturing_hook, returning_false_hook, returning_other_string_hook,
    };
    use super::try_handle_client_command;

    use crate::ffi::c::{
        game_entity::MockGameEntity,
        prelude::{clientState_t, cvar_t, privileges_t, team_t, CVar, CVarBuilder, MockClient},
    };
    use crate::ffi::python::{
        channels::TeamChatChannel,
        commands::CommandPriorities,
        events::{ChatEventDispatcher, ClientCommandDispatcher, EventDispatcherManager},
        pyshinqlx_setup_fixture::pyshinqlx_setup,
        BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, EVENT_DISPATCHERS, FREE_CHAT_CHANNEL,
        RED_TEAM_CHAT_CHANNEL, SPECTATOR_CHAT_CHANNEL,
    };
    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use arc_swap::ArcSwapOption;
    use core::ffi::c_char;

    use mockall::predicate;
    use once_cell::sync::Lazy;
    use rstest::rstest;

    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_client_command_only() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("1".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let client_command_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("client_command")
                        .expect("could not get client_command dispatcher")
                })
                .expect("could not get client_command dispatcher");

            client_command_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook"),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to client_command dispatcher");

            let result = try_handle_client_command(py, 42, "cp \"asdf\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "cp \"asdf\"")));
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", "cp \"asdf\"",))
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_client_command_with_no_event_dispatchers() {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            EVENT_DISPATCHERS.store(None);

            let result = try_handle_client_command(py, 42, "cp \"asdf\"");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_client_command_only_when_dispatcher_returns_false(
        _pyshinqlx_setup: (),
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("1".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let client_command_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("client_command")
                        .expect("could not get client_command dispatcher")
                })
                .expect("could not get client_command dispatcher");

            client_command_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_false_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to client_command dispatcher");

            let result = try_handle_client_command(py, 42, "cp \"asdf\"");
            assert!(result.is_ok_and(|value| value
                .extract::<bool>(py)
                .is_ok_and(|bool_value| !bool_value)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_client_command_only_when_dispatcher_returns_other_client_command(
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("1".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let client_command_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("client_command")
                        .expect("could not get client_command dispatcher")
                })
                .expect("could not get client_command dispatcher");
            client_command_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_other_string_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to client_command dispatcher");

            let result = try_handle_client_command(py, 42, "cp \"asdf\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "quit")));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_msg_send() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("1".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            CHAT_CHANNEL.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                )
                .expect("could not create TeamChatchannel in python")
                .into(),
            ));

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ChatEventDispatcher>())
                .expect("could not add chat dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let chat_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("chat")
                        .expect("could not get chat dispatcher")
                })
                .expect("could not get chat dispatcher");

            chat_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        capturing_hook
                            .getattr("hook")
                            .expect("could not get hook from capturing hook"),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to client_command dispatcher");

            let result = try_handle_client_command(py, 42, "say \"test with \"quotation marks\"\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "say \"test with 'quotation marks'\"")),);
            assert!(capturing_hook
                .call_method1(
                    "assert_called_with",
                    ("_", "test with 'quotation marks'", "_"),
                )
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_msg_with_no_chat_dispatcher() {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            CHAT_CHANNEL.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                )
                .expect("could not create TeamChatchannel in python")
                .into(),
            ));

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_client_command(py, 42, "say \"hi @all\"");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_msg_when_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("1".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            CHAT_CHANNEL.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                )
                .expect("could not create TeamChatchannel in python")
                .into(),
            ));

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ChatEventDispatcher>())
                .expect("could not add chat dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let chat_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("chat")
                        .expect("could not get chat dispatcher")
                })
                .expect("could not get chat dispatcher");

            chat_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_false_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to client_command dispatcher");

            let result = try_handle_client_command(py, 42, "say \"hi @all\"");
            assert!(result.is_ok_and(|value| value
                .extract::<bool>(py)
                .is_ok_and(|bool_value| !bool_value)));
        });
    }

    #[rstest]
    #[case(team_t::TEAM_FREE, "free", "free_chat", &FREE_CHAT_CHANNEL)]
    #[case(team_t::TEAM_RED, "red", "red_team_chat", &RED_TEAM_CHAT_CHANNEL)]
    #[case(team_t::TEAM_BLUE, "blue", "blue_team_chat", &BLUE_TEAM_CHAT_CHANNEL)]
    #[case(team_t::TEAM_SPECTATOR, "spectator", "spectator_chat", &SPECTATOR_CHAT_CHANNEL)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_msg_send(
        #[case] team: team_t,
        #[case] team_str: &str,
        #[case] team_name: &str,
        #[case] channel: &Lazy<ArcSwapOption<Py<TeamChatChannel>>>,
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("1".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(move |_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity.expect_get_team().returning(move || team);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            channel.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new(team_str, team_name, "print \"{}\n\"\n"),
                )
                .expect("could not create TeamChatchannel in python")
                .into(),
            ));

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ChatEventDispatcher>())
                .expect("could not add chat dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let chat_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("chat")
                        .expect("could not get chat dispatcher")
                })
                .expect("could not get chat dispatcher");

            chat_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        capturing_hook
                            .getattr("hook")
                            .expect("could not get hook from capturing hook"),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to client_command dispatcher");

            let result =
                try_handle_client_command(py, 42, "say_team \"test with \"quotation marks\"\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "say_team \"test with 'quotation marks'\"")));
            assert!(capturing_hook
                .call_method1(
                    "assert_called_with",
                    ("_", "test with 'quotation marks'", "_"),
                )
                .is_ok());
        });
    }

    #[rstest]
    #[case(team_t::TEAM_FREE, &FREE_CHAT_CHANNEL)]
    #[case(team_t::TEAM_RED, &RED_TEAM_CHAT_CHANNEL)]
    #[case(team_t::TEAM_BLUE, &BLUE_TEAM_CHAT_CHANNEL)]
    #[case(team_t::TEAM_SPECTATOR, &SPECTATOR_CHAT_CHANNEL)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_msg_with_no_team_channel(
        #[case] team: team_t,
        #[case] channel: &Lazy<ArcSwapOption<Py<TeamChatChannel>>>,
    ) {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(move |_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity.expect_get_team().returning(move || team);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            channel.store(None);

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ChatEventDispatcher>())
                .expect("could not add chat dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result =
                try_handle_client_command(py, 42, "say_team \"test with \"quotation marks\"\"");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[case(team_t::TEAM_FREE, "free", "free_chat", &FREE_CHAT_CHANNEL)]
    #[case(team_t::TEAM_RED, "red", "red_team_chat", &RED_TEAM_CHAT_CHANNEL)]
    #[case(team_t::TEAM_BLUE, "blue", "blue_team_chat", &BLUE_TEAM_CHAT_CHANNEL)]
    #[case(team_t::TEAM_SPECTATOR, "spectator", "spectator_chat", &SPECTATOR_CHAT_CHANNEL)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_msg_with_no_chat_dispatcher(
        #[case] team: team_t,
        #[case] team_str: &str,
        #[case] team_name: &str,
        #[case] channel: &Lazy<ArcSwapOption<Py<TeamChatChannel>>>,
    ) {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(move |_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity.expect_get_team().returning(move || team);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            channel.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new(team_str, team_name, "print \"{}\n\"\n"),
                )
                .expect("could not create TeamChatchannel in python")
                .into(),
            ));

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_client_command(py, 42, "say_team \"hi @all\"");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[case(team_t::TEAM_FREE, "free", "free_chat", &FREE_CHAT_CHANNEL)]
    #[case(team_t::TEAM_RED, "red", "red_team_chat", &RED_TEAM_CHAT_CHANNEL)]
    #[case(team_t::TEAM_BLUE, "blue", "blue_team_chat", &BLUE_TEAM_CHAT_CHANNEL)]
    #[case(team_t::TEAM_SPECTATOR, "spectator", "spectator_chat", &SPECTATOR_CHAT_CHANNEL)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_msg_when_dispatcher_returns_false(
        #[case] team: team_t,
        #[case] team_str: &str,
        #[case] team_name: &str,
        #[case] channel: &Lazy<ArcSwapOption<Py<TeamChatChannel>>>,
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("1".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(42))
            .returning(move |_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity.expect_get_team().returning(move || team);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            channel.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new(team_str, team_name, "print \"{}\n\"\n"),
                )
                .expect("could not create TeamChatchannel in python")
                .into(),
            ));

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())
                .expect("could not add client_command dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ChatEventDispatcher>())
                .expect("could not add chat dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let chat_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("chat")
                        .expect("could not get chat dispatcher")
                })
                .expect("could not get chat dispatcher");

            chat_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_false_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to client_command dispatcher");

            let result = try_handle_client_command(py, 42, "say_team \"hi @all\"");
            assert!(result.is_ok_and(|value| value
                .extract::<bool>(py)
                .is_ok_and(|bool_value| !bool_value)));
        });
    }
}

static RE_VOTE_ENDED: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^print "Vote (?P<result>passed|failed)\.\n"$"#)
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
                    .and_then(|value| value.parse::<i32>().ok())
                    .is_some_and(|value| value == 0)
                {
                    return Ok(py.None());
                }

                if let Some(round_number) = opt_round {
                    let ad_round_number = round_number * 2 + 1 + opt_turn.unwrap_or_default();
                    AD_ROUND_NUMBER.store(ad_round_number, Ordering::SeqCst);
                }
                Some(AD_ROUND_NUMBER.load(Ordering::SeqCst))
            } else {
                if opt_round.is_some_and(|value| value == 0) {
                    return Ok(configstring_value.into_py(py));
                }
                opt_round
            };

            if let Some(round_number) = opt_round_number {
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

#[cfg(test)]
mod handler_test_support {
    use pyo3::prelude::*;

    pub(super) fn test_plugin(py: Python<'_>) -> Bound<'_, PyAny> {
        PyModule::from_code_bound(
            py,
            r#"
import shinqlx

class test_plugin(shinqlx.Plugin):
    pass
"#,
            "",
            "",
        )
        .expect("coult not create test plugin")
        .getattr("test_plugin")
        .expect("could not get test plugin")
    }

    pub(super) fn capturing_hook(py: Python<'_>) -> Bound<'_, PyModule> {
        PyModule::from_code_bound(
            py,
            r#"
called = False
_args = None

def hook(*args):
    global called
    called = True
    global _args
    _args = args

def assert_called_with(*args):
    global called
    assert called

    global _args
    assert len(args) == len(_args), f"{len(args) = } == {len(_args) = }"
    for (expected, actual) in zip(args, _args):
        if expected == "_":
            continue
        assert expected == actual, f"{expected = } == {actual = }"
        "#,
            "",
            "",
        )
        .expect("could create test handler module")
    }

    pub(super) fn returning_false_hook(py: Python<'_>) -> Bound<'_, PyAny> {
        let returning_false_module = PyModule::from_code_bound(
            py,
            r#"
import shinqlx

def returning_false_hook(*args, **kwargs):
    return shinqlx.RET_STOP_EVENT
            "#,
            "",
            "",
        )
        .expect("could not create returning false module");
        returning_false_module
            .getattr("returning_false_hook")
            .expect("could not get returning_false_hook function")
    }

    pub(super) fn returning_other_string_hook(py: Python<'_>) -> Bound<'_, PyAny> {
        let returning_other_string_module = PyModule::from_code_bound(
            py,
            r#"
def returning_other_string(*args, **kwargs):
    return "quit"
            "#,
            "",
            "",
        )
        .expect("could not create returning false module");
        returning_other_string_module
            .getattr("returning_other_string")
            .expect("could not get returning_false_hook function")
    }
}
