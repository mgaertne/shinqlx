use super::prelude::*;
use super::{owner, pyshinqlx_get_logger, PythonReturnCodes, EVENT_DISPATCHERS};

use crate::quake_live_engine::FindCVar;
use crate::MAIN_ENGINE;

use pyo3::prelude::*;
use pyo3::{
    exceptions::{PyEnvironmentError, PyKeyError, PyValueError},
    intern,
    types::{IntoPyDict, PyList, PyTuple},
    PyTraverseError, PyVisit,
};

/// A class representing an input-triggered command.
///
/// Has information about the command itself, its usage, when and who to call when
/// action should be taken.
#[pyclass(module = "_commands", name = "Command", get_all, frozen)]
#[derive(Clone, Debug)]
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
        name.bind(py)
            .extract::<&PyList>()
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
        name.bind(py)
            .extract::<&PyTuple>()
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

        for channel in &self.channels {
            visit.call(channel)?;
        }

        for channel in &self.exclude_channels {
            visit.call(channel)?;
        }

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
            let logging_module = py.import_bound(intern!(py, "logging"))?;
            let debug_level = logging_module.getattr(intern!(py, "DEBUG"))?;
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
                        .and_then(|cvar_prefix| {
                            name.strip_prefix(cvar_prefix.get_string().as_ref())
                        })
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
        let perm = if is_client_cmd {
            self.client_cmd_perm
        } else {
            let cmd_permission_cvar = format!(
                "qlx_perm_{}",
                self.name.first().unwrap_or(&"invalid".to_string())
            );
            let configured_cmd_permission = pyshinqlx_get_cvar(py, &cmd_permission_cvar);
            configured_cmd_permission
                .ok()
                .and_then(|opt_permission| {
                    opt_permission.and_then(|value| value.parse::<i32>().ok())
                })
                .unwrap_or(self.permission)
        };

        let client_cmd_perm = if is_client_cmd {
            let client_cmd_permission_cvar = format!(
                "qlx_ccmd_perm_{}",
                self.name.first().unwrap_or(&"invalid".to_string())
            );
            let configured_client_cmd_permission =
                pyshinqlx_get_cvar(py, &client_cmd_permission_cvar);
            configured_client_cmd_permission
                .ok()
                .and_then(|opt_permission| {
                    opt_permission.and_then(|value| value.parse::<i32>().ok())
                })
                .unwrap_or(self.permission)
        } else {
            self.permission
        };

        let owner_steam_id = owner(py).unwrap_or_default().unwrap_or_default();
        if player.steam_id == owner_steam_id {
            return true;
        }

        if !is_client_cmd && perm == 0 {
            return true;
        }

        if is_client_cmd && client_cmd_perm == 0 {
            return true;
        }

        let Ok(plugin_db) = self.plugin.getattr(py, intern!(py, "db")) else {
            return false;
        };
        if plugin_db.is_none(py) {
            return false;
        }

        let Ok(player_perm_result) =
            plugin_db.call_method1(py, intern!(py, "get_permission"), (player,))
        else {
            return false;
        };
        let Ok(player_perm) = player_perm_result.extract::<i32>(py) else {
            return false;
        };

        if is_client_cmd {
            return player_perm >= client_cmd_perm;
        }
        player_perm >= perm
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
    use crate::{
        prelude::{serial, MockQuakeEngine},
        MAIN_ENGINE,
    };

    use alloc::ffi::CString;
    use core::ffi::c_char;

    use mockall::predicate;
    use rstest::*;

    use pyo3::prelude::*;
    use pyo3::{
        exceptions::{PyKeyError, PyValueError},
        types::{PyList, PyTuple},
    };

    fn test_handler(py: Python<'_>) -> PyResult<PyObject> {
        PyModule::from_code_bound(
            py,
            r#"
def handler(*args, **kwargs):
    pass
        "#,
            "",
            "",
        )
        .and_then(|py_module| py_module.getattr("handler"))
        .map(|handler| handler.unbind())
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
            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                py.None(),
                test_handler(py).expect("this should not happen"),
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
            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                py.None(),
                test_handler(py).expect("this should not happen"),
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
                test_handler(py).expect("this should not happen"),
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
                test_handler(py).expect("this should not happen"),
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
            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                "cmd_name".into_py(py),
                test_handler(py).expect("this should not happen"),
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
            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let console_channel =
                Py::new(py, ConsoleChannel::py_new()).expect("this should not happen");

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                "cmd_name".into_py(py),
                test_handler(py).expect("this should not happen"),
                0,
                vec![
                    chat_channel.clone().into_py(py),
                    console_channel.clone().into_py(py),
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
                .clone()
                .into_py(py)
                .bind(py)
                .eq(vec![
                    chat_channel.clone().into_py(py),
                    console_channel.clone().into_py(py)
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
            let chat_channel = Py::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let console_channel =
                Py::new(py, ConsoleChannel::py_new()).expect("this should not happen");

            let command = Command::py_new(
                py,
                test_plugin(py).unbind(),
                "cmd_name".into_py(py),
                test_handler(py).expect("this should not happen"),
                0,
                py.None(),
                vec![
                    chat_channel.clone().into_py(py),
                    console_channel.clone().into_py(py),
                ]
                .into_py(py),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_ok_and(|cmd| cmd
                .exclude_channels
                .clone()
                .into_py(py)
                .bind(py)
                .eq(vec![
                    chat_channel.clone().into_py(py),
                    console_channel.clone().into_py(py)
                ]
                .into_py(py)
                .bind(py))
                .expect("this should not happen")));
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
        let mut mock_engine = MockQuakeEngine::new();
        let cvar_string = CString::new("!").expect("this should not happen");
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("qlx_commandPrefix"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            });
        MAIN_ENGINE.store(Some(mock_engine.into()));
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
    commands: parking_lot::RwLock<[Vec<Command>; 5]>,
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
    fn get_commands(&self) -> Vec<Command> {
        let commands = self.commands.read();
        let mut returned = vec![];
        commands.iter().for_each(|commands| {
            returned.extend(commands.clone());
        });
        returned
    }

    /// Check if a command is already registed.
    ///
    /// Commands are unique by (command.name, command.handler).
    fn is_registered(&self, py: Python<'_>, command: &Command) -> bool {
        let commands = self.commands.read();
        commands.iter().any(|prio_cmds| {
            prio_cmds.iter().any(|cmd| {
                cmd.name.len() == command.name.len()
                    && cmd.name.iter().all(|name| command.name.contains(name))
                    && cmd
                        .handler
                        .bind(py)
                        .eq(command.handler.bind(py))
                        .unwrap_or(false)
            })
        })
    }

    pub(crate) fn add_command(
        &self,
        py: Python<'_>,
        command: Command,
        priority: usize,
    ) -> PyResult<()> {
        if self.is_registered(py, &command) {
            return Err(PyValueError::new_err(
                "Attempted to add an already registered command.",
            ));
        }
        let Some(mut commands) = self.commands.try_write() else {
            let add_command_func = PyModule::from_code_bound(
                py,
                r#"
import shinqlx


@shinqlx.next_frame
def add_command(cmd, priority):
    shinqlx.COMMANDS.add_command(cmd, priority)
        "#,
                "",
                "",
            )?
            .getattr(intern!(py, "add_command"))?;

            add_command_func.call1((command, priority))?;
            return Ok(());
        };
        commands[priority].push(command);
        Ok(())
    }

    pub(crate) fn remove_command(&self, py: Python<'_>, command: Command) -> PyResult<()> {
        if !self.is_registered(py, &command) {
            return Err(PyValueError::new_err(
                "Attempted to remove a command that was never added.",
            ));
        }

        let Some(mut commands) = self.commands.try_write() else {
            let remove_command_func = PyModule::from_code_bound(
                py,
                r#"
import shinqlx


@shinqlx.next_frame
def remove_command(cmd):
    shinqlx.COMMANDS.remove_command(cmd)
        "#,
                "",
                "",
            )?
            .getattr(intern!(py, "remove_command"))?;

            remove_command_func.call1((command,))?;
            return Ok(());
        };
        commands.iter_mut().for_each(|prio_commands| {
            prio_commands.retain(|cmd| {
                cmd.name.len() != command.name.len()
                    || !command.name.iter().all(|name| cmd.name.contains(name))
                    || cmd
                        .handler
                        .bind(py)
                        .ne(command.handler.bind(py))
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
                if !cmd.is_eligible_name(py, &name) {
                    continue;
                }
                if !cmd.is_eligible_channel(py, channel.bind(py).into_py(py)) {
                    continue;
                }
                if !cmd.is_eligible_player(py, player.clone(), is_client_cmd) {
                    continue;
                }

                if is_client_cmd {
                    pass_through = cmd.client_cmd_pass;
                }

                let dispatcher_result = command_dispatcher
                    .call_method1(intern!(py, "dispatch"), (player.clone(), cmd.clone(), msg))?;
                if dispatcher_result
                    .extract::<bool>()
                    .is_ok_and(|value| !value)
                {
                    return Ok(true);
                }

                let cmd_result =
                    cmd.execute(py, player.clone(), msg, channel.bind(py).into_py(py))?;
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
                    && !cmd.usage.is_empty()
                {
                    let usage_msg = format!("^7Usage: ^6{} {}", name, cmd.usage);
                    channel.call_method1(py, intern!(py, "reply"), (usage_msg,))?;
                } else if cmd_result_return_code
                    .as_ref()
                    .is_ok_and(|&value| value != PythonReturnCodes::RET_NONE)
                {
                    let logger = pyshinqlx_get_logger(py, None)?;
                    let cmd_handler_name = cmd.handler.getattr(py, intern!(py, "__name__"))?;
                    let logging_module = py.import_bound(intern!(py, "logging"))?;
                    let warning_level = logging_module.getattr(intern!(py, "WARNING"))?;
                    let log_record = logger.call_method(
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
                            (cmd.name.clone(), cmd_handler_name, cmd_result),
                            py.None(),
                        ),
                        Some(
                            &[(intern!(py, "func"), intern!(py, "handle_input"))]
                                .into_py_dict_bound(py),
                        ),
                    )?;
                    logger.call_method1(intern!(py, "handle"), (log_record,))?;
                }
            }
        }

        Ok(pass_through)
    }
}
