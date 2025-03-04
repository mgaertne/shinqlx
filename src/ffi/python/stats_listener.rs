use super::prelude::*;
use super::{EVENT_DISPATCHERS, log_exception};

use crate::MAIN_ENGINE;
use crate::quake_live_engine::FindCVar;

use core::sync::atomic::{AtomicBool, Ordering};
use once_cell::sync::Lazy;

use pyo3::{
    exceptions::{PyEnvironmentError, PyIOError},
    intern,
};

use serde_json::{Value, from_str};
use zmq::{Context, DONTWAIT, POLLIN, Socket, SocketType};

fn to_py_json_data<'py>(py: Python<'py>, json_str: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("json")
        .and_then(|json_module| json_module.call_method1(intern!(py, "loads"), (json_str,)))
}

fn dispatch_thread_safe(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    PyModule::from_code(
        py,
        cr#"
import shinqlx


@shinqlx.next_frame
def thread_safe_dispatch(dispatcher, *args):
    dispatcher.dispatch(*args)
    "#,
        c"",
        c"",
    )
    .and_then(|module| module.getattr(intern!(py, "thread_safe_dispatch")))
}

fn dispatch_stats_event(py: Python<'_>, stats: &str) -> PyResult<()> {
    let json_data = to_py_json_data(py, stats)?;
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "stats"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to stats dispatcher",
            )),
            |stats_dispatcher| {
                dispatch_thread_safe(py).and_then(|thread_safe_dispatcher| {
                    thread_safe_dispatcher.call1((stats_dispatcher, json_data))
                })?;
                Ok(())
            },
        )
}

fn dispatch_game_start_event(py: Python<'_>, stats: &str) -> PyResult<()> {
    let json_data = to_py_json_data(py, stats)?;
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "game_start"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to game_start dispatcher",
            )),
            |game_start_dispatcher| {
                dispatch_thread_safe(py).and_then(|thread_safe_dispatcher| {
                    thread_safe_dispatcher.call1((game_start_dispatcher, json_data))
                })?;
                Ok(())
            },
        )
}

fn dispatch_round_end_event(py: Python<'_>, stats: &str) -> PyResult<()> {
    let json_data = to_py_json_data(py, stats)?;
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "round_end"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to round_end dispatcher",
            )),
            |rount_end_dispatcher| {
                dispatch_thread_safe(py).and_then(|thread_safe_dispatcher| {
                    thread_safe_dispatcher.call1((rount_end_dispatcher, json_data))
                })?;
                Ok(())
            },
        )
}

fn dispatch_game_end_event(py: Python<'_>, stats: &str) -> PyResult<()> {
    let json_data = to_py_json_data(py, stats)?;
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "game_end"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to game_end dispatcher",
            )),
            |game_end_dispatcher| {
                dispatch_thread_safe(py).and_then(|thread_safe_dispatcher| {
                    thread_safe_dispatcher.call1((game_end_dispatcher, json_data))
                })?;
                Ok(())
            },
        )
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
        victim_name,
        opt_killer_steam_id,
        opt_killer_name,
        &stats["DATA"].to_string(),
    )?;

    Ok(())
}

fn player_by_steam_id(py: Python<'_>, steam_id: &i64) -> Option<Player> {
    pyshinqlx_players_info(py).ok().and_then(|players_info| {
        players_info.iter().find_map(|opt_player_info| {
            opt_player_info.as_ref().iter().find_map(|&player_info| {
                if player_info.steam_id != *steam_id {
                    None
                } else {
                    Some(Player {
                        valid: true.into(),
                        id: player_info.client_id,
                        user_info: player_info.userinfo.clone(),
                        steam_id: player_info.steam_id,
                        name: player_info.name.clone().into(),
                        player_info: player_info.clone().into(),
                    })
                }
            })
        })
    })
}

fn player_by_name(py: Python<'_>, name: &str) -> Option<Player> {
    pyshinqlx_players_info(py).ok().and_then(|players_info| {
        players_info.iter().find_map(|opt_player_info| {
            opt_player_info.as_ref().iter().find_map(|&player_info| {
                if player_info.name != *name {
                    None
                } else {
                    Some(Player {
                        valid: true.into(),
                        id: player_info.client_id,
                        user_info: player_info.userinfo.clone(),
                        steam_id: player_info.steam_id,
                        name: player_info.name.clone().into(),
                        player_info: player_info.clone().into(),
                    })
                }
            })
        })
    })
}

fn dispatch_player_death_events(
    py: Python<'_>,
    opt_victim_steam_id: Option<i64>,
    victim_name: &str,
    opt_killer_steam_id: Option<i64>,
    opt_killer_name: Option<String>,
    stats: &str,
) -> PyResult<()> {
    let Some(victim) = (match opt_victim_steam_id {
        Some(victim_steam_id) => player_by_steam_id(py, &victim_steam_id),
        None => player_by_name(py, victim_name),
    }) else {
        return Ok(());
    };

    let opt_killer = match opt_killer_steam_id {
        Some(killer_steam_id) => player_by_steam_id(py, &killer_steam_id),
        None => opt_killer_name.and_then(|killer_name| player_by_name(py, &killer_name)),
    };

    let json_data = to_py_json_data(py, stats)?;
    let thread_safe_dispatcher = dispatch_thread_safe(py)?;
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "death"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to death dispatcher",
            )),
            |death_dispatcher| {
                thread_safe_dispatcher.call1((
                    death_dispatcher,
                    victim.clone(),
                    opt_killer.clone(),
                    json_data.clone(),
                ))?;
                Ok(())
            },
        )?;

    if opt_killer.is_some() {
        EVENT_DISPATCHERS
            .load()
            .as_ref()
            .and_then(|event_dispatchers| {
                event_dispatchers
                    .bind(py)
                    .get_item(intern!(py, "kill"))
                    .ok()
            })
            .map_or(
                Err(PyEnvironmentError::new_err(
                    "could not get access to kill dispatcher",
                )),
                |kill_dispatcher| {
                    thread_safe_dispatcher.call1((
                        kill_dispatcher,
                        victim,
                        opt_killer,
                        json_data,
                    ))?;
                    Ok(())
                },
            )?;
    }

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
            name,
            &old_team.to_lowercase(),
            &new_team.to_lowercase(),
        )?;
    }

    Ok(())
}

fn dispatch_team_switch_event(
    py: Python<'_>,
    opt_steam_id: Option<i64>,
    name: &str,
    old_team: &str,
    new_team: &str,
) -> PyResult<()> {
    let Some(player) = (match opt_steam_id {
        Some(steam_id) => player_by_steam_id(py, &steam_id),
        None => player_by_name(py, name),
    }) else {
        return Ok(());
    };

    let thread_safe_team_switch_dispatcher = PyModule::from_code(
        py,
        cr#"
import shinqlx


@shinqlx.next_frame
def thread_safe_team_switch_dispatch(dispatcher, player, old_team, new_team):
    res = dispatcher.dispatch(player, old_team, new_team)
    if res is False:
        player.put(old_team)
        "#,
        c"",
        c"",
    )?
    .getattr(intern!(py, "thread_safe_team_switch_dispatch"))?;

    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| {
            event_dispatchers
                .bind(py)
                .get_item(intern!(py, "team_switch"))
                .ok()
        })
        .map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to team_switch dispatcher",
            )),
            |team_switch_dispatcher| {
                thread_safe_team_switch_dispatcher.call1((
                    team_switch_dispatcher,
                    player,
                    old_team,
                    new_team,
                ))?;
                Ok(())
            },
        )?;

    Ok(())
}

static IN_PROGRESS: Lazy<AtomicBool> = Lazy::new(AtomicBool::default);

/// Subscribes to the ZMQ stats protocol and calls the stats event dispatcher when
/// we get stats from it.
#[pyclass(module = "_zmq", name = "StatsListener", frozen, eq)]
#[derive(Debug)]
pub(crate) struct StatsListener {
    done: AtomicBool,
    #[pyo3(name = "address", get)]
    pub(crate) address: String,
    #[pyo3(name = "password", get)]
    password: String,
}

impl PartialEq for StatsListener {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
            && self.password == other.password
            && self.done.load(Ordering::SeqCst) == other.done.load(Ordering::SeqCst)
    }
}

#[pymethods]
impl StatsListener {
    #[new]
    pub(crate) fn py_new() -> PyResult<Self> {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            ));
        };

        let zmq_enabled_cvar = main_engine.find_cvar("zmq_stats_enable");
        if zmq_enabled_cvar.is_none_or(|cvar| cvar.get_integer() == 0) {
            return Ok(Self {
                done: true.into(),
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
                    Some(host.to_string())
                }
            })
            .unwrap_or("127.0.0.1".into());
        let port = match main_engine.find_cvar("zmq_stats_port") {
            None => main_engine
                .find_cvar("net_port")
                .map(|cvar| cvar.get_string().to_string())
                .unwrap_or_default(),
            Some(cvar) => cvar.get_string().to_string(),
        };
        let address = format!("tcp://{host}:{port}");
        let password = main_engine
            .find_cvar("zmq_stats_password")
            .map(|cvar| cvar.get_string().to_string())
            .unwrap_or_default();

        Ok(Self {
            done: false.into(),
            address,
            password,
        })
    }

    #[getter]
    fn get_done(slf: &Bound<'_, Self>) -> bool {
        slf.get_done()
    }

    fn stop(slf: &Bound<'_, Self>) {
        slf.stop()
    }

    /// Receives until 'self.done' is set to True.
    pub(crate) fn keep_receiving(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.keep_receiving()
    }

    #[pyo3(name = "_poll_zmq")]
    fn poll_zmq(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf._poll_zmq()
    }
}

pub(crate) trait StatsListenerMethods<'py> {
    fn get_done(&self) -> bool;
    fn stop(&self);
    fn keep_receiving(&self) -> PyResult<()>;
    fn _poll_zmq(&self) -> PyResult<()>;
}

impl<'py> StatsListenerMethods<'py> for Bound<'py, StatsListener> {
    fn get_done(&self) -> bool {
        self.borrow().done.load(Ordering::SeqCst)
    }

    fn stop(&self) {
        self.borrow().done.store(true, Ordering::SeqCst);
    }

    fn keep_receiving(&self) -> PyResult<()> {
        PyModule::from_code(
            self.py(),
            cr#"
import threading

def run_zmq_thread(poller):
    threading.Thread(target=poller._poll_zmq).start()
"#,
            c"",
            c"",
        )?
        .call_method1(intern!(self.py(), "run_zmq_thread"), (self,))?;

        Ok(())
    }

    fn _poll_zmq(&self) -> PyResult<()> {
        let socket = get_zmq_socket(&self.borrow().address, &self.borrow().password).map_err(
            |err: zmq::Error| {
                let error_msg = format!("zmq error: {:?}", err);
                PyIOError::new_err(error_msg)
            },
        )?;

        loop {
            if !self
                .py()
                .allow_threads(|| socket.poll(POLLIN, 250))
                .is_ok_and(|value| value == 1)
            {
                continue;
            }

            if let Ok(zmq_msg) = self.py().allow_threads(|| socket.recv_msg(DONTWAIT)) {
                if let Some(zmq_str) = zmq_msg.as_str() {
                    handle_zmq_msg(self.py(), zmq_str);
                }
            }
        }
    }
}

fn get_zmq_socket(address: &str, password: &str) -> zmq::Result<Socket> {
    let zmq_context = Context::new();

    let socket = zmq_context.socket(SocketType::SUB)?;
    socket.set_plain_username(Some("stats"))?;
    socket.set_plain_password(Some(password))?;

    socket.set_zap_domain("stats")?;

    socket.connect(address)?;
    socket.set_subscribe("".as_bytes())?;

    Ok(socket)
}

#[cfg(test)]
mod stats_listener_tests {
    use super::{StatsListener, StatsListenerMethods};

    use crate::ffi::python::pyshinqlx_setup_fixture::*;

    use crate::prelude::*;

    use crate::ffi::c::prelude::{CVar, CVarBuilder, cvar_t};

    use core::borrow::BorrowMut;

    use pretty_assertions::assert_eq;
    use rstest::*;

    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn constructor_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = StatsListener::py_new();
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn constructor_with_disabled_zmq_cvar() {
        let cvar_string = c"0";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .integer(0)
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                let result = StatsListener::py_new();
                assert_eq!(
                    result.expect("this should not happen"),
                    StatsListener {
                        done: true.into(),
                        address: "".into(),
                        password: "".into()
                    }
                );
            });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn constructor_with_defaulted_cvars() {
        let mut raw_zmq_enable_cvar = CVarBuilder::default()
            .integer(1)
            .build()
            .expect("this should not happen");
        let zmq_ip = c"";
        let mut raw_zmq_ip_cvar = CVarBuilder::default()
            .string(zmq_ip.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let zmq_password = c"";
        let mut raw_zmq_passwd_cvar = CVarBuilder::default()
            .string(zmq_password.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let net_port = c"27960";
        let mut raw_net_port_cvar = CVarBuilder::default()
            .string(net_port.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_enable_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_ip",
                move |_| CVar::try_from(raw_zmq_ip_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(|cmd| cmd == "zmq_stats_port", |_| None, 1..)
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_password",
                move |_| CVar::try_from(raw_zmq_passwd_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "net_port",
                move |_| CVar::try_from(raw_net_port_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                let result = StatsListener::py_new();
                assert_eq!(
                    result.expect("this should not happen"),
                    StatsListener {
                        done: false.into(),
                        address: "tcp://127.0.0.1:27960".into(),
                        password: "".into()
                    }
                );
            });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn constructor_with_configured_cvars() {
        let mut raw_zmq_enable_cvar = CVarBuilder::default()
            .integer(1)
            .build()
            .expect("this should not happen");
        let zmq_ip = c"192.168.0.1";
        let mut raw_zmq_ip_cvar = CVarBuilder::default()
            .string(zmq_ip.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let zmq_port = c"28960";
        let mut raw_zmq_port_cvar = CVarBuilder::default()
            .string(zmq_port.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let zmq_password = c"p4ssw0rd";
        let mut raw_zmq_password_cvar = CVarBuilder::default()
            .string(zmq_password.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_enable",
                move |_| CVar::try_from(raw_zmq_enable_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_ip",
                move |_| CVar::try_from(raw_zmq_ip_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_port",
                move |_| CVar::try_from(raw_zmq_port_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .with_find_cvar(
                |cmd| cmd == "zmq_stats_password",
                move |_| CVar::try_from(raw_zmq_password_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                let result = StatsListener::py_new();
                assert_eq!(
                    result.expect("this should not happen"),
                    StatsListener {
                        done: false.into(),
                        address: "tcp://192.168.0.1:28960".into(),
                        password: "p4ssw0rd".into()
                    }
                );
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn stop_sets_done_field(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let listener = Bound::new(
                py,
                StatsListener {
                    done: false.into(),
                    address: "".into(),
                    password: "".into(),
                },
            )
            .expect("this should not happen");

            listener.stop();

            assert_eq!(listener.get_done(), true);
        });
    }
}

fn try_handle_zmq_msg(py: Python<'_>, zmq_msg: &str) -> PyResult<()> {
    let stats = py.allow_threads(|| {
        from_str::<Value>(zmq_msg).map_err(|err: serde_json::Error| {
            let error_msg = format!("error parsing json data: {:?}", err);
            PyIOError::new_err(error_msg)
        })
    })?;

    dispatch_stats_event(py, &stats.to_string())?;
    match stats["TYPE"].as_str() {
        Some("MATCH_STARTED") => {
            IN_PROGRESS.store(true, Ordering::SeqCst);
            dispatch_game_start_event(py, &stats["DATA"].to_string())
        }
        Some("ROUND_OVER") => dispatch_round_end_event(py, &stats["DATA"].to_string()),
        Some("MATCH_REPORT") => {
            if IN_PROGRESS.load(Ordering::SeqCst) {
                dispatch_game_end_event(py, &stats["DATA"].to_string())?;
            }
            IN_PROGRESS.store(false, Ordering::SeqCst);
            Ok(())
        }
        Some("PLAYER_DEATH") => handle_player_death_event(py, stats),
        Some("PLAYER_SWITCHTEAM") => handle_team_switch_event(py, stats),
        _ => Ok(()),
    }
}

fn handle_zmq_msg(py: Python<'_>, zmq_msg: &str) {
    if let Err(e) = try_handle_zmq_msg(py, zmq_msg) {
        log_exception(py, &e);
    }
}

#[cfg(test)]
mod handle_zmq_msg_tests {
    use super::{IN_PROGRESS, handle_zmq_msg, to_py_json_data, try_handle_zmq_msg};

    use crate::ffi::python::{
        EVENT_DISPATCHERS,
        commands::CommandPriorities,
        events::{
            DeathDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
            GameEndDispatcher, GameStartDispatcher, KillDispatcher, RoundEndDispatcher,
            StatsDispatcher, TeamSwitchDispatcher,
        },
        pyshinqlx_setup_fixture::*,
        pyshinqlx_test_support::run_all_frame_tasks,
        pyshinqlx_test_support::*,
    };

    use crate::prelude::*;

    use crate::ffi::c::prelude::{
        CVar, CVarBuilder, MockClient, MockGameEntity, clientState_t, cvar_t, privileges_t, team_t,
    };

    use core::borrow::BorrowMut;
    use core::sync::atomic::Ordering;

    use mockall::predicate;
    use rstest::*;

    use pyo3::exceptions::{PyAssertionError, PyEnvironmentError, PyIOError};
    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn try_handle_zmq_msg_for_unparseable_json_msg(_pyshinqlx_setup: ()) {
        let zmq_msg = r#"{"INVALID":"#;

        Python::with_gil(|py| {
            let result = try_handle_zmq_msg(py, zmq_msg);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyIOError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn handle_zmq_msg_for_unparseable_json_msg(_pyshinqlx_setup: ()) {
        let zmq_msg = r#"{"INVALID":"#;

        Python::with_gil(|py| {
            handle_zmq_msg(py, zmq_msg);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_stats_msg_forwards_to_stats_dispatcher(_pyshinqlx_setup: ()) {
        let stats_msg = r#"{"DATA": {}, "TYPE": "STATS"}"#;

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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, stats_msg);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, stats_msg).expect("this should not happen");
                    let asdf =
                        capturing_hook.call_method1("assert_called_with", (expected_json_data,));
                    assert!(asdf.as_ref().is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_stats_msg_with_no_stats_dispatcher(_pyshinqlx_setup: ()) {
        let stats_msg = r#"{"DATA": {}, "TYPE": "STATS"}"#;

        Python::with_gil(|py| {
            let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                .expect("could not create event dispatcher manager in python");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_zmq_msg(py, stats_msg);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_game_start_msg_forwards_to_next_frame_runner(_pyshinqlx_setup: ()) {
        let game_start_data = r#"{"DATA": {"CAPTURE_LIMIT": 8, "FACTORY": "ca", "FACTORY_TITLE": "Clan Arena", "FRAG_LIMIT": 50, "GAME_TYPE": "CA", "INFECTED": 0, "INSTAGIB": 0, "MAP": "thunderstruck", "MATCH_GUID": "asdf", "MERCY_LIMIT": 0, "PLAYERS": [{"NAME": "player1", "STEAM_ID": "1234", "TEAM": 1}, {"NAME": "player2", "STEAM_ID": "5678", "TEAM": 2}], "QUADHOG": 0, "ROUND_LIMIT": 8, "SCORE_LIMIT": 150, "SERVER_TITLE": "shinqlx test server", "TIME_LIMIT": 0, "TRAINING": 0}, "TYPE": "MATCH_STARTED"}"#;

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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<GameStartDispatcher>())
                        .expect("could not add game_start dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("game_start")
                        .and_then(|game_start_dispatcher| {
                            game_start_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to game_start dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, game_start_data);
                    assert!(result.is_ok());
                    assert!(IN_PROGRESS.load(Ordering::SeqCst));

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, game_start_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (expected_json_data
                                    .get_item("DATA")
                                    .expect("this should not happen"),)
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_game_start_msg_with_no_game_start_dispatcher(_pyshinqlx_setup: ()) {
        let game_start_data = r#"{"DATA": {"CAPTURE_LIMIT": 8, "FACTORY": "ca", "FACTORY_TITLE": "Clan Arena", "FRAG_LIMIT": 50, "GAME_TYPE": "CA", "INFECTED": 0, "INSTAGIB": 0, "MAP": "thunderstruck", "MATCH_GUID": "asdf", "MERCY_LIMIT": 0, "PLAYERS": [{"NAME": "player1", "STEAM_ID": "1234", "TEAM": 1}, {"NAME": "player2", "STEAM_ID": "5678", "TEAM": 2}], "QUADHOG": 0, "ROUND_LIMIT": 8, "SCORE_LIMIT": 150, "SERVER_TITLE": "shinqlx test server", "TIME_LIMIT": 0, "TRAINING": 0}, "TYPE": "MATCH_STARTED"}"#;

        Python::with_gil(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<StatsDispatcher>())
                .expect("could not add stats dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_zmq_msg(py, game_start_data);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_round_end_msg_forwards_to_next_frame_runner(_pyshinqlx_setup: ()) {
        let round_end_data = r#"{"DATA": {"MATCH_GUID": "asdf", "ROUND": 10, "TEAM_WON": "RED", "TIME": 539, "WARMUP": false}, "TYPE": "ROUND_OVER"}"#;

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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<RoundEndDispatcher>())
                        .expect("could not add round_end dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("round_end")
                        .and_then(|round_end_dispatcher| {
                            round_end_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to round_end dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, round_end_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, round_end_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (expected_json_data
                                    .get_item("DATA")
                                    .expect("this should not happen"),)
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_round_end_msg_with_no_round_end_dispatcher(_pyshinqlx_setup: ()) {
        let round_end_data = r#"{"DATA": {"MATCH_GUID": "asdf", "ROUND": 10, "TEAM_WON": "RED", "TIME": 539, "WARMUP": false}, "TYPE": "ROUND_OVER"}"#;

        Python::with_gil(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<StatsDispatcher>())
                .expect("could not add stats dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            let result = try_handle_zmq_msg(py, round_end_data);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_game_end_msg_forwards_to_next_frame_runner(_pyshinqlx_setup: ()) {
        let game_end_data = r#"{"DATA": {"ABORTED": false, "CAPTURE_LIMIT": 8, "EXIT_MSG": "Roundlimit hit.", "FACTORY": "ca", "FACTORY_TITLE": "Clan Arena", "FIRST_SCORER": "player1", "FRAG_LIMIT": 50, "GAME_LENGTH": 590, "GAME_TYPE": "CA", "INFECTED": 0, "INSTAGIB": 0, "LAST_LEAD_CHANGE_TIME": 41300, "LAST_SCORER": "skepp", "LAST_TEAMSCORER": "none", "MAP": "x0r3", "MATCH_GUID": "asdf", "MERCY_LIMIT": 0, "QUADHOG": 0, "RESTARTED": 0, "ROUND_LIMIT": 8, "SCORE_LIMIT": 150, "SERVER_TITLE": "shinqlx test server", "TIME_LIMIT": 0, "TRAINING": 0, "TSCORE0": 3, "TSCORE1": 8}, "TYPE": "MATCH_REPORT"}"#;

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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<GameEndDispatcher>())
                        .expect("could not add game_end dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("game_end")
                        .and_then(|game_end_dispatcher| {
                            game_end_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to game_end dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    IN_PROGRESS.store(true, Ordering::SeqCst);

                    let result = try_handle_zmq_msg(py, game_end_data);
                    assert!(result.is_ok());
                    assert!(!IN_PROGRESS.load(Ordering::SeqCst));

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, game_end_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (expected_json_data
                                    .get_item("DATA")
                                    .expect("this should not happen"),)
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_game_end_msg_when_game_not_in_progress(_pyshinqlx_setup: ()) {
        let game_end_data = r#"{"DATA": {"ABORTED": false, "CAPTURE_LIMIT": 8, "EXIT_MSG": "Roundlimit hit.", "FACTORY": "ca", "FACTORY_TITLE": "Clan Arena", "FIRST_SCORER": "player1", "FRAG_LIMIT": 50, "GAME_LENGTH": 590, "GAME_TYPE": "CA", "INFECTED": 0, "INSTAGIB": 0, "LAST_LEAD_CHANGE_TIME": 41300, "LAST_SCORER": "skepp", "LAST_TEAMSCORER": "none", "MAP": "x0r3", "MATCH_GUID": "asdf", "MERCY_LIMIT": 0, "QUADHOG": 0, "RESTARTED": 0, "ROUND_LIMIT": 8, "SCORE_LIMIT": 150, "SERVER_TITLE": "shinqlx test server", "TIME_LIMIT": 0, "TRAINING": 0, "TSCORE0": 3, "TSCORE1": 8}, "TYPE": "MATCH_REPORT"}"#;

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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<GameEndDispatcher>())
                        .expect("could not add game_end dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("game_end")
                        .and_then(|game_end_dispatcher| {
                            game_end_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to game_end dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    IN_PROGRESS.store(false, Ordering::SeqCst);

                    let result = try_handle_zmq_msg(py, game_end_data);
                    assert!(result.is_ok());
                    assert!(!IN_PROGRESS.load(Ordering::SeqCst));

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, game_end_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (expected_json_data
                                    .get_item("DATA")
                                    .expect("this should not happen"),)
                            )
                            .is_err_and(|err| err.is_instance_of::<PyAssertionError>(py))
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_game_end_msg_with_no_game_end_dispatcher(_pyshinqlx_setup: ()) {
        let game_end_data = r#"{"DATA": {"ABORTED": false, "CAPTURE_LIMIT": 8, "EXIT_MSG": "Roundlimit hit.", "FACTORY": "ca", "FACTORY_TITLE": "Clan Arena", "FIRST_SCORER": "player1", "FRAG_LIMIT": 50, "GAME_LENGTH": 590, "GAME_TYPE": "CA", "INFECTED": 0, "INSTAGIB": 0, "LAST_LEAD_CHANGE_TIME": 41300, "LAST_SCORER": "skepp", "LAST_TEAMSCORER": "none", "MAP": "x0r3", "MATCH_GUID": "asdf", "MERCY_LIMIT": 0, "QUADHOG": 0, "RESTARTED": 0, "ROUND_LIMIT": 8, "SCORE_LIMIT": 150, "SERVER_TITLE": "shinqlx test server", "TIME_LIMIT": 0, "TRAINING": 0, "TSCORE0": 3, "TSCORE1": 8}, "TYPE": "MATCH_REPORT"}"#;

        Python::with_gil(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<StatsDispatcher>())
                .expect("could not add stats dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            IN_PROGRESS.store(true, Ordering::SeqCst);

            let result = try_handle_zmq_msg(py, game_end_data);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_death_msg_with_malformed_victim(_pyshinqlx_setup: ()) {
        let player_death_data =
            r#"{"DATA": {"KILLER": null, "MOD": "HURT", "VICTIM": {}}, "TYPE": "PLAYER_DEATH"}"#;
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_death_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, player_death_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_death_msg_from_trigger_hurt_entity_with_steam_id(_pyshinqlx_setup: ()) {
        let player_death_data = r#"{"DATA": {"KILLER": null, "MOD": "HURT", "VICTIM": {"NAME": "player1", "STEAM_ID": "1234"}}, "TYPE": "PLAYER_DEATH"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
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

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<DeathDispatcher>())
                        .expect("could not add death dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("death")
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to death dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_death_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, player_death_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (
                                    "_",
                                    py.None(),
                                    expected_json_data
                                        .get_item("DATA")
                                        .expect("this should not happen"),
                                )
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_death_msg_from_trigger_hurt_entity_with_name_only(_pyshinqlx_setup: ()) {
        let player_death_data = r#"{"DATA": {"KILLER": null, "MOD": "HURT", "VICTIM": {"NAME": "player1", "STEAM_ID": "-1"}}, "TYPE": "PLAYER_DEATH"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
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

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<DeathDispatcher>())
                        .expect("could not add death dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("death")
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to death dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_death_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, player_death_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (
                                    "_",
                                    py.None(),
                                    expected_json_data
                                        .get_item("DATA")
                                        .expect("this should not happen"),
                                )
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_death_msg_from_trigger_hurt_entity_when_player_cannot_be_found(
        _pyshinqlx_setup: (),
    ) {
        let player_death_data = r#"{"DATA": {"KILLER": null, "MOD": "HURT", "VICTIM": {"NAME": "player1", "STEAM_ID": "1234"}}, "TYPE": "PLAYER_DEATH"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::always())
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::always())
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

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_death_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, player_death_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_death_msg_from_trigger_hurt_entity_with_no_dispatcher(
        _pyshinqlx_setup: (),
    ) {
        let player_death_data = r#"{"DATA": {"KILLER": null, "MOD": "HURT", "VICTIM": {"NAME": "player1", "STEAM_ID": "1234"}}, "TYPE": "PLAYER_DEATH"}"#;

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<StatsDispatcher>())
                    .expect("could not add stats dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let result = try_handle_zmq_msg(py, player_death_data);
                run_all_frame_tasks(py).expect("this should not happen");

                assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_death_msg_from_other_player_with_steam_id(_pyshinqlx_setup: ()) {
        let player_death_data = r#"{"DATA": {"KILLER": {"NAME": "player2", "STEAM_ID": "5678"}, "MOD": "HURT", "VICTIM": {"NAME": "player1", "STEAM_ID": "1234"}}, "TYPE": "PLAYER_DEATH"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::eq(4))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\player2".into());
                mock_client.expect_get_steam_id().returning(|| 5678);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .withf(|client_id| ![2, 4].contains(client_id))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(4))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player2".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .withf(|client_id| ![2, 4].contains(client_id))
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

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<DeathDispatcher>())
                        .expect("could not add death dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<KillDispatcher>())
                        .expect("could not add kill dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("death")
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to death dispatcher");
                    event_dispatcher
                        .__getitem__("kill")
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to kill dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_death_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, player_death_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (
                                    "_",
                                    "_",
                                    expected_json_data
                                        .get_item("DATA")
                                        .expect("this should not happen"),
                                )
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (
                                    "_",
                                    "_",
                                    expected_json_data
                                        .get_item("DATA")
                                        .expect("this should not happen"),
                                )
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_death_msg_from_other_player_with_name_only(_pyshinqlx_setup: ()) {
        let player_death_data = r#"{"DATA": {"KILLER": {"NAME": "player2"}, "MOD": "HURT", "VICTIM": {"NAME": "player1", "STEAM_ID": "1234"}}, "TYPE": "PLAYER_DEATH"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::eq(4))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\player2".into());
                mock_client.expect_get_steam_id().returning(|| 5678);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .withf(|client_id| ![2, 4].contains(client_id))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(4))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player2".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .withf(|client_id| ![2, 4].contains(client_id))
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

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<DeathDispatcher>())
                        .expect("could not add death dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<KillDispatcher>())
                        .expect("could not add kill dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("death")
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to death dispatcher");
                    event_dispatcher
                        .__getitem__("kill")
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to kill dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_death_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, player_death_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (
                                    "_",
                                    "_",
                                    expected_json_data
                                        .get_item("DATA")
                                        .expect("this should not happen"),
                                )
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                "assert_called_with",
                                (
                                    "_",
                                    "_",
                                    expected_json_data
                                        .get_item("DATA")
                                        .expect("this should not happen"),
                                )
                            )
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_player_death_msg_from_other_player_with_no_dispatcher(_pyshinqlx_setup: ()) {
        let player_death_data = r#"{"DATA": {"KILLER": {"NAME": "player2", "STEAM_ID": "5678"}, "MOD": "HURT", "VICTIM": {"NAME": "player1", "STEAM_ID": "1234"}}, "TYPE": "PLAYER_DEATH"}"#;

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::eq(4))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\player2".into());
                mock_client.expect_get_steam_id().returning(|| 5678);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .withf(|client_id| ![2, 4].contains(client_id))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_RED);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(4))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player2".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .withf(|client_id| ![2, 4].contains(client_id))
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

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<StatsDispatcher>())
                    .expect("could not add stats dispatcher");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<DeathDispatcher>())
                    .expect("could not add death dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let result = try_handle_zmq_msg(py, player_death_data);
                run_all_frame_tasks(py).expect("this should not happen");

                assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_team_switch_msg_matching_with_steam_id(_pyshinqlx_setup: ()) {
        let player_teamswitch_data = r#"{"DATA": {"KILLER": {"NAME": "player1", "OLD_TEAM": "SPECTATOR", "STEAM_ID": "1234", "TEAM": "BLUE"}}, "TYPE": "PLAYER_SWITCHTEAM"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_SPECTATOR);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchDispatcher>())
                        .expect("could not add team_switch dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("team_switch")
                        .and_then(|team_switch_dispatcher| {
                            team_switch_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to team_switch dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_teamswitch_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data = to_py_json_data(py, player_teamswitch_data)
                        .expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", ("_", "spectator", "blue"))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_team_switch_msg_matching_with_name_only(_pyshinqlx_setup: ()) {
        let player_teamswitch_data = r#"{"DATA": {"KILLER": {"NAME": "player1", "OLD_TEAM": "SPECTATOR", "STEAM_ID": "-1", "TEAM": "BLUE"}}, "TYPE": "PLAYER_SWITCHTEAM"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_SPECTATOR);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchDispatcher>())
                        .expect("could not add team_switch dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .__getitem__("team_switch")
                        .and_then(|team_switch_dispatcher| {
                            team_switch_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to team_switch dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_teamswitch_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data = to_py_json_data(py, player_teamswitch_data)
                        .expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", ("_", "spectator", "blue"))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_team_switch_msg_matching_when_old_and_new_team_are_the_same(
        _pyshinqlx_setup: (),
    ) {
        let player_teamswitch_data = r#"{"DATA": {"KILLER": {"NAME": "player1", "OLD_TEAM": "SPECTATOR", "STEAM_ID": "1234", "TEAM": "SPECTATOR"}}, "TYPE": "PLAYER_SWITCHTEAM"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_SPECTATOR);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_teamswitch_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data = to_py_json_data(py, player_teamswitch_data)
                        .expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_team_switch_msg_matching_with_no_old_team(_pyshinqlx_setup: ()) {
        let player_teamswitch_data = r#"{"DATA": {"KILLER": {"NAME": "player1", "STEAM_ID": "1234", "TEAM": "BLUE"}}, "TYPE": "PLAYER_SWITCHTEAM"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_SPECTATOR);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_teamswitch_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data = to_py_json_data(py, player_teamswitch_data)
                        .expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_team_switch_msg_matching_with_no_new_team(_pyshinqlx_setup: ()) {
        let player_teamswitch_data = r#"{"DATA": {"KILLER": {"NAME": "player1", "OLD_TEAM": "SPECTATOR", "STEAM_ID": "1234"}}, "TYPE": "PLAYER_SWITCHTEAM"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_SPECTATOR);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_teamswitch_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data = to_py_json_data(py, player_teamswitch_data)
                        .expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_team_switch_msg_matching_with_no_name(_pyshinqlx_setup: ()) {
        let player_teamswitch_data = r#"{"DATA": {"KILLER": {"OLD_TEAM": "SPECTATOR", "STEAM_ID": "-1", "TEAM": "BLUE"}}, "TYPE": "PLAYER_SWITCHTEAM"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_SPECTATOR);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_teamswitch_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data = to_py_json_data(py, player_teamswitch_data)
                        .expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_team_switch_msg_when_player_cannot_be_found(_pyshinqlx_setup: ()) {
        let player_teamswitch_data = r#"{"DATA": {"KILLER": {"NAME": "player1", "OLD_TEAM": "SPECTATOR", "STEAM_ID": "1234", "TEAM": "BLUE"}}, "TYPE": "PLAYER_SWITCHTEAM"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let client_try_from_ctx = MockClient::from_context();
        client_try_from_ctx
            .expect()
            .with(predicate::always())
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::always())
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_SPECTATOR);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default()
            .with_max_clients(16)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .__getitem__("stats")
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr("hook")
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_teamswitch_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data = to_py_json_data(py, player_teamswitch_data)
                        .expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1("assert_called_with", (expected_json_data.clone(),))
                            .is_ok()
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_team_switch_msg_matching_with_no_dispatcher(_pyshinqlx_setup: ()) {
        let player_teamswitch_data = r#"{"DATA": {"KILLER": {"NAME": "player1", "OLD_TEAM": "SPECTATOR", "STEAM_ID": "1234", "TEAM": "BLUE"}}, "TYPE": "PLAYER_SWITCHTEAM"}"#;

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_SPECTATOR);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                event_dispatcher
                    .add_dispatcher(&py.get_type::<StatsDispatcher>())
                    .expect("could not add stats dispatcher");
                EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                let result = try_handle_zmq_msg(py, player_teamswitch_data);
                run_all_frame_tasks(py).expect("this should not happen");

                assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_team_switch_msg_when_dispatcher_returns_false(_pyshinqlx_setup: ()) {
        let player_teamswitch_data = r#"{"DATA": {"KILLER": {"NAME": "player1", "OLD_TEAM": "SPECTATOR", "STEAM_ID": "1234", "TEAM": "BLUE"}}, "TYPE": "PLAYER_SWITCHTEAM"}"#;
        let cvar_string = c"1";
        let mut raw_cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

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
                    .returning(|| r"\name\player1".into());
                mock_client.expect_get_steam_id().returning(|| 1234);
                mock_client
            });
        client_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_client = MockClient::new();
                mock_client
                    .expect_get_state()
                    .returning(|| clientState_t::CS_ACTIVE);
                mock_client
                    .expect_get_user_info()
                    .returning(|| r"\name\Mocked Player".into());
                mock_client.expect_get_steam_id().returning(|| 1235);
                mock_client
            });
        let game_entity_try_from_ctx = MockGameEntity::from_context();
        game_entity_try_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "player1".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_BLUE);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });
        game_entity_try_from_ctx
            .expect()
            .with(predicate::ne(2))
            .returning(|_client_id| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_player_name()
                    .returning(|| "Mocked Player".to_string());
                mock_game_entity
                    .expect_get_team()
                    .returning(|| team_t::TEAM_SPECTATOR);
                mock_game_entity
                    .expect_get_privileges()
                    .returning(|| privileges_t::PRIV_NONE);
                mock_game_entity
            });

        MockEngineBuilder::default()
            .with_max_clients(16)
            .with_execute_console_command("put 2 spectator", 1)
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
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchDispatcher>())
                        .expect("could not add team_switch dispatcher");
                    event_dispatcher
                        .__getitem__("team_switch")
                        .and_then(|team_switch_dispatcher| {
                            team_switch_dispatcher.call_method1(
                                "add_hook",
                                (
                                    "asdf",
                                    returning_false_hook(py),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to team_switch dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, player_teamswitch_data);
                    assert!(result.is_ok());

                    run_all_frame_tasks(py).expect("this should not happen");
                });
            });
    }
}
