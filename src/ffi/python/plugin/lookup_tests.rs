use pyo3::prelude::*;
use rstest::rstest;

use super::{
    Plugin,
    lookup::{TargetLookup, lookup_target},
};
use crate::{
    ffi::python::{
        prelude::*,
        pyshinqlx_setup_fixture::pyshinqlx_setup,
        pyshinqlx_test_support::{default_test_player, with_connected_players},
    },
    prelude::*,
};

fn player_named(id: i32, name: &str) -> Player {
    Player {
        id,
        name: name.to_string().into(),
        ..default_test_player()
    }
}

fn lobby() -> Vec<Player> {
    vec![
        player_named(1, "^1Ada"),
        player_named(2, "Ada2"),
        player_named(3, "Bob"),
        player_named(10, "Cy 3"),
    ]
}

fn found_id(lookup: TargetLookup) -> Option<i32> {
    match lookup {
        TargetLookup::Found(player) => Some(player.id),
        _ => None,
    }
}

#[rstest]
#[case("3", Some(3))]
#[case("id:3", Some(3))]
#[case("id=3", Some(3))]
#[case("10", Some(10))]
#[case("bob", Some(3))]
#[case("^2BOB", Some(3))]
#[case("ada2", Some(2))]
#[case("cy", Some(10))]
fn a_unique_player_is_found(#[case] target: &str, #[case] expected: Option<i32>) {
    assert_eq!(found_id(lookup_target(lobby(), target)), expected);
}

#[rstest]
fn a_valid_id_wins_over_a_name_that_contains_it() {
    assert_eq!(found_id(lookup_target(lobby(), "3")), Some(3));
}

#[rstest]
#[case("zzz")]
#[case("id:9")]
#[case("9")]
#[case("100")]
#[case("id:100")]
fn an_unknown_target_is_not_found(#[case] target: &str) {
    assert!(matches!(
        lookup_target(lobby(), target),
        TargetLookup::NotFound
    ));
}

#[rstest]
#[case("ada", 2)]
#[case("a", 2)]
#[case("", 4)]
fn several_matching_names_are_ambiguous(#[case] target: &str, #[case] count: usize) {
    assert!(matches!(
        lookup_target(lobby(), target),
        TargetLookup::Ambiguous(matches) if matches.len() == count
    ));
}

#[rstest]
fn a_number_that_is_no_connected_client_is_matched_against_names() {
    assert_eq!(found_id(lookup_target(lobby(), "3")), Some(3));
    let players = vec![player_named(4, "Player 007"), player_named(5, "Eve")];

    assert_eq!(found_id(lookup_target(players, "007")), Some(4));
}

fn recording_caller(py: Python<'_>) -> Bound<'_, PyAny> {
    PyModule::from_code(
        py,
        c"
class Caller:
    def __init__(self):
        self.told = []

    def tell(self, message):
        self.told.append(message)
",
        c"",
        c"",
    )
    .and_then(|module| module.getattr("Caller"))
    .and_then(|caller| caller.call0())
    .expect("could not create the caller")
}

fn told(caller: &Bound<'_, PyAny>) -> Vec<String> {
    caller
        .getattr("told")
        .and_then(|told| told.extract())
        .expect("could not get the messages")
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn the_class_method_returns_the_found_player_without_telling_anyone(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let caller = recording_caller(py);

        let result =
            Plugin::find_by_name_or_id(&py.get_type::<Plugin>(), &caller, "bob", Some(lobby()));

        assert!(result.is_ok_and(|player| player.is_some_and(|player| player.id == 3)));
        assert!(told(&caller).is_empty());
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn the_class_method_tells_the_caller_when_nobody_matches(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let caller = recording_caller(py);

        let result =
            Plugin::find_by_name_or_id(&py.get_type::<Plugin>(), &caller, "zzz", Some(lobby()));

        assert!(result.is_ok_and(|player| player.is_none()));
        assert_eq!(
            told(&caller),
            ["Sorry, but no players matched your tokens: zzz."]
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn the_class_method_lists_the_candidates_when_several_match(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let caller = recording_caller(py);

        let result =
            Plugin::find_by_name_or_id(&py.get_type::<Plugin>(), &caller, "ada", Some(lobby()));

        assert!(result.is_ok_and(|player| player.is_none()));
        assert_eq!(
            told(&caller),
            [
                "A total of ^62^7 players matched for ada:",
                "  1^6:^7 ^1Ada\n  2^6:^7 Ada2"
            ]
        );
    });
}

#[rstest]
#[case("bob", Some(2))]
#[case("2", Some(2))]
#[case("id:0", Some(0))]
#[case("^1ADA", Some(0))]
#[case("zzz", None)]
#[case("1", None)]
#[cfg_attr(miri, ignore)]
#[serial]
fn the_class_method_searches_the_connected_players_of_the_engine(
    _pyshinqlx_setup: (),
    #[case] target: &str,
    #[case] expected: Option<i32>,
) {
    with_connected_players(
        MockEngineBuilder::default(),
        &[(0, "Ada"), (2, "Bob")],
        || {
            Python::attach(|py| {
                let caller = recording_caller(py);

                let result =
                    Plugin::find_by_name_or_id(&py.get_type::<Plugin>(), &caller, target, None)
                        .expect("could not look the player up");

                assert_eq!(result.map(|player| player.id), expected);
                assert_eq!(told(&caller).len(), usize::from(expected.is_none()));
            });
        },
    );
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn the_class_method_lists_the_connected_candidates_of_the_engine(_pyshinqlx_setup: ()) {
    with_connected_players(
        MockEngineBuilder::default(),
        &[(0, "Bob"), (1, "Bobby"), (3, "Ann")],
        || {
            Python::attach(|py| {
                let caller = recording_caller(py);

                let result =
                    Plugin::find_by_name_or_id(&py.get_type::<Plugin>(), &caller, "bob", None)
                        .expect("could not look the player up");

                assert!(result.is_none());
                assert_eq!(
                    told(&caller),
                    [
                        "A total of ^62^7 players matched for bob:",
                        "  0^6:^7 Bob\n  1^6:^7 Bobby"
                    ]
                );
            });
        },
    );
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn the_class_method_works_without_a_caller_to_tell(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Plugin::find_by_name_or_id(
            &py.get_type::<Plugin>(),
            py.None().bind(py),
            "zzz",
            Some(lobby()),
        );

        assert!(result.is_ok_and(|player| player.is_none()));
    });
}
