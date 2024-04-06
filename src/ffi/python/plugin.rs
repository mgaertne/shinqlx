use super::prelude::*;

use super::{
    addadmin, addmod, addscore, addteamscore, ban, client_id, commands::CommandPriorities, demote,
    lock, mute, opsay, put, pyshinqlx_get_logger, set_teamsize, tempban, unban, unlock, unmute,
    LOADED_PLUGINS, PLUGIN_DATABASE,
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

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::IntoPyDict;
use pyo3::{
    exceptions::{PyEnvironmentError, PyValueError},
    gc::PyVisit,
    intern,
    types::{PyDict, PyList, PySet, PyTuple, PyType},
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

    #[classattr]
    #[pyo3(name = "database")]
    fn get_database() -> Option<PyObject> {
        let database_class_guard = PLUGIN_DATABASE.try_read()?;

        match database_class_guard.as_ref() {
            None => None,
            Some(db_class) => Python::with_gil(|py| Some(db_class.clone_ref(py))),
        }
    }

    #[setter(database)]
    fn set_database(slf: &Bound<'_, Self>, py: Python<'_>, _db: PyObject) -> PyResult<()> {
        let plugin_type = slf.get_type();
        let logger = pyshinqlx_get_logger(py, Some(plugin_type.name()?.into_py(py)))?;
        let logging_module = py.import_bound(intern!(py, "logging"))?;
        let warning_level = logging_module.getattr(intern!(py, "WARNING"))?;
        let log_record = logger.call_method(
            intern!(py, "makeRecord"),
            (
                intern!(py, "shinqlx"),
                warning_level,
                intern!(py, ""),
                -1,
                intern!(py, "Setting of class attribute 'database' unsupported. Plugin.database is initialized during Python initalization based on the configured cvars."),
                py.None(),
                py.None(),
            ),
            Some(
                &[(intern!(py, "func"), intern!(py, "databse"))].into_py_dict_bound(py),
            ),
        )?;
        logger.call_method1(intern!(py, "handle"), (log_record,))?;

        Ok(())
    }

    /// The database instance.
    #[getter(db)]
    fn get_db(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<PyObject> {
        let Some(database_class_guard) = PLUGIN_DATABASE.try_read() else {
            let plugin_name = Self::get_name(slf)?;
            let error_msg = format!("Plugin '{plugin_name}' does not have a database driver.");
            return Err(PyRuntimeError::new_err(error_msg));
        };

        let mut plugin = slf.borrow_mut();
        match database_class_guard.as_ref() {
            None => {
                let plugin_name = Self::get_name(slf)?;
                let error_msg = format!("Plugin '{plugin_name}' does not have a database driver.");
                return Err(PyRuntimeError::new_err(error_msg));
            }
            Some(db_class) => {
                if plugin.db_instance.bind(py).is_none() {
                    let db_instance = db_class.call1(py, (slf,))?;
                    plugin.db_instance = db_instance;
                }
            }
        };

        Ok(plugin.db_instance.clone_ref(py))
    }

    /// The name of the plugin.
    #[getter(name)]
    fn get_name(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_type().name().map(|value| value.to_string())
    }

    #[classattr]
    #[pyo3(name = "_loaded_plugins")]
    fn get_loaded_plugins() -> PyObject {
        Python::with_gil(|py| {
            let Some(loaded_plugins_guard) = LOADED_PLUGINS.try_read() else {
                return PyDict::new_bound(py).unbind().into();
            };

            let loaded_plugins: &Vec<(String, PyObject)> = loaded_plugins_guard.as_ref();
            loaded_plugins.into_py_dict_bound(py).unbind().into()
        })
    }

    /// A dictionary containing plugin names as keys and plugin instances
    /// as values of all currently loaded plugins.
    #[getter(plugins)]
    fn get_plugins<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        let Some(loaded_plugins_guard) = LOADED_PLUGINS.try_read() else {
            return PyDict::new_bound(py);
        };

        let loaded_plugins: &Vec<(String, PyObject)> = loaded_plugins_guard.as_ref();
        loaded_plugins.into_py_dict_bound(py)
    }

    /// A list of all the hooks this plugin has.
    #[getter(hooks)]
    fn get_hooks(&self) -> Vec<(String, PyObject, i32)> {
        self.hooks.clone()
    }

    /// A list of all the commands this plugin has registered.
    #[getter(commands)]
    fn get_commands(&self) -> Vec<Command> {
        let Some(commands) = self.commands.try_read() else {
            return vec![];
        };
        let cloned_commands: Vec<Command> = commands.clone();
        cloned_commands
    }

    /// A Game instance.
    #[getter(game)]
    fn get_game(&self, py: Python<'_>) -> Option<Game> {
        Game::py_new(py, true).ok()
    }

    /// An instance of :class:`logging.Logger`, but initialized for this plugin.
    #[getter(logger)]
    fn get_logger<'a>(slf: &Bound<'a, Self>) -> PyResult<Bound<'a, PyAny>> {
        let plugin_name = slf.get_type().name().map(|value| value.to_string())?;
        pyshinqlx_get_logger(slf.py(), Some(plugin_name.into_py(slf.py())))
    }

    #[pyo3(signature = (event, handler, priority = CommandPriorities::PRI_NORMAL as i32))]
    fn add_hook(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        event: String,
        handler: PyObject,
        priority: i32,
    ) -> PyResult<()> {
        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
        let event_dispatchers = shinqlx_module.getattr(intern!(py, "EVENT_DISPATCHERS"))?;
        let event_dispatcher = event_dispatchers.get_item(&event)?;

        let plugin_type = slf.get_type();
        let plugin_name = plugin_type.name()?;
        event_dispatcher
            .call_method1(intern!(py, "add_hook"), (plugin_name, &handler, priority))?;

        let Ok(mut plugin) = slf.try_borrow_mut() else {
            return Err(PyEnvironmentError::new_err("cound not borrow plugin hooks"));
        };
        plugin.hooks.push((event.clone(), handler, priority));

        Ok(())
    }

    #[pyo3(signature = (event, handler, priority = CommandPriorities::PRI_NORMAL as i32))]
    fn remove_hook(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        event: String,
        handler: PyObject,
        priority: i32,
    ) -> PyResult<()> {
        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
        let event_dispatchers = shinqlx_module.getattr(intern!(py, "EVENT_DISPATCHERS"))?;
        let event_dispatcher = event_dispatchers.get_item(&event)?;

        let plugin_type = slf.get_type();
        let plugin_name = plugin_type.name()?;
        event_dispatcher.call_method1(
            intern!(py, "remove_hook"),
            (plugin_name, &handler, priority),
        )?;

        let Ok(mut plugin) = slf.try_borrow_mut() else {
            return Err(PyEnvironmentError::new_err("cound not borrow plugin hooks"));
        };
        plugin
            .hooks
            .retain(|(hook_event, hook_handler, hook_priority)| {
                hook_event == &event
                    && hook_handler.bind(py).eq(handler.bind(py)).unwrap_or(true)
                    && hook_priority == &priority
            });

        Ok(())
    }

    #[pyo3(signature = (
        name,
        handler,
        permission = 0,
        channels = None,
        exclude_channels = None,
        priority = CommandPriorities::PRI_NORMAL as i32,
        client_cmd_pass = false,
        client_cmd_perm = 0,
        prefix = true,
        usage = ""))]
    #[allow(clippy::too_many_arguments)]
    fn add_command(
        slf: Bound<'_, Self>,
        py: Python<'_>,
        name: PyObject,
        handler: PyObject,
        permission: i32,
        channels: Option<PyObject>,
        exclude_channels: Option<PyObject>,
        priority: i32,
        client_cmd_pass: bool,
        client_cmd_perm: i32,
        prefix: bool,
        usage: &str,
    ) -> PyResult<()> {
        let Ok(plugin) = slf.try_borrow() else {
            return Err(PyEnvironmentError::new_err("cannot borrow plugin"));
        };

        let py_channels = channels.unwrap_or(py.None());
        let py_exclude_channels = exclude_channels.unwrap_or(PyTuple::empty_bound(py).into_py(py));

        let new_command = Command::py_new(
            py,
            slf.into_py(py),
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

        let mut commands_guard = plugin.commands.write();
        commands_guard.push(new_command.clone());

        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
        let commands_invoker = shinqlx_module.getattr(intern!(py, "COMMANDS"))?;
        commands_invoker.call_method1(intern!(py, "add_command"), (new_command, priority))?;

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
    #[pyo3(signature = (name, return_type), text_signature = "(name, return_type=str)")]
    fn get_cvar(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: &str,
        return_type: Option<PyObject>,
    ) -> PyResult<PyObject> {
        #[allow(clippy::question_mark)]
        let cvar = py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return None;
            };
            main_engine.find_cvar(name)
        });

        let cvar_string = cvar.as_ref().map(|value| value.get_string());

        let Some(py_return_type) = return_type else {
            return Ok(cvar_string.into_py(py));
        };
        let Ok(py_return_type_str) = py_return_type
            .bind(py)
            .getattr(intern!(py, "__name__"))
            .map(|value| value.to_string())
        else {
            return Err(PyValueError::new_err("Invalid return type: None"));
        };

        match py_return_type_str.as_str() {
            "str" => match cvar_string {
                None => Ok(py.None()),
                Some(value) => Ok(value.into_py(py)),
            },
            "int" => match cvar_string {
                None => Ok(py.None()),
                Some(value) => value
                    .parse::<i128>()
                    .map(|int| int.into_py(py))
                    .map_err(|_| {
                        let error_description =
                            format!("invalid literal for int() with base 10: '{}'", value);
                        PyValueError::new_err(error_description)
                    }),
            },
            "float" => match cvar_string {
                None => Ok(py.None()),
                Some(value) => value
                    .parse::<f64>()
                    .map(|float| float.into_py(py))
                    .map_err(|_| {
                        let error_description =
                            format!("could not convert string to float: '{}'", value);
                        PyValueError::new_err(error_description)
                    }),
            },
            "bool" => match cvar_string {
                None => Ok(false.into_py(py)),
                Some(value) => value
                    .parse::<i128>()
                    .map(|int| (int != 0).into_py(py))
                    .map_err(|_| {
                        let error_description =
                            format!("invalid literal for int() with base 10: '{}'", value);
                        PyValueError::new_err(error_description)
                    }),
            },
            "list" => match cvar_string {
                None => Ok(PyList::empty_bound(py).into_py(py)),
                Some(value) => {
                    let items: Vec<&str> = value.split(',').collect();
                    let returned = PyList::new_bound(py, items);
                    Ok(returned.into_py(py))
                }
            },
            "set" => match cvar_string {
                None => PySet::empty_bound(py).map(|set| set.into_py(py)),
                Some(value) => {
                    let items: Vec<String> =
                        value.split(',').map(|item| item.to_string()).collect();
                    let returned = PySet::new_bound::<String>(py, &items);
                    returned.map(|set| set.into_py(py))
                }
            },
            "tuple" => match cvar_string {
                None => Ok(PyTuple::empty_bound(py).into_py(py)),
                Some(value) => {
                    let items: Vec<&str> = value.split(',').collect();
                    let returned = PyTuple::new_bound(py, items);
                    Ok(returned.into_py(py))
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
    #[pyo3(signature = (name, value, flags = 0))]
    fn set_cvar(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: &str,
        value: PyObject,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.bind(py).str()?.to_string();

        py.allow_threads(|| {
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
    #[pyo3(signature = (name, value, minimum, maximum, flags = 0))]
    fn set_cvar_limit(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: &str,
        value: PyObject,
        minimum: PyObject,
        maximum: PyObject,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.bind(py).str()?.to_string();
        let minimum_str = minimum.bind(py).str()?.to_string();
        let maximum_str = maximum.bind(py).str()?.to_string();

        py.allow_threads(|| {
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
    #[pyo3(signature = (name, value, flags = 0))]
    fn set_cvar_once(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: &str,
        value: PyObject,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.bind(py).str()?.to_string();

        py.allow_threads(|| {
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
    #[pyo3(signature = (name, value, minimum, maximum, flags = 0))]
    fn set_cvar_limit_once(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: &str,
        value: PyObject,
        minimum: PyObject,
        maximum: PyObject,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.bind(py).str()?.to_string();
        let minimum_str = minimum.bind(py).str()?.to_string();
        let maximum_str = maximum.bind(py).str()?.to_string();

        py.allow_threads(|| {
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
    fn players(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<Vec<Player>> {
        Player::all_players(&py.get_type_bound::<Player>(), py)
    }

    /// Get a Player instance from the name, client ID,
    /// or Steam ID. Assumes [0, 64) to be a client ID
    /// and [64, inf) to be a Steam ID.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None))]
    fn player(
        cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: PyObject,
        player_list: Option<Vec<Player>>,
    ) -> PyResult<Option<Player>> {
        if let Ok(player) = name.extract::<Player>(py) {
            return Ok(Some(player));
        }

        if let Ok(player_id) = name.extract::<i32>(py) {
            if (0..64).contains(&player_id) {
                return Player::py_new(player_id, None).map(Some);
            }
        }

        let players = player_list.unwrap_or_else(|| Self::players(cls, py).unwrap_or_default());
        if let Ok(player_steam_id) = name.extract::<i64>(py) {
            return Ok(players
                .into_iter()
                .find(|player| player.steam_id == player_steam_id));
        }

        let Some(client_id) = client_id(py, name, Some(players.clone())) else {
            return Ok(None);
        };
        Ok(players.into_iter().find(|player| player.id == client_id))
    }

    /// Send a message to the chat, or any other channel.
    #[classmethod]
    #[pyo3(signature = (msg, chat_channel, **kwargs),
    text_signature = "(msg, chat_channel = \"chat\", **kwargs)")]
    fn msg(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        msg: &str,
        chat_channel: Option<PyObject>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;

        match chat_channel {
            None => {
                let chat_channel = shinqlx_module.getattr(intern!(py, "CHAT_CHANNEL"))?;
                chat_channel.call_method(intern!(py, "reply"), (msg,), kwargs)?;
                return Ok(());
            }
            Some(channel) => {
                let bound_channel = channel.bind(py);
                if bound_channel.is_instance_of::<AbstractChannel>() {
                    bound_channel.call_method(intern!(py, "reply"), (msg,), kwargs)?;
                    return Ok(());
                }

                let shinqlx_chat_channel = shinqlx_module.getattr(intern!(py, "CHAT_CHANNEL"))?;
                if shinqlx_chat_channel.eq(bound_channel)? {
                    shinqlx_chat_channel.call_method(intern!(py, "reply"), (msg,), kwargs)?;
                    return Ok(());
                }

                let red_team_chat_channel =
                    shinqlx_module.getattr(intern!(py, "RED_TEAM_CHAT_CHANNEL"))?;
                if red_team_chat_channel.eq(bound_channel)? {
                    red_team_chat_channel.call_method(intern!(py, "reply"), (msg,), kwargs)?;
                    return Ok(());
                }

                let blue_team_chat_channel =
                    shinqlx_module.getattr(intern!(py, "BLUE_TEAM_CHAT_CHANNEL"))?;
                if blue_team_chat_channel.eq(bound_channel)? {
                    blue_team_chat_channel.call_method(intern!(py, "reply"), (msg,), kwargs)?;
                    return Ok(());
                }

                let console_channel = shinqlx_module.getattr(intern!(py, "CONSOLE_CHANNEL"))?;
                if console_channel.eq(bound_channel)? {
                    console_channel.call_method(intern!(py, "reply"), (msg,), kwargs)?;
                    return Ok(());
                }
            }
        }
        Err(PyValueError::new_err("Invalid channel."))
    }

    /// Prints text in the console.
    #[classmethod]
    fn console(_cls: &Bound<'_, PyType>, py: Python<'_>, text: PyObject) -> PyResult<()> {
        let extracted_text = text.bind(py).str()?.to_string();
        let printed_text = format!("{extracted_text}\n");
        py.allow_threads(|| {
            shinqlx_com_printf(&printed_text);
            Ok(())
        })
    }

    /// Removes color tags from text.
    #[classmethod]
    fn clean_text(_cls: &Bound<'_, PyType>, py: Python<'_>, text: &str) -> String {
        py.allow_threads(|| clean_text(&text))
    }

    /// Get the colored name of a decolored name.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None))]
    fn colored_name(
        cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: PyObject,
        player_list: Option<Vec<Player>>,
    ) -> Option<String> {
        if let Ok(player) = name.extract::<Player>(py) {
            return Some(player.name.clone());
        }

        let Ok(searched_name) = name.bind(py).extract::<String>() else {
            return None;
        };

        let players = player_list.unwrap_or_else(|| Self::players(cls, py).unwrap_or_default());
        let clean_name = clean_text(&searched_name).to_lowercase();

        players
            .iter()
            .find(|&player| player.get_clean_name(py).to_lowercase() == clean_name)
            .map(|found_player| found_player.name.clone())
    }

    /// Get a player's client id from the name, client ID,
    /// Player instance, or Steam ID. Assumes [0, 64) to be
    /// a client ID and [64, inf) to be a Steam ID.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None))]
    fn client_id(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: PyObject,
        player_list: Option<Vec<Player>>,
    ) -> Option<i32> {
        client_id(py, name, player_list)
    }

    /// Find a player based on part of a players name.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None))]
    fn find_player(
        cls: &Bound<'_, PyType>,
        py: Python<'_>,
        name: &str,
        player_list: Option<Vec<Player>>,
    ) -> Vec<Player> {
        let players = player_list.unwrap_or_else(|| Self::players(cls, py).unwrap_or_default());

        if name.is_empty() {
            return players;
        }

        let cleaned_text = clean_text(&name).to_lowercase();
        players
            .into_iter()
            .filter(|player| clean_text(&player.name).contains(&cleaned_text))
            .collect()
    }

    /// Get a dictionary with the teams as keys and players as values.
    #[classmethod]
    #[pyo3(signature = (player_list = None))]
    fn teams<'py>(
        cls: &Bound<'py, PyType>,
        py: Python<'py>,
        player_list: Option<Vec<Player>>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let players = player_list.unwrap_or_else(|| Self::players(cls, py).unwrap_or_default());

        let result = PyDict::new_bound(py);

        let filtered_frees: Vec<PyObject> = players
            .clone()
            .into_iter()
            .filter(|player| player.get_team(py).is_ok_and(|team| team == "free"))
            .map(|player| player.into_py(py))
            .collect();
        result.set_item(intern!(py, "free"), filtered_frees)?;

        let filtered_reds: Vec<PyObject> = players
            .clone()
            .into_iter()
            .filter(|player| player.get_team(py).is_ok_and(|team| team == "red"))
            .map(|player| player.into_py(py))
            .collect();
        result.set_item(intern!(py, "red"), filtered_reds)?;

        let filtered_blues: Vec<PyObject> = players
            .clone()
            .into_iter()
            .filter(|player| player.get_team(py).is_ok_and(|team| team == "blue"))
            .map(|player| player.into_py(py))
            .collect();
        result.set_item(intern!(py, "blue"), filtered_blues)?;

        let filtered_specs: Vec<PyObject> = players
            .clone()
            .into_iter()
            .filter(|player| player.get_team(py).is_ok_and(|team| team == "spectator"))
            .map(|player| player.into_py(py))
            .collect();
        result.set_item(intern!(py, "spectator"), filtered_specs)?;

        Ok(result)
    }

    #[classmethod]
    #[pyo3(signature = (msg, recipient = None))]
    fn center_print(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        msg: &str,
        recipient: Option<PyObject>,
    ) -> PyResult<()> {
        let client_id = recipient.and_then(|recipient| client_id(py, recipient, None));

        let center_printed_cmd = format!(r#"cp "{msg}""#);
        pyshinqlx_send_server_command(py, client_id, &center_printed_cmd)?;

        Ok(())
    }

    /// Send a tell (private message) to someone.
    #[classmethod]
    #[pyo3(signature = (msg, recipient, **kwargs))]
    fn tell(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        msg: &str,
        recipient: PyObject,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let Some(recipient_client_id) = client_id(py, recipient, None) else {
            return Err(PyValueError::new_err("could not find recipient"));
        };
        let recipient_player = Player::py_new(recipient_client_id, None)?;
        let tell_channel = TellChannel::py_new(&recipient_player);

        let tell_channel_py = Py::new(py, tell_channel)?;
        tell_channel_py.call_method_bound(py, intern!(py, "reply"), (msg,), kwargs)?;

        Ok(())
    }

    #[classmethod]
    fn is_vote_active(_cls: &Bound<'_, PyType>, py: Python<'_>) -> bool {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return false;
            };

            let vote_string = main_engine.get_configstring(CS_VOTE_STRING as u16);
            !vote_string.is_empty()
        })
    }

    #[classmethod]
    fn current_vote_count(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<PyObject> {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Ok(py.None());
        };

        let yes_votes = main_engine.get_configstring(CS_VOTE_YES as u16);
        let no_votes = main_engine.get_configstring(CS_VOTE_NO as u16);

        if yes_votes.is_empty() || no_votes.is_empty() {
            return Ok(py.None());
        }

        let Ok(parsed_yes_votes) = yes_votes.parse::<i32>() else {
            let error_msg = format!("invalid literal for int() with base 10: '{}'", yes_votes);
            return Err(PyValueError::new_err(error_msg));
        };
        let Ok(parsed_no_votes) = no_votes.parse::<i32>() else {
            let error_msg = format!("invalid literal for int() with base 10: '{}'", no_votes);
            return Err(PyValueError::new_err(error_msg));
        };

        if yes_votes.is_empty() || no_votes.is_empty() {
            return Ok(py.None());
        }
        Ok((parsed_yes_votes, parsed_no_votes).into_py(py))
    }

    #[classmethod]
    #[pyo3(signature = (vote, display, time = 30))]
    fn callvote(
        cls: &Bound<'_, PyType>,
        py: Python<'_>,
        vote: &str,
        display: &str,
        time: i32,
    ) -> PyResult<bool> {
        if Self::is_vote_active(cls, py) {
            return Ok(false);
        }

        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
        let event_dispatchers = shinqlx_module.getattr(intern!(py, "EVENT_DISPATCHERS"))?;
        let vote_started_dispatcher = event_dispatchers.get_item(intern!(py, "vote_started"))?;
        vote_started_dispatcher.call_method1(intern!(py, "caller"), (py.None(),))?;

        pyshinqlx_callvote(py, vote, display, Some(time));

        Ok(true)
    }

    #[classmethod]
    fn force_vote(_cls: &Bound<'_, PyType>, py: Python<'_>, pass_it: PyObject) -> PyResult<bool> {
        pass_it
            .bind(py)
            .is_truthy()
            .map_err(|_| PyValueError::new_err("pass_it must be either True or False."))
            .and_then(|vote_passed| pyshinqlx_force_vote(py, vote_passed))
    }

    #[classmethod]
    fn teamsize(_cls: &Bound<'_, PyType>, py: Python<'_>, size: i32) -> PyResult<()> {
        set_teamsize(py, size)
    }

    #[classmethod]
    #[pyo3(signature = (player, reason = ""))]
    fn kick(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        player: PyObject,
        reason: &str,
    ) -> PyResult<()> {
        let Some(client_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let forwarded_reason = if reason.is_empty() {
            None
        } else {
            Some(reason)
        };

        pyshinqlx_kick(py, client_id, forwarded_reason)?;

        Ok(())
    }

    #[classmethod]
    fn shuffle(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "forceshuffle")
    }

    #[classmethod]
    fn cointoss(_cls: &Bound<'_, PyType>) {}

    #[classmethod]
    #[pyo3(signature = (new_map, factory = None))]
    fn change_map(
        _cls: &Bound<'_, PyType>,
        py: Python,
        new_map: &str,
        factory: Option<&str>,
    ) -> PyResult<()> {
        let mapchange_command = match factory {
            None => format!("map {}", new_map),
            Some(game_factory) => format!("map {} {}", new_map, game_factory),
        };
        pyshinqlx_console_command(py, &mapchange_command)
    }

    #[classmethod]
    fn switch(
        cls: &Bound<'_, PyType>,
        py: Python<'_>,
        player: PyObject,
        other_player: PyObject,
    ) -> PyResult<()> {
        let Some(player1) = Self::player(cls, py, player, None)? else {
            return Err(PyValueError::new_err("The first player is invalid."));
        };
        let Some(player2) = Self::player(cls, py, other_player, None)? else {
            return Err(PyValueError::new_err("The second player is invalid."));
        };

        let team1 = player1.get_team(py)?;
        let team2 = player2.get_team(py)?;

        if team1 == team2 {
            return Err(PyValueError::new_err("Both player are on the same team."));
        }

        player1.put(py, &team2)?;
        player2.put(py, &team1)?;

        Ok(())
    }

    #[classmethod]
    #[pyo3(signature = (sound_path, player = None))]
    fn play_sound(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        sound_path: &str,
        player: Option<Player>,
    ) -> PyResult<bool> {
        if sound_path.is_empty() || sound_path.contains("music/") {
            return Ok(false);
        }

        let play_sound_cmd = format!("playSound {sound_path}");
        pyshinqlx_send_server_command(py, player.map(|player| player.id), &play_sound_cmd)?;

        Ok(true)
    }

    #[classmethod]
    #[pyo3(signature = (music_path, player = None))]
    fn play_music(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        music_path: &str,
        player: Option<Player>,
    ) -> PyResult<bool> {
        if music_path.is_empty() || music_path.contains("sound/") {
            return Ok(false);
        }

        let play_music_cmd = format!("playMusic {music_path}");
        pyshinqlx_send_server_command(py, player.map(|player| player.id), &play_music_cmd)?;

        Ok(true)
    }

    #[classmethod]
    #[pyo3(signature = (player = None))]
    fn stop_sound(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        player: Option<Player>,
    ) -> PyResult<()> {
        pyshinqlx_send_server_command(py, player.map(|player| player.id), "clearSounds")?;

        Ok(())
    }

    #[classmethod]
    #[pyo3(signature = (player = None))]
    fn stop_music(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        player: Option<Player>,
    ) -> PyResult<()> {
        pyshinqlx_send_server_command(py, player.map(|player| player.id), "stopMusic")?;

        Ok(())
    }

    #[classmethod]
    #[pyo3(signature = (player, damage = 0))]
    fn slap(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        player: PyObject,
        damage: i32,
    ) -> PyResult<()> {
        let Some(client_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let slap_cmd = format!("slap {client_id} {damage}");
        pyshinqlx_console_command(py, &slap_cmd)?;

        Ok(())
    }

    #[classmethod]
    fn slay(_cls: &Bound<'_, PyType>, py: Python<'_>, player: PyObject) -> PyResult<()> {
        let Some(client_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let slay_cmd = format!("slay {client_id}");
        pyshinqlx_console_command(py, &slay_cmd)?;

        Ok(())
    }

    #[classmethod]
    fn timeout(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "timeout")
    }

    #[classmethod]
    fn timein(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "timein")
    }

    #[classmethod]
    fn allready(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "allready")
    }

    #[classmethod]
    fn pause(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "pause")
    }

    #[classmethod]
    fn unpause(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "unpause")
    }

    #[classmethod]
    #[pyo3(signature = (team = None))]
    fn lock(_cls: &Bound<'_, PyType>, py: Python<'_>, team: Option<&str>) -> PyResult<()> {
        lock(py, team)
    }

    #[classmethod]
    #[pyo3(signature = (team = None))]
    fn unlock(_cls: &Bound<'_, PyType>, py: Python<'_>, team: Option<&str>) -> PyResult<()> {
        unlock(py, team)
    }

    #[classmethod]
    fn put(_cls: &Bound<'_, PyType>, py: Python, player: PyObject, team: &str) -> PyResult<()> {
        put(py, player, team)
    }

    #[classmethod]
    fn mute(_cls: &Bound<'_, PyType>, py: Python<'_>, player: PyObject) -> PyResult<()> {
        mute(py, player)
    }

    #[classmethod]
    fn unmute(_cls: &Bound<'_, PyType>, py: Python<'_>, player: PyObject) -> PyResult<()> {
        unmute(py, player)
    }

    #[classmethod]
    fn tempban(_cls: &Bound<'_, PyType>, py: Python<'_>, player: PyObject) -> PyResult<()> {
        tempban(py, player)
    }

    #[classmethod]
    fn ban(_cls: &Bound<'_, PyType>, py: Python<'_>, player: PyObject) -> PyResult<()> {
        ban(py, player)
    }

    #[classmethod]
    fn unban(_cls: &Bound<'_, PyType>, py: Python<'_>, player: PyObject) -> PyResult<()> {
        unban(py, player)
    }

    #[classmethod]
    fn opsay(_cls: &Bound<'_, PyType>, py: Python<'_>, msg: &str) -> PyResult<()> {
        opsay(py, msg)
    }

    #[classmethod]
    fn addadmin(_cls: &Bound<'_, PyType>, py: Python<'_>, player: PyObject) -> PyResult<()> {
        addadmin(py, player)
    }

    #[classmethod]
    fn addmod(_cls: &Bound<'_, PyType>, py: Python<'_>, player: PyObject) -> PyResult<()> {
        addmod(py, player)
    }

    #[classmethod]
    fn demote(_cls: &Bound<'_, PyType>, py: Python<'_>, player: PyObject) -> PyResult<()> {
        demote(py, player)
    }

    #[classmethod]
    fn abort(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "map_restart")
    }

    #[classmethod]
    fn addscore(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        player: PyObject,
        score: i32,
    ) -> PyResult<()> {
        addscore(py, player, score)
    }

    #[classmethod]
    fn addteamscore(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        team: &str,
        score: i32,
    ) -> PyResult<()> {
        addteamscore(py, team, score)
    }

    #[classmethod]
    fn setmatchtime(_cls: &Bound<'_, PyType>, py: Python<'_>, time: i32) -> PyResult<()> {
        let setmatchtime_cmd = format!("setmatchtime {}", time);
        pyshinqlx_console_command(py, &setmatchtime_cmd)
    }
}

#[cfg(test)]
mod plugin_tests {
    use super::MAIN_ENGINE;
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;
    use std::ffi::{c_char, CString};

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyValueError};
    use rstest::rstest;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_main_engine_not_initialized() {
        MAIN_ENGINE.store(None);
        Python::with_gil(|py| {
            let result =
                Plugin::get_cvar(&py.get_type_bound::<Plugin>(), py, "sv_maxclients", None);
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
            let result = Plugin::get_cvar(&py.get_type_bound::<Plugin>(), py, "asdf", None);
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
            let result =
                Plugin::get_cvar(&py.get_type_bound::<Plugin>(), py, "sv_maxclients", None);
            assert!(result
                .expect("result was not OK")
                .extract::<String>(py)
                .is_ok_and(|value| value == "16"));
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
                py,
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
                py,
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
                py,
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
                py,
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
                py,
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
                py,
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
                py,
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
                py,
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
                py,
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
                py,
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
                py,
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
    #[serial]
    #[cfg_attr(miri, ignore)]
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

        let all_players =
            Python::with_gil(|py| Plugin::players(&py.get_type_bound::<Plugin>(), py));
        assert_eq!(
            all_players.expect("result was not ok"),
            vec![
                Player {
                    valid: true,
                    id: 0,
                    player_info: PlayerInfo {
                        client_id: 0,
                        name: "Mocked Player".to_string(),
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        userinfo: "asdf".to_string(),
                        steam_id: 1234,
                        team: team_t::TEAM_RED as i32,
                        privileges: 0,
                    },
                    name: "Mocked Player".to_string(),
                    steam_id: 1234,
                    user_info: "asdf".to_string(),
                },
                Player {
                    valid: true,
                    id: 2,
                    player_info: PlayerInfo {
                        client_id: 2,
                        name: "Mocked Player".to_string(),
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        userinfo: "asdf".to_string(),
                        steam_id: 1234,
                        team: team_t::TEAM_RED as i32,
                        privileges: 0,
                    },
                    name: "Mocked Player".to_string(),
                    steam_id: 1234,
                    user_info: "asdf".to_string(),
                },
            ]
        );
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

        let result = Python::with_gil(|py| Plugin::shuffle(&py.get_type_bound::<Plugin>(), py));
        assert!(result.is_ok());
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

        let result = Python::with_gil(|py| Plugin::timeout(&py.get_type_bound::<Plugin>(), py));
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

        let result = Python::with_gil(|py| Plugin::timein(&py.get_type_bound::<Plugin>(), py));
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

        let result = Python::with_gil(|py| Plugin::allready(&py.get_type_bound::<Plugin>(), py));
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

        let result = Python::with_gil(|py| Plugin::pause(&py.get_type_bound::<Plugin>(), py));
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

        let result = Python::with_gil(|py| Plugin::unpause(&py.get_type_bound::<Plugin>(), py));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::lock(&py.get_type_bound::<Plugin>(), py, Some("invalid_team"));
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

        let result = Python::with_gil(|py| Plugin::lock(&py.get_type_bound::<Plugin>(), py, None));
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

        let result = Python::with_gil(|py| {
            Plugin::lock(&py.get_type_bound::<Plugin>(), py, Some(locked_team))
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::unlock(&py.get_type_bound::<Plugin>(), py, Some("invalid_team"));
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

        let result =
            Python::with_gil(|py| Plugin::unlock(&py.get_type_bound::<Plugin>(), py, None));
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
            Plugin::unlock(&py.get_type_bound::<Plugin>(), py, Some(locked_team))
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
                py,
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
            let result = Plugin::put(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py), "red");
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
            Plugin::put(&py.get_type_bound::<Plugin>(), py, 2.into_py(py), new_team)
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::mute(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py));
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
            Python::with_gil(|py| Plugin::mute(&py.get_type_bound::<Plugin>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::unmute(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py));
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

        let result = Python::with_gil(|py| {
            Plugin::unmute(&py.get_type_bound::<Plugin>(), py, 2.into_py(py))
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::tempban(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py));
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

        let result = Python::with_gil(|py| {
            Plugin::tempban(&py.get_type_bound::<Plugin>(), py, 2.into_py(py))
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::ban(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py));
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
            Python::with_gil(|py| Plugin::ban(&py.get_type_bound::<Plugin>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::unban(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py));
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
            Python::with_gil(|py| Plugin::unban(&py.get_type_bound::<Plugin>(), py, 2.into_py(py)));
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

        let result =
            Python::with_gil(|py| Plugin::opsay(&py.get_type_bound::<Plugin>(), py, "asdf"));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::addadmin(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py));
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

        let result = Python::with_gil(|py| {
            Plugin::addadmin(&py.get_type_bound::<Plugin>(), py, 2.into_py(py))
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::addmod(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py));
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

        let result = Python::with_gil(|py| {
            Plugin::addmod(&py.get_type_bound::<Plugin>(), py, 2.into_py(py))
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::demote(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py));
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

        let result = Python::with_gil(|py| {
            Plugin::demote(&py.get_type_bound::<Plugin>(), py, 2.into_py(py))
        });
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

        let result = Python::with_gil(|py| Plugin::abort(&py.get_type_bound::<Plugin>(), py));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Plugin::addscore(&py.get_type_bound::<Plugin>(), py, 2048.into_py(py), 42);
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
            Plugin::addscore(&py.get_type_bound::<Plugin>(), py, 2.into_py(py), 42)
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addteamscore_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result =
                Plugin::addteamscore(&py.get_type_bound::<Plugin>(), py, "invalid_team", 42);
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
            Plugin::addteamscore(&py.get_type_bound::<Plugin>(), py, locked_team, 42)
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
            Python::with_gil(|py| Plugin::setmatchtime(&py.get_type_bound::<Plugin>(), py, 42));
        assert!(result.is_ok());
    }
}
