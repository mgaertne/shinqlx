use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

use pyo3::{
    exceptions::PyIOError,
    intern,
    types::{PyBool, PyCFunction, PyDict, PyList, PyString, PyTuple},
};
use serde_json::{Value, from_str};
use tap::TapFallible;

use super::{
    EVENT_DISPATCHERS, defer_next_frame, event_dispatcher, events_enum::EventName, is_false,
    log_exception, prelude::*, string_enums::StatsType,
};
fn to_py_json_data<'py>(py: Python<'py>, json_str: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import(intern!(py, "json"))
        .and_then(|json_module| json_module.call_method1(intern!(py, "loads"), (json_str,)))
}

/// The Python objects of parsed JSON data, as `json.loads` makes them.
fn json_to_py<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match value {
        Value::Null => py.None().into_bound(py),
        Value::Bool(flag) => PyBool::new(py, *flag).to_owned().into_any(),
        Value::Number(number) => match (number.as_i64(), number.as_u64()) {
            (Some(int), _) => int.into_pyobject(py)?.into_any(),
            (None, Some(int)) => int.into_pyobject(py)?.into_any(),
            _ => number
                .as_f64()
                .unwrap_or_default()
                .into_pyobject(py)?
                .into_any(),
        },
        Value::String(text) => PyString::new(py, text).into_any(),
        Value::Array(items) => PyList::new(
            py,
            items
                .iter()
                .map(|item| json_to_py(py, item))
                .collect::<PyResult<Vec<_>>>()?,
        )?
        .into_any(),
        Value::Object(members) => {
            let dict = PyDict::new(py);
            for (key, member) in members {
                dict.set_item(key, json_to_py(py, member)?)?;
            }
            dict.into_any()
        }
    })
}

#[cfg(test)]
mod json_to_py_tests {
    use pyo3::{prelude::*, types::PyDict};
    use rstest::*;
    use serde_json::{Value, from_str};

    use super::{json_to_py, to_py_json_data};
    use crate::{prelude::*, python::pyshinqlx_setup_fixture::*};

    fn dumped(py: Python<'_>, object: &Bound<'_, PyAny>) -> String {
        let options = PyDict::new(py);
        options
            .set_item("sort_keys", true)
            .expect("could not set the option");
        py.import("json")
            .and_then(|json| json.call_method("dumps", (object,), Some(&options)))
            .and_then(|text| text.extract::<String>())
            .expect("could not dump the object")
    }

    #[rstest]
    #[case::null("null")]
    #[case::true_flag("true")]
    #[case::false_flag("false")]
    #[case::zero("0")]
    #[case::positive_integer("42")]
    #[case::negative_integer("-42")]
    #[case::largest_i64("9223372036854775807")]
    #[case::smallest_i64("-9223372036854775808")]
    #[case::largest_u64("18446744073709551615")]
    #[case::float("0.1")]
    #[case::negative_float("-2.5")]
    #[case::exponent_is_a_float("1e3")]
    #[case::integral_float_stays_a_float("2.0")]
    #[case::empty_string("\"\"")]
    #[case::text("\"gg wp\"")]
    #[case::escapes("\"tab\\t quote\\\" slash\\\\ line\\n\"")]
    #[case::unicode("\"\u{fc}ber \\u00e9 \u{1f600}\"")]
    #[case::empty_array("[]")]
    #[case::mixed_array("[1, 1.0, true, null, \"1\", [], {}]")]
    #[case::empty_object("{}")]
    #[case::object("{\"NAME\": \"Ada\", \"KILLS\": 3, \"ACCURACY\": 0.5, \"ALIVE\": true}")]
    #[case::nested(
        "{\"DATA\": {\"KILLER\": null, \"VICTIM\": {\"STEAM_ID\": \"1\", \"POS\": [1, -2, 3.5]}}}"
    )]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn json_data_converts_like_json_loads(_pyshinqlx_setup: (), #[case] text: &str) {
        Python::attach(|py| {
            let value = from_str::<Value>(text).expect("the test data was no json");

            let converted = json_to_py(py, &value).expect("could not convert the json data");

            let loaded = to_py_json_data(py, text).expect("json.loads failed");
            assert_eq!(dumped(py, &converted), dumped(py, &loaded));
        });
    }

    #[rstest]
    #[case::boolean("true", "bool")]
    #[case::integer("7", "int")]
    #[case::largest_u64("18446744073709551615", "int")]
    #[case::float("7.0", "float")]
    #[case::text("\"7\"", "str")]
    #[case::null("null", "NoneType")]
    #[case::array("[7]", "list")]
    #[case::object("{\"a\": 7}", "dict")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn json_data_converts_to_the_python_type(
        _pyshinqlx_setup: (),
        #[case] text: &str,
        #[case] type_name: &str,
    ) {
        Python::attach(|py| {
            let value = from_str::<Value>(text).expect("the test data was no json");

            let converted = json_to_py(py, &value).expect("could not convert the json data");

            let name = converted
                .get_type()
                .name()
                .and_then(|name| name.extract::<String>())
                .expect("could not get the type name");
            assert_eq!(name, type_name);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_big_integer_keeps_its_value(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let value =
                from_str::<Value>("18446744073709551615").expect("the test data was no json");

            let converted = json_to_py(py, &value).expect("could not convert the json data");

            assert_eq!(
                converted
                    .extract::<u64>()
                    .expect("the result was not an integer"),
                u64::MAX
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn an_object_keeps_its_members_as_a_dict(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let value =
                from_str::<Value>("{\"a\": 1, \"b\": [2]}").expect("the test data was no json");

            let converted = json_to_py(py, &value).expect("could not convert the json data");

            let dict = converted
                .cast::<PyDict>()
                .expect("the result was not a dict");
            assert_eq!(dict.len(), 2);
            assert_eq!(
                dict.get_item("a")
                    .expect("could not read the member")
                    .expect("the member was missing")
                    .extract::<i64>()
                    .expect("the member was not an integer"),
                1
            );
        });
    }
}

/// Dispatches the event in the next frame, because the stats are received in another thread.
fn dispatch_next_frame<'py, A>(dispatcher: &Bound<'py, PyAny>, args: A) -> PyResult<()>
where
    A: IntoPyObject<'py, Output = Bound<'py, PyTuple>>,
    PyErr: From<A::Error>,
{
    defer_next_frame(
        &dispatcher.getattr(intern!(dispatcher.py(), "dispatch"))?,
        args,
    )
}

/// Whether the dispatcher for the event exists and nobody is listening to it.
fn dispatcher_without_hooks(py: Python<'_>, event: EventName) -> bool {
    EVENT_DISPATCHERS
        .load()
        .as_ref()
        .and_then(|event_dispatchers| event_dispatchers.bind(py).get_item(event.as_str()).ok())
        .and_then(|dispatcher| {
            dispatcher
                .cast::<EventDispatcher>()
                .ok()
                .map(|dispatcher| !dispatcher.has_hooks())
        })
        .unwrap_or(false)
}

fn dispatch_stats_event(py: Python<'_>, stats: &str) -> PyResult<()> {
    event_dispatcher(
        py,
        intern!(py, EventName::Stats.as_str()),
        "could not get access to stats dispatcher",
    )
    .and_then(|stats_dispatcher| {
        if !stats_dispatcher.cast::<EventDispatcher>()?.has_hooks() {
            return Ok(());
        }

        dispatch_next_frame(&stats_dispatcher, (to_py_json_data(py, stats)?,))
    })
}

fn dispatch_json_event(
    py: Python<'_>,
    event: &Bound<'_, PyString>,
    error_msg: &'static str,
    stats: &Value,
) -> PyResult<()> {
    let json_data = json_to_py(py, stats)?;
    event_dispatcher(py, event, error_msg)
        .and_then(|dispatcher| dispatch_next_frame(&dispatcher, (json_data,)))
}

fn dispatch_game_start_event(py: Python<'_>, stats: &Value) -> PyResult<()> {
    dispatch_json_event(
        py,
        intern!(py, EventName::GameStart.as_str()),
        "could not get access to game_start dispatcher",
        stats,
    )
}

fn dispatch_round_end_event(py: Python<'_>, stats: &Value) -> PyResult<()> {
    dispatch_json_event(
        py,
        intern!(py, EventName::RoundEnd.as_str()),
        "could not get access to round_end dispatcher",
        stats,
    )
}

fn dispatch_game_end_event(py: Python<'_>, stats: &Value) -> PyResult<()> {
    dispatch_json_event(
        py,
        intern!(py, EventName::GameEnd.as_str()),
        "could not get access to game_end dispatcher",
        stats,
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
        &stats["DATA"],
    )?;

    Ok(())
}

fn player_matching<F>(py: Python<'_>, predicate: F) -> Option<Player>
where
    F: Fn(&PlayerInfo) -> bool,
{
    pyshinqlx_players_info(py).ok().and_then(|players_info| {
        players_info
            .into_iter()
            .flatten()
            .find(|player_info| predicate(player_info))
            .map(Player::from)
    })
}

fn player_by_steam_id(py: Python<'_>, steam_id: &i64) -> Option<Player> {
    player_matching(py, |player_info| player_info.steam_id == *steam_id)
}

fn player_by_name(py: Python<'_>, name: &str) -> Option<Player> {
    player_matching(py, |player_info| player_info.name == *name)
}

fn dispatch_player_death_events(
    py: Python<'_>,
    opt_victim_steam_id: Option<i64>,
    victim_name: &str,
    opt_killer_steam_id: Option<i64>,
    opt_killer_name: Option<String>,
    stats: &Value,
) -> PyResult<()> {
    if dispatcher_without_hooks(py, EventName::Death)
        && dispatcher_without_hooks(py, EventName::Kill)
    {
        return Ok(());
    }

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

    let json_data = json_to_py(py, stats)?;
    event_dispatcher(
        py,
        intern!(py, EventName::Death.as_str()),
        "could not get access to death dispatcher",
    )
    .and_then(|death_dispatcher| {
        dispatch_next_frame(
            &death_dispatcher,
            (
                victim.to_owned(),
                opt_killer.to_owned(),
                json_data.to_owned(),
            ),
        )
    })?;

    if opt_killer.is_some() {
        event_dispatcher(
            py,
            intern!(py, EventName::Kill.as_str()),
            "could not get access to kill dispatcher",
        )
        .and_then(|kill_dispatcher| {
            dispatch_next_frame(&kill_dispatcher, (victim, opt_killer, json_data))
        })?;
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

    let team_switch_dispatcher = event_dispatcher(
        py,
        intern!(py, EventName::TeamSwitch.as_str()),
        "could not get access to team_switch dispatcher",
    )?
    .unbind();
    let player = Bound::new(py, player)?.unbind();
    let (old_team, new_team) = (old_team.to_owned(), new_team.to_owned());
    let dispatch_and_revert = PyCFunction::new_closure(
        py,
        Some(c"team_switch_dispatch"),
        None,
        move |arguments: &Bound<'_, PyTuple>, _keywords: Option<&Bound<'_, PyDict>>| {
            let py = arguments.py();
            let player = player.bind(py);
            let result = team_switch_dispatcher
                .bind(py)
                .call_method1(intern!(py, "dispatch"), (player, &old_team, &new_team))?;
            if is_false(&result) {
                player.put(&old_team)?;
            }
            Ok::<(), PyErr>(())
        },
    )?;
    defer_next_frame(dispatch_and_revert.as_any(), ())
}

static IN_PROGRESS: LazyLock<AtomicBool> = LazyLock::new(AtomicBool::default);

/// The stats listener, which reads the stats from the ZMQ socket of the server instead of the hook of the engine.
#[cfg(feature = "zmq-stats")]
pub(crate) mod socket {
    use core::{
        hint::cold_path,
        pin::pin,
        sync::atomic::{AtomicBool, Ordering},
        task::Poll,
    };

    use arzmq::{
        prelude::{
            ContextBuilder, Receiver, SocketBuilder, SubscribeBuilder, SubscribeSocket, ZmqResult,
        },
        security::SecurityMechanism,
    };
    use async_io::block_on;
    use futures::{
        future::{Either, poll_fn, select},
        task::AtomicWaker,
    };
    use pyo3::exceptions::{PyEnvironmentError, PyIOError};

    use super::{handle_zmq_msg, log_exception};
    use crate::{MAIN_ENGINE, engine::FindCVar, python::prelude::*};

    /// Subscribes to the ZMQ stats protocol and calls the stats event dispatcher when
    /// we get stats from it.
    #[pyclass(module = "_zmq", name = "StatsListener", frozen, eq)]
    #[derive(Debug)]
    pub(crate) struct StatsListener {
        done: AtomicBool,
        stopped: AtomicWaker,
        #[pyo3(name = "address", get)]
        pub(crate) address: String,
        #[pyo3(name = "password", get)]
        password: String,
    }

    impl StatsListener {
        fn new(done: bool, address: String, password: String) -> Self {
            Self {
                done: done.into(),
                stopped: AtomicWaker::new(),
                address,
                password,
            }
        }
    }

    impl PartialEq for StatsListener {
        fn eq(&self, other: &Self) -> bool {
            self.address == other.address
                && self.password == other.password
                && self.done.load(Ordering::Acquire) == other.done.load(Ordering::Acquire)
        }
    }

    #[pymethods]
    impl StatsListener {
        #[new]
        pub(crate) fn py_new() -> PyResult<Self> {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                cold_path();
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            let zmq_enabled_cvar = main_engine.find_cvar("zmq_stats_enable");
            if zmq_enabled_cvar.is_none_or(|cvar| cvar.get_integer() == 0) {
                return Ok(Self::new(true, String::new(), String::new()));
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
                .unwrap_or_else(|| "127.0.0.1".into());
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

            Ok(Self::new(false, address, password))
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
    }

    pub(crate) trait StatsListenerMethods<'py> {
        fn get_done(&self) -> bool;
        fn stop(&self);
        fn keep_receiving(&self) -> PyResult<()>;
    }

    impl<'py> StatsListenerMethods<'py> for Bound<'py, StatsListener> {
        fn get_done(&self) -> bool {
            self.get().done.load(Ordering::Acquire)
        }

        fn stop(&self) {
            let listener = self.get();
            listener.done.store(true, Ordering::Release);
            listener.stopped.wake();
        }

        fn keep_receiving(&self) -> PyResult<()> {
            spawn_receiver(self.clone().unbind())
                .map(|_| ())
                .map_err(|err| {
                    PyIOError::new_err(format!("could not start the stats thread: {err}"))
                })
        }
    }

    /// Waits until the listener is stopped.
    fn stopped(listener: &StatsListener) -> impl Future<Output = ()> {
        poll_fn(|context| {
            listener.stopped.register(context.waker());
            if listener.done.load(Ordering::Acquire) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
    }

    /// Hands every message of the stats socket to the handler until the listener is stopped.
    async fn receive_stats(
        listener: &StatsListener,
        mut handle_message: impl FnMut(&str),
    ) -> ZmqResult<()> {
        let socket = get_zmq_socket(&listener.address, &listener.password)?;
        let mut stopped = pin!(stopped(listener));

        while !listener.done.load(Ordering::Acquire) {
            match select(pin!(socket.recv_msg_wait()), stopped.as_mut()).await {
                Either::Left((message, _)) => handle_message(&message?.to_string()),
                Either::Right(_) => break,
            }
        }
        Ok(())
    }

    /// Receives the stats in a thread of its own, which sleeps until a message arrives.
    fn spawn_receiver(listener: Py<StatsListener>) -> std::io::Result<std::thread::JoinHandle<()>> {
        std::thread::Builder::new()
            .name("shinqlx-stats".into())
            .spawn(move || {
                let result = block_on(receive_stats(listener.get(), |message| {
                    Python::attach(|py| handle_zmq_msg(py, message));
                }));
                if let Err(err) = result {
                    Python::attach(|py| {
                        log_exception(py, &PyIOError::new_err(format!("zmq error: {err:?}")));
                    });
                }
            })
    }

    fn get_zmq_socket(address: &str, password: &str) -> ZmqResult<SubscribeSocket> {
        let context = ContextBuilder::default()
            .blocky(false)
            .max_sockets(10)
            .io_threads(1)
            .build()?;

        let socket_builder = SocketBuilder::default()
            .receive_timeout(0)
            .receive_highwater_mark(0)
            .send_timeout(0)
            .send_highwater_mark(0);
        let socket_builder = if password.is_empty() {
            socket_builder
        } else {
            socket_builder
                .zap_domain("stats")
                .security_mechanism(SecurityMechanism::Plain {
                    username: "stats".into(),
                    password: password.into(),
                })
        };

        let config = SubscribeBuilder::default()
            .socket_builder(socket_builder)
            .subscribe("");

        let socket = config.build_from_context(&context)?;

        socket.connect(address)?;

        Ok(socket)
    }

    #[cfg(test)]
    mod stats_listener_tests {
        use alloc::sync::Arc;
        use core::{borrow::BorrowMut, time::Duration};
        use std::sync::mpsc;

        use arzmq::prelude::{Context, PublishSocket, SendFlags, Sender};
        use async_io::block_on;
        use pretty_assertions::assert_eq;
        use pyo3::{exceptions::PyEnvironmentError, prelude::*};
        use rstest::*;

        use super::{
            Ordering, StatsListener, StatsListenerMethods, get_zmq_socket, receive_stats,
            spawn_receiver,
        };
        use crate::{
            engine::prelude::{CVar, CVarBuilder, cvar_t},
            prelude::*,
            python::pyshinqlx_setup_fixture::*,
        };

        #[rstest]
        #[case::without_password("")]
        #[case::with_password("p4ssw0rd")]
        #[cfg_attr(miri, ignore)]
        fn get_zmq_socket_creates_socket_with_and_without_password(#[case] password: &str) {
            let result = get_zmq_socket("tcp://127.0.0.1:1", password);

            assert!(result.is_ok());
        }

        #[rstest]
        #[cfg_attr(miri, ignore)]
        #[serial]
        fn constructor_with_no_main_engine(_pyshinqlx_setup: ()) {
            Python::attach(|py| {
                let result = StatsListener::py_new();
                assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
            });
        }

        #[test]
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
                        StatsListener::new(true, String::new(), String::new())
                    );
                });
        }

        #[test]
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
                        StatsListener::new(false, "tcp://127.0.0.1:27960".into(), String::new())
                    );
                });
        }

        #[test]
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
                        StatsListener::new(
                            false,
                            "tcp://192.168.0.1:28960".into(),
                            "p4ssw0rd".into(),
                        )
                    );
                });
        }

        #[rstest]
        #[cfg_attr(miri, ignore)]
        #[serial]
        fn stop_sets_done_field(_pyshinqlx_setup: ()) {
            Python::attach(|py| {
                let listener =
                    Bound::new(py, StatsListener::new(false, String::new(), String::new()))
                        .expect("this should not happen");

                listener.stop();

                assert_eq!(listener.get_done(), true);
            });
        }

        fn receive_in_thread(
            listener: &Arc<StatsListener>,
            messages: mpsc::Sender<String>,
        ) -> mpsc::Receiver<Result<(), String>> {
            let (finished, result) = mpsc::channel();
            let receiving = listener.clone();
            std::thread::spawn(move || {
                let outcome = block_on(receive_stats(&receiving, |message| {
                    let _ = messages.send(message.to_owned());
                }));
                let _ = finished.send(outcome.map_err(|err| format!("{err:?}")));
            });
            result
        }

        fn stop(listener: &StatsListener) {
            listener.done.store(true, Ordering::Release);
            listener.stopped.wake();
        }

        #[test]
        #[cfg_attr(miri, ignore)]
        fn receive_stats_returns_right_away_when_already_done() {
            let listener = StatsListener::new(true, "tcp://127.0.0.1:1".into(), String::new());

            let result = block_on(receive_stats(&listener, |_| {}));

            assert!(result.is_ok());
        }

        #[test]
        #[cfg_attr(miri, ignore)]
        fn receive_stats_ends_when_stopped_while_waiting() {
            let listener = Arc::new(StatsListener::new(
                false,
                "tcp://127.0.0.1:1".into(),
                String::new(),
            ));
            let (messages, _received) = mpsc::channel();
            let result = receive_in_thread(&listener, messages);

            std::thread::sleep(Duration::from_millis(200));
            stop(&listener);

            assert_eq!(result.recv_timeout(Duration::from_secs(10)), Ok(Ok(())));
        }

        #[test]
        #[cfg_attr(miri, ignore)]
        fn receive_stats_hands_every_message_to_the_handler() {
            let context = Context::new().expect("could not create the context");
            let publisher =
                PublishSocket::from_context(&context).expect("could not create the publisher");
            publisher
                .bind("tcp://127.0.0.1:*")
                .expect("could not bind the publisher");
            let address = publisher
                .last_endpoint()
                .expect("could not get the endpoint");
            let listener = Arc::new(StatsListener::new(false, address, String::new()));
            let (messages, received) = mpsc::channel();
            let result = receive_in_thread(&listener, messages);

            for _ in 0..100 {
                let _ = publisher.send_msg("warmup", SendFlags::DONT_WAIT);
                if received.recv_timeout(Duration::from_millis(50)).is_ok() {
                    break;
                }
            }
            for message in ["first", "second", "third"] {
                let _ = publisher.send_msg(message, SendFlags::DONT_WAIT);
            }
            let mut collected = vec![];
            while collected.last().map(String::as_str) != Some("third") {
                collected.push(
                    received
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap_or_else(|_| panic!("the messages did not arrive: {collected:?}")),
                );
            }
            collected.retain(|message| message != "warmup");
            stop(&listener);

            assert_eq!(collected, ["first", "second", "third"]);
            assert_eq!(result.recv_timeout(Duration::from_secs(10)), Ok(Ok(())));
        }

        #[rstest]
        #[cfg_attr(miri, ignore)]
        #[serial]
        fn the_receiver_thread_ends_after_stop(_pyshinqlx_setup: ()) {
            let listener = Python::attach(|py| {
                Py::new(
                    py,
                    StatsListener::new(false, "tcp://127.0.0.1:1".into(), String::new()),
                )
                .expect("could not create the listener")
            });
            let handle = spawn_receiver(Python::attach(|py| listener.clone_ref(py)))
                .expect("could not start the receiver");

            std::thread::sleep(Duration::from_millis(200));
            Python::attach(|py| listener.bind(py).stop());

            assert!(handle.join().is_ok());
        }
    }
}

fn try_handle_zmq_msg(py: Python<'_>, zmq_msg: &str) -> PyResult<()> {
    let stats = py.detach(|| {
        from_str::<Value>(zmq_msg).map_err(|err: serde_json::Error| {
            let error_msg = format!("error parsing json data: {err:?}");
            PyIOError::new_err(error_msg)
        })
    })?;

    dispatch_stats_event(py, zmq_msg)?;
    match stats["TYPE"].as_str().and_then(StatsType::from_name) {
        Some(StatsType::MatchStarted) => {
            IN_PROGRESS.store(true, Ordering::Release);
            dispatch_game_start_event(py, &stats["DATA"])
        }
        Some(StatsType::RoundOver) => dispatch_round_end_event(py, &stats["DATA"]),
        Some(StatsType::MatchReport) => {
            if IN_PROGRESS.load(Ordering::Acquire) {
                dispatch_game_end_event(py, &stats["DATA"])?;
            }
            IN_PROGRESS.store(false, Ordering::Release);
            Ok(())
        }
        Some(StatsType::PlayerDeath) => handle_player_death_event(py, stats),
        Some(StatsType::PlayerSwitchteam) => handle_team_switch_event(py, stats),
        _ => Ok(()),
    }
}

pub(crate) fn handle_zmq_msg(py: Python<'_>, zmq_msg: &str) {
    let _ = try_handle_zmq_msg(py, zmq_msg).tap_err(|e| {
        log_exception(py, e);
    });
}

#[cfg(test)]
mod handle_zmq_msg_tests {
    use core::{borrow::BorrowMut, sync::atomic::Ordering};

    use mockall::predicate;
    use pyo3::{
        exceptions::{PyAssertionError, PyEnvironmentError, PyIOError},
        intern,
        prelude::*,
    };
    use rstest::*;

    use super::{IN_PROGRESS, handle_zmq_msg, to_py_json_data, try_handle_zmq_msg};
    use crate::{
        engine::prelude::{
            CVar, CVarBuilder, MockClient, MockGameEntity, clientState_t, cvar_t, privileges_t,
            team_t,
        },
        prelude::*,
        python::{
            EVENT_DISPATCHERS,
            PythonReturnCodes::RET_STOP_EVENT,
            commands::CommandPriorities,
            events::{
                DeathDispatcher, EventDispatcherManager, EventDispatcherManagerMethods,
                GameEndDispatcher, GameStartDispatcher, KillDispatcher, RoundEndDispatcher,
                StatsDispatcher, TeamSwitchDispatcher,
            },
            pyshinqlx_setup_fixture::*,
            pyshinqlx_test_support::{run_all_frame_tasks, *},
        },
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_zmq_msg_for_unparseable_json_msg(_pyshinqlx_setup: ()) {
        let zmq_msg = r#"{"INVALID":"#;

        Python::attach(|py| {
            let result = try_handle_zmq_msg(py, zmq_msg);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyIOError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_zmq_msg_for_unparseable_json_msg(_pyshinqlx_setup: ()) {
        let zmq_msg = r#"{"INVALID":"#;

        Python::attach(|py| {
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
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                    let asdf = capturing_hook
                        .call_method1(intern!(py, "assert_called_with"), (expected_json_data,));
                    assert!(asdf.as_ref().is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_handle_stats_msg_with_no_stats_dispatcher(_pyshinqlx_setup: ()) {
        let stats_msg = r#"{"DATA": {}, "TYPE": "STATS"}"#;

        Python::attach(|py| {
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "game_start"))
                        .and_then(|game_start_dispatcher| {
                            game_start_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to game_start dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    let result = try_handle_zmq_msg(py, game_start_data);
                    assert!(result.is_ok());
                    assert!(IN_PROGRESS.load(Ordering::Acquire));

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, game_start_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data.to_owned(),)
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data
                                    .get_item(intern!(py, "DATA"))
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

        Python::attach(|py| {
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "round_end"))
                        .and_then(|round_end_dispatcher| {
                            round_end_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data.to_owned(),)
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data
                                    .get_item(intern!(py, "DATA"))
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

        Python::attach(|py| {
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "game_end"))
                        .and_then(|game_end_dispatcher| {
                            game_end_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to game_end dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    IN_PROGRESS.store(true, Ordering::Release);

                    let result = try_handle_zmq_msg(py, game_end_data);
                    assert!(result.is_ok());
                    assert!(!IN_PROGRESS.load(Ordering::Acquire));

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, game_end_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data.to_owned(),)
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data
                                    .get_item(intern!(py, "DATA"))
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "game_end"))
                        .and_then(|game_end_dispatcher| {
                            game_end_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to game_end dispatcher");
                    EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

                    IN_PROGRESS.store(false, Ordering::Release);

                    let result = try_handle_zmq_msg(py, game_end_data);
                    assert!(result.is_ok());
                    assert!(!IN_PROGRESS.load(Ordering::Acquire));

                    run_all_frame_tasks(py).expect("this should not happen");

                    let expected_json_data =
                        to_py_json_data(py, game_end_data).expect("this should not happen");
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data.to_owned(),)
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data
                                    .get_item(intern!(py, "DATA"))
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

        Python::attach(|py| {
            let event_dispatcher =
                Bound::new(py, EventDispatcherManager::default()).expect("this should not happen");
            event_dispatcher
                .add_dispatcher(&py.get_type::<StatsDispatcher>())
                .expect("could not add stats dispatcher");
            EVENT_DISPATCHERS.store(Some(event_dispatcher.unbind().into()));

            IN_PROGRESS.store(true, Ordering::Release);

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
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(intern!(py, "assert_called_with"), (expected_json_data,))
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "death"))
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data.to_owned(),)
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (
                                    "_",
                                    py.None(),
                                    expected_json_data
                                        .get_item(intern!(py, "DATA"))
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "death"))
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data.to_owned(),)
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (
                                    "_",
                                    py.None(),
                                    expected_json_data
                                        .get_item(intern!(py, "DATA"))
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
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(intern!(py, "assert_called_with"), (expected_json_data,))
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
            Python::attach(|py| {
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "death"))
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to death dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "kill"))
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data.to_owned(),)
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (
                                    "_",
                                    "_",
                                    expected_json_data
                                        .get_item(intern!(py, "DATA"))
                                        .expect("this should not happen"),
                                )
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (
                                    "_",
                                    "_",
                                    expected_json_data
                                        .get_item(intern!(py, "DATA"))
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "death"))
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to death dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "kill"))
                        .and_then(|death_dispatcher| {
                            death_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (expected_json_data.to_owned(),)
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (
                                    "_",
                                    "_",
                                    expected_json_data
                                        .get_item(intern!(py, "DATA"))
                                        .expect("this should not happen"),
                                )
                            )
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                (
                                    "_",
                                    "_",
                                    expected_json_data
                                        .get_item(intern!(py, "DATA"))
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
            Python::attach(|py| {
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "team_switch"))
                        .and_then(|team_switch_dispatcher| {
                            team_switch_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(intern!(py, "assert_called_with"), (expected_json_data,))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                ("_", "spectator", "blue")
                            )
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
                Python::attach(|py| {
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
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
                                        .expect("could not get capturing hook"),
                                    CommandPriorities::PRI_NORMAL as i32,
                                ),
                            )
                        })
                        .expect("could not add hook to stats dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "team_switch"))
                        .and_then(|team_switch_dispatcher| {
                            team_switch_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(intern!(py, "assert_called_with"), (expected_json_data,))
                            .is_ok()
                    );
                    assert!(
                        capturing_hook
                            .call_method1(
                                intern!(py, "assert_called_with"),
                                ("_", "spectator", "blue")
                            )
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
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(intern!(py, "assert_called_with"), (expected_json_data,))
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
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(intern!(py, "assert_called_with"), (expected_json_data,))
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
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(intern!(py, "assert_called_with"), (expected_json_data,))
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
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(intern!(py, "assert_called_with"), (expected_json_data,))
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
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    let capturing_hook = capturing_hook(py);
                    event_dispatcher
                        .get_item(intern!(py, "stats"))
                        .and_then(|stats_dispatcher| {
                            stats_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    capturing_hook
                                        .getattr(intern!(py, "hook"))
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
                            .call_method1(intern!(py, "assert_called_with"), (expected_json_data,))
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
            Python::attach(|py| {
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
                Python::attach(|py| {
                    let event_dispatcher = Bound::new(py, EventDispatcherManager::default())
                        .expect("this should not happen");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<StatsDispatcher>())
                        .expect("could not add stats dispatcher");
                    event_dispatcher
                        .add_dispatcher(&py.get_type::<TeamSwitchDispatcher>())
                        .expect("could not add team_switch dispatcher");
                    event_dispatcher
                        .get_item(intern!(py, "team_switch"))
                        .and_then(|team_switch_dispatcher| {
                            team_switch_dispatcher.call_method1(
                                intern!(py, "add_hook"),
                                (
                                    "asdf",
                                    python_function_returning(py, &(RET_STOP_EVENT as i32)),
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
