use super::prelude::*;

use super::{
    addadmin, addmod, addscore, addteamscore, ban, client_id, commands::CommandPriorities, demote,
    lock, mute, opsay, put, pyshinqlx_get_logger, set_teamsize, tempban, unban, unlock, unmute,
    BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, COMMANDS, CONSOLE_CHANNEL, EVENT_DISPATCHERS,
    RED_TEAM_CHAT_CHANNEL,
};
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_com_printf;
#[cfg(not(test))]
use crate::hooks::shinqlx_com_printf;

use crate::MAIN_ENGINE;
use crate::{
    ffi::c::prelude::{CS_VOTE_NO, CS_VOTE_STRING, CS_VOTE_YES},
    quake_live_engine::{ConsoleCommand, FindCVar, GetCVar, GetConfigstring, SetCVarLimit},
};

use pyo3::prelude::*;
use pyo3::{
    exceptions::{PyEnvironmentError, PyRuntimeError, PyValueError},
    gc::PyVisit,
    intern,
    types::{IntoPyDict, PyBool, PyDict, PyList, PySet, PyTuple, PyType},
    PyTraverseError,
};

/// The base plugin class.
///
/// Every plugin must inherit this or a subclass of this. It does not support any database
/// by itself, but it has a *database* static variable that must be a subclass of the
/// abstract class :class:`shinqlx.database.AbstractDatabase`. This abstract class requires
/// a few methods that deal with permissions. This will make sure that simple plugins that
/// only care about permissions can work on any database. Abstraction beyond that is hard,
/// so any use of the database past that point will be uncharted territory, meaning the
/// plugin will likely be database-specific unless you abstract it yourself.
///
/// Permissions for commands can be overriden in the config. If you have a plugin called
/// ``my_plugin`` with a command ``my_command``, you could override its permission
/// requirement by adding ``perm_my_command: 3`` under a ``[my_plugin]`` header.
/// This allows users to set custom permissions without having to edit the scripts.
///
/// .. warning::
///     I/O is the bane of single-threaded applications. You do **not** want blocking operations
///     in code called by commands or events. That could make players lag. Helper decorators
///     like :func:`shinqlx.thread` can be useful.
#[pyclass(name = "Plugin", module = "_plugin", subclass)]
pub(crate) struct Plugin {
    hooks: Vec<(String, PyObject, i32)>,
    commands: parking_lot::RwLock<Vec<Command>>,
    db_instance: PyObject,
}

#[pymethods]
impl Plugin {
    #[new]
    fn py_new(py: Python<'_>) -> Self {
        Self {
            hooks: vec![],
            commands: parking_lot::RwLock::new(vec![]),
            db_instance: py.None(),
        }
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        for hook in &self.hooks {
            visit.call(&hook.1)?;
        }

        visit.call(&self.db_instance)?;
        Ok(())
    }

    fn __clear__(&mut self) {
        self.hooks.clear();
    }

    fn __str__(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_type().name().map(|value| value.to_string())
    }

    /// The database instance.
    #[getter(db)]
    fn get_db(slf: &Bound<'_, Self>) -> PyResult<PyObject> {
        let plugin_name = Self::get_name(slf)?;
        let Ok(db_class) = slf
            .py()
            .get_type_bound::<Plugin>()
            .getattr(intern!(slf.py(), "database"))
        else {
            let error_msg = format!("a Plugin '{plugin_name}' does not have a database driver.");
            return Err(PyRuntimeError::new_err(error_msg));
        };

        if db_class.is_none() {
            let error_msg = format!("b Plugin '{plugin_name}' does not have a database driver.");
            return Err(PyRuntimeError::new_err(error_msg));
        }

        let mut plugin = slf.borrow_mut();
        if plugin.db_instance.bind(slf.py()).is_none() {
            let db_instance = db_class.call1((slf,))?;
            plugin.db_instance = db_instance.unbind();
        }

        Ok(plugin.db_instance.clone_ref(slf.py()))
    }

    /// The name of the plugin.
    #[getter(name)]
    fn get_name(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_type().name().map(|value| value.to_string())
    }

    /// A dictionary containing plugin names as keys and plugin instances
    /// as values of all currently loaded plugins.
    #[getter(plugins)]
    fn get_plugins<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let loaded_plugins = py
            .get_type_bound::<Plugin>()
            .getattr(intern!(py, "_loaded_plugins"))?
            .extract::<&PyDict>()?;

        Ok(loaded_plugins.into_py_dict_bound(py))
    }

    /// A list of all the hooks this plugin has.
    #[getter(hooks)]
    fn get_hooks(&self, py: Python<'_>) -> Vec<(String, PyObject, i32)> {
        py.allow_threads(|| self.hooks.clone())
    }

    /// A list of all the commands this plugin has registered.
    #[getter(commands)]
    fn get_commands(&self, py: Python<'_>) -> Vec<Command> {
        py.allow_threads(|| {
            self.commands
                .try_read()
                .map(|commands| commands.clone())
                .unwrap_or_default()
        })
    }

    /// A Game instance.
    #[getter(game)]
    fn get_game(&self, py: Python<'_>) -> Option<Game> {
        Game::py_new(py, true).ok()
    }

    /// An instance of :class:`logging.Logger`, but initialized for this plugin.
    #[getter(logger)]
    pub(crate) fn get_logger<'a>(slf: &Bound<'a, Self>) -> PyResult<Bound<'a, PyAny>> {
        let plugin_name = slf.get_type().name().map(|value| value.to_string())?;
        pyshinqlx_get_logger(slf.py(), Some(plugin_name.into_py(slf.py())))
    }

    #[pyo3(signature = (event, handler, priority = CommandPriorities::PRI_NORMAL as i32), text_signature = "(event, handler, priority = PRI_NORMAL)")]
    fn add_hook(
        slf: &Bound<'_, Self>,
        event: String,
        handler: PyObject,
        priority: i32,
    ) -> PyResult<()> {
        let Some(event_dispatcher) = EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| event_dispatchers.bind(slf.py()).get_item(&event).ok())
        else {
            return Err(PyEnvironmentError::new_err(
                "could not get access to event dispatcher",
            ));
        };

        let plugin_type = slf.get_type();
        let plugin_name = plugin_type.name()?;
        event_dispatcher.call_method1(
            intern!(slf.py(), "add_hook"),
            (plugin_name, &handler, priority),
        )?;

        let Ok(mut plugin) = slf.try_borrow_mut() else {
            return Err(PyEnvironmentError::new_err("could not borrow plugin hooks"));
        };
        plugin.hooks.push((event.clone(), handler, priority));

        Ok(())
    }

    #[pyo3(signature = (event, handler, priority = CommandPriorities::PRI_NORMAL as i32), text_signature = "(event, handler, priority = PRI_NORMAL)")]
    fn remove_hook(
        slf: &Bound<'_, Self>,
        event: String,
        handler: PyObject,
        priority: i32,
    ) -> PyResult<()> {
        let Some(event_dispatcher) = EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| event_dispatchers.bind(slf.py()).get_item(&event).ok())
        else {
            return Err(PyEnvironmentError::new_err(
                "could not get access to console print dispatcher",
            ));
        };

        let plugin_type = slf.get_type();
        let plugin_name = plugin_type.name()?;
        event_dispatcher.call_method1(
            intern!(slf.py(), "remove_hook"),
            (plugin_name, &handler, priority),
        )?;

        let Ok(mut plugin) = slf.try_borrow_mut() else {
            return Err(PyEnvironmentError::new_err("cound not borrow plugin hooks"));
        };
        plugin
            .hooks
            .retain(|(hook_event, hook_handler, hook_priority)| {
                hook_event == &event
                    && hook_handler
                        .bind(slf.py())
                        .eq(handler.bind(slf.py()))
                        .unwrap_or(true)
                    && hook_priority == &priority
            });

        Ok(())
    }

    #[pyo3(
    signature = (
        name,
        handler,
        permission = 0,
        channels = None,
        exclude_channels = None,
        priority = CommandPriorities::PRI_NORMAL as u32,
        client_cmd_pass = false,
        client_cmd_perm = 0,
        prefix = true,
        usage = ""),
    text_signature = "(name, handler, permission = 0, channels = None, exclude_channels = None, priority = PRI_NORMAL, client_cmd_pass = false, client_cmd_perm = 0, prefix = true, usage = \"\")")]
    #[allow(clippy::too_many_arguments)]
    fn add_command(
        slf: Bound<'_, Self>,
        name: PyObject,
        handler: PyObject,
        permission: i32,
        channels: Option<PyObject>,
        exclude_channels: Option<PyObject>,
        priority: u32,
        client_cmd_pass: bool,
        client_cmd_perm: i32,
        prefix: bool,
        usage: &str,
    ) -> PyResult<()> {
        let Ok(plugin) = slf.try_borrow() else {
            return Err(PyEnvironmentError::new_err("cannot borrow plugin"));
        };

        let py_channels = channels.unwrap_or(slf.py().None());
        let py_exclude_channels =
            exclude_channels.unwrap_or(PyTuple::empty_bound(slf.py()).into_py(slf.py()));

        let new_command = Command::py_new(
            slf.py(),
            slf.as_ref().into_py(slf.py()),
            name,
            handler,
            permission,
            py_channels,
            py_exclude_channels,
            client_cmd_pass,
            client_cmd_perm,
            prefix,
            usage,
        )?;

        plugin.commands.write().push(new_command.clone());

        if let Some(ref commands) = *COMMANDS.load() {
            commands
                .borrow(slf.py())
                .add_command(slf.py(), new_command, priority as usize)?;
        }

        Ok(())
    }

    fn remove_command(&self, py: Python<'_>, name: PyObject, handler: PyObject) {
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

        self.commands.write().retain(|existing_command| {
            names
                .iter()
                .all(|name| existing_command.name.contains(name))
                && existing_command
                    .handler
                    .bind(py)
                    .ne(handler.bind(py))
                    .unwrap_or(true)
        });
    }

    /// Gets the value of a cvar as a string.
    #[classmethod]
    #[pyo3(signature = (name, return_type = None), text_signature = "(name, return_type=str)")]
    fn get_cvar(
        cls: &Bound<'_, PyType>,
        name: &str,
        return_type: Option<Py<PyType>>,
    ) -> PyResult<PyObject> {
        #[allow(clippy::question_mark)]
        let cvar = cls.py().allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return None;
            };
            main_engine.find_cvar(name)
        });

        let cvar_string = cvar.as_ref().map(|value| value.get_string());

        let Some(py_return_type) = return_type else {
            return Ok(cvar_string.into_py(cls.py()));
        };
        let py_return_type_str = py_return_type
            .bind(cls.py())
            .getattr(intern!(cls.py(), "__name__"))
            .and_then(|value| value.extract::<String>())
            .unwrap_or("Python type without __name__".into());

        match py_return_type_str.as_str() {
            "str" => match cvar_string {
                None => Ok(cls.py().None()),
                Some(value) => Ok(value.into_py(cls.py())),
            },
            "int" => match cvar_string {
                None => Ok(cls.py().None()),
                Some(value) => value
                    .parse::<i128>()
                    .map(|int| int.into_py(cls.py()))
                    .map_err(|_| {
                        let error_description =
                            format!("invalid literal for int() with base 10: '{}'", value);
                        PyValueError::new_err(error_description)
                    }),
            },
            "float" => match cvar_string {
                None => Ok(cls.py().None()),
                Some(value) => value
                    .parse::<f64>()
                    .map(|float| float.into_py(cls.py()))
                    .map_err(|_| {
                        let error_description =
                            format!("could not convert string to float: '{}'", value);
                        PyValueError::new_err(error_description)
                    }),
            },
            "bool" => match cvar_string {
                None => Ok(false.into_py(cls.py())),
                Some(value) => value
                    .parse::<i128>()
                    .map(|int| (int != 0).into_py(cls.py()))
                    .map_err(|_| {
                        let error_description =
                            format!("invalid literal for int() with base 10: '{}'", value);
                        PyValueError::new_err(error_description)
                    }),
            },
            "list" => match cvar_string {
                None => Ok(PyList::empty_bound(cls.py()).into_py(cls.py())),
                Some(value) => {
                    let items: Vec<&str> = value.split(',').collect();
                    let returned = PyList::new_bound(cls.py(), items);
                    Ok(returned.into_py(cls.py()))
                }
            },
            "set" => match cvar_string {
                None => PySet::empty_bound(cls.py()).map(|set| set.into_py(cls.py())),
                Some(value) => {
                    let items: Vec<String> =
                        value.split(',').map(|item| item.to_string()).collect();
                    let returned = PySet::new_bound::<String>(cls.py(), &items);
                    returned.map(|set| set.into_py(cls.py()))
                }
            },
            "tuple" => match cvar_string {
                None => Ok(PyTuple::empty_bound(cls.py()).into_py(cls.py())),
                Some(value) => {
                    let items: Vec<&str> = value.split(',').collect();
                    let returned = PyTuple::new_bound(cls.py(), items);
                    Ok(returned.into_py(cls.py()))
                }
            },
            value => {
                let error_description = format!("Invalid return type: {}", value);
                Err(PyValueError::new_err(error_description))
            }
        }
    }

    /// Sets a cvar. If the cvar exists, it will be set as if set from the console,
    /// otherwise create it.
    #[classmethod]
    #[pyo3(signature = (name, value, flags = 0), text_signature = "(name, value, flags = 0)")]
    fn set_cvar(
        cls: &Bound<'_, PyType>,
        name: &str,
        value: PyObject,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.bind(cls.py()).str()?.to_string();

        cls.py().allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err("could not get main_engine"));
            };
            let cvar = main_engine.find_cvar(name);

            if cvar.is_none() {
                main_engine.get_cvar(name, value_str.as_str(), Some(flags));
                Ok(true)
            } else {
                let console_cmd = format!(r#"{name} "{value_str}""#);
                main_engine.execute_console_command(console_cmd.as_str());
                Ok(false)
            }
        })
    }

    /// Sets a cvar with upper and lower limits. If the cvar exists, it will be set
    /// as if set from the console, otherwise create it.
    #[classmethod]
    #[pyo3(signature = (name, value, minimum, maximum, flags = 0), text_signature = "(name, value, minimum, maximum, flags = 0)")]
    fn set_cvar_limit(
        cls: &Bound<'_, PyType>,
        name: &str,
        value: PyObject,
        minimum: PyObject,
        maximum: PyObject,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.bind(cls.py()).str()?.to_string();
        let minimum_str = minimum.bind(cls.py()).str()?.to_string();
        let maximum_str = maximum.bind(cls.py()).str()?.to_string();

        cls.py().allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err("could not get main_engine"));
            };
            let cvar = main_engine.find_cvar(name);

            main_engine.set_cvar_limit(
                name,
                value_str.as_str(),
                minimum_str.as_str(),
                maximum_str.as_str(),
                Some(flags),
            );

            Ok(cvar.is_none())
        })
    }

    /// Sets a cvar. If the cvar exists, do nothing.
    #[classmethod]
    #[pyo3(signature = (name, value, flags = 0), text_signature = "(name, value, flags = 0)")]
    fn set_cvar_once(
        cls: &Bound<'_, PyType>,
        name: &str,
        value: PyObject,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.bind(cls.py()).str()?.to_string();

        cls.py().allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err("could not get main_engine"));
            };
            let cvar = main_engine.find_cvar(name);

            if cvar.is_none() {
                main_engine.get_cvar(name, value_str.as_str(), Some(flags));
            }
            Ok(cvar.is_none())
        })
    }

    /// Sets a cvar with upper and lower limits. If the cvar exists, not do anything.
    #[classmethod]
    #[pyo3(signature = (name, value, minimum, maximum, flags = 0), text_signature = "(name, value, minimum, maximum, flags = 0)")]
    fn set_cvar_limit_once(
        cls: &Bound<'_, PyType>,
        name: &str,
        value: PyObject,
        minimum: PyObject,
        maximum: PyObject,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.bind(cls.py()).str()?.to_string();
        let minimum_str = minimum.bind(cls.py()).str()?.to_string();
        let maximum_str = maximum.bind(cls.py()).str()?.to_string();

        cls.py().allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err("could not get main_engine"));
            };
            let cvar = main_engine.find_cvar(name);

            if cvar.is_none() {
                main_engine.set_cvar_limit(
                    name,
                    value_str.as_str(),
                    minimum_str.as_str(),
                    maximum_str.as_str(),
                    Some(flags),
                );
            }

            Ok(cvar.is_none())
        })
    }

    /// Get a list of all the players on the server.
    #[classmethod]
    fn players(cls: &Bound<'_, PyType>) -> PyResult<Vec<Player>> {
        Player::all_players(&cls.py().get_type_bound::<Player>(), cls.py())
    }

    /// Get a Player instance from the name, client ID,
    /// or Steam ID. Assumes [0, 64) to be a client ID
    /// and [64, inf) to be a Steam ID.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None), text_signature = "(name, player_list = None)")]
    fn player(
        cls: &Bound<'_, PyType>,
        name: PyObject,
        player_list: Option<Vec<Player>>,
    ) -> PyResult<Option<Player>> {
        if let Ok(player) = name.extract::<Player>(cls.py()) {
            return Ok(Some(player));
        }

        if let Ok(player_id) = name.extract::<i32>(cls.py()) {
            if (0..64).contains(&player_id) {
                return Player::py_new(player_id, None).map(Some);
            }
        }

        let players = player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());
        if let Ok(player_steam_id) = name.extract::<i64>(cls.py()) {
            return Ok(players
                .into_iter()
                .find(|player| player.steam_id == player_steam_id));
        }

        let Some(client_id) = client_id(cls.py(), name, Some(players.clone())) else {
            return Ok(None);
        };
        Ok(players.into_iter().find(|player| player.id == client_id))
    }

    /// Send a message to the chat, or any other channel.
    #[classmethod]
    #[pyo3(signature = (msg, chat_channel = None, **kwargs),
    text_signature = "(msg, chat_channel = \"chat\", **kwargs)")]
    fn msg(
        cls: &Bound<'_, PyType>,
        msg: &str,
        chat_channel: Option<PyObject>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let limit = kwargs.map_or(100i32, |pydict| {
            pydict
                .get_item("limit")
                .ok()
                .flatten()
                .and_then(|value| value.extract::<i32>().ok())
                .unwrap_or(100i32)
        });
        let delimiter = kwargs.map_or(" ".to_owned(), |pydict| {
            pydict
                .get_item("delimiter")
                .ok()
                .flatten()
                .and_then(|value| value.extract::<String>().ok())
                .unwrap_or(" ".to_owned())
        });

        match chat_channel {
            None => {
                if let Some(ref main_chat_channel) = *CHAT_CHANNEL.load() {
                    let main_channel = main_chat_channel.borrow(cls.py()).into_super();
                    ChatChannel::reply(main_channel, cls.py(), msg, limit, &delimiter)?;
                }
                return Ok(());
            }
            Some(channel) => {
                let bound_channel = channel.bind(cls.py());
                if bound_channel
                    .get_type()
                    .is_subclass(&cls.py().get_type_bound::<AbstractChannel>())
                    .unwrap_or(false)
                {
                    bound_channel.call_method(intern!(cls.py(), "reply"), (msg,), kwargs)?;
                    return Ok(());
                }

                for global_channel in [
                    &CHAT_CHANNEL,
                    &RED_TEAM_CHAT_CHANNEL,
                    &BLUE_TEAM_CHAT_CHANNEL,
                ] {
                    if let Some(ref global_chat_channel) = *global_channel.load() {
                        if global_chat_channel.bind(cls.py()).eq(bound_channel)? {
                            let main_channel = global_chat_channel.borrow(cls.py()).into_super();
                            ChatChannel::reply(main_channel, cls.py(), msg, limit, &delimiter)?;
                            return Ok(());
                        }
                    }
                }

                if let Some(ref console_channel) = *CONSOLE_CHANNEL.load() {
                    if console_channel.bind(cls.py()).eq(bound_channel)? {
                        ConsoleChannel::reply(
                            console_channel.get(),
                            cls.py(),
                            msg,
                            limit,
                            &delimiter,
                        )?;
                        return Ok(());
                    }
                }
            }
        }
        Err(PyValueError::new_err("Invalid channel."))
    }

    /// Prints text in the console.
    #[classmethod]
    fn console(cls: &Bound<'_, PyType>, text: PyObject) -> PyResult<()> {
        let extracted_text = text.bind(cls.py()).str()?.to_string();
        let printed_text = format!("{extracted_text}\n");
        cls.py().allow_threads(|| {
            shinqlx_com_printf(&printed_text);
            Ok(())
        })
    }

    /// Removes color tags from text.
    #[classmethod]
    fn clean_text(cls: &Bound<'_, PyType>, text: &str) -> String {
        cls.py().allow_threads(|| clean_text(&text))
    }

    /// Get the colored name of a decolored name.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None), text_signature = "(name, player_list = None)")]
    fn colored_name(
        cls: &Bound<'_, PyType>,
        name: PyObject,
        player_list: Option<Vec<Player>>,
    ) -> Option<String> {
        if let Ok(player) = name.extract::<Player>(cls.py()) {
            return Some(player.name.clone());
        }

        let Ok(searched_name) = name.bind(cls.py()).str().map(|value| value.to_string()) else {
            return None;
        };

        let players = player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());
        let clean_name = clean_text(&searched_name).to_lowercase();

        players
            .iter()
            .find(|&player| player.get_clean_name(cls.py()).to_lowercase() == clean_name)
            .map(|found_player| found_player.name.clone())
    }

    /// Get a player's client id from the name, client ID,
    /// Player instance, or Steam ID. Assumes [0, 64) to be
    /// a client ID and [64, inf) to be a Steam ID.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None), text_signature = "(name, player_list = None)")]
    fn client_id(
        cls: &Bound<'_, PyType>,
        name: PyObject,
        player_list: Option<Vec<Player>>,
    ) -> Option<i32> {
        client_id(cls.py(), name, player_list)
    }

    /// Find a player based on part of a players name.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None), text_signature = "(name, player_list = None)")]
    fn find_player(
        cls: &Bound<'_, PyType>,
        name: &str,
        player_list: Option<Vec<Player>>,
    ) -> Vec<Player> {
        let players = player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());

        cls.py().allow_threads(|| {
            if name.is_empty() {
                return players;
            }

            let cleaned_text = clean_text(&name).to_lowercase();
            players
                .into_iter()
                .filter(|player| {
                    clean_text(&player.name)
                        .to_lowercase()
                        .contains(&cleaned_text)
                })
                .collect()
        })
    }

    /// Get a dictionary with the teams as keys and players as values.
    #[classmethod]
    #[pyo3(signature = (player_list = None), text_signature = "(player_list = None)")]
    fn teams<'py>(
        cls: &Bound<'py, PyType>,
        player_list: Option<Vec<Player>>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let players = player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());

        let result = PyDict::new_bound(cls.py());

        let filtered_frees: Vec<PyObject> = players
            .clone()
            .into_iter()
            .filter(|player| player.get_team(cls.py()).is_ok_and(|team| team == "free"))
            .map(|player| player.into_py(cls.py()))
            .collect();
        result.set_item(intern!(cls.py(), "free"), filtered_frees)?;

        let filtered_reds: Vec<PyObject> = players
            .clone()
            .into_iter()
            .filter(|player| player.get_team(cls.py()).is_ok_and(|team| team == "red"))
            .map(|player| player.into_py(cls.py()))
            .collect();
        result.set_item(intern!(cls.py(), "red"), filtered_reds)?;

        let filtered_blues: Vec<PyObject> = players
            .clone()
            .into_iter()
            .filter(|player| player.get_team(cls.py()).is_ok_and(|team| team == "blue"))
            .map(|player| player.into_py(cls.py()))
            .collect();
        result.set_item(intern!(cls.py(), "blue"), filtered_blues)?;

        let filtered_specs: Vec<PyObject> = players
            .clone()
            .into_iter()
            .filter(|player| {
                player
                    .get_team(cls.py())
                    .is_ok_and(|team| team == "spectator")
            })
            .map(|player| player.into_py(cls.py()))
            .collect();
        result.set_item(intern!(cls.py(), "spectator"), filtered_specs)?;

        Ok(result)
    }

    #[classmethod]
    #[pyo3(signature = (msg, recipient = None), text_signature = "(msg, recipient = None)")]
    fn center_print(
        cls: &Bound<'_, PyType>,
        msg: &str,
        recipient: Option<PyObject>,
    ) -> PyResult<()> {
        let client_id = recipient.and_then(|recipient| client_id(cls.py(), recipient, None));

        let center_printed_cmd = format!(r#"cp "{msg}""#);
        pyshinqlx_send_server_command(cls.py(), client_id, &center_printed_cmd)?;

        Ok(())
    }

    /// Send a tell (private message) to someone.
    #[classmethod]
    #[pyo3(signature = (msg, recipient, **kwargs))]
    fn tell(
        cls: &Bound<'_, PyType>,
        msg: &str,
        recipient: PyObject,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let Some(recipient_client_id) = client_id(cls.py(), recipient, None) else {
            return Err(PyValueError::new_err("could not find recipient"));
        };
        let recipient_player = Player::py_new(recipient_client_id, None)?;
        let tell_channel = TellChannel::py_new(&recipient_player);

        let tell_channel_py = Py::new(cls.py(), tell_channel)?;
        tell_channel_py
            .bind(cls.py())
            .call_method(intern!(cls.py(), "reply"), (msg,), kwargs)?;

        Ok(())
    }

    #[classmethod]
    fn is_vote_active(cls: &Bound<'_, PyType>) -> bool {
        cls.py().allow_threads(|| {
            MAIN_ENGINE
                .load()
                .as_ref()
                .map(|main_engine| {
                    !main_engine
                        .get_configstring(CS_VOTE_STRING as u16)
                        .is_empty()
                })
                .unwrap_or(false)
        })
    }

    #[classmethod]
    fn current_vote_count(cls: &Bound<'_, PyType>) -> PyResult<PyObject> {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Ok(cls.py().None());
        };

        let yes_votes = main_engine.get_configstring(CS_VOTE_YES as u16);
        let no_votes = main_engine.get_configstring(CS_VOTE_NO as u16);

        if yes_votes.is_empty() || no_votes.is_empty() {
            return Ok(cls.py().None());
        }

        let Ok(parsed_yes_votes) = yes_votes.parse::<i32>() else {
            let error_msg = format!("invalid literal for int() with base 10: '{}'", yes_votes);
            return Err(PyValueError::new_err(error_msg));
        };
        let Ok(parsed_no_votes) = no_votes.parse::<i32>() else {
            let error_msg = format!("invalid literal for int() with base 10: '{}'", no_votes);
            return Err(PyValueError::new_err(error_msg));
        };

        Ok((parsed_yes_votes, parsed_no_votes).into_py(cls.py()))
    }

    #[classmethod]
    #[pyo3(signature = (vote, display, time = 30), text_signature = "(vote, display, time = 30)")]
    fn callvote(cls: &Bound<'_, PyType>, vote: &str, display: &str, time: i32) -> PyResult<bool> {
        if Self::is_vote_active(cls) {
            return Ok(false);
        }

        let Some(vote_started_dispatcher) =
            EVENT_DISPATCHERS
                .load()
                .as_ref()
                .and_then(|event_dispatchers| {
                    event_dispatchers
                        .bind(cls.py())
                        .get_item(intern!(cls.py(), "vote_started"))
                        .ok()
                })
        else {
            return Err(PyEnvironmentError::new_err(
                "could not get access to vote started dispatcher",
            ));
        };
        vote_started_dispatcher.call_method1(intern!(cls.py(), "caller"), (cls.py().None(),))?;

        pyshinqlx_callvote(cls.py(), vote, display, Some(time));

        Ok(true)
    }

    #[classmethod]
    fn force_vote(cls: &Bound<'_, PyType>, pass_it: PyObject) -> PyResult<bool> {
        pass_it
            .downcast_bound::<PyBool>(cls.py())
            .map_err(|_| PyValueError::new_err("pass_it must be either True or False."))
            .and_then(|vote_passed| pyshinqlx_force_vote(cls.py(), vote_passed.is_true()))
    }

    #[classmethod]
    fn teamsize(cls: &Bound<'_, PyType>, size: i32) -> PyResult<()> {
        set_teamsize(cls.py(), size)
    }

    #[classmethod]
    #[pyo3(signature = (player, reason = ""), text_signature = "(player, reason = \"\")")]
    fn kick(cls: &Bound<'_, PyType>, player: PyObject, reason: &str) -> PyResult<()> {
        let Some(client_id) = client_id(cls.py(), player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let forwarded_reason = if reason.is_empty() {
            None
        } else {
            Some(reason)
        };

        pyshinqlx_kick(cls.py(), client_id, forwarded_reason)?;

        Ok(())
    }

    #[classmethod]
    fn shuffle(cls: &Bound<'_, PyType>) -> PyResult<()> {
        pyshinqlx_console_command(cls.py(), "forceshuffle")
    }

    #[classmethod]
    fn cointoss(_cls: &Bound<'_, PyType>) {}

    #[classmethod]
    #[pyo3(signature = (new_map, factory = None), text_signature = "(new_map, factory = None)")]
    fn change_map(cls: &Bound<'_, PyType>, new_map: &str, factory: Option<&str>) -> PyResult<()> {
        let mapchange_command = match factory {
            None => format!("map {}", new_map),
            Some(game_factory) => format!("map {} {}", new_map, game_factory),
        };
        pyshinqlx_console_command(cls.py(), &mapchange_command)
    }

    #[classmethod]
    fn switch(cls: &Bound<'_, PyType>, player: PyObject, other_player: PyObject) -> PyResult<()> {
        let Some(player1) = Self::player(cls, player, None)? else {
            return Err(PyValueError::new_err("The first player is invalid."));
        };
        let Some(player2) = Self::player(cls, other_player, None)? else {
            return Err(PyValueError::new_err("The second player is invalid."));
        };

        let team1 = player1.get_team(cls.py())?;
        let team2 = player2.get_team(cls.py())?;

        if team1 == team2 {
            return Err(PyValueError::new_err("Both player are on the same team."));
        }

        player1.put(cls.py(), &team2)?;
        player2.put(cls.py(), &team1)?;

        Ok(())
    }

    #[classmethod]
    #[pyo3(signature = (sound_path, player = None), text_signature = "(sound_path, player = None)")]
    fn play_sound(
        cls: &Bound<'_, PyType>,
        sound_path: &str,
        player: Option<Player>,
    ) -> PyResult<bool> {
        if sound_path.is_empty() || sound_path.contains("music/") {
            return Ok(false);
        }

        let play_sound_cmd = format!("playSound {sound_path}");
        pyshinqlx_send_server_command(cls.py(), player.map(|player| player.id), &play_sound_cmd)?;

        Ok(true)
    }

    #[classmethod]
    #[pyo3(signature = (music_path, player = None), text_signature = "(music_path, player = None)")]
    fn play_music(
        cls: &Bound<'_, PyType>,
        music_path: &str,
        player: Option<Player>,
    ) -> PyResult<bool> {
        if music_path.is_empty() || music_path.contains("sound/") {
            return Ok(false);
        }

        let play_music_cmd = format!("playMusic {music_path}");
        pyshinqlx_send_server_command(cls.py(), player.map(|player| player.id), &play_music_cmd)?;

        Ok(true)
    }

    #[classmethod]
    #[pyo3(signature = (player = None), text_signature = "(player = None)")]
    fn stop_sound(cls: &Bound<'_, PyType>, player: Option<Player>) -> PyResult<()> {
        pyshinqlx_send_server_command(cls.py(), player.map(|player| player.id), "clearSounds")?;

        Ok(())
    }

    #[classmethod]
    #[pyo3(signature = (player = None), text_signature = "(player = None)")]
    fn stop_music(cls: &Bound<'_, PyType>, player: Option<Player>) -> PyResult<()> {
        pyshinqlx_send_server_command(cls.py(), player.map(|player| player.id), "stopMusic")?;

        Ok(())
    }

    #[classmethod]
    #[pyo3(signature = (player, damage = 0), text_signature = "(player, damage = 0)")]
    fn slap(cls: &Bound<'_, PyType>, player: PyObject, damage: i32) -> PyResult<()> {
        let Some(client_id) = client_id(cls.py(), player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let slap_cmd = format!("slap {client_id} {damage}");
        pyshinqlx_console_command(cls.py(), &slap_cmd)?;

        Ok(())
    }

    #[classmethod]
    fn slay(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        let Some(client_id) = client_id(cls.py(), player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let slay_cmd = format!("slay {client_id}");
        pyshinqlx_console_command(cls.py(), &slay_cmd)?;

        Ok(())
    }

    #[classmethod]
    fn timeout(cls: &Bound<'_, PyType>) -> PyResult<()> {
        pyshinqlx_console_command(cls.py(), "timeout")
    }

    #[classmethod]
    fn timein(cls: &Bound<'_, PyType>) -> PyResult<()> {
        pyshinqlx_console_command(cls.py(), "timein")
    }

    #[classmethod]
    fn allready(cls: &Bound<'_, PyType>) -> PyResult<()> {
        pyshinqlx_console_command(cls.py(), "allready")
    }

    #[classmethod]
    fn pause(cls: &Bound<'_, PyType>) -> PyResult<()> {
        pyshinqlx_console_command(cls.py(), "pause")
    }

    #[classmethod]
    fn unpause(cls: &Bound<'_, PyType>) -> PyResult<()> {
        pyshinqlx_console_command(cls.py(), "unpause")
    }

    #[classmethod]
    #[pyo3(signature = (team = None), text_signature = "(team = None)")]
    fn lock(cls: &Bound<'_, PyType>, team: Option<&str>) -> PyResult<()> {
        lock(cls.py(), team)
    }

    #[classmethod]
    #[pyo3(signature = (team = None), text_signature = "(team = None)")]
    fn unlock(cls: &Bound<'_, PyType>, team: Option<&str>) -> PyResult<()> {
        unlock(cls.py(), team)
    }

    #[classmethod]
    fn put(cls: &Bound<'_, PyType>, player: PyObject, team: &str) -> PyResult<()> {
        put(cls.py(), player, team)
    }

    #[classmethod]
    fn mute(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        mute(cls.py(), player)
    }

    #[classmethod]
    fn unmute(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        unmute(cls.py(), player)
    }

    #[classmethod]
    fn tempban(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        tempban(cls.py(), player)
    }

    #[classmethod]
    fn ban(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        ban(cls.py(), player)
    }

    #[classmethod]
    fn unban(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        unban(cls.py(), player)
    }

    #[classmethod]
    fn opsay(cls: &Bound<'_, PyType>, msg: &str) -> PyResult<()> {
        opsay(cls.py(), msg)
    }

    #[classmethod]
    fn addadmin(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        addadmin(cls.py(), player)
    }

    #[classmethod]
    fn addmod(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        addmod(cls.py(), player)
    }

    #[classmethod]
    fn demote(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        demote(cls.py(), player)
    }

    #[classmethod]
    fn abort(cls: &Bound<'_, PyType>) -> PyResult<()> {
        pyshinqlx_console_command(cls.py(), "map_restart")
    }

    #[classmethod]
    fn addscore(cls: &Bound<'_, PyType>, player: PyObject, score: i32) -> PyResult<()> {
        addscore(cls.py(), player, score)
    }

    #[classmethod]
    fn addteamscore(cls: &Bound<'_, PyType>, team: &str, score: i32) -> PyResult<()> {
        addteamscore(cls.py(), team, score)
    }

    #[classmethod]
    fn setmatchtime(cls: &Bound<'_, PyType>, time: i32) -> PyResult<()> {
        let setmatchtime_cmd = format!("setmatchtime {}", time);
        pyshinqlx_console_command(cls.py(), &setmatchtime_cmd)
    }
}

#[cfg(test)]
mod plugin_tests {
    use super::MAIN_ENGINE;
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use crate::ffi::python::{
        pyshinqlx_test_support::run_all_frame_tasks, BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL,
        CONSOLE_CHANNEL, EVENT_DISPATCHERS, RED_TEAM_CHAT_CHANNEL,
    };
    use crate::hooks::mock_hooks::{
        shinqlx_com_printf_context, shinqlx_drop_client_context,
        shinqlx_send_server_command_context,
    };

    use alloc::ffi::CString;
    use core::ffi::c_char;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::{
        exceptions::{PyEnvironmentError, PyRuntimeError, PyValueError},
        types::{
            IntoPyDict, PyBool, PyDate, PyDict, PyFloat, PyInt, PyList, PySet, PyString, PyTuple,
        },
    };
    use rstest::rstest;

    fn default_test_player_info() -> PlayerInfo {
        PlayerInfo {
            client_id: 0,
            name: "Mocked Player".to_string(),
            connection_state: clientState_t::CS_ACTIVE as i32,
            userinfo: "asdf".to_string(),
            steam_id: 1234,
            team: team_t::TEAM_RED as i32,
            privileges: 0,
        }
    }

    fn default_test_player() -> Player {
        Player {
            valid: true,
            id: 0,
            player_info: default_test_player_info(),
            name: "Mocked Player".to_string(),
            steam_id: 1234,
            user_info: "asdf".to_string(),
        }
    }

    fn test_plugin(py: Python) -> Result<Bound<PyAny>, PyErr> {
        let extended_plugin = PyModule::from_code_bound(
            py,
            r#"
from shinqlx import Plugin


class subplugin(Plugin):
    def __init__(self):
        super().__init__()
"#,
            "",
            "",
        )?
        .getattr("subplugin")?;
        Ok(extended_plugin)
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn plugin_can_be_subclassed_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let extended_plugin = test_plugin(py)?;
            extended_plugin.call0()?;
            Ok::<(), PyErr>(())
        })
        .unwrap();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn str_returns_plugin_typename(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let extended_plugin = test_plugin(py)?;
            let plugin_instance = extended_plugin.call0()?;
            let plugin_str = plugin_instance.str()?;
            assert_eq!(plugin_str.to_string(), "subplugin");
            Ok::<(), PyErr>(())
        })
        .unwrap();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_db_when_no_db_type_set(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            py.get_type_bound::<Plugin>().delattr("database")?;
            let extended_plugin = test_plugin(py)?;
            let plugin_instance = extended_plugin.call0()?;
            let result = plugin_instance.getattr("db");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyRuntimeError>(py)),);
            Ok::<(), PyErr>(())
        })
        .unwrap();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_db_when_no_db_set(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            py.get_type_bound::<Plugin>()
                .setattr("database", py.None())?;
            let extended_plugin = test_plugin(py)?;
            let plugin_instance = extended_plugin.call0()?;
            let result = plugin_instance.getattr("db");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyRuntimeError>(py)),);
            Ok::<(), PyErr>(())
        })
        .unwrap();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_db_when_db_set_to_redis(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let redis_type = py.get_type_bound::<Redis>();
            py.get_type_bound::<Plugin>()
                .setattr("database", (&redis_type).into_py(py))?;

            let extended_plugin = test_plugin(py)?;
            let plugin_instance = extended_plugin.call0()?;
            let result = plugin_instance.getattr("db");
            assert!(result.is_ok_and(|db| db.is_instance(&redis_type).unwrap()),);
            Ok::<(), PyErr>(())
        })
        .unwrap();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn name_property_returns_plugin_typename(_pyshinqlx_setup: ()) {
        let res: PyResult<()> = Python::with_gil(|py| {
            let extended_plugin = test_plugin(py)?;
            let plugin_instance = extended_plugin.call0()?;
            let plugin_str = plugin_instance.getattr("name")?;
            assert_eq!(plugin_str.extract::<&str>()?, "subplugin");
            Ok(())
        });
        assert!(res.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn plugins_property_returns_loaded_plugins(_pyshinqlx_setup: ()) {
        let res: PyResult<()> = Python::with_gil(|py| {
            let loaded_plugins = py
                .get_type_bound::<Plugin>()
                .getattr("_loaded_plugins")?
                .extract::<&PyDict>()?;
            loaded_plugins.set_item("asdf", "asdfplugin")?;
            loaded_plugins.set_item("qwertz", "qwertzplugin")?;

            let extended_plugin = test_plugin(py)?;
            let plugin_instance = extended_plugin.call0()?;
            let plugins = plugin_instance.getattr("plugins")?.extract::<&PyDict>()?;
            assert!(plugins.eq(loaded_plugins)?);
            Ok(())
        });
        assert!(res.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn hooks_property_returns_plugin_hooks(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let plugin = Plugin {
                hooks: vec![
                    ("asdf".to_string(), py.None(), 1),
                    ("qwertz".to_string(), py.None(), 0),
                ],
                commands: Default::default(),
                db_instance: py.None(),
            };

            let hooks = plugin.get_hooks(py);
            assert_eq!(hooks.len(), 2);
            let elem1 = hooks.first();
            assert!(elem1.is_some_and(|(hook1, pyobj1, prio1)| hook1 == "asdf"
                && pyobj1.is_none(py)
                && prio1 == &1));
            let elem2 = hooks.get(1);
            assert!(elem2.is_some_and(|(hook2, pyobj2, prio2)| hook2 == "qwertz"
                && pyobj2.is_none(py)
                && prio2 == &0));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn commands_property_when_no_commands_exist(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let plugin = Plugin {
                hooks: Default::default(),
                commands: Default::default(),
                db_instance: py.None(),
            };

            assert_eq!(plugin.get_commands(py).len(), 0);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_property_when_no_game_running(_pyshinqlx_setup: ()) {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let plugin = Plugin {
                hooks: Default::default(),
                commands: Default::default(),
                db_instance: py.None(),
            };

            assert!(plugin.get_game(py).is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_property_when_a_game_exists(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "asdf".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let plugin = Plugin {
                hooks: Default::default(),
                commands: Default::default(),
                db_instance: py.None(),
            };

            assert!(plugin.get_game(py).is_some());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_logger(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let logger_type = py
                .import_bound("logging")?
                .getattr("Logger")
                .expect("could not get logging.Logger");

            let extended_plugin = test_plugin(py)?;
            let plugin_instance = extended_plugin.call0()?;
            let result = plugin_instance.getattr("logger");
            assert!(result
                .as_ref()
                .is_ok_and(|logger| logger.is_instance(&logger_type).unwrap()
                    && logger
                        .getattr("name")
                        .expect("could not get logger name")
                        .str()
                        .unwrap()
                        .to_string()
                        == "shinqlx.subplugin"),);
            Ok::<(), PyErr>(())
        })
        .expect("python result was not ok.");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = Plugin::get_cvar(&py.get_type_bound::<Plugin>(), "sv_maxclients", None);
            assert!(result.is_ok_and(|value| value.is_none(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_cvar_not_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("asdf"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::get_cvar(&py.get_type_bound::<Plugin>(), "asdf", None);
            assert!(result.expect("result was not OK").is_none(py));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_cvar_is_found() {
        let cvar_string = CString::new("16").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::get_cvar(&py.get_type_bound::<Plugin>(), "sv_maxclients", None);
            assert!(result
                .expect("result was not OK")
                .extract::<String>(py)
                .is_ok_and(|value| value == "16"));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_str() {
        let cvar_string = CString::new("16").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyString>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );
            assert!(result
                .expect("result was not OK")
                .extract::<String>(py)
                .is_ok_and(|value| value == "16"));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_str_when_no_cvar_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyString>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );
            assert!(result.expect("result was not OK").is_none(py));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_int() {
        let cvar_string = CString::new("16").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyInt>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );
            assert!(result
                .expect("result was not OK")
                .extract::<i32>(py)
                .is_ok_and(|value| value == 16));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_int_when_cvar_cannot_be_converted_to_int() {
        let cvar_string = CString::new("asdf").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyInt>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_int_when_no_cvar_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyInt>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );
            assert!(result.expect("result was not OK").is_none(py));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_float() {
        let cvar_string = CString::new("16").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyFloat>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );
            assert!(result
                .expect("result was not OK")
                .extract::<f64>(py)
                .is_ok_and(|value| value == 16.0));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_float_when_cvar_cannot_be_converted_to_float() {
        let cvar_string = CString::new("asdf").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyFloat>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_float_when_no_cvar_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyFloat>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );
            assert!(result.expect("result was not OK").is_none(py));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_bool() {
        let cvar_string = CString::new("16").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyBool>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            )
            .expect("result was not OK");
            assert!(
                result.bind(py).is_instance_of::<PyBool>()
                    && result.extract::<bool>(py).is_ok_and(|value| value)
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_bool_when_no_cvar_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyBool>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            )
            .expect("result was not OK");

            assert!(
                result.bind(py).is_instance_of::<PyBool>()
                    && result.extract::<bool>(py).is_ok_and(|value| !value)
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_bool_when_cvar_cannot_be_converted_to_int() {
        let cvar_string = CString::new("asdf").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyBool>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_list() {
        let cvar_string = CString::new("2, 4, 6, 8, 10").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyList>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            )
            .expect("result was not OK");
            assert!(
                result.bind(py).is_instance_of::<PyList>()
                    && result
                        .downcast_bound::<PyList>(py)
                        .is_ok_and(|value| value.len() == 5)
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_list_when_no_cvar_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyList>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            )
            .expect("result was not OK");

            assert!(
                result.bind(py).is_instance_of::<PyList>()
                    && result
                        .downcast_bound::<PyList>(py)
                        .is_ok_and(|value| value.is_empty())
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_set() {
        let cvar_string = CString::new("2, 4, 6, 8, 10").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PySet>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            )
            .expect("result was not OK");
            assert!(
                result.bind(py).is_instance_of::<PySet>()
                    && result
                        .downcast_bound::<PySet>(py)
                        .is_ok_and(|value| value.len() == 5)
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_set_when_no_cvar_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PySet>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            )
            .expect("result was not OK");

            assert!(
                result.bind(py).is_instance_of::<PySet>()
                    && result
                        .downcast_bound::<PySet>(py)
                        .is_ok_and(|value| value.is_empty())
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_tuple() {
        let cvar_string = CString::new("2, 4, 6, 8, 10").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyTuple>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            )
            .expect("result was not OK");
            assert!(
                result.bind(py).is_instance_of::<PyTuple>()
                    && result
                        .downcast_bound::<PyTuple>(py)
                        .is_ok_and(|value| value.len() == 5)
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_tuple_when_no_cvar_found() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyTuple>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            )
            .expect("result was not OK");

            assert!(
                result.bind(py).is_instance_of::<PyTuple>()
                    && result
                        .downcast_bound::<PyTuple>(py)
                        .is_ok_and(|value| value.is_empty())
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_with_invalid_type_conversion() {
        let cvar_string = CString::new("16").expect("result was not OK");
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(move |_| {
                let mut raw_cvar = CVarBuilder::default()
                    .string(cvar_string.as_ptr() as *mut c_char)
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let py_str_type = py.get_type_bound::<PyDate>();
            let result = Plugin::get_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                Some(py_str_type.unbind()),
            );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = Plugin::set_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                "64".into_py(py),
                0,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_for_not_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        mock_engine
            .expect_get_cvar()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::set_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                "64".into_py(py),
                cvar_flags::CVAR_ROM as i32,
            )
        });
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_for_already_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default()
                    .build()
                    .expect("this should not happen");
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq(r#"sv_maxclients "64""#))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::set_cvar(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                "64".into_py(py),
                cvar_flags::CVAR_ROM as i32,
            )
        });
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = Plugin::set_cvar_limit(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                64i32.into_py(py),
                1i32.into_py(py),
                64i32.into_py(py),
                0,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_forwards_parameters_to_main_engine_call() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .times(1);
        mock_engine
            .expect_set_cvar_limit()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq("1"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_CHEAT as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::set_cvar_limit(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                64i32.into_py(py),
                1i32.into_py(py),
                64i32.into_py(py),
                cvar_flags::CVAR_CHEAT as i32,
            )
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_once_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = Plugin::set_cvar_once(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                "64".into_py(py),
                0,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_once_for_not_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None)
            .times(1);
        mock_engine
            .expect_get_cvar()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::set_cvar_once(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                64i32.into_py(py),
                cvar_flags::CVAR_ROM as i32,
            )
        })
        .unwrap();
        assert_eq!(result, true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_once_for_already_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default().build().unwrap();
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        mock_engine.expect_get_cvar().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::set_cvar_once(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                "64".into_py(py),
                cvar_flags::CVAR_ROM as i32,
            )
        })
        .unwrap();
        assert_eq!(result, false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_once_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result = Plugin::set_cvar_limit_once(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                "64".into_py(py),
                "1".into_py(py),
                "64".into_py(py),
                0,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_once_when_no_previous_value_is_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| None);
        mock_engine
            .expect_set_cvar_limit()
            .with(
                predicate::eq("sv_maxclients"),
                predicate::eq("64"),
                predicate::eq("1"),
                predicate::eq("64"),
                predicate::eq(Some(cvar_flags::CVAR_CHEAT as i32)),
            )
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::set_cvar_limit_once(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                "64".into_py(py),
                "1".into_py(py),
                "64".into_py(py),
                cvar_flags::CVAR_CHEAT as i32,
            )
        });
        assert!(result.is_ok_and(|value| value));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_once_for_already_existing_cvar() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .returning(|_| {
                let mut raw_cvar = CVarBuilder::default().build().unwrap();
                CVar::try_from(&mut raw_cvar as *mut cvar_t).ok()
            })
            .times(1);
        mock_engine.expect_set_cvar_limit().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::set_cvar_limit_once(
                &py.get_type_bound::<Plugin>(),
                "sv_maxclients",
                "64".into_py(py),
                "1".into_py(py),
                "64".into_py(py),
                cvar_flags::CVAR_ROM as i32,
            )
        })
        .unwrap();
        assert_eq!(result, false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn all_players_for_existing_clients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 3);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
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

        client_try_from_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_FREE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| "asdf".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
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
        game_entity_try_from_ctx.expect().returning(|_client_id| {
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

        let all_players = Python::with_gil(|py| Plugin::players(&py.get_type_bound::<Plugin>()));
        assert_eq!(
            all_players.expect("result was not ok"),
            vec![
                default_test_player(),
                Player {
                    id: 2,
                    player_info: PlayerInfo {
                        client_id: 2,
                        ..default_test_player_info()
                    },
                    ..default_test_player()
                },
            ]
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_player() {
        Python::with_gil(|py| {
            let result = Plugin::player(
                &py.get_type_bound::<Plugin>(),
                default_test_player().clone().into_py(py),
                None,
            );
            assert!(result
                .expect("result was not ok")
                .is_some_and(|result_player| default_test_player() == result_player));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_for_player_id() {
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
            let result = Plugin::player(&py.get_type_bound::<Plugin>(), 42i32.into_py(py), None);
            assert!(result
                .expect("result was not ok")
                .is_some_and(|player| player
                    == Player {
                        id: 42,
                        player_info: PlayerInfo {
                            client_id: 42,
                            ..default_test_player_info()
                        },
                        ..default_test_player()
                    },),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_steam_id_from_player_list() {
        Python::with_gil(|py| {
            let result = Plugin::player(
                &py.get_type_bound::<Plugin>(),
                1234.into_py(py),
                Some(vec![default_test_player()]),
            );
            assert!(result
                .expect("result was not ok")
                .is_some_and(|result_player| result_player == default_test_player()));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_steam_id_not_in_provided_player_list() {
        Python::with_gil(|py| {
            let result = Plugin::player(
                &py.get_type_bound::<Plugin>(),
                4321.into_py(py),
                Some(vec![default_test_player()]),
            );
            assert!(result.expect("result was not ok").is_none());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_name_from_player_list() {
        Python::with_gil(|py| {
            let result = Plugin::player(
                &py.get_type_bound::<Plugin>(),
                "Mocked Player".into_py(py),
                Some(vec![default_test_player()]),
            );
            assert!(result
                .expect("result was not ok")
                .is_some_and(|result_player| result_player == default_test_player()));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_name_not_in_provided_player_list() {
        Python::with_gil(|py| {
            let result = Plugin::player(
                &py.get_type_bound::<Plugin>(),
                "disconnected".into_py(py),
                Some(vec![default_test_player()]),
            );
            assert!(result.expect("result was not ok").is_none());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn msg_for_invalid_channel() {
        Python::with_gil(|py| {
            let result = Plugin::msg(
                &py.get_type_bound::<Plugin>(),
                "asdf",
                Some("asdf".into_py(py)),
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_default_channel() {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "print \"asdf\n\"\n")
            .times(1);

        Python::with_gil(|py| {
            CHAT_CHANNEL.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                )
                .expect("creating new chat channel failed.")
                .into(),
            ));

            let result = Plugin::msg(&py.get_type_bound::<Plugin>(), "asdf", None, None);
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_chat_channel_with_kwargs() {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "print \"asdf qwertz\n\"\n")
            .times(1);

        Python::with_gil(|py| {
            CHAT_CHANNEL.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
                )
                .expect("creating new chat channel failed.")
                .into(),
            ));

            let result = Plugin::msg(
                &py.get_type_bound::<Plugin>(),
                "asdf qwertz",
                Some("chat".into_py(py)),
                Some(
                    &[("limit", 23i32.into_py(py)), ("delimiter", "_".into_py(py))]
                        .into_py_dict_bound(py),
                ),
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_red_team_chat_channel() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 8);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .withf(|client_id| (0..8).contains(client_id))
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
            .withf(|client_id| (0..8).contains(client_id))
            .returning(|client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(move || match client_id {
                        0 => team_t::TEAM_FREE,
                        1 => team_t::TEAM_RED,
                        2 => team_t::TEAM_BLUE,
                        4 => team_t::TEAM_FREE,
                        5 => team_t::TEAM_RED,
                        6 => team_t::TEAM_BLUE,
                        _ => team_t::TEAM_SPECTATOR,
                    });
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "print \"asdf qwertz\n\"\n")
            .times(2);

        Python::with_gil(|py| {
            RED_TEAM_CHAT_CHANNEL.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new("red", "red_team_chat", "print \"{}\n\"\n"),
                )
                .expect("creating new chat channel failed.")
                .into(),
            ));

            let result = Plugin::msg(
                &py.get_type_bound::<Plugin>(),
                "asdf qwertz",
                Some("red_team_chat".into_py(py)),
                None,
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_blue_team_chat_channel() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 8);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .withf(|client_id| (0..8).contains(client_id))
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
            .withf(|client_id| (0..8).contains(client_id))
            .returning(|client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(move || match client_id {
                        0 => team_t::TEAM_FREE,
                        1 => team_t::TEAM_RED,
                        2 => team_t::TEAM_BLUE,
                        4 => team_t::TEAM_FREE,
                        5 => team_t::TEAM_RED,
                        6 => team_t::TEAM_BLUE,
                        _ => team_t::TEAM_SPECTATOR,
                    });
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "print \"asdf qwertz\n\"\n")
            .times(2);

        Python::with_gil(|py| {
            BLUE_TEAM_CHAT_CHANNEL.store(Some(
                Py::new(
                    py,
                    TeamChatChannel::py_new("blue", "blue_team_chat", "print \"{}\n\"\n"),
                )
                .expect("creating new chat channel failed.")
                .into(),
            ));

            let result = Plugin::msg(
                &py.get_type_bound::<Plugin>(),
                "asdf qwertz",
                Some("blue_team_chat".into_py(py)),
                None,
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_console_channel() {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .withf(|msg| msg == "asdf\n")
            .times(1);

        Python::with_gil(|py| {
            CONSOLE_CHANNEL.store(Some(
                Py::new(py, ConsoleChannel::py_new())
                    .expect("creating new console channel failed.")
                    .into(),
            ));

            let result = Plugin::msg(
                &py.get_type_bound::<Plugin>(),
                "asdf",
                Some("console".into_py(py)),
                None,
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_provided_channel() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
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

        let channel = TellChannel::py_new(&default_test_player());

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "print \"asdf qwertz\n\"\n")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::msg(
                &py.get_type_bound::<Plugin>(),
                "asdf qwertz",
                Some(
                    Py::new(py, channel)
                        .expect("could not create tell channel")
                        .into_py(py),
                ),
                None,
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_prints_to_console() {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .with(predicate::eq("asdf\n"))
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::console(&py.get_type_bound::<Plugin>(), "asdf".into_py(py));
            assert!(result.as_ref().is_ok(), "{:?}", result.as_ref());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn clean_text_cleans_text_from_color_tags() {
        Python::with_gil(|py| {
            let result = Plugin::clean_text(
                &py.get_type_bound::<Plugin>(),
                "^0a^1b^2c^3d^4e^5f^6g^7h^8i^9j",
            );
            assert_eq!(result, "abcdefgh^8i^9j");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn colored_name_for_provided_player() {
        Python::with_gil(|py| {
            let result = Plugin::colored_name(
                &py.get_type_bound::<Plugin>(),
                default_test_player().into_py(py),
                None,
            );
            assert_eq!(result.expect("result was none"), "Mocked Player");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn colored_name_for_player_in_provided_playerlist() {
        Python::with_gil(|py| {
            let result = Plugin::colored_name(
                &py.get_type_bound::<Plugin>(),
                "Mocked Player".into_py(py),
                Some(vec![default_test_player()]),
            );
            assert_eq!(result.expect("result was none"), "Mocked Player");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn colored_name_for_player_with_colored_name_in_provided_playerlist() {
        let player = Player {
            player_info: PlayerInfo {
                name: "^1Mocked ^4Player".to_string(),
                ..default_test_player_info()
            },
            name: "^1Mocked ^4Player".to_string(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::colored_name(
                &py.get_type_bound::<Plugin>(),
                "Mocked Player".into_py(py),
                Some(vec![player]),
            );
            assert_eq!(result.expect("result was none"), "^1Mocked ^4Player");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn colored_name_for_unavailable_player() {
        let player = Player {
            player_info: PlayerInfo {
                name: "^1Mocked ^4Player".to_string(),
                ..default_test_player_info()
            },
            name: "^1Mocked ^4Player".to_string(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::colored_name(
                &py.get_type_bound::<Plugin>(),
                "disconnected Player".into_py(py),
                Some(vec![player]),
            );
            assert!(result.is_none());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_integer_in_client_id_range() {
        Python::with_gil(|py| {
            let result = Plugin::client_id(&py.get_type_bound::<Plugin>(), 42i32.into_py(py), None);
            assert!(result.is_some_and(|value| value == 42));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_player() {
        let player = Player {
            id: 21,
            player_info: PlayerInfo {
                client_id: 21,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result =
                Plugin::client_id(&py.get_type_bound::<Plugin>(), player.into_py(py), None);
            assert!(result.is_some_and(|value| value == 21));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_steam_id() {
        let player = Player {
            id: 21,
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 21,
                steam_id: 1234,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type_bound::<Plugin>(),
                1234i64.into_py(py),
                Some(vec![player]),
            );
            assert!(result.is_some_and(|value| value == 21));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_steam_id_not_in_player_list() {
        let player = Player {
            steam_id: 1234,
            player_info: PlayerInfo {
                steam_id: 1234,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type_bound::<Plugin>(),
                4321i64.into_py(py),
                Some(vec![player]),
            );
            assert!(result.is_none());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_player_name() {
        let player = Player {
            id: 21,
            name: "Mocked Player".to_string(),
            player_info: PlayerInfo {
                client_id: 21,
                name: "Mocked Player".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type_bound::<Plugin>(),
                "Mocked Player".into_py(py),
                Some(vec![player]),
            );
            assert!(result.is_some_and(|value| value == 21));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_player_name_not_in_player_list() {
        let player = Player {
            player_info: PlayerInfo {
                name: "Mocked Player".to_string(),
                ..default_test_player_info()
            },
            name: "Mocked Player".to_string(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type_bound::<Plugin>(),
                "UnknownPlayer".into_py(py),
                Some(vec![player]),
            );
            assert!(result.is_none());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_unsupported_search_criteria() {
        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type_bound::<Plugin>(),
                3.42f64.into_py(py),
                Some(vec![default_test_player()]),
            );
            assert!(result.is_none());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn find_player_with_empty_str_returns_player_list() {
        let player1 = Player {
            id: 21,
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 21,
                steam_id: 1234,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player2 = Player {
            id: 0,
            steam_id: 1235,
            player_info: PlayerInfo {
                client_id: 0,
                steam_id: 1235,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        Python::with_gil(|py| {
            let result = Plugin::find_player(
                &py.get_type_bound::<Plugin>(),
                "",
                Some(vec![player1.clone(), player2.clone()]),
            );
            assert_eq!(result, vec![player1, player2]);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn find_players_by_matching_provided_names() {
        let player1 = Player {
            id: 21,
            name: "^1Found ^4Player".to_string(),
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 21,
                name: "^1Found ^4Player".to_string(),
                steam_id: 1234,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player2 = Player {
            id: 0,
            name: "non-matching Player".to_string(),
            steam_id: 1235,
            player_info: PlayerInfo {
                client_id: 0,
                name: "non-matching Player".to_string(),
                steam_id: 1235,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player3 = Player {
            id: 5,
            name: "found Player".to_string(),
            steam_id: 1236,
            player_info: PlayerInfo {
                client_id: 5,
                name: "found Player".to_string(),
                steam_id: 1236,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        Python::with_gil(|py| {
            let result = Plugin::find_player(
                &py.get_type_bound::<Plugin>(),
                "foU^3nd",
                Some(vec![player1.clone(), player2.clone(), player3.clone()]),
            );
            assert_eq!(result, vec![player1, player3]);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn find_players_when_no_player_matches() {
        let player1 = Player {
            id: 21,
            name: "^1non-matching ^4Player".to_string(),
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 21,
                name: "^1non-matching ^4Player".to_string(),
                steam_id: 1234,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player2 = Player {
            id: 0,
            name: "non-matching Player".to_string(),
            steam_id: 1235,
            player_info: PlayerInfo {
                client_id: 0,
                name: "non-matching Player".to_string(),
                steam_id: 1235,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player3 = Player {
            id: 5,
            name: "non-matching Player".to_string(),
            steam_id: 1236,
            player_info: PlayerInfo {
                client_id: 5,
                name: "non-matching Player".to_string(),
                steam_id: 1236,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        Python::with_gil(|py| {
            let result = Plugin::find_player(
                &py.get_type_bound::<Plugin>(),
                "no-such-player",
                Some(vec![player1, player2, player3]),
            );
            assert!(result.is_empty());
        });
    }

    #[test]
    #[cfg_attr(iri, ignore)]
    fn teams_when_no_player_in_player_list() {
        Python::with_gil(|py| {
            let result = Plugin::teams(&py.get_type_bound::<Plugin>(), Some(vec![]));
            assert!(result
                .expect("result was not ok")
                .eq([
                    ("free".to_string().into_py(py), PyList::empty_bound(py)),
                    ("red".into_py(py), PyList::empty_bound(py)),
                    ("blue".into_py(py), PyList::empty_bound(py)),
                    ("spectator".into_py(py), PyList::empty_bound(py))
                ]
                .into_py_dict_bound(py))
                .expect("comparison was not ok"),);
        });
    }

    #[test]
    #[cfg_attr(iri, ignore)]
    fn teams_when_every_team_has_one_player() {
        let player1 = Player {
            id: 0,
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 0,
                steam_id: 1234,
                team: team_t::TEAM_FREE as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player2 = Player {
            id: 1,
            steam_id: 1235,
            player_info: PlayerInfo {
                client_id: 1,
                steam_id: 1235,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player3 = Player {
            id: 2,
            steam_id: 1236,
            player_info: PlayerInfo {
                client_id: 2,
                steam_id: 1236,
                team: team_t::TEAM_BLUE as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player4 = Player {
            id: 3,
            steam_id: 1237,
            player_info: PlayerInfo {
                client_id: 3,
                steam_id: 1234,
                team: team_t::TEAM_SPECTATOR as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        Python::with_gil(|py| {
            let result = Plugin::teams(
                &py.get_type_bound::<Plugin>(),
                Some(vec![
                    player4.clone(),
                    player3.clone(),
                    player2.clone(),
                    player1.clone(),
                ]),
            );
            assert!(result
                .expect("result was not ok")
                .eq([
                    ("free".to_string().into_py(py), vec![player1].into_py(py)),
                    ("red".into_py(py), vec![player2].into_py(py)),
                    ("blue".into_py(py), vec![player3].into_py(py)),
                    ("spectator".into_py(py), vec![player4].into_py(py))
                ]
                .into_py_dict_bound(py))
                .expect("comparison was not ok"),);
        });
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn center_print_to_all_players_sends_center_print_server_command() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let send_server_cmd_ctx = shinqlx_send_server_command_context();
        send_server_cmd_ctx
            .expect()
            .withf(|recipients, cmd| recipients.is_none() && cmd == "cp \"asdf\"")
            .times(1);

        let result = Python::with_gil(|py| {
            Plugin::center_print(&py.get_type_bound::<Plugin>(), "asdf", None)
        });
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn center_print_to_paetticular_player_sends_center_print_server_command() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx.expect().returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
        });

        let send_server_cmd_ctx = shinqlx_send_server_command_context();
        send_server_cmd_ctx
            .expect()
            .withf(|recipients, cmd| recipients.is_some() && cmd == "cp \"asdf\"")
            .times(1);

        let player = default_test_player();

        let result = Python::with_gil(|py| {
            Plugin::center_print(
                &py.get_type_bound::<Plugin>(),
                "asdf",
                Some(player.into_py(py)),
            )
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tell_sends_msg_to_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
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

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "print \"asdf\n\"\n");

        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(0))
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
            let result = Plugin::tell(
                &py.get_type_bound::<Plugin>(),
                "asdf",
                default_test_player().into_py(py),
                None,
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_vote_active_when_configstring_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .return_const("vote is active");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            assert_eq!(Plugin::is_vote_active(&py.get_type_bound::<Plugin>()), true);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_vote_active_when_configstring_empty() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .return_const("");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            assert_eq!(
                Plugin::is_vote_active(&py.get_type_bound::<Plugin>()),
                false
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_vote_active_when_main_engine_not_set() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            assert_eq!(
                Plugin::is_vote_active(&py.get_type_bound::<Plugin>()),
                false
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_when_main_engine_not_set() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::current_vote_count(&py.get_type_bound::<Plugin>());
            assert!(result.is_ok_and(|value| value.is_none(py)))
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_when_yes_votes_are_empty() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .return_const("");
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .return_const("42");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::current_vote_count(&py.get_type_bound::<Plugin>());
            assert!(result.is_ok_and(|value| value.is_none(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_when_no_votes_are_empty() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .return_const("42");
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .return_const("");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::current_vote_count(&py.get_type_bound::<Plugin>());
            assert!(result.is_ok_and(|value| value.is_none(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_with_proper_vote_counts() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .return_const("42");
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .return_const("21");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::current_vote_count(&py.get_type_bound::<Plugin>());
            assert!(result
                .expect("result was not ok")
                .bind(py)
                .eq(PyTuple::new_bound(py, vec![42, 21]))
                .expect("comparison was not ok"));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_with_unparseable_yes_vote_counts() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .return_const("asdf");
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .return_const("21");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::current_vote_count(&py.get_type_bound::<Plugin>());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_with_unparseable_no_vote_counts() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_YES as u16))
            .return_const("42");
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_NO as u16))
            .return_const("asdf");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::current_vote_count(&py.get_type_bound::<Plugin>());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_when_vote_is_active() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .return_const("map overkill ca");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::callvote(
                &py.get_type_bound::<Plugin>(),
                "map thunderstruck ca",
                "map thunderstruck ca",
                30,
            );
            assert!(result.is_ok_and(|value| !value));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_calls_vote() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .return_const("");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let current_level_ctx = MockTestCurrentLevel::try_get_context();
        current_level_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level
                .expect_callvote()
                .withf(|vote_str, vote_disp_str, opt_time| {
                    vote_str == "map thunderstruck ca"
                        && vote_disp_str == "map thunderstruck ca"
                        && opt_time.is_some_and(|value| value == 30)
                })
                .times(1);
            Ok(mock_level)
        });

        Python::with_gil(|py| {
            let event_dispatcher = EventDispatcherManager::default();
            event_dispatcher
                .add_dispatcher(py, py.get_type_bound::<VoteStartedDispatcher>())
                .expect("could not add vote_started dispatcher");
            EVENT_DISPATCHERS.store(Some(
                Py::new(py, event_dispatcher)
                    .expect("could not create event dispatcher manager in python")
                    .into(),
            ));

            let result = Plugin::callvote(
                &py.get_type_bound::<Plugin>(),
                "map thunderstruck ca",
                "map thunderstruck ca",
                30,
            );
            assert!(result.is_ok_and(|value| value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_when_event_dispatcher_not_available() {
        EVENT_DISPATCHERS.store(None);

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(CS_VOTE_STRING as u16))
            .return_const("");
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::callvote(
                &py.get_type_bound::<Plugin>(),
                "map thunderstruck ca",
                "map thunderstruck ca",
                30,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_with_unparseable_value() {
        Python::with_gil(|py| {
            let result = Plugin::force_vote(&py.get_type_bound::<Plugin>(), "asdf".into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_forces_vote_passed() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_vote_state()
                        .with(predicate::eq(true))
                        .times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity
            });

        Python::with_gil(|py| {
            let result = Plugin::force_vote(&py.get_type_bound::<Plugin>(), true.into_py(py));
            assert!(result.is_ok_and(|value| value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_forces_vote_fail() {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_vote_state()
                        .with(predicate::eq(false))
                        .times(1);
                    Ok(mock_game_client)
                });
                mock_game_entity
            });

        Python::with_gil(|py| {
            let result = Plugin::force_vote(&py.get_type_bound::<Plugin>(), false.into_py(py));
            assert!(result.is_ok_and(|value| value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn teamsize_sets_teamsize() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("teamsize"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| cvar == "teamsize" && value == "42" && flags.is_none())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::teamsize(&py.get_type_bound::<Plugin>(), 42);
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_for_unknown_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::kick(&py.get_type_bound::<Plugin>(), 1.23.into_py(py), "");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_for_existing_player_without_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "was kicked.")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::kick(
                &py.get_type_bound::<Plugin>(),
                default_test_player().into_py(py),
                "",
            );
            assert!(result.as_ref().is_ok(), "{:?}", result.as_ref());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_for_existing_player_with_reason() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let drop_client_ctx = shinqlx_drop_client_context();
        drop_client_ctx
            .expect()
            .withf(|_client, reason| reason == "All your base are belong to us!")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::kick(
                &py.get_type_bound::<Plugin>(),
                default_test_player().into_py(py),
                "All your base are belong to us!",
            );
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn shuffle_forces_shuffle() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("forceshuffle"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::shuffle(&py.get_type_bound::<Plugin>()));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cointoss_does_nothing() {
        Python::with_gil(|py| {
            Plugin::cointoss(&py.get_type_bound::<Plugin>());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn change_map_with_no_factory() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("map thunderstruck"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::change_map(&py.get_type_bound::<Plugin>(), "thunderstruck", None);
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn change_map_with_factory() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("map thunderstruck ffa"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result =
                Plugin::change_map(&py.get_type_bound::<Plugin>(), "thunderstruck", Some("ffa"));
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn change_map_when_no_main_engine_set() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result =
                Plugin::change_map(&py.get_type_bound::<Plugin>(), "thunderstruck", Some("ffa"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn switch_with_invalid_player1() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::switch(
                &py.get_type_bound::<Plugin>(),
                1.23.into_py(py),
                default_test_player().into_py(py),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn switch_with_invalid_player2() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::switch(
                &py.get_type_bound::<Plugin>(),
                default_test_player().into_py(py),
                1.23.into_py(py),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn switch_with_players_on_same_team() {
        MAIN_ENGINE.store(None);

        let player1 = Player {
            id: 0,
            player_info: PlayerInfo {
                client_id: 0,
                team: team_t::TEAM_RED as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player2 = Player {
            id: 1,
            player_info: PlayerInfo {
                client_id: 1,
                team: team_t::TEAM_RED as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::switch(
                &py.get_type_bound::<Plugin>(),
                player1.into_py(py),
                player2.into_py(py),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn switch_with_players_on_different_team() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("put 0 blue"))
            .times(1);
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("put 1 red"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let player1 = Player {
            id: 0,
            player_info: PlayerInfo {
                client_id: 0,
                team: team_t::TEAM_RED as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        let player2 = Player {
            id: 1,
            player_info: PlayerInfo {
                client_id: 1,
                team: team_t::TEAM_BLUE as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::switch(
                &py.get_type_bound::<Plugin>(),
                player1.into_py(py),
                player2.into_py(py),
            );
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_sound_to_all_players() {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "playSound sound/vo/midair.ogg")
            .times(1);

        Python::with_gil(|py| {
            let result =
                Plugin::play_sound(&py.get_type_bound::<Plugin>(), "sound/vo/midair.ogg", None);
            assert!(result.is_ok_and(|value| value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_sound_to_a_specific_player() {
        let player = default_test_player();

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "playSound sound/vo/midair.ogg")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::play_sound(
                &py.get_type_bound::<Plugin>(),
                "sound/vo/midair.ogg",
                Some(player),
            );
            assert!(result.is_ok_and(|value| value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_sound_with_empty_soundpath() {
        Python::with_gil(|py| {
            let result = Plugin::play_sound(&py.get_type_bound::<Plugin>(), "", None);
            assert!(result.is_ok_and(|value| !value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_sound_for_sound_containing_music() {
        Python::with_gil(|py| {
            let result =
                Plugin::play_sound(&py.get_type_bound::<Plugin>(), "music/sonic1.ogg", None);
            assert!(result.is_ok_and(|value| !value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_music_to_all_players() {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "playMusic music/sonic1.ogg")
            .times(1);

        Python::with_gil(|py| {
            let result =
                Plugin::play_music(&py.get_type_bound::<Plugin>(), "music/sonic1.ogg", None);
            assert!(result.is_ok_and(|value| value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_music_to_a_specific_player() {
        let player = default_test_player();

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "playMusic music/sonic1.ogg")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::play_music(
                &py.get_type_bound::<Plugin>(),
                "music/sonic1.ogg",
                Some(player),
            );
            assert!(result.is_ok_and(|value| value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_music_with_empty_musicpath() {
        Python::with_gil(|py| {
            let result = Plugin::play_music(&py.get_type_bound::<Plugin>(), "", None);
            assert!(result.is_ok_and(|value| !value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_music_for_music_containing_sound() {
        Python::with_gil(|py| {
            let result =
                Plugin::play_music(&py.get_type_bound::<Plugin>(), "sound/vo/midair.ogg", None);
            assert!(result.is_ok_and(|value| !value),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stop_sound_for_all_players() {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "clearSounds")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::stop_sound(&py.get_type_bound::<Plugin>(), None);
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stop_sound_for_a_specific_player() {
        let player = default_test_player();

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "clearSounds")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::stop_sound(&py.get_type_bound::<Plugin>(), Some(player));
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stop_music_for_all_players() {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "stopMusic")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::stop_music(&py.get_type_bound::<Plugin>(), None);
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stop_music_for_a_specific_player() {
        let player = default_test_player();

        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(0))
            .returning(move |_| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .return_const(clientState_t::CS_ACTIVE);
                mock_client
            });

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "stopMusic")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::stop_music(&py.get_type_bound::<Plugin>(), Some(player));
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slap_for_invalid_player() {
        Python::with_gil(|py| {
            let result = Plugin::slap(&py.get_type_bound::<Plugin>(), py.None(), 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slap_for_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("slap 21 42"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::slap(&py.get_type_bound::<Plugin>(), 21.into_py(py), 42);
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_for_invalid_player() {
        Python::with_gil(|py| {
            let result = Plugin::slay(&py.get_type_bound::<Plugin>(), py.None());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_for_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("slay 21"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Plugin::slay(&py.get_type_bound::<Plugin>(), 21.into_py(py));
            assert!(result.is_ok());
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn timeout_pauses_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("timeout"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::timeout(&py.get_type_bound::<Plugin>()));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn timein_unpauses_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("timein"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::timein(&py.get_type_bound::<Plugin>()));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn allready_readies_all_players() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("allready"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::allready(&py.get_type_bound::<Plugin>()));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pause_pauses_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("pause"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::pause(&py.get_type_bound::<Plugin>()));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unpause_unpauses_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("unpause"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::unpause(&py.get_type_bound::<Plugin>()));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::lock(&py.get_type_bound::<Plugin>(), Some("invalid_team"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_with_no_team() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("lock"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::lock(&py.get_type_bound::<Plugin>(), None));
        assert!(result.is_ok());
    }

    #[rstest]
    #[case("red")]
    #[case("RED")]
    #[case("free")]
    #[case("blue")]
    #[case("spectator")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_a_specific_team(#[case] locked_team: &str) {
        let lock_cmd = format!("lock {}", locked_team.to_lowercase());
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .withf(move |cmd| cmd == lock_cmd)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::lock(&py.get_type_bound::<Plugin>(), Some(locked_team)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::unlock(&py.get_type_bound::<Plugin>(), Some("invalid_team"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_no_team() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("unlock"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::unlock(&py.get_type_bound::<Plugin>(), None));
        assert!(result.is_ok());
    }

    #[rstest]
    #[case("red")]
    #[case("RED")]
    #[case("free")]
    #[case("blue")]
    #[case("spectator")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_a_specific_team(#[case] locked_team: &str) {
        let unlock_cmd = format!("unlock {}", locked_team.to_lowercase());
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .withf(move |cmd| cmd == unlock_cmd)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::unlock(&py.get_type_bound::<Plugin>(), Some(locked_team))
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::put(
                &py.get_type_bound::<Plugin>(),
                2.into_py(py),
                "invalid team",
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::put(&py.get_type_bound::<Plugin>(), 2048.into_py(py), "red");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case("red")]
    #[case("RED")]
    #[case("free")]
    #[case("blue")]
    #[case("spectator")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_put_player_on_a_specific_team(#[case] new_team: &str) {
        let put_cmd = format!("put 2 {}", new_team.to_lowercase());
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .withf(move |cmd| cmd == put_cmd)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::put(&py.get_type_bound::<Plugin>(), 2.into_py(py), new_team)
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::mute(&py.get_type_bound::<Plugin>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_mutes_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("mute 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::mute(&py.get_type_bound::<Plugin>(), 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::unmute(&py.get_type_bound::<Plugin>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_unmutes_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("unmute 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::unmute(&py.get_type_bound::<Plugin>(), 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::tempban(&py.get_type_bound::<Plugin>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_tempbans_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("tempban 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::tempban(&py.get_type_bound::<Plugin>(), 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::ban(&py.get_type_bound::<Plugin>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_bans_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("ban 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::ban(&py.get_type_bound::<Plugin>(), 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::unban(&py.get_type_bound::<Plugin>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_unbans_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("unban 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::unban(&py.get_type_bound::<Plugin>(), 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn opsay_sends_op_message() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("opsay asdf"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::opsay(&py.get_type_bound::<Plugin>(), "asdf"));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::addadmin(&py.get_type_bound::<Plugin>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_adds_player_to_admins() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("addadmin 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::addadmin(&py.get_type_bound::<Plugin>(), 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::addmod(&py.get_type_bound::<Plugin>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_adds_player_to_moderators() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("addmod 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::addmod(&py.get_type_bound::<Plugin>(), 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::demote(&py.get_type_bound::<Plugin>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_demotes_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("demote 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::demote(&py.get_type_bound::<Plugin>(), 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn abort_aborts_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("map_restart"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Plugin::abort(&py.get_type_bound::<Plugin>()));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::addscore(&py.get_type_bound::<Plugin>(), 2048.into_py(py), 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_adds_score_to_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("addscore 2 42"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::addscore(&py.get_type_bound::<Plugin>(), 2.into_py(py), 42)
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addteamscore_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::addteamscore(&py.get_type_bound::<Plugin>(), "invalid_team", 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case("red")]
    #[case("RED")]
    #[case("free")]
    #[case("blue")]
    #[case("spectator")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addteamscore_adds_score_to_team(#[case] locked_team: &str) {
        let unlock_cmd = format!("addteamscore {} 42", locked_team.to_lowercase());
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .withf(move |cmd| cmd == unlock_cmd)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Plugin::addteamscore(&py.get_type_bound::<Plugin>(), locked_team, 42)
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setmatchtime_sets_match_time() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("setmatchtime 42"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Plugin::setmatchtime(&py.get_type_bound::<Plugin>(), 42));
        assert!(result.is_ok());
    }
}
