use pyo3::{
    intern,
    prelude::*,
    types::{PyDict, PyModule},
};

macro_rules! event_names {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// The events that plugins can hook, as the dispatchers register them.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum EventName {
            $($variant),+
        }

        impl EventName {
            pub(crate) const ALL: &[Self] = &[$(Self::$variant),+];

            /// The name under which the dispatcher of the event is registered.
            pub(crate) const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }
        }
    };
}

event_names! {
    Chat => "chat",
    ClientCommand => "client_command",
    Command => "command",
    ConsolePrint => "console_print",
    Damage => "damage",
    Death => "death",
    Frame => "frame",
    GameCountdown => "game_countdown",
    GameEnd => "game_end",
    GameStart => "game_start",
    KamikazeExplode => "kamikaze_explode",
    KamikazeUse => "kamikaze_use",
    Kill => "kill",
    Map => "map",
    NewGame => "new_game",
    PlayerConnect => "player_connect",
    PlayerDisconnect => "player_disconnect",
    PlayerLoaded => "player_loaded",
    PlayerSpawn => "player_spawn",
    RoundCountdown => "round_countdown",
    RoundEnd => "round_end",
    RoundStart => "round_start",
    ServerCommand => "server_command",
    SetConfigstring => "set_configstring",
    Stats => "stats",
    TeamSwitch => "team_switch",
    TeamSwitchAttempt => "team_switch_attempt",
    Unload => "unload",
    Userinfo => "userinfo",
    Vote => "vote",
    VoteCalled => "vote_called",
    VoteEnded => "vote_ended",
    VoteStarted => "vote_started",
}

fn events_class_source() -> String {
    let members = EventName::ALL
        .iter()
        .map(|event| {
            let name = event.as_str();
            format!("    {} = \"{name}\"", name.to_uppercase())
        })
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
    let source = alloc::ffi::CString::new(events_class_source())?;
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

    use super::{EventName, register_events_enum};
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

            let names = EventName::ALL
                .iter()
                .map(|event| event.as_str())
                .collect::<Vec<_>>();
            assert_eq!(members, names);
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
