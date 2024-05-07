use crate::ffi::c::prelude::*;

use super::prelude::{
    parse_variables, pyshinqlx_get_cvar, AbstractChannel, Player, RconDummyPlayer,
    VoteStartedDispatcher, MAX_MSG_LENGTH,
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
    types::{IntoPyDict, PyBool, PyDict},
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
    use crate::prelude::serial;
    use crate::MAIN_ENGINE;

    use pyo3::prelude::*;

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

        Python::with_gil(|py| {
            let result = handle_rcon(py, "asdf");
            assert!(result.is_some_and(|value| value));
        });
    }
}

static RE_SAY: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^say +(?P<quote>"?)(?P<msg>.+)$"#)
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_SAY_TEAM: Lazy<Regex> = Lazy::new(|| {
    RegexBuilder::new(r#"^say_team +(?P<quote>"?)(?P<msg>.+)$"#)
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
    if return_value
        .extract::<&PyBool>()
        .is_ok_and(|value| !value.is_true())
    {
        return Ok(false.into_py(py));
    };

    let updated_cmd = return_value.extract::<&str>().unwrap_or(cmd);

    if let Some(reformatted_msg) = RE_SAY.captures(updated_cmd).and_then(|captures| {
        captures.name("msg").map(|msg| {
            captures
                .name("quote")
                .filter(|value| !value.as_str().is_empty())
                .map(|quote| {
                    msg.as_str()
                        .strip_suffix(quote.as_str())
                        .unwrap_or(msg.as_str())
                })
                .unwrap_or(msg.as_str())
                .replace('"', "'")
        })
    }) {
        let result = EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "chat"))
                    .ok()
            })
            .map_or(
                Err(PyEnvironmentError::new_err(
                    "could not get access to chat dispatcher",
                )),
                |chat_dispatcher| {
                    let Some(ref main_chat_channel) = *CHAT_CHANNEL.load() else {
                        return Err(PyEnvironmentError::new_err(
                            "could not get access to main chat channel",
                        ));
                    };

                    chat_dispatcher.call_method1(
                        intern!(py, "dispatch"),
                        (player.clone(), &reformatted_msg, main_chat_channel.as_ref()),
                    )
                },
            )?;

        if result
            .extract::<&PyBool>()
            .is_ok_and(|value| !value.is_true())
        {
            return Ok(false.into_py(py));
        }
        return Ok(format!("say \"{reformatted_msg}\"").into_py(py));
    }

    if let Some(reformatted_msg) = RE_SAY_TEAM.captures(updated_cmd).and_then(|captures| {
        captures.name("msg").map(|msg| {
            captures
                .name("quote")
                .filter(|value| !value.as_str().is_empty())
                .map(|quote| {
                    msg.as_str()
                        .strip_suffix(quote.as_str())
                        .unwrap_or(msg.as_str())
                })
                .unwrap_or(msg.as_str())
                .replace('"', "'")
        })
    }) {
        let result = EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "chat"))
                    .ok()
            })
            .map_or(
                Err(PyEnvironmentError::new_err(
                    "could not get access to chat dispatcher",
                )),
                |chat_dispatcher| {
                    let channel = match player.get_team(py)?.as_str() {
                        "free" => &FREE_CHAT_CHANNEL,
                        "red" => &RED_TEAM_CHAT_CHANNEL,
                        "blue" => &BLUE_TEAM_CHAT_CHANNEL,
                        _ => &SPECTATOR_CHAT_CHANNEL,
                    };
                    let Some(ref chat_channel) = *channel.load() else {
                        return Err(PyEnvironmentError::new_err(
                            "could not get access to team chat channel",
                        ));
                    };
                    chat_dispatcher.call_method1(
                        intern!(py, "dispatch"),
                        (player.clone(), &reformatted_msg, chat_channel.bind(py)),
                    )
                },
            )?;
        if result
            .extract::<&PyBool>()
            .is_ok_and(|value| !value.is_true())
        {
            return Ok(false.into_py(py));
        }
        return Ok(format!("say_team \"{reformatted_msg}\"").into_py(py));
    }

    if let Some((vote, args)) = RE_CALLVOTE.captures(updated_cmd).and_then(|captures| {
        captures.name("cmd").map(|vote_cmd| {
            (
                vote_cmd,
                captures
                    .name("args")
                    .map(|matched| matched.as_str())
                    .unwrap_or(""),
            )
        })
    }) {
        if !is_vote_active() {
            EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers
                        .bind(py)
                        .get_item(intern!(py, "vote_started"))
                        .and_then(|dispatcher| dispatcher.extract::<VoteStartedDispatcher>())
                        .ok()
                })
                .map_or(
                    Err(PyEnvironmentError::new_err(
                        "could not get access to vote started dispatcher",
                    )),
                    |mut vote_started_dispatcher| {
                        vote_started_dispatcher.caller(py, player.clone().into_py(py));
                        Ok(())
                    },
                )?;
            let result = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers
                        .bind(py)
                        .get_item(intern!(py, "vote_called"))
                        .ok()
                })
                .map_or(
                    Err(PyEnvironmentError::new_err(
                        "could not get access to vote called dispatcher",
                    )),
                    |vote_called_dispatcher| {
                        vote_called_dispatcher.call_method1(
                            intern!(py, "dispatch"),
                            (player.clone(), vote.as_str(), args),
                        )
                    },
                )?;
            if result
                .extract::<&PyBool>()
                .is_ok_and(|value| !value.is_true())
            {
                return Ok(false.into_py(py));
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(arg) = RE_VOTE
        .captures(updated_cmd)
        .and_then(|captures| captures.name("arg"))
    {
        if is_vote_active() && ["y", "Y", "1", "n", "N", "2"].contains(&arg.as_str()) {
            let vote = ["y", "Y", "1"].contains(&arg.as_str());
            let result = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers
                        .bind(py)
                        .get_item(intern!(py, "vote"))
                        .ok()
                })
                .map_or(
                    Err(PyEnvironmentError::new_err(
                        "could not get access to vote dispatcher",
                    )),
                    |vote_dispatcher| {
                        vote_dispatcher
                            .call_method1(intern!(py, "dispatch"), (player.clone(), vote))
                    },
                )?;
            if result
                .extract::<&PyBool>()
                .is_ok_and(|value| !value.is_true())
            {
                return Ok(false.into_py(py));
            }
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(arg) = RE_TEAM
        .captures(updated_cmd)
        .and_then(|captures| captures.name("arg"))
    {
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
        let result = EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "team_switch_attempt"))
                    .ok()
            })
            .map_or(
                Err(PyEnvironmentError::new_err(
                    "could not get access to team switch attempt dispatcher",
                )),
                |team_switch_attempt_dispatcher| {
                    team_switch_attempt_dispatcher.call_method1(
                        intern!(py, "dispatch"),
                        (player.clone(), current_team, target_team),
                    )
                },
            )?;
        if result
            .extract::<&PyBool>()
            .is_ok_and(|value| !value.is_true())
        {
            return Ok(false.into_py(py));
        }
        return Ok(updated_cmd.into_py(py));
    }

    if let Some(vars) = RE_USERINFO
        .captures(updated_cmd)
        .and_then(|captures| captures.name("vars"))
    {
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
            let result = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers
                        .bind(py)
                        .get_item(intern!(py, "userinfo"))
                        .ok()
                })
                .map_or(
                    Err(PyEnvironmentError::new_err(
                        "could not get access to userinfo dispatcher",
                    )),
                    |userinfo_dispatcher| {
                        userinfo_dispatcher.call_method1(
                            intern!(py, "dispatch"),
                            (player.clone(), &changed.into_py_dict_bound(py)),
                        )
                    },
                )?;
            if result
                .extract::<&PyBool>()
                .is_ok_and(|value| !value.is_true())
            {
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
    use super::{handle_client_command, try_handle_client_command};

    use crate::ffi::c::{
        game_entity::MockGameEntity,
        prelude::{
            clientState_t, cvar_t, privileges_t, team_t, CVar, CVarBuilder, MockClient,
            CS_VOTE_STRING,
        },
    };
    use crate::ffi::python::{
        channels::TeamChatChannel,
        commands::CommandPriorities,
        events::{
            ChatEventDispatcher, ClientCommandDispatcher, EventDispatcherManager,
            TeamSwitchAttemptDispatcher, UserinfoDispatcher, VoteCalledDispatcher, VoteDispatcher,
            VoteStartedDispatcher,
        },
        pyshinqlx_setup_fixture::pyshinqlx_setup,
        BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, EVENT_DISPATCHERS, FREE_CHAT_CHANNEL,
        RED_TEAM_CHAT_CHANNEL, SPECTATOR_CHAT_CHANNEL,
    };
    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use arc_swap::ArcSwapOption;
    use core::ffi::c_char;
    use once_cell::sync::Lazy;

    use mockall::predicate;
    use rstest::rstest;

    use pyo3::prelude::*;
    use pyo3::{
        exceptions::{PyAssertionError, PyEnvironmentError},
        types::{IntoPyDict, PyBool},
    };

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
                .extract::<&PyBool>(py)
                .is_ok_and(|bool_value| !bool_value.is_true())));
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

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_msg_with_no_chat_channel() {
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
            CHAT_CHANNEL.store(None);

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
                .extract::<&PyBool>(py)
                .is_ok_and(|bool_value| !bool_value.is_true())));
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
            assert!(result.is_ok_and(|value| {
                value
                    .extract::<String>(py)
                    .is_ok_and(|str_value| str_value == "say_team \"test with 'quotation marks'\"")
            }));
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
                .extract::<&PyBool>(py)
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote() {
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
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "".into());
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteCalledDispatcher>())
                .expect("could not add vote_called dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteStartedDispatcher>())
                .expect("could not add vote_started dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_called_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote_called")
                        .expect("could not get chat dispatcher")
                })
                .expect("could not get chat dispatcher");

            vote_called_dispatcher
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

            let result = try_handle_client_command(py, 42, "callvote map \"thunderstruck\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "callvote map \"thunderstruck\"")),);
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", "map", "thunderstruck"))
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote_when_vote_is_already_running() {
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
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "allready".into());
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteCalledDispatcher>())
                .expect("could not add vote_called dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteStartedDispatcher>())
                .expect("could not add vote_started dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_called_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote_called")
                        .expect("could not get vote_called dispatcher")
                })
                .expect("could not get vote_called dispatcher");

            vote_called_dispatcher
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
                .expect("could not add hook to vote_called dispatcher");

            let result = try_handle_client_command(py, 42, "callvote map \"thunderstruck\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "callvote map \"thunderstruck\"")),);
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", "_", "_"))
                .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote_with_no_vote_called_dispatcher() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "".into());
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteStartedDispatcher>())
                .expect("could not add vote_started dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_client_command(py, 42, "cv restart");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote_with_no_vote_started_dispatcher() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "".into());
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteCalledDispatcher>())
                .expect("could not add vote_called dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_client_command(py, 42, "cv restart");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote_when_dispatcher_returns_false(_pyshinqlx_setup: ()) {
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
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "".into());
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteCalledDispatcher>())
                .expect("could not add vote_called dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteStartedDispatcher>())
                .expect("could not add vote_started dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let vote_called_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote_called")
                        .expect("could not get vote_called dispatcher")
                })
                .expect("could not get vote_called dispatcher");

            vote_called_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_false_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to client_command dispatcher");

            let result = try_handle_client_command(py, 42, "callvote map \"thunderstruck\"");
            assert!(result.is_ok_and(|value| value
                .extract::<&PyBool>(py)
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });
    }

    #[rstest]
    #[case("y", true)]
    #[case("Y", true)]
    #[case("1", true)]
    #[case("n", false)]
    #[case("N", false)]
    #[case("2", false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_vote_command(#[case] vote_arg: &str, #[case] vote: bool) {
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
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck".into());
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteDispatcher>())
                .expect("could not add vote dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote")
                        .expect("could not get vote dispatcher")
                })
                .expect("could not get vote dispatcher");

            vote_dispatcher
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
                .expect("could not add hook to vote dispatcher");

            let client_command = format!("vote {vote_arg}");
            let result = try_handle_client_command(py, 42, &client_command);
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == client_command)),);
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", vote,))
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_vote_command_for_unhandled_vote() {
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
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck".into());
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteDispatcher>())
                .expect("could not add vote dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote")
                        .expect("could not get vote dispatcher")
                })
                .expect("could not get vote dispatcher");

            vote_dispatcher
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
                .expect("could not add hook to vote dispatcher");

            let result = try_handle_client_command(py, 42, "vote 3");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "vote 3")),);
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", "_",))
                .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py)));
        });
    }

    #[rstest]
    #[case("y")]
    #[case("Y")]
    #[case("1")]
    #[case("n")]
    #[case("N")]
    #[case("2")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_vote_command_when_no_vote_running(#[case] vote_arg: &str) {
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
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "".into());
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteDispatcher>())
                .expect("could not add vote dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote")
                        .expect("could not get vote dispatcher")
                })
                .expect("could not get vote dispatcher");

            vote_dispatcher
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
                .expect("could not add hook to vote dispatcher");

            let client_command = format!("vote {vote_arg}");
            let result = try_handle_client_command(py, 42, &client_command);
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == client_command)),);
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", "_",))
                .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_vote_command_with_no_vote_dispatcher() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck".into());
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

            let result = try_handle_client_command(py, 42, "vote 1");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[case("y")]
    #[case("Y")]
    #[case("1")]
    #[case("n")]
    #[case("N")]
    #[case("2")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_vote_command_when_dispatcher_returns_false(
        #[case] vote_arg: &str,
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
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck".into());
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteDispatcher>())
                .expect("could not add vote dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let vote_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote")
                        .expect("could not get vote dispatcher")
                })
                .expect("could not get vote dispatcher");

            vote_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_false_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to vote dispatcher");

            let client_command = format!("vote {vote_arg}");
            let result = try_handle_client_command(py, 42, &client_command);
            assert!(result.is_ok_and(|value| value
                .extract::<&PyBool>(py)
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });
    }

    #[rstest]
    #[case("f", "free", team_t::TEAM_SPECTATOR)]
    #[case("r", "red", team_t::TEAM_SPECTATOR)]
    #[case("b", "blue", team_t::TEAM_SPECTATOR)]
    #[case("s", "spectator", team_t::TEAM_RED)]
    #[case("a", "any", team_t::TEAM_SPECTATOR)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_switch_command(
        #[case] team_char: &str,
        #[case] team_str: &str,
        #[case] player_team: team_t,
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
            .returning(move |_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(move || player_team);
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<TeamSwitchAttemptDispatcher>())
                .expect("could not add team_switch_attempt dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let team_switch_attempt_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("team_switch_attempt")
                        .expect("could not get team_switch_attempt dispatcher")
                })
                .expect("could not get team_switch_attmpt dispatcher");

            team_switch_attempt_dispatcher
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
                .expect("could not add hook to team_switch_attempt dispatcher");

            let client_command = format!("team {team_char}");
            let result = try_handle_client_command(py, 42, &client_command);
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == client_command)),);
            let current_team = match player_team {
                team_t::TEAM_SPECTATOR => "spectator",
                team_t::TEAM_RED => "red",
                team_t::TEAM_BLUE => "blue",
                team_t::TEAM_FREE => "free",
                _ => "invalid team",
            };
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", current_team, team_str))
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_switch_for_unhandled_team() {
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<TeamSwitchAttemptDispatcher>())
                .expect("could not add team_switch_attempt dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let team_switch_attempt_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("team_switch_attempt")
                        .expect("could not get team_switch_attempt dispatcher")
                })
                .expect("could not get team_switch_attempt dispatcher");

            team_switch_attempt_dispatcher
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
                .expect("could not add hook to team_switch_attempt dispatcher");

            let result = try_handle_client_command(py, 42, "team c");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "team c")),);
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", "_",))
                .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py)));
        });
    }

    #[rstest]
    #[case("f", team_t::TEAM_FREE)]
    #[case("s", team_t::TEAM_SPECTATOR)]
    #[case("r", team_t::TEAM_RED)]
    #[case("b", team_t::TEAM_BLUE)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_switch_when_player_already_on_target_team(
        #[case] team_char: &str,
        #[case] player_team: team_t,
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
            .returning(move |_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(move || player_team);
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<TeamSwitchAttemptDispatcher>())
                .expect("could not add team_switch_attempt dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let team_switch_attempt_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("team_switch_attempt")
                        .expect("could not get team_switch_attempt dispatcher")
                })
                .expect("could not get team_switch_attempt dispatcher");

            team_switch_attempt_dispatcher
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
                .expect("could not add hook to team_switch_attempt dispatcher");

            let client_command = format!("team {team_char}");
            let result = try_handle_client_command(py, 42, &client_command);
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == client_command)),);
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", "_", "_",))
                .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_switch_attempt_command_with_no_dispatcher() {
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

            let result = try_handle_client_command(py, 42, "team a");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[case("f", team_t::TEAM_SPECTATOR)]
    #[case("r", team_t::TEAM_SPECTATOR)]
    #[case("b", team_t::TEAM_SPECTATOR)]
    #[case("s", team_t::TEAM_RED)]
    #[case("a", team_t::TEAM_SPECTATOR)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_switch_attempt_command_when_dispatcher_returns_false(
        #[case] team_char: &str,
        #[case] player_team: team_t,
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
            .returning(move |_client_id| {
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
                mock_game_entity
                    .expect_get_team()
                    .returning(move || player_team);
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<TeamSwitchAttemptDispatcher>())
                .expect("could not add team_switch_attempt dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let team_switch_attempt_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("team_switch_attempt")
                        .expect("could not get team_switch_attempt dispatcher")
                })
                .expect("could not get team_switch_attempt dispatcher");

            team_switch_attempt_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_false_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to team_switch_attempt dispatcher");

            let client_command = format!("team {team_char}");
            let result = try_handle_client_command(py, 42, &client_command);
            assert!(result.is_ok_and(|value| value
                .extract::<&PyBool>(py)
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_when_nothing_changed() {
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
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
                mock_game_entity
                    .expect_get_team()
                    .returning(move || team_t::TEAM_RED);
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<UserinfoDispatcher>())
                .expect("could not add userinfo dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let userinfo_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("userinfo")
                        .expect("could not get userinfo dispatcher")
                })
                .expect("could not get userinfo dispatcher");

            userinfo_dispatcher
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
                .expect("could not add hook to userinfo dispatcher");

            let result =
                try_handle_client_command(py, 42, r#"userinfo "\name\Mocked Player\sex\male""#);
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == r#"userinfo "\name\Mocked Player\sex\male""#)),);
            assert!(capturing_hook
                .call_method1("assert_called_with", ("_", "_",))
                .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_with_changes() {
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
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
                mock_game_entity
                    .expect_get_team()
                    .returning(move || team_t::TEAM_RED);
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<UserinfoDispatcher>())
                .expect("could not add userinfo dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let userinfo_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("userinfo")
                        .expect("could not get userinfo dispatcher")
                })
                .expect("could not get userinfo dispatcher");

            userinfo_dispatcher
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
                .expect("could not add hook to userinfo dispatcher");

            let result =
                try_handle_client_command(py, 42, r#"userinfo "\name\Mocked Player\sex\female""#);
            assert!(
                result.is_ok_and(|value| value.extract::<String>(py).is_ok_and(
                    |str_value| str_value == r#"userinfo "\name\Mocked Player\sex\female""#
                )),
            );
            assert!(capturing_hook
                .call_method1(
                    "assert_called_with",
                    ("_", [("sex", "female"),].into_py_dict_bound(py),)
                )
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_with_no_event_dispatcher() {
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
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
                mock_game_entity
                    .expect_get_team()
                    .returning(move || team_t::TEAM_RED);
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

            let result =
                try_handle_client_command(py, 42, r#"userinfo "\name\Mocked Player\sex\female""#);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_with_dispatcher_returns_false() {
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
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
                mock_game_entity
                    .expect_get_team()
                    .returning(move || team_t::TEAM_RED);
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<UserinfoDispatcher>())
                .expect("could not add userinfo dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let userinfo_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("userinfo")
                        .expect("could not get userinfo dispatcher")
                })
                .expect("could not get userinfo dispatcher");

            userinfo_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_false_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to userinfo dispatcher");

            let result =
                try_handle_client_command(py, 42, r#"userinfo "\name\Mocked Player\sex\female""#);
            assert!(result.is_ok_and(|value| value
                .extract::<&PyBool>(py)
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_with_dispatcher_returns_other_userinfo() {
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
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
                mock_game_entity
                    .expect_get_team()
                    .returning(move || team_t::TEAM_RED);
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
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<UserinfoDispatcher>())
                .expect("could not add userinfo dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let returning_other_userinfo_module = PyModule::from_code_bound(
                py,
                r#"
def returning_other_userinfo_hook(*args, **kwargs):
    return {"name": "Changed Player", "sex": "male", "country": "GB"}
                "#,
                "",
                "",
            )
            .expect("could not create returning other userinfo module");
            let returning_other_userinfo = returning_other_userinfo_module
                .getattr("returning_other_userinfo_hook")
                .expect("could not get returning_other_userinfo_hook function");
            let userinfo_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("userinfo")
                        .expect("could not get userinfo dispatcher")
                })
                .expect("could not get userinfo dispatcher");

            userinfo_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_other_userinfo,
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to userinfo dispatcher");

            let result =
                try_handle_client_command(py, 42, r#"userinfo "\name\Mocked Player\sex\female""#);
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value
                    == r#"userinfo "\name\Changed Player\sex\male\country\GB""#)),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_client_command_with_no_event_dispatchers() {
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
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
                mock_game_entity
                    .expect_get_team()
                    .returning(move || team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        EVENT_DISPATCHERS.store(None);

        Python::with_gil(|py| {
            let result = handle_client_command(py, 42, "asdf");
            assert!(result
                .downcast_bound::<PyBool>(py)
                .is_ok_and(|value| value.is_true()));
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
    let Ok(player) = (0..MAX_CLIENTS as i32)
        .find(|&id| id == client_id)
        .map_or(Ok(py.None()), |id| {
            Player::py_new(id, None).map(|player| player.into_py(py))
        })
    else {
        return Ok(true.into_py(py));
    };

    let return_value = EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "server_command"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to server command dispatcher",
            )),
            |server_command_dispatcher| {
                server_command_dispatcher.call_method1(intern!(py, "dispatch"), (player, cmd))
            },
        )?;
    if return_value
        .extract::<&PyBool>()
        .is_ok_and(|value| !value.is_true())
    {
        return Ok(false.into_py(py));
    };

    let updated_cmd = return_value.extract::<&str>().unwrap_or(cmd);

    RE_VOTE_ENDED
        .captures(updated_cmd)
        .map_or(Ok(updated_cmd.into_py(py)), |captures| {
            EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers
                        .bind(py)
                        .get_item(intern!(py, "vote_ended"))
                        .ok()
                })
                .map_or(
                    Err(PyEnvironmentError::new_err(
                        "could not get access to vote ended dispatcher",
                    )),
                    |vote_ended_dispatcher| {
                        let vote_passed = captures
                            .name("result")
                            .is_some_and(|value| value.as_str() == "passed");
                        vote_ended_dispatcher.call_method1(intern!(py, "dispatch"), (vote_passed,))
                    },
                )?;
            Ok(updated_cmd.into_py(py))
        })
}

#[pyfunction]
pub(crate) fn handle_server_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyObject {
    try_handle_server_command(py, client_id, cmd).unwrap_or_else(|e| {
        log_exception(py, &e);
        true.into_py(py)
    })
}

#[cfg(test)]
mod handle_server_command_tests {
    use super::handler_test_support::{
        capturing_hook, returning_false_hook, returning_other_string_hook,
    };
    use super::{handle_server_command, try_handle_server_command};

    use crate::ffi::c::{
        game_entity::MockGameEntity,
        prelude::{
            clientState_t, cvar_t, privileges_t, team_t, CVar, CVarBuilder, MockClient, CS_VOTE_NO,
            CS_VOTE_STRING, CS_VOTE_YES,
        },
    };
    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use crate::ffi::python::{
        commands::CommandPriorities,
        events::{EventDispatcherManager, ServerCommandDispatcher, VoteEndedDispatcher},
        EVENT_DISPATCHERS,
    };

    use core::ffi::c_char;
    use mockall::predicate;

    use pyo3::prelude::*;
    use pyo3::{exceptions::PyEnvironmentError, types::PyBool};

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_general_server_command() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ServerCommandDispatcher>())
                .expect("could not add server_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let server_command_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("server_command")
                        .expect("could not get server_command dispatcher")
                })
                .expect("could not get server_command dispatcher");
            server_command_dispatcher
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
                .expect("could not add hook to server_command dispatcher");

            let result = try_handle_server_command(py, -1, "cp \"asdf\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "cp \"asdf\"")));
            assert!(capturing_hook
                .call_method1("assert_called_with", (py.None(), "cp \"asdf\"",))
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_dedicated_player_server_command() {
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
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
                mock_game_entity
                    .expect_get_team()
                    .returning(move || team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ServerCommandDispatcher>())
                .expect("could not add server_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let server_command_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("server_command")
                        .expect("could not get server_command dispatcher")
                })
                .expect("could not get server_command dispatcher");
            server_command_dispatcher
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
                .expect("could not add hook to server_command dispatcher");

            let result = try_handle_server_command(py, 42, "cp \"asdf\"");
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
    fn handle_server_command_for_server_command_with_no_event_dispatcher() {
        EVENT_DISPATCHERS.store(None);

        Python::with_gil(|py| {
            let result = try_handle_server_command(py, -1, "cp \"asdf\"");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_server_command_returning_false() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ServerCommandDispatcher>())
                .expect("could not add server_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let server_command_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("server_command")
                        .expect("could not get server_command dispatcher")
                })
                .expect("could not get server_command dispatcher");
            server_command_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_false_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to server_command dispatcher");

            let result = try_handle_server_command(py, -1, "cp \"asdf\"");
            assert!(result.is_ok_and(|value| value
                .extract::<&PyBool>(py)
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_server_command_returning_other_string() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ServerCommandDispatcher>())
                .expect("could not add server_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let server_command_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("server_command")
                        .expect("could not get server_command dispatcher")
                })
                .expect("could not get server_command dispatcher");
            server_command_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_other_string_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to server_command dispatcher");

            let result = try_handle_server_command(py, -1, "cp \"asdf\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "quit")));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_indicating_vote_passed() {
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
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "42".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "1".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ServerCommandDispatcher>())
                .expect("could not add server_command dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteEndedDispatcher>())
                .expect("could not add vote_ended dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_ended_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote_ended")
                        .expect("could not get vote_ended dispatcher")
                })
                .expect("could not get vote_ended dispatcher");
            vote_ended_dispatcher
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
                .expect("could not add hook to vote_enede dispatcher");

            let result = try_handle_server_command(py, -1, "print \"Vote passed.\n\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "print \"Vote passed.\n\"")));
            assert!(capturing_hook
                .call_method1(
                    "assert_called_with",
                    ((42, 1), "map", "thunderstruck", true,)
                )
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_indicating_vote_failed() {
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
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .returning(|_| "map thunderstruck".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .returning(|_| "1".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .returning(|_| "42".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ServerCommandDispatcher>())
                .expect("could not add server_command dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteEndedDispatcher>())
                .expect("could not add vote_ended dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_ended_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote_ended")
                        .expect("could not get vote_ended dispatcher")
                })
                .expect("could not get vote_ended dispatcher");
            vote_ended_dispatcher
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
                .expect("could not add hook to vote_enede dispatcher");

            let result = try_handle_server_command(py, -1, "print \"Vote failed.\n\"");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "print \"Vote failed.\n\"")));
            assert!(capturing_hook
                .call_method1(
                    "assert_called_with",
                    ((1, 42), "map", "thunderstruck", false,)
                )
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_vote_ended_with_no_dispatcher() {
        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<ServerCommandDispatcher>())
                .expect("could not add server_command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_server_command(py, -1, "print \"Vote passed.\n\"");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_with_no_event_dispatchers() {
        EVENT_DISPATCHERS.store(None);
        Python::with_gil(|py| {
            let result = handle_server_command(py, -1, "asdf");
            assert!(result
                .downcast_bound::<PyBool>(py)
                .is_ok_and(|value| value.is_true()));
        });
    }
}

fn try_run_frame_tasks(py: Python<'_>) -> PyResult<()> {
    py.import_bound(intern!(py, "shinqlx"))
        .and_then(|shinqlx_module| shinqlx_module.getattr(intern!(py, "frame_tasks")))
        .and_then(|frame_tasks| {
            frame_tasks
                .call_method(
                    intern!(py, "run"),
                    (),
                    Some(&[(intern!(py, "blocking"), false)].into_py_dict_bound(py)),
                )
                .map(|_| ())
        })
}

fn try_handle_frame(py: Python<'_>) -> PyResult<()> {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "frame"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to frame dispatcher",
            )),
            |frame_dispatcher| {
                frame_dispatcher
                    .call_method0(intern!(py, "dispatch"))
                    .map(|_| Ok(()))
            },
        )?
}

fn transfer_next_frame_tasks(py: Python<'_>) {
    PyModule::from_code_bound(
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
    )
    .and_then(|next_frame_tasks_runner| {
        next_frame_tasks_runner.call_method0("next_frame_tasks_runner")?;
        Ok(())
    })
    .unwrap_or_else(|e| log_exception(py, &e));
}

/// This will be called every frame. To allow threads to call stuff from the
/// main thread, tasks can be scheduled using the :func:`shinqlx.next_frame` decorator
/// and have it be executed here.
#[pyfunction]
pub(crate) fn handle_frame(py: Python<'_>) -> Option<bool> {
    while let Err(e) = try_run_frame_tasks(py) {
        log_exception(py, &e);
    }

    let return_value = try_handle_frame(py).map_or_else(
        |e| {
            log_exception(py, &e);
            Some(true)
        },
        |_| None,
    );

    transfer_next_frame_tasks(py);

    return_value
}

#[cfg(test)]
mod handle_run_frame_tests {
    use super::handler_test_support::capturing_hook;
    use super::{handle_frame, transfer_next_frame_tasks, try_handle_frame, try_run_frame_tasks};

    use crate::ffi::python::{
        commands::CommandPriorities, events::FrameEventDispatcher, pyshinqlx_setup,
        EventDispatcherManager, EVENT_DISPATCHERS,
    };

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder};

    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use core::ffi::c_char;
    use mockall::predicate;

    use rstest::rstest;

    use pyo3::prelude::*;
    use pyo3::{
        exceptions::{PyEnvironmentError, PyValueError},
        types::{IntoPyDict, PyBool, PyDict, PyTuple},
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_run_frame_tasks_with_no_pending_tasks(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let shinqlx_module = py.import_bound("shinqlx").expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr("frame_tasks")
                .expect("this should not happen");
            py.run_bound(
                r#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(&[("frame_tasks", frame_tasks)].into_py_dict_bound(py)),
            )
            .expect("this should not happend");

            let result = try_run_frame_tasks(py);
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_run_frame_tasks_pending_task_throws_exception(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let shinqlx_module = py.import_bound("shinqlx").expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr("frame_tasks")
                .expect("this should not happen");
            py.run_bound(
                r#"
def throws_exception():
    raise ValueError("stop calling me!")

for event in frame_tasks.queue:
    frame_tasks.cancel(event)

frame_tasks.enter(0, 1, throws_exception, (), {})
"#,
                None,
                Some(&[("frame_tasks", frame_tasks)].into_py_dict_bound(py)),
            )
            .expect("this should not happend");

            let result = try_run_frame_tasks(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_run_frame_tasks_pending_task_succeeds(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let shinqlx_module = py.import_bound("shinqlx").expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr("frame_tasks")
                .expect("this should not happen");
            py.run_bound(
                r#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)

frame_tasks.enter(0, 1, capturing_hook, ("asdf", 42), {})
"#,
                None,
                Some(
                    &[
                        ("frame_tasks", frame_tasks),
                        (
                            "capturing_hook",
                            capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook"),
                        ),
                    ]
                    .into_py_dict_bound(py),
                ),
            )
            .expect("this should not happend");

            let result = try_run_frame_tasks(py);
            assert!(result.is_ok());
            assert!(capturing_hook
                .call_method1("assert_called_with", ("asdf", 42,))
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_frame_with_hook() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<FrameEventDispatcher>())
                .expect("could not add frame dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let frame_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("frame")
                        .expect("could not get frame dispatcher")
                })
                .expect("could not get frame dispatcher");
            frame_dispatcher
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
                .expect("could not add hook to frame dispatcher");

            let result = try_handle_frame(py);
            assert!(result.is_ok());
            assert!(capturing_hook
                .call_method1("assert_called_with", ())
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_frame_with_no_event_dispatchers() {
        Python::with_gil(|py| {
            EVENT_DISPATCHERS.store(None);

            let result = try_handle_frame(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn transfer_next_frame_tasks_with_none_pending(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let shinqlx_module = py.import_bound("shinqlx").expect("this should not happen");
            let next_frame_tasks = shinqlx_module
                .getattr("next_frame_tasks")
                .expect("this should not happen");
            py.run_bound(
                r#"
while not next_frame_tasks.empty():
    next_frame_tasks.get_nowait()
            "#,
                None,
                Some(&[("next_frame_tasks", next_frame_tasks.clone())].into_py_dict_bound(py)),
            )
            .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr("frame_tasks")
                .expect("this should not happen");
            py.run_bound(
                r#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(&[("frame_tasks", frame_tasks.clone())].into_py_dict_bound(py)),
            )
            .expect("this should not happend");

            transfer_next_frame_tasks(py);
            assert!(frame_tasks.call_method0("empty").is_ok_and(|value| value
                .extract::<&PyBool>()
                .expect("this should not happen")
                .is_true()));
            assert!(next_frame_tasks
                .call_method0("empty")
                .is_ok_and(|value| value
                    .extract::<&PyBool>()
                    .expect("this should not happen")
                    .is_true()));
            py.run_bound(
                r#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(&[("frame_tasks", frame_tasks.clone())].into_py_dict_bound(py)),
            )
            .expect("this should not happend");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn transfer_next_frame_tasks_with_pending_tasks_for_next_frame(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let shinqlx_module = py.import_bound("shinqlx").expect("this should not happen");
            let next_frame_tasks = shinqlx_module
                .getattr("next_frame_tasks")
                .expect("this should not happen");
            next_frame_tasks
                .call_method1(
                    "put_nowait",
                    ((
                        capturing_hook,
                        PyTuple::empty_bound(py),
                        PyDict::new_bound(py),
                    ),),
                )
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr("frame_tasks")
                .expect("this should not happen");
            py.run_bound(
                r#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(&[("frame_tasks", frame_tasks.clone())].into_py_dict_bound(py)),
            )
            .expect("this should not happend");

            transfer_next_frame_tasks(py);
            assert!(frame_tasks.call_method0("empty").is_ok_and(|value| !value
                .extract::<&PyBool>()
                .expect("this should not happen")
                .is_true()));
            assert!(next_frame_tasks
                .call_method0("empty")
                .is_ok_and(|value| value
                    .extract::<&PyBool>()
                    .expect("this should not happen")
                    .is_true()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_frame_when_frame_tasks_throws_exception(_pyshinqlx_setup: ()) {
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

        Python::with_gil(|py| {
            let shinqlx_module = py.import_bound("shinqlx").expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr("frame_tasks")
                .expect("this should not happen");
            py.run_bound(
                r#"
def throws_exception():
    raise ValueError("stop calling me!")

for event in frame_tasks.queue:
    frame_tasks.cancel(event)

frame_tasks.enter(0, 1, throws_exception, (), {})
"#,
                None,
                Some(&[("frame_tasks", frame_tasks)].into_py_dict_bound(py)),
            )
            .expect("this should not happend");

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<FrameEventDispatcher>())
                .expect("could not add frame dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));
            let capturing_hook = capturing_hook(py);
            let frame_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("frame")
                        .expect("could not get frame dispatcher")
                })
                .expect("could not get frame dispatcher");
            frame_dispatcher
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
                .expect("could not add hook to frame dispatcher");

            let result = handle_frame(py);
            assert!(result.is_none());
            assert!(capturing_hook
                .call_method1("assert_called_with", ())
                .is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_frame_when_frame_handler_throws_exception(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let shinqlx_module = py.import_bound("shinqlx").expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr("frame_tasks")
                .expect("this should not happen");
            py.run_bound(
                r#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(&[("frame_tasks", frame_tasks)].into_py_dict_bound(py)),
            )
            .expect("this should not happend");

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<FrameEventDispatcher>())
                .expect("could not add frame dispatcher");
            EVENT_DISPATCHERS.store(None);

            let result = handle_frame(py);
            assert!(result.is_some_and(|value| value));
        });
    }
}

static ZMQ_WARNING_ISSUED: AtomicBool = AtomicBool::new(false);
static IS_FIRST_GAME: AtomicBool = AtomicBool::new(true);

fn try_handle_new_game(py: Python<'_>, is_restart: bool) -> PyResult<()> {
    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    if IS_FIRST_GAME.load(Ordering::SeqCst) {
        late_init(&shinqlx_module, py)?;
        IS_FIRST_GAME.store(false, Ordering::SeqCst);

        let zmq_enabled = pyshinqlx_get_cvar(py, "zmq_stats_enable")?
            .map(|value| value != "0")
            .unwrap_or(false);
        if !zmq_enabled && !ZMQ_WARNING_ISSUED.load(Ordering::SeqCst) {
            pyshinqlx_get_logger(py, None).and_then(|logger| {
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
                Ok(())
            })?;

            ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);
        }
    }

    set_map_subtitles(&shinqlx_module)?;

    if !is_restart {
        let map_name = pyshinqlx_get_cvar(py, "mapname")?;
        let factory_name = pyshinqlx_get_cvar(py, "g_factory")?;
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers.bind(py).get_item(intern!(py, "map")).ok()
            })
            .map_or(
                Err(PyEnvironmentError::new_err(
                    "could not get access to map dispatcher",
                )),
                |map_dispatcher| {
                    map_dispatcher.call_method1(intern!(py, "dispatch"), (map_name, factory_name))
                },
            )?;
    }

    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "new_game"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to new game dispatcher",
            )),
            |new_game_dispatcher| new_game_dispatcher.call_method0(intern!(py, "dispatch")),
        )?;

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

#[cfg(test)]
mod handle_new_game_tests {
    use super::{
        handle_new_game, handler_test_support::capturing_hook, try_handle_new_game, IS_FIRST_GAME,
        ZMQ_WARNING_ISSUED,
    };

    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use crate::ffi::python::{
        commands::CommandPriorities,
        events::{EventDispatcherManager, MapDispatcher, NewGameDispatcher},
        pyshinqlx_setup_fixture::*,
        EVENT_DISPATCHERS,
    };

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder, CS_AUTHOR, CS_AUTHOR2, CS_MESSAGE};
    use crate::hooks::mock_hooks::shinqlx_set_configstring_context;

    use core::ffi::c_char;
    use core::sync::atomic::Ordering;

    use mockall::predicate;
    use rstest::rstest;

    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_game_restarted_stores_map_titles_and_authors(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_MESSAGE as u16))
            .returning(|_| "thunderstruck".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_AUTHOR as u16))
            .returning(|_| "Till 'Firestarter' Merker".into());
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_AUTHOR2 as u16))
            .returning(|_| "None".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|&configstring, value| {
                configstring == CS_AUTHOR
                    && value.contains(" - Running shinqlx ")
                    && value.contains(" with plugins ")
            })
            .times(1);
        set_configstring_ctx
            .expect()
            .withf(|&configstring, value| {
                configstring == CS_AUTHOR2
                    && value.ends_with(
                        " - Check ^6https://github.com/mgaertne/shinqlx^7 for more details.",
                    )
            })
            .times(1);

        IS_FIRST_GAME.store(false, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<NewGameDispatcher>())
                .expect("could not add new_game dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_new_game(py, true);
            assert!(result.is_ok());

            let pyshinqlx_module = py.import_bound("shinqlx").expect("this should not happen");
            assert!(pyshinqlx_module
                .getattr("_map_title")
                .and_then(|value| value.extract::<String>())
                .is_ok_and(|str_value| str_value == "thunderstruck"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_game_restarted_invokes_new_game_dispatcher(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_configstring().withf(|index| {
            [CS_MESSAGE as u16, CS_AUTHOR as u16, CS_AUTHOR2 as u16].contains(index)
        });
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

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<NewGameDispatcher>())
                .expect("could not add new_game dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let new_game_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("new_game")
                        .expect("could not get new_game dispatcher")
                })
                .expect("could not get new_game dispatcher");
            new_game_dispatcher
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
                .expect("could not add hook to new_game dispatcher");

            let result = try_handle_new_game(py, true);
            assert!(result.is_ok());

            assert!(capturing_hook
                .call_method1("assert_called_with", ())
                .is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_game_restarted_with_missing_new_game_dispatcher(
        _pyshinqlx_setup: (),
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_configstring().withf(|index| {
            [CS_MESSAGE as u16, CS_AUTHOR as u16, CS_AUTHOR2 as u16].contains(index)
        });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_new_game(py, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_new_map_loaded_invokes_map_dispatcher(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_configstring().withf(|index| {
            [CS_MESSAGE as u16, CS_AUTHOR as u16, CS_AUTHOR2 as u16].contains(index)
        });
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
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("mapname"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("campgrounds".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_factory"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("ffa".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<NewGameDispatcher>())
                .expect("could not add new_game dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<MapDispatcher>())
                .expect("could not add map dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let map_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("map")
                        .expect("could not get map dispatcher")
                })
                .expect("could not get map dispatcher");
            map_dispatcher
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
                .expect("could not add hook to map dispatcher");

            let new_game_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("new_game")
                        .expect("could not get new_game dispatcher")
                })
                .expect("could not get new_game dispatcher");
            new_game_dispatcher
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
                .expect("could not add hook to new_game dispatcher");

            let result = try_handle_new_game(py, false);
            assert!(result.is_ok());

            assert!(capturing_hook
                .call_method1("assert_called_with", ("campgrounds", "ffa"))
                .is_ok());
            assert!(capturing_hook
                .call_method1("assert_called_with", ())
                .is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_new_map_loaded_with_missing_map_dispatcher(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_configstring().withf(|index| {
            [CS_MESSAGE as u16, CS_AUTHOR as u16, CS_AUTHOR2 as u16].contains(index)
        });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("mapname"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("campgrounds".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_factory"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("ffa".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<NewGameDispatcher>())
                .expect("could not add new_game dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_new_game(py, false);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_first_game_with_zmq_enabled(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_configstring().withf(|index| {
            [CS_MESSAGE as u16, CS_AUTHOR as u16, CS_AUTHOR2 as u16].contains(index)
        });
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
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("mapname"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("campgrounds".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_factory"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("ffa".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("qlx_pluginsPath"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(".".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine.expect_find_cvar().withf(|name| {
            [
                "qlx_owner",
                "qlx_plugins",
                "qlx_database",
                "qlx_commandPrefix",
                "qlx_logs",
                "qlx_logsSize",
                "qlx_redisAddress",
                "qlx_redisDatabase",
                "qlx_redisUnixSocket",
                "qlx_redisPassword",
                "fs_homepath",
            ]
            .contains(&name)
        });
        mock_engine.expect_get_cvar().withf(|name, _, _| {
            [
                "qlx_owner",
                "qlx_plugins",
                "qlx_database",
                "qlx_commandPrefix",
                "qlx_logs",
                "qlx_logsSize",
                "qlx_redisAddress",
                "qlx_redisDatabase",
                "qlx_redisUnixSocket",
                "qlx_redisPassword",
            ]
            .contains(&name)
        });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(true, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(false, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<NewGameDispatcher>())
                .expect("could not add new_game dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_new_game(py, true);
            assert!(result.as_ref().is_ok(), "{:?}", result.as_ref());

            assert!(!IS_FIRST_GAME.load(Ordering::SeqCst));
            assert!(!ZMQ_WARNING_ISSUED.load(Ordering::SeqCst));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_first_game_with_zmq_disabled(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_configstring().withf(|index| {
            [CS_MESSAGE as u16, CS_AUTHOR as u16, CS_AUTHOR2 as u16].contains(index)
        });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("0".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("mapname"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("campgrounds".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_factory"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("ffa".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("qlx_pluginsPath"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(".".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine.expect_find_cvar().withf(|name| {
            [
                "qlx_owner",
                "qlx_plugins",
                "qlx_database",
                "qlx_commandPrefix",
                "qlx_logs",
                "qlx_logsSize",
                "qlx_redisAddress",
                "qlx_redisDatabase",
                "qlx_redisUnixSocket",
                "qlx_redisPassword",
                "fs_homepath",
            ]
            .contains(&name)
        });
        mock_engine.expect_get_cvar().withf(|name, _, _| {
            [
                "qlx_owner",
                "qlx_plugins",
                "qlx_database",
                "qlx_commandPrefix",
                "qlx_logs",
                "qlx_logsSize",
                "qlx_redisAddress",
                "qlx_redisDatabase",
                "qlx_redisUnixSocket",
                "qlx_redisPassword",
            ]
            .contains(&name)
        });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(true, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(false, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<NewGameDispatcher>())
                .expect("could not add new_game dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_new_game(py, true);
            assert!(result.as_ref().is_ok(), "{:?}", result.as_ref());

            assert!(!IS_FIRST_GAME.load(Ordering::SeqCst));
            assert!(ZMQ_WARNING_ISSUED.load(Ordering::SeqCst));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_first_game_with_zmq_disabled_when_warning_already_issued(
        _pyshinqlx_setup: (),
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_configstring().withf(|index| {
            [CS_MESSAGE as u16, CS_AUTHOR as u16, CS_AUTHOR2 as u16].contains(index)
        });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("zmq_stats_enable"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("0".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("mapname"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("campgrounds".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_factory"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string("ffa".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("qlx_pluginsPath"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(".".as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        mock_engine.expect_find_cvar().withf(|name| {
            [
                "qlx_owner",
                "qlx_plugins",
                "qlx_database",
                "qlx_commandPrefix",
                "qlx_logs",
                "qlx_logsSize",
                "qlx_redisAddress",
                "qlx_redisDatabase",
                "qlx_redisUnixSocket",
                "qlx_redisPassword",
                "fs_homepath",
            ]
            .contains(&name)
        });
        mock_engine.expect_get_cvar().withf(|name, _, _| {
            [
                "qlx_owner",
                "qlx_plugins",
                "qlx_database",
                "qlx_commandPrefix",
                "qlx_logs",
                "qlx_logsSize",
                "qlx_redisAddress",
                "qlx_redisDatabase",
                "qlx_redisUnixSocket",
                "qlx_redisPassword",
            ]
            .contains(&name)
        });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(true, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<NewGameDispatcher>())
                .expect("could not add new_game dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_new_game(py, true);
            assert!(result.as_ref().is_ok(), "{:?}", result.as_ref());

            assert!(!IS_FIRST_GAME.load(Ordering::SeqCst));
            assert!(ZMQ_WARNING_ISSUED.load(Ordering::SeqCst));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_new_game_when_game_restarted_with_missing_new_game_dispatcher(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_configstring().withf(|index| {
            [CS_MESSAGE as u16, CS_AUTHOR as u16, CS_AUTHOR2 as u16].contains(index)
        });
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = handle_new_game(py, true);
            assert!(result.is_some_and(|value| value));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_new_game_when_dispatcher_returns_ok(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_configstring().withf(|index| {
            [CS_MESSAGE as u16, CS_AUTHOR as u16, CS_AUTHOR2 as u16].contains(index)
        });
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

        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::SeqCst);
        ZMQ_WARNING_ISSUED.store(true, Ordering::SeqCst);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<NewGameDispatcher>())
                .expect("could not add new_game dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = handle_new_game(py, true);
            assert!(result.is_none());
        });
    }
}

static AD_ROUND_NUMBER: AtomicI32 = AtomicI32::new(0);

fn try_handle_set_configstring(py: Python<'_>, index: u32, value: &str) -> PyResult<PyObject> {
    let result = EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "set_configstring"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to set configstring dispatcher",
            )),
            |set_configstring_dispatcher| {
                set_configstring_dispatcher
                    .call_method1(intern!(py, "dispatch"), (index.into_py(py), value))
            },
        )?;

    if result
        .extract::<&PyBool>()
        .is_ok_and(|result_value| !result_value.is_true())
    {
        return Ok(false.into_py(py));
    }

    let configstring_value = result.extract::<String>().unwrap_or(value.to_string());
    match index {
        CS_VOTE_STRING => {
            if configstring_value.is_empty() {
                return Ok(configstring_value.into_py(py));
            }

            let (vote, args) = configstring_value
                .split_once(' ')
                .unwrap_or((configstring_value.as_str(), ""));
            EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers
                        .bind(py)
                        .get_item(intern!(py, "vote_started"))
                        .ok()
                })
                .map_or(
                    Err(PyEnvironmentError::new_err(
                        "could not get access to vote started dispatcher",
                    )),
                    |vote_started_dispatcher| {
                        vote_started_dispatcher
                            .call_method1(intern!(py, "dispatch"), (vote, args))?;

                        Ok(py.None())
                    },
                )
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
            if (old_state, new_state) == ("PRE_GAME", "COUNT_DOWN") {
                AD_ROUND_NUMBER.store(1, Ordering::SeqCst);
                EVENT_DISPATCHERS
                    .load()
                    .as_ref()
                    .and_then(|event_dispatchers| {
                        event_dispatchers
                            .bind(py)
                            .get_item(intern!(py, "game_countdown"))
                            .ok()
                    })
                    .map_or(
                        Err(PyEnvironmentError::new_err(
                            "could not get access to game countdown dispatcher",
                        )),
                        |game_countdown_dispatcher| {
                            game_countdown_dispatcher.call_method0(intern!(py, "dispatch"))?;
                            Ok(())
                        },
                    )?;
            }
            if ![
                ("PRE_GAME", "IN_PROGRESS"),
                ("PRE_GAME", "COUNT_DOWN"),
                ("COUNT_DOWN", "IN_PROGRESS"),
                ("IN_PROGRESS", "PRE_GAME"),
                ("COUNT_DOWN", "PRE_GAME"),
            ]
            .contains(&(old_state, new_state))
            {
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

#[cfg(test)]
mod handle_set_configstring_tests {
    use super::{
        handler_test_support::{capturing_hook, returning_false_hook, returning_other_string_hook},
        try_handle_set_configstring, AD_ROUND_NUMBER,
    };

    use crate::ffi::python::{
        commands::CommandPriorities,
        events::{EventDispatcherManager, SetConfigstringDispatcher, VoteStartedDispatcher},
        EVENT_DISPATCHERS,
    };

    use crate::prelude::{serial, MockQuakeEngine};
    use crate::MAIN_ENGINE;

    use crate::ffi::c::prelude::{
        cvar_t, CVar, CVarBuilder, CS_AUTHOR, CS_SERVERINFO, CS_VOTE_STRING,
    };

    use core::ffi::c_char;
    use core::sync::atomic::Ordering;

    use mockall::predicate;
    use pretty_assertions::assert_eq;

    use crate::ffi::python::events::GameCountdownDispatcher;
    use pyo3::prelude::*;
    use pyo3::{
        exceptions::{PyAssertionError, PyEnvironmentError},
        types::PyBool,
    };

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_forwards_to_python() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));
            let capturing_hook = capturing_hook(py);
            let set_configstring_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("set_configstring")
                        .expect("could not get set_configstring dispatcher")
                })
                .expect("could not get set_configstring dispatcher");
            set_configstring_dispatcher
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
                .expect("could not add hook to set_configstring dispatcher");

            let result = try_handle_set_configstring(py, CS_AUTHOR, "ShiN0");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "ShiN0")));
            assert!(capturing_hook
                .call_method1("assert_called_with", (CS_AUTHOR, "ShiN0"))
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_when_dispatcher_returns_false() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let set_configstring_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("set_configstring")
                        .expect("could not get set_configstring dispatcher")
                })
                .expect("could not get set_configstring dispatcher");
            set_configstring_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_false_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to set_configstring dispatcher");

            let result = try_handle_set_configstring(py, CS_AUTHOR, "ShiN0");
            assert!(result.is_ok_and(|value| value
                .extract::<&PyBool>(py)
                .is_ok_and(|bool_value| !bool_value.is_true())));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_when_dispatcher_is_missing() {
        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_set_configstring(py, CS_AUTHOR, "ShiN0");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_when_dispatcher_returns_other_value() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let set_configstring_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("set_configstring")
                        .expect("could not get set_configstring dispatcher")
                })
                .expect("could not get set_configstring dispatcher");
            set_configstring_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        returning_other_string_hook(py),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to set_configstring dispatcher");

            let result = try_handle_set_configstring(py, CS_AUTHOR, "ShiN0");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == "quit")));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_vote_string_change_with_one_word_vote() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteStartedDispatcher>())
                .expect("could not add vote_started dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_started_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote_started")
                        .expect("could not get vote_started dispatcher")
                })
                .expect("could not get vote_started dispatcher");
            vote_started_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        capturing_hook
                            .getattr("hook")
                            .expect("could not get handler from test module"),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to vote_started dispatcher");

            let result = try_handle_set_configstring(py, CS_VOTE_STRING, "restart");
            assert!(result.is_ok_and(|value| value.is_none(py)));
            assert!(capturing_hook
                .call_method1("assert_called_with", (py.None(), "restart", ""))
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_vote_string_change_with_multiword_vote() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteStartedDispatcher>())
                .expect("could not add vote_started dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_started_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote_started")
                        .expect("could not get vote_started dispatcher")
                })
                .expect("could not get vote_started dispatcher");
            vote_started_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        capturing_hook
                            .getattr("hook")
                            .expect("could not get handler from test module"),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to vote_started dispatcher");

            let result = try_handle_set_configstring(py, CS_VOTE_STRING, "map thunderstruck");
            assert!(result.is_ok_and(|value| value.is_none(py)));
            assert!(capturing_hook
                .call_method1("assert_called_with", (py.None(), "map", "thunderstruck"))
                .is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_vote_string_change_with_empty_votestring() {
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteStartedDispatcher>())
                .expect("could not add vote_started dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let vote_started_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("vote_started")
                        .expect("could not get vote_started dispatcher")
                })
                .expect("could not get vote_started dispatcher");
            vote_started_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        capturing_hook
                            .getattr("hook")
                            .expect("could not get handler from test module"),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to vote_started dispatcher");

            let result = try_handle_set_configstring(py, CS_VOTE_STRING, "");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value.is_empty())));
            assert!(capturing_hook
                .call_method1("assert_called_with", (py.None(), "map", "thunderstruck"))
                .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_vote_string_change_with_no_vote_started_dispatcher() {
        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_set_configstring(py, CS_VOTE_STRING, "kick ShiN0");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_server_info_change_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\PRE_GAME");
            assert!(result.is_ok_and(|value| value.is_none(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_server_info_change_with_no_prior_info_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_SERVERINFO as u16))
            .returning(|_| "".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\PRE_GAME");
            assert!(result.is_ok_and(|value| value.is_none(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_server_info_change_with_same_gamestate_as_before() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_SERVERINFO as u16))
            .returning(|_| r"\g_gameState\PRE_GAME".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\PRE_GAME");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == r"\g_gameState\PRE_GAME")));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_game_countdown_change() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_SERVERINFO as u16))
            .returning(|_| r"\g_gameState\PRE_GAME".into());
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

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<GameCountdownDispatcher>())
                .expect("could not add game_countdown dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let capturing_hook = capturing_hook(py);
            let game_countdown_dispatcher = EVENT_DISPATCHERS
                .load()
                .as_ref()
                .map(|event_dispatcher| {
                    event_dispatcher
                        .bind(py)
                        .get_item("game_countdown")
                        .expect("could not get game_countdown dispatcher")
                })
                .expect("could not get game_countdown dispatcher");
            game_countdown_dispatcher
                .call_method1(
                    "add_hook",
                    (
                        "asdf",
                        capturing_hook
                            .getattr("hook")
                            .expect("could not get handler from test module"),
                        CommandPriorities::PRI_NORMAL as i32,
                    ),
                )
                .expect("could not add hook to game_countdown dispatcher");

            let result = try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\COUNT_DOWN");
            assert!(result.is_ok_and(|value| value
                .extract::<String>(py)
                .is_ok_and(|str_value| str_value == r"\g_gameState\COUNT_DOWN")));
            assert!(capturing_hook
                .call_method1("assert_called_with", ())
                .is_ok());
            assert_eq!(AD_ROUND_NUMBER.load(Ordering::SeqCst), 1);
        });
    }
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
    if result
        .extract::<&PyBool>()
        .is_ok_and(|value| !value.is_true())
    {
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
_args = []

def hook(*args):
    global _args
    _args.append(args)

def assert_called_with(*args):
    global _args
    assert(len(_args) > 0)

    called_with = _args.pop(0)
    assert len(args) == len(called_with), f"{args = } {len(args) = } == {called_with = } {len(called_with) = }"
    for (expected, actual) in zip(args, called_with):
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
