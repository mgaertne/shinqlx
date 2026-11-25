//! The Rust enums behind the string enumerations of the Python API.
//!
//! Each enum is the one source of its values: the Python class is generated from `MEMBERS`, and the Rust code
//! matches on the enum instead of on magic strings.

use core::fmt;

/// `FreeForAll` becomes `FREE_FOR_ALL`, the name of the Python member.
pub(crate) fn member_name(variant: &str) -> String {
    let mut name = String::with_capacity(variant.len() + 4);
    for (index, character) in variant.chars().enumerate() {
        if index > 0 && character.is_ascii_uppercase() {
            name.push('_');
        }
        name.push(character.to_ascii_uppercase());
    }
    name
}

/// The source of a Python `str` enumeration with the given members, as pairs of variant name and value.
pub(crate) fn python_class_source(
    class_name: &str,
    doc: &str,
    members: &[(&str, &str)],
    extra: &str,
) -> String {
    let members = members
        .iter()
        .map(|(variant, value)| format!("    {} = \"{value}\"", member_name(variant)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "class {class_name}(str, enum.Enum):
    \"\"\"{doc}\"\"\"

    def __str__(self):
        return str.__str__(self)

    def __format__(self, format_spec):
        return str.__format__(str(self), format_spec)
{extra}
{members}
"
    )
}

macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        closed $name:ident {
            $($variant:ident => $value:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub(crate) enum $name {
            $($variant),+
        }

        impl $name {
            /// Variant name and value of every member.
            pub(crate) const MEMBERS: &[(&str, &str)] = &[$((stringify!($variant), $value)),+];

            /// The member with this value, if there is one.
            #[allow(dead_code, reason = "not every closed enumeration is parsed from a string")]
            pub(crate) fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($value => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub(crate) const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
    (
        $(#[$meta:meta])*
        open $name:ident {
            $($variant:ident => $value:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub(crate) enum $name {
            $($variant),+,
            /// A value that is not one of the members, e.g. the vote of a plugin.
            Other(String),
        }

        impl $name {
            /// Variant name and value of every member, without `Other`.
            pub(crate) const MEMBERS: &[(&str, &str)] = &[$((stringify!($variant), $value)),+];

            pub(crate) fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $value,)+
                    Self::Other(other) => other,
                }
            }
        }

        impl From<&str> for $name {
            fn from(name: &str) -> Self {
                match name {
                    $($value => Self::$variant,)+
                    other => Self::Other(other.to_owned()),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

string_enum! {
    /// The votes of the game; the votes of plugins are `Other`.
    open VoteType {
        Map => "map",
        MapRestart => "map_restart",
        Nextmap => "nextmap",
        Kick => "kick",
        Clientkick => "clientkick",
        Shuffle => "shuffle",
        Teamsize => "teamsize",
        Timelimit => "timelimit",
        Fraglimit => "fraglimit",
        Roundlimit => "roundlimit",
        Scorelimit => "scorelimit",
        Capturelimit => "capturelimit",
        Weaprespawn => "weaprespawn",
        Cointoss => "cointoss",
        Allready => "allready",
        Abort => "abort",
        Lock => "lock",
        Unlock => "unlock",
        Pause => "pause",
        Unpause => "unpause",
        Forfeit => "forfeit",
    }
}

string_enum! {
    /// The `TYPE` of a stats message.
    closed StatsType {
        FlagStatus => "FLAG_STATUS",
        MatchReport => "MATCH_REPORT",
        MatchStarted => "MATCH_STARTED",
        PlayerConnect => "PLAYER_CONNECT",
        PlayerDeath => "PLAYER_DEATH",
        PlayerDisconnect => "PLAYER_DISCONNECT",
        PlayerKill => "PLAYER_KILL",
        PlayerMedal => "PLAYER_MEDAL",
        PlayerStats => "PLAYER_STATS",
        PlayerSwitchteam => "PLAYER_SWITCHTEAM",
        RoundOver => "ROUND_OVER",
    }
}

string_enum! {
    /// The name of a channel of the framework.
    closed ChatChannel {
        Console => "console",
        Chat => "chat",
        Tell => "tell",
        RedTeamChat => "red_team_chat",
        BlueTeamChat => "blue_team_chat",
        FreeChat => "free_chat",
        SpectatorChat => "spectator_chat",
    }
}

/// The type of an entity of the game; `General` keeps the number of the engine, which is 0 for a plain entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EntityType {
    General(i32),
    Player,
    Item,
    Missile,
    Mover,
    Beam,
    Portal,
    Speaker,
    PushTrigger,
    TeleportTrigger,
    Invisible,
    Grapple,
    Team,
    Events,
}

impl EntityType {
    /// Variant name and value of every member.
    pub(crate) const MEMBERS: &[(&str, &str)] = &[
        ("General", "general"),
        ("Player", "player"),
        ("Item", "item"),
        ("Missile", "missile"),
        ("Mover", "mover"),
        ("Beam", "beam"),
        ("Portal", "portal"),
        ("Speaker", "speaker"),
        ("PushTrigger", "push_trigger"),
        ("TeleportTrigger", "teleport_trigger"),
        ("Invisible", "invisible"),
        ("Grapple", "grapple"),
        ("Team", "team"),
        ("Events", "events"),
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::General(_) => "general",
            Self::Player => "player",
            Self::Item => "item",
            Self::Missile => "missile",
            Self::Mover => "mover",
            Self::Beam => "beam",
            Self::Portal => "portal",
            Self::Speaker => "speaker",
            Self::PushTrigger => "push_trigger",
            Self::TeleportTrigger => "teleport_trigger",
            Self::Invisible => "invisible",
            Self::Grapple => "grapple",
            Self::Team => "team",
            Self::Events => "events",
        }
    }
}

impl fmt::Display for EntityType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<i32> for EntityType {
    /// The values from `ET_EVENTS` on are events with a number added; anything else unknown is `General`.
    fn from(entity_type: i32) -> Self {
        match entity_type {
            1 => Self::Player,
            2 => Self::Item,
            3 => Self::Missile,
            4 => Self::Mover,
            5 => Self::Beam,
            6 => Self::Portal,
            7 => Self::Speaker,
            8 => Self::PushTrigger,
            9 => Self::TeleportTrigger,
            10 => Self::Invisible,
            11 => Self::Grapple,
            12 => Self::Team,
            13.. => Self::Events,
            other => Self::General(other),
        }
    }
}

#[cfg(test)]
mod string_enums_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::{ChatChannel, EntityType, StatsType, VoteType, member_name};

    #[rstest]
    #[case("Map", "MAP")]
    #[case("FreeForAll", "FREE_FOR_ALL")]
    #[case("PlayerSwitchteam", "PLAYER_SWITCHTEAM")]
    #[case("RedTeamChat", "RED_TEAM_CHAT")]
    fn member_names_are_upper_snake_case(#[case] variant: &str, #[case] expected: &str) {
        assert_eq!(member_name(variant), expected);
    }

    #[test]
    fn known_votes_are_members_and_other_votes_are_kept() {
        assert_eq!(VoteType::from("teamsize"), VoteType::Teamsize);
        assert_eq!(
            VoteType::from("my_custom_vote"),
            VoteType::Other("my_custom_vote".to_string())
        );
        assert_eq!(VoteType::from("my_custom_vote").as_str(), "my_custom_vote");
        assert_eq!(VoteType::MapRestart.to_string(), "map_restart");
    }

    #[test]
    fn every_known_vote_round_trips() {
        VoteType::MEMBERS.iter().for_each(|(_, value)| {
            assert_eq!(VoteType::from(*value).as_str(), *value);
            assert!(!matches!(VoteType::from(*value), VoteType::Other(_)));
        });
    }

    #[test]
    fn closed_enums_round_trip_and_reject_unknown_values() {
        StatsType::MEMBERS.iter().for_each(|(_, value)| {
            assert_eq!(
                StatsType::from_name(value).map(StatsType::as_str),
                Some(*value)
            );
        });
        assert_eq!(StatsType::from_name("PLAYER_NOTHING"), None);
        assert_eq!(ChatChannel::from_name("tell"), Some(ChatChannel::Tell));
        assert_eq!(ChatChannel::from_name("whisper"), None);
    }

    #[rstest]
    #[case(0, EntityType::General(0))]
    #[case(1, EntityType::Player)]
    #[case(2, EntityType::Item)]
    #[case(12, EntityType::Team)]
    #[case(13, EntityType::Events)]
    #[case(40, EntityType::Events)]
    #[case(-1, EntityType::General(-1))]
    fn entity_types_follow_the_numbers_of_the_game(
        #[case] value: i32,
        #[case] expected: EntityType,
    ) {
        assert_eq!(EntityType::from(value), expected);
    }
}
