mod channels;
mod commands;
mod dispatchers;
mod embed;
mod events;
mod flight;
mod game;
mod handlers;
mod holdable;
mod player;
mod player_info;
mod player_state;
mod player_stats;
mod plugin;
mod powerups;
mod stats_listener;
mod vector3;
mod weapons;

pub(crate) mod prelude {
    pub(crate) use super::channels::{
        AbstractChannel, ChatChannel, ClientCommandChannel, ConsoleChannel, TeamChatChannel,
        TellChannel, MAX_MSG_LENGTH,
    };
    pub(crate) use super::commands::{Command, CommandInvoker};
    pub(crate) use super::embed::*;
    pub(crate) use super::events::{
        ChatEventDispatcher, ClientCommandDispatcher, CommandDispatcher, ConsolePrintDispatcher,
        DamageDispatcher, DeathDispatcher, EventDispatcher, EventDispatcherManager,
        FrameEventDispatcher, GameCountdownDispatcher, GameEndDispatcher, GameStartDispatcher,
        KamikazeExplodeDispatcher, KamikazeUseDispatcher, KillDispatcher, MapDispatcher,
        NewGameDispatcher, PlayerConnectDispatcher, PlayerDisconnectDispatcher,
        PlayerLoadedDispatcher, PlayerSpawnDispatcher, RoundCountdownDispatcher,
        RoundEndDispatcher, RoundStartDispatcher, ServerCommandDispatcher,
        SetConfigstringDispatcher, StatsDispatcher, TeamSwitchAttemptDispatcher,
        TeamSwitchDispatcher, UnloadDispatcher, UserinfoDispatcher, VoteCalledDispatcher,
        VoteDispatcher, VoteEndedDispatcher, VoteStartedDispatcher,
    };
    pub(crate) use super::flight::Flight;
    pub(crate) use super::game::{Game, NonexistentGameError};
    #[cfg(test)]
    #[allow(unused_imports)]
    pub(crate) use super::handlers::mock_handlers::{
        handle_client_command, handle_console_print, handle_damage, handle_frame,
        handle_kamikaze_explode, handle_kamikaze_use, handle_new_game, handle_player_connect,
        handle_player_disconnect, handle_player_loaded, handle_player_spawn, handle_rcon,
        handle_server_command, handle_set_configstring, register_handlers,
    };
    #[cfg(test)]
    #[allow(unused_imports)]
    pub(crate) use super::handlers::mock_handlers::{
        handle_client_command_context, handle_console_print_context, handle_damage_context,
        handle_frame_context, handle_kamikaze_explode_context, handle_kamikaze_use_context,
        handle_new_game_context, handle_player_connect_context, handle_player_disconnect_context,
        handle_player_loaded_context, handle_player_spawn_context, handle_rcon_context,
        handle_server_command_context, handle_set_configstring_context, register_handlers_context,
    };
    #[cfg(not(test))]
    #[allow(unused_imports)]
    pub(crate) use super::handlers::{
        handle_client_command, handle_console_print, handle_damage, handle_frame,
        handle_kamikaze_explode, handle_kamikaze_use, handle_new_game, handle_player_connect,
        handle_player_disconnect, handle_player_loaded, handle_player_spawn, handle_rcon,
        handle_server_command, handle_set_configstring, register_handlers,
    };
    pub(crate) use super::holdable::Holdable;
    pub(crate) use super::player::{
        AbstractDummyPlayer, NonexistentPlayerError, Player, RconDummyPlayer,
    };
    pub(crate) use super::player_info::PlayerInfo;
    pub(crate) use super::player_state::PlayerState;
    pub(crate) use super::player_stats::PlayerStats;
    pub(crate) use super::powerups::Powerups;
    pub(crate) use super::stats_listener::StatsListener;
    pub(crate) use super::vector3::Vector3;
    pub(crate) use super::weapons::Weapons;

    pub(crate) use super::{clean_text, parse_variables};

    pub(crate) use super::{ALLOW_FREE_CLIENT, CUSTOM_COMMAND_HANDLER};

    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        pyshinqlx_initialize, pyshinqlx_is_initialized, pyshinqlx_reload,
    };
    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        pyshinqlx_initialize_context, pyshinqlx_is_initialized_context, pyshinqlx_reload_context,
    };
    pub(crate) use super::PythonInitializationError;
    #[cfg(not(test))]
    pub(crate) use super::{pyshinqlx_initialize, pyshinqlx_is_initialized, pyshinqlx_reload};

    #[cfg(not(test))]
    pub(crate) use super::dispatchers::{
        client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
        client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher,
        damage_dispatcher, frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher,
        new_game_dispatcher, rcon_dispatcher, server_command_dispatcher,
        set_configstring_dispatcher,
    };
    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
        client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher,
        damage_dispatcher, frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher,
        new_game_dispatcher, rcon_dispatcher, server_command_dispatcher,
        set_configstring_dispatcher,
    };
    #[cfg(test)]
    pub(crate) use super::mock_python_tests::{
        client_command_dispatcher_context, client_connect_dispatcher_context,
        client_disconnect_dispatcher_context, client_loaded_dispatcher_context,
        client_spawn_dispatcher_context, console_print_dispatcher_context,
        damage_dispatcher_context, frame_dispatcher_context, kamikaze_explode_dispatcher_context,
        kamikaze_use_dispatcher_context, new_game_dispatcher_context, rcon_dispatcher_context,
        server_command_dispatcher_context, set_configstring_dispatcher_context,
    };

    #[cfg(test)]
    pub(crate) use super::pyshinqlx_setup_fixture::*;

    pub(crate) use pyo3::prelude::*;
}

use crate::ffi::c::prelude::*;
use crate::quake_live_engine::FindCVar;
use crate::MAIN_ENGINE;
use crate::_INIT_TIME;
use prelude::*;

use commands::CommandPriorities;

use arc_swap::ArcSwapOption;
use core::{
    ops::Deref,
    str::FromStr,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use itertools::Itertools;
use log::*;
use once_cell::sync::Lazy;
use regex::Regex;

use pyo3::{
    append_to_inittab, create_exception,
    exceptions::{PyEnvironmentError, PyException, PyValueError},
    intern, prepare_freethreaded_python,
    types::{IntoPyDict, PyDelta, PyDict, PyFunction, PyString, PyTuple},
};

pub(crate) static ALLOW_FREE_CLIENT: AtomicU64 = AtomicU64::new(0);

pub(crate) static CUSTOM_COMMAND_HANDLER: Lazy<ArcSwapOption<PyObject>> =
    Lazy::new(ArcSwapOption::empty);

// Used primarily in Python, but defined here and added using PyModule_AddIntMacro().
#[allow(non_camel_case_types)]
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum PythonReturnCodes {
    RET_NONE,
    RET_STOP,
    // Stop execution of event handlers within Python.
    RET_STOP_EVENT,
    // Only stop the event, but let other handlers process it.
    RET_STOP_ALL,
    // Stop execution at an engine level. SCARY STUFF!
    RET_USAGE, // Used for commands. Replies to the channel with a command's usage.
}

impl FromPyObject<'_> for PythonReturnCodes {
    fn extract_bound(item: &Bound<'_, PyAny>) -> PyResult<Self> {
        if item.is_none() {
            return Ok(PythonReturnCodes::RET_NONE);
        }
        let item_i32 = item.extract::<i32>();
        if item_i32
            .as_ref()
            .is_ok_and(|&value| value == PythonReturnCodes::RET_NONE as i32)
        {
            return Ok(PythonReturnCodes::RET_NONE);
        }
        if item_i32
            .as_ref()
            .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP as i32)
        {
            return Ok(PythonReturnCodes::RET_STOP);
        }
        if item_i32
            .as_ref()
            .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_ALL as i32)
        {
            return Ok(PythonReturnCodes::RET_STOP_ALL);
        }
        if item_i32
            .as_ref()
            .is_ok_and(|&value| value == PythonReturnCodes::RET_STOP_EVENT as i32)
        {
            return Ok(PythonReturnCodes::RET_STOP_EVENT);
        }
        if item_i32
            .as_ref()
            .is_ok_and(|&value| value == PythonReturnCodes::RET_USAGE as i32)
        {
            return Ok(PythonReturnCodes::RET_USAGE);
        }

        Err(PyValueError::new_err("unsupported PythonReturnCode"))
    }
}

create_exception!(pyshinqlx_module, PluginLoadError, PyException);
create_exception!(pyshinqlx_module, PluginUnloadError, PyException);

pub(crate) fn clean_text<T>(text: &T) -> String
where
    T: AsRef<str>,
{
    let re = Regex::new(r#"\^[0-7]"#).unwrap();
    re.replace_all(text.as_ref(), "").to_string()
}

pub(crate) fn parse_variables(varstr: &str) -> ParsedVariables {
    varstr
        .parse::<ParsedVariables>()
        .unwrap_or(ParsedVariables { items: vec![] })
}

pub(crate) struct ParsedVariables {
    items: Vec<(String, String)>,
}

impl FromStr for ParsedVariables {
    type Err = &'static str;

    fn from_str(varstr: &str) -> Result<Self, Self::Err> {
        if varstr.trim().is_empty() {
            return Ok(Self { items: vec![] });
        }

        let stripped_varstr = varstr.strip_prefix('\\').unwrap_or(varstr).to_string();

        let varstr_vec: Vec<String> = stripped_varstr
            .split('\\')
            .map(|value| value.to_string())
            .collect();

        if varstr_vec.len() % 2 == 1 {
            warn!(target: "shinqlx", "Uneven number of keys and values: {}", varstr);
        }
        Ok(Self {
            items: varstr_vec.into_iter().tuples().collect(),
        })
    }
}

impl From<ParsedVariables> for String {
    fn from(value: ParsedVariables) -> Self {
        value
            .items
            .iter()
            .map(|(key, value)| format!(r"\{key}\{value}"))
            .join("")
    }
}

impl IntoPyDict for ParsedVariables {
    fn into_py_dict(self, py: Python<'_>) -> &PyDict {
        #[allow(deprecated)]
        self.items.into_py_dict(py)
    }

    fn into_py_dict_bound(self, py: Python<'_>) -> Bound<'_, PyDict> {
        self.items.into_py_dict_bound(py)
    }
}

impl Deref for ParsedVariables {
    type Target = Vec<(String, String)>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl ParsedVariables {
    pub fn get<T>(&self, item: T) -> Option<String>
    where
        T: AsRef<str>,
    {
        self.items
            .iter()
            .filter(|(key, _value)| *key == item.as_ref())
            .map(|(_key, value)| value)
            .next()
            .cloned()
    }

    pub fn set(&mut self, item: &str, value: &str) {
        let mut new_items: Vec<(String, String)> = self
            .items
            .clone()
            .into_iter()
            .filter(|(key, _value)| key != item)
            .collect();
        new_items.push((item.into(), value.into()));
        self.items = new_items;
    }
}

#[cfg(test)]
mod parsed_variables_test {
    use super::ParsedVariables;
    use core::str::FromStr;

    #[test]
    fn test_parse_variables_with_space() {
        let variables = ParsedVariables::from_str(r"\name\Unnamed Player\country\de")
            .expect("this should not happen");
        assert!(variables
            .get("name")
            .is_some_and(|value| value == "Unnamed Player"));
        assert!(variables.get("country").is_some_and(|value| value == "de"));
    }
}

pub(crate) fn client_id(
    py: Python<'_>,
    name: PyObject,
    player_list: Option<Vec<Player>>,
) -> Option<i32> {
    if let Ok(value) = name.extract::<i32>(py) {
        if (0..64).contains(&value) {
            return Some(value);
        }
    }

    if let Ok(player) = name.extract::<Player>(py) {
        return Some(player.id);
    }

    let all_players = player_list.unwrap_or_else(|| {
        Player::all_players(&py.get_type_bound::<Player>(), py).unwrap_or_default()
    });

    if let Ok(steam_id) = name.extract::<i64>(py) {
        return all_players
            .iter()
            .find(|&player| player.steam_id == steam_id)
            .map(|player| player.id);
    }

    if let Ok(player_name) = name.extract::<String>(py) {
        let clean_name = clean_text(&player_name).to_lowercase();
        return all_players
            .iter()
            .find(|&player| clean_text(&player.name).to_lowercase() == clean_name)
            .map(|player| player.id);
    }

    None
}

#[pyfunction]
#[pyo3(pass_module)]
fn set_map_subtitles(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let map_title = pyshinqlx_get_configstring(module.py(), CS_MESSAGE)?;
    module.setattr(intern!(module.py(), "_map_title"), map_title)?;

    let mut map_subtitle1 = pyshinqlx_get_configstring(module.py(), CS_AUTHOR)?;
    module.setattr(
        intern!(module.py(), "_map_subtitle1"),
        map_subtitle1.clone(),
    )?;

    let mut map_subtitle2 = pyshinqlx_get_configstring(module.py(), CS_AUTHOR2)?;
    module.setattr(
        intern!(module.py(), "_map_subtitle2"),
        map_subtitle2.clone(),
    )?;

    if !map_subtitle1.is_empty() {
        map_subtitle1.push_str(" - ");
    }

    map_subtitle1.push_str("Running shinqlx ^6");
    map_subtitle1.push_str(env!("SHINQLX_VERSION"));
    map_subtitle1.push_str("^7 with plugins ^6");
    let plugins_version = module
        .getattr(intern!(module.py(), "__plugins_version__"))
        .map(|value| value.extract::<String>().unwrap_or("NOT_SET".to_string()))
        .unwrap_or("NOT_SET".to_string());
    map_subtitle1.push_str(&plugins_version);
    map_subtitle1.push_str("^7.");

    pyshinqlx_set_configstring(module.py(), CS_AUTHOR, &map_subtitle1)?;

    if !map_subtitle2.is_empty() {
        map_subtitle2.push_str(" - ");
    }
    map_subtitle2.push_str("Check ^6https://github.com/mgaertne/shinqlx^7 for more details.");
    pyshinqlx_set_configstring(module.py(), CS_AUTHOR2, &map_subtitle2)?;

    Ok(())
}

/// Parses strings of key-value pairs delimited by r"\" and puts
/// them into a dictionary.
#[pyfunction]
#[pyo3(name = "parse_variables")]
#[pyo3(signature = (varstr, ordered = false))]
fn pyshinqlx_parse_variables<'py>(
    py: Python<'py>,
    varstr: &str,
    #[allow(unused_variables)] ordered: bool,
) -> Bound<'py, PyDict> {
    let parsed_variables = py.allow_threads(|| parse_variables(varstr));
    parsed_variables.into_py_dict_bound(py)
}

fn get_logger_name(py: Python<'_>, plugin: Option<PyObject>) -> String {
    let opt_plugin_name = plugin.and_then(|req_plugin| {
        req_plugin
            .bind(py)
            .str()
            .ok()
            .map(|plugin_name| plugin_name.to_string())
    });
    py.allow_threads(|| {
        opt_plugin_name
            .map(|plugin_name| format!("shinqlx.{plugin_name}"))
            .unwrap_or("shinqlx".to_string())
    })
}

/// Provides a logger that should be used by your plugin for debugging, info and error reporting. It will automatically output to both the server console as well as to a file.
#[pyfunction]
#[pyo3(name = "get_logger")]
#[pyo3(signature = (plugin = None))]
pub(crate) fn pyshinqlx_get_logger(
    py: Python<'_>,
    plugin: Option<PyObject>,
) -> PyResult<Bound<'_, PyAny>> {
    let logger_name = get_logger_name(py, plugin);
    PyModule::import_bound(py, intern!(py, "logging"))?
        .call_method1(intern!(py, "getLogger"), (logger_name,))
}

#[pyfunction]
#[pyo3(name = "_configure_logger")]
fn pyshinqlx_configure_logger(py: Python<'_>) -> PyResult<()> {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return Err(PyEnvironmentError::new_err("no main engine found"));
    };
    let homepath = main_engine
        .find_cvar("fs_homepath")
        .map(|homepath_cvar| homepath_cvar.get_string().to_string())
        .unwrap_or_default();
    let num_max_logs = main_engine
        .find_cvar("qlx_logs")
        .map(|max_logs_cvar| max_logs_cvar.get_integer())
        .unwrap_or_default();
    let max_logsize = main_engine
        .find_cvar("qlx_logsSize")
        .map(|max_logsize_cvar| max_logsize_cvar.get_integer())
        .unwrap_or_default();

    let logging_module = py.import_bound(intern!(py, "logging"))?;
    let debug_level = logging_module.getattr(intern!(py, "DEBUG"))?;
    let info_level = logging_module.getattr(intern!(py, "INFO"))?;
    let logger =
        logging_module.call_method1(intern!(py, "getLogger"), (intern!(py, "shinqlx"),))?;
    logger.call_method1(intern!(py, "setLevel"), (debug_level.clone(),))?;

    let console_fmt = logging_module.call_method1(
        intern!(py, "Formatter"),
        (
            "[%(name)s.%(funcName)s] %(levelname)s: %(message)s",
            "%H:%M:%S",
        ),
    )?;

    let console_handler = logging_module.call_method0("StreamHandler")?;
    console_handler.call_method1(intern!(py, "setLevel"), (info_level,))?;
    console_handler.call_method1(intern!(py, "setFormatter"), (console_fmt,))?;
    logger.call_method1(intern!(py, "addHandler"), (console_handler,))?;

    let file_fmt = logging_module.call_method1(
        intern!(py, "Formatter"),
        (
            "(%(asctime)s) [%(levelname)s @ %(name)s.%(funcName)s] %(message)s",
            "%H:%M:%S",
        ),
    )?;
    let file_path = format!("{homepath}/shinqlx.log");
    let handlers_submodule = py.import_bound("logging.handlers")?;
    let file_handler = handlers_submodule.call_method(
        "RotatingFileHandler",
        (file_path,),
        Some(
            &[
                ("encoding", "utf-8".into_py(py)),
                ("maxBytes", max_logsize.into_py(py)),
                ("backupCount", num_max_logs.into_py(py)),
            ]
            .into_py_dict_bound(py),
        ),
    )?;
    file_handler.call_method1(intern!(py, "setLevel"), (debug_level,))?;
    file_handler.call_method1(intern!(py, "setFormatter"), (file_fmt,))?;
    logger.call_method1(intern!(py, "addHandler"), (file_handler,))?;

    let datetime_module = py.import_bound("datetime")?;
    let datetime_now = datetime_module.getattr("datetime")?.call_method0("now")?;
    logger.call_method1(
        intern!(py, "info"),
        (
            "============================= shinqlx run @ %s =============================",
            datetime_now,
        ),
    )?;
    Ok(())
}

/// Logs an exception using :func:`get_logger`. Call this in an except block.
#[pyfunction]
#[pyo3(name = "log_exception")]
#[pyo3(signature = (plugin = None))]
fn pyshinqlx_log_exception(py: Python<'_>, plugin: Option<PyObject>) -> PyResult<()> {
    let sys_module = py.import_bound(intern!(py, "sys"))?;
    let exc_info = sys_module.call_method0(intern!(py, "exc_info"))?;
    let exc_tuple = exc_info.extract::<&PyTuple>()?;

    let traceback_module = py.import_bound(intern!(py, "traceback"))?;
    let formatted_traceback: Vec<String> = traceback_module
        .call_method1(intern!(py, "format_exception"), exc_tuple)?
        .extract()?;

    try_log_messages(
        py,
        plugin,
        intern!(py, "log_exception"),
        formatted_traceback,
    )?;

    Ok(())
}

/// A handler for unhandled exceptions.
#[pyfunction]
#[pyo3(name = "handle_exception")]
fn pyshinqlx_handle_exception(
    py: Python<'_>,
    exc_type: Py<PyAny>,
    exc_value: Py<PyAny>,
    exc_traceback: Py<PyAny>,
) -> PyResult<()> {
    let traceback_module = py.import_bound(intern!(py, "traceback"))?;

    let formatted_traceback: Vec<String> = traceback_module
        .call_method1(
            intern!(py, "format_exception"),
            (exc_type, exc_value, exc_traceback),
        )?
        .extract()?;

    try_log_messages(py, None, intern!(py, "log_exception"), formatted_traceback)?;

    Ok(())
}

#[pyfunction]
#[pyo3(name = "threading_excepthook")]
fn pyshinqlx_handle_threading_exception(py: Python<'_>, args: Py<PyAny>) -> PyResult<()> {
    pyshinqlx_handle_exception(
        py,
        args.getattr(py, intern!(py, "exc_type"))?,
        args.getattr(py, intern!(py, "exc_value"))?,
        args.getattr(py, intern!(py, "exc_traceback"))?,
    )
}

fn try_log_exception(py: Python<'_>, exception: &PyErr) -> PyResult<()> {
    let traceback_module = py.import_bound(intern!(py, "traceback"))?;
    let formatted_traceback: Vec<String> = traceback_module
        .call_method1(
            intern!(py, "format_exception"),
            (
                exception.get_type_bound(py),
                exception.value_bound(py),
                exception.traceback_bound(py),
            ),
        )?
        .extract()?;

    try_log_messages(py, None, intern!(py, "log_exception"), formatted_traceback)?;

    Ok(())
}

pub(crate) fn log_exception(py: Python<'_>, exception: &PyErr) {
    let _ = try_log_exception(py, exception);
}

fn try_log_messages(
    py: Python<'_>,
    plugin: Option<PyObject>,
    function: &Bound<'_, PyString>,
    messages: Vec<String>,
) -> Result<(), PyErr> {
    let logging_module = py.import_bound(intern!(py, "logging"))?;
    let error_level = logging_module.getattr(intern!(py, "ERROR"))?;
    let logger_name = get_logger_name(py, plugin);
    let py_logger = logging_module.call_method1(intern!(py, "getLogger"), (logger_name,))?;

    for line in messages {
        let log_record = py_logger.call_method(
            intern!(py, "makeRecord"),
            (
                intern!(py, "shinqlx"),
                &error_level,
                intern!(py, ""),
                -1,
                line.trim_end(),
                py.None(),
                py.None(),
            ),
            Some(&[(intern!(py, "func"), function)].into_py_dict_bound(py)),
        )?;
        py_logger.call_method1(intern!(py, "handle"), (log_record,))?;
    }
    Ok(())
}

#[pyfunction]
fn next_frame(py: Python<'_>, func: Py<PyFunction>) -> PyResult<Bound<'_, PyAny>> {
    let next_frame_func = PyModule::from_code_bound(
        py,
        r#"
from functools import wraps

import shinqlx


def next_frame(func):
    @wraps(func)
    def f(*args, **kwargs):
        shinqlx.next_frame_tasks.put_nowait((func, args, kwargs))

    return f
        "#,
        "",
        "",
    )?
    .getattr(intern!(py, "next_frame"))?;

    next_frame_func.call1((func.into_py(py),))
}

/// Delay a function call a certain amount of time.
///
///     .. note::
///         It cannot guarantee you that it will be called right as the timer
///         expires, but unless some plugin is for some reason blocking, then
///         you can expect it to be called practically as soon as it expires.
#[pyfunction]
fn delay(py: Python<'_>, time: f32) -> PyResult<Bound<'_, PyAny>> {
    let delay_func = PyModule::from_code_bound(
        py,
        r#"
from functools import wraps

import shinqlx


def delay(time):
    def wrap(func):
        @wraps(func)
        def f(*args, **kwargs):
            shinqlx.frame_tasks.enter(time, 1, func, args, kwargs)

        return f

    return wrap
    "#,
        "",
        "",
    )?
    .getattr(intern!(py, "delay"))?;

    delay_func.call1((time.into_py(py),))
}

/// Starts a thread with the function passed as its target. If a function decorated
/// with this is called within a function also decorated, it will **not** create a second
/// thread unless told to do so with the *force* keyword.
#[pyfunction]
#[pyo3(signature = (func, force = false))]
fn thread(py: Python<'_>, func: Py<PyFunction>, force: bool) -> PyResult<Bound<'_, PyAny>> {
    let thread_func = PyModule::from_code_bound(
        py,
        r#"
import threading
from functools import wraps

import shinqlx


def thread(func, force=False):
    @wraps(func)
    def f(*args, **kwargs):
        if not force and threading.current_thread().name.endswith(shinqlx._thread_name):
            func(*args, **kwargs)
        else:
            name = f"{func.__name__}-{str(shinqlx._thread_count)}-{shinqlx._thread_name}"
            t = threading.Thread(
                target=func, name=name, args=args, kwargs=kwargs, daemon=True
            )
            t.start()
            shinqlx._thread_count += 1

            return t

    return f
        "#,
        "",
        "",
    )?
    .getattr(intern!(py, "thread"))?;

    thread_func.call1((func.into_py(py), force.into_py(py)))
}

/// Returns a :class:`datetime.timedelta` instance of the time since initialized.
#[pyfunction]
fn uptime(py: Python<'_>) -> PyResult<Bound<'_, PyDelta>> {
    let (elapsed_days, elapsed_seconds, elapsed_microseconds) = py.allow_threads(|| {
        let elapsed = _INIT_TIME.elapsed();
        let elapsed_days: i32 = (elapsed.as_secs() / (24 * 60 * 60))
            .try_into()
            .unwrap_or_default();
        let elapsed_seconds: i32 = (elapsed.as_secs() % (24 * 60 * 60))
            .try_into()
            .unwrap_or_default();
        let elapsed_microseconds: i32 = elapsed.subsec_micros().try_into().unwrap_or_default();
        (elapsed_days, elapsed_seconds, elapsed_microseconds)
    });
    PyDelta::new_bound(
        py,
        elapsed_days,
        elapsed_seconds,
        elapsed_microseconds,
        false,
    )
}

/// Returns the SteamID64 of the owner. This is set in the config.
#[pyfunction]
fn owner(py: Python<'_>) -> PyResult<Option<i64>> {
    let Ok(Some(owner_cvar)) = pyshinqlx_get_cvar(py, "qlx_owner") else {
        error!(target: "shinqlx", "Failed to parse the Owner Steam ID. Make sure it's in SteamID64 format.");
        return Ok(None);
    };

    py.allow_threads(|| {
        let Ok(steam_id) = owner_cvar.parse::<i64>() else {
            error!(target: "shinqlx", "Failed to parse the Owner Steam ID. Make sure it's in SteamID64 format.");
            return Ok(None);
        };

        if steam_id < 0 {
            error!(target: "shinqlx", "Failed to parse the Owner Steam ID. Make sure it's in SteamID64 format.");
            return Ok(None);
        }

        Ok(Some(steam_id))
    })
}

/// Returns the :class:`shinqlx.StatsListener` instance used to listen for stats.
#[pyfunction(name = "stats_listener")]
fn get_stats_listener(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    shinqlx_module.getattr(intern!(py, "_stats"))
}

fn try_get_plugins_version(path: &str) -> Result<String, git2::Error> {
    let repository = git2::Repository::open(path)?;

    let mut describe_options_binding = git2::DescribeOptions::default();
    let describe_options = describe_options_binding
        .describe_tags()
        .show_commit_oid_as_fallback(true);
    let describe = repository.describe(describe_options)?;
    let mut describe_format_options_binding = git2::DescribeFormatOptions::default();
    let desribe_format_options = describe_format_options_binding
        .always_use_long_format(true)
        .dirty_suffix("-dirty");
    let plugins_version = describe.format(Some(desribe_format_options))?;

    let Some(branch) = repository
        .revparse_ext("HEAD")
        .map(|(_, branch_option)| branch_option)
        .ok()
        .flatten()
    else {
        return Ok(plugins_version);
    };

    let Some(branch_name) = branch.shorthand() else {
        return Ok(plugins_version);
    };

    let returned = format!("{}-{}", plugins_version, branch_name);
    Ok(returned)
}

fn get_plugins_version(path: &str) -> String {
    try_get_plugins_version(path).unwrap_or("NOT_SET".to_string())
}

#[pyfunction(name = "set_plugins_version")]
fn set_plugins_version(py: Python<'_>, path: &str) {
    let plugins_version = py.allow_threads(|| get_plugins_version(path));

    if let Ok(shinqlx_module) = py.import_bound(intern!(py, "shinqlx")) {
        let _ = shinqlx_module.setattr(intern!(py, "__plugins_version__"), plugins_version);
    }
}

static DEFAULT_PLUGINS: [&str; 10] = [
    "plugin_manager",
    "essentials",
    "motd",
    "permission",
    "ban",
    "silence",
    "clan",
    "names",
    "log",
    "workshop",
];

#[pyfunction(name = "load_preset_plugins")]
fn load_preset_plugins(py: Python<'_>) -> PyResult<()> {
    if let Err(e) = try_get_plugins_path() {
        return Err(PluginLoadError::new_err(e));
    }

    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return Err(PluginLoadError::new_err("no main engine found"));
    };

    let Some(plugins_cvar) = main_engine.find_cvar("qlx_plugins") else {
        return Ok(());
    };

    let plugins_str = plugins_cvar.get_string();
    let mut plugins: Vec<&str> = plugins_str.split(',').map(|value| value.trim()).collect();
    if plugins.contains(&"DEFAULT") {
        plugins.extend_from_slice(&DEFAULT_PLUGINS);
        plugins.retain(|&value| value != "DEFAULT");
    }
    plugins.iter().unique().for_each(|&plugin| {
        let _ = load_plugin(py, plugin);
    });

    Ok(())
}

fn try_load_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    let plugins_path = try_get_plugins_path().map_err(PyEnvironmentError::new_err)?;

    let os_module = py.import_bound(intern!(py, "os"))?;
    let os_path_module = os_module.getattr(intern!(py, "path"))?;

    let importlib_module = py.import_bound(intern!(py, "importlib"))?;
    let plugins_dir = os_path_module.call_method1(intern!(py, "basename"), (&plugins_path,))?;

    let plugin_import_path = format!("{}.{}", plugins_dir, &plugin);
    let module =
        importlib_module.call_method1(intern!(py, "import_module"), (plugin_import_path,))?;

    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    let plugin_pystring = PyString::new_bound(py, plugin);
    let modules = shinqlx_module.getattr(intern!(py, "_modules"))?;
    modules.set_item(&plugin_pystring, &module)?;

    if !module.hasattr(&plugin_pystring)? {
        return Err(PluginLoadError::new_err(
            "The plugin needs to have a class with the exact name as the file, minus the .py.",
        ));
    }

    let shinqlx_plugin_module = shinqlx_module.getattr(intern!(py, "Plugin"))?;

    let plugin_class = module.getattr(&plugin_pystring)?;
    if !plugin_class
        .get_type()
        .is_subclass(&shinqlx_plugin_module.get_type())
        .unwrap_or(false)
    {
        return Err(PluginLoadError::new_err(
            "Attempted to load a plugin that is not a subclass of 'shinqlx.Plugin'.",
        ));
    }

    let loaded_plugins = shinqlx_plugin_module.getattr(intern!(py, "_loaded_plugins"))?;
    let loaded_plugin = plugin_class.call0()?;
    loaded_plugins.set_item(&plugin_pystring, loaded_plugin)?;

    Ok(())
}

#[pyfunction(name = "load_plugin")]
fn load_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    let logger = pyshinqlx_get_logger(py, None)?;
    let logging_module = py.import_bound(intern!(py, "logging"))?;
    let info_level = logging_module.getattr(intern!(py, "INFO"))?;

    let log_record = logger.call_method(
        intern!(py, "makeRecord"),
        (
            intern!(py, "shinqlx"),
            &info_level,
            intern!(py, ""),
            -1,
            intern!(py, "Loading plugin '%s'..."),
            (plugin,),
            py.None(),
        ),
        Some(&[(intern!(py, "func"), intern!(py, "load_plugin"))].into_py_dict_bound(py)),
    )?;
    logger.call_method1(intern!(py, "handle"), (log_record,))?;

    let Ok(plugins_path) = try_get_plugins_path() else {
        return Err(PluginLoadError::new_err(
            "cvar qlx_pluginsPath misconfigured",
        ));
    };

    let plugin_filename = format!("{}.py", &plugin);
    let joined_path = plugins_path.join(plugin_filename);
    if !joined_path.as_path().is_file() {
        return Err(PluginLoadError::new_err("No such plugin exists."));
    }

    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    let plugin_module = shinqlx_module.getattr(intern!(py, "Plugin"))?;
    let loaded_plugins = plugin_module.getattr(intern!(py, "_loaded_plugins"))?;

    let plugin_loaded = loaded_plugins.contains(plugin)?;
    if plugin_loaded {
        shinqlx_module.call_method1(intern!(py, "reload_plugin"), (plugin,))?;
        return Ok(());
    }

    let plugin_load_result = try_load_plugin(py, plugin);
    if let Err(ref e) = plugin_load_result {
        log_exception(py, e);
    }

    plugin_load_result
}

fn try_unload_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    let event_dispatchers = shinqlx_module.getattr(intern!(py, "EVENT_DISPATCHERS"))?;
    let unload_dispatcher = event_dispatchers.get_item(intern!(py, "unload"))?;
    unload_dispatcher.call_method1(intern!(py, "dispatch"), (plugin,))?;

    let shinqlx_plugin_module = shinqlx_module.getattr(intern!(py, "Plugin"))?;
    let loaded_plugins = shinqlx_plugin_module.getattr(intern!(py, "_loaded_plugins"))?;
    let loaded_plugin = loaded_plugins.get_item(plugin)?;

    let shinqlx_event_dispatchers = shinqlx_module.getattr(intern!(py, "EVENT_DISPATCHERS"))?;
    let plugin_hooks = loaded_plugin.getattr(intern!(py, "hooks"))?;
    plugin_hooks.iter()?.flatten().for_each(|hook| {
        if let Ok(hook_tuple) = hook.extract::<&PyTuple>() {
            let Ok(event_name) = hook_tuple.get_item(0) else {
                return;
            };
            let Ok(event_handler) = hook_tuple.get_item(1) else {
                return;
            };
            let Ok(event_priority) = hook_tuple.get_item(2) else {
                return;
            };
            let Ok(event_dispatcher) = shinqlx_event_dispatchers.get_item(event_name) else {
                return;
            };
            let Ok(plugin_name) = loaded_plugin.getattr(intern!(py, "name")) else {
                return;
            };

            if let Err(ref e) = event_dispatcher.call_method1(
                intern!(py, "remove_hook"),
                (plugin_name, event_handler, event_priority),
            ) {
                log_exception(py, e);
            }
        }
    });

    let shinqlx_commands = shinqlx_module.getattr(intern!(py, "COMMANDS"))?;
    let plugin_commands = loaded_plugin.getattr(intern!(py, "commands"))?;
    plugin_commands.iter()?.flatten().for_each(|cmd| {
        if let Ok(py_cmd) = cmd.extract::<Command>() {
            let result = shinqlx_commands.call_method1(intern!(py, "remove_command"), (py_cmd,));

            if let Err(ref e) = result {
                log_exception(py, e);
            }
        };
    });

    loaded_plugins.del_item(plugin)?;
    Ok(())
}

#[pyfunction(name = "unload_plugin")]
fn unload_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    let logger = pyshinqlx_get_logger(py, None)?;
    let logging_module = py.import_bound(intern!(py, "logging"))?;
    let info_level = logging_module.getattr(intern!(py, "INFO"))?;

    let log_record = logger.call_method(
        intern!(py, "makeRecord"),
        (
            intern!(py, "shinqlx"),
            &info_level,
            intern!(py, ""),
            -1,
            intern!(py, "Unloading plugin '%s'..."),
            (plugin,),
            py.None(),
        ),
        Some(&[(intern!(py, "func"), intern!(py, "unload_plugin"))].into_py_dict_bound(py)),
    )?;
    logger.call_method1(intern!(py, "handle"), (log_record,))?;

    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
    let plugin_module = shinqlx_module.getattr(intern!(py, "Plugin"))?;
    if let Ok(loaded_plugins) = plugin_module.getattr(intern!(py, "_loaded_plugins")) {
        let plugin_loaded = loaded_plugins.contains(plugin)?;
        if !plugin_loaded {
            return Err(PluginUnloadError::new_err(
                "Attempted to unload a plugin that is not loaded.",
            ));
        }
    };

    let plugin_unload_result = try_unload_plugin(py, plugin);
    if let Err(ref e) = plugin_unload_result {
        log_exception(py, e);
    }

    plugin_unload_result
}

fn try_reload_plugin(py: Python, plugin: &str) -> PyResult<()> {
    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;

    if let Ok(loaded_modules) = shinqlx_module.getattr(intern!(py, "_modules")) {
        if loaded_modules.contains(plugin)? {
            let loaded_plugin_module = loaded_modules.get_item(plugin)?;

            let importlib_module = py.import_bound(intern!(py, "importlib"))?;
            let module =
                importlib_module.call_method1(intern!(py, "reload"), (loaded_plugin_module,))?;

            loaded_modules.set_item(plugin, module)?;
        }
    };
    load_plugin(py, plugin)?;
    Ok(())
}

#[pyfunction(name = "reload_plugin")]
fn reload_plugin(py: Python<'_>, plugin: &str) -> PyResult<()> {
    let _ = unload_plugin(py, plugin);

    let plugin_reload_result = try_reload_plugin(py, plugin);
    if let Err(ref e) = plugin_reload_result {
        log_exception(py, e);
    }

    plugin_reload_result
}

#[pyfunction(name = "initialize_cvars")]
fn initialize_cvars(py: Python<'_>) -> PyResult<()> {
    pyshinqlx_set_cvar_once(py, "qlx_owner", "-1".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_plugins", DEFAULT_PLUGINS.join(", ").into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_pluginsPath", "shinqlx-plugins".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_database", "Redis".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_commandPrefix", "!".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_logs", "2".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_logsSize", "3000000".into_py(py), 0)?;

    pyshinqlx_set_cvar_once(py, "qlx_redisAddress", "127.0.0.1".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_redisDatabase", "0".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_redisUnixSocket", "0".into_py(py), 0)?;
    pyshinqlx_set_cvar_once(py, "qlx_redisPassword", "".into_py(py), 0)?;

    Ok(())
}

#[pyfunction(name = "initialize")]
fn initialize(_py: Python<'_>) {
    register_handlers()
}

fn try_get_plugins_path() -> Result<std::path::PathBuf, &'static str> {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return Err("no main engine found");
    };

    let Some(plugins_path_cvar) = main_engine.find_cvar("qlx_pluginsPath") else {
        return Err("qlx_pluginsPath cvar not found");
    };
    let plugins_path_str = plugins_path_cvar.get_string();

    let plugins_path = std::path::Path::new(plugins_path_str.as_ref());
    if !plugins_path.is_dir() {
        return Err("qlx_pluginsPath is not pointing to an existing directory");
    }

    plugins_path
        .canonicalize()
        .map_err(|_| "canonicalize returned an error")
}

/// Initialization that needs to be called after QLDS has finished
/// its own initialization.
#[pyfunction(name = "late_init")]
fn late_init(py: Python<'_>) -> PyResult<()> {
    let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;

    initialize_cvars(py)?;

    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return Err(PyEnvironmentError::new_err("no main engine found"));
    };

    let database_cvar = main_engine.find_cvar("qlx_database");
    if database_cvar.is_some_and(|value| value.get_string().to_lowercase() == "redis") {
        let database_module = shinqlx_module.getattr(intern!(py, "database"))?;
        let redis_database_module = database_module.getattr(intern!(py, "Redis"))?;

        let plugin_module = shinqlx_module.getattr(intern!(py, "Plugin"))?;
        plugin_module.setattr(intern!(py, "database"), redis_database_module)?;
    }

    let sys_module = py.import_bound(intern!(py, "sys"))?;

    if let Ok(real_plugins_path) = try_get_plugins_path() {
        set_plugins_version(py, &real_plugins_path.to_string_lossy());

        let Some(plugins_path_dirname) = real_plugins_path
            .parent()
            .map(|value| value.to_string_lossy())
        else {
            return Err(PyEnvironmentError::new_err(
                "could not determine directory name of qlx_pluginsPath",
            ));
        };
        let sys_path_module = sys_module.getattr(intern!(py, "path"))?;
        sys_path_module.call_method1(intern!(py, "append"), (plugins_path_dirname,))?;
    }

    pyshinqlx_configure_logger(py)?;
    let logger = pyshinqlx_get_logger(py, None)?;

    let logging_module = py.import_bound(intern!(py, "logging"))?;
    let info_level = logging_module.getattr(intern!(py, "INFO"))?;

    let handle_exception = shinqlx_module.getattr(intern!(py, "handle_exception"))?;
    sys_module.setattr(intern!(py, "excepthook"), handle_exception)?;

    let threading_module = py.import_bound(intern!(py, "threading"))?;
    let threading_except_hook = shinqlx_module.getattr(intern!(py, "threading_excepthook"))?;
    threading_module.setattr(intern!(py, "excepthook"), threading_except_hook)?;

    let log_record = logger.call_method(
        intern!(py, "makeRecord"),
        (
            intern!(py, "shinqlx"),
            &info_level,
            intern!(py, ""),
            -1,
            intern!(py, "Loading preset plugins..."),
            py.None(),
            py.None(),
        ),
        Some(&[(intern!(py, "func"), intern!(py, "late_init"))].into_py_dict_bound(py)),
    )?;
    logger.call_method1(intern!(py, "handle"), (log_record,))?;

    shinqlx_module.call_method0(intern!(py, "load_preset_plugins"))?;

    let stats_enable_cvar = main_engine.find_cvar("zmq_stats_enable");
    if stats_enable_cvar.is_some_and(|value| value.get_string() != "0") {
        shinqlx_module.setattr(
            intern!(py, "_stats"),
            Py::new(py, StatsListener::py_new()?)?,
        )?;
        let stats_value = shinqlx_module.getattr(intern!(py, "_stats"))?;

        let stats_address = stats_value.getattr(intern!(py, "address"))?;
        let log_record = logger.call_method(
            intern!(py, "makeRecord"),
            (
                intern!(py, "shinqlx"),
                &info_level,
                intern!(py, ""),
                -1,
                intern!(py, "Stats listener started on %s."),
                (stats_address,),
                py.None(),
            ),
            Some(&[(intern!(py, "func"), intern!(py, "late_init"))].into_py_dict_bound(py)),
        )?;
        logger.call_method1(intern!(py, "handle"), (log_record,))?;

        stats_value.call_method0(intern!(py, "keep_receiving"))?;
    }

    let log_record = logger.call_method(
        intern!(py, "makeRecord"),
        (
            intern!(py, "shinqlx"),
            &info_level,
            intern!(py, ""),
            -1,
            intern!(py, "We're good to go!"),
            py.None(),
            py.None(),
        ),
        Some(&[(intern!(py, "func"), intern!(py, "late_init"))].into_py_dict_bound(py)),
    )?;
    logger.call_method1(intern!(py, "handle"), (log_record,))?;

    Ok(())
}

#[pymodule]
#[pyo3(name = "shinqlx")]
fn pyshinqlx_root_module(_py: Python<'_>, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}

#[pymodule]
#[pyo3(name = "_shinqlx")]
fn pyshinqlx_module(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // from __init__.py
    m.add("_map_title", "")?;
    m.add("_map_subtitle1", "")?;
    m.add("_map_subtitle2", "")?;

    // from _shinqlx.pyi
    m.add("__version__", env!("SHINQLX_VERSION"))?;
    m.add("DEBUG", cfg!(debug_assertions))?;

    // Set a bunch of constants. We set them here because if you define functions in Python that use module
    // constants as keyword defaults, we have to always make sure they're exported first, and fuck that.
    m.add("RET_NONE", PythonReturnCodes::RET_NONE as i32)?;
    m.add("RET_STOP", PythonReturnCodes::RET_STOP as i32)?;
    m.add("RET_STOP_EVENT", PythonReturnCodes::RET_STOP_EVENT as i32)?;
    m.add("RET_STOP_ALL", PythonReturnCodes::RET_STOP_ALL as i32)?;
    m.add("RET_USAGE", PythonReturnCodes::RET_USAGE as i32)?;
    m.add("PRI_HIGHEST", CommandPriorities::PRI_HIGHEST as i32)?;
    m.add("PRI_HIGH", CommandPriorities::PRI_HIGH as i32)?;
    m.add("PRI_NORMAL", CommandPriorities::PRI_NORMAL as i32)?;
    m.add("PRI_LOW", CommandPriorities::PRI_LOW as i32)?;
    m.add("PRI_LOWEST", CommandPriorities::PRI_LOWEST as i32)?;

    // Cvar flags.
    m.add("CVAR_ARCHIVE", cvar_flags::CVAR_ARCHIVE as i32)?;
    m.add("CVAR_USERINFO", cvar_flags::CVAR_USERINFO as i32)?;
    m.add("CVAR_SERVERINFO", cvar_flags::CVAR_SERVERINFO as i32)?;
    m.add("CVAR_SYSTEMINFO", cvar_flags::CVAR_SYSTEMINFO as i32)?;
    m.add("CVAR_INIT", cvar_flags::CVAR_INIT as i32)?;
    m.add("CVAR_LATCH", cvar_flags::CVAR_LATCH as i32)?;
    m.add("CVAR_ROM", cvar_flags::CVAR_ROM as i32)?;
    m.add("CVAR_USER_CREATED", cvar_flags::CVAR_USER_CREATED as i32)?;
    m.add("CVAR_TEMP", cvar_flags::CVAR_TEMP as i32)?;
    m.add("CVAR_CHEAT", cvar_flags::CVAR_CHEAT as i32)?;
    m.add("CVAR_NORESTART", cvar_flags::CVAR_NORESTART as i32)?;

    // Privileges.
    m.add("PRIV_NONE", privileges_t::PRIV_NONE as i32)?;
    m.add("PRIV_MOD", privileges_t::PRIV_MOD as i32)?;
    m.add("PRIV_ADMIN", privileges_t::PRIV_ADMIN as i32)?;
    m.add("PRIV_ROOT", privileges_t::PRIV_ROOT as i32)?;
    m.add("PRIV_BANNED", privileges_t::PRIV_BANNED as i32)?;

    // Connection states.
    m.add("CS_FREE", clientState_t::CS_FREE as i32)?;
    m.add("CS_ZOMBIE", clientState_t::CS_ZOMBIE as i32)?;
    m.add("CS_CONNECTED", clientState_t::CS_CONNECTED as i32)?;
    m.add("CS_PRIMED", clientState_t::CS_PRIMED as i32)?;
    m.add("CS_ACTIVE", clientState_t::CS_ACTIVE as i32)?;

    // Teams.
    m.add("TEAM_FREE", team_t::TEAM_FREE as i32)?;
    m.add("TEAM_RED", team_t::TEAM_RED as i32)?;
    m.add("TEAM_BLUE", team_t::TEAM_BLUE as i32)?;
    m.add("TEAM_SPECTATOR", team_t::TEAM_SPECTATOR as i32)?;

    // Means of death.
    m.add("MOD_UNKNOWN", meansOfDeath_t::MOD_UNKNOWN as i32)?;
    m.add("MOD_SHOTGUN", meansOfDeath_t::MOD_SHOTGUN as i32)?;
    m.add("MOD_GAUNTLET", meansOfDeath_t::MOD_GAUNTLET as i32)?;
    m.add("MOD_MACHINEGUN", meansOfDeath_t::MOD_MACHINEGUN as i32)?;
    m.add("MOD_GRENADE", meansOfDeath_t::MOD_GRENADE as i32)?;
    m.add(
        "MOD_GRENADE_SPLASH",
        meansOfDeath_t::MOD_GRENADE_SPLASH as i32,
    )?;
    m.add("MOD_ROCKET", meansOfDeath_t::MOD_ROCKET as i32)?;
    m.add(
        "MOD_ROCKET_SPLASH",
        meansOfDeath_t::MOD_ROCKET_SPLASH as i32,
    )?;
    m.add("MOD_PLASMA", meansOfDeath_t::MOD_PLASMA as i32)?;
    m.add(
        "MOD_PLASMA_SPLASH",
        meansOfDeath_t::MOD_PLASMA_SPLASH as i32,
    )?;
    m.add("MOD_RAILGUN", meansOfDeath_t::MOD_RAILGUN as i32)?;
    m.add("MOD_LIGHTNING", meansOfDeath_t::MOD_LIGHTNING as i32)?;
    m.add("MOD_BFG", meansOfDeath_t::MOD_BFG as i32)?;
    m.add("MOD_BFG_SPLASH", meansOfDeath_t::MOD_BFG_SPLASH as i32)?;
    m.add("MOD_WATER", meansOfDeath_t::MOD_WATER as i32)?;
    m.add("MOD_SLIME", meansOfDeath_t::MOD_SLIME as i32)?;
    m.add("MOD_LAVA", meansOfDeath_t::MOD_LAVA as i32)?;
    m.add("MOD_CRUSH", meansOfDeath_t::MOD_CRUSH as i32)?;
    m.add("MOD_TELEFRAG", meansOfDeath_t::MOD_TELEFRAG as i32)?;
    m.add("MOD_FALLING", meansOfDeath_t::MOD_FALLING as i32)?;
    m.add("MOD_SUICIDE", meansOfDeath_t::MOD_SUICIDE as i32)?;
    m.add("MOD_TARGET_LASER", meansOfDeath_t::MOD_TARGET_LASER as i32)?;
    m.add("MOD_TRIGGER_HURT", meansOfDeath_t::MOD_TRIGGER_HURT as i32)?;
    m.add("MOD_NAIL", meansOfDeath_t::MOD_NAIL as i32)?;
    m.add("MOD_CHAINGUN", meansOfDeath_t::MOD_CHAINGUN as i32)?;
    m.add(
        "MOD_PROXIMITY_MINE",
        meansOfDeath_t::MOD_PROXIMITY_MINE as i32,
    )?;
    m.add("MOD_KAMIKAZE", meansOfDeath_t::MOD_KAMIKAZE as i32)?;
    m.add("MOD_JUICED", meansOfDeath_t::MOD_JUICED as i32)?;
    m.add("MOD_GRAPPLE", meansOfDeath_t::MOD_GRAPPLE as i32)?;
    m.add("MOD_SWITCH_TEAMS", meansOfDeath_t::MOD_SWITCH_TEAMS as i32)?;
    m.add("MOD_THAW", meansOfDeath_t::MOD_THAW as i32)?;
    m.add(
        "MOD_LIGHTNING_DISCHARGE",
        meansOfDeath_t::MOD_LIGHTNING_DISCHARGE as i32,
    )?;
    m.add("MOD_HMG", meansOfDeath_t::MOD_HMG as i32)?;
    m.add(
        "MOD_RAILGUN_HEADSHOT",
        meansOfDeath_t::MOD_RAILGUN_HEADSHOT as i32,
    )?;

    m.add("DAMAGE_RADIUS", DAMAGE_RADIUS as i32)?;
    m.add("DAMAGE_NO_ARMOR", DAMAGE_NO_ARMOR as i32)?;
    m.add("DAMAGE_NO_KNOCKBACK", DAMAGE_NO_KNOCKBACK as i32)?;
    m.add("DAMAGE_NO_PROTECTION", DAMAGE_NO_PROTECTION as i32)?;
    m.add(
        "DAMAGE_NO_TEAM_PROTECTION",
        DAMAGE_NO_TEAM_PROTECTION as i32,
    )?;

    m.add_class::<Vector3>()?;
    m.add_class::<Flight>()?;
    m.add_class::<Powerups>()?;
    m.add_class::<Weapons>()?;
    m.add_class::<PlayerInfo>()?;
    m.add_class::<PlayerState>()?;
    m.add_class::<PlayerStats>()?;

    m.add_function(wrap_pyfunction!(pyshinqlx_player_info, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_players_info, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_userinfo, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_send_server_command, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_client_command, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_console_command, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_cvar, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar_limit, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_kick, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_console_print, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_force_vote, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_add_console_command, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_register_handler, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_player_state, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_player_stats, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_position, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_velocity, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_noclip, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_health, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_armor, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_weapons, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_weapon, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_ammo, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_powerups, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_holdable, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_drop_holdable, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_flight, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_invulnerability, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_score, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_callvote, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_allow_single_player, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_player_spawn, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_privileges, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_destroy_kamikaze_timers, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_spawn_item, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_remove_dropped_items, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_slay_with_mod, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_replace_items, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_dev_print_items, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_force_weapon_respawn_time, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_entity_targets, m)?)?;

    // from _core.py
    m.add("PluginLoadError", py.get_type_bound::<PluginLoadError>())?;
    m.add(
        "PluginUnloadError",
        py.get_type_bound::<PluginUnloadError>(),
    )?;

    m.add(
        "TEAMS",
        &[
            (team_t::TEAM_FREE as i32, "free"),
            (team_t::TEAM_RED as i32, "red"),
            (team_t::TEAM_BLUE as i32, "blue"),
            (team_t::TEAM_SPECTATOR as i32, "spectator"),
        ]
        .into_py_dict_bound(py),
    )?;
    // Game types
    m.add(
        "GAMETYPES",
        &[
            (0, "Free for All"),
            (1, "Duel"),
            (2, "Race"),
            (3, "Team Deathmatch"),
            (4, "Clan Arena"),
            (5, "Capture the Flag"),
            (6, "One Flag"),
            (8, "Harvester"),
            (9, "Freeze Tag"),
            (10, "Domination"),
            (11, "Attack and Defend"),
            (12, "Red Rover"),
        ]
        .into_py_dict_bound(py),
    )?;
    m.add(
        "GAMETYPES_SHORT",
        &[
            (0, "ffa"),
            (1, "duel"),
            (2, "race"),
            (3, "tdm"),
            (4, "ca"),
            (5, "ctf"),
            (6, "1f"),
            (8, "har"),
            (9, "ft"),
            (10, "dom"),
            (11, "ad"),
            (12, "rr"),
        ]
        .into_py_dict_bound(py),
    )?;
    m.add(
        "CONNECTION_STATES",
        &[
            (clientState_t::CS_FREE as i32, "free"),
            (clientState_t::CS_ZOMBIE as i32, "zombie"),
            (clientState_t::CS_CONNECTED as i32, "connected"),
            (clientState_t::CS_PRIMED as i32, "primed"),
            (clientState_t::CS_ACTIVE as i32, "active"),
        ]
        .into_py_dict_bound(py),
    )?;
    // Weapons
    m.add(
        "WEAPONS",
        &[
            (weapon_t::WP_GAUNTLET as i32, "g"),
            (weapon_t::WP_MACHINEGUN as i32, "mg"),
            (weapon_t::WP_SHOTGUN as i32, "sg"),
            (weapon_t::WP_GRENADE_LAUNCHER as i32, "gl"),
            (weapon_t::WP_ROCKET_LAUNCHER as i32, "rl"),
            (weapon_t::WP_LIGHTNING as i32, "lg"),
            (weapon_t::WP_RAILGUN as i32, "rg"),
            (weapon_t::WP_PLASMAGUN as i32, "pg"),
            (weapon_t::WP_BFG as i32, "bfg"),
            (weapon_t::WP_GRAPPLING_HOOK as i32, "gh"),
            (weapon_t::WP_NAILGUN as i32, "ng"),
            (weapon_t::WP_PROX_LAUNCHER as i32, "pl"),
            (weapon_t::WP_CHAINGUN as i32, "cg"),
            (weapon_t::WP_HMG as i32, "hmg"),
            (weapon_t::WP_HANDS as i32, "hands"),
        ]
        .into_py_dict_bound(py),
    )?;
    m.add("DEFAULT_PLUGINS", PyTuple::new_bound(py, DEFAULT_PLUGINS))?;

    m.add("_thread_count", 0)?;
    m.add("_thread_name", "shinqlxthread")?;

    m.add("_stats", py.None())?;
    m.add("_modules", PyDict::new_bound(py))?;

    m.add_function(wrap_pyfunction!(pyshinqlx_parse_variables, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_get_logger, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_configure_logger, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_log_exception, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_handle_exception, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_handle_threading_exception, m)?)?;
    m.add_function(wrap_pyfunction!(uptime, m)?)?;
    m.add_function(wrap_pyfunction!(owner, m)?)?;
    m.add_function(wrap_pyfunction!(get_stats_listener, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar_once, m)?)?;
    m.add_function(wrap_pyfunction!(pyshinqlx_set_cvar_limit_once, m)?)?;
    m.add_function(wrap_pyfunction!(set_plugins_version, m)?)?;
    m.add_function(wrap_pyfunction!(set_map_subtitles, m)?)?;
    m.add_function(wrap_pyfunction!(next_frame, m)?)?;
    m.add_function(wrap_pyfunction!(delay, m)?)?;
    m.add_function(wrap_pyfunction!(thread, m)?)?;
    m.add_function(wrap_pyfunction!(load_preset_plugins, m)?)?;
    m.add_function(wrap_pyfunction!(load_plugin, m)?)?;
    m.add_function(wrap_pyfunction!(unload_plugin, m)?)?;
    m.add_function(wrap_pyfunction!(reload_plugin, m)?)?;
    m.add_function(wrap_pyfunction!(initialize_cvars, m)?)?;
    m.add_function(wrap_pyfunction!(initialize, m)?)?;
    m.add_function(wrap_pyfunction!(late_init, m)?)?;

    // from _game.py
    m.add_class::<Game>()?;
    m.add(
        "NonexistentGameError",
        py.get_type_bound::<NonexistentGameError>(),
    )?;

    // from _player.py
    m.add_class::<Player>()?;
    m.add(
        "NonexistentPlayerError",
        py.get_type_bound::<NonexistentPlayerError>(),
    )?;
    m.add_class::<AbstractDummyPlayer>()?;
    m.add_class::<RconDummyPlayer>()?;

    // from _commands.py
    m.add("MAX_MSG_LENGTH", MAX_MSG_LENGTH)?;
    let regex_module = py.import_bound("re")?;
    m.add(
        "re_color_tag",
        regex_module.call_method1("compile", (r"\^[0-7]",))?,
    )?;
    m.add_class::<AbstractChannel>()?;
    m.add_class::<ChatChannel>()?;
    m.add_class::<TeamChatChannel>()?;
    m.add_class::<TellChannel>()?;
    m.add_class::<ConsoleChannel>()?;
    m.add_class::<ClientCommandChannel>()?;
    m.add(
        "CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new("all", "chat", "print \"{}\n\"\n"),
        )?
        .to_object(py),
    )?;
    m.add(
        "RED_TEAM_CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new("red", "red_team_chat", "print \"{}\n\"\n"),
        )?
        .to_object(py),
    )?;
    m.add(
        "BLUE_TEAM_CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new("blue", "blue_team_chat", "print \"{}\n\"\n"),
        )?
        .to_object(py),
    )?;
    m.add(
        "FREE_CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new("free", "free_chat", "print \"{}\n\"\n"),
        )?
        .to_object(py),
    )?;
    m.add(
        "SPECTATOR_CHAT_CHANNEL",
        Py::new(
            py,
            TeamChatChannel::py_new("spectator", "spectator_chat", "print \"{}\n\"\n"),
        )?
        .to_object(py),
    )?;
    m.add(
        "CONSOLE_CHANNEL",
        Py::new(py, ConsoleChannel::py_new())?.to_object(py),
    )?;
    m.add_class::<Command>()?;
    m.add_class::<CommandInvoker>()?;
    m.add(
        "COMMANDS",
        Py::new(py, CommandInvoker::py_new())?.to_object(py),
    )?;

    // from _handlers.py
    let sched_module = py.import_bound("sched")?;
    m.add("frame_tasks", sched_module.call_method0("scheduler")?)?;
    let queue_module = py.import_bound("queue")?;
    m.add("next_frame_tasks", queue_module.call_method0("Queue")?)?;

    m.add_function(wrap_pyfunction!(handlers::handle_rcon, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_client_command, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_server_command, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_frame, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_new_game, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_set_configstring, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_player_connect, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_player_loaded, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_player_disconnect, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_player_spawn, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_kamikaze_use, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_kamikaze_explode, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_damage, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::handle_console_print, m)?)?;
    m.add_function(wrap_pyfunction!(handlers::redirect_print, m)?)?;
    m.add_class::<handlers::PrintRedirector>()?;
    m.add_function(wrap_pyfunction!(handlers::register_handlers, m)?)?;

    // from _events.py
    let regex_module = py.import_bound("re")?;
    m.add(
        "_re_vote",
        regex_module.call_method1("compile", (r#"^(?P<cmd>[^ ]+)(?: "?(?P<args>.*?)"?)?$"#,))?,
    )?;
    m.add_class::<EventDispatcher>()?;
    m.add_class::<ConsolePrintDispatcher>()?;
    m.add_class::<CommandDispatcher>()?;
    m.add_class::<ClientCommandDispatcher>()?;
    m.add_class::<ServerCommandDispatcher>()?;
    m.add_class::<FrameEventDispatcher>()?;
    m.add_class::<SetConfigstringDispatcher>()?;
    m.add_class::<ChatEventDispatcher>()?;
    m.add_class::<UnloadDispatcher>()?;
    m.add_class::<PlayerConnectDispatcher>()?;
    m.add_class::<PlayerLoadedDispatcher>()?;
    m.add_class::<PlayerDisconnectDispatcher>()?;
    m.add_class::<PlayerSpawnDispatcher>()?;
    m.add_class::<StatsDispatcher>()?;
    m.add_class::<VoteCalledDispatcher>()?;
    m.add_class::<VoteStartedDispatcher>()?;
    m.add_class::<VoteEndedDispatcher>()?;
    m.add_class::<VoteDispatcher>()?;
    m.add_class::<GameCountdownDispatcher>()?;
    m.add_class::<GameStartDispatcher>()?;
    m.add_class::<GameEndDispatcher>()?;
    m.add_class::<RoundCountdownDispatcher>()?;
    m.add_class::<RoundStartDispatcher>()?;
    m.add_class::<RoundEndDispatcher>()?;
    m.add_class::<TeamSwitchDispatcher>()?;
    m.add_class::<TeamSwitchAttemptDispatcher>()?;
    m.add_class::<MapDispatcher>()?;
    m.add_class::<NewGameDispatcher>()?;
    m.add_class::<KillDispatcher>()?;
    m.add_class::<DeathDispatcher>()?;
    m.add_class::<UserinfoDispatcher>()?;
    m.add_class::<KamikazeUseDispatcher>()?;
    m.add_class::<KamikazeExplodeDispatcher>()?;
    m.add_class::<DamageDispatcher>()?;
    m.add_class::<EventDispatcherManager>()?;

    let event_dispatchers = EventDispatcherManager::default();
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<ConsolePrintDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<CommandDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<ClientCommandDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<ServerCommandDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<FrameEventDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<SetConfigstringDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<ChatEventDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<UnloadDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<PlayerConnectDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<PlayerLoadedDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<PlayerDisconnectDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<PlayerSpawnDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<KamikazeUseDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<KamikazeExplodeDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<StatsDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<VoteCalledDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<VoteStartedDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<VoteEndedDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<VoteDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<GameCountdownDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<GameStartDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<GameEndDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<RoundCountdownDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<RoundStartDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<RoundEndDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<TeamSwitchDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<TeamSwitchAttemptDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<MapDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<NewGameDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<KillDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<DeathDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<UserinfoDispatcher>())?;
    event_dispatchers.add_dispatcher(py, py.get_type_bound::<DamageDispatcher>())?;
    m.add(
        "EVENT_DISPATCHERS",
        Py::new(py, event_dispatchers)?.to_object(py),
    )?;

    // from _zmq.py
    m.add_class::<StatsListener>()?;

    Ok(())
}

pub(crate) static PYSHINQLX_INITIALIZED: AtomicBool = AtomicBool::new(false);

pub(crate) fn pyshinqlx_is_initialized() -> bool {
    PYSHINQLX_INITIALIZED.load(Ordering::SeqCst)
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(crate) enum PythonInitializationError {
    MainScriptError,
    #[cfg_attr(test, allow(dead_code))]
    AlreadyInitialized,
    NotInitializedError,
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn pyshinqlx_initialize() -> Result<(), PythonInitializationError> {
    if pyshinqlx_is_initialized() {
        error!(target: "shinqlx", "pyshinqlx_initialize was called while already initialized");
        return Err(PythonInitializationError::AlreadyInitialized);
    }

    debug!(target: "shinqlx", "Initializing Python...");
    append_to_inittab!(pyshinqlx_module);
    prepare_freethreaded_python();
    let init_result = Python::with_gil(|py| {
        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
        shinqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    });
    match init_result {
        Err(e) => {
            error!(target: "shinqlx", "{:?}", e);
            error!(target: "shinqlx", "loader sequence returned an error. Did you modify the loader?");
            Err(PythonInitializationError::MainScriptError)
        }
        Ok(_) => {
            PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            debug!(target: "shinqlx", "Python initialized!");
            Ok(())
        }
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn pyshinqlx_reload() -> Result<(), PythonInitializationError> {
    if !pyshinqlx_is_initialized() {
        error!(target: "shinqlx", "pyshinqlx_finalize was called before being initialized");
        return Err(PythonInitializationError::NotInitializedError);
    }

    CUSTOM_COMMAND_HANDLER.store(None);

    let reinit_result = Python::with_gil(|py| {
        let importlib_module = py.import_bound("importlib")?;
        let shinqlx_module = py.import_bound(intern!(py, "shinqlx"))?;
        let new_shinqlx_module = importlib_module.call_method1("reload", (shinqlx_module,))?;
        new_shinqlx_module.call_method0("initialize")?;
        Ok::<(), PyErr>(())
    });
    match reinit_result {
        Err(_) => {
            PYSHINQLX_INITIALIZED.store(false, Ordering::SeqCst);
            Err(PythonInitializationError::MainScriptError)
        }
        Ok(()) => {
            PYSHINQLX_INITIALIZED.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, mockall::automock)]
#[allow(dead_code)]
pub(crate) mod python_tests {
    use super::PythonInitializationError;

    pub(crate) fn rcon_dispatcher<T>(_cmd: T)
    where
        T: AsRef<str> + 'static,
    {
    }

    pub(crate) fn client_command_dispatcher(_client_id: i32, _cmd: String) -> Option<String> {
        None
    }

    pub(crate) fn server_command_dispatcher(
        _client_id: Option<i32>,
        _cmd: String,
    ) -> Option<String> {
        None
    }

    pub(crate) fn client_loaded_dispatcher(_client_id: i32) {}

    pub(crate) fn set_configstring_dispatcher(_index: u32, _value: &str) -> Option<String> {
        None
    }

    pub(crate) fn client_disconnect_dispatcher(_client_id: i32, _reason: &str) {}

    pub(crate) fn console_print_dispatcher(_msg: &str) -> Option<String> {
        None
    }

    pub(crate) fn new_game_dispatcher(_restart: bool) {}

    pub(crate) fn frame_dispatcher() {}

    pub(crate) fn client_connect_dispatcher(_client_id: i32, _is_bot: bool) -> Option<String> {
        None
    }

    pub(crate) fn client_spawn_dispatcher(_client_id: i32) {}

    pub(crate) fn kamikaze_use_dispatcher(_client_id: i32) {}

    pub(crate) fn kamikaze_explode_dispatcher(_client_id: i32, _is_used_on_demand: bool) {}

    pub(crate) fn damage_dispatcher(
        _target_client_id: i32,
        _attacker_client_id: Option<i32>,
        _damage: i32,
        _dflags: i32,
        _means_of_death: i32,
    ) {
    }

    pub(crate) fn pyshinqlx_is_initialized() -> bool {
        false
    }

    pub(crate) fn pyshinqlx_initialize() -> Result<(), PythonInitializationError> {
        Ok(())
    }

    pub(crate) fn pyshinqlx_reload() -> Result<(), PythonInitializationError> {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod pyshinqlx_setup_fixture {
    use super::pyshinqlx_module;

    use pyo3::{append_to_inittab, ffi::Py_IsInitialized, prepare_freethreaded_python};
    use rstest::fixture;

    #[fixture]
    #[once]
    pub(crate) fn pyshinqlx_setup() {
        if unsafe { Py_IsInitialized() } == 0 {
            append_to_inittab!(pyshinqlx_module);
            prepare_freethreaded_python();
        }
    }
}
