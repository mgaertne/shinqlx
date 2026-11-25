//! The game types of Quake Live, shared by the Python layer and the map scan.

/// The game types of Quake Live; a type that this does not know keeps the number of the engine in `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GameTypes {
    FreeForAll,
    Duel,
    Race,
    TeamDeathmatch,
    ClanArena,
    CaptureTheFlag,
    OneFlag,
    Harvester,
    FreezeTag,
    Domination,
    AttackAndDefend,
    RedRover,
    Unknown(i32),
}

impl GameTypes {
    /// Every game type that is known, in the order of the game.
    pub(crate) const ALL: [Self; 12] = [
        Self::FreeForAll,
        Self::Duel,
        Self::Race,
        Self::TeamDeathmatch,
        Self::ClanArena,
        Self::CaptureTheFlag,
        Self::OneFlag,
        Self::Harvester,
        Self::FreezeTag,
        Self::Domination,
        Self::AttackAndDefend,
        Self::RedRover,
    ];

    /// The number of the engine for this game type.
    pub(crate) const fn id(self) -> i32 {
        match self {
            Self::FreeForAll => 0,
            Self::Duel => 1,
            Self::Race => 2,
            Self::TeamDeathmatch => 3,
            Self::ClanArena => 4,
            Self::CaptureTheFlag => 5,
            Self::OneFlag => 6,
            Self::Harvester => 8,
            Self::FreezeTag => 9,
            Self::Domination => 10,
            Self::AttackAndDefend => 11,
            Self::RedRover => 12,
            Self::Unknown(id) => id,
        }
    }

    pub(crate) const fn type_long(&self) -> &'static str {
        match self {
            GameTypes::FreeForAll => "Free for All",
            GameTypes::Duel => "Duel",
            GameTypes::Race => "Race",
            GameTypes::TeamDeathmatch => "Team Deathmatch",
            GameTypes::ClanArena => "Clan Arena",
            GameTypes::CaptureTheFlag => "Capture the Flag",
            GameTypes::OneFlag => "One Flag",
            GameTypes::Harvester => "Harvester",
            GameTypes::FreezeTag => "Freeze Tag",
            GameTypes::Domination => "Domination",
            GameTypes::AttackAndDefend => "Attack and Defend",
            GameTypes::RedRover => "Red Rover",
            _ => "unknown",
        }
    }

    pub(crate) const fn type_short(&self) -> &'static str {
        match self {
            GameTypes::FreeForAll => "ffa",
            GameTypes::Duel => "duel",
            GameTypes::Race => "race",
            GameTypes::TeamDeathmatch => "tdm",
            GameTypes::ClanArena => "ca",
            GameTypes::CaptureTheFlag => "ctf",
            GameTypes::OneFlag => "1f",
            GameTypes::Harvester => "har",
            GameTypes::FreezeTag => "ft",
            GameTypes::Domination => "dom",
            GameTypes::AttackAndDefend => "ad",
            GameTypes::RedRover => "rr",
            _ => "N/A",
        }
    }
}

impl From<i32> for GameTypes {
    fn from(value: i32) -> Self {
        match value {
            0 => GameTypes::FreeForAll,
            1 => GameTypes::Duel,
            2 => GameTypes::Race,
            3 => GameTypes::TeamDeathmatch,
            4 => GameTypes::ClanArena,
            5 => GameTypes::CaptureTheFlag,
            6 => GameTypes::OneFlag,
            8 => GameTypes::Harvester,
            9 => GameTypes::FreezeTag,
            10 => GameTypes::Domination,
            11 => GameTypes::AttackAndDefend,
            12 => GameTypes::RedRover,
            unknown => GameTypes::Unknown(unknown),
        }
    }
}
