use pyo3::{
    exceptions::{PyEnvironmentError, PyKeyError, PyValueError},
    prelude::*,
    types::{PyBool, PyDict, PyInt, PyString, PyTuple},
};
use rstest::rstest;

use super::{
    super::{prelude::pyshinqlx_setup, pyshinqlx_test_support::default_test_player},
    NonexistentPlayerError, Player,
};
use crate::prelude::*;

fn player(py: Python<'_>) -> Bound<'_, Player> {
    Bound::new(py, default_test_player()).expect("this should not happen")
}

fn value_for<'py>(py: Python<'py>, property: &str) -> Bound<'py, PyAny> {
    match property {
        "cvars" => PyDict::new(py).into_any(),
        "colors" => PyTuple::new(py, [1, 2])
            .expect("this should not happen")
            .into_any(),
        "handicap" | "autohop" | "autoaction" | "predictitems" | "health" | "armor" | "score" => {
            PyInt::new(py, 10).into_any()
        }
        "noclip" | "is_alive" => PyBool::new(py, true).to_owned().into_any(),
        "team" => PyString::new(py, "red").into_any(),
        "privileges" => PyString::new(py, "mod").into_any(),
        "country" => PyString::new(py, "DE").into_any(),
        "holdable" => PyString::new(py, "kamikaze").into_any(),
        _ => PyString::new(py, "value").into_any(),
    }
}

#[rstest]
#[case("cvars", "{}")]
#[case("steam_id", "1234567890")]
#[case("id", "2")]
#[case("ip", "")]
#[case("clan", "")]
#[case("name", "^7")]
#[case("clean_name", "")]
#[case("qport", "-1")]
#[case("team", "spectator")]
#[case("colors", "(0.0, 0.0)")]
#[case("connection_state", "connected")]
#[case("privileges", "None")]
#[case("_valid", "True")]
#[case("valid", "True")]
#[case("channel", "tell")]
#[cfg_attr(miri, ignore)]
#[serial]
fn cached_properties_need_no_engine(
    _pyshinqlx_setup: (),
    #[case] property: &str,
    #[case] expected: &str,
) {
    Python::attach(|py| {
        let result = player(py).getattr(property);

        assert!(
            result
                .as_ref()
                .is_ok_and(|value| value.to_string() == expected),
            "{property}: {result:?}"
        );
    });
}

#[rstest]
#[case("model")]
#[case("headmodel")]
#[case("handicap")]
#[case("autohop")]
#[case("autoaction")]
#[case("predictitems")]
#[case("country")]
#[cfg_attr(miri, ignore)]
#[serial]
fn userinfo_properties_without_the_key_raise_a_key_error(
    _pyshinqlx_setup: (),
    #[case] property: &str,
) {
    Python::attach(|py| {
        let result = player(py).getattr(property);

        assert!(
            result
                .as_ref()
                .is_err_and(|err| err.is_instance_of::<PyKeyError>(py)),
            "{property}: {result:?}"
        );
    });
}

#[rstest]
#[case("state")]
#[case("stats")]
#[case("ping")]
#[case("holdable")]
#[case("noclip")]
#[case("health")]
#[case("armor")]
#[case("is_alive")]
#[case("is_frozen")]
#[case("is_chatting")]
#[case("score")]
#[cfg_attr(miri, ignore)]
#[serial]
fn engine_properties_raise_an_environment_error_without_the_engine(
    _pyshinqlx_setup: (),
    #[case] property: &str,
) {
    Python::attach(|py| {
        let result = player(py).getattr(property);

        assert!(
            result
                .as_ref()
                .is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)),
            "{property}: {result:?}"
        );
    });
}

#[rstest]
#[case("cvars")]
#[case("name")]
#[case("team")]
#[case("colors")]
#[case("model")]
#[case("headmodel")]
#[case("handicap")]
#[case("autohop")]
#[case("autoaction")]
#[case("predictitems")]
#[case("privileges")]
#[case("country")]
#[case("holdable")]
#[case("noclip")]
#[case("health")]
#[case("armor")]
#[case("is_alive")]
#[case("score")]
#[cfg_attr(miri, ignore)]
#[serial]
fn writing_properties_raises_an_environment_error_without_the_engine(
    _pyshinqlx_setup: (),
    #[case] property: &str,
) {
    Python::attach(|py| {
        let result = player(py).setattr(property, value_for(py, property));

        assert!(
            result
                .as_ref()
                .is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)),
            "{property}: {result:?}"
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn writing_the_clan_does_not_need_the_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = player(py).setattr("clan", "x");

        assert!(result.is_ok());
    });
}

#[rstest]
#[case("position")]
#[case("velocity")]
#[case("weapons")]
#[case("weapon")]
#[case("ammo")]
#[case("powerups")]
#[case("flight")]
#[case("drop_holdable")]
#[case("ban")]
#[case("tempban")]
#[case("addadmin")]
#[case("addmod")]
#[case("demote")]
#[case("mute")]
#[case("unmute")]
#[case("slap")]
#[case("slay")]
#[case("kick")]
#[cfg_attr(miri, ignore)]
#[serial]
fn methods_without_arguments_raise_an_environment_error_without_the_engine(
    _pyshinqlx_setup: (),
    #[case] method: &str,
) {
    Python::attach(|py| {
        let result = player(py).call_method0(method);

        assert!(
            result
                .as_ref()
                .is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)),
            "{method}: {result:?}"
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn methods_with_arguments_raise_an_environment_error_without_the_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = player(py);

        let results = [
            ("center_print", player.call_method1("center_print", ("m",))),
            ("put", player.call_method1("put", ("red",))),
            ("addscore", player.call_method1("addscore", (1,))),
            ("slay_with_mod", player.call_method1("slay_with_mod", (1,))),
        ];

        for (method, result) in results {
            assert!(
                result
                    .as_ref()
                    .is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)),
                "{method}: {result:?}"
            );
        }
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn switch_refuses_players_of_the_same_team(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = player(py).call_method1("switch", (player(py),));

        assert!(
            result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)
                && err.value(py).to_string() == "Both players are on the same team.")
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn invalidating_a_player_makes_it_invalid(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = player(py);

        let result = player.call_method1("_invalidate", ("gone",));

        assert!(
            result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)
                && err.value(py).to_string() == "gone")
        );

        assert!(
            player
                .getattr("valid")
                .is_ok_and(|valid| valid.to_string() == "False")
        );
        assert!(
            player
                .getattr("_valid")
                .is_ok_and(|valid| valid.to_string() == "False")
        );
    });
}
