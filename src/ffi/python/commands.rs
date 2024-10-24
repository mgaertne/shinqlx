use super::prelude::*;
use super::{get_cvar, owner, pyshinqlx_get_logger, PythonReturnCodes, EVENT_DISPATCHERS};

use crate::quake_live_engine::FindCVar;
use crate::MAIN_ENGINE;

use pyo3::prelude::*;
use pyo3::{
    exceptions::{PyEnvironmentError, PyKeyError, PyValueError},
    intern,
    types::{IntoPyDict, PyBool, PyList, PyTuple},
    PyTraverseError, PyVisit,
};

/// A class representing an input-triggered command.
///
/// Has information about the command itself, its usage, when and who to call when
/// action should be taken.
#[pyclass(module = "_commands", name = "Command", get_all, frozen)]
#[derive(Debug)]
pub(crate) struct Command {
    plugin: Py<PyAny>,
    pub(crate) name: Vec<String>,
    pub(crate) handler: Py<PyAny>,
    permission: i32,
    channels: Vec<Py<PyAny>>,
    exclude_channels: Vec<Py<PyAny>>,
    client_cmd_pass: bool,
    client_cmd_perm: i32,
    prefix: bool,
    usage: String,
}

#[pymethods]
impl Command {
    #[new]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn py_new(
        py: Python<'_>,
        plugin: PyObject,
        name: PyObject,
        handler: PyObject,
        permission: i32,
        channels: PyObject,
        exclude_channels: PyObject,
        client_cmd_pass: bool,
        client_cmd_perm: i32,
        prefix: bool,
        usage: &str,
    ) -> PyResult<Self> {
        if !handler.bind(py).is_callable() {
            return Err(PyValueError::new_err(
                "'handler' must be a callable function.",
            ));
        }
        if !channels.is_none(py) && channels.getattr(py, "__iter__").is_err() {
            return Err(PyValueError::new_err(
                "'channels' must be a finite iterable or None.",
            ));
        }
        if !exclude_channels.is_none(py) && exclude_channels.getattr(py, "__iter__").is_err() {
            return Err(PyValueError::new_err(
                "'exclude_channels' must be a finite iterable or None.",
            ));
        }

        let mut names = vec![];
        name.extract::<Bound<'_, PyList>>(py)
            .ok()
            .iter()
            .for_each(|py_list| {
                py_list.iter().for_each(|py_alias| {
                    py_alias
                        .extract::<String>()
                        .ok()
                        .iter()
                        .for_each(|alias| names.push(alias.to_lowercase()));
                })
            });
        name.extract::<Bound<'_, PyTuple>>(py)
            .ok()
            .iter()
            .for_each(|py_tuple| {
                py_tuple.iter().for_each(|py_alias| {
                    py_alias
                        .extract::<String>()
                        .ok()
                        .iter()
                        .for_each(|alias| names.push(alias.to_lowercase()));
                })
            });
        name.extract::<String>(py)
            .ok()
            .iter()
            .for_each(|py_string| {
                names.push(py_string.clone());
            });

        let channels_vec = if channels.is_none(py) {
            vec![]
        } else {
            let mut collected = vec![];
            if let Ok(mut iter) = channels.bind(py).iter() {
                while let Some(Ok(value)) = iter.next() {
                    collected.push(value.into_py(py));
                }
            }
            collected
        };
        let exclude_channels_vec = if exclude_channels.is_none(py) {
            vec![]
        } else {
            let mut collected = vec![];
            if let Ok(mut iter) = exclude_channels.bind(py).iter() {
                while let Some(Ok(value)) = iter.next() {
                    collected.push(value.into_py(py));
                }
            }
            collected
        };

        Ok(Self {
            plugin,
            name: names,
            handler,
            permission,
            channels: channels_vec,
            exclude_channels: exclude_channels_vec,
            client_cmd_pass,
            client_cmd_perm,
            prefix,
            usage: usage.into(),
        })
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.plugin)?;
        visit.call(&self.handler)?;

        self.channels
            .iter()
            .map(|channel| visit.call(channel))
            .collect::<Result<Vec<_>, PyTraverseError>>()?;

        self.exclude_channels
            .iter()
            .map(|channel| visit.call(channel))
            .collect::<Result<Vec<_>, PyTraverseError>>()?;

        Ok(())
    }

    fn execute(
        &self,
        py: Python<'_>,
        player: Player,
        msg: &str,
        channel: PyObject,
    ) -> PyResult<PyObject> {
        let Some(command_name) = self.name.first() else {
            return Err(PyKeyError::new_err("command has no 'name'"));
        };

        let plugin = self.plugin.clone_ref(py);
        let plugin_name = plugin.getattr(py, intern!(py, "name"))?;
        pyshinqlx_get_logger(py, Some(plugin)).and_then(|logger| {
            let debug_level = py
                .import_bound(intern!(py, "logging"))
                .and_then(|logging_module| logging_module.getattr(intern!(py, "DEBUG")))?;
            logger
                .call_method(
                    intern!(py, "makeRecord"),
                    (
                        intern!(py, "shinqlx"),
                        debug_level,
                        intern!(py, ""),
                        -1,
                        intern!(py, "%s executed: %s @ %s -> %s"),
                        (player.steam_id, command_name, plugin_name, &channel),
                        py.None(),
                    ),
                    Some(&[(intern!(py, "func"), intern!(py, "execute"))].into_py_dict_bound(py)),
                )
                .and_then(|log_record| logger.call_method1(intern!(py, "handle"), (log_record,)))
        })?;

        let msg_vec: Vec<&str> = msg.split(' ').collect();
        self.handler
            .bind(py)
            .call1((player, msg_vec, &channel))
            .map(|return_value| return_value.unbind())
    }

    fn is_eligible_name(&self, py: Python<'_>, name: &str) -> bool {
        py.allow_threads(|| {
            let compared_name = if !self.prefix {
                Some(name)
            } else {
                MAIN_ENGINE.load().as_ref().and_then(|main_engine| {
                    main_engine
                        .find_cvar("qlx_commandPrefix")
                        .and_then(|cvar_prefix| name.strip_prefix(&cvar_prefix.get_string()))
                })
            };

            compared_name.is_some_and(|name| self.name.contains(&name.to_lowercase()))
        })
    }

    /// Check if a chat channel is one this command should execute in.
    ///
    /// Exclude takes precedence.
    fn is_eligible_channel(&self, py: Python<'_>, channel: PyObject) -> bool {
        if self.exclude_channels.iter().any(|exclude_channel| {
            exclude_channel
                .bind(py)
                .eq(channel.bind(py))
                .unwrap_or(false)
        }) {
            return false;
        }

        self.channels.is_empty()
            || self.channels.iter().any(|allowed_channel| {
                allowed_channel
                    .bind(py)
                    .eq(channel.bind(py))
                    .unwrap_or(false)
            })
    }

    /// Check if a player has the rights to execute the command.
    fn is_eligible_player(&self, py: Python<'_>, player: Player, is_client_cmd: bool) -> bool {
        if owner()
            .unwrap_or_default()
            .is_some_and(|owner_steam_id| player.steam_id == owner_steam_id)
        {
            return true;
        }

        let perm = if is_client_cmd {
            let client_cmd_permission_cvar = format!(
                "qlx_ccmd_perm_{}",
                self.name.first().unwrap_or(&"invalid".to_string())
            );
            get_cvar(&client_cmd_permission_cvar)
                .unwrap_or_default()
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(self.client_cmd_perm)
        } else {
            let cmd_permission_cvar = format!(
                "qlx_perm_{}",
                self.name.first().unwrap_or(&"invalid".to_string())
            );
            let configured_cmd_permission = get_cvar(&cmd_permission_cvar);
            configured_cmd_permission
                .unwrap_or_default()
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(self.permission)
        };

        if perm == 0 {
            return true;
        }

        self.plugin
            .getattr(py, intern!(py, "db"))
            .ok()
            .filter(|value| !value.is_none(py))
            .and_then(|plugin_db| {
                plugin_db
                    .call_method1(py, intern!(py, "get_permission"), (player,))
                    .ok()
            })
            .and_then(|player_perm_result| player_perm_result.extract::<i32>(py).ok())
            .is_some_and(|player_perm| player_perm >= perm)
    }
}

#[cfg(test)]
mod command_tests {
    use super::Command;

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder};
    use crate::ffi::python::{
        prelude::{ChatChannel, ConsoleChannel, TeamChatChannel},
        pyshinqlx_setup_fixture::pyshinqlx_setup,
        pyshinqlx_test_support::*,
    };
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use rstest::*;

    use pyo3::prelude::*;
    use pyo3::{
        exceptions::{PyKeyError, PyValueError},
        types::{PyList, PyTuple},
    };

    fn test_plugin_with_permission_db(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
        let test_plugin = test_plugin(py);
        PyModule::from_code_bound(
            py,
            r#"
class mocked_db:
    def get_permission(*args):
        return 2
            "#,
            "",
            "",
        )
        .and_then(|db_stub| db_stub.getattr("mocked_db"))
        .and_then(|db_class| db_class.call0())
        .and_then(|db_instance| test_plugin.setattr("db", db_instance))?;
        Ok(test_plugin)
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_uncallable_handler(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                py.None(),
                true.into_py(py),
                0,
                py.None(),
                py.None(),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_wrong_channel_type(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                py.None(),
                capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                0,
                chat_channel.into_any(),
                py.None(),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_wrong_exclude_channel_type(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                py.None(),
                capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                0,
                py.None(),
                chat_channel.into_any(),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_names_in_pylist(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let names_vec = vec![
                "name1".to_string(),
                "name2".to_string(),
                "name3".to_string(),
            ];
            let names_pylist = PyList::new_bound(py, &names_vec);

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                names_pylist.into_py(py),
                capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                0,
                py.None(),
                py.None(),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_ok_and(|cmd| cmd.name == names_vec));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_names_in_pytuple(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let names_vec = vec![
                "name1".to_string(),
                "name2".to_string(),
                "name3".to_string(),
            ];
            let names_pylist = PyTuple::new_bound(py, &names_vec);

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                names_pylist.into_py(py),
                capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                0,
                py.None(),
                py.None(),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_ok_and(|cmd| cmd.name == names_vec));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_single_name_as_string(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                "cmd_name".into_py(py),
                capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                0,
                py.None(),
                py.None(),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_ok_and(|cmd| cmd.name == vec!["cmd_name".to_string()]));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_multiple_whitelist_channels(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let console_channel =
                Py::new(py, ConsoleChannel::py_new()).expect("this should not happen");

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                "cmd_name".into_py(py),
                capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                0,
                vec![
                    chat_channel.clone_ref(py).into_py(py),
                    console_channel.clone_ref(py).into_py(py),
                ]
                .into_py(py),
                py.None(),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_ok_and(|cmd| cmd
                .channels
                .iter()
                .map(|channel| channel.clone_ref(py))
                .collect::<Vec<PyObject>>()
                .into_py(py)
                .bind(py)
                .eq(vec![
                    chat_channel.clone_ref(py).into_py(py),
                    console_channel.clone_ref(py).into_py(py)
                ]
                .into_py(py)
                .bind(py))
                .expect("this should not happen")));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_multiple_exclude_channels(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let console_channel =
                Py::new(py, ConsoleChannel::py_new()).expect("this should not happen");

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                "cmd_name".into_py(py),
                capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                0,
                py.None(),
                vec![
                    chat_channel.clone_ref(py).into_py(py),
                    console_channel.clone_ref(py).into_py(py),
                ]
                .into_py(py),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_ok_and(|cmd| cmd
                .exclude_channels
                .iter()
                .map(|channel| channel.clone_ref(py))
                .collect::<Vec<PyObject>>()
                .into_py(py)
                .bind(py)
                .eq(vec![
                    chat_channel.clone_ref(py).into_py(py),
                    console_channel.clone_ref(py).into_py(py)
                ]
                .into_py(py)
                .bind(py))
                .expect("this should not happen")));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn command_can_be_traversed_for_garbage_collector(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let console_channel =
                Py::new(py, ConsoleChannel::py_new()).expect("this should not happen");

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                "cmd_name".into_py(py),
                capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                0,
                vec![chat_channel.into_py(py)].into_py(py),
                vec![console_channel.into_py(py)].into_py(py),
                true,
                0,
                true,
                "",
            )
            .expect("this should not happen");
            let _py_command = Py::new(py, command).expect("this should not happen");

            let result = py
                .import_bound("gc")
                .and_then(|gc| gc.call_method0("collect"));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn execute_calls_handler(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let command = Command {
                plugin: test_plugin(py).unbind(),
                name: vec!["cmd".to_string()],
                handler: capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                permission: 0,
                channels: vec![],
                exclude_channels: vec![],
                client_cmd_pass: false,
                client_cmd_perm: 0,
                prefix: false,
                usage: "".to_string(),
            };

            let result = command.execute(py, default_test_player(), "cmd", py.None());
            assert!(result.is_ok());
            assert!(capturing_hook
                .call_method1(
                    "assert_called_with",
                    (default_test_player(), ["cmd"], py.None(),)
                )
                .is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn execute_when_name_is_empty(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let command = Command {
                plugin: test_plugin(py).unbind(),
                name: vec![],
                handler: capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                permission: 0,
                channels: vec![],
                exclude_channels: vec![],
                client_cmd_pass: false,
                client_cmd_perm: 0,
                prefix: false,
                usage: "".to_string(),
            };

            let result = command.execute(py, default_test_player(), "cmd", py.None());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_eligible_name_with_no_prefix(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let command = Command {
                plugin: test_plugin(py).unbind(),
                name: vec!["cmd_name".into()],
                handler: capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                permission: 0,
                channels: vec![],
                exclude_channels: vec![],
                client_cmd_pass: false,
                client_cmd_perm: 0,
                prefix: false,
                usage: "".to_string(),
            };

            assert!(command.is_eligible_name(py, "cmd_name"));
            assert!(!command.is_eligible_name(py, "unmatched_cmd_name"));
            assert!(!command.is_eligible_name(py, "!cmd_name"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligible_name_with_prefix(_pyshinqlx_setup: ()) {
        let cvar_string = c"!";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_commandPrefix",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let command = Command {
                        plugin: test_plugin(py).unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 0,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 0,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    assert!(!command.is_eligible_name(py, "cmd_name"));
                    assert!(!command.is_eligible_name(py, "!unmatched_cmd_name"));
                    assert!(command.is_eligible_name(py, "!cmd_name"));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_eligilble_channel_when_none_are_configured(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let command = Command {
                plugin: test_plugin(py).unbind(),
                name: vec!["cmd_name".into()],
                handler: capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                permission: 0,
                channels: vec![],
                exclude_channels: vec![],
                client_cmd_pass: false,
                client_cmd_perm: 0,
                prefix: true,
                usage: "".to_string(),
            };

            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            assert!(command.is_eligible_channel(py, chat_channel.into_py(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_eligilble_channel_with_configured_allowed_channels(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let console_channel =
                Py::new(py, ConsoleChannel::py_new()).expect("this should not happen");
            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let red_team_chat_channel = Py::new(
                py,
                TeamChatChannel::py_new("red", "red_team", "print \"{}\n\"\n"),
            )
            .expect("this should not happen");

            let capturing_hook = capturing_hook(py);
            let command = Command {
                plugin: test_plugin(py).unbind(),
                name: vec!["cmd_name".into()],
                handler: capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                permission: 0,
                channels: vec![
                    console_channel.clone_ref(py).into_py(py),
                    chat_channel.clone_ref(py).into_py(py),
                ],
                exclude_channels: vec![],
                client_cmd_pass: false,
                client_cmd_perm: 0,
                prefix: true,
                usage: "".to_string(),
            };

            assert!(command.is_eligible_channel(py, chat_channel.into_py(py)));
            assert!(command.is_eligible_channel(py, console_channel.into_py(py)));
            assert!(!command.is_eligible_channel(py, red_team_chat_channel.into_py(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_eligilble_channel_with_configured_allowed_and_exclude_channels(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let console_channel =
                Py::new(py, ConsoleChannel::py_new()).expect("this should not happen");
            let chat_channel = Py::new(
                py,
                TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
            )
            .expect("this should not happen");
            let red_team_chat_channel = Py::new(
                py,
                TeamChatChannel::py_new("red", "red_team", "print \"{}\n\"\n"),
            )
            .expect("this should not happen");
            let blue_team_chat_channel = Py::new(
                py,
                TeamChatChannel::py_new("blue", "blue_team", "print \"{}\n\"\n"),
            )
            .expect("this should not happen");

            let capturing_hook = capturing_hook(py);
            let command = Command {
                plugin: test_plugin(py).unbind(),
                name: vec!["cmd_name".into()],
                handler: capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                permission: 0,
                channels: vec![
                    console_channel.clone_ref(py).into_py(py),
                    chat_channel.clone_ref(py).into_py(py),
                ],
                exclude_channels: vec![
                    red_team_chat_channel.clone_ref(py).into_py(py),
                    blue_team_chat_channel.clone_ref(py).into_py(py),
                ],
                client_cmd_pass: false,
                client_cmd_perm: 0,
                prefix: true,
                usage: "".to_string(),
            };

            assert!(command.is_eligible_channel(py, chat_channel.into_py(py)));
            assert!(command.is_eligible_channel(py, console_channel.into_py(py)));
            assert!(!command.is_eligible_channel(py, red_team_chat_channel.into_py(py)));
            assert!(!command.is_eligible_channel(py, blue_team_chat_channel.into_py(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_regular_cmd_and_owner(_pyshinqlx_setup: ()) {
        let owner = c"1234567890";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let command = Command {
                        plugin: test_plugin(py).unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 5,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 5,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(command.is_eligible_player(py, player, false));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_client_cmd_and_owner(_pyshinqlx_setup: ()) {
        let owner = c"1234567890";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let command = Command {
                        plugin: test_plugin(py).unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 5,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 5,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(command.is_eligible_player(py, player, true));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_regular_cmd_with_configured_cvar(_pyshinqlx_setup: ()) {
        let cmd_perm = c"0";
        let mut raw_permission_cvar = CVarBuilder::default()
            .string(cmd_perm.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let owner = c"9876543210";
        let mut raw_owner_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_perm_cmd_name",
                move |_| CVar::try_from(raw_permission_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_owner_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let command = Command {
                        plugin: test_plugin(py).unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 5,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 5,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(command.is_eligible_player(py, player, false));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_regular_cmd_with_no_configured_cvar(_pyshinqlx_setup: ()) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let command = Command {
                        plugin: test_plugin(py).unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 0,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 0,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(command.is_eligible_player(py, player, false));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_client_cmd_with_configured_cvar(_pyshinqlx_setup: ()) {
        let cmd_perm = c"0";
        let owner = c"9876543210";
        let mut raw_permission_cvar = CVarBuilder::default()
            .string(cmd_perm.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let mut raw_owner_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_ccmd_perm_cmd_name",
                move |_| CVar::try_from(raw_permission_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_owner_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let command = Command {
                        plugin: test_plugin(py).unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 5,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 5,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(command.is_eligible_player(py, player, true));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_client_cmd_with_no_configured_cvar(_pyshinqlx_setup: ()) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let command = Command {
                        plugin: test_plugin(py).unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 0,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 0,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(command.is_eligible_player(py, player, true));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_regular_cmd_when_plugin_has_no_db(_pyshinqlx_setup: ()) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let test_plugin = test_plugin(py);
                    test_plugin
                        .setattr("db", py.None())
                        .expect("this should not happen");
                    let command = Command {
                        plugin: test_plugin.unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 5,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 5,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(!command.is_eligible_player(py, player, false));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_client_cmd_when_plugin_has_no_db(_pyshinqlx_setup: ()) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let test_plugin = test_plugin(py);
                    test_plugin
                        .setattr("db", py.None())
                        .expect("this should not happen");
                    let command = Command {
                        plugin: test_plugin.unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 5,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 5,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(!command.is_eligible_player(py, player, true));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_regular_cmd_when_plugin_db_returns_higher_permission(
        _pyshinqlx_setup: (),
    ) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let test_plugin =
                        test_plugin_with_permission_db(py).expect("this should not happend");
                    let command = Command {
                        plugin: test_plugin.unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 1,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 1,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(command.is_eligible_player(py, player, false));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_regular_cmd_when_plugin_db_returns_lower_permission(
        _pyshinqlx_setup: (),
    ) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let test_plugin =
                        test_plugin_with_permission_db(py).expect("this should not happend");
                    let command = Command {
                        plugin: test_plugin.unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 3,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 3,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(!command.is_eligible_player(py, player, false));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_client_cmd_when_plugin_db_returns_higher_permission(
        _pyshinqlx_setup: (),
    ) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let test_plugin =
                        test_plugin_with_permission_db(py).expect("this should not happend");
                    let command = Command {
                        plugin: test_plugin.unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 1,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 1,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(command.is_eligible_player(py, player, true));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_eligilble_player_for_client_cmd_when_plugin_db_returns_lower_permission(
        _pyshinqlx_setup: (),
    ) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let capturing_hook = capturing_hook(py);
                    let test_plugin =
                        test_plugin_with_permission_db(py).expect("this should not happend");
                    let command = Command {
                        plugin: test_plugin.unbind(),
                        name: vec!["cmd_name".into()],
                        handler: capturing_hook
                            .getattr("hook")
                            .expect("could not get capturing hook")
                            .unbind(),
                        permission: 3,
                        channels: vec![],
                        exclude_channels: vec![],
                        client_cmd_pass: false,
                        client_cmd_perm: 3,
                        prefix: true,
                        usage: "".to_string(),
                    };

                    let player = default_test_player();

                    assert!(!command.is_eligible_player(py, player, true));
                });
            });
    }
}

#[allow(non_camel_case_types)]
pub(crate) enum CommandPriorities {
    PRI_HIGHEST,
    PRI_HIGH,
    PRI_NORMAL,
    PRI_LOW,
    PRI_LOWEST,
}

/// Holds all commands and executes them whenever we get input and should execute.
#[pyclass(module = "_commands", name = "CommandInvoker")]
pub(crate) struct CommandInvoker {
    commands: parking_lot::RwLock<[Vec<Py<Command>>; 5]>,
}

#[pymethods]
impl CommandInvoker {
    #[new]
    pub(crate) fn py_new() -> Self {
        Self {
            commands: parking_lot::RwLock::new([vec![], vec![], vec![], vec![], vec![]]),
        }
    }

    #[getter(commands)]
    fn get_commands(&self, py: Python<'_>) -> Vec<Command> {
        let commands = self.commands.read();
        let mut returned = vec![];
        commands.iter().for_each(|cmds| {
            returned.extend(
                cmds.iter()
                    .map(|command| Command {
                        plugin: command.bind(py).borrow().plugin.clone_ref(py),
                        name: command.bind(py).borrow().name.clone(),
                        handler: command.bind(py).borrow().handler.clone_ref(py),
                        permission: command.bind(py).borrow().permission,
                        channels: command
                            .bind(py)
                            .borrow()
                            .channels
                            .iter()
                            .map(|channel| channel.clone_ref(py))
                            .collect(),
                        exclude_channels: command
                            .bind(py)
                            .borrow()
                            .exclude_channels
                            .iter()
                            .map(|channel| channel.clone_ref(py))
                            .collect(),
                        client_cmd_pass: command.bind(py).borrow().client_cmd_pass,
                        client_cmd_perm: command.bind(py).borrow().client_cmd_perm,
                        prefix: command.bind(py).borrow().prefix,
                        usage: command.bind(py).borrow().usage.clone(),
                    })
                    .collect::<Vec<Command>>(),
            );
        });
        returned
    }

    /// Check if a command is already registed.
    ///
    /// Commands are unique by (command.name, command.handler).
    fn is_registered(&self, py: Python<'_>, command: &Bound<'_, Command>) -> bool {
        let commands = self.commands.read();
        commands.iter().any(|prio_cmds| {
            prio_cmds.iter().any(|cmd| {
                cmd.bind(py).borrow().name.len() == command.borrow().name.len()
                    && cmd
                        .bind(py)
                        .borrow()
                        .name
                        .iter()
                        .all(|name| command.borrow().name.contains(name))
                    && cmd
                        .bind(py)
                        .borrow()
                        .handler
                        .bind(py)
                        .eq(command.borrow().handler.bind(py))
                        .unwrap_or(false)
            })
        })
    }

    pub(crate) fn add_command(
        &self,
        py: Python<'_>,
        command: Bound<'_, Command>,
        priority: usize,
    ) -> PyResult<()> {
        if self.is_registered(py, &command) {
            return Err(PyValueError::new_err(
                "Attempted to add an already registered command.",
            ));
        }
        let Some(mut commands) = self.commands.try_write() else {
            return PyModule::from_code_bound(
                py,
                r#"
import shinqlx


@shinqlx.next_frame
def add_command(cmd, priority):
    shinqlx.COMMANDS.add_command(cmd, priority)
        "#,
                "",
                "",
            )
            .and_then(|module| {
                module.call_method1(intern!(py, "add_command"), (&command, priority))
            })
            .map(|_| ());
        };

        commands[priority].push(command.unbind());
        Ok(())
    }

    pub(crate) fn remove_command(
        &self,
        py: Python<'_>,
        command: Bound<'_, Command>,
    ) -> PyResult<()> {
        if !self.is_registered(py, &command) {
            return Err(PyValueError::new_err(
                "Attempted to remove a command that was never added.",
            ));
        }

        let Some(mut commands) = self.commands.try_write() else {
            return PyModule::from_code_bound(
                py,
                r#"
import shinqlx


@shinqlx.next_frame
def remove_command(cmd):
    shinqlx.COMMANDS.remove_command(cmd)
        "#,
                "",
                "",
            )
            .and_then(|module| module.call_method1(intern!(py, "remove_command"), (command,)))
            .map(|_| ());
        };
        commands.iter_mut().for_each(|prio_commands| {
            prio_commands.retain(|cmd| {
                cmd.bind(py).borrow().name.len() != command.borrow().name.len()
                    || !command
                        .borrow()
                        .name
                        .iter()
                        .all(|name| cmd.bind(py).borrow().name.contains(name))
                    || cmd
                        .bind(py)
                        .borrow()
                        .handler
                        .bind(py)
                        .ne(command.borrow().handler.bind(py))
                        .unwrap_or(true)
            });
        });

        Ok(())
    }

    pub(crate) fn handle_input(
        &self,
        py: Python<'_>,
        player: &Player,
        msg: &str,
        channel: PyObject,
    ) -> PyResult<bool> {
        let Some(name) = msg
            .split_whitespace()
            .next()
            .map(|value| value.to_lowercase())
        else {
            return Ok(false);
        };
        let Ok(channel_name) = channel
            .bind(py)
            .str()
            .map(|channel_name_str| channel_name_str.to_string())
        else {
            return Ok(false);
        };
        let is_client_cmd = channel_name == "client_command";
        let mut pass_through = true;

        let Some(command_dispatcher) =
            EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers
                        .bind(py)
                        .get_item(intern!(py, "command"))
                        .ok()
                })
        else {
            return Err(PyEnvironmentError::new_err(
                "could not get access to command dispatcher",
            ));
        };

        let commands = self.commands.read();
        for priority_level in 0..commands.len() {
            for cmd in commands[priority_level].iter() {
                if !cmd.bind(py).borrow().is_eligible_name(py, &name) {
                    continue;
                }
                if !cmd
                    .bind(py)
                    .borrow()
                    .is_eligible_channel(py, channel.bind(py).into_py(py))
                {
                    continue;
                }
                if !cmd
                    .bind(py)
                    .borrow()
                    .is_eligible_player(py, player.clone(), is_client_cmd)
                {
                    continue;
                }

                if is_client_cmd {
                    pass_through = cmd.bind(py).borrow().client_cmd_pass;
                }

                let cmd_copy = Command {
                    plugin: cmd.bind(py).borrow().plugin.clone_ref(py),
                    name: cmd.bind(py).borrow().name.clone(),
                    handler: cmd.bind(py).borrow().handler.clone_ref(py),
                    permission: cmd.bind(py).borrow().permission,
                    channels: cmd
                        .bind(py)
                        .borrow()
                        .channels
                        .iter()
                        .map(|channel| channel.clone_ref(py))
                        .collect(),
                    exclude_channels: cmd
                        .bind(py)
                        .borrow()
                        .exclude_channels
                        .iter()
                        .map(|channel| channel.clone_ref(py))
                        .collect(),
                    client_cmd_pass: cmd.bind(py).borrow().client_cmd_pass,
                    client_cmd_perm: cmd.bind(py).borrow().client_cmd_perm,
                    prefix: cmd.bind(py).borrow().prefix,
                    usage: cmd.bind(py).borrow().usage.clone(),
                };

                let dispatcher_result = command_dispatcher
                    .call_method1(intern!(py, "dispatch"), (player.clone(), cmd_copy, msg))?;
                if dispatcher_result
                    .extract::<Bound<'_, PyBool>>()
                    .is_ok_and(|value| !value.is_true())
                {
                    return Ok(true);
                }

                let cmd_result = cmd.bind(py).borrow().execute(
                    py,
                    player.clone(),
                    msg,
                    channel.bind(py).into_py(py),
                )?;
                let cmd_result_return_code = cmd_result.extract::<PythonReturnCodes>(py);
                if cmd_result_return_code.as_ref().is_ok_and(|value| {
                    [PythonReturnCodes::RET_STOP, PythonReturnCodes::RET_STOP_ALL].contains(value)
                }) {
                    return Ok(false);
                }
                if cmd_result_return_code
                    .as_ref()
                    .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_EVENT)
                {
                    pass_through = false;
                } else if cmd_result_return_code
                    .as_ref()
                    .is_ok_and(|&value| value == PythonReturnCodes::RET_USAGE)
                {
                    if !cmd.bind(py).borrow().usage.is_empty() {
                        let usage_msg =
                            format!("^7Usage: ^6{} {}", name, cmd.bind(py).borrow().usage);
                        channel.call_method1(py, intern!(py, "reply"), (usage_msg,))?;
                    }
                } else if !cmd_result_return_code
                    .as_ref()
                    .is_ok_and(|&value| value == PythonReturnCodes::RET_NONE)
                {
                    pyshinqlx_get_logger(py, None).and_then(|logger| {
                        let cmd_handler_name = cmd
                            .bind(py)
                            .borrow()
                            .handler
                            .getattr(py, intern!(py, "__name__"))?;
                        let warning_level =
                            py.import_bound(intern!(py, "logging"))
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
                                    intern!(
                            py,
                            "Command '%s' with handler '%s' returned an unknown return value: %s"
                        ),
                                    (
                                        cmd.bind(py).borrow().name.clone(),
                                        cmd_handler_name,
                                        cmd_result,
                                    ),
                                    py.None(),
                                ),
                                Some(
                                    &[(intern!(py, "func"), intern!(py, "handle_input"))]
                                        .into_py_dict_bound(py),
                                ),
                            )
                            .and_then(|log_record| {
                                logger.call_method1(intern!(py, "handle"), (log_record,))
                            })
                    })?;
                }
            }
        }

        Ok(pass_through)
    }
}

#[cfg(test)]
mod command_invoker_tests {
    use super::{Command, CommandInvoker, CommandPriorities};

    use crate::ffi::python::channels::{ChatChannel, ClientCommandChannel, TeamChatChannel};
    use crate::ffi::python::events::{CommandDispatcher, EventDispatcherManager};
    use crate::ffi::python::pyshinqlx_setup_fixture::*;
    use crate::ffi::python::pyshinqlx_test_support::{
        capturing_hook, default_test_player, returning_false_hook, run_all_frame_tasks, test_plugin,
    };
    use crate::ffi::python::{PythonReturnCodes, EVENT_DISPATCHERS};

    use crate::prelude::*;

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder};
    use crate::hooks::mock_hooks::shinqlx_send_server_command_context;

    use core::borrow::BorrowMut;

    use mockall::predicate;
    use rstest::*;

    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    fn default_command(py: Python<'_>) -> Command {
        let capturing_hook = capturing_hook(py);
        Command {
            plugin: test_plugin(py).unbind(),
            name: vec!["cmd_name".into()],
            handler: capturing_hook
                .getattr("hook")
                .expect("could not get capturing hook")
                .unbind(),
            permission: 0,
            channels: vec![],
            exclude_channels: vec![],
            client_cmd_pass: false,
            client_cmd_perm: 0,
            prefix: true,
            usage: "".to_string(),
        }
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn emoty_command_invoker_has_empty_commands(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker = CommandInvoker::py_new();
            assert!(command_invoker.get_commands(py).is_empty());
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_registered_for_unregistered_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker = CommandInvoker::py_new();

            let py_command = Py::new(py, default_command(py)).expect("this should not happen");
            assert!(!command_invoker.is_registered(py, py_command.bind(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_registered_with_variations_of_registered_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker = CommandInvoker::py_new();
            let py_command = Py::new(py, default_command(py))
                .expect("this should not happen")
                .into_bound(py);
            command_invoker
                .add_command(py, py_command, CommandPriorities::PRI_NORMAL as usize)
                .expect("this should not happen");

            let unregistered_command1 = Command {
                handler: py.None(),
                ..default_command(py)
            };
            let py_unregistered_command1 =
                Py::new(py, unregistered_command1).expect("this should not happen");
            assert!(!command_invoker.is_registered(py, py_unregistered_command1.bind(py)));

            let unregistered_command2 = Command {
                name: vec!["cmd_name".into(), "cmd_alias1".into(), "cmd_alias2".into()],
                ..default_command(py)
            };
            let py_unregistered_command2 =
                Py::new(py, unregistered_command2).expect("this should not happen");
            assert!(!command_invoker.is_registered(py, py_unregistered_command2.bind(py)));

            let unregistered_command3 = Command {
                name: vec!["mismatched_cmd_name".into()],
                ..default_command(py)
            };
            let py_unregistered_command3 =
                Py::new(py, unregistered_command3).expect("this should not happen");
            assert!(!command_invoker.is_registered(py, py_unregistered_command3.bind(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_registered_for_registered_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker = CommandInvoker::py_new();
            let py_command = Py::new(py, default_command(py)).expect("this should not happen");
            command_invoker
                .add_command(
                    py,
                    py_command.clone_ref(py).into_bound(py),
                    CommandPriorities::PRI_NORMAL as usize,
                )
                .expect("this should not happen");

            assert!(command_invoker.is_registered(py, py_command.bind(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_command_adds_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker = CommandInvoker::py_new();
            let py_command = Py::new(py, default_command(py)).expect("this should not happen");

            let result = command_invoker.add_command(
                py,
                py_command.into_bound(py),
                CommandPriorities::PRI_NORMAL as usize,
            );
            assert!(result.is_ok());
            assert!(command_invoker
                .get_commands(py)
                .first()
                .is_some_and(|cmd| cmd.name[0] == "cmd_name"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_command_for_already_added_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker = CommandInvoker::py_new();
            let py_command = Py::new(py, default_command(py)).expect("this should not happen");

            command_invoker
                .add_command(
                    py,
                    py_command.clone_ref(py).into_bound(py),
                    CommandPriorities::PRI_NORMAL as usize,
                )
                .expect("this should not happen");

            let result = command_invoker.add_command(
                py,
                py_command.into_bound(py),
                CommandPriorities::PRI_NORMAL as usize,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_command_for_command_not_added(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker = CommandInvoker::py_new();
            let py_command = Py::new(py, default_command(py)).expect("this should not happen");

            let result = command_invoker.remove_command(py, py_command.into_bound(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_command_removes_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker = CommandInvoker::py_new();
            let py_command = Py::new(py, default_command(py)).expect("this should not happen");

            command_invoker
                .add_command(
                    py,
                    py_command.clone_ref(py).into_bound(py),
                    CommandPriorities::PRI_NORMAL as usize,
                )
                .expect("this should not happen");

            let result = command_invoker.remove_command(py, py_command.into_bound(py));
            assert!(result.is_ok());
            assert!(command_invoker.get_commands(py).is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn handle_input_for_empty_input(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = default_test_player();
            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let command_invoker = CommandInvoker::py_new();

            let result = command_invoker.handle_input(py, &player, " ", chat_channel.into_any());
            assert!(result.is_ok_and(|value| !value));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_with_no_event_dispatcher(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = default_test_player();
            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            EVENT_DISPATCHERS.store(None);

            let command_invoker = CommandInvoker::py_new();

            let result =
                command_invoker.handle_input(py, &player, "cmd_name", chat_channel.into_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_with_non_eligible_cmd_name(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = default_test_player();
            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                .expect("could not add command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let py_command = Py::new(py, default_command(py)).expect("this should not happen");

            let command_invoker = CommandInvoker::py_new();
            command_invoker
                .add_command(
                    py,
                    py_command.into_bound(py),
                    CommandPriorities::PRI_NORMAL as usize,
                )
                .expect("this should not happen");

            let result =
                command_invoker.handle_input(py, &player, "other_name", chat_channel.into_any());
            assert!(result.is_ok_and(|pass_through| pass_through));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_with_non_eligible_channel(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = default_test_player();
            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                .expect("could not add command dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let py_command = Py::new(py, default_command(py)).expect("this should not happen");

            let command_invoker = CommandInvoker::py_new();
            command_invoker
                .add_command(
                    py,
                    py_command.into_bound(py),
                    CommandPriorities::PRI_NORMAL as usize,
                )
                .expect("this should not happen");

            let result =
                command_invoker.handle_input(py, &player, "cmd_name", chat_channel.into_any());
            assert!(result.is_ok_and(|pass_through| pass_through));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_with_non_eligible_player(_pyshinqlx_setup: ()) {
        let prefix = c"!";
        let mut raw_cmdprefix_cvar = CVarBuilder::default()
            .string(prefix.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_commandPrefix",
                move |_| CVar::try_from(raw_cmdprefix_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let player = default_test_player();
                    let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                        .expect("this should not happen");

                    let event_dispatcher = EventDispatcherManager::default();
                    event_dispatcher
                        .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    let py_command =
                        Py::new(py, default_command(py)).expect("this should not happen");

                    let command_invoker = CommandInvoker::py_new();
                    command_invoker
                        .add_command(
                            py,
                            py_command.into_bound(py),
                            CommandPriorities::PRI_NORMAL as usize,
                        )
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        py,
                        &player,
                        "cmd_name",
                        chat_channel.into_any(),
                    );
                    assert!(result.is_ok_and(|pass_through| pass_through));
                });
            });
    }

    #[rstest]
    #[case(true)]
    #[case(false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_for_client_cmd_with_pass_through(
        #[case] pass_through: bool,
        _pyshinqlx_setup: (),
    ) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let player = default_test_player();
                    let client_command_channel = Py::new(py, ClientCommandChannel::py_new(&player))
                        .expect("this should not happen");

                    let event_dispatcher = EventDispatcherManager::default();
                    event_dispatcher
                        .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    let capturing_hook = capturing_hook(py);
                    let py_command = Py::new(
                        py,
                        Command {
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            client_cmd_pass: pass_through,
                            prefix: false,
                            ..default_command(py)
                        },
                    )
                    .expect("this should not happen");

                    let command_invoker = CommandInvoker::py_new();
                    command_invoker
                        .add_command(
                            py,
                            py_command.into_bound(py),
                            CommandPriorities::PRI_NORMAL as usize,
                        )
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        py,
                        &player,
                        "cmd_name",
                        client_command_channel.into_any(),
                    );
                    assert!(
                        result.is_ok_and(|actual_pass_through| actual_pass_through == pass_through)
                    );
                    assert!(capturing_hook
                        .call_method1("assert_called_with", ("_", ["cmd_name"], "_"))
                        .is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_when_event_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        // let owner = c"9876543210";
        // let mut raw_cvar = CVarBuilder::default()
        //     .string(owner.as_ptr().cast_mut())
        //     .build()
        //    .expect("this should not happen");

        MockEngineBuilder::default()
            // .with_find_cvar(
            //     predicate::eq("qlx_owner"),
            //     move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
            //     1..,
            // )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let player = default_test_player();
                    let client_command_channel = Py::new(py, ClientCommandChannel::py_new(&player))
                        .expect("this should not happen");

                    let event_dispatcher = EventDispatcherManager::default();
                    event_dispatcher
                        .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    event_dispatcher
                        .__getitem__(py, "command")
                        .and_then(|command_dispatcher| {
                            command_dispatcher.call_method1(
                                py,
                                "add_hook",
                                (
                                    "asdf",
                                    returning_false_hook(py),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to vote dispatcher");
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    let py_command =
                        Py::new(py, default_command(py)).expect("this should not happen");

                    let command_invoker = CommandInvoker::py_new();
                    command_invoker
                        .add_command(
                            py,
                            py_command.into_bound(py),
                            CommandPriorities::PRI_NORMAL as usize,
                        )
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        py,
                        &player,
                        "cmd_name",
                        client_command_channel.into_any(),
                    );
                    assert!(result.is_ok_and(|pass_through| pass_through));
                });
            });
    }

    #[rstest]
    #[case(PythonReturnCodes::RET_STOP, false)]
    #[case(PythonReturnCodes::RET_STOP_ALL, false)]
    #[case(PythonReturnCodes::RET_STOP_EVENT, false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_when_cmd_returns_various_values(
        #[case] return_code: PythonReturnCodes,
        #[case] expect_ok_value: bool,
        _pyshinqlx_setup: (),
    ) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let player = default_test_player();
                    let client_command_channel = Py::new(py, ClientCommandChannel::py_new(&player))
                        .expect("this should not happen");

                    let event_dispatcher = EventDispatcherManager::default();
                    event_dispatcher
                        .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    let module_definition = format!(
                        r#"
def cmd_handler(*args, **kwargs):
    return {}
            "#,
                        return_code as i32
                    );
                    let handler = PyModule::from_code_bound(py, &module_definition, "", "")
                        .and_then(|result| result.getattr("cmd_handler"))
                        .expect("this should not happen");
                    let command = Command {
                        handler: handler.unbind(),
                        prefix: false,
                        ..default_command(py)
                    };
                    let py_command = Py::new(py, command).expect("this should not happen");

                    let command_invoker = CommandInvoker::py_new();
                    command_invoker
                        .add_command(
                            py,
                            py_command.into_bound(py),
                            CommandPriorities::PRI_NORMAL as usize,
                        )
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        py,
                        &player,
                        "cmd_name",
                        client_command_channel.into_any(),
                    );
                    assert!(result.is_ok_and(|pass_through| pass_through == expect_ok_value));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_for_non_usage_cmd_when_cmd_returns_usage_return(_pyshinqlx_setup: ()) {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx.expect().times(0);

        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let player = default_test_player();
                    let chat_channel = Py::new(
                        py,
                        TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                    )
                    .expect("this should not happen");

                    let event_dispatcher = EventDispatcherManager::default();
                    event_dispatcher
                        .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    let module_definition = format!(
                        r#"
def cmd_handler(*args, **kwargs):
    return {}
            "#,
                        PythonReturnCodes::RET_USAGE as i32
                    );
                    let handler = PyModule::from_code_bound(py, &module_definition, "", "")
                        .and_then(|result| result.getattr("cmd_handler"))
                        .expect("this should not happen");
                    let command = Command {
                        handler: handler.unbind(),
                        prefix: false,
                        ..default_command(py)
                    };
                    let py_command = Py::new(py, command).expect("this should not happen");

                    let command_invoker = CommandInvoker::py_new();
                    command_invoker
                        .add_command(
                            py,
                            py_command.into_bound(py),
                            CommandPriorities::PRI_NORMAL as usize,
                        )
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        py,
                        &player,
                        "cmd_name",
                        chat_channel.into_any(),
                    );
                    assert!(result.is_ok_and(|pass_through| pass_through));

                    run_all_frame_tasks(py).expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_for_cmd_with_usage_when_cmd_returns_usage_return(_pyshinqlx_setup: ()) {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .with(
                predicate::always(),
                predicate::eq("print \"^7Usage: ^6cmd_name how to use me\n\"\n"),
            )
            .times(1);

        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let player = default_test_player();
                    let chat_channel = Py::new(
                        py,
                        TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                    )
                    .expect("this should not happen");

                    let event_dispatcher = EventDispatcherManager::default();
                    event_dispatcher
                        .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    let module_definition = format!(
                        r#"
def cmd_handler(*args, **kwargs):
    return {}
            "#,
                        PythonReturnCodes::RET_USAGE as i32
                    );
                    let handler = PyModule::from_code_bound(py, &module_definition, "", "")
                        .and_then(|result| result.getattr("cmd_handler"))
                        .expect("this should not happen");
                    let command = Command {
                        handler: handler.unbind(),
                        prefix: false,
                        usage: "how to use me".to_string(),
                        ..default_command(py)
                    };
                    let py_command = Py::new(py, command).expect("this should not happen");

                    let command_invoker = CommandInvoker::py_new();
                    command_invoker
                        .add_command(
                            py,
                            py_command.into_bound(py),
                            CommandPriorities::PRI_NORMAL as usize,
                        )
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        py,
                        &player,
                        "cmd_name",
                        chat_channel.into_any(),
                    );
                    assert!(result.is_ok_and(|pass_through| pass_through));

                    run_all_frame_tasks(py).expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_when_handler_returns_unrecognize_return_code(_pyshinqlx_setup: ()) {
        let owner = c"9876543210";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let player = default_test_player();
                    let chat_channel = Py::new(
                        py,
                        TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                    )
                    .expect("this should not happen");

                    let event_dispatcher = EventDispatcherManager::default();
                    event_dispatcher
                        .add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(
                        Py::new(py, event_dispatcher)
                            .expect("could not create event dispatcher manager in python")
                            .into(),
                    ));

                    let handler = PyModule::from_code_bound(
                        py,
                        r#"
def cmd_handler(*args, **kwargs):
    return dict()
            "#,
                        "",
                        "",
                    )
                    .and_then(|result| result.getattr("cmd_handler"))
                    .expect("this should not happen");
                    let command = Command {
                        handler: handler.unbind(),
                        prefix: false,
                        ..default_command(py)
                    };
                    let py_command = Py::new(py, command).expect("this should not happen");

                    let command_invoker = CommandInvoker::py_new();
                    command_invoker
                        .add_command(
                            py,
                            py_command.into_bound(py),
                            CommandPriorities::PRI_NORMAL as usize,
                        )
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        py,
                        &player,
                        "cmd_name",
                        chat_channel.into_any(),
                    );
                    assert!(result.is_ok_and(|pass_through| pass_through));
                });
            });
    }
}
