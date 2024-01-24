use crate::prelude::*;
use pyo3::prelude::*;

fn try_log_exception(py: Python<'_>, exception: PyErr) -> PyResult<()> {
    let logging_module = py.import("logging")?;
    let traceback_module = py.import("traceback")?;

    let py_logger = logging_module.call_method1("getLogger", ("shinqlx",))?;

    let formatted_traceback: Vec<String> = traceback_module
        .call_method1(
            "format_exception",
            (
                exception.get_type(py),
                exception.value(py),
                exception.traceback(py),
            ),
        )?
        .extract()?;

    formatted_traceback.iter().for_each(|line| {
        let _ = py_logger.call_method1("error", (line.trim_end(),));
    });

    Ok(())
}

fn log_exception(py: Python<'_>, exception: PyErr) {
    let _ = try_log_exception(py, exception);
}

fn try_handle_rcon(py: Python<'_>, cmd: String) -> PyResult<Option<bool>> {
    let rcon_dummy_player = Py::new(py, RconDummyPlayer::py_new())?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_console_channel = shinqlx_module.getattr("CONSOLE_CHANNEL")?;

    let shinqlx_commands = shinqlx_module.getattr("COMMANDS")?;
    let _ = shinqlx_commands.call_method1(
        "handle_input",
        (rcon_dummy_player, cmd, shinqlx_console_channel),
    )?;
    Ok(None)
}

/// Console commands that are to be processed as regular pyshinqlx
/// commands as if the owner executes it. This allows the owner to
/// interact with the Python part of shinqlx without having to connect.
#[pyfunction]
pub(crate) fn handle_rcon(py: Python<'_>, cmd: String) -> Option<bool> {
    try_handle_rcon(py, cmd).unwrap_or_else(|e| {
        log_exception(py, e);
        Some(true)
    })
}

fn try_handle_player_connect(py: Python<'_>, client_id: i32, _is_bot: bool) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let player_connect_dispatcher = shinqlx_event_dispatchers.get_item("player_connect")?;
    player_connect_dispatcher
        .call_method1("dispatch", (player,))
        .map(|value| value.into_py(py))
}

/// This will be called whenever a player tries to connect. If the dispatcher
/// returns False, it will not allow the player to connect and instead show them
/// a message explaining why. The default message is "You are banned from this
/// server.", but it can be set with :func:`shinqlx.set_ban_message`.
#[pyfunction]
pub(crate) fn handle_player_connect(py: Python<'_>, client_id: i32, is_bot: bool) -> PyObject {
    try_handle_player_connect(py, client_id, is_bot).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_player_loaded(py: Python<'_>, client_id: i32) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let player_loaded_dispatcher = shinqlx_event_dispatchers.get_item("player_loaded")?;
    player_loaded_dispatcher
        .call_method1("dispatch", (player,))
        .map(|value| value.into_py(py))
}

/// This will be called whenever a player has connected and finished loading,
/// meaning it'll go off a bit later than the usual "X connected" messages.
/// This will not trigger on bots.his will be called whenever a player tries to connect. If the dispatcher
#[pyfunction]
pub(crate) fn handle_player_loaded(py: Python<'_>, client_id: i32) -> PyObject {
    try_handle_player_loaded(py, client_id).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_player_disconnect(
    py: Python<'_>,
    client_id: i32,
    reason: Option<String>,
) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let player_disconnect_dispatcher = shinqlx_event_dispatchers.get_item("player_disconnect")?;
    player_disconnect_dispatcher
        .call_method1("dispatch", (player, reason))
        .map(|value| value.into_py(py))
}

/// This will be called whenever a player disconnects.
#[pyfunction]
pub(crate) fn handle_player_disconnect(
    py: Python<'_>,
    client_id: i32,
    reason: Option<String>,
) -> PyObject {
    try_handle_player_disconnect(py, client_id, reason).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_player_spawn(py: Python<'_>, client_id: i32) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let player_spawn_dispatcher = shinqlx_event_dispatchers.get_item("player_spawn")?;
    player_spawn_dispatcher
        .call_method1("dispatch", (player,))
        .map(|value| value.into_py(py))
}

/// Called when a player spawns. Note that a spectator going in free spectate mode
/// makes the client spawn, so you'll want to check for that if you only want "actual"
/// spawns.
#[pyfunction]
pub(crate) fn handle_player_spawn(py: Python<'_>, client_id: i32) -> PyObject {
    try_handle_player_spawn(py, client_id).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_kamikaze_use(py: Python<'_>, client_id: i32) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let kamikaze_use_dispatcher = shinqlx_event_dispatchers.get_item("kamikaze_use")?;
    kamikaze_use_dispatcher
        .call_method1("dispatch", (player,))
        .map(|value| value.into_py(py))
}

/// This will be called whenever player uses kamikaze item.
#[pyfunction]
pub(crate) fn handle_kamikaze_use(py: Python<'_>, client_id: i32) -> PyObject {
    try_handle_kamikaze_use(py, client_id).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_kamikaze_explode(
    py: Python<'_>,
    client_id: i32,
    is_used_on_demand: bool,
) -> PyResult<PyObject> {
    let player = Player::py_new(client_id, None)?;

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let kamikaze_explode_dispatcher = shinqlx_event_dispatchers.get_item("kamikaze_explode")?;
    kamikaze_explode_dispatcher
        .call_method1("dispatch", (player, is_used_on_demand))
        .map(|value| value.into_py(py))
}

/// This will be called whenever kamikaze explodes.
#[pyfunction]
pub(crate) fn handle_kamikaze_explode(
    py: Python<'_>,
    client_id: i32,
    is_used_on_demand: bool,
) -> PyObject {
    try_handle_kamikaze_explode(py, client_id, is_used_on_demand).unwrap_or_else(|e| {
        log_exception(py, e);
        true.into_py(py)
    })
}

fn try_handle_damage(
    py: Python<'_>,
    target_id: i32,
    attacker_id: Option<i32>,
    damage: i32,
    dflags: i32,
    means_of_death: i32,
) -> PyResult<Option<bool>> {
    let target_player = if (0..MAX_CLIENTS as i32).contains(&target_id) {
        Player::py_new(target_id, None)?.into_py(py)
    } else {
        target_id.into_py(py)
    };

    let attacker_player = attacker_id.and_then(|attacker_id| {
        if (0..MAX_CLIENTS as i32).contains(&attacker_id) {
            Player::py_new(attacker_id, None)
                .ok()
                .map(|player| player.into_py(py))
        } else {
            Some(attacker_id.into_py(py))
        }
    });

    let shinqlx_module = py.import("shinqlx")?;
    let shinqlx_event_dispatchers = shinqlx_module.getattr("EVENT_DISPATCHERS")?;
    let damage_dispatcher = shinqlx_event_dispatchers.get_item("damage")?;
    let _ = damage_dispatcher.call_method1(
        "dispatch",
        (
            target_player,
            attacker_player,
            damage,
            dflags,
            means_of_death,
        ),
    )?;
    Ok(None)
}

#[pyfunction]
#[pyo3(signature = (target_id, attacker_id, damage, dflags, means_of_death))]
pub(crate) fn handle_damage(
    py: Python<'_>,
    target_id: i32,
    attacker_id: Option<i32>,
    damage: i32,
    dflags: i32,
    means_of_death: i32,
) -> Option<bool> {
    try_handle_damage(py, target_id, attacker_id, damage, dflags, means_of_death).unwrap_or_else(
        |e| {
            log_exception(py, e);
            Some(true)
        },
    )
}