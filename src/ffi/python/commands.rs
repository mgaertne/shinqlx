use super::prelude::*;
use super::{owner, pyshinqlx_get_logger};

use pyo3::prelude::*;
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
#[pyclass(module = "_commands", name = "Command", get_all)]
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
    ) -> PyResult<Command> {
        if !handler.as_ref(py).is_callable() {
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
        name.as_ref(py)
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
        name.as_ref(py)
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
            if let Ok(mut iter) = channels.as_ref(py).iter() {
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
            if let Ok(mut iter) = exclude_channels.as_ref(py).iter() {
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
        let plugin = self.plugin.as_ref(py).into_py(py);
        let plugin_name = plugin.getattr(py, intern!(py, "name"))?;
        let logger = pyshinqlx_get_logger(py, Some(plugin))?;
        logger.call_method1(
            intern!(py, "debug"),
            (
                "%s executed: %s @ %s -> %s",
                player.steam_id,
                command_name,
                plugin_name,
                &channel,
            ),
        )?;

        let msg_vec: Vec<&str> = msg.split(' ').collect();
        self.handler
            .as_ref(py)
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
            .as_ref(py)
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
