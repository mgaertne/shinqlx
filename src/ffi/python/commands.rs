use super::prelude::*;
use super::{EVENT_DISPATCHERS, PythonReturnCodes, get_cvar, owner, pyshinqlx_get_logger};

use crate::MAIN_ENGINE;
use crate::quake_live_engine::FindCVar;

use pyo3::prelude::*;
use pyo3::{
    PyTraverseError, PyVisit,
    exceptions::{PyEnvironmentError, PyKeyError, PyValueError},
    intern,
    types::{IntoPyDict, PyBool, PyList, PyString, PyTuple},
};

/// A class representing an input-triggered command.
///
/// Has information about the command itself, its usage, when and who to call when
/// action should be taken.
#[pyclass(module = "_commands", name = "Command", frozen)]
#[derive(Debug)]
pub(crate) struct Command {
    #[pyo3(get)]
    pub(crate) plugin: Py<PyAny>,
    #[pyo3(get)]
    pub(crate) name: Vec<String>,
    #[pyo3(get)]
    pub(crate) handler: Py<PyAny>,
    #[pyo3(get)]
    pub(crate) permission: i32,
    pub(crate) channels: parking_lot::RwLock<Vec<Py<PyAny>>>,
    pub(crate) exclude_channels: parking_lot::RwLock<Vec<Py<PyAny>>>,
    #[pyo3(get)]
    pub(crate) client_cmd_pass: bool,
    #[pyo3(get)]
    pub(crate) client_cmd_perm: i32,
    #[pyo3(get)]
    pub(crate) prefix: bool,
    #[pyo3(get)]
    pub(crate) usage: String,
}

#[pymethods]
impl Command {
    #[new]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn py_new(
        plugin: &Bound<'_, PyAny>,
        name: &Bound<'_, PyAny>,
        handler: &Bound<'_, PyAny>,
        permission: i32,
        channels: &Bound<'_, PyAny>,
        exclude_channels: &Bound<'_, PyAny>,
        client_cmd_pass: bool,
        client_cmd_perm: i32,
        prefix: bool,
        usage: &str,
    ) -> PyResult<Self> {
        if !handler.is_callable() {
            return Err(PyValueError::new_err(
                "'handler' must be a callable function.",
            ));
        }
        if !channels.is_none() && channels.getattr("__iter__").is_err() {
            return Err(PyValueError::new_err(
                "'channels' must be a finite iterable or None.",
            ));
        }
        if !exclude_channels.is_none() && exclude_channels.getattr("__iter__").is_err() {
            return Err(PyValueError::new_err(
                "'exclude_channels' must be a finite iterable or None.",
            ));
        }

        let mut names = vec![];
        name.downcast::<PyList>().ok().iter().for_each(|py_list| {
            py_list.iter().for_each(|py_alias| {
                py_alias
                    .extract::<String>()
                    .ok()
                    .iter()
                    .for_each(|alias| names.push(alias.to_lowercase()));
            })
        });
        name.downcast::<PyTuple>().ok().iter().for_each(|py_tuple| {
            py_tuple.iter().for_each(|py_alias| {
                py_alias
                    .extract::<String>()
                    .ok()
                    .iter()
                    .for_each(|alias| names.push(alias.to_lowercase()));
            })
        });
        name.extract::<String>().ok().iter().for_each(|py_string| {
            names.push(py_string.clone());
        });

        let channels_vec = if channels.is_none() {
            vec![]
        } else {
            channels
                .try_iter()?
                .try_iter()?
                .filter_map(|iter_value| iter_value.ok().map(|value| value.unbind()))
                .collect::<Vec<PyObject>>()
        };
        let exclude_channels_vec = if exclude_channels.is_none() {
            vec![]
        } else {
            exclude_channels
                .try_iter()?
                .try_iter()?
                .filter_map(|iter_value| iter_value.ok().map(|value| value.unbind()))
                .collect::<Vec<PyObject>>()
        };

        Ok(Self {
            plugin: plugin.clone().unbind(),
            name: names,
            handler: handler.clone().unbind(),
            permission,
            channels: channels_vec.into(),
            exclude_channels: exclude_channels_vec.into(),
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
            .read()
            .iter()
            .map(|channel| visit.call(channel))
            .collect::<Result<Vec<_>, PyTraverseError>>()?;

        self.exclude_channels
            .read()
            .iter()
            .map(|channel| visit.call(channel))
            .collect::<Result<Vec<_>, PyTraverseError>>()?;

        Ok(())
    }

    fn __clear__(&self) {
        self.channels.write().clear();
        self.exclude_channels.write().clear();
    }

    #[getter(channels)]
    fn get_channels(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        self.channels
            .read()
            .iter()
            .map(|channel| channel.clone_ref(py))
            .collect()
    }

    #[getter(exclude_channels)]
    fn get_exclude_channels(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        self.exclude_channels
            .read()
            .iter()
            .map(|channel| channel.clone_ref(py))
            .collect()
    }

    fn execute<'py>(
        slf: &Bound<'py, Self>,
        player: &Bound<'py, Player>,
        msg: &str,
        channel: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.execute(player, msg, channel)
    }

    fn is_eligible_name(slf: &Bound<'_, Self>, name: &str) -> bool {
        slf.is_eligible_name(name)
    }

    /// Check if a chat channel is one this command should execute in.
    ///
    /// Exclude takes precedence.
    fn is_eligible_channel(slf: &Bound<'_, Self>, channel: &Bound<'_, PyAny>) -> bool {
        slf.is_eligible_channel(channel)
    }

    /// Check if a player has the rights to execute the command.
    fn is_eligible_player(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        is_client_cmd: bool,
    ) -> bool {
        slf.is_eligible_player(player, is_client_cmd)
    }
}

pub(crate) trait CommandMethods<'py> {
    fn execute(
        &self,
        player: &Bound<'py, Player>,
        msg: &str,
        channel: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn is_eligible_name(&self, name: &str) -> bool;
    fn is_eligible_channel(&self, channel: &Bound<'py, PyAny>) -> bool;
    fn is_eligible_player(&self, player: &Bound<'py, Player>, is_client_cmd: bool) -> bool;
}

impl<'py> CommandMethods<'py> for Bound<'py, Command> {
    fn execute(
        &self,
        player: &Bound<'py, Player>,
        msg: &str,
        channel: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let slf = self.borrow();
        let Some(command_name) = slf.name.first() else {
            return Err(PyKeyError::new_err("command has no 'name'"));
        };

        let plugin = slf.plugin.bind(self.py());
        let plugin_name = plugin.downcast::<Plugin>()?.get_name()?;
        pyshinqlx_get_logger(
            self.py(),
            Some(PyString::new(self.py(), &plugin_name).into_any()),
        )
        .and_then(|logger| {
            let debug_level = self
                .py()
                .import(intern!(self.py(), "logging"))
                .and_then(|logging_module| logging_module.getattr(intern!(self.py(), "DEBUG")))?;
            logger
                .call_method(
                    intern!(self.py(), "makeRecord"),
                    (
                        intern!(self.py(), "shinqlx"),
                        debug_level,
                        intern!(self.py(), ""),
                        -1,
                        intern!(self.py(), "%s executed: %s @ %s -> %s"),
                        (
                            player.borrow().steam_id,
                            command_name,
                            plugin_name,
                            &channel,
                        ),
                        self.py().None(),
                    ),
                    Some(
                        &[(intern!(self.py(), "func"), intern!(self.py(), "execute"))]
                            .into_py_dict(self.py())?,
                    ),
                )
                .and_then(|log_record| {
                    logger.call_method1(intern!(self.py(), "handle"), (log_record,))
                })
        })?;

        let msg_vec: Vec<&str> = msg.split(' ').collect();
        slf.handler
            .bind(self.py())
            .call1((player, msg_vec, &channel))
    }
    fn is_eligible_name(&self, name: &str) -> bool {
        let compared_name = if !self.borrow().prefix {
            Some(name)
        } else {
            MAIN_ENGINE.load().as_ref().and_then(|main_engine| {
                main_engine
                    .find_cvar("qlx_commandPrefix")
                    .and_then(|cvar_prefix| name.strip_prefix(&cvar_prefix.get_string()))
            })
        };

        compared_name.is_some_and(|name| self.borrow().name.contains(&name.to_lowercase()))
    }
    fn is_eligible_channel(&self, channel: &Bound<'py, PyAny>) -> bool {
        if self
            .borrow()
            .exclude_channels
            .read()
            .iter()
            .any(|exclude_channel| {
                exclude_channel
                    .bind(self.py())
                    .eq(channel.clone())
                    .unwrap_or(false)
            })
        {
            return false;
        }

        self.borrow().channels.read().is_empty()
            || self.borrow().channels.read().iter().any(|allowed_channel| {
                allowed_channel
                    .bind(self.py())
                    .eq(channel.clone())
                    .unwrap_or(false)
            })
    }
    fn is_eligible_player(&self, player: &Bound<'py, Player>, is_client_cmd: bool) -> bool {
        if owner()
            .unwrap_or_default()
            .is_some_and(|owner_steam_id| player.borrow().steam_id == owner_steam_id)
        {
            return true;
        }

        let perm = if is_client_cmd {
            let client_cmd_permission_cvar = format!(
                "qlx_ccmd_perm_{}",
                self.borrow().name.first().unwrap_or(&"invalid".to_string())
            );
            get_cvar(&client_cmd_permission_cvar)
                .unwrap_or_default()
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(self.borrow().client_cmd_perm)
        } else {
            let cmd_permission_cvar = format!(
                "qlx_perm_{}",
                self.borrow().name.first().unwrap_or(&"invalid".to_string())
            );
            let configured_cmd_permission = get_cvar(&cmd_permission_cvar);
            configured_cmd_permission
                .unwrap_or_default()
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(self.borrow().permission)
        };

        if perm == 0 {
            return true;
        }

        self.borrow()
            .plugin
            .bind(self.py())
            .getattr(intern!(self.py(), "db"))
            .ok()
            .filter(|value| !value.is_none())
            .and_then(|plugin_db| {
                plugin_db
                    .call_method1(intern!(self.py(), "get_permission"), (&player,))
                    .ok()
            })
            .and_then(|player_perm_result| player_perm_result.extract::<i32>().ok())
            .is_some_and(|player_perm| player_perm >= perm)
    }
}

#[cfg(test)]
mod command_tests {
    use crate::ffi::c::prelude::{CVar, CVarBuilder, cvar_t};
    use crate::ffi::python::{prelude::*, pyshinqlx_test_support::*};
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use rstest::*;

    use pyo3::prelude::*;
    use pyo3::{
        exceptions::{PyKeyError, PyValueError},
        types::{PyBool, PyList, PyString, PyTuple},
    };

    fn test_plugin_with_permission_db(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
        let test_plugin = test_plugin(py);
        PyModule::from_code(
            py,
            cr#"
class mocked_db:
    def get_permission(*args):
        return 2
            "#,
            c"",
            c"",
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
                &test_plugin(py).call0().expect("this should not happen"),
                py.None().bind(py),
                PyBool::new(py, true).as_any(),
                0,
                py.None().bind(py),
                py.None().bind(py),
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

            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let command = Command::py_new(
                &test_plugin(py).call0().expect("this should not happen"),
                py.None().bind(py),
                &capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook"),
                0,
                chat_channel.as_any(),
                py.None().bind(py),
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

            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let command = Command::py_new(
                &test_plugin(py).call0().expect("this should not happen"),
                py.None().bind(py),
                &capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook"),
                0,
                py.None().bind(py),
                chat_channel.as_any(),
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
            let names_pylist = PyList::new(py, &names_vec).expect("this should not happen");

            let command = Command::py_new(
                &test_plugin(py).call0().expect("this should not happen"),
                PyTuple::new(py, names_pylist)
                    .expect("this should not happen")
                    .as_any(),
                &capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook"),
                0,
                py.None().bind(py),
                py.None().bind(py),
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
            let names_pylist = PyTuple::new(py, &names_vec).expect("this should not happen");

            let command = Command::py_new(
                &test_plugin(py).call0().expect("this should not happen"),
                PyTuple::new(py, names_pylist)
                    .expect("this should not happen")
                    .as_any(),
                &capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook"),
                0,
                py.None().bind(py),
                py.None().bind(py),
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
                &test_plugin(py).call0().expect("this should not happen"),
                PyString::new(py, "cmd_name").as_any(),
                &capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook"),
                0,
                py.None().bind(py),
                py.None().bind(py),
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

            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let console_channel =
                Bound::new(py, ConsoleChannel::py_new()).expect("this should not happen");

            let command = Command::py_new(
                &test_plugin(py).call0().expect("this should not happen"),
                PyString::new(py, "cmd_name").as_any(),
                &capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook"),
                0,
                PyList::new(
                    py,
                    [
                        chat_channel.clone().as_any(),
                        console_channel.clone().as_any(),
                    ],
                )
                .expect("this should not happen")
                .as_any(),
                py.None().bind(py),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_ok_and(|cmd| {
                PyList::new(
                    py,
                    cmd.channels
                        .read()
                        .iter()
                        .map(|channel| channel.clone_ref(py))
                        .collect::<Vec<PyObject>>(),
                )
                .expect("this should not happen")
                .eq(PyList::new(
                    py,
                    [
                        chat_channel.clone().as_any(),
                        console_channel.clone().as_any(),
                    ],
                )
                .expect("this should not happen"))
                .expect("this should not happen")
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn constructor_with_multiple_exclude_channels(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let console_channel =
                Bound::new(py, ConsoleChannel::py_new()).expect("this should not happen");

            let command = Command::py_new(
                &test_plugin(py).call0().expect("this should not happen"),
                PyString::new(py, "cmd_name").as_any(),
                &capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook"),
                0,
                py.None().bind(py),
                PyList::new(
                    py,
                    [
                        chat_channel.clone().as_any(),
                        console_channel.clone().as_any(),
                    ],
                )
                .expect("This should not happen")
                .as_any(),
                true,
                0,
                true,
                "",
            );
            assert!(command.is_ok_and(|cmd| {
                PyList::new(
                    py,
                    cmd.exclude_channels
                        .read()
                        .iter()
                        .map(|channel| channel.clone_ref(py))
                        .collect::<Vec<PyObject>>(),
                )
                .expect("this should not happen")
                .eq(PyList::new(
                    py,
                    [
                        chat_channel.clone().as_any(),
                        console_channel.clone().as_any(),
                    ],
                )
                .expect("this should not happen"))
                .expect("this should not happen")
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn command_can_be_traversed_for_garbage_collector(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);

            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let console_channel =
                Bound::new(py, ConsoleChannel::py_new()).expect("this should not happen");

            let command = Command::py_new(
                &test_plugin(py).call0().expect("this should not happen"),
                PyString::new(py, "cmd_name").as_any(),
                &capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook"),
                0,
                PyList::new(py, [chat_channel.into_any()])
                    .expect("this shold not happen")
                    .as_any(),
                PyList::new(py, [console_channel.into_any()])
                    .expect("this should not happen")
                    .as_any(),
                true,
                0,
                true,
                "",
            )
            .expect("this should not happen");
            let _py_command = Bound::new(py, command).expect("this should not happen");

            let result = py.import("gc").and_then(|gc| gc.call_method0("collect"));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn execute_calls_handler(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let command = Command {
                plugin: test_plugin(py)
                    .call0()
                    .expect("this should not happen")
                    .unbind(),
                name: vec!["cmd".to_string()],
                handler: capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                permission: 0,
                channels: vec![].into(),
                exclude_channels: vec![].into(),
                client_cmd_pass: false,
                client_cmd_perm: 0,
                prefix: false,
                usage: "".to_string(),
            };

            let result = Bound::new(py, command)
                .expect("this should not happen")
                .execute(
                    &Bound::new(py, default_test_player()).expect("this should not happen"),
                    "cmd",
                    py.None().bind(py),
                );
            assert!(result.is_ok());
            assert!(
                capturing_hook
                    .call_method1(
                        "assert_called_with",
                        (default_test_player(), ["cmd"], py.None(),)
                    )
                    .is_ok()
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn execute_when_name_is_empty(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let command = Command {
                plugin: test_plugin(py)
                    .call0()
                    .expect("this should not happen")
                    .unbind(),
                name: vec![],
                handler: capturing_hook
                    .getattr("hook")
                    .expect("could not get capturing hook")
                    .unbind(),
                permission: 0,
                channels: vec![].into(),
                exclude_channels: vec![].into(),
                client_cmd_pass: false,
                client_cmd_perm: 0,
                prefix: false,
                usage: "".to_string(),
            };

            let result = Bound::new(py, command)
                .expect("this should not happen")
                .execute(
                    &Bound::new(py, default_test_player()).expect("this should not happen"),
                    "cmd",
                    py.None().bind(py),
                );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_eligible_name_with_no_prefix(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let command = Bound::new(
                py,
                Command {
                    plugin: test_plugin(py)
                        .call0()
                        .expect("this should not happen")
                        .unbind(),
                    name: vec!["cmd_name".into()],
                    handler: capturing_hook
                        .getattr("hook")
                        .expect("could not get capturing hook")
                        .unbind(),
                    permission: 0,
                    channels: vec![].into(),
                    exclude_channels: vec![].into(),
                    client_cmd_pass: false,
                    client_cmd_perm: 0,
                    prefix: false,
                    usage: "".to_string(),
                },
            )
            .expect("this should not happen");

            assert!(command.is_eligible_name("cmd_name"));
            assert!(!command.is_eligible_name("unmatched_cmd_name"));
            assert!(!command.is_eligible_name("!cmd_name"));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin(py)
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 0,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 0,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    assert!(!command.is_eligible_name("cmd_name"));
                    assert!(!command.is_eligible_name("!unmatched_cmd_name"));
                    assert!(command.is_eligible_name("!cmd_name"));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_eligilble_channel_when_none_are_configured(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let capturing_hook = capturing_hook(py);
            let command = Bound::new(
                py,
                Command {
                    plugin: test_plugin(py)
                        .call0()
                        .expect("this should not happen")
                        .unbind(),
                    name: vec!["cmd_name".into()],
                    handler: capturing_hook
                        .getattr("hook")
                        .expect("could not get capturing hook")
                        .unbind(),
                    permission: 0,
                    channels: vec![].into(),
                    exclude_channels: vec![].into(),
                    client_cmd_pass: false,
                    client_cmd_perm: 0,
                    prefix: true,
                    usage: "".to_string(),
                },
            )
            .expect("this should not happen");

            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            assert!(command.is_eligible_channel(chat_channel.as_any()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_eligilble_channel_with_configured_allowed_channels(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let console_channel =
                Bound::new(py, ConsoleChannel::py_new()).expect("this should not happen");
            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");
            let red_team_chat_channel = Bound::new(
                py,
                TeamChatChannel::py_new("red", "red_team", "print \"{}\n\"\n"),
            )
            .expect("this should not happen");

            let capturing_hook = capturing_hook(py);
            let command = Bound::new(
                py,
                Command {
                    plugin: test_plugin(py)
                        .call0()
                        .expect("this should not happen")
                        .unbind(),
                    name: vec!["cmd_name".into()],
                    handler: capturing_hook
                        .getattr("hook")
                        .expect("could not get capturing hook")
                        .unbind(),
                    permission: 0,
                    channels: vec![
                        console_channel.clone().into_any().unbind(),
                        chat_channel.clone().into_any().unbind(),
                    ]
                    .into(),
                    exclude_channels: vec![].into(),
                    client_cmd_pass: false,
                    client_cmd_perm: 0,
                    prefix: true,
                    usage: "".to_string(),
                },
            )
            .expect("this should not happen");

            assert!(command.is_eligible_channel(chat_channel.as_any()));
            assert!(command.is_eligible_channel(console_channel.as_any()));
            assert!(!command.is_eligible_channel(red_team_chat_channel.as_any()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_eligilble_channel_with_configured_allowed_and_exclude_channels(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let console_channel =
                Bound::new(py, ConsoleChannel::py_new()).expect("this should not happen");
            let chat_channel = Bound::new(
                py,
                TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
            )
            .expect("this should not happen");
            let red_team_chat_channel = Bound::new(
                py,
                TeamChatChannel::py_new("red", "red_team", "print \"{}\n\"\n"),
            )
            .expect("this should not happen");
            let blue_team_chat_channel = Bound::new(
                py,
                TeamChatChannel::py_new("blue", "blue_team", "print \"{}\n\"\n"),
            )
            .expect("this should not happen");

            let capturing_hook = capturing_hook(py);
            let command = Bound::new(
                py,
                Command {
                    plugin: test_plugin(py)
                        .call0()
                        .expect("this should not happen")
                        .unbind(),
                    name: vec!["cmd_name".into()],
                    handler: capturing_hook
                        .getattr("hook")
                        .expect("could not get capturing hook")
                        .unbind(),
                    permission: 0,
                    channels: vec![
                        console_channel.clone().into_any().unbind(),
                        chat_channel.clone().into_any().unbind(),
                    ]
                    .into(),
                    exclude_channels: vec![
                        red_team_chat_channel.clone().into_any().unbind(),
                        blue_team_chat_channel.clone().into_any().unbind(),
                    ]
                    .into(),
                    client_cmd_pass: false,
                    client_cmd_perm: 0,
                    prefix: true,
                    usage: "".to_string(),
                },
            )
            .expect("this should not happen");

            assert!(command.is_eligible_channel(chat_channel.as_any()));
            assert!(command.is_eligible_channel(console_channel.as_any()));
            assert!(!command.is_eligible_channel(red_team_chat_channel.as_any()));
            assert!(!command.is_eligible_channel(blue_team_chat_channel.as_any()));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin(py)
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 5,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 5,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(command.is_eligible_player(&player, false));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin(py).unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 5,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 5,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(command.is_eligible_player(&player, true));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin(py)
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 5,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 5,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(command.is_eligible_player(&player, false));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin(py)
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 0,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 0,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(command.is_eligible_player(&player, false));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin(py)
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 5,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 5,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(command.is_eligible_player(&player, true));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin(py)
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 0,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 0,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(command.is_eligible_player(&player, true));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 5,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 5,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(!command.is_eligible_player(&player, false));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 5,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 5,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(!command.is_eligible_player(&player, true));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 1,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 1,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(command.is_eligible_player(&player, false));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 3,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 3,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(!command.is_eligible_player(&player, false));
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
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin
                                .call0()
                                .expect("this should not happen")
                                .unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 1,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 1,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(command.is_eligible_player(&player, true));
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
                        test_plugin_with_permission_db(py).expect("this should not happen");
                    let command = Bound::new(
                        py,
                        Command {
                            plugin: test_plugin.unbind(),
                            name: vec!["cmd_name".into()],
                            handler: capturing_hook
                                .getattr("hook")
                                .expect("could not get capturing hook")
                                .unbind(),
                            permission: 3,
                            channels: vec![].into(),
                            exclude_channels: vec![].into(),
                            client_cmd_pass: false,
                            client_cmd_perm: 3,
                            prefix: true,
                            usage: "".to_string(),
                        },
                    )
                    .expect("this should not happen");

                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    assert!(!command.is_eligible_player(&player, true));
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
#[pyclass(module = "_commands", name = "CommandInvoker", frozen)]
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

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        self.commands
            .read()
            .iter()
            .flat_map(|prio_cmds| prio_cmds.iter().map(|cmd| visit.call(cmd)))
            .collect::<Result<Vec<_>, PyTraverseError>>()
            .map(|_| ())
    }

    fn __clear__(&self) {
        self.commands.write().iter_mut().for_each(|prio_cmds| {
            prio_cmds.clear();
        });
    }

    #[getter(commands)]
    fn get_commands(slf: &Bound<'_, Self>) -> Vec<Command> {
        slf.get_commands()
    }

    /// Check if a command is already registed.
    ///
    /// Commands are unique by (command.name, command.handler).
    fn is_registered(slf: &Bound<'_, Self>, command: &Bound<'_, Command>) -> bool {
        slf.is_registered(command)
    }

    pub(crate) fn add_command(
        slf: &Bound<'_, Self>,
        command: &Bound<'_, Command>,
        priority: usize,
    ) -> PyResult<()> {
        slf.add_command(command, priority)
    }

    pub(crate) fn remove_command(
        slf: &Bound<'_, Self>,
        command: &Bound<'_, Command>,
    ) -> PyResult<()> {
        slf.remove_command(command)
    }

    pub(crate) fn handle_input(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: &str,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        slf.handle_input(player, msg, channel)
    }
}

pub(crate) trait CommandInvokerMethods {
    fn get_commands(&self) -> Vec<Command>;

    fn is_registered(&self, command: &Bound<'_, Command>) -> bool;

    fn add_command(&self, command: &Bound<'_, Command>, priority: usize) -> PyResult<()>;

    fn remove_command(&self, command: &Bound<'_, Command>) -> PyResult<()>;

    fn handle_input(
        &self,
        player: &Bound<'_, Player>,
        msg: &str,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<bool>;
}

impl CommandInvokerMethods for Bound<'_, CommandInvoker> {
    fn get_commands(&self) -> Vec<Command> {
        self.borrow()
            .commands
            .read()
            .iter()
            .flat_map(|cmds| {
                cmds.iter().map(|command| {
                    let bound_cmd = command.bind(self.py()).borrow();
                    Command {
                        plugin: bound_cmd.plugin.clone_ref(self.py()),
                        name: bound_cmd.name.clone(),
                        handler: bound_cmd.handler.clone_ref(self.py()),
                        permission: bound_cmd.permission,
                        channels: bound_cmd
                            .channels
                            .read()
                            .iter()
                            .map(|channel| channel.clone_ref(self.py()))
                            .collect::<Vec<Py<PyAny>>>()
                            .into(),
                        exclude_channels: bound_cmd
                            .exclude_channels
                            .read()
                            .iter()
                            .map(|channel| channel.clone_ref(self.py()))
                            .collect::<Vec<Py<PyAny>>>()
                            .into(),
                        client_cmd_pass: bound_cmd.client_cmd_pass,
                        client_cmd_perm: bound_cmd.client_cmd_perm,
                        prefix: bound_cmd.prefix,
                        usage: bound_cmd.usage.clone(),
                    }
                })
            })
            .collect()
    }

    fn is_registered(&self, command: &Bound<'_, Command>) -> bool {
        let borrowed_command = command.borrow();
        self.borrow().commands.read().iter().any(|prio_cmds| {
            prio_cmds.iter().any(|cmd| {
                let bound_cmd = cmd.bind(self.py()).borrow();
                bound_cmd.name.len() == borrowed_command.name.len()
                    && bound_cmd
                        .name
                        .iter()
                        .all(|name| borrowed_command.name.contains(name))
                    && bound_cmd
                        .handler
                        .bind(self.py())
                        .eq(borrowed_command.handler.bind(self.py()))
                        .unwrap_or(false)
            })
        })
    }

    fn add_command(&self, command: &Bound<'_, Command>, priority: usize) -> PyResult<()> {
        if self.is_registered(command) {
            return Err(PyValueError::new_err(
                "Attempted to add an already registered command.",
            ));
        }
        let slf = self.borrow();
        let Some(mut commands) = slf.commands.try_write() else {
            return PyModule::from_code(
                self.py(),
                cr#"
import shinqlx


@shinqlx.next_frame
def add_command(cmd, priority):
    shinqlx.COMMANDS.add_command(cmd, priority)
        "#,
                c"",
                c"",
            )
            .and_then(|module| {
                module.call_method1(intern!(self.py(), "add_command"), (&command, priority))
            })
            .map(|_| ());
        };

        commands[priority].push(command.clone().unbind());
        Ok(())
    }

    fn remove_command(&self, command: &Bound<'_, Command>) -> PyResult<()> {
        if !self.is_registered(command) {
            return Err(PyValueError::new_err(
                "Attempted to remove a command that was never added.",
            ));
        }

        let slf = self.borrow();
        let Some(mut commands) = slf.commands.try_write() else {
            return PyModule::from_code(
                self.py(),
                cr#"
import shinqlx


@shinqlx.next_frame
def remove_command(cmd):
    shinqlx.COMMANDS.remove_command(cmd)
        "#,
                c"",
                c"",
            )
            .and_then(|module| {
                module.call_method1(intern!(self.py(), "remove_command"), (command,))
            })
            .map(|_| ());
        };
        let borrowed_command = command.borrow();
        commands.iter_mut().for_each(|prio_commands| {
            prio_commands.retain(|cmd| {
                let bound_cmd = cmd.bind(self.py()).borrow();
                bound_cmd.name.len() != borrowed_command.name.len()
                    || !borrowed_command
                        .name
                        .iter()
                        .all(|name| bound_cmd.name.contains(name))
                    || bound_cmd
                        .handler
                        .bind(self.py())
                        .ne(borrowed_command.handler.bind(self.py()))
                        .unwrap_or(true)
            });
        });

        Ok(())
    }

    fn handle_input(
        &self,
        player: &Bound<'_, Player>,
        msg: &str,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let Some(name) = msg
            .split_whitespace()
            .next()
            .map(|value| value.to_lowercase())
        else {
            return Ok(false);
        };
        let Ok(channel_name) = channel
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
                        .bind(self.py())
                        .get_item(intern!(self.py(), "command"))
                        .ok()
                })
        else {
            return Err(PyEnvironmentError::new_err(
                "could not get access to command dispatcher",
            ));
        };

        let slf = self.borrow();
        let commands = slf.commands.read();
        for priority_level in 0..commands.len() {
            for cmd in commands[priority_level].iter() {
                let bound_cmd = cmd.bind(self.py());
                if !bound_cmd.is_eligible_name(&name) {
                    continue;
                }
                if !bound_cmd.is_eligible_channel(channel) {
                    continue;
                }
                if !bound_cmd.is_eligible_player(player, is_client_cmd) {
                    continue;
                }

                if is_client_cmd {
                    pass_through = bound_cmd.borrow().client_cmd_pass;
                }

                let cmd_copy = Command {
                    plugin: bound_cmd.borrow().plugin.clone_ref(self.py()),
                    name: bound_cmd.borrow().name.clone(),
                    handler: bound_cmd.borrow().handler.clone_ref(self.py()),
                    permission: bound_cmd.borrow().permission,
                    channels: bound_cmd
                        .borrow()
                        .channels
                        .read()
                        .iter()
                        .map(|channel| channel.clone_ref(self.py()))
                        .collect::<Vec<Py<PyAny>>>()
                        .into(),
                    exclude_channels: bound_cmd
                        .borrow()
                        .exclude_channels
                        .read()
                        .iter()
                        .map(|channel| channel.clone_ref(self.py()))
                        .collect::<Vec<Py<PyAny>>>()
                        .into(),
                    client_cmd_pass: bound_cmd.borrow().client_cmd_pass,
                    client_cmd_perm: bound_cmd.borrow().client_cmd_perm,
                    prefix: bound_cmd.borrow().prefix,
                    usage: bound_cmd.borrow().usage.clone(),
                };

                let dispatcher_result = CommandDispatcherMethods::dispatch(
                    command_dispatcher.downcast()?,
                    &Bound::new(self.py(), player.clone())?,
                    &Bound::new(self.py(), cmd_copy)?,
                    msg,
                )?;
                if dispatcher_result
                    .downcast::<PyBool>()
                    .is_ok_and(|value| !value.is_true())
                {
                    return Ok(true);
                }

                let cmd_result = bound_cmd.execute(player, msg, channel)?;
                let cmd_result_return_code = cmd_result.extract::<PythonReturnCodes>();
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
                    if !bound_cmd.borrow().usage.is_empty() {
                        let usage_msg = format!("^7Usage: ^6{} {}", name, bound_cmd.borrow().usage);
                        channel.call_method1(intern!(self.py(), "reply"), (&usage_msg,))?;
                    }
                } else if !cmd_result_return_code
                    .as_ref()
                    .is_ok_and(|&value| value == PythonReturnCodes::RET_NONE)
                {
                    pyshinqlx_get_logger(self.py(), None).and_then(|logger| {
                        let cmd_handler_name = bound_cmd
                            .borrow()
                            .handler
                            .getattr(self.py(), intern!(self.py(), "__name__"))?;
                        let warning_level = self
                            .py()
                            .import(intern!(self.py(), "logging"))
                            .and_then(|logging_module| {
                                logging_module.getattr(intern!(self.py(), "WARNING"))
                            })?;
                        logger
                            .call_method(
                                intern!(self.py(), "makeRecord"),
                                (
                                    intern!(self.py(), "shinqlx"),
                                    warning_level,
                                    intern!(self.py(), ""),
                                    -1,
                                    intern!(
                            self.py(),
                            "Command '%s' with handler '%s' returned an unknown return value: %s"
                        ),
                                    (
                                        bound_cmd.borrow().name.clone(),
                                        cmd_handler_name,
                                        cmd_result,
                                    ),
                                    self.py().None(),
                                ),
                                Some(
                                    &[(
                                        intern!(self.py(), "func"),
                                        intern!(self.py(), "handle_input"),
                                    )]
                                    .into_py_dict(self.py())?,
                                ),
                            )
                            .and_then(|log_record| {
                                logger.call_method1(intern!(self.py(), "handle"), (log_record,))
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
    use super::CommandPriorities;

    use crate::ffi::python::prelude::*;
    use crate::ffi::python::pyshinqlx_test_support::{
        capturing_hook, default_command, default_test_player, returning_false_hook,
        run_all_frame_tasks,
    };
    use crate::ffi::python::{EVENT_DISPATCHERS, PythonReturnCodes};

    use crate::prelude::*;

    use crate::ffi::c::prelude::{CVar, CVarBuilder, cvar_t};
    use crate::hooks::mock_hooks::shinqlx_send_server_command_context;

    use core::borrow::BorrowMut;
    use git2::IntoCString;
    use mockall::predicate;
    use rstest::*;

    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn emoty_command_invoker_has_empty_commands(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            assert!(command_invoker.get_commands().is_empty());
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_registered_for_unregistered_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");

            let py_command = Bound::new(py, default_command(py)).expect("this should not happen");
            assert!(!command_invoker.is_registered(&py_command));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_registered_with_variations_of_registered_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            let py_command = Bound::new(py, default_command(py)).expect("this should not happen");
            command_invoker
                .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                .expect("this should not happen");

            let unregistered_command1 = Command {
                handler: py.None(),
                ..default_command(py)
            };
            let py_unregistered_command1 =
                Bound::new(py, unregistered_command1).expect("this should not happen");
            assert!(!command_invoker.is_registered(&py_unregistered_command1));

            let unregistered_command2 = Command {
                name: vec!["cmd_name".into(), "cmd_alias1".into(), "cmd_alias2".into()],
                ..default_command(py)
            };
            let py_unregistered_command2 =
                Bound::new(py, unregistered_command2).expect("this should not happen");
            assert!(!command_invoker.is_registered(&py_unregistered_command2));

            let unregistered_command3 = Command {
                name: vec!["mismatched_cmd_name".into()],
                ..default_command(py)
            };
            let py_unregistered_command3 =
                Bound::new(py, unregistered_command3).expect("this should not happen");
            assert!(!command_invoker.is_registered(&py_unregistered_command3));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn is_registered_for_registered_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            let py_command = Bound::new(py, default_command(py)).expect("this should not happen");
            command_invoker
                .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                .expect("this should not happen");

            assert!(command_invoker.is_registered(&py_command));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_command_adds_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            let py_command = Bound::new(py, default_command(py)).expect("this should not happen");

            let result =
                command_invoker.add_command(&py_command, CommandPriorities::PRI_NORMAL as usize);
            assert!(result.is_ok());
            assert!(
                command_invoker
                    .get_commands()
                    .first()
                    .is_some_and(|cmd| cmd.name[0] == "cmd_name")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn add_command_for_already_added_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            let py_command = Bound::new(py, default_command(py)).expect("this should not happen");

            command_invoker
                .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                .expect("this should not happen");

            let result =
                command_invoker.add_command(&py_command, CommandPriorities::PRI_NORMAL as usize);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_command_for_command_not_added(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            let py_command = Bound::new(py, default_command(py)).expect("this should not happen");

            let result = command_invoker.remove_command(&py_command);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn remove_command_removes_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            let py_command = Bound::new(py, default_command(py)).expect("this should not happen");

            command_invoker
                .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                .expect("this should not happen");

            let result = command_invoker.remove_command(&py_command);
            assert!(result.is_ok());
            assert!(command_invoker.get_commands().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn handle_input_for_empty_input(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");
            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");

            let result = command_invoker.handle_input(&player, " ", chat_channel.as_any());
            assert!(result.is_ok_and(|value| !value));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_with_no_event_dispatcher(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");
            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            EVENT_DISPATCHERS.store(None);

            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");

            let result = command_invoker.handle_input(&player, "cmd_name", chat_channel.as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_with_non_eligible_cmd_name(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");
            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<CommandDispatcher>())
                .expect("could not add command dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let py_command = Bound::new(py, default_command(py)).expect("this should not happen");

            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            command_invoker
                .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                .expect("this should not happen");

            let result = command_invoker.handle_input(&player, "other_name", chat_channel.as_any());
            assert!(result.is_ok_and(|pass_through| pass_through));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_with_non_eligible_channel(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");
            let chat_channel = Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                .expect("this should not happen");

            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<CommandDispatcher>())
                .expect("could not add command dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let py_command = Bound::new(py, default_command(py)).expect("this should not happen");

            let command_invoker =
                Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
            command_invoker
                .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                .expect("this should not happen");

            let result = command_invoker.handle_input(&player, "cmd_name", chat_channel.as_any());
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
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");
                    let chat_channel =
                        Bound::new(py, ChatChannel::py_new("chat", "print \"{}\n\"\n"))
                            .expect("this should not happen");

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let py_command =
                        Bound::new(py, default_command(py)).expect("this should not happen");

                    let command_invoker =
                        Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
                    command_invoker
                        .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                        .expect("this should not happen");

                    let result =
                        command_invoker.handle_input(&player, "cmd_name", chat_channel.as_any());
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
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");
                    let client_command_channel =
                        Bound::new(py, ClientCommandChannel::py_new(player.get()))
                            .expect("this should not happen");

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let capturing_hook = capturing_hook(py);
                    let py_command = Bound::new(
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

                    let command_invoker =
                        Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
                    command_invoker
                        .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        &player,
                        "cmd_name",
                        client_command_channel.as_any(),
                    );
                    assert!(
                        result.is_ok_and(|actual_pass_through| actual_pass_through == pass_through)
                    );
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", ("_", ["cmd_name"], "_"))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_input_when_event_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd != "qlx_owner", |_| None, 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");
                    let client_command_channel =
                        Bound::new(py, ClientCommandChannel::py_new(player.get()))
                            .expect("this should not happen");

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    event_dispatcher
                        .__getitem__("command")
                        .and_then(|command_dispatcher| {
                            command_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    returning_false_hook(py),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to vote dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let py_command =
                        Bound::new(py, default_command(py)).expect("this should not happen");

                    let command_invoker =
                        Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
                    command_invoker
                        .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        &player,
                        "cmd_name",
                        client_command_channel.as_any(),
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
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");
                    let client_command_channel =
                        Bound::new(py, ClientCommandChannel::py_new(player.get()))
                            .expect("this should not happen");

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let module_definition = format!(
                        r#"
def cmd_handler(*args, **kwargs):
    return {}
            "#,
                        return_code as i32
                    );
                    let handler = PyModule::from_code(
                        py,
                        &module_definition
                            .into_c_string()
                            .expect("this should not happen"),
                        c"",
                        c"",
                    )
                    .and_then(|result| result.getattr("cmd_handler"))
                    .expect("this should not happen");
                    let command = Command {
                        handler: handler.unbind(),
                        prefix: false,
                        ..default_command(py)
                    };
                    let py_command = Bound::new(py, command).expect("this should not happen");

                    let command_invoker =
                        Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
                    command_invoker
                        .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                        .expect("this should not happen");

                    let result = command_invoker.handle_input(
                        &player,
                        "cmd_name",
                        client_command_channel.as_any(),
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
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");
                    let chat_channel = Bound::new(
                        py,
                        TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                    )
                    .expect("this should not happen");

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let module_definition = format!(
                        r#"
def cmd_handler(*args, **kwargs):
    return {}
            "#,
                        PythonReturnCodes::RET_USAGE as i32
                    );
                    let handler = PyModule::from_code(
                        py,
                        &module_definition
                            .into_c_string()
                            .expect("this should not happen"),
                        c"",
                        c"",
                    )
                    .and_then(|result| result.getattr("cmd_handler"))
                    .expect("this should not happen");
                    let command = Command {
                        handler: handler.unbind(),
                        prefix: false,
                        ..default_command(py)
                    };
                    let py_command = Bound::new(py, command).expect("this should not happen");

                    let command_invoker =
                        Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
                    command_invoker
                        .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                        .expect("this should not happen");

                    let result =
                        command_invoker.handle_input(&player, "cmd_name", chat_channel.as_any());
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
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");
                    let chat_channel = Bound::new(
                        py,
                        TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                    )
                    .expect("this should not happen");

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let module_definition = format!(
                        r#"
def cmd_handler(*args, **kwargs):
    return {}
            "#,
                        PythonReturnCodes::RET_USAGE as i32
                    );
                    let handler = PyModule::from_code(
                        py,
                        &module_definition
                            .into_c_string()
                            .expect("this should not happen"),
                        c"",
                        c"",
                    )
                    .and_then(|result| result.getattr("cmd_handler"))
                    .expect("this should not happen");
                    let command = Command {
                        handler: handler.unbind(),
                        prefix: false,
                        usage: "how to use me".to_string(),
                        ..default_command(py)
                    };
                    let py_command = Bound::new(py, command).expect("this should not happen");

                    let command_invoker =
                        Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
                    command_invoker
                        .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                        .expect("this should not happen");

                    let result =
                        command_invoker.handle_input(&player, "cmd_name", chat_channel.as_any());
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
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");
                    let chat_channel = Bound::new(
                        py,
                        TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                    )
                    .expect("this should not happen");

                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<CommandDispatcher>())
                        .expect("could not add command dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let handler = PyModule::from_code(
                        py,
                        cr#"
def cmd_handler(*args, **kwargs):
    return dict()
            "#,
                        c"",
                        c"",
                    )
                    .and_then(|result| result.getattr("cmd_handler"))
                    .expect("this should not happen");
                    let command = Command {
                        handler: handler.unbind(),
                        prefix: false,
                        ..default_command(py)
                    };
                    let py_command = Bound::new(py, command).expect("this should not happen");

                    let command_invoker =
                        Bound::new(py, CommandInvoker::py_new()).expect("this should not happen");
                    command_invoker
                        .add_command(&py_command, CommandPriorities::PRI_NORMAL as usize)
                        .expect("this should not happen");

                    let result =
                        command_invoker.handle_input(&player, "cmd_name", chat_channel.as_any());
                    assert!(result.is_ok_and(|pass_through| pass_through));
                });
            });
    }
}
