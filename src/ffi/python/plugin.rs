use core::hint::cold_path;

use itertools::Itertools;
use pyo3::{
    BoundObject, PyTraverseError,
    exceptions::{PyEnvironmentError, PyRuntimeError, PyValueError},
    gc::PyVisit,
    intern,
    prelude::*,
    types::{PyBool, PyDict, PyFloat, PyInt, PyList, PySet, PyString, PyTuple, PyType},
};
use tap::TapOptional;

mod lookup;
use self::lookup::{TargetLookup, lookup_target, players_matching_name};
use super::{
    BLUE_TEAM_CHAT_CHANNEL, CHAT_CHANNEL, COMMANDS, CONSOLE_CHANNEL, CommandInvokerMethods,
    EVENT_DISPATCHERS, EventDispatcherMethods, RED_TEAM_CHAT_CHANNEL, Teams, addadmin, addmod,
    addscore, addteamscore, ban, client_id, commands::CommandPriorities, console_command, demote,
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

fn tell_caller(player: &Bound<'_, PyAny>, message: &str) -> PyResult<()> {
    if player.is_none() {
        return Ok(());
    }
    player
        .call_method1(intern!(player.py(), "tell"), (message,))
        .map(|_| ())
}

/// The types a cvar can be read as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CvarType {
    Str,
    Int,
    Float,
    Bool,
    List,
    Set,
    Tuple,
}

impl CvarType {
    /// The type for a Python type object. The builtin types are recognized by identity, other types
    /// by their `__name__`. Returns the name of the type as error if cvars cannot be read as it.
    pub(super) fn from_type(python_type: &Bound<'_, PyType>) -> Result<Self, String> {
        let py = python_type.py();
        if python_type.is(py.get_type::<PyString>()) {
            return Ok(Self::Str);
        }
        if python_type.is(py.get_type::<PyInt>()) {
            return Ok(Self::Int);
        }
        if python_type.is(py.get_type::<PyBool>()) {
            return Ok(Self::Bool);
        }
        if python_type.is(py.get_type::<PyFloat>()) {
            return Ok(Self::Float);
        }
        if python_type.is(py.get_type::<PyList>()) {
            return Ok(Self::List);
        }
        if python_type.is(py.get_type::<PySet>()) {
            return Ok(Self::Set);
        }
        if python_type.is(py.get_type::<PyTuple>()) {
            return Ok(Self::Tuple);
        }

        let type_name = python_type
            .getattr(intern!(py, "__name__"))
            .and_then(|value| value.extract::<String>())
            .unwrap_or_else(|_| "Python type without __name__".into());
        Self::from_name(&type_name).ok_or(type_name)
    }

    /// The type for the `__name__` of a Python type, if cvars can be read as it.
    pub(super) fn from_name(name: &str) -> Option<Self> {
        match name {
            "str" => Some(Self::Str),
            "int" => Some(Self::Int),
            "float" => Some(Self::Float),
            "bool" => Some(Self::Bool),
            "list" => Some(Self::List),
            "set" => Some(Self::Set),
            "tuple" => Some(Self::Tuple),
            _ => None,
        }
    }
}

/// Reads the current value of the cvar `name` and converts it to `cvar_type`.
pub(super) fn read_cvar_as<'py>(
    py: Python<'py>,
    name: &str,
    cvar_type: CvarType,
) -> PyResult<Bound<'py, PyAny>> {
    let cvar_string = MAIN_ENGINE
        .load()
        .as_ref()
        .and_then(|engine| engine.find_cvar(name))
        .map(|value| value.get_string());

    match cvar_type {
        CvarType::Str => match cvar_string {
            None => Ok(py.None().into_bound(py)),
            Some(value) => Ok(PyString::intern(py, &value).into_any()),
        },
        CvarType::Int => match cvar_string {
            None => Ok(py.None().into_bound(py)),
            Some(value) => value.parse::<i64>().map_or_else(
                |_| {
                    cold_path();
                    let error_description =
                        format!("invalid literal for int() with base 10: '{value}'");
                    Err(PyValueError::new_err(error_description))
                },
                |int| Ok(PyInt::new(py, int).into_any()),
            ),
        },
        CvarType::Float => match cvar_string {
            None => Ok(py.None().into_bound(py)),
            Some(value) => value.parse::<f64>().map_or_else(
                |_| {
                    cold_path();
                    let error_description = format!("could not convert string to float: '{value}'");
                    Err(PyValueError::new_err(error_description))
                },
                |float| Ok(PyFloat::new(py, float).into_any()),
            ),
        },
        CvarType::Bool => match cvar_string {
            None => Ok(PyBool::new(py, false).into_any().to_owned()),
            Some(value) => value.parse::<i64>().map_or_else(
                |_| {
                    cold_path();
                    let error_description =
                        format!("invalid literal for int() with base 10: '{value}'");
                    Err(PyValueError::new_err(error_description))
                },
                |int| Ok(PyBool::new(py, int != 0).into_any().to_owned()),
            ),
        },
        CvarType::List => match cvar_string {
            None => Ok(PyList::empty(py).into_any()),
            Some(value) => {
                let items = value.split(',').collect::<Vec<_>>();
                Ok(PyList::new(py, items)?.into_any())
            }
        },
        CvarType::Set => match cvar_string {
            None => PySet::empty(py).map(|set| set.into_any()),
            Some(value) => {
                let items = value
                    .split(',')
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>();
                Ok(PySet::new::<String>(py, items)?.into_any())
            }
        },
        CvarType::Tuple => match cvar_string {
            None => Ok(PyTuple::empty(py).into_any()),
            Some(value) => {
                let items = value.split(',').collect::<Vec<_>>();
                Ok(PyTuple::new(py, items)?.into_any())
            }
        },
    }
}

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
    hooks: parking_lot::RwLock<Vec<(String, Py<PyAny>, i32)>>,
    commands: parking_lot::RwLock<Vec<Py<Command>>>,
    db_instance: parking_lot::RwLock<Py<PyAny>>,
}

#[pymethods]
impl Plugin {
    #[new]
    pub(crate) fn py_new(py: Python<'_>) -> Self {
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
        Python::attach(|py| *self.db_instance.write() = py.None());
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

    #[pyo3(signature = (event, handler, priority = CommandPriorities::PRI_NORMAL as i32), text_signature = "($self, event, handler, priority = PRI_NORMAL)")]
    fn add_hook(
        slf: &Bound<'_, Self>,
        event: &str,
        handler: &Bound<'_, PyAny>,
        priority: i32,
    ) -> PyResult<()> {
        slf.add_hook(event, handler, priority)
    }

    #[pyo3(signature = (event, handler, priority = CommandPriorities::PRI_NORMAL as i32), text_signature = "($self, event, handler, priority = PRI_NORMAL)")]
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
        client_cmd_perm = 5,
        prefix = true,
        usage = ""),
    text_signature = "($self, name, handler, permission = 0, channels = None, exclude_channels = None, priority = PRI_NORMAL, client_cmd_pass = False, client_cmd_perm = 5, prefix = True, usage = \"\")")]
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors minqlx.Plugin.add_command"
    )]
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
        let cvar_type = match return_type {
            None => CvarType::Str,
            Some(py_return_type) => {
                CvarType::from_type(py_return_type.bind(cls.py())).map_err(|type_name| {
                    cold_path();
                    PyValueError::new_err(format!("Invalid return type: {type_name}"))
                })?
            }
        };

        read_cvar_as(cls.py(), name, cvar_type)
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

        cls.py().detach(|| {
            MAIN_ENGINE.load().as_ref().map_or_else(
                || {
                    cold_path();
                    Err(PyEnvironmentError::new_err("could not get main_engine"))
                },
                |main_engine| match main_engine.find_cvar(name) {
                    None => {
                        main_engine.get_cvar(name, value_str.as_str(), Some(flags));
                        Ok(true)
                    }
                    Some(_) => {
                        let console_cmd = format!(r#"{name} "{value_str}""#);
                        main_engine.execute_console_command(console_cmd.as_str());
                        Ok(false)
                    }
                },
            )
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

        cls.py().detach(|| {
            MAIN_ENGINE.load().as_ref().map_or_else(
                || {
                    cold_path();
                    Err(PyEnvironmentError::new_err("could not get main_engine"))
                },
                |main_engine| {
                    let cvar = main_engine.find_cvar(name);

                    main_engine.set_cvar_limit(
                        name,
                        value_str.as_str(),
                        minimum_str.as_str(),
                        maximum_str.as_str(),
                        Some(flags),
                    );

                    Ok(cvar.is_none())
                },
            )
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

        cls.py().detach(|| {
            MAIN_ENGINE.load().as_ref().map_or_else(
                || {
                    cold_path();
                    Err(PyEnvironmentError::new_err("could not get main_engine"))
                },
                |main_engine| {
                    Ok(main_engine
                        .find_cvar(name)
                        .tap_none(|| {
                            main_engine.get_cvar(name, value_str.as_str(), Some(flags));
                        })
                        .is_none())
                },
            )
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

        cls.py().detach(|| {
            MAIN_ENGINE.load().as_ref().map_or_else(
                || {
                    cold_path();
                    Err(PyEnvironmentError::new_err("could not get main_engine"))
                },
                |main_engine| {
                    Ok(main_engine
                        .find_cvar(name)
                        .tap_none(|| {
                            main_engine.set_cvar_limit(
                                name,
                                value_str.as_str(),
                                minimum_str.as_str(),
                                maximum_str.as_str(),
                                Some(flags),
                            );
                        })
                        .is_none())
                },
            )
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
        match name.extract::<Player>() {
            Ok(player) => Ok(Some(player)),
            _ => match name.extract::<i32>() {
                Ok(player_id) if (0..64).contains(&player_id) => {
                    Ok(Some(Player::from_client_id(player_id, None)))
                }
                _ => {
                    let players =
                        player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());
                    match name.extract::<i64>() {
                        Ok(player_steam_id) => Ok(players
                            .iter()
                            .find(|player| player.steam_id == player_steam_id)
                            .cloned()),
                        _ => match client_id(cls.py(), name, Some(players.to_owned())) {
                            Some(client_id) => Ok(players
                                .iter()
                                .find(|player| player.id == client_id)
                                .cloned()),
                            _ => Ok(None),
                        },
                    }
                }
            },
        }
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
                .get_item(intern!(cls.py(), "limit"))
                .ok()
                .flatten()
                .and_then(|value| value.extract::<i32>().ok())
                .unwrap_or(100i32)
        });
        let delimiter = kwargs.map_or_else(
            || " ".to_owned(),
            |pydict| {
                pydict
                    .get_item(intern!(cls.py(), "delimiter"))
                    .ok()
                    .flatten()
                    .and_then(|value| value.extract::<String>().ok())
                    .unwrap_or_else(|| " ".to_owned())
            },
        );

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
                    .as_any()
                    .get_type()
                    .is_subclass(&cls.py().get_type::<AbstractChannel>())
                    .unwrap_or(false)
                {
                    return channel
                        .as_any()
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
                                .eq(channel.as_any())
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
                            .eq(channel.as_any())
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
        cold_path();
        Err(PyValueError::new_err("Invalid channel."))
    }

    /// Prints text in the console.
    #[classmethod]
    fn console(cls: &Bound<'_, PyType>, text: &Bound<'_, PyAny>) -> PyResult<()> {
        let extracted_text = text.str()?.to_string();
        let printed_text = format!("{extracted_text}\n");
        cls.py().detach(|| {
            shinqlx_com_printf(&printed_text);
            Ok(())
        })
    }

    /// Removes color tags from text.
    #[classmethod]
    fn clean_text<'py>(
        _cls: &Bound<'py, PyType>,
        text: &Bound<'py, PyString>,
    ) -> PyResult<Bound<'py, PyString>> {
        clean_py_text(text)
    }

    /// Get the colored name of a decolored name.
    #[classmethod]
    #[pyo3(signature = (name, player_list = None), text_signature = "(name, player_list = None)")]
    fn colored_name(
        cls: &Bound<'_, PyType>,
        name: &Bound<'_, PyAny>,
        player_list: Option<Vec<Player>>,
    ) -> Option<String> {
        match name.extract::<Player>() {
            Ok(player) => Some(player.name.read().to_owned()),
            Err(_) => name
                .str()
                .map(|searched_name| {
                    let players =
                        player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());
                    let clean_name = clean_text(&searched_name.to_string()).to_lowercase();

                    players
                        .iter()
                        .find(|&player| {
                            clean_text(&(&*player.name.read())).to_lowercase() == clean_name
                        })
                        .map(|found_player| found_player.name.read().to_owned())
                })
                .ok()
                .flatten(),
        }
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

        players_matching_name(players, name)
    }

    /// Find a player by name or client ID and tell the caller when there is no or more than one
    /// match.
    ///
    /// A client ID of a connected player, also written as ``id:5``, always wins. Otherwise,
    /// the target is matched against part of the names of the players. Returns ``None`` if
    /// there was not exactly one player.
    #[classmethod]
    #[pyo3(signature = (player, target, player_list = None), text_signature = "(player, target, player_list = None)")]
    pub(crate) fn find_by_name_or_id(
        cls: &Bound<'_, PyType>,
        player: &Bound<'_, PyAny>,
        target: &str,
        player_list: Option<Vec<Player>>,
    ) -> PyResult<Option<Player>> {
        let players = player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());

        match lookup_target(players, target) {
            TargetLookup::Found(found) => Ok(Some(found)),
            TargetLookup::NotFound => {
                tell_caller(
                    player,
                    &format!("Sorry, but no players matched your tokens: {target}."),
                )?;
                Ok(None)
            }
            TargetLookup::Ambiguous(matches) => {
                let listing = matches
                    .iter()
                    .map(|found| format!("  {}^6:^7 {}", found.id, found.name.read()))
                    .join("\n");
                tell_caller(
                    player,
                    &format!(
                        "A total of ^6{}^7 players matched for {target}:",
                        matches.len()
                    ),
                )?;
                tell_caller(player, &listing)?;
                Ok(None)
            }
        }
    }

    /// Get a dictionary with the teams as keys and players as values.
    #[classmethod]
    #[pyo3(signature = (player_list = None), text_signature = "(player_list = None)")]
    fn teams<'py>(
        cls: &Bound<'py, PyType>,
        player_list: Option<Vec<Player>>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let players = player_list.unwrap_or_else(|| Self::players(cls).unwrap_or_default());

        [Teams::Free, Teams::Red, Teams::Blue, Teams::Spectator]
            .iter()
            .try_fold(PyDict::new(cls.py()), |result, team| {
                let team_str = team.to_string();
                let filtered_players = players
                    .iter()
                    .filter(|player| player.get_team(cls.py()).is_ok_and(|team| team == team_str))
                    .cloned()
                    .collect::<Vec<_>>();
                result.set_item(
                    PyString::intern(cls.py(), team_str.as_str()),
                    filtered_players,
                )?;

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
            cold_path();
            return Err(PyValueError::new_err("could not find recipient"));
        };
        let recipient_player = Player::from_client_id(recipient_client_id, None);
        let tell_channel = Bound::new(
            cls.py(),
            TellChannel::py_new(
                cls.py(),
                &recipient_player,
                cls.py().None().bind(cls.py()),
                None,
            ),
        )?;

        tell_channel
            .call_method(intern!(cls.py(), "reply"), (msg,), kwargs)
            .map(|_| ())
    }

    #[classmethod]
    fn is_vote_active(_cls: &Bound<'_, PyType>) -> bool {
        is_vote_active()
    }

    #[classmethod]
    fn current_vote_count<'py>(cls: &Bound<'py, PyType>) -> PyResult<Bound<'py, PyAny>> {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            cold_path();
            return Ok(cls.py().None().into_bound(cls.py()));
        };

        let yes_votes = main_engine.get_configstring(CS_VOTE_YES as u16);
        let no_votes = main_engine.get_configstring(CS_VOTE_NO as u16);

        if yes_votes.is_empty() || no_votes.is_empty() {
            return Ok(cls.py().None().into_bound(cls.py()));
        }

        let Ok(parsed_yes_votes) = yes_votes.parse::<i32>() else {
            cold_path();
            let error_msg = format!("invalid literal for int() with base 10: '{yes_votes}'");
            return Err(PyValueError::new_err(error_msg));
        };
        let Ok(parsed_no_votes) = no_votes.parse::<i32>() else {
            cold_path();
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
            .map_or_else(
                || {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to vote started dispatcher",
                    ))
                },
                |vote_started_dispatcher| {
                    vote_started_dispatcher
                        .cast::<VoteStartedDispatcher>()?
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
            .cast::<PyBool>()
            .map_err(|_| PyValueError::new_err("pass_it must be either True or False."))
            .and_then(|vote_passed| pyshinqlx_force_vote(cls.py(), vote_passed.is_true()))
    }

    #[classmethod]
    fn teamsize(cls: &Bound<'_, PyType>, size: i32) -> PyResult<()> {
        cls.py().detach(|| set_teamsize(size))
    }

    #[classmethod]
    #[pyo3(signature = (player, reason = ""), text_signature = "(player, reason = \"\")")]
    fn kick(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>, reason: &str) -> PyResult<()> {
        let Some(client_id) = client_id(cls.py(), player, None) else {
            cold_path();
            return Err(PyValueError::new_err("Invalid player."));
        };

        let forwarded_reason = if reason.is_empty() {
            None
        } else {
            Some(reason)
        };

        pyshinqlx_kick(cls.py(), client_id, forwarded_reason)
    }

    #[classmethod]
    fn shuffle(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().detach(|| console_command("forceshuffle"))
    }

    #[classmethod]
    #[pyo3(signature = (new_map, factory = None), text_signature = "(new_map, factory = None)")]
    fn change_map(cls: &Bound<'_, PyType>, new_map: &str, factory: Option<&str>) -> PyResult<()> {
        cls.py().detach(|| {
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
            cold_path();
            return Err(PyValueError::new_err("The first player is invalid."));
        };
        let Some(player2) = Self::player(cls, other_player, None)? else {
            cold_path();
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
                cls.py().detach(|| {
                    let slap_cmd = format!("slap {client_id} {damage}");
                    console_command(&slap_cmd)
                })
            },
        )
    }

    #[classmethod]
    fn slay(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        client_id(cls.py(), player, None).map_or(
            Err(PyValueError::new_err("Invalid player.")),
            |client_id| {
                cls.py().detach(|| {
                    let slay_cmd = format!("slay {client_id}");
                    console_command(&slay_cmd)
                })
            },
        )
    }

    #[classmethod]
    fn timeout(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().detach(|| console_command("timeout"))
    }

    #[classmethod]
    fn timein(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().detach(|| console_command("timein"))
    }

    #[classmethod]
    fn allready(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().detach(|| console_command("allready"))
    }

    #[classmethod]
    fn pause(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().detach(|| console_command("pause"))
    }

    #[classmethod]
    fn unpause(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().detach(|| console_command("unpause"))
    }

    #[classmethod]
    #[pyo3(signature = (team = None), text_signature = "(team = None)")]
    fn lock(cls: &Bound<'_, PyType>, team: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let team = team.map(name_of).transpose()?;
        let team = team.as_deref();
        cls.py().detach(|| lock(team))
    }

    #[classmethod]
    #[pyo3(signature = (team = None), text_signature = "(team = None)")]
    fn unlock(cls: &Bound<'_, PyType>, team: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let team = team.map(name_of).transpose()?;
        let team = team.as_deref();
        cls.py().detach(|| unlock(team))
    }

    #[classmethod]
    fn put(
        cls: &Bound<'_, PyType>,
        player: &Bound<'_, PyAny>,
        team: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        put(cls.py(), player, &name_of(team)?)
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
        cls.py().detach(|| opsay(msg))
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
        cls.py().detach(|| console_command("map_restart"))
    }

    #[classmethod]
    fn addscore(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>, score: i32) -> PyResult<()> {
        addscore(cls.py(), player, score)
    }

    #[classmethod]
    fn addteamscore(cls: &Bound<'_, PyType>, team: &Bound<'_, PyAny>, score: i32) -> PyResult<()> {
        let team = name_of(team)?;
        cls.py().detach(|| addteamscore(&team, score))
    }

    #[classmethod]
    fn setmatchtime(cls: &Bound<'_, PyType>, time: i32) -> PyResult<()> {
        cls.py().detach(|| {
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
    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors minqlx.Plugin.add_command"
    )]
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
            cold_path();
            let error_msg = format!("Plugin '{plugin_name}' does not have a database driver.");
            return Err(PyRuntimeError::new_err(error_msg));
        };

        if db_class.is_none() {
            cold_path();
            let error_msg = format!("Plugin '{plugin_name}' does not have a database driver.");
            return Err(PyRuntimeError::new_err(error_msg));
        }

        let plugin = self.get();
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

        Ok(loaded_plugins.cast()?.to_owned())
    }

    fn get_hooks(&self) -> Vec<(String, Py<PyAny>, i32)> {
        self.get()
            .hooks
            .read()
            .iter()
            .map(|(name, handler, permission)| {
                (name.clone(), handler.clone_ref(self.py()), *permission)
            })
            .collect()
    }

    fn get_commands(&self) -> Vec<Py<Command>> {
        self.get()
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
                    Some(PyString::intern(self.py(), &plugin_name).into_any()),
                )
            })
    }

    fn add_hook(&self, event: &str, handler: &Bound<'_, PyAny>, priority: i32) -> PyResult<()> {
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| event_dispatchers.bind(self.py()).get_item(event).ok())
            .map_or_else(
                || {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to event dispatcher",
                    ))
                },
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

        self.get()
            .hooks
            .write()
            .push((event.to_string(), handler.to_owned().unbind(), priority));
        Ok(())
    }

    fn remove_hook(&self, event: &str, handler: &Bound<'_, PyAny>, priority: i32) -> PyResult<()> {
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| event_dispatchers.bind(self.py()).get_item(event).ok())
            .map_or_else(
                || {
                    cold_path();
                    Err(PyEnvironmentError::new_err(
                        "could not get access to event dispatchers",
                    ))
                },
                |event_dispatcher| {
                    let plugin_type = self.get_type();
                    let plugin_name = plugin_type.name()?;
                    EventDispatcherMethods::remove_hook(
                        event_dispatcher.cast()?,
                        &plugin_name.to_string(),
                        handler,
                        priority,
                    )?;
                    Ok(())
                },
            )?;

        self.get()
            .hooks
            .write()
            .retain(|(hook_event, hook_handler, hook_priority)| {
                hook_event != event
                    || hook_handler
                        .bind(self.py())
                        .ne(handler.as_any())
                        .unwrap_or(false)
                    || hook_priority != &priority
            });
        Ok(())
    }

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

        self.get()
            .commands
            .write()
            .push(py_command.to_owned().unbind());

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
        name.cast::<PyList>().ok().tap_some(|py_list| {
            names.extend(py_list.iter().filter_map(|py_alias| {
                py_alias
                    .extract::<String>()
                    .ok()
                    .map(|alias| alias.to_lowercase())
            }));
        });
        name.cast::<PyTuple>().ok().tap_some(|py_tuple| {
            names.extend(py_tuple.iter().filter_map(|py_alias| {
                py_alias
                    .extract::<String>()
                    .ok()
                    .map(|alias| alias.to_lowercase())
            }));
        });
        name.extract::<String>().ok().tap_some(|py_string| {
            names.push(py_string.to_lowercase());
        });

        self.get()
            .commands
            .read()
            .iter()
            .find(|&existing_command| {
                existing_command.bind(self.py()).get().name.len() == names.len()
                    && existing_command
                        .bind(self.py())
                        .get()
                        .name
                        .iter()
                        .all(|name| names.contains(name))
                    && existing_command
                        .bind(self.py())
                        .get()
                        .handler
                        .bind(self.py())
                        .eq(handler)
                        .unwrap_or(false)
            })
            .map_or(Ok(()), |command| {
                COMMANDS.load().as_ref().map_or_else(
                    || {
                        cold_path();
                        Err(PyEnvironmentError::new_err(
                            "could not get access to commands",
                        ))
                    },
                    |commands| {
                        commands
                            .bind(self.py())
                            .remove_command(command.bind(self.py()))
                    },
                )
            })?;

        self.get().commands.write().retain(|existing_command| {
            existing_command.bind(self.py()).get().name.len() != names.len()
                || !existing_command
                    .bind(self.py())
                    .get()
                    .name
                    .iter()
                    .all(|name| names.contains(name))
                || existing_command
                    .bind(self.py())
                    .get()
                    .handler
                    .bind(self.py())
                    .ne(handler)
                    .unwrap_or(true)
        });
        Ok(())
    }
}

#[cfg(test)]
mod lookup_tests;
#[cfg(test)]
mod tests;
