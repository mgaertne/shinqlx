use pyo3::{
    BoundObject, PyTraverseError,
    exceptions::{PyEnvironmentError, PyRuntimeError, PyValueError},
    gc::PyVisit,
    intern,
    prelude::*,
    types::{PyBool, PyDict, PyFloat, PyInt, PyList, PySet, PyString, PyTuple, PyType},
};

use super::{
    BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, COMMANDS, CONSOLE_CHANNEL, CommandInvokerMethods,
    EVENT_DISPATCHERS, EventDispatcherMethods, RED_TEAM_CHAT_CHANNEL, addadmin, addmod, addscore,
    addteamscore, ban, client_id, commands::CommandPriorities, console_command, demote,
    is_vote_active, lock, mute, opsay, prelude::*, put, pyshinqlx_get_logger, set_teamsize,
    tempban, unban, unlock, unmute,
};
#[cfg(test)]
use crate::hooks::mock_hooks::shinqlx_com_printf;
#[cfg(not(test))]
use crate::hooks::shinqlx_com_printf;
use crate::{
    MAIN_ENGINE,
    ffi::c::prelude::{CS_VOTE_NO, CS_VOTE_YES},
    quake_live_engine::{ConsoleCommand, FindCVar, GetCVar, GetConfigstring, SetCVarLimit},
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
#[pyclass(name = "Plugin", module = "_plugin", subclass, frozen)]
pub(crate) struct Plugin {
    hooks: parking_lot::RwLock<Vec<(String, PyObject, i32)>>,
    commands: parking_lot::RwLock<Vec<Py<Command>>>,
    db_instance: parking_lot::RwLock<PyObject>,
}

#[pymethods]
impl Plugin {
    #[new]
    fn py_new(py: Python<'_>) -> Self {
        Self {
            hooks: vec![].into(),
            commands: vec![].into(),
            db_instance: py.None().into(),
        }
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        self.hooks
            .read()
            .iter()
            .map(|(_, handler, _)| visit.call(handler))
            .collect::<Result<Vec<_>, PyTraverseError>>()?;
        self.commands
            .read()
            .iter()
            .map(|cmd| visit.call(cmd))
            .collect::<Result<Vec<_>, PyTraverseError>>()?;
        visit.call(&*self.db_instance.read())?;
        Ok(())
    }

    fn __clear__(&self) {
        self.hooks.write().clear();
        self.commands.write().clear();
    }

    fn __str__(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_type().name().map(|value| value.to_string())
    }

    /// The database instance.
    #[getter(db)]
    fn get_db<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        slf.get_db()
    }

    /// The name of the plugin.
    #[getter(name)]
    fn get_name(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_name()
    }

    /// A dictionary containing plugin names as keys and plugin instances
    /// as values of all currently loaded plugins.
    #[getter(plugins)]
    fn get_plugins<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyDict>> {
        slf.get_plugins()
    }

    /// A list of all the hooks this plugin has.
    #[getter(hooks)]
    fn get_hooks(slf: &Bound<'_, Self>) -> Vec<(String, Py<PyAny>, i32)> {
        slf.get_hooks()
    }

    /// A list of all the commands this plugin has registered.
    #[getter(commands)]
    fn get_commands(slf: &Bound<'_, Self>) -> Vec<Py<Command>> {
        slf.get_commands()
    }

    /// A Game instance.
    #[getter(game)]
    fn get_game(slf: &Bound<'_, Self>) -> Option<Game> {
        slf.get_game()
    }

    /// An instance of :class:`logging.Logger`, but initialized for this plugin.
    #[getter(logger)]
    pub(crate) fn get_logger<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        slf.get_logger()
    }

    #[pyo3(signature = (event, handler, priority = CommandPriorities::PRI_NORMAL as i32), text_signature = "(event, handler, priority = PRI_NORMAL)")]
    fn add_hook(
        slf: &Bound<'_, Self>,
        event: &str,
        handler: &Bound<'_, PyAny>,
        priority: i32,
    ) -> PyResult<()> {
        slf.add_hook(event, handler, priority)
    }

    #[pyo3(signature = (event, handler, priority = CommandPriorities::PRI_NORMAL as i32), text_signature = "(event, handler, priority = PRI_NORMAL)")]
    fn remove_hook(
        slf: &Bound<'_, Self>,
        event: &str,
        handler: &Bound<'_, PyAny>,
        priority: i32,
    ) -> PyResult<()> {
        slf.remove_hook(event, handler, priority)
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
        slf: &Bound<'_, Self>,
        name: &Bound<'_, PyAny>,
        handler: &Bound<'_, PyAny>,
        permission: i32,
        channels: Option<Bound<'_, PyAny>>,
        exclude_channels: Option<Bound<'_, PyAny>>,
        priority: u32,
        client_cmd_pass: bool,
        client_cmd_perm: i32,
        prefix: bool,
        usage: &str,
    ) -> PyResult<()> {
        slf.add_command(
            name,
            handler,
            permission,
            channels,
            exclude_channels,
            priority,
            client_cmd_pass,
            client_cmd_perm,
            prefix,
            usage,
        )
    }

    fn remove_command(
        slf: &Bound<'_, Self>,
        name: &Bound<'_, PyAny>,
        handler: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        slf.remove_command(name, handler)
    }

    /// Gets the value of a cvar as a string.
    #[classmethod]
    #[pyo3(signature = (name, return_type = None), text_signature = "(name, return_type=str)")]
    fn get_cvar<'py>(
        cls: &Bound<'py, PyType>,
        name: &str,
        return_type: Option<Py<PyType>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        #[allow(clippy::question_mark)]
        let cvar = cls.py().allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return None;
            };
            main_engine.find_cvar(name)
        });

        let cvar_string = cvar.as_ref().map(|value| value.get_string());

        let Some(py_return_type) = return_type else {
            return match cvar_string {
                None => Ok(cls.py().None().into_bound(cls.py())),
                Some(value) => Ok(PyString::new(cls.py(), &value).into_any()),
            };
        };
        let py_return_type_str = py_return_type
            .bind(cls.py())
            .getattr(intern!(cls.py(), "__name__"))
            .and_then(|value| value.extract::<String>())
            .unwrap_or("Python type without __name__".into());

        match py_return_type_str.as_str() {
            "str" => match cvar_string {
                None => Ok(cls.py().None().into_bound(cls.py())),
                Some(value) => Ok(PyString::new(cls.py(), &value).into_any()),
            },
            "int" => match cvar_string {
                None => Ok(cls.py().None().into_bound(cls.py())),
                Some(value) => value.parse::<i64>().map_or_else(
                    |_| {
                        let error_description =
                            format!("invalid literal for int() with base 10: '{value}'");
                        Err(PyValueError::new_err(error_description))
                    },
                    |int| Ok(PyInt::new(cls.py(), int).into_any()),
                ),
            },
            "float" => match cvar_string {
                None => Ok(cls.py().None().into_bound(cls.py())),
                Some(value) => value.parse::<f64>().map_or_else(
                    |_| {
                        let error_description =
                            format!("could not convert string to float: '{value}'");
                        Err(PyValueError::new_err(error_description))
                    },
                    |float| Ok(PyFloat::new(cls.py(), float).into_any()),
                ),
            },
            "bool" => match cvar_string {
                None => Ok(PyBool::new(cls.py(), false).into_any().to_owned()),
                Some(value) => value.parse::<i64>().map_or_else(
                    |_| {
                        let error_description =
                            format!("invalid literal for int() with base 10: '{value}'");
                        Err(PyValueError::new_err(error_description))
                    },
                    |int| Ok(PyBool::new(cls.py(), int != 0).into_any().to_owned()),
                ),
            },
            "list" => match cvar_string {
                None => Ok(PyList::empty(cls.py()).into_any()),
                Some(value) => {
                    let items: Vec<&str> = value.split(',').collect();
                    let returned = PyList::new(cls.py(), items)?;
                    Ok(PyList::new(cls.py(), returned)?.into_any())
                }
            },
            "set" => match cvar_string {
                None => PySet::empty(cls.py()).map(|set| set.into_any()),
                Some(value) => {
                    let items: Vec<String> =
                        value.split(',').map(|item| item.to_string()).collect();
                    Ok(PySet::new::<String>(cls.py(), items)?.into_any())
                }
            },
            "tuple" => match cvar_string {
                None => Ok(PyTuple::empty(cls.py()).into_any()),
                Some(value) => {
                    let items: Vec<&str> = value.split(',').collect();
                    Ok(PyTuple::new(cls.py(), items)?.into_any())
                }
            },
            value => {
                let error_description = format!("Invalid return type: {value}");
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
        value: &Bound<'_, PyAny>,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.str()?.to_string();

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
        value: &Bound<'_, PyAny>,
        minimum: &Bound<'_, PyAny>,
        maximum: &Bound<'_, PyAny>,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.str()?.to_string();
        let minimum_str = minimum.str()?.to_string();
        let maximum_str = maximum.str()?.to_string();

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
        value: &Bound<'_, PyAny>,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.str()?.to_string();

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
        value: &Bound<'_, PyAny>,
        minimum: &Bound<'_, PyAny>,
        maximum: &Bound<'_, PyAny>,
        flags: i32,
    ) -> PyResult<bool> {
        let value_str = value.str()?.to_string();
        let minimum_str = minimum.str()?.to_string();
        let maximum_str = maximum.str()?.to_string();

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
        Player::all_players(&cls.py().get_type::<Player>())
    }

    /// Get a Player instance from the name, client ID,
    /// or Steam ID. Assumes [0, 64) to be a client ID
    /// and [64, inf) to be a Steam ID.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None), text_signature = "(name, player_list = None)")]
    fn player(
        cls: &Bound<'_, PyType>,
        name: &Bound<'_, PyAny>,
        player_list: Option<Vec<Player>>,
    ) -> PyResult<Option<Player>> {
        if let Ok(player) = name.extract::<Player>() {
            return Ok(Some(player));
        }

        if let Ok(player_id) = name.extract::<i32>() {
            if (0..64).contains(&player_id) {
                return Player::py_new(player_id, None).map(Some);
            }
        }

        let players = player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());
        if let Ok(player_steam_id) = name.extract::<i64>() {
            return Ok(players
                .iter()
                .find(|player| player.steam_id == player_steam_id)
                .cloned());
        }

        let Some(client_id) = client_id(cls.py(), name, Some(players.clone())) else {
            return Ok(None);
        };
        Ok(players
            .iter()
            .find(|player| player.id == client_id)
            .cloned())
    }

    /// Send a message to the chat, or any other channel.
    #[classmethod]
    #[pyo3(signature = (msg, chat_channel = None, **kwargs),
    text_signature = "(msg, chat_channel = \"chat\", **kwargs)")]
    fn msg(
        cls: &Bound<'_, PyType>,
        msg: &str,
        chat_channel: Option<Bound<'_, PyAny>>,
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
                return CHAT_CHANNEL
                    .load()
                    .as_ref()
                    .map(|main_chat_channel| {
                        main_chat_channel
                            .bind(cls.py())
                            .as_super()
                            .reply(msg, limit, &delimiter)
                    })
                    .unwrap_or(Ok(()));
            }
            Some(channel) => {
                if channel
                    .as_ref()
                    .get_type()
                    .is_subclass(&cls.py().get_type::<AbstractChannel>())
                    .unwrap_or(false)
                {
                    return channel
                        .as_ref()
                        .call_method(intern!(cls.py(), "reply"), (msg,), kwargs)
                        .map(|_| ());
                }

                for global_channel in [
                    &CHAT_CHANNEL,
                    &RED_TEAM_CHAT_CHANNEL,
                    &BLUE_TEAM_CHAT_CHANNEL,
                ] {
                    if let Some(result) = global_channel
                        .load()
                        .as_ref()
                        .filter(|global_chat_channel| {
                            global_chat_channel
                                .bind(cls.py())
                                .eq(channel.as_ref())
                                .unwrap_or(false)
                        })
                        .map(|global_chat_channel| {
                            global_chat_channel
                                .bind(cls.py())
                                .as_super()
                                .reply(msg, limit, &delimiter)
                        })
                    {
                        return result;
                    }
                }

                if let Some(result) = CONSOLE_CHANNEL
                    .load()
                    .as_ref()
                    .filter(|console_channel| {
                        console_channel
                            .bind(cls.py())
                            .eq(channel.as_ref())
                            .unwrap_or(false)
                    })
                    .map(|console_channel| {
                        console_channel.bind(cls.py()).reply(msg, limit, &delimiter)
                    })
                {
                    return result;
                }
            }
        }
        Err(PyValueError::new_err("Invalid channel."))
    }

    /// Prints text in the console.
    #[classmethod]
    fn console(cls: &Bound<'_, PyType>, text: &Bound<'_, PyAny>) -> PyResult<()> {
        let extracted_text = text.str()?.to_string();
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
        name: &Bound<'_, PyAny>,
        player_list: Option<Vec<Player>>,
    ) -> Option<String> {
        if let Ok(player) = name.extract::<Player>() {
            return Some(player.name.read().clone());
        }

        let Ok(searched_name) = name.str().map(|value| value.to_string()) else {
            return None;
        };

        let players = player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());
        let clean_name = clean_text(&searched_name).to_lowercase();

        players
            .iter()
            .find(|&player| clean_text(&(&*player.name.read())).to_lowercase() == clean_name)
            .map(|found_player| found_player.name.read().clone())
    }

    /// Get a player's client id from the name, client ID,
    /// Player instance, or Steam ID. Assumes [0, 64) to be
    /// a client ID and [64, inf) to be a Steam ID.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None), text_signature = "(name, player_list = None)")]
    fn client_id(
        cls: &Bound<'_, PyType>,
        name: &Bound<'_, PyAny>,
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
                .iter()
                .filter(|player| {
                    let player_name = &*player.name.read();
                    clean_text(player_name)
                        .to_lowercase()
                        .contains(&cleaned_text)
                })
                .cloned()
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

        [
            intern!(cls.py(), "free"),
            intern!(cls.py(), "red"),
            intern!(cls.py(), "blue"),
            intern!(cls.py(), "spectator"),
        ]
        .iter()
        .try_fold(PyDict::new(cls.py()), |result, team_str| {
            let filtered_players: Vec<Player> = players
                .iter()
                .filter(|player| {
                    player
                        .get_team(cls.py())
                        .is_ok_and(|team| team == team_str.to_string())
                })
                .cloned()
                .collect();
            result.set_item(team_str, filtered_players)?;

            Ok(result)
        })
    }

    #[classmethod]
    #[pyo3(signature = (msg, recipient = None), text_signature = "(msg, recipient = None)")]
    fn center_print(
        cls: &Bound<'_, PyType>,
        msg: &str,
        recipient: Option<Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let client_id = recipient.and_then(|recipient| client_id(cls.py(), &recipient, None));

        let center_printed_cmd = format!(r#"cp "{msg}""#);
        pyshinqlx_send_server_command(cls.py(), client_id, &center_printed_cmd).map(|_| ())
    }

    /// Send a tell (private message) to someone.
    #[classmethod]
    #[pyo3(signature = (msg, recipient, **kwargs))]
    fn tell(
        cls: &Bound<'_, PyType>,
        msg: &str,
        recipient: &Bound<'_, PyAny>,
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
            .call_method(intern!(cls.py(), "reply"), (msg,), kwargs)
            .map(|_| ())
    }

    #[classmethod]
    fn is_vote_active(cls: &Bound<'_, PyType>) -> bool {
        cls.py().allow_threads(is_vote_active)
    }

    #[classmethod]
    fn current_vote_count<'py>(cls: &Bound<'py, PyType>) -> PyResult<Bound<'py, PyAny>> {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Ok(cls.py().None().into_bound(cls.py()));
        };

        let yes_votes = main_engine.get_configstring(CS_VOTE_YES as u16);
        let no_votes = main_engine.get_configstring(CS_VOTE_NO as u16);

        if yes_votes.is_empty() || no_votes.is_empty() {
            return Ok(cls.py().None().into_bound(cls.py()));
        }

        let Ok(parsed_yes_votes) = yes_votes.parse::<i32>() else {
            let error_msg = format!("invalid literal for int() with base 10: '{yes_votes}'");
            return Err(PyValueError::new_err(error_msg));
        };
        let Ok(parsed_no_votes) = no_votes.parse::<i32>() else {
            let error_msg = format!("invalid literal for int() with base 10: '{no_votes}'");
            return Err(PyValueError::new_err(error_msg));
        };

        Ok(PyTuple::new(cls.py(), [parsed_yes_votes, parsed_no_votes])?.into_any())
    }

    #[classmethod]
    #[pyo3(signature = (vote, display, time = 30), text_signature = "(vote, display, time = 30)")]
    fn callvote(cls: &Bound<'_, PyType>, vote: &str, display: &str, time: i32) -> PyResult<bool> {
        if Self::is_vote_active(cls) {
            return Ok(false);
        }

        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(cls.py())
                    .get_item(intern!(cls.py(), "vote_started"))
                    .ok()
            })
            .map_or(
                Err(PyEnvironmentError::new_err(
                    "could not get access to vote started dispatcher",
                )),
                |vote_started_dispatcher| {
                    vote_started_dispatcher
                        .downcast::<VoteStartedDispatcher>()?
                        .caller(cls.py().None().bind(cls.py()));
                    Ok(())
                },
            )?;

        pyshinqlx_callvote(cls.py(), vote, display, Some(time));

        Ok(true)
    }

    #[classmethod]
    fn force_vote(cls: &Bound<'_, PyType>, pass_it: &Bound<'_, PyAny>) -> PyResult<bool> {
        pass_it
            .downcast::<PyBool>()
            .map_err(|_| PyValueError::new_err("pass_it must be either True or False."))
            .and_then(|vote_passed| pyshinqlx_force_vote(cls.py(), vote_passed.is_true()))
    }

    #[classmethod]
    fn teamsize(cls: &Bound<'_, PyType>, size: i32) -> PyResult<()> {
        cls.py().allow_threads(|| set_teamsize(size))
    }

    #[classmethod]
    #[pyo3(signature = (player, reason = ""), text_signature = "(player, reason = \"\")")]
    fn kick(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>, reason: &str) -> PyResult<()> {
        let Some(client_id) = client_id(cls.py(), player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let forwarded_reason = if reason.is_empty() {
            None
        } else {
            Some(reason)
        };

        pyshinqlx_kick(cls.py(), client_id, forwarded_reason).map(|_| ())
    }

    #[classmethod]
    fn shuffle(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("forceshuffle"))
    }

    #[classmethod]
    fn cointoss(_cls: &Bound<'_, PyType>) {}

    #[classmethod]
    #[pyo3(signature = (new_map, factory = None), text_signature = "(new_map, factory = None)")]
    fn change_map(cls: &Bound<'_, PyType>, new_map: &str, factory: Option<&str>) -> PyResult<()> {
        cls.py().allow_threads(|| {
            let mapchange_command = match factory {
                None => format!("map {new_map}"),
                Some(game_factory) => format!("map {new_map} {game_factory}"),
            };
            console_command(&mapchange_command)
        })
    }

    #[classmethod]
    fn switch(
        cls: &Bound<'_, PyType>,
        player: &Bound<'_, PyAny>,
        other_player: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let Some(player1) = Self::player(cls, player, None)? else {
            return Err(PyValueError::new_err("The first player is invalid."));
        };
        let Some(player2) = Self::player(cls, other_player, None)? else {
            return Err(PyValueError::new_err("The second player is invalid."));
        };

        Bound::new(cls.py(), player1)?.switch(&Bound::new(cls.py(), player2)?)
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
        pyshinqlx_send_server_command(cls.py(), player.map(|player| player.id), &play_sound_cmd)
            .map(|_| true)
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
        pyshinqlx_send_server_command(cls.py(), player.map(|player| player.id), &play_music_cmd)
            .map(|_| true)
    }

    #[classmethod]
    #[pyo3(signature = (player = None), text_signature = "(player = None)")]
    fn stop_sound(cls: &Bound<'_, PyType>, player: Option<Player>) -> PyResult<()> {
        pyshinqlx_send_server_command(cls.py(), player.map(|player| player.id), "clearSounds")
            .map(|_| ())
    }

    #[classmethod]
    #[pyo3(signature = (player = None), text_signature = "(player = None)")]
    fn stop_music(cls: &Bound<'_, PyType>, player: Option<Player>) -> PyResult<()> {
        pyshinqlx_send_server_command(cls.py(), player.map(|player| player.id), "stopMusic")
            .map(|_| ())
    }

    #[classmethod]
    #[pyo3(signature = (player, damage = 0), text_signature = "(player, damage = 0)")]
    fn slap(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>, damage: i32) -> PyResult<()> {
        client_id(cls.py(), player, None).map_or(
            Err(PyValueError::new_err("Invalid player.")),
            |client_id| {
                cls.py().allow_threads(|| {
                    let slap_cmd = format!("slap {client_id} {damage}");
                    console_command(&slap_cmd).map(|_| ())
                })
            },
        )
    }

    #[classmethod]
    fn slay(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        client_id(cls.py(), player, None).map_or(
            Err(PyValueError::new_err("Invalid player.")),
            |client_id| {
                cls.py().allow_threads(|| {
                    let slay_cmd = format!("slay {client_id}");
                    console_command(&slay_cmd).map(|_| ())
                })
            },
        )
    }

    #[classmethod]
    fn timeout(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("timeout"))
    }

    #[classmethod]
    fn timein(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("timein"))
    }

    #[classmethod]
    fn allready(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("allready"))
    }

    #[classmethod]
    fn pause(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("pause"))
    }

    #[classmethod]
    fn unpause(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("unpause"))
    }

    #[classmethod]
    #[pyo3(signature = (team = None), text_signature = "(team = None)")]
    fn lock(cls: &Bound<'_, PyType>, team: Option<&str>) -> PyResult<()> {
        cls.py().allow_threads(|| lock(team))
    }

    #[classmethod]
    #[pyo3(signature = (team = None), text_signature = "(team = None)")]
    fn unlock(cls: &Bound<'_, PyType>, team: Option<&str>) -> PyResult<()> {
        cls.py().allow_threads(|| unlock(team))
    }

    #[classmethod]
    fn put(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>, team: &str) -> PyResult<()> {
        put(cls.py(), player, team)
    }

    #[classmethod]
    fn mute(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        mute(cls.py(), player)
    }

    #[classmethod]
    fn unmute(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        unmute(cls.py(), player)
    }

    #[classmethod]
    fn tempban(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        tempban(cls.py(), player)
    }

    #[classmethod]
    fn ban(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        ban(cls.py(), player)
    }

    #[classmethod]
    fn unban(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        unban(cls.py(), player)
    }

    #[classmethod]
    fn opsay(cls: &Bound<'_, PyType>, msg: &str) -> PyResult<()> {
        cls.py().allow_threads(|| opsay(msg))
    }

    #[classmethod]
    fn addadmin(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        addadmin(cls.py(), player)
    }

    #[classmethod]
    fn addmod(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        addmod(cls.py(), player)
    }

    #[classmethod]
    fn demote(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        demote(cls.py(), player)
    }

    #[classmethod]
    fn abort(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("map_restart"))
    }

    #[classmethod]
    fn addscore(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>, score: i32) -> PyResult<()> {
        addscore(cls.py(), player, score)
    }

    #[classmethod]
    fn addteamscore(cls: &Bound<'_, PyType>, team: &str, score: i32) -> PyResult<()> {
        cls.py().allow_threads(|| addteamscore(team, score))
    }

    #[classmethod]
    fn setmatchtime(cls: &Bound<'_, PyType>, time: i32) -> PyResult<()> {
        cls.py().allow_threads(|| {
            let setmatchtime_cmd = format!("setmatchtime {time}");
            console_command(&setmatchtime_cmd)
        })
    }
}

pub(crate) trait PluginMethods<'py> {
    fn get_db(&self) -> PyResult<Bound<'py, PyAny>>;
    fn get_name(&self) -> PyResult<String>;
    fn get_plugins(&self) -> PyResult<Bound<'py, PyDict>>;
    fn get_hooks(&self) -> Vec<(String, Py<PyAny>, i32)>;
    fn get_commands(&self) -> Vec<Py<Command>>;
    fn get_game(&self) -> Option<Game>;
    fn get_logger(&self) -> PyResult<Bound<'py, PyAny>>;
    fn add_hook(&self, event: &str, handler: &Bound<'_, PyAny>, priority: i32) -> PyResult<()>;
    fn remove_hook(&self, event: &str, handler: &Bound<'_, PyAny>, priority: i32) -> PyResult<()>;
    #[allow(clippy::too_many_arguments)]
    fn add_command(
        &self,
        name: &Bound<'_, PyAny>,
        handler: &Bound<'_, PyAny>,
        permission: i32,
        channels: Option<Bound<'_, PyAny>>,
        exclude_channels: Option<Bound<'_, PyAny>>,
        priority: u32,
        client_cmd_pass: bool,
        client_cmd_perm: i32,
        prefix: bool,
        usage: &str,
    ) -> PyResult<()>;
    fn remove_command(&self, name: &Bound<'py, PyAny>, handler: &Bound<'py, PyAny>)
    -> PyResult<()>;
}

impl<'py> PluginMethods<'py> for Bound<'py, Plugin> {
    fn get_db(&self) -> PyResult<Bound<'py, PyAny>> {
        let plugin_name = self.get_name()?;
        let Ok(db_class) = self
            .py()
            .get_type::<Plugin>()
            .getattr(intern!(self.py(), "database"))
        else {
            let error_msg = format!("Plugin '{plugin_name}' does not have a database driver.");
            return Err(PyRuntimeError::new_err(error_msg));
        };

        if db_class.is_none() {
            let error_msg = format!("Plugin '{plugin_name}' does not have a database driver.");
            return Err(PyRuntimeError::new_err(error_msg));
        }

        let plugin = self.borrow();
        if plugin.db_instance.read().bind(self.py()).is_none() {
            let db_instance = db_class.call1((self,))?;
            *plugin.db_instance.write() = db_instance.unbind();
        }

        let returned = plugin
            .db_instance
            .read()
            .clone_ref(self.py())
            .into_bound(self.py());
        Ok(returned)
    }

    fn get_name(&self) -> PyResult<String> {
        self.get_type().name().map(|value| value.to_string())
    }

    fn get_plugins(&self) -> PyResult<Bound<'py, PyDict>> {
        let loaded_plugins = self
            .py()
            .get_type::<Plugin>()
            .getattr(intern!(self.py(), "_loaded_plugins"))?;

        Ok(loaded_plugins.downcast()?.clone())
    }

    fn get_hooks(&self) -> Vec<(String, Py<PyAny>, i32)> {
        self.borrow()
            .hooks
            .read()
            .iter()
            .map(|(name, handler, permission)| {
                (name.clone(), handler.clone_ref(self.py()), *permission)
            })
            .collect()
    }

    fn get_commands(&self) -> Vec<Py<Command>> {
        self.borrow()
            .commands
            .read()
            .iter()
            .map(|command| command.clone_ref(self.py()))
            .collect()
    }

    fn get_game(&self) -> Option<Game> {
        Game::py_new(self.py(), true).ok()
    }

    fn get_logger(&self) -> PyResult<Bound<'py, PyAny>> {
        self.get_type()
            .name()
            .map(|value| value.to_string())
            .and_then(|plugin_name| {
                pyshinqlx_get_logger(
                    self.py(),
                    Some(PyString::new(self.py(), &plugin_name).into_any()),
                )
            })
    }

    fn add_hook(&self, event: &str, handler: &Bound<'_, PyAny>, priority: i32) -> PyResult<()> {
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| event_dispatchers.bind(self.py()).get_item(event).ok())
            .map_or(
                Err(PyEnvironmentError::new_err(
                    "could not get access to event dispatcher",
                )),
                |event_dispatcher| {
                    let plugin_type = self.get_type();
                    let plugin_name = plugin_type.name()?;
                    event_dispatcher.call_method1(
                        intern!(self.py(), "add_hook"),
                        (plugin_name, &handler, priority),
                    )?;
                    Ok(())
                },
            )?;

        self.try_borrow().map_or(
            Err(PyEnvironmentError::new_err("could not borrow plugin hooks")),
            |plugin| {
                plugin
                    .hooks
                    .write()
                    .push((event.to_string(), handler.clone().unbind(), priority));
                Ok(())
            },
        )
    }

    fn remove_hook(&self, event: &str, handler: &Bound<'_, PyAny>, priority: i32) -> PyResult<()> {
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| event_dispatchers.bind(self.py()).get_item(event).ok())
            .map_or(
                Err(PyEnvironmentError::new_err(
                    "could not get access to event dispatchers",
                )),
                |event_dispatcher| {
                    let plugin_type = self.get_type();
                    let plugin_name = plugin_type.name()?;
                    EventDispatcherMethods::remove_hook(
                        event_dispatcher.downcast()?,
                        &plugin_name.to_string(),
                        handler,
                        priority,
                    )?;
                    Ok(())
                },
            )?;

        self.try_borrow().map_or(
            Err(PyEnvironmentError::new_err("could not borrow plugin hooks")),
            |plugin| {
                plugin
                    .hooks
                    .write()
                    .retain(|(hook_event, hook_handler, hook_priority)| {
                        hook_event != event
                            || hook_handler
                                .bind(self.py())
                                .ne(handler.as_ref())
                                .unwrap_or(false)
                            || hook_priority != &priority
                    });
                Ok(())
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn add_command(
        &self,
        name: &Bound<'_, PyAny>,
        handler: &Bound<'_, PyAny>,
        permission: i32,
        channels: Option<Bound<'_, PyAny>>,
        exclude_channels: Option<Bound<'_, PyAny>>,
        priority: u32,
        client_cmd_pass: bool,
        client_cmd_perm: i32,
        prefix: bool,
        usage: &str,
    ) -> PyResult<()> {
        let py_channels = channels.unwrap_or(self.py().None().into_bound(self.py()));
        let py_exclude_channels = exclude_channels.unwrap_or(PyTuple::empty(self.py()).into_any());

        let new_command = Command::py_new(
            self,
            name,
            handler,
            permission,
            &py_channels,
            &py_exclude_channels,
            client_cmd_pass,
            client_cmd_perm,
            prefix,
            usage,
        )?;
        let py_command = Bound::new(self.py(), new_command)?;

        self.try_borrow().map_or(
            Err(PyEnvironmentError::new_err("cound not borrow plugin hooks")),
            |plugin| {
                plugin.commands.write().push(py_command.clone().unbind());
                Ok(())
            },
        )?;

        COMMANDS.load().as_ref().map_or(Ok(()), |commands| {
            commands
                .bind(self.py())
                .add_command(&py_command, priority as usize)
        })
    }

    fn remove_command(
        &self,
        name: &Bound<'py, PyAny>,
        handler: &Bound<'py, PyAny>,
    ) -> PyResult<()> {
        let mut names = vec![];
        name.downcast::<PyList>().ok().iter().for_each(|py_list| {
            py_list.iter().for_each(|py_alias| {
                py_alias
                    .extract::<String>()
                    .ok()
                    .iter()
                    .for_each(|alias| names.push(alias.clone()));
            })
        });
        name.downcast::<PyTuple>().ok().iter().for_each(|py_tuple| {
            py_tuple.iter().for_each(|py_alias| {
                py_alias
                    .extract::<String>()
                    .ok()
                    .iter()
                    .for_each(|alias| names.push(alias.clone()));
            })
        });
        name.extract::<String>().ok().iter().for_each(|py_string| {
            names.push(py_string.clone());
        });

        self.borrow()
            .commands
            .read()
            .iter()
            .find(|&existing_command| {
                existing_command.bind(self.py()).borrow().name.len() == names.len()
                    && existing_command
                        .bind(self.py())
                        .borrow()
                        .name
                        .iter()
                        .all(|name| names.contains(name))
                    && existing_command
                        .bind(self.py())
                        .borrow()
                        .handler
                        .bind(self.py())
                        .eq(handler)
                        .unwrap_or(false)
            })
            .map_or(Ok(()), |command| {
                COMMANDS.load().as_ref().map_or(
                    Err(PyEnvironmentError::new_err(
                        "could not get access to commands",
                    )),
                    |commands| {
                        commands
                            .bind(self.py())
                            .remove_command(command.bind(self.py()))
                    },
                )
            })?;

        self.try_borrow().map_or(
            Err(PyEnvironmentError::new_err("cound not borrow plugin hooks")),
            |plugin| {
                plugin.commands.write().retain(|existing_command| {
                    existing_command.bind(self.py()).borrow().name.len() != names.len()
                        || !existing_command
                            .bind(self.py())
                            .borrow()
                            .name
                            .iter()
                            .all(|name| names.contains(name))
                        || existing_command
                            .bind(self.py())
                            .borrow()
                            .handler
                            .bind(self.py())
                            .ne(handler)
                            .unwrap_or(true)
                });
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod plugin_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::{
        IntoPyObjectExt,
        exceptions::{PyEnvironmentError, PyRuntimeError, PyValueError},
        types::{
            IntoPyDict, PyBool, PyDate, PyDict, PyFloat, PyInt, PyList, PySet, PyString, PyTuple,
        },
    };
    use rstest::rstest;

    use super::PluginMethods;
    use crate::{
        ffi::{
            c::prelude::*,
            python::{
                BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, COMMANDS, CONSOLE_CHANNEL, EVENT_DISPATCHERS,
                RED_TEAM_CHAT_CHANNEL, commands::CommandPriorities,
                events::EventDispatcherManagerMethods, prelude::*, pyshinqlx_test_support::*,
            },
        },
        hooks::mock_hooks::{
            shinqlx_com_printf_context, shinqlx_drop_client_context,
            shinqlx_send_server_command_context,
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn plugin_can_be_subclassed_from_python(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let extended_plugin = test_plugin(py);

            let result = extended_plugin.call0();

            assert!(result.is_ok());
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn plugin_can_be_traversed_for_garbage_collector(_pyshinqlx_setup: ()) {
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
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let plugin_instance = test_plugin(py)
                        .call0()
                        .expect("could not create plugin instance");

                    plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .add_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("this should not happen");

                    let result = py.import("gc").and_then(|gc| gc.call_method0("collect"));
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn str_returns_plugin_typename(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let plugin_instance = test_plugin(py).call0().expect("this should not happen");

            let plugin_str = plugin_instance.str();

            assert!(plugin_str.is_ok_and(|plugin_name| plugin_name == "test_plugin"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_db_when_no_db_type_set(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let extended_plugin = test_plugin(py);
            py.get_type::<Plugin>()
                .delattr("database")
                .expect("this should not happen");
            let plugin_instance = extended_plugin.call0().expect("this should not happen");

            let result = plugin_instance
                .downcast()
                .expect("this should not happen")
                .get_db();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyRuntimeError>(py)),);
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_db_when_no_db_set(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let extended_plugin = test_plugin(py);
            py.get_type::<Plugin>()
                .setattr("database", py.None())
                .expect("this should not happen");
            let plugin_instance = extended_plugin.call0().expect("this should not happen");

            let result = plugin_instance
                .downcast()
                .expect("this should not happen")
                .get_db();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyRuntimeError>(py)),);
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_db_when_db_set_to_redis(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let extended_plugin = test_plugin(py);
            let redis_type = py.get_type::<Redis>();
            py.get_type::<Plugin>()
                .setattr("database", redis_type.as_ref())
                .expect("this should not happen");
            let plugin_instance = extended_plugin.call0().expect("this should not happen");

            let result = plugin_instance
                .downcast()
                .expect("this should not happen")
                .get_db();

            assert!(result.is_ok_and(|db| db.is_instance(&redis_type).unwrap()));
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn name_property_returns_plugin_typename(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let plugin_instance = test_plugin(py).call0().expect("this should not happen");

            let plugin_str = plugin_instance
                .downcast::<Plugin>()
                .expect("this should not happen")
                .get_name();

            assert!(plugin_str.is_ok_and(|plugin_name| plugin_name == "test_plugin"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn plugins_property_returns_loaded_plugins(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let extended_plugin = test_plugin(py);

            let loaded_plugins = PyDict::new(py);
            loaded_plugins
                .set_item("asdf", "asdfplugin")
                .expect("this should not happen");
            loaded_plugins
                .set_item("qwertz", "qwertzplugin")
                .expect("this should not happen");
            py.get_type::<Plugin>()
                .setattr("_loaded_plugins", &loaded_plugins)
                .expect("this should not happen");
            let plugin_instance = extended_plugin.call0().expect("this should not happen");

            let plugins = plugin_instance
                .downcast::<Plugin>()
                .expect("this should not happen")
                .get_plugins();

            assert!(plugins.is_ok_and(|plugins_dict| {
                plugins_dict
                    .eq(loaded_plugins)
                    .expect("this should not happen")
            }));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn hooks_property_returns_plugin_hooks(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let plugin = Bound::new(
                py,
                Plugin {
                    hooks: vec![
                        ("asdf".to_string(), py.None(), 1),
                        ("qwertz".to_string(), py.None(), 0),
                    ]
                    .into(),
                    commands: Default::default(),
                    db_instance: py.None().into(),
                },
            )
            .expect("this should not happen");

            let hooks = plugin.get_hooks();
            assert_eq!(hooks.len(), 2);
            let elem1 = hooks.first();
            assert!(elem1.is_some_and(|(hook1, pyobj1, prio1)| hook1 == "asdf"
                && pyobj1.bind(py).is_none()
                && *prio1 == 1));
            let elem2 = hooks.get(1);
            assert!(elem2.is_some_and(|(hook2, pyobj2, prio2)| hook2 == "qwertz"
                && pyobj2.bind(py).is_none()
                && *prio2 == 0));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn commands_property_when_no_commands_exist(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let plugin = Bound::new(
                py,
                Plugin {
                    hooks: Default::default(),
                    commands: Default::default(),
                    db_instance: py.None().into(),
                },
            )
            .expect("this should not happen");

            assert_eq!(plugin.get_commands().len(), 0);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_property_when_no_game_running(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let plugin = Bound::new(
                py,
                Plugin {
                    hooks: Default::default(),
                    commands: Default::default(),
                    db_instance: py.None().into(),
                },
            )
            .expect("this should not happen");

            assert!(plugin.get_game().is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_property_when_a_game_exists(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "asdf", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let plugin = Bound::new(
                        py,
                        Plugin {
                            hooks: Default::default(),
                            commands: Default::default(),
                            db_instance: py.None().into(),
                        },
                    )
                    .expect("this should not happen");

                    assert!(plugin.get_game().is_some());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_logger(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let logger_type = py
                .import("logging")
                .expect("this should not happen")
                .getattr("Logger")
                .expect("could not get logging.Logger");

            let plugin_instance = test_plugin(py).call0().expect("this should not happen");

            let result = plugin_instance
                .downcast::<Plugin>()
                .expect("this should not happen")
                .get_logger();

            assert!(result.is_ok_and(|logger| {
                logger.is_instance(&logger_type).unwrap()
                    && logger
                        .getattr("name")
                        .expect("could not get logger name")
                        .str()
                        .unwrap()
                        == "shinqlx.test_plugin"
            }),);
        })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_hook_with_no_event_dispatchers(_pyshinqlx_setup: ()) {
        EVENT_DISPATCHERS.store(None);

        Python::with_gil(|py| {
            let plugin_instance = test_plugin(py)
                .call0()
                .expect("could not create plugin instance");

            let result = plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .add_hook(
                    "team_switch",
                    py.None().bind(py),
                    CommandPriorities::PRI_NORMAL as i32,
                );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_hook_adds_hook_to_plugin_hooks(_pyshinqlx_setup: ()) {
        let cvar_string = c"0";
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
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let plugin_instance = test_plugin(py)
                        .call0()
                        .expect("could not create plugin instance");

                    let result = plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .add_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_NORMAL as i32,
                        );
                    assert!(result.is_ok());
                    assert_eq!(
                        plugin_instance
                            .getattr("hooks")
                            .expect("could not get hooks")
                            .downcast::<PyList>()
                            .expect("could not downcast to list")
                            .len(),
                        1
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_hook_adds_hook_to_event_dispatchers(_pyshinqlx_setup: ()) {
        let cvar_string = c"0";
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
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let plugin_instance = test_plugin(py)
                        .call0()
                        .expect("could not create plugin instance");

                    let result = plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .add_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_NORMAL as i32,
                        );

                    assert!(result.is_ok());
                    assert!(
                        EVENT_DISPATCHERS
                            .load()
                            .as_ref()
                            .expect("could not get access to event dispatchers")
                            .getattr(py, "_dispatchers")
                            .expect("could not get dispatchers")
                            .downcast_bound::<PyDict>(py)
                            .expect("could not downcast to dict")
                            .get_item("team_switch_attempt")
                            .expect("could not get team switch attempt dispatcher")
                            .is_some_and(|team_switch_attempt_dispatcher| {
                                team_switch_attempt_dispatcher
                                    .getattr("plugins")
                                    .expect("could not get plugins")
                                    .downcast::<PyDict>()
                                    .expect("could not downcast to dict")
                                    .get_item("test_plugin")
                                    .is_ok_and(|opt_plugin| {
                                        opt_plugin.is_some_and(|plugin| {
                                            plugin
                                                .get_item(CommandPriorities::PRI_NORMAL as i32)
                                                .is_ok_and(|normal_hooks| {
                                                    normal_hooks.len().is_ok_and(|len| len == 1)
                                                })
                                        })
                                    })
                            })
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_hook_with_no_event_dispatchers(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            EVENT_DISPATCHERS.store(None);
            let plugin_instance = test_plugin(py)
                .call0()
                .expect("could not create plugin instance");

            let result = plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .remove_hook(
                    "team_switch",
                    py.None().bind(py),
                    CommandPriorities::PRI_NORMAL as i32,
                );

            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_hook_removes_hook_from_event_dispatchers(_pyshinqlx_setup: ()) {
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
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let plugin_instance = test_plugin(py)
                        .call0()
                        .expect("could not create plugin instance");

                    plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .add_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("could not add command");

                    let result = plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .remove_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_NORMAL as i32,
                        );

                    assert!(result.is_ok());
                    assert!(
                        EVENT_DISPATCHERS
                            .load()
                            .as_ref()
                            .expect("could not get access to event dispatchers")
                            .getattr(py, "_dispatchers")
                            .expect("could not get dispatchers")
                            .downcast_bound::<PyDict>(py)
                            .expect("could not downcast to dict")
                            .get_item("team_switch_attempt")
                            .expect("could not get team switch attempt dispatcher")
                            .is_some_and(|team_switch_attempt_dispatcher| {
                                team_switch_attempt_dispatcher
                                    .getattr("plugins")
                                    .expect("could not get plugins")
                                    .downcast::<PyDict>()
                                    .expect("could not downcast to dict")
                                    .get_item("test_plugin")
                                    .is_ok_and(|opt_plugin| {
                                        opt_plugin.is_some_and(|plugin| {
                                            plugin
                                                .get_item(CommandPriorities::PRI_NORMAL as i32)
                                                .is_ok_and(|normal_hooks| {
                                                    normal_hooks.len().is_ok_and(|len| len == 0)
                                                })
                                        })
                                    })
                            })
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_hook_removes_hook_from_plugin_instance(_pyshinqlx_setup: ()) {
        let cvar_string = c"0";
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
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let plugin_instance = test_plugin(py)
                        .call0()
                        .expect("could not create plugin instance");

                    plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .add_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("could not add command");

                    let result = plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .remove_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_NORMAL as i32,
                        );

                    assert!(result.is_ok());
                    assert!(
                        plugin_instance
                            .getattr("hooks")
                            .expect("could not get hooks")
                            .downcast::<PyList>()
                            .expect("could not downcast to list")
                            .is_empty()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_hook_removes_hook_when_other_hook_with_different_priority_exists(
        _pyshinqlx_setup: (),
    ) {
        let cvar_string = c"0";
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
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchAttemptDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let plugin_instance = test_plugin(py)
                        .call0()
                        .expect("could not create plugin instance");

                    plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .add_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_NORMAL as i32,
                        )
                        .expect("could not add command");

                    plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .add_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_HIGH as i32,
                        )
                        .expect("could not add command");

                    let result = plugin_instance
                        .downcast::<Plugin>()
                        .expect("could not downcast instance to plugin")
                        .remove_hook(
                            "team_switch_attempt",
                            py.None().bind(py),
                            CommandPriorities::PRI_NORMAL as i32,
                        );

                    assert!(result.is_ok());
                    assert_eq!(
                        plugin_instance
                            .getattr("hooks")
                            .expect("could not get hooks")
                            .downcast::<PyList>()
                            .expect("could not downcast to list")
                            .len(),
                        1
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_command_adds_a_new_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_handler = PyModule::from_code(
                py,
                cr#"
def handler():
    pass
            "#,
                c"",
                c"",
            )
            .expect("could not get module from code")
            .getattr("handler")
            .expect("could not get handler");
            let command_invoker = CommandInvoker::py_new();
            COMMANDS.store(Some(
                Py::new(py, command_invoker)
                    .expect("could not create command invoker in python")
                    .into(),
            ));

            let plugin_instance = test_plugin(py)
                .call0()
                .expect("could not create plugin instance");

            let result = plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .add_command(
                    PyString::new(py, "slap").as_any(),
                    &command_handler,
                    0,
                    None,
                    None,
                    CommandPriorities::PRI_NORMAL as u32,
                    false,
                    0,
                    true,
                    "",
                );

            assert!(result.is_ok());
            assert!(
                COMMANDS
                    .load()
                    .as_ref()
                    .expect("could not get access to commands")
                    .getattr(py, "commands")
                    .expect("could not get commands")
                    .downcast_bound::<PyList>(py)
                    .expect("could not downcast to list")
                    .get_item(0)
                    .expect("could not get first command")
                    .getattr("name")
                    .expect("could not get name attr")
                    .get_item(0)
                    .expect("could not get first name of command")
                    .str()
                    .is_ok_and(|value| value == "slap")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_command_stores_command_in_plugin(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_handler = PyModule::from_code(
                py,
                cr#"
def handler():
    pass
            "#,
                c"",
                c"",
            )
            .expect("could not get module from code")
            .getattr("handler")
            .expect("could not get handler");
            let command_invoker = CommandInvoker::py_new();
            COMMANDS.store(Some(
                Py::new(py, command_invoker)
                    .expect("could not create command invoker in python")
                    .into(),
            ));

            let plugin_instance = test_plugin(py)
                .call0()
                .expect("could not create plugin instance");

            let result = plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .add_command(
                    PyString::new(py, "slap").as_any(),
                    &command_handler,
                    0,
                    None,
                    None,
                    CommandPriorities::PRI_NORMAL as u32,
                    false,
                    0,
                    true,
                    "",
                );

            assert!(result.is_ok());
            assert_eq!(
                plugin_instance
                    .getattr("commands")
                    .expect("could not get commands")
                    .downcast::<PyList>()
                    .expect("could not downcast to list")
                    .len(),
                1
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_command_removes_command(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_handler = PyModule::from_code(
                py,
                cr#"
def handler():
    pass
            "#,
                c"",
                c"",
            )
            .expect("could not get module from code")
            .getattr("handler")
            .expect("could not get handler");
            let command_invoker = CommandInvoker::py_new();
            COMMANDS.store(Some(
                Py::new(py, command_invoker)
                    .expect("could not create command invoker in python")
                    .into(),
            ));

            let plugin_instance = test_plugin(py)
                .call0()
                .expect("could not create plugin instance");

            plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .add_command(
                    PyString::new(py, "slap").as_any(),
                    &command_handler,
                    0,
                    None,
                    None,
                    CommandPriorities::PRI_NORMAL as u32,
                    false,
                    0,
                    true,
                    "",
                )
                .expect("could not add command");

            let result = plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .remove_command(PyString::new(py, "slap").as_any(), &command_handler);

            assert!(result.is_ok());
            assert!(
                COMMANDS
                    .load()
                    .as_ref()
                    .expect("could not get access to commands")
                    .getattr(py, "commands")
                    .expect("could not get commands")
                    .downcast_bound::<PyList>(py)
                    .expect("could not downcast to list")
                    .is_empty()
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_command_removes_command_with_other_cmd_left_in_place(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_handler = PyModule::from_code(
                py,
                cr#"
def handler():
    pass
            "#,
                c"",
                c"",
            )
            .expect("could not get module from code")
            .getattr("handler")
            .expect("could not get handler");
            let command_invoker = CommandInvoker::py_new();
            COMMANDS.store(Some(
                Py::new(py, command_invoker)
                    .expect("could not create command invoker in python")
                    .into(),
            ));

            let plugin_instance = test_plugin(py)
                .call0()
                .expect("could not create plugin instance");

            plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .add_command(
                    PyString::new(py, "slap").as_any(),
                    &command_handler,
                    0,
                    None,
                    None,
                    CommandPriorities::PRI_NORMAL as u32,
                    false,
                    0,
                    true,
                    "",
                )
                .expect("could not add command");

            plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .add_command(
                    PyTuple::new(py, ["slay", "asdf"])
                        .expect("this should not happen")
                        .as_any(),
                    &command_handler,
                    0,
                    None,
                    None,
                    CommandPriorities::PRI_NORMAL as u32,
                    false,
                    0,
                    true,
                    "",
                )
                .expect("could not add command");

            let result = plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .remove_command(
                    PyTuple::new(py, ["slay", "asdf"])
                        .expect("this should not happen")
                        .as_any(),
                    &command_handler,
                );

            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("could not run all frame tasks");
            assert_eq!(
                COMMANDS
                    .load()
                    .as_ref()
                    .expect("could not get access to commands")
                    .getattr(py, "commands")
                    .expect("could not get commands")
                    .downcast_bound::<PyList>(py)
                    .expect("could not downcast to list")
                    .len(),
                1
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_command_for_list_of_command_names(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_handler = PyModule::from_code(
                py,
                cr#"
def handler():
    pass
            "#,
                c"",
                c"",
            )
            .expect("could not get module from code")
            .getattr("handler")
            .expect("could not get handler");
            let command_invoker = CommandInvoker::py_new();
            COMMANDS.store(Some(
                Py::new(py, command_invoker)
                    .expect("could not create command invoker in python")
                    .into(),
            ));

            let plugin_instance = test_plugin(py)
                .call0()
                .expect("could not create plugin instance");

            plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .add_command(
                    PyTuple::new(py, ["slay", "asdf"])
                        .expect("this should not happen")
                        .as_any(),
                    &command_handler,
                    0,
                    None,
                    None,
                    CommandPriorities::PRI_NORMAL as u32,
                    false,
                    0,
                    true,
                    "",
                )
                .expect("could not add command");

            let result = plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .remove_command(
                    PyTuple::new(py, ["slay", "asdf"])
                        .expect("this should not happen")
                        .as_any(),
                    &command_handler,
                );

            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("could not run all frame tasks");
            assert!(
                COMMANDS
                    .load()
                    .as_ref()
                    .expect("could not get access to commands")
                    .getattr(py, "commands")
                    .expect("could not get commands")
                    .downcast_bound::<PyList>(py)
                    .expect("could not downcast to list")
                    .is_empty(),
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn remove_command_removes_command_in_plugin_instance(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let command_handler = PyModule::from_code(
                py,
                cr#"
def handler():
    pass
            "#,
                c"",
                c"",
            )
            .expect("could not get module from code")
            .getattr("handler")
            .expect("could not get handler");
            let command_invoker = CommandInvoker::py_new();
            COMMANDS.store(Some(
                Py::new(py, command_invoker)
                    .expect("could not create command invoker in python")
                    .into(),
            ));

            let plugin_instance = test_plugin(py)
                .call0()
                .expect("could not create plugin instance");

            plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .add_command(
                    PyString::new(py, "slap").as_any(),
                    &command_handler,
                    0,
                    None,
                    None,
                    CommandPriorities::PRI_NORMAL as u32,
                    false,
                    0,
                    true,
                    "",
                )
                .expect("could not add command");

            let result = plugin_instance
                .downcast::<Plugin>()
                .expect("could not downcast instance to plugin")
                .remove_command(PyString::new(py, "slap").as_any(), &command_handler);

            assert!(result.is_ok());
            assert!(
                plugin_instance
                    .getattr("commands")
                    .expect("could not get commands")
                    .downcast::<PyList>()
                    .expect("could not downcast to list")
                    .is_empty(),
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::get_cvar(&py.get_type::<Plugin>(), "sv_maxclients", None);
            assert!(result.is_ok_and(|value| value.is_none()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_cvar_not_found(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "asdf", |_| None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::get_cvar(&py.get_type::<Plugin>(), "asdf", None);
                    assert!(result.expect("result was not OK").is_none());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_when_cvar_is_found(_pyshinqlx_setup: ()) {
        let cvar_string = c"16";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::get_cvar(&py.get_type::<Plugin>(), "sv_maxclients", None);
                    assert!(
                        result
                            .expect("result was not OK")
                            .extract::<String>()
                            .is_ok_and(|value| value == "16")
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_str(_pyshinqlx_setup: ()) {
        let cvar_string = c"16";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyString>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );
                    assert!(
                        result
                            .expect("result was not OK")
                            .extract::<String>()
                            .is_ok_and(|value| value == "16")
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_str_when_no_cvar_found(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyString>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );
                    assert!(result.expect("result was not OK").is_none());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_int(_pyshinqlx_setup: ()) {
        let cvar_string = c"16";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyInt>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );
                    assert!(
                        result
                            .expect("result was not OK")
                            .extract::<i32>()
                            .is_ok_and(|value| value == 16)
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_int_when_cvar_cannot_be_converted_to_int(_pyshinqlx_setup: ()) {
        let cvar_string = c"asdf";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyInt>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_int_when_no_cvar_found(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyInt>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );
                    assert!(result.expect("result was not OK").is_none());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_float(_pyshinqlx_setup: ()) {
        let cvar_string = c"16";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyFloat>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );
                    assert!(
                        result
                            .expect("result was not OK")
                            .extract::<f64>()
                            .is_ok_and(|value| value == 16.0)
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_float_when_cvar_cannot_be_converted_to_float(_pyshinqlx_setup: ()) {
        let cvar_string = c"asdf";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyFloat>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_float_when_no_cvar_found(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyFloat>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );
                    assert!(result.expect("result was not OK").is_none());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_bool(_pyshinqlx_setup: ()) {
        let cvar_string = c"16";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyBool>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    )
                    .expect("result was not OK");
                    assert!(
                        result.is_instance_of::<PyBool>()
                            && result.extract::<bool>().is_ok_and(|value| value)
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_bool_when_no_cvar_found(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyBool>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    )
                    .expect("result was not OK");

                    assert!(
                        result.is_instance_of::<PyBool>()
                            && result.extract::<bool>().is_ok_and(|value| !value)
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_bool_when_cvar_cannot_be_converted_to_int(_pyshinqlx_setup: ()) {
        let cvar_string = c"asdf";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyBool>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );

                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_list(_pyshinqlx_setup: ()) {
        let cvar_string = c"2, 4, 6, 8, 10";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyList>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    )
                    .expect("result was not OK");
                    assert!(
                        result.is_instance_of::<PyList>()
                            && result
                                .downcast::<PyList>()
                                .is_ok_and(|value| value.len() == 5)
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_list_when_no_cvar_found(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyList>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    )
                    .expect("result was not OK");

                    assert!(
                        result.is_instance_of::<PyList>()
                            && result
                                .downcast::<PyList>()
                                .is_ok_and(|value| value.is_empty())
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_set(_pyshinqlx_setup: ()) {
        let cvar_string = c"2, 4, 6, 8, 10";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PySet>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    )
                    .expect("result was not OK");
                    assert!(
                        result.is_instance_of::<PySet>()
                            && result
                                .downcast::<PySet>()
                                .is_ok_and(|value| value.len() == 5)
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_set_when_no_cvar_found(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PySet>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    )
                    .expect("result was not OK");

                    assert!(
                        result.is_instance_of::<PySet>()
                            && result
                                .downcast::<PySet>()
                                .is_ok_and(|value| value.is_empty())
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_converts_to_tuple(_pyshinqlx_setup: ()) {
        let cvar_string = c"2, 4, 6, 8, 10";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyTuple>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    )
                    .expect("result was not OK");
                    assert!(
                        result.is_instance_of::<PyTuple>()
                            && result
                                .downcast::<PyTuple>()
                                .is_ok_and(|value| value.len() == 5)
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_tuple_when_no_cvar_found(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyTuple>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    )
                    .expect("result was not OK");

                    assert!(
                        result.is_instance_of::<PyTuple>()
                            && result
                                .downcast::<PyTuple>()
                                .is_ok_and(|value| value.is_empty())
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_with_invalid_type_conversion(_pyshinqlx_setup: ()) {
        let cvar_string = c"16";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let py_str_type = py.get_type::<PyDate>();
                    let result = Plugin::get_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        Some(py_str_type.unbind()),
                    );

                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::set_cvar(
                &py.get_type::<Plugin>(),
                "sv_maxclients",
                PyString::new(py, "64").as_any(),
                0,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_for_not_existing_cvar(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .with(
                        predicate::eq("sv_maxclients"),
                        predicate::eq("64"),
                        predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
                    )
                    .times(1);
            })
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::set_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        PyString::new(py, "64").as_any(),
                        cvar_flags::CVAR_ROM as i32,
                    )
                });
                assert_eq!(result.expect("result was not OK"), true);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_for_already_existing_cvar(_pyshinqlx_setup: ()) {
        let mut raw_cvar = CVarBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_execute_console_command(r#"sv_maxclients "64""#, 1)
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::set_cvar(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        PyString::new(py, "64").as_any(),
                        cvar_flags::CVAR_ROM as i32,
                    )
                });
                assert_eq!(result.expect("result was not OK"), false);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::set_cvar_limit(
                &py.get_type::<Plugin>(),
                "sv_maxclients",
                PyInt::new(py, 64i32).as_any(),
                PyInt::new(py, 1i32).as_any(),
                PyInt::new(py, 64i32).as_any(),
                0,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_forwards_parameters_to_main_engine_call(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .configure(|mock_engine| {
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
            })
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::set_cvar_limit(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        PyInt::new(py, 64i32).as_any(),
                        PyInt::new(py, 1i32).as_any(),
                        PyInt::new(py, 64i32).as_any(),
                        cvar_flags::CVAR_CHEAT as i32,
                    )
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_once_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::set_cvar_once(
                &py.get_type::<Plugin>(),
                "sv_maxclients",
                PyString::new(py, "64").as_any(),
                0,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_once_for_not_existing_cvar(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .with(
                        predicate::eq("sv_maxclients"),
                        predicate::eq("64"),
                        predicate::eq(Some(cvar_flags::CVAR_ROM as i32)),
                    )
                    .times(1);
            })
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::set_cvar_once(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        PyInt::new(py, 64i32).as_any(),
                        cvar_flags::CVAR_ROM as i32,
                    )
                })
                .unwrap();
                assert_eq!(result, true);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_once_for_already_existing_cvar(_pyshinqlx_setup: ()) {
        let mut raw_cvar = CVarBuilder::default().build().unwrap();

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .configure(|mock_engine| {
                mock_engine.expect_get_cvar().times(0);
            })
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::set_cvar_once(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        PyString::new(py, "64").as_any(),
                        cvar_flags::CVAR_ROM as i32,
                    )
                })
                .unwrap();
                assert_eq!(result, false);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_once_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::set_cvar_limit_once(
                &py.get_type::<Plugin>(),
                "sv_maxclients",
                PyString::new(py, "64").as_any(),
                PyString::new(py, "1").as_any(),
                PyString::new(py, "64").as_any(),
                0,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_once_when_no_previous_value_is_set(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1..9)
            .configure(|mock_engine| {
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
            })
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::set_cvar_limit_once(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        PyString::new(py, "64").as_any(),
                        PyString::new(py, "1").as_any(),
                        PyString::new(py, "64").as_any(),
                        cvar_flags::CVAR_CHEAT as i32,
                    )
                });
                assert!(result.is_ok_and(|value| value));
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_once_for_already_existing_cvar(_pyshinqlx_setup: ()) {
        let mut raw_cvar = CVarBuilder::default().build().unwrap();

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "sv_maxclients",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1,
            )
            .configure(|mock_engine| {
                mock_engine.expect_set_cvar_limit().times(0);
            })
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::set_cvar_limit_once(
                        &py.get_type::<Plugin>(),
                        "sv_maxclients",
                        PyString::new(py, "64").as_any(),
                        PyString::new(py, "1").as_any(),
                        PyString::new(py, "64").as_any(),
                        cvar_flags::CVAR_ROM as i32,
                    )
                })
                .unwrap();
                assert_eq!(result, false);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn all_players_for_existing_clients(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default().with_max_clients(3).run(|| {
            let all_players = Python::with_gil(|py| Plugin::players(&py.get_type::<Plugin>()));
            assert_eq!(
                all_players.expect("result was not ok"),
                vec![
                    Player {
                        id: 0,
                        player_info: PlayerInfo {
                            client_id: 0,
                            name: "Mocked Player".into(),
                            connection_state: clientState_t::CS_ACTIVE as i32,
                            userinfo: "asdf".into(),
                            steam_id: 1234,
                            team: team_t::TEAM_RED as i32,
                            ..default_test_player_info()
                        }
                        .into(),
                        user_info: "asdf".into(),
                        steam_id: 1234,
                        name: "Mocked Player".to_string().into(),
                        ..default_test_player()
                    },
                    Player {
                        id: 2,
                        player_info: PlayerInfo {
                            client_id: 2,
                            name: "Mocked Player".into(),
                            connection_state: clientState_t::CS_ACTIVE as i32,
                            userinfo: "asdf".into(),
                            steam_id: 1234,
                            team: team_t::TEAM_RED as i32,
                            ..default_test_player_info()
                        }
                        .into(),
                        user_info: "asdf".into(),
                        steam_id: 1234,
                        name: "Mocked Player".to_string().into(),
                        ..default_test_player()
                    },
                ]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::player(
                &py.get_type::<Plugin>(),
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
                None,
            );
            assert!(
                result
                    .expect("result was not ok")
                    .is_some_and(|result_player| default_test_player() == result_player)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_for_player_id(_pyshinqlx_setup: ()) {
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
                Python::with_gil(|py| {
                    let result = Plugin::player(
                        &py.get_type::<Plugin>(),
                        PyInt::new(py, 42i32).as_any(),
                        None,
                    );
                    assert!(result.expect("result was not ok").is_some_and(|player| {
                        player
                            == Player {
                                id: 42,
                                name: "Mocked Player".to_string().into(),
                                steam_id: 1234,
                                user_info: "asdf".into(),
                                player_info: PlayerInfo {
                                    client_id: 42,
                                    name: "Mocked Player".into(),
                                    team: team_t::TEAM_RED as i32,
                                    steam_id: 1234,
                                    userinfo: "asdf".into(),
                                    connection_state: clientState_t::CS_ACTIVE as i32,
                                    ..default_test_player_info()
                                }
                                .into(),
                                ..default_test_player()
                            }
                    }));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_steam_id_from_player_list(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Player {
                steam_id: 1234,
                player_info: PlayerInfo {
                    steam_id: 1234,
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            };
            let result = Plugin::player(
                &py.get_type::<Plugin>(),
                PyInt::new(py, 1234i32).as_any(),
                Some(vec![player.clone()]),
            );
            assert!(
                result
                    .expect("result was not ok")
                    .is_some_and(|result_player| result_player == player)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_steam_id_not_in_provided_player_list(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::player(
                &py.get_type::<Plugin>(),
                PyInt::new(py, 4321i32).as_any(),
                Some(vec![default_test_player()]),
            );
            assert!(result.expect("result was not ok").is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_name_from_player_list(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Player {
                name: "Mocked Player".to_string().into(),
                player_info: PlayerInfo {
                    name: "Mocked Player".into(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            };
            let result = Plugin::player(
                &py.get_type::<Plugin>(),
                PyString::new(py, "Mocked Player").as_any(),
                Some(vec![player.clone()]),
            );
            assert!(
                result
                    .expect("result was not ok")
                    .is_some_and(|result_player| result_player == player)
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn player_for_provided_name_not_in_provided_player_list(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::player(
                &py.get_type::<Plugin>(),
                PyString::new(py, "disconnected").as_any(),
                Some(vec![default_test_player()]),
            );
            assert!(result.expect("result was not ok").is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn msg_for_invalid_channel(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::msg(
                &py.get_type::<Plugin>(),
                "asdf",
                Some(PyString::new(py, "asdf").into_any()),
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_default_channel(_pyshinqlx_setup: ()) {
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

            let result = Plugin::msg(&py.get_type::<Plugin>(), "asdf", None, None);
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_chat_channel_with_kwargs(_pyshinqlx_setup: ()) {
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
                &py.get_type::<Plugin>(),
                "asdf qwertz",
                Some(PyString::new(py, "chat").into_any()),
                Some(
                    &[
                        ("limit", PyInt::new(py, 23i32).as_any()),
                        ("delimiter", PyString::new(py, "_").as_any()),
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_red_team_chat_channel(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default().with_max_clients(8).run(|| {
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
                    &py.get_type::<Plugin>(),
                    "asdf qwertz",
                    Some(PyString::new(py, "red_team_chat").into_any()),
                    None,
                );
                assert!(result.is_ok());
                run_all_frame_tasks(py).expect("running frame tasks returned an error");
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_blue_team_chat_channel(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default().with_max_clients(8).run(|| {
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
                    &py.get_type::<Plugin>(),
                    "asdf qwertz",
                    Some(PyString::new(py, "blue_team_chat").into_any()),
                    None,
                );
                assert!(result.is_ok());
                run_all_frame_tasks(py).expect("running frame tasks returned an error");
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_console_channel(_pyshinqlx_setup: ()) {
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
                &py.get_type::<Plugin>(),
                "asdf",
                Some(PyString::new(py, "console").into_any()),
                None,
            );
            assert!(result.is_ok());
            run_all_frame_tasks(py).expect("running frame tasks returned an error");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn msg_for_provided_channel(_pyshinqlx_setup: ()) {
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

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "print \"asdf qwertz\n\"\n")
            .times(1);

        let channel = TellChannel::py_new(&default_test_player());

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = Plugin::msg(
                    &py.get_type::<Plugin>(),
                    "asdf qwertz",
                    Some(
                        Py::new(py, channel)
                            .expect("could not create tell channel")
                            .into_bound(py)
                            .into_any(),
                    ),
                    None,
                );
                assert!(result.is_ok());
                run_all_frame_tasks(py).expect("running frame tasks returned an error");
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn console_prints_to_console(_pyshinqlx_setup: ()) {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .with(predicate::eq("asdf\n"))
            .times(1);

        Python::with_gil(|py| {
            let result =
                Plugin::console(&py.get_type::<Plugin>(), PyString::new(py, "asdf").as_any());
            assert!(result.as_ref().is_ok(), "{:?}", result.as_ref());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn clean_text_cleans_text_from_color_tags(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result =
                Plugin::clean_text(&py.get_type::<Plugin>(), "^0a^1b^2c^3d^4e^5f^6g^7h^8i^9j");
            assert_eq!(result, "abcdefgh^8i^9j");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn colored_name_for_provided_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Player {
                name: "Mocked Player".to_string().into(),
                ..default_test_player()
            };
            let result = Plugin::colored_name(
                &py.get_type::<Plugin>(),
                Bound::new(py, player)
                    .expect("this should not happen")
                    .as_any(),
                None,
            );
            assert_eq!(result.expect("result was none"), "Mocked Player");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn colored_name_for_player_in_provided_playerlist(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Player {
                name: "Mocked Player".to_string().into(),
                ..default_test_player()
            };
            let result = Plugin::colored_name(
                &py.get_type::<Plugin>(),
                PyString::new(py, "Mocked Player").as_any(),
                Some(vec![player]),
            );
            assert_eq!(result.expect("result was none"), "Mocked Player");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn colored_name_for_player_with_colored_name_in_provided_playerlist(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                name: "^1Mocked ^4Player".to_string(),
                ..default_test_player_info()
            }
            .into(),
            name: "^1Mocked ^4Player".to_string().into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::colored_name(
                &py.get_type::<Plugin>(),
                PyString::new(py, "Mocked Player").as_any(),
                Some(vec![player]),
            );
            assert_eq!(result.expect("result was none"), "^1Mocked ^4Player");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn colored_name_for_unavailable_player(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                name: "^1Mocked ^4Player".to_string(),
                ..default_test_player_info()
            }
            .into(),
            name: "^1Mocked ^4Player".to_string().into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::colored_name(
                &py.get_type::<Plugin>(),
                PyString::new(py, "disconnected Player").as_any(),
                Some(vec![player]),
            );
            assert!(result.is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_integer_in_client_id_range(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type::<Plugin>(),
                PyInt::new(py, 42i32).as_any(),
                None,
            );
            assert!(result.is_some_and(|value| value == 42));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_player(_pyshinqlx_setup: ()) {
        let player = Player {
            id: 21,
            player_info: PlayerInfo {
                client_id: 21,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type::<Plugin>(),
                Bound::new(py, player)
                    .expect("this should not happen")
                    .as_any(),
                None,
            );
            assert!(result.is_some_and(|value| value == 21));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_steam_id(_pyshinqlx_setup: ()) {
        let player = Player {
            id: 21,
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 21,
                steam_id: 1234,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type::<Plugin>(),
                PyInt::new(py, 1234i64).as_any(),
                Some(vec![player]),
            );
            assert!(result.is_some_and(|value| value == 21));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_steam_id_not_in_player_list(_pyshinqlx_setup: ()) {
        let player = Player {
            steam_id: 1234,
            player_info: PlayerInfo {
                steam_id: 1234,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type::<Plugin>(),
                PyInt::new(py, 4321i64).as_any(),
                Some(vec![player]),
            );
            assert!(result.is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_player_name(_pyshinqlx_setup: ()) {
        let player = Player {
            id: 21,
            name: "Mocked Player".to_string().into(),
            player_info: PlayerInfo {
                client_id: 21,
                name: "Mocked Player".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type::<Plugin>(),
                PyString::new(py, "Mocked Player").as_any(),
                Some(vec![player]),
            );
            assert!(result.is_some_and(|value| value == 21));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_player_name_not_in_player_list(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                name: "Mocked Player".to_string(),
                ..default_test_player_info()
            }
            .into(),
            name: "Mocked Player".to_string().into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type::<Plugin>(),
                PyString::new(py, "UnknownPlayer").as_any(),
                Some(vec![player]),
            );
            assert!(result.is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn client_id_for_unsupported_search_criteria(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::client_id(
                &py.get_type::<Plugin>(),
                PyFloat::new(py, 3.42f64).as_any(),
                Some(vec![default_test_player()]),
            );
            assert!(result.is_none());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn find_player_with_empty_str_returns_player_list(_pyshinqlx_setup: ()) {
        let player1 = Player {
            id: 21,
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 21,
                steam_id: 1234,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let player2 = Player {
            id: 0,
            steam_id: 1235,
            player_info: PlayerInfo {
                client_id: 0,
                steam_id: 1235,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        Python::with_gil(|py| {
            let result = Plugin::find_player(
                &py.get_type::<Plugin>(),
                "",
                Some(vec![player1.clone(), player2.clone()]),
            );
            assert_eq!(result, vec![player1, player2]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn find_players_by_matching_provided_names(_pyshinqlx_setup: ()) {
        let player1 = Player {
            id: 21,
            name: "^1Found ^4Player".to_string().into(),
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 21,
                name: "^1Found ^4Player".to_string(),
                steam_id: 1234,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let player2 = Player {
            id: 0,
            name: "non-matching Player".to_string().into(),
            steam_id: 1235,
            player_info: PlayerInfo {
                client_id: 0,
                name: "non-matching Player".to_string(),
                steam_id: 1235,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let player3 = Player {
            id: 5,
            name: "found Player".to_string().into(),
            steam_id: 1236,
            player_info: PlayerInfo {
                client_id: 5,
                name: "found Player".to_string(),
                steam_id: 1236,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        Python::with_gil(|py| {
            let result = Plugin::find_player(
                &py.get_type::<Plugin>(),
                "foU^3nd",
                Some(vec![player1.clone(), player2.clone(), player3.clone()]),
            );
            assert_eq!(result, vec![player1, player3]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn find_players_when_no_player_matches(_pyshinqlx_setup: ()) {
        let player1 = Player {
            id: 21,
            name: "^1non-matching ^4Player".to_string().into(),
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 21,
                name: "^1non-matching ^4Player".to_string(),
                steam_id: 1234,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let player2 = Player {
            id: 0,
            name: "non-matching Player".to_string().into(),
            steam_id: 1235,
            player_info: PlayerInfo {
                client_id: 0,
                name: "non-matching Player".to_string(),
                steam_id: 1235,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let player3 = Player {
            id: 5,
            name: "non-matching Player".to_string().into(),
            steam_id: 1236,
            player_info: PlayerInfo {
                client_id: 5,
                name: "non-matching Player".to_string(),
                steam_id: 1236,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        Python::with_gil(|py| {
            let result = Plugin::find_player(
                &py.get_type::<Plugin>(),
                "no-such-player",
                Some(vec![player1, player2, player3]),
            );
            assert!(result.is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn teams_when_no_player_in_player_list(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::teams(&py.get_type::<Plugin>(), Some(vec![]));
            assert!(
                result
                    .expect("result was not ok")
                    .eq([
                        (PyString::new(py, "free").into_any(), PyList::empty(py)),
                        (PyString::new(py, "red").into_any(), PyList::empty(py)),
                        (PyString::new(py, "blue").into_any(), PyList::empty(py)),
                        (PyString::new(py, "spectator").into_any(), PyList::empty(py))
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"))
                    .expect("comparison was not ok"),
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn teams_when_every_team_has_one_player(_pyshinqlx_setup: ()) {
        let player1 = Player {
            id: 0,
            steam_id: 1234,
            player_info: PlayerInfo {
                client_id: 0,
                steam_id: 1234,
                team: team_t::TEAM_FREE as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let player2 = Player {
            id: 1,
            steam_id: 1235,
            player_info: PlayerInfo {
                client_id: 1,
                steam_id: 1235,
                team: team_t::TEAM_RED as i32,
                ..default_test_player_info()
            }
            .into(),
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
            }
            .into(),
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
            }
            .into(),
            ..default_test_player()
        };
        Python::with_gil(|py| {
            let result = Plugin::teams(
                &py.get_type::<Plugin>(),
                Some(vec![
                    player4.clone(),
                    player3.clone(),
                    player2.clone(),
                    player1.clone(),
                ]),
            );
            assert!(
                result
                    .expect("result was not ok")
                    .eq([
                        (
                            PyString::new(py, "free").into_any(),
                            vec![player1]
                                .into_bound_py_any(py)
                                .expect("this should not happen"),
                        ),
                        (
                            PyString::new(py, "red").into_any(),
                            vec![player2]
                                .into_bound_py_any(py)
                                .expect("this should not happen")
                        ),
                        (
                            PyString::new(py, "blue").into_any(),
                            vec![player3]
                                .into_bound_py_any(py)
                                .expect("this should not happen")
                        ),
                        (
                            PyString::new(py, "spectator").into_any(),
                            vec![player4]
                                .into_bound_py_any(py)
                                .expect("this should not happen")
                        )
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"))
                    .expect("comparison was not ok")
            );
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn center_print_to_all_players_sends_center_print_server_command(_pyshinqlx_setup: ()) {
        let send_server_cmd_ctx = shinqlx_send_server_command_context();
        send_server_cmd_ctx
            .expect()
            .withf(|recipients, cmd| recipients.is_none() && cmd == "cp \"asdf\"")
            .times(1);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result =
                Python::with_gil(|py| Plugin::center_print(&py.get_type::<Plugin>(), "asdf", None));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn center_print_to_paetticular_player_sends_center_print_server_command(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                Plugin::center_print(
                    &py.get_type::<Plugin>(),
                    "asdf",
                    Some(Bound::new(py, player)?.into_any()),
                )
            });
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tell_sends_msg_to_player(_pyshinqlx_setup: ()) {
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

        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_some() && cmd == "print \"asdf\n\"\n");

        MockGameEntityBuilder::default()
            .with_player_name(|| "Mocked Player".to_string(), 1..)
            .with_team(|| team_t::TEAM_RED, 1..)
            .with_privileges(|| privileges_t::PRIV_NONE, 1..)
            .run(predicate::eq(2), || {
                MockEngineBuilder::default().with_max_clients(16).run(|| {
                    Python::with_gil(|py| {
                        let result = Plugin::tell(
                            &py.get_type::<Plugin>(),
                            "asdf",
                            Bound::new(py, default_test_player())
                                .expect("this should not happen")
                                .as_any(),
                            None,
                        );
                        assert!(result.is_ok());
                        run_all_frame_tasks(py).expect("running frame tasks returned an error");
                    });
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_vote_active_when_configstring_set(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_STRING as u16, "vote is active", 1)
            .run(|| {
                Python::with_gil(|py| {
                    assert_eq!(Plugin::is_vote_active(&py.get_type::<Plugin>()), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_vote_active_when_configstring_empty(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
            .run(|| {
                Python::with_gil(|py| {
                    assert_eq!(Plugin::is_vote_active(&py.get_type::<Plugin>()), false);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_vote_active_when_main_engine_not_set(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            assert_eq!(Plugin::is_vote_active(&py.get_type::<Plugin>()), false);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_when_main_engine_not_set(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
            assert!(result.is_ok_and(|value| value.is_none()))
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_when_yes_votes_are_empty(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_YES as u16, "", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "42", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                    assert!(result.is_ok_and(|value| value.is_none()));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_when_no_votes_are_empty(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_YES as u16, "42", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                    assert!(result.is_ok_and(|value| value.is_none()));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_with_proper_vote_counts(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_YES as u16, "42", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "21", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                    assert!(
                        result
                            .expect("result was not ok")
                            .eq(PyTuple::new(py, vec![42, 21]).expect("this should not happen"))
                            .expect("comparison was not ok")
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_with_unparseable_yes_vote_counts(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_YES as u16, "asdf", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "21", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn current_vote_count_with_unparseable_no_vote_counts(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_YES as u16, "42", 1)
            .with_get_configstring(CS_VOTE_NO as u16, "asdf", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::current_vote_count(&py.get_type::<Plugin>());
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_when_vote_is_active(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_STRING as u16, "map overkill ca", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::callvote(
                        &py.get_type::<Plugin>(),
                        "map thunderstruck ca",
                        "map thunderstruck ca",
                        30,
                    );
                    assert!(result.is_ok_and(|value| !value));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_calls_vote(_pyshinqlx_setup: ()) {
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

        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<VoteStartedDispatcher>())
                        .expect("could not add vote_started dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = Plugin::callvote(
                        &py.get_type::<Plugin>(),
                        "map thunderstruck ca",
                        "map thunderstruck ca",
                        30,
                    );
                    assert!(result.is_ok_and(|value| value),);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn callvote_when_event_dispatcher_not_available(_pyshinqlx_setup: ()) {
        EVENT_DISPATCHERS.store(None);

        MockEngineBuilder::default()
            .with_get_configstring(CS_VOTE_STRING as u16, "", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::callvote(
                        &py.get_type::<Plugin>(),
                        "map thunderstruck ca",
                        "map thunderstruck ca",
                        30,
                    );
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_with_unparseable_value(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result =
                Plugin::force_vote(&py.get_type::<Plugin>(), PyString::new(py, "asdf").as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_forces_vote_passed(_pyshinqlx_setup: ()) {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

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

        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_vote_state()
                    .with(predicate::eq(true))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::eq(0), || {
                MockEngineBuilder::default().with_max_clients(1).run(|| {
                    Python::with_gil(|py| {
                        let result = Plugin::force_vote(
                            &py.get_type::<Plugin>(),
                            PyBool::new(py, true).as_any(),
                        );
                        assert!(result.is_ok_and(|value| value),);
                    });
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn force_vote_forces_vote_fail(_pyshinqlx_setup: ()) {
        let current_level_try_get_ctx = MockTestCurrentLevel::try_get_context();
        current_level_try_get_ctx.expect().returning(|| {
            let mut mock_level = MockTestCurrentLevel::new();
            mock_level.expect_get_vote_time().return_const(21);
            Ok(mock_level)
        });

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

        MockGameEntityBuilder::default()
            .with_game_client(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_vote_state()
                    .with(predicate::eq(false))
                    .times(1);
                Ok(mock_game_client)
            })
            .run(predicate::eq(0), || {
                MockEngineBuilder::default().with_max_clients(1).run(|| {
                    Python::with_gil(|py| {
                        let result = Plugin::force_vote(
                            &py.get_type::<Plugin>(),
                            PyBool::new(py, false).as_any(),
                        );
                        assert!(result.is_ok_and(|value| value),);
                    });
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn teamsize_sets_teamsize(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "teamsize", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "teamsize" && value == "42" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::teamsize(&py.get_type::<Plugin>(), 42);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_for_unknown_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::kick(
                &py.get_type::<Plugin>(),
                PyFloat::new(py, 1.23f64).as_any(),
                "",
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_for_existing_player_without_reason(_pyshinqlx_setup: ()) {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = Plugin::kick(
                    &py.get_type::<Plugin>(),
                    Bound::new(py, default_test_player())
                        .expect("this should not happen")
                        .as_any(),
                    "",
                );
                assert!(result.as_ref().is_ok(), "{:?}", result.as_ref());
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn kick_for_existing_player_with_reason(_pyshinqlx_setup: ()) {
        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = Plugin::kick(
                    &py.get_type::<Plugin>(),
                    Bound::new(py, default_test_player())
                        .expect("this should not happen")
                        .as_any(),
                    "All your base are belong to us!",
                );
                assert!(result.is_ok());
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn shuffle_forces_shuffle(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("forceshuffle", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::shuffle(&py.get_type::<Plugin>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn cointoss_does_nothing(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            Plugin::cointoss(&py.get_type::<Plugin>());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn change_map_with_no_factory(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("map thunderstruck", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result =
                        Plugin::change_map(&py.get_type::<Plugin>(), "thunderstruck", None);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn change_map_with_factory(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("map thunderstruck ffa", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result =
                        Plugin::change_map(&py.get_type::<Plugin>(), "thunderstruck", Some("ffa"));
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn change_map_when_no_main_engine_set(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::change_map(&py.get_type::<Plugin>(), "thunderstruck", Some("ffa"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn switch_with_invalid_player1(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::switch(
                &py.get_type::<Plugin>(),
                PyFloat::new(py, 1.23f64).as_any(),
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn switch_with_invalid_player2(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::switch(
                &py.get_type::<Plugin>(),
                Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .as_any(),
                PyFloat::new(py, 1.23f64).as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn switch_with_players_on_same_team(_pyshinqlx_setup: ()) {
        let player1 = Player {
            id: 0,
            player_info: PlayerInfo {
                client_id: 0,
                team: team_t::TEAM_RED as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let player2 = Player {
            id: 1,
            player_info: PlayerInfo {
                client_id: 1,
                team: team_t::TEAM_RED as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = Plugin::switch(
                &py.get_type::<Plugin>(),
                Bound::new(py, player1)
                    .expect("this should not happen")
                    .as_any(),
                Bound::new(py, player2)
                    .expect("this should not happen")
                    .as_any(),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn switch_with_players_on_different_team(_pyshinqlx_setup: ()) {
        let player1 = Player {
            id: 0,
            player_info: PlayerInfo {
                client_id: 0,
                team: team_t::TEAM_RED as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let player2 = Player {
            id: 1,
            player_info: PlayerInfo {
                client_id: 1,
                team: team_t::TEAM_BLUE as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default()
            .with_execute_console_command("put 0 blue", 1)
            .with_execute_console_command("put 1 red", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::switch(
                        &py.get_type::<Plugin>(),
                        Bound::new(py, player1)
                            .expect("this should not happen")
                            .as_any(),
                        Bound::new(py, player2)
                            .expect("this should not happen")
                            .as_any(),
                    );
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_sound_to_all_players(_pyshinqlx_setup: ()) {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "playSound sound/vo/midair.ogg")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::play_sound(&py.get_type::<Plugin>(), "sound/vo/midair.ogg", None);
            assert!(result.is_ok_and(|value| value),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_sound_to_a_specific_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = Plugin::play_sound(
                    &py.get_type::<Plugin>(),
                    "sound/vo/midair.ogg",
                    Some(player),
                );
                assert!(result.is_ok_and(|value| value),);
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_sound_with_empty_soundpath(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::play_sound(&py.get_type::<Plugin>(), "", None);
            assert!(result.is_ok_and(|value| !value),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_sound_for_sound_containing_music(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::play_sound(&py.get_type::<Plugin>(), "music/sonic1.ogg", None);
            assert!(result.is_ok_and(|value| !value),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_music_to_all_players(_pyshinqlx_setup: ()) {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "playMusic music/sonic1.ogg")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::play_music(&py.get_type::<Plugin>(), "music/sonic1.ogg", None);
            assert!(result.is_ok_and(|value| value),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_music_to_a_specific_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result =
                    Plugin::play_music(&py.get_type::<Plugin>(), "music/sonic1.ogg", Some(player));
                assert!(result.is_ok_and(|value| value),);
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_music_with_empty_musicpath(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::play_music(&py.get_type::<Plugin>(), "", None);
            assert!(result.is_ok_and(|value| !value),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn play_music_for_music_containing_sound(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::play_music(&py.get_type::<Plugin>(), "sound/vo/midair.ogg", None);
            assert!(result.is_ok_and(|value| !value),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stop_sound_for_all_players(_pyshinqlx_setup: ()) {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "clearSounds")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::stop_sound(&py.get_type::<Plugin>(), None);
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stop_sound_for_a_specific_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = Plugin::stop_sound(&py.get_type::<Plugin>(), Some(player));
                assert!(result.is_ok());
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stop_music_for_all_players(_pyshinqlx_setup: ()) {
        let send_server_command_ctx = shinqlx_send_server_command_context();
        send_server_command_ctx
            .expect()
            .withf(|client, cmd| client.is_none() && cmd == "stopMusic")
            .times(1);

        Python::with_gil(|py| {
            let result = Plugin::stop_music(&py.get_type::<Plugin>(), None);
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stop_music_for_a_specific_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::eq(2))
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = Plugin::stop_music(&py.get_type::<Plugin>(), Some(player));
                assert!(result.is_ok());
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slap_for_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::slap(&py.get_type::<Plugin>(), py.None().bind(py), 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slap_for_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("slap 21 42", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result =
                        Plugin::slap(&py.get_type::<Plugin>(), PyInt::new(py, 21i32).as_any(), 42);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_for_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::slay(&py.get_type::<Plugin>(), py.None().bind(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slay_for_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("slay 21", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let result =
                        Plugin::slay(&py.get_type::<Plugin>(), PyInt::new(py, 21i32).as_any());
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn timeout_pauses_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("timeout", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::timeout(&py.get_type::<Plugin>()));
                assert!(result.is_ok());
            })
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn timein_unpauses_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("timein", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::timein(&py.get_type::<Plugin>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn allready_readies_all_players(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("allready", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::allready(&py.get_type::<Plugin>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pause_pauses_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("pause", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::pause(&py.get_type::<Plugin>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unpause_unpauses_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("unpause", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::unpause(&py.get_type::<Plugin>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_with_invalid_team(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .configure(|_mock_engine| {})
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::lock(&py.get_type::<Plugin>(), Some("invalid_team"));
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_with_no_team(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("lock", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::lock(&py.get_type::<Plugin>(), None));
                assert!(result.is_ok());
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
    fn lock_a_specific_team(_pyshinqlx_setup: (), #[case] locked_team: &str) {
        MockEngineBuilder::default()
            .with_execute_console_command(format!("lock {}", locked_team.to_lowercase()), 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::lock(&py.get_type::<Plugin>(), Some(locked_team))
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_invalid_team(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .configure(|_mock_engine| {})
            .run(|| {
                Python::with_gil(|py| {
                    let result = Plugin::unlock(&py.get_type::<Plugin>(), Some("invalid_team"));
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_no_team(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("unlock", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::unlock(&py.get_type::<Plugin>(), None));
                assert!(result.is_ok());
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
    fn unlock_a_specific_team(_pyshinqlx_setup: (), #[case] locked_team: &str) {
        MockEngineBuilder::default()
            .with_execute_console_command(format!("unlock {}", locked_team.to_lowercase()), 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::unlock(&py.get_type::<Plugin>(), Some(locked_team))
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_team(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::put(
                &py.get_type::<Plugin>(),
                PyInt::new(py, 2i32).as_any(),
                "invalid team",
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::put(
                &py.get_type::<Plugin>(),
                PyInt::new(py, 2048i32).as_any(),
                "red",
            );
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
    fn put_put_player_on_a_specific_team(_pyshinqlx_setup: (), #[case] new_team: &str) {
        MockEngineBuilder::default()
            .with_execute_console_command(format!("put 2 {}", new_team.to_lowercase()), 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::put(
                        &py.get_type::<Plugin>(),
                        PyInt::new(py, 2i32).as_any(),
                        new_team,
                    )
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::mute(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_mutes_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("mute 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::mute(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::unmute(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_unmutes_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("unmute 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::unmute(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result =
                Plugin::tempban(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_tempbans_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("tempban 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::tempban(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::ban(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_bans_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("ban 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::ban(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::unban(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_unbans_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("unban 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::unban(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn opsay_sends_op_message(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("opsay asdf", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::opsay(&py.get_type::<Plugin>(), "asdf"));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result =
                Plugin::addadmin(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_adds_player_to_admins(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("addadmin 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::addadmin(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::addmod(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_adds_player_to_moderators(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("addmod 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::addmod(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::demote(&py.get_type::<Plugin>(), PyInt::new(py, 2048i32).as_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_demotes_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("demote 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::demote(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn abort_aborts_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("map_restart", 1)
            .run(|| {
                let result = Python::with_gil(|py| Plugin::abort(&py.get_type::<Plugin>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::addscore(
                &py.get_type::<Plugin>(),
                PyInt::new(py, 2048i32).as_any(),
                42,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_adds_score_to_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("addscore 2 42", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Plugin::addscore(&py.get_type::<Plugin>(), PyInt::new(py, 2i32).as_any(), 42)
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addteamscore_with_invalid_team(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Plugin::addteamscore(&py.get_type::<Plugin>(), "invalid_team", 42);
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
    fn addteamscore_adds_score_to_team(_pyshinqlx_setup: (), #[case] team: &str) {
        MockEngineBuilder::default()
            .with_execute_console_command(format!("addteamscore {} 42", team.to_lowercase()), 1)
            .run(|| {
                let result =
                    Python::with_gil(|py| Plugin::addteamscore(&py.get_type::<Plugin>(), team, 42));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setmatchtime_sets_match_time(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("setmatchtime 42", 1)
            .run(|| {
                let result =
                    Python::with_gil(|py| Plugin::setmatchtime(&py.get_type::<Plugin>(), 42));
                assert!(result.is_ok());
            });
    }
}
