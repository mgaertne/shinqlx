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

            /// The event that a dispatcher with this `name` stands for.
            pub(crate) fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }

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
    CvarChanged => "cvar_changed",
    Damage => "damage",
    Death => "death",
    FlagStatus => "flag_status",
    Frame => "frame",
    GameCountdown => "game_countdown",
    GameEnd => "game_end",
    GameStart => "game_start",
    ItemTouch => "item_touch",
    KamikazeExplode => "kamikaze_explode",
    KamikazeUse => "kamikaze_use",
    Kill => "kill",
    Map => "map",
    Medal => "medal",
    NewGame => "new_game",
    Objective => "objective",
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
    WeaponFired => "weapon_fired",
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


class Objective(str, enum.Enum):
    """The objectives of the team modes that the event ``objective`` reports; the members are strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

    CAPTURE = "capture"
    RETURN = "return"
    ASSIST = "assist"
    BASE_DEFENSE = "base_defense"
    CARRIER_DEFENSE = "carrier_defense"
    FRAG_CARRIER = "frag_carrier"
    UNKNOWN = "unknown"


class FlagTeam(str, enum.Enum):
    """The flags that the event ``flag_status`` reports; the members are strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

    RED = "red"
    BLUE = "blue"
    NEUTRAL = "neutral"


class Medal(str, enum.Enum):
    """The medals that the event ``medal`` reports, named as the game module names them; the members are strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

    ACCURACY = "ACCURACY"
    ASSIST = "ASSIST"
    COMBOKILL = "COMBOKILL"
    DEFENSE = "DEFENSE"
    EXCELLENT = "EXCELLENT"
    FIRSTFRAG = "FIRSTFRAG"
    HUMILIATION = "HUMILIATION"
    IMPRESSIVE = "IMPRESSIVE"
    MIDAIR = "MIDAIR"
    PERFECT = "PERFECT"
    PERFORATED = "PERFORATED"
    QUADGOD = "QUADGOD"
    RAMPAGE = "RAMPAGE"
    REVENGE = "REVENGE"


class FlagStatus(str, enum.Enum):
    """The states of a flag that the event ``flag_status`` reports; the members are strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

    AT_BASE = "at_base"
    TAKEN = "taken"
    DROPPED = "dropped"


_GAME_TYPE_LONG_NAMES = {{
    "ffa": "Free for All",
    "duel": "Duel",
    "race": "Race",
    "tdm": "Team Deathmatch",
    "ca": "Clan Arena",
    "ctf": "Capture the Flag",
    "1f": "One Flag",
    "har": "Harvester",
    "ft": "Freeze Tag",
    "dom": "Domination",
    "ad": "Attack and Defend",
    "rr": "Red Rover",
    "N/A": "unknown",
}}


class GameTypes(str, enum.Enum):
    """The game types, as the short names that ``Game.type_short`` returns; the members are strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

    @property
    def long_name(self):
        """The name that ``Game.type`` and ``Game.gametype`` return, e.g. ``"Clan Arena"``."""
        return _GAME_TYPE_LONG_NAMES[self.value]

    FREE_FOR_ALL = "ffa"
    DUEL = "duel"
    RACE = "race"
    TEAM_DEATHMATCH = "tdm"
    CLAN_ARENA = "ca"
    CAPTURE_THE_FLAG = "ctf"
    ONE_FLAG = "1f"
    HARVESTER = "har"
    FREEZE_TAG = "ft"
    DOMINATION = "dom"
    ATTACK_AND_DEFEND = "ad"
    RED_ROVER = "rr"
    UNKNOWN = "N/A"


class VoteTypes(str, enum.Enum):
    """The votes of the game itself, as the event ``vote_called`` passes them; custom votes stay plain strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

    MAP = "map"
    MAP_RESTART = "map_restart"
    NEXTMAP = "nextmap"
    KICK = "kick"
    CLIENTKICK = "clientkick"
    SHUFFLE = "shuffle"
    TEAMSIZE = "teamsize"
    TIMELIMIT = "timelimit"
    FRAGLIMIT = "fraglimit"
    ROUNDLIMIT = "roundlimit"
    SCORELIMIT = "scorelimit"
    CAPTURELIMIT = "capturelimit"
    WEAPRESPAWN = "weaprespawn"
    COINTOSS = "cointoss"
    ALLREADY = "allready"
    ABORT = "abort"
    LOCK = "lock"
    UNLOCK = "unlock"
    PAUSE = "pause"
    UNPAUSE = "unpause"
    FORFEIT = "forfeit"


class StatsTypes(str, enum.Enum):
    """The values of ``TYPE`` in the data of the stats events; the members are strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

    FLAG_STATUS = "FLAG_STATUS"
    MATCH_REPORT = "MATCH_REPORT"
    MATCH_STARTED = "MATCH_STARTED"
    PLAYER_CONNECT = "PLAYER_CONNECT"
    PLAYER_DEATH = "PLAYER_DEATH"
    PLAYER_DISCONNECT = "PLAYER_DISCONNECT"
    PLAYER_KILL = "PLAYER_KILL"
    PLAYER_MEDAL = "PLAYER_MEDAL"
    PLAYER_STATS = "PLAYER_STATS"
    PLAYER_SWITCHTEAM = "PLAYER_SWITCHTEAM"
    ROUND_OVER = "ROUND_OVER"


class ChatChannels(str, enum.Enum):
    """The names of the channels of the framework, as ``AbstractChannel.name`` returns them; the members are strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

    CONSOLE = "console"
    CHAT = "chat"
    TELL = "tell"
    RED_TEAM_CHAT = "red_team_chat"
    BLUE_TEAM_CHAT = "blue_team_chat"
    FREE_CHAT = "free_chat"
    SPECTATOR_CHAT = "spectator_chat"


class EntityTypes(str, enum.Enum):
    """The types of the entities of the game, as ``EntityInfo.etype`` reports them; the members are strings."""

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)

    GENERAL = "general"
    PLAYER = "player"
    ITEM = "item"
    MISSILE = "missile"
    MOVER = "mover"
    BEAM = "beam"
    PORTAL = "portal"
    SPEAKER = "speaker"
    PUSH_TRIGGER = "push_trigger"
    TELEPORT_TRIGGER = "teleport_trigger"
    INVISIBLE = "invisible"
    GRAPPLE = "grapple"
    TEAM = "team"
    EVENTS = "events"
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
    m.add("Events", events)?;

    let objective = globals.get_item(intern!(py, "Objective"))?.ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("Objective could not be created")
    })?;
    m.add("Objective", objective)?;

    [
        "FlagTeam",
        "FlagStatus",
        "Medal",
        "GameTypes",
        "VoteTypes",
        "StatsTypes",
        "ChatChannels",
        "EntityTypes",
    ]
    .into_iter()
    .try_for_each(|name| {
        let member = globals.get_item(name)?.ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("{name} could not be created"))
        })?;
        m.add(name, member)
    })
}

#[cfg(test)]
mod events_enum_tests {
    use pyo3::{intern, prelude::*, types::PyDict};
    use rstest::rstest;

    use super::{EventName, register_events_enum};
    use crate::{
        ffi::python::{GameTypes, pyshinqlx_setup_fixture::pyshinqlx_setup},
        prelude::*,
    };

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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_types_match_the_names_of_the_game(_pyshinqlx_setup: ()) {
        Python::attach(|py| {
            let module = PyModule::new(py, "events_test").expect("could not create module");
            register_events_enum(&module).expect("could not register events");
            let game_types = module
                .getattr(intern!(py, "GameTypes"))
                .expect("no GameTypes");

            (0..=13).map(GameTypes::from).for_each(|game_type| {
                let member = game_types
                    .call1((game_type.type_short(),))
                    .expect("no member for the short name");
                assert_eq!(
                    member
                        .getattr(intern!(py, "long_name"))
                        .and_then(|name| name.extract::<String>())
                        .ok()
                        .as_deref(),
                    Some(game_type.type_long()),
                );
            });
        });
    }

    #[rstest]
    #[case("VoteTypes", "map", "MAP")]
    #[case("StatsTypes", "PLAYER_DEATH", "PLAYER_DEATH")]
    #[case("ChatChannels", "red_team_chat", "RED_TEAM_CHAT")]
    #[case("EntityTypes", "item", "ITEM")]
    #[case("GameTypes", "ca", "CLAN_ARENA")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn string_enums_compare_and_format_like_their_strings(
        _pyshinqlx_setup: (),
        #[case] class: &str,
        #[case] value: &str,
        #[case] member: &str,
    ) {
        Python::attach(|py| {
            let module = PyModule::new(py, "events_test").expect("could not create module");
            register_events_enum(&module).expect("could not register events");
            let globals = PyDict::new(py);
            globals
                .set_item(
                    "member",
                    module
                        .getattr(class)
                        .and_then(|enum_class| enum_class.getattr(member))
                        .expect("no member"),
                )
                .expect("could not set item");
            globals
                .set_item("value", value)
                .expect("could not set item");

            let result = py.run(
                c"
assert member == value and value == member
assert str(member) == value
assert f'{member}' == value
assert hash(member) == hash(value)
",
                Some(&globals),
                None,
            );
            assert!(result.is_ok(), "{result:?}");
        });
    }
}
