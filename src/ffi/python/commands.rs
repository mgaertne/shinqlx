use super::prelude::*;
use super::{owner, pyshinqlx_get_logger, PythonReturnCodes};

use pyo3::prelude::*;
use pyo3::types::IntoPyDict;
use pyo3::{
    exceptions::PyKeyError,
    exceptions::PyValueError,
    intern,
    types::{PyList, PyTuple},
};

/// A class representing an input-triggered command.
///
/// Has information about the command itself, its usage, when and who to call when
/// action should be taken.
#[pyclass(module = "_commands", name = "Command", get_all, frozen)]
#[derive(Clone)]
pub(crate) struct Command {
    plugin: Py<PyAny>,
    name: Vec<String>,
    handler: Py<PyAny>,
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
    fn py_new(
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
        usage: String,
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
                        .for_each(|alias| names.push(alias.clone()));
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
                        .for_each(|alias| names.push(alias.clone()));
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
            usage,
        })
    }

    fn execute(
        &self,
        py: Python<'_>,
        player: Player,
        msg: String,
        channel: PyObject,
    ) -> PyResult<PyObject> {
        let Some(command_name) = self.name.first() else {
            return Err(PyKeyError::new_err("command has no 'name'"));
        };
        let plugin = self.plugin.bind(py).into_py(py);
        let plugin_name = plugin.getattr(py, intern!(py, "name"))?;
        let logger = pyshinqlx_get_logger(py, Some(plugin))?;
        let logging_module = py.import_bound(intern!(py, "logging"))?;
        let debug_level = logging_module.getattr(intern!(py, "DEBUG"))?;
        let log_record = logger.call_method(
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
        )?;
        logger.call_method1(intern!(py, "handle"), (log_record,))?;

        let msg_vec: Vec<&str> = msg.split(' ').collect();
        self.handler
            .bind(py)
            .into_py(py)
            .call1(py, (player, msg_vec, &channel))
    }

    fn is_eligible_name(&self, py: Python<'_>, name: String) -> bool {
        let compared_name = if !self.prefix {
            Some(name.as_str())
        } else {
            pyshinqlx_get_cvar(py, "qlx_commandPrefix")
                .ok()
                .flatten()
                .and_then(|prefix| name.strip_prefix(prefix.as_str()))
        };

        compared_name.is_some_and(|name| self.name.contains(&name.to_lowercase()))
    }

    /// Check if a chat channel is one this command should execute in.
    ///
    /// Exclude takes precedence.
    fn is_eligible_channel(&self, py: Python<'_>, channel: PyObject) -> bool {
        let Some(channel_name) = channel
            .bind(py)
            .str()
            .ok()
            .and_then(|channel_name_str| channel_name_str.extract::<String>().ok())
        else {
            return false;
        };

        let exclude_channel_names: Vec<String> = self
            .exclude_channels
            .iter()
            .flat_map(|channel| channel.extract::<String>(py).ok())
            .collect();
        if exclude_channel_names.contains(&channel_name) {
            return false;
        }

        let channel_names: Vec<String> = self
            .channels
            .iter()
            .flat_map(|channel| channel.extract::<String>(py).ok())
            .collect();
        channel_names.is_empty() || channel_names.contains(&channel_name)
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
    commands: [Vec<Command>; 5],
}

#[pymethods]
impl CommandInvoker {
    #[new]
    pub(crate) fn py_new() -> Self {
        Self {
            commands: [vec![], vec![], vec![], vec![], vec![]],
        }
    }

    #[getter(commands)]
    fn get_commands(&self) -> Vec<Command> {
        let mut returned = vec![];
        for index in 0..self.commands.len() {
            returned.extend(self.commands[index].clone());
        }
        returned
    }

    /// Check if a command is already registed.
    ///
    /// Commands are unique by (command.name, command.handler).
    fn is_registered(&self, py: Python<'_>, command: &Command) -> bool {
        self.commands.iter().any(|prio_cmds| {
            prio_cmds.iter().any(|cmd| {
                cmd.name == command.name
                    && cmd
                        .handler
                        .bind(py)
                        .eq(command.handler.bind(py))
                        .unwrap_or(false)
            })
        })
    }

    fn add_command(&mut self, py: Python<'_>, command: Command, priority: usize) -> PyResult<()> {
        if self.is_registered(py, &command) {
            return Err(PyValueError::new_err(
                "Attempted to add an already registered command.",
            ));
        }
        self.commands[priority].push(command);
        Ok(())
    }

    fn remove_command(&mut self, py: Python<'_>, command: Command) -> PyResult<()> {
        if !self.is_registered(py, &command) {
            return Err(PyValueError::new_err(
                "Attempted to add an already registered command.",
            ));
        }

        for index in 0..self.commands.len() {
            self.commands[index].retain(|cmd| {
                cmd.name != command.name
                    && cmd
                        .handler
                        .bind(py)
                        .ne(command.handler.bind(py))
                        .unwrap_or(true)
            })
        }

        Ok(())
    }

    fn handle_input(
        &self,
        py: Python<'_>,
        player: Player,
        msg: String,
        channel: PyObject,
    ) -> PyResult<bool> {
        let Some(name) = msg
            .split_whitespace()
            .next()
            .map(|value| value.to_lowercase().to_string())
        else {
            return Ok(false);
        };
        let Some(channel_name) = channel
            .bind(py)
            .str()
            .ok()
            .and_then(|channel_name_str| channel_name_str.extract::<String>().ok())
        else {
            return Ok(false);
        };
        let is_client_cmd = channel_name == "client_command";
        let mut pass_through = true;

        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
        let event_dispatchers = shinqlx_module.getattr(intern!(py, "EVENT_DISPATCHERS"))?;
        let command_dispatcher = event_dispatchers.get_item(intern!(py, "command"))?;

        for priority_level in 0..self.commands.len() {
            for cmd in &self.commands[priority_level] {
                if !cmd.is_eligible_name(py, name.clone()) {
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

                let dispatcher_result = command_dispatcher.call_method1(
                    intern!(py, "dispatch"),
                    (player.clone(), (*cmd).clone(), msg.clone()),
                )?;
                if dispatcher_result
                    .extract::<bool>()
                    .is_ok_and(|value| !value)
                {
                    return Ok(true);
                }

                let cmd_result = cmd.execute(
                    py,
                    player.clone(),
                    msg.clone(),
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
                    .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_EVENT)
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
