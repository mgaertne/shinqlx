use pyo3::{
    intern,
    prelude::*,
    types::{PyDict, PyModule},
};

/// The names of the events that plugins can hook, as the dispatchers register them.
pub(crate) const EVENT_NAMES: [&str; 33] = [
    "chat",
    "client_command",
    "command",
    "console_print",
    "damage",
    "death",
    "frame",
    "game_countdown",
    "game_end",
    "game_start",
    "kamikaze_explode",
    "kamikaze_use",
    "kill",
    "map",
    "new_game",
    "player_connect",
    "player_disconnect",
    "player_loaded",
    "player_spawn",
    "round_countdown",
    "round_end",
    "round_start",
    "server_command",
    "set_configstring",
    "stats",
    "team_switch",
    "team_switch_attempt",
    "unload",
    "userinfo",
    "vote",
    "vote_called",
    "vote_ended",
    "vote_started",
];

fn events_class_source() -> String {
    let members = EVENT_NAMES
        .iter()
        .map(|name| format!("    {} = \"{name}\"", name.to_uppercase()))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"import enum


class Events(str, enum.Enum):
    """The names of the events that plugins can hook; the members are strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

{members}
"#
    )
}

/// Adds the `Events` string enumeration to the module.
///
/// It is created with Python's `enum`, because a string enumeration cannot be a Rust class: its
/// members have to be real strings to be accepted, and to hash, like the names they stand for.
pub(crate) fn register_events_enum(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let globals = PyDict::new(py);
    globals.set_item(intern!(py, "__name__"), "_shinqlx")?;
    let source = std::ffi::CString::new(events_class_source())?;
    py.run(&source, Some(&globals), None)?;

    let events = globals
        .get_item(intern!(py, "Events"))?
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("Events could not be created"))?;
    m.add("Events", events)
}

#[cfg(test)]
mod events_enum_tests {
    use pyo3::{intern, prelude::*, types::PyDict};
    use rstest::rstest;

    use super::{EVENT_NAMES, register_events_enum};
    use crate::{ffi::python::pyshinqlx_setup_fixture::pyshinqlx_setup, prelude::*};

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn events_behave_like_the_strings_they_stand_for(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = PyModule::new(py, "events_test").expect("could not create module");
            register_events_enum(&module).expect("could not register events");
            let globals = PyDict::new(py);
            globals
                .set_item(
                    "Events",
                    module.getattr(intern!(py, "Events")).expect("no Events"),
                )
                .expect("could not set item");

            let result = py.run(
                c"
assert Events.CHAT == 'chat'
assert 'player_connect' == Events.PLAYER_CONNECT
assert Events.CHAT != 'vote'
assert str(Events.GAME_START) == 'game_start'
assert f'{Events.MAP}' == 'map'
assert isinstance(Events.KILL, str)
assert {'chat': 1}[Events.CHAT] == 1
assert {Events.CHAT: 2}['chat'] == 2
assert Events('death') is Events.DEATH
assert Events.CHAT.value == 'chat'
",
                Some(&globals),
                None,
            );

            assert!(result.is_ok(), "{result:?}");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn every_name_is_a_member(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = PyModule::new(py, "events_test").expect("could not create module");
            register_events_enum(&module).expect("could not register events");
            let events = module.getattr(intern!(py, "Events")).expect("no Events");

            let members = events
                .try_iter()
                .expect("Events is not iterable")
                .filter_map(|member| member.ok())
                .filter_map(|member| member.extract::<String>().ok())
                .collect::<Vec<_>>();

            assert_eq!(members, EVENT_NAMES);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn members_can_be_extracted_as_rust_strings(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = PyModule::new(py, "events_test").expect("could not create module");
            register_events_enum(&module).expect("could not register events");
            let member = module
                .getattr(intern!(py, "Events"))
                .and_then(|events| events.getattr(intern!(py, "PLAYER_CONNECT")))
                .expect("no member");

            assert_eq!(member.extract::<&str>().ok(), Some("player_connect"));
        });
    }
}
