use alloc::sync::Arc;
use core::{
    hint::cold_path,
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
};
use std::sync::LazyLock;

use arc_swap::ArcSwapOption;
use itertools::Itertools;
use pyo3::{
    BoundObject, IntoPyObjectExt, PyTraverseError, PyVisit,
    exceptions::{PyEnvironmentError, PyKeyError, PyValueError},
    intern,
    prelude::*,
    types::{IntoPyDict, PyBool, PyDict, PyInt, PyString},
};
use rayon::prelude::*;
use regex::{Regex, RegexBuilder};
use tap::{TapFallible, TapOptional};

use super::{
    BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, COMMANDS, CONSOLE_CHANNEL, EVENT_DISPATCHERS,
    FREE_CHAT_CHANNEL, RED_TEAM_CHAT_CHANNEL, SPECTATOR_CHAT_CHANNEL, get_cvar, is_vote_active,
    late_init, log_exception, prelude::*, pyshinqlx_get_logger, set_map_subtitles,
};
use crate::{
    MAIN_ENGINE,
    ffi::c::prelude::*,
    quake_live_engine::{FindCVar, GetConfigstring},
};

fn try_handle_rcon(py: Python<'_>, cmd: &str) -> PyResult<Option<bool>> {
    COMMANDS.load().as_ref().map_or(Ok(None), |commands| {
        let rcon_dummy_player =
            Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))?;

        let shinqlx_console_channel = CONSOLE_CHANNEL
            .load()
            .as_ref()
            .map_or(py.None(), |channel| channel.clone_ref(py).into_any());

        commands
            .bind(py)
            .handle_input(
                rcon_dummy_player.as_super().as_super(),
                cmd,
                shinqlx_console_channel.bind(py),
            )
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
    use pyo3::{intern, prelude::*, types::PyString};
    use rstest::*;

    use super::{handle_rcon, try_handle_rcon};
    use crate::{
        ffi::python::{
            COMMANDS, EVENT_DISPATCHERS, commands::CommandPriorities, prelude::*,
            pyshinqlx_test_support::*,
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_rcon_with_no_commands(_pyshinqlx_setup: ()) {
        COMMANDS.store(None);
        EVENT_DISPATCHERS.store(None);

        Python::attach(|py| {
            let result = try_handle_rcon(py, "asdf");
            assert!(result.is_ok_and(|value| value.is_none()));
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_rcon_with_command_invoker_in_place(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let plugin = test_plugin(py).call0().expect("this should not happen");
            let capturing_hook = capturing_hook(py);
            let cmd_handler = capturing_hook
                .getattr(intern!(py, "hook"))
                .expect("could not get handler from test module");
            let command = Command::py_new(
                &plugin,
                PyString::intern(py, "asdf").as_any(),
                &cmd_handler,
                0,
                py.None().bind(py),
                py.None().bind(py),
                false,
                0,
                false,
                "",
            )
            .expect("could not create command");
            let py_command = Bound::new(py, command).expect("this should not happen");

            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            command_invoker
                .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                .expect("could not add command to command invoker");
            COMMANDS.store(Some(command_invoker.unbind().into()));

            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<CommandDispatcher>())
                .expect("could not add command dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_rcon(py, "asdf");
            assert!(result.is_ok_and(|value| value.is_none()));
            assert!(
                capturing_hook
                    .call_method1(intern!(py, "assert_called_with"), ("_", ["asdf"], "_"))
                    .is_ok()
            );
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_rcon_with_no_main_engine(_pyshinqlx_setup: ()) {
        EVENT_DISPATCHERS.store(None);

        Python::attach(|py| {
            let plugin = test_plugin(py).call0().expect("this should not happen");
            let cmd_handler = python_function_raising_exception(py);

            let command = Command::py_new(
                &plugin,
                PyString::intern(py, "asdf").as_any(),
                &cmd_handler,
                0,
                py.None().bind(py),
                py.None().bind(py),
                false,
                0,
                false,
                "",
            )
            .expect("could not create command");
            let py_command = Bound::new(py, command).expect("this should not happen");

            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            command_invoker
                .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                .expect("could not add command to command invoker");
            COMMANDS.store(Some(command_invoker.unbind().into()));

            let result = handle_rcon(py, "asdf");
            assert!(result.is_some_and(|value| value));
        });
    }
}

static RE_SAY: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r#"^say +(?P<quote>"?)(?P<msg>.+)$"#)
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_SAY_TEAM: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r#"^say_team +(?P<quote>"?)(?P<msg>.+)$"#)
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_CALLVOTE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r#"^(?:cv|callvote) +(?P<cmd>[^ ]+)(?: "?(?P<args>.+?)"?)?$"#)
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_VOTE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^vote +(?P<arg>.)")
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_TEAM: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r"^team +(?P<arg>.)")
        .case_insensitive(true)
        .multi_line(true)
        .build()
        .unwrap()
});
static RE_USERINFO: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r#"^userinfo "(?P<vars>.+)"$"#)
        .multi_line(true)
        .build()
        .unwrap()
});

fn try_handle_client_command(py: Python<'_>, client_id: i32, cmd: &str) -> PyResult<Py<PyAny>> {
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
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to client command dispatcher",
                ))
            },
            |client_command_dispatcher| {
                ClientCommandDispatcherMethods::dispatch(
                    client_command_dispatcher.downcast()?,
                    &Bound::new(py, player.to_owned())?,
                    cmd,
                )
            },
        )?;
    if return_value
        .downcast::<PyBool>()
        .is_ok_and(|value| !value.is_true())
    {
        return Ok(PyBool::new(py, false).into_any().unbind());
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
                {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to chat dispatcher",
                    ))
                },
                |chat_dispatcher| {
                    let Some(ref main_chat_channel) = *CHAT_CHANNEL.load() else {
                        cold_path();
                        return Err(PyEnvironmentError::new_err(
                            "could not get access to main chat channel",
                        ));
                    };

                    ChatEventDispatcherMethods::dispatch(
                        chat_dispatcher.downcast()?,
                        &Bound::new(py, player)?,
                        &reformatted_msg,
                        main_chat_channel.bind(py),
                    )
                },
            )?;

        return match result.downcast::<PyBool>() {
            Ok(py_bool) if !py_bool.is_true() => Ok(PyBool::new(py, false).into_any().unbind()),
            _ => {
                let new_command = format!("say \"{reformatted_msg}\"");
                Ok(PyString::new(py, &new_command).into_any().unbind())
            }
        };
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
                {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to chat dispatcher",
                    ))
                },
                |chat_dispatcher| {
                    let channel = match player.get_team(py)?.as_str() {
                        "free" => &FREE_CHAT_CHANNEL,
                        "red" => &RED_TEAM_CHAT_CHANNEL,
                        "blue" => &BLUE_TEAM_CHAT_CHANNEL,
                        _ => &SPECTATOR_CHAT_CHANNEL,
                    };
                    let Some(ref chat_channel) = *channel.load() else {
                        cold_path();
                        return Err(PyEnvironmentError::new_err(
                            "could not get access to team chat channel",
                        ));
                    };
                    ChatEventDispatcherMethods::dispatch(
                        chat_dispatcher.downcast()?,
                        &Bound::new(py, player)?,
                        &reformatted_msg,
                        chat_channel.bind(py),
                    )
                },
            )?;

        return match result.downcast::<PyBool>() {
            Ok(py_bool) if !py_bool.is_true() => Ok(PyBool::new(py, false).into_any().unbind()),
            _ => {
                let new_command = format!("say_team \"{reformatted_msg}\"");
                Ok(PyString::new(py, &new_command).into_any().unbind())
            }
        };
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
        if is_vote_active() {
            return Ok(PyString::new(py, updated_cmd).into_any().unbind());
        }
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
                {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to vote started dispatcher",
                    ))
                },
                |vote_started_dispatcher| {
                    VoteStartedDispatcherMethods::caller(
                        vote_started_dispatcher.downcast()?,
                        Bound::new(py, player.to_owned())?.as_any(),
                    );
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
                {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to vote called dispatcher",
                    ))
                },
                |vote_called_dispatcher| {
                    VoteCalledDispatcherMethods::dispatch(
                        vote_called_dispatcher.downcast()?,
                        &Bound::new(py, player.to_owned())?,
                        vote.as_str(),
                        PyString::new(py, args).as_any(),
                    )
                },
            )?;

        return match result.downcast::<PyBool>() {
            Ok(py_bool) if !py_bool.is_true() => Ok(PyBool::new(py, false).into_any().unbind()),
            _ => Ok(PyString::new(py, updated_cmd).into_any().unbind()),
        };
    }

    if let Some(arg) = RE_VOTE
        .captures(updated_cmd)
        .and_then(|captures| captures.name("arg"))
    {
        if !is_vote_active() || !["y", "Y", "1", "n", "N", "2"].contains(&arg.as_str()) {
            return Ok(PyString::new(py, updated_cmd).into_any().unbind());
        }
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
                {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to vote dispatcher",
                    ))
                },
                |vote_dispatcher| {
                    VoteDispatcherMethods::dispatch(
                        vote_dispatcher.downcast()?,
                        &Bound::new(py, player.to_owned())?,
                        vote,
                    )
                },
            )?;

        return match result.downcast::<PyBool>() {
            Ok(py_bool) if !py_bool.is_true() => Ok(PyBool::new(py, false).into_any().unbind()),
            _ => Ok(PyString::new(py, updated_cmd).into_any().unbind()),
        };
    }

    if let Some(arg) = RE_TEAM
        .captures(updated_cmd)
        .and_then(|captures| captures.name("arg"))
    {
        let current_team = player.get_team(py)?;
        if !["f", "r", "b", "s", "a"].contains(&arg.as_str())
            || current_team.starts_with(arg.as_str())
        {
            return Ok(PyString::new(py, updated_cmd).into_any().unbind());
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
                {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to team switch attempt dispatcher",
                    ))
                },
                |team_switch_attempt_dispatcher| {
                    TeamSwitchAttemptDispatcherMethods::dispatch(
                        team_switch_attempt_dispatcher.downcast()?,
                        &Bound::new(py, player)?,
                        &current_team,
                        target_team,
                    )
                },
            )?;

        return match result.downcast::<PyBool>() {
            Ok(py_bool) if !py_bool.is_true() => Ok(PyBool::new(py, false).into_any().unbind()),
            _ => Ok(PyString::new(py, updated_cmd).into_any().unbind()),
        };
    }

    if let Some(vars) = RE_USERINFO
        .captures(updated_cmd)
        .and_then(|captures| captures.name("vars"))
    {
        let new_info = parse_variables(vars.as_str());
        let old_info = parse_variables(&player.user_info);

        let changed = new_info
            .items
            .par_iter()
            .filter(|(key, new_value)| {
                let opt_old_value = old_info.get(key);
                opt_old_value.is_none()
                    || opt_old_value.is_some_and(|old_value| old_value != *new_value)
            })
            .collect::<Vec<_>>();

        if changed.is_empty() {
            return Ok(PyString::new(py, updated_cmd).into_any().unbind());
        }
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
                {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to userinfo dispatcher",
                    ))
                },
                |userinfo_dispatcher| {
                    UserinfoDispatcherMethods::dispatch(
                        userinfo_dispatcher.downcast()?,
                        &Bound::new(py, player.to_owned())?,
                        &changed.into_py_dict(py)?,
                    )
                },
            )?;

        return match result.downcast::<PyBool>() {
            Ok(py_bool) if !py_bool.is_true() => Ok(PyBool::new(py, false).into_any().unbind()),
            _ => match result.downcast::<PyDict>() {
                Ok(changed_values) => {
                    let updated_info = new_info.into_py_dict(py)?;
                    updated_info
                        .update(changed_values.to_owned().as_mapping())
                        .map(|_| {
                            let formatted_key_values = updated_info
                                .iter()
                                .map(|(key, value)| format!(r"\{key}\{value}"))
                                .join("");

                            let new_command = format!(r#"userinfo "{formatted_key_values}""#);
                            PyString::new(py, &new_command).into_any().unbind()
                        })
                }
                _ => Ok(PyString::new(py, updated_cmd).into_any().unbind()),
            },
        };
    }

    Ok(PyString::new(py, updated_cmd).into_any().unbind())
}

/// Client commands are commands such as "say", "say_team", "scores",
/// "disconnect" and so on. This function parses those and passes it
/// on to the event dispatcher.
#[pyfunction]
pub(crate) fn handle_client_command(py: Python<'_>, client_id: i32, cmd: &str) -> Py<PyAny> {
    try_handle_client_command(py, client_id, cmd).unwrap_or_else(|e| {
        log_exception(py, &e);
        PyBool::new(py, true).to_owned().into_any().unbind()
    })
}

#[cfg(test)]
mod handle_client_command_tests {
    use core::borrow::BorrowMut;
    use std::sync::LazyLock;

    use arc_swap::ArcSwapOption;
    use mockall::predicate;
    use pyo3::{
        exceptions::{PyAssertionError, PyEnvironmentError},
        intern,
        prelude::*,
        types::{IntoPyDict, PyBool},
    };
    use rstest::rstest;

    use super::{handle_client_command, try_handle_client_command};
    use crate::{
        ffi::{
            c::prelude::{
                CS_VOTE_STRING, CVar, CVarBuilder, MockClient, MockGameEntityBuilder,
                clientState_t, cvar_t, privileges_t, team_t,
            },
            python::{
                BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, EVENT_DISPATCHERS, FREE_CHAT_CHANNEL,
                PythonReturnCodes::RET_STOP_EVENT,
                RED_TEAM_CHAT_CHANNEL, SPECTATOR_CHAT_CHANNEL, Teams,
                channels::TeamChatChannel,
                commands::CommandPriorities,
                events::{
                    ChatEventDispatcher, ClientCommandDispatcher, EventDispatcher,
                    EventDispatcherManager, EventDispatcherManagerMethods, EventDispatcherMethods,
                    TeamSwitchAttemptDispatcher, UserinfoDispatcher, VoteCalledDispatcher,
                    VoteDispatcher, VoteStartedDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_client_command_only(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "client_command"))
                                .and_then(|client_command_dispatcher| {
                                    client_command_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to client_command dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "cp \"asdf\"");
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>(py)
                                    .is_ok_and(|str_value| str_value == "cp \"asdf\"")
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", "cp \"asdf\"",)
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_client_command_with_no_event_dispatchers(
        _pyshinqlx_setup: (),
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    EVENT_DISPATCHERS.store(None);

                    let result = try_handle_client_command(py, 42, "cp \"asdf\"");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_client_command_only_when_dispatcher_returns_false(
        _pyshinqlx_setup: (),
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .get_item(intern!(py, "client_command"))
                                .and_then(|client_command_dispatcher| {
                                    client_command_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &python_function_returning(
                                                py,
                                                &(RET_STOP_EVENT as i32),
                                            ),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to client_command dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "cp \"asdf\"");
                            assert!(result.is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| !bool_value.is_true())
                            }));
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_client_command_only_when_dispatcher_returns_other_client_command(
        _pyshinqlx_setup: (),
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .get_item(intern!(py, "client_command"))
                                .and_then(|client_command_dispatcher| {
                                    client_command_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &python_function_returning(py, &"quit"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to client_command dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "cp \"asdf\"");
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>(py)
                                    .is_ok_and(|str_value| str_value == "quit")
                            }));
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_msg_send(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let channel = Bound::new(
                                py,
                                TeamChatChannel::py_new(
                                    py,
                                    "all",
                                    "chat",
                                    "print \"{}\n\"\n",
                                    py.None().bind(py),
                                    None,
                                ),
                            )
                            .expect("could not create TeamChatchannel in python");
                            CHAT_CHANNEL.store(Some(channel.unbind().into()));

                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ChatEventDispatcher>())
                                .expect("could not add chat dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "chat"))
                                .and_then(|chat_dispatcher| {
                                    chat_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to chat dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(
                                py,
                                42,
                                "say \"test with \"quotation marks\"\"",
                            );
                            assert!(result.is_ok_and(|value| {
                                value.extract::<String>(py).is_ok_and(|str_value| {
                                    str_value == "say \"test with 'quotation marks'\""
                                })
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", "test with 'quotation marks'", "_"),
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_msg_that_caused_panic(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let channel = Bound::new(
                                py,
                                TeamChatChannel::py_new(
                                    py,
                                    "all",
                                    "chat",
                                    "print \"{}\n\"\n",
                                    py.None().bind(py),
                                    None,
                                ),
                            )
                            .expect("could not create TeamChatchannel in python");
                            CHAT_CHANNEL.store(Some(channel.unbind().into()));

                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ChatEventDispatcher>())
                                .expect("could not add chat dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "chat"))
                                .and_then(|chat_dispatcher| {
                                    chat_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to chat dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(
                                py,
                                42,
                                "say \"bob: долбоеб ты оказывается)\"\n",
                            );
                            assert!(result.is_ok_and(|value| {
                                value.extract::<String>(py).is_ok_and(|str_value| {
                                    str_value == "say \"bob: долбоеб ты оказывается)\""
                                })
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", "bob: долбоеб ты оказывается)", "_"),
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_msg_with_no_chat_dispatcher(_pyshinqlx_setup: ()) {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    let channel = Bound::new(
                        py,
                        TeamChatChannel::py_new(
                            py,
                            "all",
                            "chat",
                            "print \"{}\n\"\n",
                            py.None().bind(py),
                            None,
                        ),
                    )
                    .expect("could not create TeamChatchannel in python");
                    CHAT_CHANNEL.store(Some(channel.unbind().into()));

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                        .expect("could not add client_command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_client_command(py, 42, "say \"hi @all\"");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_msg_with_no_chat_channel(_pyshinqlx_setup: ()) {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    CHAT_CHANNEL.store(None);

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                        .expect("could not add client_command dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ChatEventDispatcher>())
                        .expect("could not add chat dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_client_command(py, 42, "say \"hi @all\"");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_msg_when_dispatcher_returns_false(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let channel = Bound::new(
                                py,
                                TeamChatChannel::py_new(
                                    py,
                                    "all",
                                    "chat",
                                    "print \"{}\n\"\n",
                                    py.None().bind(py),
                                    None,
                                ),
                            )
                            .expect("could not create TeamChatchannel in python");
                            CHAT_CHANNEL.store(Some(channel.unbind().into()));

                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ChatEventDispatcher>())
                                .expect("could not add chat dispatcher");
                            event_dispatcher
                                .get_item(intern!(py, "chat"))
                                .and_then(|chat_dispatcher| {
                                    chat_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &python_function_returning(
                                                py,
                                                &(RET_STOP_EVENT as i32),
                                            ),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to chat dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "say \"hi @all\"");
                            assert!(result.is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| !bool_value.is_true())
                            }));
                        });
                    });
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
        _pyshinqlx_setup: (),
        #[case] team: team_t,
        #[case] team_str: &str,
        #[case] team_name: &str,
        #[case] channel: &LazyLock<ArcSwapOption<Py<TeamChatChannel>>>,
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(move || team, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let py_channel = Bound::new(
                                py,
                                TeamChatChannel::py_new(
                                    py,
                                    team_str,
                                    team_name,
                                    "print \"{}\n\"\n",
                                    py.None().bind(py),
                                    None,
                                ),
                            )
                            .expect("could not create TeamChatchannel in python");
                            channel.store(Some(py_channel.unbind().into()));

                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ChatEventDispatcher>())
                                .expect("could not add chat dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "chat"))
                                .and_then(|chat_dispatcher| {
                                    chat_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to chat dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(
                                py,
                                42,
                                "say_team \"test with \"quotation marks\"\"",
                            );
                            assert!(result.is_ok_and(|value| {
                                value.extract::<String>(py).is_ok_and(|str_value| {
                                    str_value == "say_team \"test with 'quotation marks'\""
                                })
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", "test with 'quotation marks'", "_"),
                                    )
                                    .is_ok()
                            );
                        });
                    });
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
        _pyshinqlx_setup: (),
        #[case] team: team_t,
        #[case] channel: &LazyLock<ArcSwapOption<Py<TeamChatChannel>>>,
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(move || team, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    channel.store(None);

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                        .expect("could not add client_command dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ChatEventDispatcher>())
                        .expect("could not add chat dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_client_command(
                        py,
                        42,
                        "say_team \"test with \"quotation marks\"\"",
                    );
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
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
        _pyshinqlx_setup: (),
        #[case] team: team_t,
        #[case] team_str: &str,
        #[case] team_name: &str,
        #[case] channel: &LazyLock<ArcSwapOption<Py<TeamChatChannel>>>,
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(move || team, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    let py_channel = Bound::new(
                        py,
                        TeamChatChannel::py_new(
                            py,
                            team_str,
                            team_name,
                            "print \"{}\n\"\n",
                            py.None().bind(py),
                            None,
                        ),
                    )
                    .expect("could not create TeamChatchannel in python");
                    channel.store(Some(py_channel.unbind().into()));

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                        .expect("could not add client_command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_client_command(py, 42, "say_team \"hi @all\"");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
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
        _pyshinqlx_setup: (),
        #[case] team: team_t,
        #[case] team_str: &str,
        #[case] team_name: &str,
        #[case] channel: &LazyLock<ArcSwapOption<Py<TeamChatChannel>>>,
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(move || team, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let py_channel = Bound::new(
                                py,
                                TeamChatChannel::py_new(
                                    py,
                                    team_str,
                                    team_name,
                                    "print \"{}\n\"\n",
                                    py.None().bind(py),
                                    None,
                                ),
                            )
                            .expect("could not create TeamChatchannel in python");
                            channel.store(Some(py_channel.unbind().into()));

                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("This should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ChatEventDispatcher>())
                                .expect("could not add chat dispatcher");
                            event_dispatcher
                                .get_item(intern!(py, "chat"))
                                .and_then(|chat_dispatcher| {
                                    chat_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &python_function_returning(
                                                py,
                                                &(RET_STOP_EVENT as i32),
                                            ),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to chat dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "say_team \"hi @all\"");
                            assert!(result.is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| !bool_value.is_true())
                            }));
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteCalledDispatcher>())
                                .expect("could not add vote_called dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
                                .expect("could not add vote_started dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "vote_called"))
                                .and_then(|vote_called_dispatcher| {
                                    vote_called_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to vote_called dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result =
                                try_handle_client_command(py, 42, "callvote map \"thunderstruck\"");
                            assert!(result.is_ok_and(|value| {
                                value.extract::<String>(py).is_ok_and(|str_value| {
                                    str_value == "callvote map \"thunderstruck\""
                                })
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", "map", "thunderstruck")
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote_when_vote_is_already_running(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .with_get_configstring(CS_VOTE_STRING as u16, "allready", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteCalledDispatcher>())
                                .expect("could not add vote_called dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
                                .expect("could not add vote_started dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "vote_called"))
                                .and_then(|vote_called_dispatcher| {
                                    vote_called_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to vote_called dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result =
                                try_handle_client_command(py, 42, "callvote map \"thunderstruck\"");
                            assert!(result.is_ok_and(|value| {
                                value.extract::<String>(py).is_ok_and(|str_value| {
                                    str_value == "callvote map \"thunderstruck\""
                                })
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", "_", "_")
                                    )
                                    .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py))
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote_with_no_vote_called_dispatcher(_pyshinqlx_setup: ()) {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
                                .expect("could not add vote_started dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "cv restart");
                            assert!(
                                result
                                    .is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py))
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote_with_no_vote_started_dispatcher(
        _pyshinqlx_setup: (),
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteCalledDispatcher>())
                                .expect("could not add vote_called dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "cv restart");
                            assert!(
                                result
                                    .is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py))
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_callvote_when_dispatcher_returns_false(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteCalledDispatcher>())
                                .expect("could not add vote_called dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
                                .expect("could not add vote_started dispatcher");
                            event_dispatcher
                                .get_item(intern!(py, "vote_called"))
                                .and_then(|vote_called_dispatcher| {
                                    vote_called_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &python_function_returning(
                                                py,
                                                &(RET_STOP_EVENT as i32),
                                            ),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to vote_called dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result =
                                try_handle_client_command(py, 42, "callvote map \"thunderstruck\"");
                            assert!(result.is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| !bool_value.is_true())
                            }));
                        });
                    });
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
    fn try_handle_client_command_for_vote_command(
        _pyshinqlx_setup: (),
        #[case] vote_arg: &str,
        #[case] vote: bool,
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteDispatcher>())
                                .expect("could not add vote dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "vote"))
                                .and_then(|vote_dispatcher| {
                                    vote_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to vote dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let client_command = format!("vote {vote_arg}");
                            let result = try_handle_client_command(py, 42, &client_command);
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>(py)
                                    .is_ok_and(|str_value| str_value == client_command)
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_", vote,))
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_vote_command_for_unhandled_vote(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteDispatcher>())
                                .expect("could not add vote dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "vote"))
                                .and_then(|vote_dispatcher| {
                                    vote_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to vote dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "vote 3");
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>(py)
                                    .is_ok_and(|str_value| str_value == "vote 3")
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_", "_",))
                                    .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py))
                            );
                        });
                    });
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
    fn try_handle_client_command_for_vote_command_when_no_vote_running(
        _pyshinqlx_setup: (),
        #[case] vote_arg: &str,
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not hapopen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteDispatcher>())
                                .expect("could not add vote dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "vote"))
                                .and_then(|vote_dispatcher| {
                                    vote_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to vote dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let client_command = format!("vote {vote_arg}");
                            let result = try_handle_client_command(py, 42, &client_command);
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>(py)
                                    .is_ok_and(|str_value| str_value == client_command)
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_", "_",))
                                    .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py))
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_vote_command_with_no_vote_dispatcher(_pyshinqlx_setup: ()) {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "vote 1");
                            assert!(
                                result
                                    .is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py))
                            );
                        });
                    });
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
        _pyshinqlx_setup: (),
        #[case] vote_arg: &str,
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck", 1)
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<VoteDispatcher>())
                                .expect("could not add vote dispatcher");
                            event_dispatcher
                                .get_item(intern!(py, "vote"))
                                .and_then(|vote_dispatcher| {
                                    vote_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &python_function_returning(
                                                py,
                                                &(RET_STOP_EVENT as i32),
                                            ),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to vote dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let client_command = format!("vote {vote_arg}");
                            let result = try_handle_client_command(py, 42, &client_command);
                            assert!(result.is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| !bool_value.is_true())
                            }));
                        });
                    });
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
        _pyshinqlx_setup: (),
        #[case] team_char: &str,
        #[case] team_str: &str,
        #[case] player_team: team_t,
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(move || player_team, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                                .expect("could not add team_switch_attempt dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "team_switch_attempt"))
                                .and_then(|team_switch_attempt_dispatcher| {
                                    team_switch_attempt_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to team_switch_attempt dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let client_command = format!("team {team_char}");
                            let result = try_handle_client_command(py, 42, &client_command);
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>(py)
                                    .is_ok_and(|str_value| str_value == client_command)
                            }),);
                            let current_team = Teams::from(player_team).to_string();
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", current_team, team_str)
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_switch_for_unhandled_team(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                                .expect("could not add team_switch_attempt dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "team_switch_attempt"))
                                .and_then(|team_switch_attempt_dispatcher| {
                                    team_switch_attempt_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to team_switch_attempt dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(py, 42, "team c");
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>(py)
                                    .is_ok_and(|str_value| str_value == "team c")
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_", "_",))
                                    .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py))
                            );
                        });
                    });
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
        _pyshinqlx_setup: (),
        #[case] team_char: &str,
        #[case] player_team: team_t,
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(move || player_team, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                                .expect("could not add team_switch_attempt dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "team_switch_attempt"))
                                .and_then(|team_switch_attempt_dispatcher| {
                                    team_switch_attempt_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to team_switch_attempt dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let client_command = format!("team {team_char}");
                            let result = try_handle_client_command(py, 42, &client_command);
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>(py)
                                    .is_ok_and(|str_value| str_value == client_command)
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", "_", "_",)
                                    )
                                    .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py))
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_team_switch_attempt_command_with_no_dispatcher(
        _pyshinqlx_setup: (),
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                        .expect("could not add client_command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_client_command(py, 42, "team a");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
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
        _pyshinqlx_setup: (),
        #[case] team_char: &str,
        #[case] player_team: team_t,
    ) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(move || player_team, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                                .expect("could not add team_switch_attempt dispatcher");
                            event_dispatcher
                                .get_item(intern!(py, "team_switch_attempt"))
                                .and_then(|team_switch_attempt_dispatcher| {
                                    team_switch_attempt_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &python_function_returning(
                                                py,
                                                &(RET_STOP_EVENT as i32),
                                            ),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to team_switch_attempt dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let client_command = format!("team {team_char}");
                            let result = try_handle_client_command(py, 42, &client_command);
                            assert!(result.is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| !bool_value.is_true())
                            }));
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_when_nothing_changed(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<UserinfoDispatcher>())
                                .expect("could not add userinfo dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "userinfo"))
                                .and_then(|userinfo_dispatcher| {
                                    userinfo_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to userinfo dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(
                                py,
                                42,
                                r#"userinfo "\name\Mocked Player\sex\male""#,
                            );
                            assert!(result.is_ok_and(|value| {
                                value.extract::<String>(py).is_ok_and(|str_value| {
                                    str_value == r#"userinfo "\name\Mocked Player\sex\male""#
                                })
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_", "_",))
                                    .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py))
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_with_changes(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<UserinfoDispatcher>())
                                .expect("could not add userinfo dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "userinfo"))
                                .and_then(|userinfo_dispatcher| {
                                    userinfo_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to userinfo dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(
                                py,
                                42,
                                r#"userinfo "\name\Mocked Player\sex\female""#,
                            );
                            assert!(result.is_ok_and(|value| {
                                value.extract::<String>(py).is_ok_and(|str_value| {
                                    str_value == r#"userinfo "\name\Mocked Player\sex\female""#
                                })
                            }),);
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        (
                                            "_",
                                            [("sex", "female"),]
                                                .into_py_dict(py)
                                                .expect("this shouöld not happen"),
                                        )
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_with_no_event_dispatcher(
        _pyshinqlx_setup: (),
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                        .expect("could not add client_command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_client_command(
                        py,
                        42,
                        r#"userinfo "\name\Mocked Player\sex\female""#,
                    );
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_with_dispatcher_returns_false(
        _pyshinqlx_setup: (),
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<UserinfoDispatcher>())
                                .expect("could not add userinfo dispatcher");
                            event_dispatcher
                                .get_item(intern!(py, "userinfo"))
                                .and_then(|userinfo_dispatcher| {
                                    userinfo_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &python_function_returning(
                                                py,
                                                &(RET_STOP_EVENT as i32),
                                            ),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to userinfo dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(
                                py,
                                42,
                                r#"userinfo "\name\Mocked Player\sex\female""#,
                            );
                            assert!(result.is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| !bool_value.is_true())
                            }));
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_client_command_for_userinfo_change_with_dispatcher_returns_other_userinfo(
        _pyshinqlx_setup: (),
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
                    .returning(|| r"\name\Mocked Player\sex\male".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ClientCommandDispatcher>())
                                .expect("could not add client_command dispatcher");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<UserinfoDispatcher>())
                                .expect("could not add userinfo dispatcher");
                            let returning_other_userinfo_module = PyModule::from_code(
                                py,
                                cr#"
def returning_other_userinfo_hook(*args, **kwargs):
    return {"name": "Changed Player", "sex": "male", "country": "GB"}
                "#,
                                c"",
                                c"",
                            )
                            .expect("could not create returning other userinfo module");
                            event_dispatcher
                                .get_item(intern!(py, "userinfo"))
                                .and_then(|userinfo_dispatcher| {
                                    userinfo_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &returning_other_userinfo_module
                                                .getattr(intern!(
                                                    py,
                                                    "returning_other_userinfo_hook"
                                                ))
                                                .expect("could not get hook from capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to userinfo dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_client_command(
                                py,
                                42,
                                r#"userinfo "\name\Mocked Player\sex\female""#,
                            );
                            assert!(result.is_ok_and(|value| {
                                value.extract::<String>(py).is_ok_and(|str_value| {
                                    str_value
                                        == r#"userinfo "\name\Changed Player\sex\male\country\GB""#
                                })
                            }),);
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_client_command_with_no_event_dispatchers(_pyshinqlx_setup: ()) {
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

        EVENT_DISPATCHERS.store(None);

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    let result = handle_client_command(py, 42, "asdf");
                    assert!(
                        result
                            .downcast_bound::<PyBool>(py)
                            .is_ok_and(|value| value.is_true())
                    );
                });
            });
    }
}

static RE_VOTE_ENDED: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(r#"^print "Vote (?P<result>passed|failed)\.\n"$"#)
        .multi_line(true)
        .build()
        .unwrap()
});

fn try_handle_server_command<'py>(
    py: Python<'py>,
    client_id: i32,
    cmd: &str,
) -> PyResult<Bound<'py, PyAny>> {
    let Ok(player) = (0..MAX_CLIENTS as i32)
        .find(|&id| id == client_id)
        .map_or(Ok(py.None().bind(py).to_owned()), |id| {
            Player::py_new(id, None).and_then(|player| Ok(Bound::new(py, player)?.into_any()))
        })
    else {
        cold_path();
        return Ok(PyBool::new(py, true).to_owned().into_any());
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
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to server command dispatcher",
                ))
            },
            |server_command_dispatcher| {
                ServerCommandDispatcherMethods::dispatch(
                    server_command_dispatcher.downcast()?,
                    &player,
                    cmd,
                )
            },
        )?;
    if return_value
        .downcast::<PyBool>()
        .is_ok_and(|value| !value.is_true())
    {
        return Ok(PyBool::new(py, false).to_owned().into_any());
    };

    let updated_cmd = return_value.extract::<&str>().unwrap_or(cmd);

    RE_VOTE_ENDED.captures(updated_cmd).map_or(
        Ok(PyString::new(py, updated_cmd).into_any()),
        |captures| {
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
                    {
                        cold_path();
                        Err(PyEnvironmentError::new_err(
                            "could not get access to vote ended dispatcher",
                        ))
                    },
                    |vote_ended_dispatcher| {
                        let vote_passed = captures
                            .name("result")
                            .is_some_and(|value| value.as_str() == "passed");
                        VoteEndedDispatcherMethods::dispatch(
                            vote_ended_dispatcher.downcast()?,
                            vote_passed,
                        )
                    },
                )?;
            Ok(PyString::new(py, updated_cmd).into_any())
        },
    )
}

#[pyfunction]
pub(crate) fn handle_server_command<'py>(py: Python<'py>, client_id: i32, cmd: &str) -> Py<PyAny> {
    try_handle_server_command(py, client_id, cmd)
        .unwrap_or_else(|e| {
            log_exception(py, &e);
            PyBool::new(py, true).to_owned().into_any()
        })
        .unbind()
}

#[cfg(test)]
mod handle_server_command_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::*;

    use super::{handle_server_command, try_handle_server_command};
    use crate::{
        ffi::{
            c::{
                game_entity::MockGameEntityBuilder,
                prelude::{
                    CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES, CVar, CVarBuilder, MockClient,
                    clientState_t, cvar_t, privileges_t, team_t,
                },
            },
            python::{
                EVENT_DISPATCHERS,
                PythonReturnCodes::RET_STOP_EVENT,
                commands::CommandPriorities,
                events::{
                    EventDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                    EventDispatcherMethods, ServerCommandDispatcher, VoteEndedDispatcher,
                },
                pyshinqlx_setup_fixture::*,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_general_server_command(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ServerCommandDispatcher>())
                        .expect("could not add server_command dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "server_command"))
                        .and_then(|server_command_dispatcher| {
                            server_command_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to server_command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_server_command(py, -1, "cp \"asdf\"");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>()
                            .is_ok_and(|str_value| str_value == "cp \"asdf\"")
                    }));
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (py.None(), "cp \"asdf\"",)
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_dedicated_player_server_command(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<ServerCommandDispatcher>())
                                .expect("could not add server_command dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "server_command"))
                                .and_then(|server_command_dispatcher| {
                                    server_command_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to server_command dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_server_command(py, 42, "cp \"asdf\"");
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>()
                                    .is_ok_and(|str_value| str_value == "cp \"asdf\"")
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", "cp \"asdf\"",)
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_server_command_with_no_event_dispatcher(_pyshinqlx_setup: ()) {
        EVENT_DISPATCHERS.store(None);

        Python::attach(|py| {
            let result = try_handle_server_command(py, -1, "cp \"asdf\"");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_server_command_returning_false(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ServerCommandDispatcher>())
                        .expect("could not add server_command dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "server_command"))
                        .and_then(|server_command_dispatcher| {
                            server_command_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &python_function_returning(py, &(RET_STOP_EVENT as i32)),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to server_command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_server_command(py, -1, "cp \"asdf\"");
                    assert!(result.is_ok_and(|value| {
                        value
                            .downcast::<PyBool>()
                            .is_ok_and(|bool_value| !bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_server_command_returning_other_string(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ServerCommandDispatcher>())
                        .expect("could not add server_command dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "server_command"))
                        .and_then(|server_command_dispatcher| {
                            server_command_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &python_function_returning(py, &"quit"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to server_command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_server_command(py, -1, "cp \"asdf\"");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>()
                            .is_ok_and(|str_value| str_value == "quit")
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_indicating_vote_passed(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "42", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "1", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ServerCommandDispatcher>())
                        .expect("could not add server_command dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<VoteEndedDispatcher>())
                        .expect("could not add vote_ended dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "vote_ended"))
                        .and_then(|vote_ended_dispatcher| {
                            vote_ended_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to vote_ended dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_server_command(py, -1, "print \"Vote passed.\n\"");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>()
                            .is_ok_and(|str_value| str_value == "print \"Vote passed.\n\"")
                    }));
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                ((42, 1), "map", "thunderstruck", true,)
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_indicating_vote_failed(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_get_configstring(CS_VOTE_STRING as u16, "map thunderstruck", 1)
            .with_get_configstring(CS_VOTE_YES as u16, "1", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "42", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ServerCommandDispatcher>())
                        .expect("could not add server_command dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<VoteEndedDispatcher>())
                        .expect("could not add vote_ended dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "vote_ended"))
                        .and_then(|vote_ended_dispatcher| {
                            vote_ended_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to vote_ended dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_server_command(py, -1, "print \"Vote failed.\n\"");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>()
                            .is_ok_and(|str_value| str_value == "print \"Vote failed.\n\"")
                    }));
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                ((1, 42), "map", "thunderstruck", false,)
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_for_vote_ended_with_no_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<ServerCommandDispatcher>())
                .expect("could not add server_command dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_server_command(py, -1, "print \"Vote passed.\n\"");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_server_command_with_no_event_dispatchers(_pyshinqlx_setup: ()) {
        EVENT_DISPATCHERS.store(None);
        Python::attach(|py| {
            let result = handle_server_command(py, -1, "asdf");
            assert!(
                result
                    .downcast_bound::<PyBool>(py)
                    .is_ok_and(|value| value.is_true())
            );
        });
    }
}

fn try_run_frame_tasks(py: Python<'_>) -> PyResult<()> {
    py.import(intern!(py, "shinqlx"))
        .and_then(|shinqlx_module| shinqlx_module.getattr(intern!(py, "frame_tasks")))
        .and_then(|frame_tasks| {
            frame_tasks
                .call_method(
                    intern!(py, "run"),
                    (),
                    Some(&[(intern!(py, "blocking"), false)].into_py_dict(py)?),
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
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to frame dispatcher",
                ))
            },
            |frame_dispatcher| {
                FrameEventDispatcherMethods::dispatch(frame_dispatcher.downcast()?).map(|_| Ok(()))
            },
        )?
}

fn transfer_next_frame_tasks(py: Python<'_>) {
    PyModule::from_code(
        py,
        cr#"
from shinqlx import next_frame_tasks, frame_tasks

def next_frame_tasks_runner():
    while not next_frame_tasks.empty():
        func, args, kwargs = next_frame_tasks.get_nowait()
        frame_tasks.enter(0, 1, func, args, kwargs)
"#,
        c"",
        c"",
    )
    .and_then(|next_frame_tasks_runner| {
        next_frame_tasks_runner.call_method0(intern!(py, "next_frame_tasks_runner"))?;
        Ok(())
    })
    .unwrap_or_else(|e| log_exception(py, &e));
}

/// This will be called every frame. To allow threads to call stuff from the
/// main thread, tasks can be scheduled using the :func:`shinqlx.next_frame` decorator
/// and have it be executed here.
#[pyfunction]
pub(crate) fn handle_frame(py: Python<'_>) -> Option<bool> {
    while try_run_frame_tasks(py)
        .tap_err(|e| {
            log_exception(py, e);
        })
        .is_err()
    {}

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
mod handle_frame_tests {
    use core::borrow::BorrowMut;

    use pyo3::{
        exceptions::{PyEnvironmentError, PyValueError},
        intern,
        prelude::*,
        types::{IntoPyDict, PyBool, PyDict, PyTuple},
    };
    use rstest::rstest;

    use super::{handle_frame, transfer_next_frame_tasks, try_handle_frame, try_run_frame_tasks};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                EVENT_DISPATCHERS, EventDispatcher, EventDispatcherManager,
                EventDispatcherManagerMethods, EventDispatcherMethods, FrameEventDispatcher,
                commands::CommandPriorities, pyshinqlx_setup, pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_run_frame_tasks_with_no_pending_tasks(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            py.run(
                cr#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(
                    &[("frame_tasks", frame_tasks)]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
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
        Python::attach(|py| {
            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            py.run(
                cr#"
def throws_exception():
    raise ValueError("stop calling me!")

for event in frame_tasks.queue:
    frame_tasks.cancel(event)

frame_tasks.enter(0, 1, throws_exception, (), {})
"#,
                None,
                Some(
                    &[("frame_tasks", frame_tasks)]
                        .into_py_dict(py)
                        .expect("this shouöd not happen"),
                ),
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
        Python::attach(|py| {
            let capturing_hook = capturing_hook(py);
            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            py.run(
                cr#"
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
                                .getattr(intern!(py, "hook"))
                                .expect("could not get capturing hook"),
                        ),
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            )
            .expect("this should not happend");

            let result = try_run_frame_tasks(py);
            assert!(result.is_ok());
            assert!(
                capturing_hook
                    .call_method1(intern!(py, "assert_called_with"), ("asdf", 42,))
                    .is_ok()
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_frame_with_hook(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<FrameEventDispatcher>())
                        .expect("could not add frame dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "frame"))
                        .and_then(|frame_dispatcher| {
                            frame_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to frame dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_frame(py);
                    assert!(result.is_ok());
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), ())
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_frame_with_no_event_dispatchers(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            EVENT_DISPATCHERS.store(None);

            let result = try_handle_frame(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn transfer_next_frame_tasks_with_none_pending(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let next_frame_tasks = shinqlx_module
                .getattr(intern!(py, "next_frame_tasks"))
                .expect("this should not happen");
            py.run(
                cr#"
while not next_frame_tasks.empty():
    next_frame_tasks.get_nowait()
            "#,
                None,
                Some(
                    &[("next_frame_tasks", next_frame_tasks.to_owned())]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            )
            .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            py.run(
                cr#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(
                    &[("frame_tasks", frame_tasks.to_owned())]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            )
            .expect("this should not happend");

            transfer_next_frame_tasks(py);
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| {
                        value
                            .downcast::<PyBool>()
                            .expect("this should not happen")
                            .is_true()
                    })
            );
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| {
                        value
                            .downcast::<PyBool>()
                            .expect("this should not happen")
                            .is_true()
                    })
            );
            py.run(
                cr#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(
                    &[("frame_tasks", frame_tasks.to_owned())]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            )
            .expect("this should not happend");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn transfer_next_frame_tasks_with_pending_tasks_for_next_frame(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let capturing_hook = capturing_hook(py);
            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let next_frame_tasks = shinqlx_module
                .getattr(intern!(py, "next_frame_tasks"))
                .expect("this should not happen");
            next_frame_tasks
                .call_method1(
                    intern!(py, "put_nowait"),
                    ((capturing_hook, PyTuple::empty(py), PyDict::new(py)),),
                )
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            py.run(
                cr#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(
                    &[("frame_tasks", frame_tasks.to_owned())]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            )
            .expect("this should not happend");

            transfer_next_frame_tasks(py);
            assert!(
                frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| {
                        !value
                            .downcast::<PyBool>()
                            .expect("this should not happen")
                            .is_true()
                    })
            );
            assert!(
                next_frame_tasks
                    .call_method0(intern!(py, "empty"))
                    .is_ok_and(|value| {
                        value
                            .downcast::<PyBool>()
                            .expect("this should not happen")
                            .is_true()
                    })
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_frame_when_frame_tasks_throws_exception(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let shinqlx_module = py
                        .import(intern!(py, "shinqlx"))
                        .expect("this should not happen");
                    let frame_tasks = shinqlx_module
                        .getattr(intern!(py, "frame_tasks"))
                        .expect("this should not happen");
                    py.run(
                        cr#"
def throws_exception():
    raise ValueError("stop calling me!")

for event in frame_tasks.queue:
    frame_tasks.cancel(event)

frame_tasks.enter(0, 1, throws_exception, (), {})
"#,
                        None,
                        Some(
                            &[("frame_tasks", frame_tasks)]
                                .into_py_dict(py)
                                .expect("this should not happen"),
                        ),
                    )
                    .expect("this should not happend");

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<FrameEventDispatcher>())
                        .expect("could not add frame dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "frame"))
                        .and_then(|frame_dispatcher| {
                            frame_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to frame dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = handle_frame(py);
                    assert!(result.is_none());
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), ())
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_frame_when_frame_handler_throws_exception(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let shinqlx_module = py
                .import(intern!(py, "shinqlx"))
                .expect("this should not happen");
            let frame_tasks = shinqlx_module
                .getattr(intern!(py, "frame_tasks"))
                .expect("this should not happen");
            py.run(
                cr#"
for event in frame_tasks.queue:
    frame_tasks.cancel(event)
"#,
                None,
                Some(
                    &[("frame_tasks", frame_tasks)]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            )
            .expect("this should not happend");

            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this shold not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<FrameEventDispatcher>())
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
    let shinqlx_module = py.import(intern!(py, "shinqlx"))?;
    if IS_FIRST_GAME.load(Ordering::Acquire) {
        late_init(&shinqlx_module)?;
        IS_FIRST_GAME.store(false, Ordering::Release);

        let zmq_enabled = MAIN_ENGINE.load().as_ref().is_some_and(|main_engine| {
            main_engine
                .find_cvar("zmq_stats_enable")
                .is_some_and(|cvar| cvar.get_string() != "0")
        });
        if !zmq_enabled && !ZMQ_WARNING_ISSUED.load(Ordering::Acquire) {
            pyshinqlx_get_logger(py, None).and_then(|logger| {
                let warning_level = py.import(intern!(py, "logging")).and_then(|logging_module| logging_module.getattr(intern!(py, "WARNING")))?;
                logger.call_method(
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
                        &[(intern!(py, "func"), intern!(py, "handle_new_game"))].into_py_dict(py)?,
                    ),
                ).and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
            })?;

            ZMQ_WARNING_ISSUED.store(true, Ordering::Release);
        }
    }

    set_map_subtitles(&shinqlx_module)?;

    if !is_restart {
        let map_name = get_cvar("mapname")?;
        let factory_name = get_cvar("g_factory")?;
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers.bind(py).get_item(intern!(py, "map")).ok()
            })
            .map_or(
                {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to map dispatcher",
                    ))
                },
                |map_dispatcher| {
                    MapDispatcherMethods::dispatch(
                        map_dispatcher.downcast()?,
                        &map_name.unwrap_or_default(),
                        &factory_name.unwrap_or_default(),
                    )
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
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to new game dispatcher",
                ))
            },
            |new_game_dispatcher| {
                NewGameDispatcherMethods::dispatch(new_game_dispatcher.downcast()?)
            },
        )?;

    Ok(())
}

#[pyfunction]
pub(crate) fn handle_new_game(py: Python<'_>, is_restart: bool) -> Option<bool> {
    match try_handle_new_game(py, is_restart).tap_err(|e| {
        cold_path();
        log_exception(py, e);
    }) {
        Err(_) => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod handle_new_game_tests {
    use alloc::ffi::CString;
    use core::{borrow::BorrowMut, sync::atomic::Ordering};

    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*};
    use rstest::*;

    use super::{IS_FIRST_GAME, ZMQ_WARNING_ISSUED, handle_new_game, try_handle_new_game};
    use crate::{
        ffi::{
            c::prelude::{CS_AUTHOR, CS_AUTHOR2, CS_MESSAGE, CVar, CVarBuilder, cvar_t},
            python::{
                EVENT_DISPATCHERS,
                commands::CommandPriorities,
                events::{
                    EventDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                    EventDispatcherMethods, MapDispatcher, NewGameDispatcher,
                },
                pyshinqlx_setup,
                pyshinqlx_test_support::{run_all_frame_tasks, *},
            },
        },
        hooks::mock_hooks::shinqlx_set_configstring_context,
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_game_restarted_stores_map_titles_and_authors(_pyshinqlx_setup: ()) {
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

        IS_FIRST_GAME.store(false, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(true, Ordering::Release);

        MockEngineBuilder::default()
            .with_get_configstring(CS_MESSAGE as u16, "thunderstruck", 1)
            .with_get_configstring(CS_AUTHOR as u16, "Till 'Firestarter' Merker", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "None", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<NewGameDispatcher>())
                        .expect("could not add new_game dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_new_game(py, true);
                    assert!(result.is_ok());

                    let pyshinqlx_module = py
                        .import(intern!(py, "shinqlx"))
                        .expect("this should not happen");
                    assert!(
                        pyshinqlx_module
                            .getattr(intern!(py, "_map_title"))
                            .and_then(|value| value.extract::<String>())
                            .is_ok_and(|str_value| str_value == "thunderstruck")
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_game_restarted_invokes_new_game_dispatcher(_pyshinqlx_setup: ()) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(true, Ordering::Release);

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_get_configstring(CS_MESSAGE as u16, "", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<NewGameDispatcher>())
                        .expect("could not add new_game dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "new_game"))
                        .and_then(|new_game_dispatcher| {
                            new_game_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to new_game dispatcher");
                    run_all_frame_tasks(py).expect("this should not happen");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_new_game(py, true);
                    assert!(result.is_ok());

                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), ())
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_game_restarted_with_missing_new_game_dispatcher(
        _pyshinqlx_setup: (),
    ) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(true, Ordering::Release);

        MockEngineBuilder::default()
            .with_get_configstring(CS_MESSAGE as u16, "", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = EventDispatcherManager::default();
                    run_all_frame_tasks(py).expect("this should not happen");
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    let result = try_handle_new_game(py, true);
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_new_map_loaded_invokes_map_dispatcher(_pyshinqlx_setup: ()) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(true, Ordering::Release);

        let cvar_string = c"1";
        let mut raw_zmq_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let map_string = c"campgrounds";
        let mut raw_mapname_cvar = CVarBuilder::default()
            .string(map_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let factory_string = c"ffa";
        let mut raw_factory_cvar = CVarBuilder::default()
            .string(factory_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "mapname",
                move |_| CVar::try_from(raw_mapname_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "g_factory",
                move |_| CVar::try_from(raw_factory_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_get_configstring(CS_MESSAGE as u16, "", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<NewGameDispatcher>())
                        .expect("could not add new_game dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<MapDispatcher>())
                        .expect("could not add map dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "new_game"))
                        .and_then(|new_game_dispatcher| {
                            new_game_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to new_game dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "map"))
                        .and_then(|map_dispatcher| {
                            map_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to map dispatcher");
                    run_all_frame_tasks(py).expect("this should not happen");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_new_game(py, false);
                    assert!(result.is_ok());

                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), ("campgrounds", "ffa"))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), ())
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_new_game_when_new_map_loaded_with_missing_map_dispatcher(_pyshinqlx_setup: ()) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(true, Ordering::Release);

        let map_string = c"campgrounds";
        let mut raw_mapname_cvar = CVarBuilder::default()
            .string(map_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let g_factory_string = c"ffa";
        let mut raw_factory_cvar = CVarBuilder::default()
            .string(g_factory_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "mapname",
                move |_| CVar::try_from(raw_mapname_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "g_factory",
                move |_| CVar::try_from(raw_factory_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_get_configstring(CS_MESSAGE as u16, "", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<NewGameDispatcher>())
                        .expect("could not add new_game dispatcher");
                    run_all_frame_tasks(py).expect("this should not happen");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_new_game(py, false);
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    static TEMP_DIR: std::sync::LazyLock<tempfile::TempDir> = std::sync::LazyLock::new(|| {
        tempfile::Builder::new()
            .tempdir()
            .expect("this should not happen")
    });

    #[rstest]
    #[cfg_attr(any(miri, target_os = "macos"), ignore)]
    #[serial]
    fn try_handle_new_game_when_first_game_with_zmq_enabled(_pyshinqlx_setup: ()) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(true, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(false, Ordering::Release);

        let temp_path = CString::new(TEMP_DIR.path().to_string_lossy().to_string())
            .expect("this should not happen");
        let cvar_string = c"1";
        let mut raw_zmq_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(temp_path.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |name| ["qlx_pluginsPath", "fs_homepath"].contains(&name),
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |name| {
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
                },
                |_| None,
                1..,
            )
            .with_get_configstring(CS_MESSAGE as u16, "", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .configure(|mock_engine| {
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
            })
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<NewGameDispatcher>())
                        .expect("could not add new_game dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_new_game(py, true);
                    let _ = py
                        .import(intern!(py, "shinqlx"))
                        .and_then(|module| module.setattr(intern!(py, "_stats"), py.None()));
                    assert!(result.is_ok());

                    assert!(!IS_FIRST_GAME.load(Ordering::Acquire));
                    assert!(!ZMQ_WARNING_ISSUED.load(Ordering::Acquire));
                });
            });
    }

    #[rstest]
    #[cfg_attr(any(miri, target_os = "macos"), ignore)]
    #[serial]
    fn try_handle_new_game_when_first_game_with_zmq_disabled(_pyshinqlx_setup: ()) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(true, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(false, Ordering::Release);

        let temp_path = CString::new(TEMP_DIR.path().to_string_lossy().to_string())
            .expect("this should not happen");
        let cvar_string = c"0";
        let mut raw_zmq_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(temp_path.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |name| ["qlx_pluginsPath", "fs_homepath"].contains(&name),
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |name| {
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
                },
                |_| None,
                1..,
            )
            .with_get_configstring(CS_MESSAGE as u16, "", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .configure(|mock_engine| {
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
            })
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<NewGameDispatcher>())
                        .expect("could not add new_game dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_new_game(py, true);
                    let _ = py
                        .import(intern!(py, "shinqlx"))
                        .and_then(|module| module.setattr(intern!(py, "_stats"), py.None()));
                    assert!(result.is_ok());

                    assert!(!IS_FIRST_GAME.load(Ordering::Acquire));
                    assert!(ZMQ_WARNING_ISSUED.load(Ordering::Acquire));
                });
            });
    }

    #[rstest]
    #[cfg_attr(any(miri, target_os = "macos"), ignore)]
    #[serial]
    fn try_handle_new_game_when_first_game_with_zmq_disabled_when_warning_already_issued(
        _pyshinqlx_setup: (),
    ) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(true, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(true, Ordering::Release);

        let temp_path = CString::new(TEMP_DIR.path().to_string_lossy().to_string())
            .expect("this should not happen");
        let cvar_string = c"0";
        let mut raw_zmq_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let mut raw_pluginspath_cvar = CVarBuilder::default()
            .string(temp_path.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |name| ["qlx_pluginsPath", "fs_homepath"].contains(&name),
                move |_| CVar::try_from(raw_pluginspath_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |name| {
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
                },
                |_| None,
                1..,
            )
            .with_get_configstring(CS_MESSAGE as u16, "", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .configure(|mock_engine| {
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
            })
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<NewGameDispatcher>())
                        .expect("could not add new_game dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_new_game(py, true);
                    let _ = py
                        .import(intern!(py, "shinqlx"))
                        .and_then(|module| module.setattr(intern!(py, "_stats"), py.None()));
                    assert!(result.is_ok());

                    assert!(!IS_FIRST_GAME.load(Ordering::Acquire));
                    assert!(ZMQ_WARNING_ISSUED.load(Ordering::Acquire));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_new_game_when_game_restarted_with_missing_new_game_dispatcher(_pyshinqlx_setup: ()) {
        EVENT_DISPATCHERS.store(None);

        IS_FIRST_GAME.store(false, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(true, Ordering::Release);

        Python::attach(|py| {
            let result = handle_new_game(py, true);
            assert!(result.is_some_and(|value| value));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_new_game_when_dispatcher_returns_ok(_pyshinqlx_setup: ()) {
        let set_configstring_ctx = shinqlx_set_configstring_context();
        set_configstring_ctx
            .expect()
            .withf(|index, _| [CS_AUTHOR, CS_AUTHOR2].contains(index));

        IS_FIRST_GAME.store(false, Ordering::Release);
        ZMQ_WARNING_ISSUED.store(true, Ordering::Release);

        MockEngineBuilder::default()
            .with_get_configstring(CS_MESSAGE as u16, "", 1)
            .with_get_configstring(CS_AUTHOR as u16, "", 1)
            .with_get_configstring(CS_AUTHOR2 as u16, "", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<NewGameDispatcher>())
                        .expect("could not add new_game dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = handle_new_game(py, true);
                    assert!(result.is_none());
                });
            });
    }
}

static AD_ROUND_NUMBER: AtomicI32 = AtomicI32::new(0);

fn try_handle_set_configstring(py: Python<'_>, index: u32, value: &str) -> PyResult<Py<PyAny>> {
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
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to set configstring dispatcher",
                ))
            },
            |set_configstring_dispatcher| {
                SetConfigstringDispatcherMethods::dispatch(
                    set_configstring_dispatcher.downcast()?,
                    index,
                    value,
                )
            },
        )?;

    if result
        .downcast::<PyBool>()
        .is_ok_and(|result_value| !result_value.is_true())
    {
        return Ok(PyBool::new(py, false).to_owned().into_any().unbind());
    }

    let configstring_value = result.extract::<String>().unwrap_or(value.to_string());
    match index {
        CS_VOTE_STRING if !configstring_value.is_empty() => {
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
                    {
                        cold_path();
                        Err(PyEnvironmentError::new_err(
                            "could not get access to vote started dispatcher",
                        ))
                    },
                    |vote_started_dispatcher| {
                        VoteStartedDispatcherMethods::dispatch(
                            vote_started_dispatcher.downcast()?,
                            vote,
                            PyString::new(py, args).as_any(),
                        )?;

                        Ok(py.None())
                    },
                )
        }
        CS_SERVERINFO => {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                cold_path();
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
                return Ok(PyString::new(py, &configstring_value).into_any().unbind());
            }
            if (old_state, new_state) == ("PRE_GAME", "COUNT_DOWN") {
                AD_ROUND_NUMBER.store(1, Ordering::Release);
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
                        {
                            cold_path();
                            Err(PyEnvironmentError::new_err(
                                "could not get access to game countdown dispatcher",
                            ))
                        },
                        |game_countdown_dispatcher| {
                            GameCountdownDispatcherMethods::dispatch(
                                game_countdown_dispatcher.downcast()?,
                            )
                            .map(|_| ())
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
                pyshinqlx_get_logger(py, None).and_then(|logger| {
                    let warning = format!("UNKNOWN GAME STATES: {old_state} - {new_state}");
                    let warning_level =
                        py.import(intern!(py, "logging"))
                            .and_then(|logging_module| {
                                logging_module.getattr(intern!(py, "WARNING"))
                            })?;
                    logger
                        .call_method(
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
                                    .into_py_dict(py)?,
                            ),
                        )
                        .and_then(|log_record| {
                            logger.call_method1(intern!(py, "handle"), (log_record,))
                        })
                })?;
            }
            Ok(PyString::new(py, &configstring_value).into_any().unbind())
        }
        CS_ROUND_STATUS => {
            let cvars = parse_variables(&configstring_value);
            if cvars.is_empty() {
                return Ok(PyString::intern(py, &configstring_value)
                    .into_any()
                    .unbind());
            }

            let cs_round_number = cvars.get("round").map_or(
                {
                    cold_path();
                    Err(PyKeyError::new_err("'round'"))
                },
                |round_str| {
                    round_str.parse::<i32>().map_err(|_| {
                        let error_msg =
                            format!("invalid literal for int() with base 10: {round_str}");
                        PyValueError::new_err(error_msg)
                    })
                },
            )?;
            let round_number = if cvars.contains("turn") {
                let cs_state = cvars.get("state").map_or(
                    {
                        cold_path();
                        Err(PyKeyError::new_err("'state'"))
                    },
                    |state_str| {
                        state_str.parse::<i32>().map_err(|_| {
                            let error_msg =
                                format!("invalid literal for int() with base 10: {state_str}");
                            PyValueError::new_err(error_msg)
                        })
                    },
                )?;
                if cs_state == 0 {
                    return Ok(py.None());
                }

                let cs_turn = cvars.get("turn").map_or(
                    {
                        cold_path();
                        Err(PyKeyError::new_err("'turn'"))
                    },
                    |turn_str| {
                        turn_str.parse::<i32>().map_err(|_| {
                            let error_msg =
                                format!("invalid literal for int() with base 10: {turn_str}");
                            PyValueError::new_err(error_msg)
                        })
                    },
                )?;
                let ad_round_number = cs_round_number * 2 + 1 + cs_turn;
                AD_ROUND_NUMBER.store(ad_round_number, Ordering::Release);
                AD_ROUND_NUMBER.load(Ordering::Acquire)
            } else {
                if cs_round_number == 0 {
                    return Ok(PyString::new(py, &configstring_value).into_any().unbind());
                }
                cs_round_number
            };

            if cvars.contains("time") {
                EVENT_DISPATCHERS
                    .load()
                    .as_ref()
                    .and_then(|event_dispatchers| {
                        event_dispatchers
                            .bind(py)
                            .get_item(intern!(py, "round_countdown"))
                            .ok()
                    })
                    .map_or(
                        {
                            cold_path();
                            Err(PyEnvironmentError::new_err(
                                "could not get access to round countdown dispatcher",
                            ))
                        },
                        |round_dispatcher| {
                            RoundCountdownDispatcherMethods::dispatch(
                                round_dispatcher.downcast()?,
                                round_number,
                            )
                            .map(|_| py.None())
                        },
                    )
            } else {
                EVENT_DISPATCHERS
                    .load()
                    .as_ref()
                    .and_then(|event_dispatchers| {
                        event_dispatchers
                            .bind(py)
                            .get_item(intern!(py, "round_start"))
                            .ok()
                    })
                    .map_or(
                        {
                            cold_path();
                            Err(PyEnvironmentError::new_err(
                                "could not get access to round start dispatcher",
                            ))
                        },
                        |round_dispatcher| {
                            RoundStartDispatcherMethods::dispatch(
                                round_dispatcher.downcast()?,
                                round_number,
                            )
                            .map(|_| py.None())
                        },
                    )
            }
        }
        _ => Ok(PyString::new(py, &configstring_value).into_any().unbind()),
    }
}

/// Called whenever the server tries to set a configstring. Can return
/// False to stop the event.
#[pyfunction]
pub(crate) fn handle_set_configstring(py: Python<'_>, index: u32, value: &str) -> Py<PyAny> {
    try_handle_set_configstring(py, index, value).unwrap_or_else(|e| {
        log_exception(py, &e);
        PyBool::new(py, true).to_owned().into_any().unbind()
    })
}

#[cfg(test)]
mod handle_set_configstring_tests {
    use core::{borrow::BorrowMut, sync::atomic::Ordering};

    use pretty_assertions::assert_eq;
    use pyo3::{
        exceptions::{PyAssertionError, PyEnvironmentError, PyKeyError, PyValueError},
        intern,
        prelude::*,
        types::PyBool,
    };
    use rstest::rstest;

    use super::{AD_ROUND_NUMBER, handle_set_configstring, try_handle_set_configstring};
    use crate::{
        ffi::{
            c::prelude::{
                CS_ALLREADY_TIME, CS_AUTHOR, CS_ROUND_STATUS, CS_SERVERINFO, CS_VOTE_STRING, CVar,
                CVarBuilder, cvar_t,
            },
            python::{
                EVENT_DISPATCHERS,
                PythonReturnCodes::RET_STOP_EVENT,
                commands::CommandPriorities,
                events::{
                    EventDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                    EventDispatcherMethods, GameCountdownDispatcher, RoundCountdownDispatcher,
                    RoundStartDispatcher, SetConfigstringDispatcher, VoteStartedDispatcher,
                },
                pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_forwards_to_python(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "set_configstring"))
                        .and_then(|set_configstring_dispatcher| {
                            set_configstring_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to set_configstring dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_set_configstring(py, CS_AUTHOR, "ShiN0");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>(py)
                            .is_ok_and(|str_value| str_value == "ShiN0")
                    }));
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), (CS_AUTHOR, "ShiN0"))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_when_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "set_configstring"))
                        .and_then(|set_configstring_dispatcher| {
                            set_configstring_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &python_function_returning(py, &(RET_STOP_EVENT as i32)),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to set_configstring dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_set_configstring(py, CS_AUTHOR, "ShiN0");
                    assert!(result.is_ok_and(|value| {
                        value
                            .bind(py)
                            .downcast::<PyBool>()
                            .is_ok_and(|bool_value| !bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_when_dispatcher_is_missing(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_when_dispatcher_returns_other_value(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "set_configstring"))
                        .and_then(|set_configstring_dispatcher| {
                            set_configstring_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &python_function_returning(py, &"quit"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to set_configstring dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_set_configstring(py, CS_AUTHOR, "ShiN0");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>(py)
                            .is_ok_and(|str_value| str_value == "quit")
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_vote_string_change_with_one_word_vote(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "vote_started"))
                        .and_then(|vote_start_dispatcher| {
                            vote_start_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_set_configstring(py, CS_VOTE_STRING, "restart");
                    assert!(result.is_ok_and(|value| value.is_none(py)));
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (py.None(), "restart", "")
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_vote_string_change_with_multiword_vote(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "vote_started"))
                        .and_then(|vote_start_dispatcher| {
                            vote_start_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result =
                        try_handle_set_configstring(py, CS_VOTE_STRING, "map thunderstruck");
                    assert!(result.is_ok_and(|value| value.is_none(py)));
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (py.None(), "map", "thunderstruck")
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_vote_string_change_with_empty_votestring(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "vote_started"))
                        .and_then(|vote_start_dispatcher| {
                            vote_start_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_set_configstring(py, CS_VOTE_STRING, "");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>(py)
                            .is_ok_and(|str_value| str_value.is_empty())
                    }));
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (py.None(), "map", "thunderstruck")
                            )
                            .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py))
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_vote_string_change_with_no_vote_started_dispatcher(
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(py, CS_VOTE_STRING, "kick ShiN0");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_server_info_change_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\PRE_GAME");
            assert!(result.is_ok_and(|value| value.is_none(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_server_info_change_with_no_prior_info_set(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result =
                        try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\PRE_GAME");
                    assert!(result.is_ok_and(|value| value.is_none(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_server_info_change_with_same_gamestate_as_before(
        _pyshinqlx_setup: (),
    ) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gameState\PRE_GAME", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result =
                        try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\PRE_GAME");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>(py)
                            .is_ok_and(|str_value| str_value == r"\g_gameState\PRE_GAME")
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_game_countdown_change(_pyshinqlx_setup: ()) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gameState\PRE_GAME", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<GameCountdownDispatcher>())
                        .expect("could not add game_countdown dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "game_countdown"))
                        .and_then(|game_countdown_dispatcher| {
                            game_countdown_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to game_countdown dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    AD_ROUND_NUMBER.store(42, Ordering::Release);
                    let result =
                        try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\COUNT_DOWN");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>(py)
                            .is_ok_and(|str_value| str_value == r"\g_gameState\COUNT_DOWN")
                    }));
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), ())
                            .is_ok()
                    );
                    assert_eq!(AD_ROUND_NUMBER.load(Ordering::Acquire), 1);
                });
            });
    }

    #[rstest]
    #[case("PRE_GAME", "IN_PROGRESS")]
    #[case("COUNT_DOWN", "IN_PROGRESS")]
    #[case("IN_PROGRESS", "PRE_GAME")]
    #[case("COUNT_DOWN", "PRE_GAME")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_valid_changes(
        _pyshinqlx_setup: (),
        #[case] old_state: &'static str,
        #[case] new_state: &str,
    ) {
        let new_configstring = format!(r"\g_gameState\{new_state}");

        MockEngineBuilder::default()
            .with_get_configstring(
                CS_SERVERINFO as u16,
                format!(r"\g_gameState\{old_state}"),
                1,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_set_configstring(py, CS_SERVERINFO, &new_configstring);
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>(py)
                            .is_ok_and(|str_value| str_value == new_configstring)
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_game_countdown_change_with_missing_countdown_dispatcher(
        _pyshinqlx_setup: (),
    ) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gameState\PRE_GAME", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    AD_ROUND_NUMBER.store(42, Ordering::Release);
                    let result =
                        try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\COUNT_DOWN");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                    assert_eq!(AD_ROUND_NUMBER.load(Ordering::Acquire), 1);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_invalid_state_change(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gameState\IN_PROGRESS", 1)
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result =
                        try_handle_set_configstring(py, CS_SERVERINFO, r"\g_gameState\COUNT_DOWN");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>(py)
                            .is_ok_and(|str_value| str_value == r"\g_gameState\COUNT_DOWN")
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_with_no_turn_in_value_triggering_round_start(
        _pyshinqlx_setup: (),
    ) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<RoundStartDispatcher>())
                        .expect("could not add round_start dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "round_start"))
                        .and_then(|round_start_dispatcher| {
                            round_start_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to round_start dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\7");
                    assert!(result.is_ok_and(|value| value.is_none(py)));
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), (7,))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_for_ad_triggering_round_start(
        _pyshinqlx_setup: (),
    ) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<RoundStartDispatcher>())
                        .expect("could not add round_start dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "round_start"))
                        .and_then(|round_start_dispatcher| {
                            round_start_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to round_start dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_set_configstring(
                        py,
                        CS_ROUND_STATUS,
                        r"\round\7\turn\3\state\1",
                    );
                    assert!(result.is_ok_and(|value| value.is_none(py)));
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), (18,))
                            .is_ok()
                    );
                    assert_eq!(AD_ROUND_NUMBER.load(Ordering::Acquire), 18);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_with_no_turn_in_value_triggering_round_countdown(
        _pyshinqlx_setup: (),
    ) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<RoundCountdownDispatcher>())
                        .expect("could not add round_countdown dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "round_countdown"))
                        .and_then(|round_countdown_dispatcher| {
                            round_countdown_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to round_countdown dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result =
                        try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\7\time\11");
                    assert!(result.is_ok_and(|value| value.is_none(py)));
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), (7,))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_ad_triggering_round_countdown(
        _pyshinqlx_setup: (),
    ) {
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                        .expect("could not add set_configstring dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<RoundCountdownDispatcher>())
                        .expect("could not add round_countdown dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "round_countdown"))
                        .and_then(|round_countdown_dispatcher| {
                            round_countdown_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to round_countdown dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_set_configstring(
                        py,
                        CS_ROUND_STATUS,
                        r"\round\3\turn\1\state\3\time\11",
                    );
                    assert!(result.is_ok_and(|value| value.is_none(py)));
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), (8,))
                            .is_ok()
                    );
                    assert_eq!(AD_ROUND_NUMBER.load(Ordering::Acquire), 8);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_with_no_turn_in_value_triggering_round_countdown_with_no_dispatcher(
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\7\time\11");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_for_ad_triggering_round_countdown_with_no_dispatcher(
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(
                py,
                CS_ROUND_STATUS,
                r"\round\7\turn\1\state\2\time\11",
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_with_no_turn_in_value_triggering_round_start_with_no_dispatcher(
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\7");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_for_ad_triggering_round_start_with_no_dispatcher(
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result =
                try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\7\turn\2\state\5");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_with_empty_string(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(py, CS_ROUND_STATUS, "");
            assert!(result.is_ok_and(|value| {
                value
                    .extract::<String>(py)
                    .is_ok_and(|str_value| str_value.is_empty())
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_for_round_zero(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\0");
            assert!(result.is_ok_and(|value| {
                value
                    .extract::<String>(py)
                    .is_ok_and(|str_value| str_value == r"\round\0")
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_for_ad_state_zero(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result =
                try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\0\turn\0\state\0");
            assert!(result.is_ok_and(|value| value.is_none(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_for_unparseable_round_number(
        _pyshinqlx_setup: (),
    ) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_for_unparseable_state(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result =
                try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\1\turn\1\state\asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_for_unparseable_turn(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result =
                try_handle_set_configstring(py, CS_ROUND_STATUS, r"\round\1\turn\asdf\state\1");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_with_no_round_number(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(py, CS_ROUND_STATUS, r"\asdf\asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_set_configstring_for_round_status_change_with_no_state(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<SetConfigstringDispatcher>())
                .expect("could not add set_configstring dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_set_configstring(py, CS_ROUND_STATUS, r"\asdf\1\turn\1");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_set_configstring_with_no_event_dispatchers(_pyshinqlx_setup: ()) {
        EVENT_DISPATCHERS.store(None);

        Python::attach(|py| {
            let result = handle_set_configstring(py, CS_ALLREADY_TIME, "42");
            assert!(
                result
                    .bind(py)
                    .downcast::<PyBool>()
                    .is_ok_and(|bool_value| bool_value.is_true())
            );
        });
    }
}

fn try_handle_player_connect(py: Python<'_>, client_id: i32, _is_bot: bool) -> PyResult<Py<PyAny>> {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "player_connect"))
                .ok()
        })
        .map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to player connect dispatcher",
                ))
            },
            |player_connect_dispatcher| {
                let player = Player::py_new(client_id, None)?;

                PlayerConnectDispatcherMethods::dispatch(
                    player_connect_dispatcher.downcast()?,
                    &Bound::new(py, player)?,
                )
                .map(|value| value.unbind())
            },
        )
}

/// This will be called whenever a player tries to connect. If the dispatcher
/// returns False, it will not allow the player to connect and instead show them
/// a message explaining why. The default message is "You are banned from this
/// server.", but it can be set with :func:`shinqlx.set_ban_message`.
#[pyfunction]
pub(crate) fn handle_player_connect(py: Python<'_>, client_id: i32, is_bot: bool) -> Py<PyAny> {
    try_handle_player_connect(py, client_id, is_bot).unwrap_or_else(|e| {
        log_exception(py, &e);
        PyBool::new(py, true).to_owned().into_any().unbind()
    })
}

#[cfg(test)]
mod handle_player_connect_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::*;

    use super::{handle_player_connect, try_handle_player_connect};
    use crate::{
        ffi::{
            c::prelude::{
                CVar, CVarBuilder, MockClient, MockGameEntityBuilder, clientState_t, cvar_t,
                privileges_t, team_t,
            },
            python::{
                EVENT_DISPATCHERS,
                commands::CommandPriorities,
                events::{
                    EventDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                    EventDispatcherMethods, PlayerConnectDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_connect_forwards_to_dispatcher(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<PlayerConnectDispatcher>())
                                .expect("could not add player_connect dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "player_connect"))
                                .and_then(|player_connect_dispatcher| {
                                    player_connect_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to player_connect dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_player_connect(py, 42, false);
                            assert!(result.as_ref().is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| bool_value.is_true())
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_",))
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_connect_with_no_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_player_connect(py, 42, false);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_connect_when_dispatcher_returns_other_string(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<PlayerConnectDispatcher>())
                                .expect("could not add player_connect dispatcher");
                            event_dispatcher
                                .get_item(intern!(py, "player_connect"))
                                .and_then(|player_connect_dispatcher| {
                                    player_connect_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &python_function_returning(py, &"quit"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to player_connect dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_player_connect(py, 42, false);
                            assert!(result.is_ok_and(|value| {
                                value
                                    .extract::<String>(py)
                                    .is_ok_and(|str_value| str_value == "quit")
                            }));
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_player_connect_when_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::attach(|py| {
                let event_dispatcher = EventDispatcherManager::default();
                EVENT_DISPATCHERS.store(Some(
                    Py::new(py, event_dispatcher)
                        .expect("could not create event dispatcher manager in python")
                        .into(),
                ));

                let result = handle_player_connect(py, 42, false);
                assert!(
                    result
                        .bind(py)
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true())
                );
            });
        });
    }
}

fn try_handle_player_loaded(py: Python<'_>, client_id: i32) -> PyResult<Py<PyAny>> {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "player_loaded"))
                .ok()
        })
        .map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to player loaded dispatcher",
                ))
            },
            |player_loaded_dispatcher| {
                let player = Player::py_new(client_id, None)?;

                PlayerLoadedDispatcherMethods::dispatch(
                    player_loaded_dispatcher.downcast()?,
                    &Bound::new(py, player)?,
                )
                .map(|value| value.unbind())
            },
        )
}

/// This will be called whenever a player has connected and finished loading,
/// meaning it'll go off a bit later than the usual "X connected" messages.
/// This will not trigger on bots.his will be called whenever a player tries to connect. If the dispatcher
#[pyfunction]
pub(crate) fn handle_player_loaded(py: Python<'_>, client_id: i32) -> Py<PyAny> {
    try_handle_player_loaded(py, client_id).unwrap_or_else(|e| {
        log_exception(py, &e);
        PyBool::new(py, true).to_owned().into_any().unbind()
    })
}

#[cfg(test)]
mod handle_player_loaded_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::*;

    use super::{handle_player_loaded, try_handle_player_loaded};
    use crate::{
        ffi::{
            c::prelude::{
                CVar, CVarBuilder, MockClient, MockGameEntityBuilder, clientState_t, cvar_t,
                privileges_t, team_t,
            },
            python::{
                EVENT_DISPATCHERS,
                commands::CommandPriorities,
                events::{
                    EventDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                    EventDispatcherMethods, PlayerLoadedDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_loaded_forwards_to_dispatcher(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<PlayerLoadedDispatcher>())
                                .expect("could not add client_loaded dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "player_loaded"))
                                .and_then(|player_loaded_dispatcher| {
                                    player_loaded_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to player_loaded dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_player_loaded(py, 42);
                            assert!(result.as_ref().is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| bool_value.is_true())
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_",))
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_loaded_with_no_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_player_loaded(py, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_player_loaded_when_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::attach(|py| {
                let event_dispatcher = EventDispatcherManager::default();
                EVENT_DISPATCHERS.store(Some(
                    Py::new(py, event_dispatcher)
                        .expect("could not create event dispatcher manager in python")
                        .into(),
                ));

                let result = handle_player_loaded(py, 42);
                assert!(
                    result
                        .bind(py)
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true())
                );
            });
        });
    }
}

fn try_handle_player_disconnect(
    py: Python<'_>,
    client_id: i32,
    reason: Option<String>,
) -> PyResult<Py<PyAny>> {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "player_disconnect"))
                .ok()
        })
        .map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to player disconnect dispatcher",
                ))
            },
            |player_disconnect_dispatcher| {
                let player = Player::py_new(client_id, None)?;

                PlayerDisconnectDispatcherMethods::dispatch(
                    player_disconnect_dispatcher.downcast()?,
                    &Bound::new(py, player)?,
                    &reason.into_bound_py_any(py)?,
                )
                .map(|value| value.unbind())
            },
        )
}

/// This will be called whenever a player disconnects.
#[pyfunction]
#[pyo3(signature = (client_id, reason=None), text_signature = "(client_id, reason=None)")]
pub(crate) fn handle_player_disconnect(
    py: Python<'_>,
    client_id: i32,
    reason: Option<String>,
) -> Py<PyAny> {
    try_handle_player_disconnect(py, client_id, reason).unwrap_or_else(|e| {
        log_exception(py, &e);
        PyBool::new(py, true).to_owned().into_any().unbind()
    })
}

#[cfg(test)]
mod handle_player_disconnect_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::*;

    use super::{handle_player_disconnect, try_handle_player_disconnect};
    use crate::{
        ffi::{
            c::prelude::{
                CVar, CVarBuilder, MockClient, MockGameEntityBuilder, clientState_t, cvar_t,
                privileges_t, team_t,
            },
            python::{
                EVENT_DISPATCHERS,
                commands::CommandPriorities,
                events::{
                    EventDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                    EventDispatcherMethods, PlayerDisconnectDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_disconnect_forwards_to_dispatcher(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<PlayerDisconnectDispatcher>())
                                .expect("could not add player_disconnect dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "player_disconnect"))
                                .and_then(|player_disconnect_dispatcher| {
                                    player_disconnect_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to player_disconnect dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_player_disconnect(py, 42, None);
                            assert!(result.as_ref().is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| bool_value.is_true())
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_", "_"))
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_disconnect_with_no_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_player_disconnect(py, 42, Some("disconnected".into()));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_player_disconnect_when_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::attach(|py| {
                let event_dispatcher = EventDispatcherManager::default();
                EVENT_DISPATCHERS.store(Some(
                    Py::new(py, event_dispatcher)
                        .expect("could not create event dispatcher manager in python")
                        .into(),
                ));

                let result = handle_player_disconnect(py, 42, None);
                assert!(
                    result
                        .bind(py)
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true())
                );
            });
        });
    }
}

fn try_handle_player_spawn(py: Python<'_>, client_id: i32) -> PyResult<Py<PyAny>> {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "player_spawn"))
                .ok()
        })
        .map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to player spawn dispatcher",
                ))
            },
            |player_spawn_dispatcher| {
                let player = Player::py_new(client_id, None)?;

                PlayerSpawnDispatcherMethods::dispatch(
                    player_spawn_dispatcher.downcast()?,
                    &Bound::new(py, player)?,
                )
                .map(|value| value.unbind())
            },
        )
}

/// Called when a player spawns. Note that a spectator going in free spectate mode
/// makes the client spawn, so you'll want to check for that if you only want "actual"
/// spawns.
#[pyfunction]
pub(crate) fn handle_player_spawn(py: Python<'_>, client_id: i32) -> Py<PyAny> {
    try_handle_player_spawn(py, client_id).unwrap_or_else(|e| {
        log_exception(py, &e);
        PyBool::new(py, true).to_owned().into_any().unbind()
    })
}

#[cfg(test)]
mod handle_player_spawn_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::*;

    use super::{handle_player_spawn, try_handle_player_spawn};
    use crate::{
        ffi::{
            c::prelude::{
                CVar, CVarBuilder, MockClient, MockGameEntityBuilder, clientState_t, cvar_t,
                privileges_t, team_t,
            },
            python::{
                EVENT_DISPATCHERS,
                commands::CommandPriorities,
                events::{
                    EventDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                    EventDispatcherMethods, PlayerSpawnDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_spawn_forwards_to_dispatcher(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<PlayerSpawnDispatcher>())
                                .expect("could not add player_spawn dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "player_spawn"))
                                .and_then(|player_spawn_dispatcher| {
                                    player_spawn_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to player_spawn dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_player_spawn(py, 42);
                            assert!(result.as_ref().is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| bool_value.is_true())
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_",))
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_spawn_with_no_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_player_spawn(py, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_player_spawn_when_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::attach(|py| {
                let event_dispatcher = EventDispatcherManager::default();
                EVENT_DISPATCHERS.store(Some(
                    Py::new(py, event_dispatcher)
                        .expect("could not create event dispatcher manager in python")
                        .into(),
                ));

                let result = handle_player_spawn(py, 42);
                assert!(
                    result
                        .bind(py)
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true())
                );
            });
        });
    }
}

fn try_handle_kamikaze_use(py: Python<'_>, client_id: i32) -> PyResult<Py<PyAny>> {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "kamikaze_use"))
                .ok()
        })
        .map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to kamikaze use dispatcher",
                ))
            },
            |kamikaze_use_dispatcher| {
                let player = Player::py_new(client_id, None)?;

                KamikazeUseDispatcherMethods::dispatch(
                    kamikaze_use_dispatcher.downcast()?,
                    &Bound::new(py, player)?,
                )
                .map(|value| value.unbind())
            },
        )
}

/// This will be called whenever player uses kamikaze item.
#[pyfunction]
pub(crate) fn handle_kamikaze_use(py: Python<'_>, client_id: i32) -> Py<PyAny> {
    try_handle_kamikaze_use(py, client_id).unwrap_or_else(|e| {
        log_exception(py, &e);
        PyBool::new(py, true).to_owned().into_any().unbind()
    })
}

#[cfg(test)]
mod handle_kamikaze_use_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::*;

    use super::{handle_kamikaze_use, try_handle_kamikaze_use};
    use crate::{
        ffi::{
            c::prelude::{
                CVar, CVarBuilder, MockClient, MockGameEntityBuilder, clientState_t, cvar_t,
                privileges_t, team_t,
            },
            python::{
                EVENT_DISPATCHERS,
                commands::CommandPriorities,
                events::{
                    EventDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                    EventDispatcherMethods, KamikazeUseDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_kamikaze_use_forwards_to_dispatcher(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<KamikazeUseDispatcher>())
                                .expect("could not add kamikaze_use dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "kamikaze_use"))
                                .and_then(|kamikaze_use_dispatcher| {
                                    kamikaze_use_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to kamikaze_use dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_kamikaze_use(py, 42);
                            assert!(result.as_ref().is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| bool_value.is_true())
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_",))
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_kamikaze_use_with_no_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_kamikaze_use(py, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_kamikaze_use_when_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::attach(|py| {
                let event_dispatcher = EventDispatcherManager::default();
                EVENT_DISPATCHERS.store(Some(
                    Py::new(py, event_dispatcher)
                        .expect("could not create event dispatcher manager in python")
                        .into(),
                ));

                let result = handle_kamikaze_use(py, 42);
                assert!(
                    result
                        .bind(py)
                        .downcast::<PyBool>()
                        .is_ok_and(|bool_value| bool_value.is_true())
                );
            });
        });
    }
}

fn try_handle_kamikaze_explode(
    py: Python<'_>,
    client_id: i32,
    is_used_on_demand: bool,
) -> PyResult<Py<PyAny>> {
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
        cold_path();
        return Err(PyEnvironmentError::new_err(
            "could not get access to kamikaze explode dispatcher",
        ));
    };

    KamikazeExplodeDispatcherMethods::dispatch(
        kamikaze_explode_dispatcher.downcast()?,
        &Bound::new(py, player)?,
        is_used_on_demand,
    )
    .map(|value| value.unbind())
}

/// This will be called whenever kamikaze explodes.
#[pyfunction]
pub(crate) fn handle_kamikaze_explode(
    py: Python<'_>,
    client_id: i32,
    is_used_on_demand: bool,
) -> Py<PyAny> {
    try_handle_kamikaze_explode(py, client_id, is_used_on_demand).unwrap_or_else(|e| {
        log_exception(py, &e);
        PyBool::new(py, true).to_owned().into_any().unbind()
    })
}

#[cfg(test)]
mod handle_kamikaze_explode_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::*;

    use super::{handle_kamikaze_explode, try_handle_kamikaze_explode};
    use crate::{
        ffi::{
            c::prelude::{
                CVar, CVarBuilder, MockClient, MockGameEntityBuilder, clientState_t, cvar_t,
                privileges_t, team_t,
            },
            python::{
                EVENT_DISPATCHERS,
                commands::CommandPriorities,
                events::{
                    EventDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                    EventDispatcherMethods, KamikazeExplodeDispatcher,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_kamikaze_explode_forwards_to_dispatcher(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<KamikazeExplodeDispatcher>())
                                .expect("could not add kamikaze_explode dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "kamikaze_explode"))
                                .and_then(|kamikaze_explode_dispatcher| {
                                    kamikaze_explode_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to kamikaze_explode dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_kamikaze_explode(py, 42, false);
                            assert!(result.as_ref().is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| bool_value.is_true())
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_", false))
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_kamikaze_explode_used_on_demand_forwards_to_dispatcher(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<KamikazeExplodeDispatcher>())
                                .expect("could not add kamikaze_explode dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "kamikaze_explode"))
                                .and_then(|kamikaze_explode_dispatcher| {
                                    kamikaze_explode_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to kamikaze_explode dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_kamikaze_explode(py, 42, true);
                            assert!(result.as_ref().is_ok_and(|value| {
                                value
                                    .bind(py)
                                    .downcast::<PyBool>()
                                    .is_ok_and(|bool_value| bool_value.is_true())
                            }));
                            assert!(
                                capturing_hook
                                    .call_method1(intern!(py, "assert_called_with"), ("_", true))
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_kamikaze_explode_with_no_dispatcher(_pyshinqlx_setup: ()) {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                Python::attach(|py| {
                    let event_dispatcher = EventDispatcherManager::default();
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    let result = try_handle_kamikaze_explode(py, 42, true);
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_kamikaze_explode_when_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
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

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default().run(|| {
                    Python::attach(|py| {
                        let event_dispatcher = EventDispatcherManager::default();
                        EVENT_DISPATCHERS.store(Some(
                            Py::new(py, event_dispatcher)
                                .expect("could not create event dispatcher manager in python")
                                .into(),
                        ));

                        let result = handle_kamikaze_explode(py, 42, false);
                        assert!(
                            result
                                .bind(py)
                                .downcast::<PyBool>()
                                .is_ok_and(|bool_value| bool_value.is_true())
                        );
                    });
                });
            });
    }
}

fn try_handle_damage(
    py: Python<'_>,
    target_id: i32,
    attacker_id: Option<i32>,
    damage: i32,
    dflags: i32,
    means_of_death: i32,
) -> PyResult<Option<bool>> {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "damage"))
                .ok()
        })
        .map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to damage dispatcher",
                ))
            },
            |damage_dispatcher| {
                let target_player = if (0..MAX_CLIENTS as i32).contains(&target_id) {
                    Bound::new(py, Player::py_new(target_id, None)?)?.into_any()
                } else {
                    PyInt::new(py, target_id).into_any()
                };

                let attacker_player = attacker_id.and_then(|attacker_id| {
                    if (0..MAX_CLIENTS as i32).contains(&attacker_id) {
                        Player::py_new(attacker_id, None)
                            .and_then(|player| {
                                Bound::new(py, player).map(|py_player| py_player.into_any())
                            })
                            .ok()
                    } else {
                        attacker_id.into_bound_py_any(py).ok()
                    }
                });

                DamageDispatcherMethods::dispatch(
                    damage_dispatcher.downcast()?,
                    &target_player,
                    &attacker_player.unwrap_or(py.None().bind(py).to_owned()),
                    damage,
                    dflags,
                    means_of_death,
                )
                .map(|_| None)
            },
        )
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

#[cfg(test)]
mod handle_damage_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*};
    use rstest::*;

    use super::{handle_damage, try_handle_damage};
    use crate::{
        ffi::{
            c::prelude::{
                CVar, CVarBuilder, DAMAGE_NO_ARMOR, DAMAGE_NO_PROTECTION, DAMAGE_RADIUS,
                MockClient, MockGameEntityBuilder, clientState_t, cvar_t,
                meansOfDeath_t::{MOD_ROCKET, MOD_ROCKET_SPLASH, MOD_TRIGGER_HURT},
                privileges_t, team_t,
            },
            python::{
                EVENT_DISPATCHERS,
                commands::CommandPriorities,
                events::{
                    DamageDispatcher, EventDispatcher, EventDispatcherManager,
                    EventDispatcherManagerMethods, EventDispatcherMethods,
                },
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_damage_forwards_to_dispatcher(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<DamageDispatcher>())
                                .expect("could not add damage dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "damage"))
                                .and_then(|damage_dispatcher| {
                                    damage_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to damage dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_damage(
                                py,
                                42,
                                None,
                                42,
                                DAMAGE_NO_PROTECTION as i32,
                                MOD_ROCKET as i32,
                            );
                            assert!(result.as_ref().is_ok_and(|value| value.is_none()));
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        (
                                            "_",
                                            py.None(),
                                            42,
                                            DAMAGE_NO_PROTECTION,
                                            MOD_ROCKET as i32
                                        )
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_damage_forwards_to_dispatcher_with_entity_target(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<DamageDispatcher>())
                                .expect("could not add damage dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "damage"))
                                .and_then(|damage_dispatcher| {
                                    damage_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to damage dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_damage(
                                py,
                                420,
                                Some(42),
                                42,
                                DAMAGE_NO_PROTECTION as i32,
                                MOD_ROCKET as i32,
                            );
                            assert!(result.as_ref().is_ok_and(|value| value.is_none()));
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        (420, "_", 42, DAMAGE_NO_PROTECTION, MOD_ROCKET as i32)
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_damage_forwards_to_dispatcher_with_entity_attacking(_pyshinqlx_setup: ()) {
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

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(42), || {
                MockEngineBuilder::default()
                    .with_find_cvar(
                        |cmd| cmd == "zmq_stats_enable",
                        move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                        1..,
                    )
                    .run(|| {
                        Python::attach(|py| {
                            let event_dispatcher =
                                Bound::new(py, EventDispatcherManager::default())
                                    .expect("this should not happen");
                            event_dispatcher
                                .add_dispatcher(&py.get_type::<DamageDispatcher>())
                                .expect("could not add damage dispatcher");
                            let capturing_hook = capturing_hook(py);
                            event_dispatcher
                                .get_item(intern!(py, "damage"))
                                .and_then(|damage_dispatcher| {
                                    damage_dispatcher
                                        .downcast::<EventDispatcher>()
                                        .expect("this should not happen")
                                        .add_hook(
                                            "asdf",
                                            &capturing_hook
                                                .getattr(intern!(py, "hook"))
                                                .expect("could not get capturing hook"),
                                            CommandPriorities::PRI_NORMAL as i32,
                                        )
                                })
                                .expect("could not add hook to damage dispatcher");
                            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                            let result = try_handle_damage(
                                py,
                                42,
                                Some(420),
                                42,
                                DAMAGE_NO_PROTECTION as i32,
                                MOD_ROCKET as i32,
                            );
                            assert!(result.as_ref().is_ok_and(|value| value.is_none()));
                            assert!(
                                capturing_hook
                                    .call_method1(
                                        intern!(py, "assert_called_with"),
                                        ("_", 420, 42, DAMAGE_NO_PROTECTION, MOD_ROCKET as i32)
                                    )
                                    .is_ok()
                            );
                        });
                    });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_daamage_with_no_dispatcher(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_damage(
                py,
                42,
                Some(420),
                21,
                DAMAGE_NO_ARMOR as i32,
                MOD_TRIGGER_HURT as i32,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_damage_when_dispatcher_throws_exception(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::attach(|py| {
                let event_dispatcher = EventDispatcherManager::default();
                EVENT_DISPATCHERS.store(Some(
                    Py::new(py, event_dispatcher)
                        .expect("could not create event dispatcher manager in python")
                        .into(),
                ));

                let result = handle_damage(
                    py,
                    42,
                    None,
                    100,
                    DAMAGE_RADIUS as i32,
                    MOD_ROCKET_SPLASH as i32,
                );
                assert!(result.is_some_and(|value| value));
            });
        });
    }
}

static PRINT_REDIRECTION: LazyLock<ArcSwapOption<Py<PrintRedirector>>> =
    LazyLock::new(ArcSwapOption::empty);

fn try_handle_console_print(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
    pyshinqlx_get_logger(py, None).and_then(|logger| {
        let debug_level = py
            .import(intern!(py, "logging"))
            .and_then(|logging_module| logging_module.getattr(intern!(py, "DEBUG")))?;
        logger
            .call_method(
                intern!(py, "makeRecord"),
                (
                    intern!(py, "shinqlx"),
                    debug_level,
                    intern!(py, ""),
                    -1,
                    text.trim_end_matches('\n'),
                    py.None(),
                    py.None(),
                ),
                Some(
                    &[(intern!(py, "func"), intern!(py, "handle_console_print"))]
                        .into_py_dict(py)?,
                ),
            )
            .and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
    })?;

    let result = EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "console_print"))
                .ok()
        })
        .map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to console print dispatcher",
                ))
            },
            |console_print_dispatcher| {
                ConsolePrintDispatcherMethods::dispatch(console_print_dispatcher.downcast()?, text)
            },
        )?;
    if result
        .downcast::<PyBool>()
        .is_ok_and(|value| !value.is_true())
    {
        return Ok(PyBool::new(py, false).to_owned().into_any().unbind());
    }

    PRINT_REDIRECTION
        .load()
        .as_ref()
        .tap_some(|print_redirector| {
            print_redirector.bind(py).append(text);
        });

    let returned = result.extract::<String>().unwrap_or(text.to_string());
    Ok(PyString::new(py, &returned).into_any().unbind())
}

/// Called whenever the server prints something to the console and when rcon is used.
#[pyfunction]
pub(crate) fn handle_console_print(py: Python<'_>, text: &str) -> Py<PyAny> {
    if text.is_empty() {
        return py.None();
    }

    try_handle_console_print(py, text).unwrap_or_else(|e| {
        log_exception(py, &e);
        PyBool::new(py, true).to_owned().into_any().unbind()
    })
}

#[cfg(test)]
mod handle_console_print_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, intern, prelude::*, types::PyBool};
    use rstest::*;
    use tap::TapOptional;

    use super::{
        PRINT_REDIRECTION, PrintRedirector, PrintRedirectorMethods, handle_console_print,
        try_handle_console_print,
    };
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{
                EVENT_DISPATCHERS,
                PythonReturnCodes::RET_STOP_EVENT,
                commands::CommandPriorities,
                events::{
                    ConsolePrintDispatcher, EventDispatcher, EventDispatcherManager,
                    EventDispatcherManagerMethods, EventDispatcherMethods,
                },
                prelude::ConsoleChannel,
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::*,
            },
        },
        hooks::mock_hooks::shinqlx_com_printf_context,
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_console_print_forwards_to_python(_pyshinqlx_setup: ()) {
        PRINT_REDIRECTION.store(None);

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ConsolePrintDispatcher>())
                        .expect("could not add console_print dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "console_print"))
                        .and_then(|console_print_dispatcher| {
                            console_print_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to console_print dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_console_print(py, "asdf");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>(py)
                            .is_ok_and(|str_value| str_value == "asdf")
                    }));
                    assert!(
                        capturing_hook
                            .call_method1(intern!(py, "assert_called_with"), ("asdf",))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_console_print_when_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        PRINT_REDIRECTION.store(None);

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ConsolePrintDispatcher>())
                        .expect("could not add console_print dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "console_print"))
                        .and_then(|console_print_dispatcher| {
                            console_print_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &python_function_returning(py, &(RET_STOP_EVENT as i32)),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to console_print dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_console_print(py, "asdf");
                    assert!(result.is_ok_and(|value| {
                        value
                            .bind(py)
                            .downcast::<PyBool>()
                            .is_ok_and(|bool_value| !bool_value.is_true())
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_console_print_when_dispatcher_returns_other_string(_pyshinqlx_setup: ()) {
        PRINT_REDIRECTION.store(None);

        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<ConsolePrintDispatcher>())
                        .expect("could not add console_print dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "console_print"))
                        .and_then(|console_print_dispatcher| {
                            console_print_dispatcher
                                .downcast::<EventDispatcher>()
                                .expect("this should not happen")
                                .add_hook(
                                    "asdf",
                                    &python_function_returning(py, &"quit"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                )
                        })
                        .expect("could not add hook to console_print dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_console_print(py, "asdf");
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<String>(py)
                            .is_ok_and(|str_value| str_value == "quit")
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_console_print_when_print_is_redirected(_pyshinqlx_setup: ()) {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .with(predicate::eq("asdf\n"))
            .times(1);

        MockEngineBuilder::default().run(|| {
            Python::attach(|py| {
                let console_channel =
                    Py::new(py, ConsoleChannel::py_new(py, py.None().bind(py), None))
                        .expect("this should not happen");
                let print_redirector =
                    PrintRedirector::py_new(py, console_channel.into_bound(py).into_any())
                        .expect("this should not happen");
                PRINT_REDIRECTION.store(Some(
                    Bound::new(py, print_redirector)
                        .expect("this should not happen")
                        .unbind()
                        .into(),
                ));

                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<ConsolePrintDispatcher>())
                    .expect("could not add console_print dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let result = try_handle_console_print(py, "asdf");
                assert!(result.is_ok_and(|value| {
                    value
                        .extract::<String>(py)
                        .is_ok_and(|str_value| str_value == "asdf")
                }));

                PRINT_REDIRECTION
                    .load()
                    .as_ref()
                    .tap_some(|print_redirector| {
                        print_redirector
                            .bind(py)
                            .flush()
                            .expect("this should not happen")
                    });
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_console_print_with_no_dispatcher(_pyshinqlx_setup: ()) {
        PRINT_REDIRECTION.store(None);

        Python::attach(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = try_handle_console_print(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_console_print_with_empty_text(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = handle_console_print(py, "");
            assert!(result.is_none(py));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_console_print_with_no_dispatcher(_pyshinqlx_setup: ()) {
        PRINT_REDIRECTION.store(None);

        Python::attach(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = handle_console_print(py, "asdf");
            assert!(
                result
                    .bind(py)
                    .downcast::<PyBool>()
                    .is_ok_and(|bool_value| bool_value.is_true())
            );
        });
    }
}

#[pyclass(module = "_handlers", name = "PrintRedirector", frozen)]
pub(crate) struct PrintRedirector {
    channel: Py<PyAny>,
    print_buffer: parking_lot::RwLock<String>,
}

#[pymethods]
impl PrintRedirector {
    #[new]
    fn py_new(_py: Python<'_>, channel: Bound<'_, PyAny>) -> PyResult<PrintRedirector> {
        if !channel.is_instance_of::<AbstractChannel>() {
            cold_path();
            return Err(PyValueError::new_err(
                "The redirection channel must be an instance of shinqlx.AbstractChannel.",
            ));
        }

        Ok(PrintRedirector {
            channel: channel.unbind(),
            print_buffer: parking_lot::RwLock::new(String::with_capacity(MAX_MSG_LENGTH as usize)),
        })
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.channel)
    }

    #[pyo3(name = "__enter__")]
    fn context_manager_enter(slf: Bound<'_, Self>) -> Bound<'_, Self> {
        slf.context_manager_enter()
    }

    #[pyo3(name = "__exit__")]
    fn context_manager_exit(
        slf: &Bound<'_, Self>,
        exc_type: Py<PyAny>,
        exc_value: Py<PyAny>,
        exc_traceback: Py<PyAny>,
    ) -> PyResult<()> {
        slf.context_manager_exit(exc_type, exc_value, exc_traceback)
    }

    fn flush(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.flush()
    }

    fn append(slf: &Bound<'_, Self>, text: &str) {
        slf.append(text)
    }
}

pub(crate) trait PrintRedirectorMethods {
    fn context_manager_enter(self) -> Self;
    fn context_manager_exit(
        &self,
        exc_type: Py<PyAny>,
        exc_value: Py<PyAny>,
        exc_traceback: Py<PyAny>,
    ) -> PyResult<()>;
    fn flush(&self) -> PyResult<()>;
    fn append(&self, text: &str);
}

impl PrintRedirectorMethods for Bound<'_, PrintRedirector> {
    fn context_manager_enter(self) -> Self {
        PRINT_REDIRECTION.store(Some(Arc::new(self.to_owned().unbind())));
        self
    }

    #[allow(unused_variables)]
    fn context_manager_exit(
        &self,
        exc_type: Py<PyAny>,
        exc_value: Py<PyAny>,
        exc_traceback: Py<PyAny>,
    ) -> PyResult<()> {
        self.flush()?;
        PRINT_REDIRECTION.store(None);
        Ok(())
    }

    fn flush(&self) -> PyResult<()> {
        let print_buffer_contents = self.get().print_buffer.read().to_owned();
        self.get().print_buffer.write().clear();

        let _ = self
            .get()
            .channel
            .bind(self.py())
            .call_method1(intern!(self.py(), "reply"), (print_buffer_contents,))?;

        Ok(())
    }

    fn append(&self, text: &str) {
        self.get().print_buffer.write().push_str(text);
    }
}

#[cfg(test)]
mod print_redirector_tests {
    use pyo3::{exceptions::PyValueError, intern, prelude::*};
    use rstest::*;

    use super::PrintRedirector;
    use crate::ffi::python::{prelude::ChatChannel, pyshinqlx_setup_fixture::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_wrong_type(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let result = PrintRedirector::py_new(py, py.None().into_bound(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_subclass_of_abstract_channel(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let channel = Bound::new(
                py,
                ChatChannel::py_new(py, "chat", "print \"{}\n\"\n", py.None().bind(py), None),
            )
            .expect("this should not happen");
            let result = PrintRedirector::py_new(py, channel.into_any());
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn print_redirector_can_be_traversed_for_garbage_collector(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let channel = Bound::new(
                py,
                ChatChannel::py_new(py, "chat", "print \"{}\n\"\n", py.None().bind(py), None),
            )
            .expect("this should not happen");
            let print_redirector =
                PrintRedirector::py_new(py, channel.into_any()).expect("this should not happen");
            let _py_command = Py::new(py, print_redirector).expect("this should not happen");

            let result = py
                .import(intern!(py, "gc"))
                .and_then(|gc| gc.call_method0(intern!(py, "collect")));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn python_context_manager_interaction(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let sample_module = PyModule::from_code(
                py,
                cr#"
from shinqlx import AbstractChannel, redirect_print


captured_text = ""


class CapturingChannel(AbstractChannel):
    def __init__(self):
        super().__init__("capturing")

    def reply(self, msg):
        global captured_text
        captured_text += msg


def test_function():
    channel = CapturingChannel()
    with redirect_print(channel) as redirect:
        redirect.append("this should be printed\n")
        redirect.append("second line should be printed\n")

    assert(captured_text == "this should be printed\nsecond line should be printed\n")
                "#,
                c"print_redirector_sample.py",
                c"print_redirector_sample",
            )
            .expect("this should not happen");
            let result = sample_module.call_method0(intern!(py, "test_function"));
            assert!(result.as_ref().is_ok(), "{:?}", result.as_ref());
        });
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
pub(crate) fn redirect_print(
    py: Python<'_>,
    channel: Bound<'_, PyAny>,
) -> PyResult<PrintRedirector> {
    PrintRedirector::py_new(py, channel)
}

#[cfg(test)]
mod redirect_print_tests {
    use pyo3::prelude::*;
    use rstest::*;

    use super::redirect_print;
    use crate::ffi::python::{prelude::ChatChannel, pyshinqlx_setup_fixture::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn redirect_print_returns_print_redirector(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let channel = Bound::new(
                py,
                ChatChannel::py_new(py, "chat", "print \"{}\n\"\n", py.None().bind(py), None),
            )
            .expect("this should not happen");
            let result = redirect_print(py, channel.into_any());
            assert!(result.is_ok());
        });
    }
}

#[pyfunction]
pub(crate) fn register_handlers() {}

#[cfg(test)]
mod register_handlers_tests {
    use super::register_handlers;

    #[test]
    fn register_handlers_does_nothing() {
        register_handlers();
    }
}

#[cfg(test)]
#[mockall::automock]
#[allow(dead_code, clippy::module_inception)]
pub(crate) mod handlers {
    use pyo3::prelude::*;

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_rcon<'a>(_py: Python<'a>, _cmd: &str) -> Option<bool> {
        None
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_client_command<'a>(
        py: Python<'a>,
        _client_id: i32,
        _cmd: &str,
    ) -> Py<PyAny> {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_server_command<'a>(
        py: Python<'a>,
        _client_id: i32,
        _cmd: &str,
    ) -> Py<PyAny> {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_frame<'a>(_py: Python<'a>) -> Option<bool> {
        None
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_new_game<'a>(_py: Python<'a>, _is_restart: bool) -> Option<bool> {
        None
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_set_configstring<'a>(
        py: Python<'a>,
        _index: u32,
        _value: &str,
    ) -> Py<PyAny> {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_player_connect<'a>(
        py: Python<'a>,
        _client_id: i32,
        _is_bot: bool,
    ) -> Py<PyAny> {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_player_loaded<'a>(py: Python<'a>, _client_id: i32) -> Py<PyAny> {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_player_disconnect<'a>(
        py: Python<'a>,
        _client_id: i32,
        _reason: Option<String>,
    ) -> Py<PyAny> {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_player_spawn<'a>(py: Python<'a>, _client_id: i32) -> Py<PyAny> {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_kamikaze_use<'a>(py: Python<'a>, _client_id: i32) -> Py<PyAny> {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_kamikaze_explode<'a>(
        py: Python<'a>,
        _client_id: i32,
        _is_used_on_demand: bool,
    ) -> Py<PyAny> {
        py.None()
    }

    #[allow(clippy::needless_lifetimes)]
    #[cfg(not(tarpaulin_include))]
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
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn handle_console_print<'a>(py: Python<'a>, _text: &str) -> Py<PyAny> {
        py.None()
    }

    #[cfg(not(tarpaulin_include))]
    pub(crate) fn register_handlers() {}
}
