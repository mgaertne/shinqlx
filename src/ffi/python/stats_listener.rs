use super::prelude::*;
use crate::quake_live_engine::FindCVar;
use crate::MAIN_ENGINE;

use pyo3::{exceptions::PyEnvironmentError, intern};
use serde_json::Value;
use zmq::{Context, SocketType, DONTWAIT, POLLIN};

fn dispatch_stats_event(py: Python<'_>, stats: String) -> PyResult<()> {
    PyModule::from_code(
        py,
        r#"
import json
import shinqlx

@shinqlx.next_frame
def dispatch_stats_event(stats):
    data = json.loads(stats)
    shinqlx.EVENT_DISPATCHERS["stats"].dispatch(data)
        "#,
        "",
        "",
    )?
    .call_method1(intern!(py, "dispatch_stats_event"), (stats,))?;
    Ok(())
}

fn dispatch_game_start_event(py: Python<'_>, stats: String) -> PyResult<()> {
    PyModule::from_code(
        py,
        r#"
import json
import shinqlx

@shinqlx.next_frame
def dispatch_game_start_event(stats):
    data = json.loads(stats)
    shinqlx.EVENT_DISPATCHERS["game_start"].dispatch(data)
        "#,
        "",
        "",
    )?
    .call_method1(intern!(py, "dispatch_game_start_event"), (stats,))?;
    Ok(())
}

fn dispatch_round_end_event(py: Python<'_>, stats: String) -> PyResult<()> {
    PyModule::from_code(
        py,
        r#"
import json
import shinqlx

@shinqlx.next_frame
def dispatch_round_end_event(stats):
    data = json.loads(stats)
    shinqlx.EVENT_DISPATCHERS["round_end"].dispatch(data)
        "#,
        "",
        "",
    )?
    .call_method1(intern!(py, "dispatch_round_end_event"), (stats,))?;
    Ok(())
}

fn dispatch_game_end_event(py: Python<'_>, stats: String) -> PyResult<()> {
    PyModule::from_code(
        py,
        r#"
import json
import shinqlx

@shinqlx.next_frame
def dispatch_game_end_event(stats):
    data = json.loads(stats)
    shinqlx.EVENT_DISPATCHERS["game_end"].dispatch(data)
        "#,
        "",
        "",
    )?
    .call_method1(intern!(py, "dispatch_game_end_event"), (stats,))?;
    Ok(())
}

fn handle_player_death_event(py: Python<'_>, stats: Value) -> PyResult<()> {
    let opt_victim_steam_id = stats["DATA"]["VICTIM"]["STEAM_ID"]
        .as_str()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|&value| value > 0);
    let Some(victim_name) = stats["DATA"]["VICTIM"]["NAME"].as_str() else {
        return Ok(());
    };

    let (opt_killer_steam_id, opt_killer_name) = if stats["DATA"]["KILLER"].is_null() {
        (None, None)
    } else {
        (
            stats["DATA"]["KILLER"]["STEAM_ID"]
                .as_str()
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|&value| value > 0),
            stats["DATA"]["KILLER"]["NAME"]
                .as_str()
                .map(|value| value.to_string()),
        )
    };

    dispatch_player_death_events(
        py,
        opt_victim_steam_id,
        victim_name.into(),
        opt_killer_steam_id,
        opt_killer_name,
        stats["DATA"].to_string(),
    )?;

    Ok(())
}

fn player_by_steam_id(py: Python<'_>, steam_id: &i64) -> Option<Player> {
    let Ok(players_info) = pyshinqlx_players_info(py) else {
        return None;
    };
    players_info.iter().find_map(|opt_player_info| {
        opt_player_info.as_ref().iter().find_map(|&player_info| {
            if player_info.steam_id != *steam_id {
                None
            } else {
                Some(Player {
                    valid: true,
                    id: player_info.client_id,
                    user_info: player_info.userinfo.clone(),
                    steam_id: player_info.steam_id,
                    name: player_info.name.clone(),
                    player_info: player_info.clone(),
                })
            }
        })
    })
}

fn player_by_name(py: Python<'_>, name: &String) -> Option<Player> {
    let Ok(players_info) = pyshinqlx_players_info(py) else {
        return None;
    };
    players_info.iter().find_map(|opt_player_info| {
        opt_player_info.as_ref().iter().find_map(|&player_info| {
            if player_info.name != *name {
                None
            } else {
                Some(Player {
                    valid: true,
                    id: player_info.client_id,
                    user_info: player_info.userinfo.clone(),
                    steam_id: player_info.steam_id,
                    name: player_info.name.clone(),
                    player_info: player_info.clone(),
                })
            }
        })
    })
}

fn dispatch_player_death_events(
    py: Python<'_>,
    opt_victim_steam_id: Option<i64>,
    victim_name: String,
    opt_killer_steam_id: Option<i64>,
    opt_killer_name: Option<String>,
    stats: String,
) -> PyResult<()> {
    let Some(victim) = (match opt_victim_steam_id {
        Some(victim_steam_id) => player_by_steam_id(py, &victim_steam_id),
        None => player_by_name(py, &victim_name),
    }) else {
        return Ok(());
    };

    let opt_killer = match opt_killer_steam_id {
        Some(killer_steam_id) => player_by_steam_id(py, &killer_steam_id),
        None => opt_killer_name.and_then(|killer_name| player_by_name(py, &killer_name)),
    };

    PyModule::from_code(
        py,
        r#"
import json
import shinqlx

@shinqlx.next_frame
def dispatch_death_event(victim, killer, stats):
    data = json.loads(stats)
    shinqlx.EVENT_DISPATCHERS["death"].dispatch(victim, killer, data)
    if killer:
        shinqlx.EVENT_DISPATCHERS["kill"].dispatch(victim, killer, data)
"#,
        "",
        "",
    )?
    .call_method1(
        intern!(py, "dispatch_death_event"),
        (victim, opt_killer, stats),
    )?;

    Ok(())
}

fn handle_team_switch_event(py: Python<'_>, stats: Value) -> PyResult<()> {
    let opt_steam_id = stats["DATA"]["KILLER"]["STEAM_ID"]
        .as_str()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|&value| value > 0);
    let Some(name) = stats["DATA"]["KILLER"]["NAME"].as_str() else {
        return Ok(());
    };

    let Some(old_team) = stats["DATA"]["KILLER"]["OLD_TEAM"].as_str() else {
        return Ok(());
    };
    let Some(new_team) = stats["DATA"]["KILLER"]["TEAM"].as_str() else {
        return Ok(());
    };

    if old_team.to_lowercase() != new_team.to_lowercase() {
        dispatch_team_switch_event(
            py,
            opt_steam_id,
            name.into(),
            old_team.to_lowercase(),
            new_team.to_lowercase(),
        )?;
    }

    Ok(())
}

fn dispatch_team_switch_event(
    py: Python<'_>,
    opt_steam_id: Option<i64>,
    name: String,
    old_team: String,
    new_team: String,
) -> PyResult<()> {
    let Some(player) = (match opt_steam_id {
        Some(steam_id) => player_by_steam_id(py, &steam_id),
        None => player_by_name(py, &name),
    }) else {
        return Ok(());
    };

    let dispatch_module = PyModule::from_code(
        py,
        r#"
import shinqlx

@shinqlx.next_frame
def dispatch_team_switch_event(player, old_team, new_team):
    shinqlx.EVENT_DISPATCHERS["team_switch"].dispatch(player, old_team, new_team)
        "#,
        "",
        "",
    )?;

    let player_id = player.id;
    let py_result = dispatch_module.call_method1(
        intern!(py, "dispatch_team_switch_event"),
        (player, &old_team, &new_team),
    )?;
    if py_result.extract::<bool>().is_ok_and(|value| !value) {
        let team_change_cmd = format!("put {} {}", player_id, &old_team);
        pyshinqlx_console_command(py, &team_change_cmd)?;
    }

    Ok(())
}

/// Subscribes to the ZMQ stats protocol and calls the stats event dispatcher when
/// we get stats from it.
#[pyclass(module = "_zmq", name = "StatsListener", get_all)]
pub(crate) struct StatsListener {
    #[pyo3(name = "done")]
    done: bool,
    #[pyo3(name = "address")]
    address: String,
    #[pyo3(name = "password")]
    password: String,
}

#[pymethods]
impl StatsListener {
    #[new]
    fn py_new() -> PyResult<Self> {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let zmq_enabled_cvar = main_engine.find_cvar("zmq_stats_enable");
        if !zmq_enabled_cvar.is_some_and(|cvar| cvar.get_integer() != 0) {
            return Ok(Self {
                done: true,
                address: Default::default(),
                password: Default::default(),
            });
        }

        let host = main_engine
            .find_cvar("zmq_stats_ip")
            .and_then(|ip| {
                let host = ip.get_string();
                if host.is_empty() {
                    None
                } else {
                    Some(host)
                }
            })
            .unwrap_or("127.0.0.1".into());
        let port = match main_engine.find_cvar("zmq_stats_port") {
            None => main_engine
                .find_cvar("net_port")
                .map(|cvar| cvar.get_string())
                .unwrap_or_default(),
            Some(cvar) => cvar.get_string(),
        };
        let address = format!("tcp://{host}:{port}");
        let password = main_engine
            .find_cvar("zmq_stats_password")
            .map(|cvar| cvar.get_string())
            .unwrap_or_default();

        Ok(Self {
            done: false,
            address,
            password,
        })
    }

    fn stop(&mut self) {
        self.done = true;
    }

    /// Receives until 'self.done' is set to True.
    fn keep_receiving(slf: &PyCell<Self>, py: Python<'_>) -> PyResult<()> {
        PyModule::from_code(
            py,
            r#"
import threading

def run_zmq_thread(poller):
    threading.Thread(target=poller._poll_zmq).start()
"#,
            "",
            "",
        )?
        .call_method1(intern!(py, "run_zmq_thread"), (slf,))?;

        Ok(())
    }

    #[pyo3(name = "_poll_zmq")]
    fn poll_zmq(&self, py: Python<'_>) -> PyResult<()> {
        let Some(socket) = py.allow_threads(|| {
            let zmq_context = Context::new();

            let socket = zmq_context.socket(SocketType::SUB).ok()?;
            socket.set_plain_username(Some("stats")).ok()?;
            socket.set_plain_password(Some(&self.password)).ok()?;

            socket.set_zap_domain("stats").ok()?;

            socket.connect(&self.address).ok()?;
            socket.set_subscribe("".as_bytes()).ok()?;

            Some(socket)
        }) else {
            return Ok(());
        };
        let mut in_progress = false;

        loop {
            if let Some(stats) = py.allow_threads(|| {
                if socket.poll(POLLIN, 250).unwrap_or(0) == 0 {
                    return None;
                };
                let message = socket.recv_msg(DONTWAIT).ok()?;
                let message_str = message.as_str()?;
                let stats = serde_json::from_str::<Value>(message_str).ok()?;
                Some(stats)
            }) {
                dispatch_stats_event(py, stats.to_string())?;
                match stats["TYPE"].as_str() {
                    Some("MATCH_STARTED") => {
                        in_progress = true;
                        dispatch_game_start_event(py, stats["DATA"].to_string())?;
                    }
                    Some("ROUND_OVER") => {
                        dispatch_round_end_event(py, stats["DATA"].to_string())?;
                    }
                    Some("MATCH_REPORT") => {
                        if in_progress {
                            dispatch_game_end_event(py, stats["DATA"].to_string())?;
                        }
                        in_progress = false;
                    }
                    Some("PLAYER_DEATH") => {
                        handle_player_death_event(py, stats)?;
                    }
                    Some("PLAYER_SWITCHTEAM") => {
                        handle_team_switch_event(py, stats)?;
                    }
                    _ => continue,
                }
            }
        }
    }
}
