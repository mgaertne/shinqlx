//! Reads the raw bits of the states as what they mean in the game.

use super::{
    fields::{entity_field, player_field},
    state::{ARRAY_LENGTH, EntityState, PlayerState},
};

const STAT_HEALTH: usize = 0;
const STAT_WEAPONS: usize = 3;
const STAT_ARMOR: usize = 4;
/// Entity types from here on are one-off events, numbered by what they exceed it.
const FIRST_EVENT_TYPE: u32 = 13;
/// The two bits above the number of an event only tell repetitions apart.
const EVENT_NUMBER_MASK: u32 = 0xff;
/// What the game uses for trajectories that name no gravity of their own.
const DEFAULT_GRAVITY: i32 = 800;

macro_rules! numbered {
    ($(#[$meta:meta])* $name:ident { $($(#[$variant_meta:meta])* $variant:ident = $number:literal,)+ }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$variant_meta])* $variant,)+
        }

        impl $name {
            /// The variant the game gives this number.
            pub fn from_number(number: u32) -> Option<Self> {
                match number {
                    $($number => Some(Self::$variant),)+
                    _ => None,
                }
            }

            /// The number the game gives the variant.
            pub fn number(self) -> u32 {
                match self {
                    $(Self::$variant => $number,)+
                }
            }
        }
    };
}

numbered! {
    /// A weapon.
    #[allow(missing_docs, reason = "the names are the documentation")]
    Weapon {
        Gauntlet = 1,
        Machinegun = 2,
        Shotgun = 3,
        GrenadeLauncher = 4,
        RocketLauncher = 5,
        LightningGun = 6,
        Railgun = 7,
        Plasmagun = 8,
        Bfg = 9,
        GrapplingHook = 10,
        Nailgun = 11,
        ProximityLauncher = 12,
        Chaingun = 13,
        HeavyMachinegun = 14,
        Hands = 15,
    }
}

numbered! {
    /// How a player died.
    #[allow(missing_docs, reason = "the names are the documentation")]
    MeansOfDeath {
        Unknown = 0,
        Shotgun = 1,
        Gauntlet = 2,
        Machinegun = 3,
        Grenade = 4,
        GrenadeSplash = 5,
        Rocket = 6,
        RocketSplash = 7,
        Plasma = 8,
        PlasmaSplash = 9,
        Railgun = 10,
        Lightning = 11,
        Bfg = 12,
        BfgSplash = 13,
        Water = 14,
        Slime = 15,
        Lava = 16,
        Crush = 17,
        Telefrag = 18,
        Falling = 19,
        Suicide = 20,
        TargetLaser = 21,
        TriggerHurt = 22,
        Nail = 23,
        Chaingun = 24,
        ProximityMine = 25,
        Kamikaze = 26,
        Juiced = 27,
        Grapple = 28,
        SwitchTeams = 29,
        Thaw = 30,
        LightningDischarge = 31,
        HeavyMachinegun = 32,
        RailgunHeadshot = 33,
    }
}

numbered! {
    /// How a position or angle changes over time.
    TrajectoryKind {
        /// It does not change.
        Stationary = 0,
        /// It is only known frame by frame.
        Interpolate = 1,
        /// It changes at a constant speed.
        Linear = 2,
        /// Like `Linear`, for a limited time.
        LinearStop = 3,
        /// It swings around its base.
        Sine = 4,
        /// It flies and falls.
        Gravity = 5,
    }
}

/// What an entity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EntityKind {
    /// Anything without a type of its own.
    General,
    /// A player.
    Player,
    /// An item to pick up.
    Item,
    /// A projectile.
    Missile,
    /// A moving part of the map.
    Mover,
    /// A beam.
    Beam,
    /// A portal.
    Portal,
    /// A source of sound.
    Speaker,
    /// A jump pad.
    PushTrigger,
    /// A teleporter.
    TeleportTrigger,
    /// Nothing to see.
    Invisible,
    /// A hooked grappling hook.
    Grapple,
    /// A team object.
    Team,
    /// Something that happens once, with the number of the event.
    Event(u32),
}

/// The change of a position or an angle over time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trajectory {
    /// How it changes, `None` for a kind that is not known.
    pub kind: Option<TrajectoryKind>,
    /// The server time it starts at.
    pub start: i32,
    /// How long it lasts or how long one swing takes, in milliseconds.
    pub duration: i32,
    /// The value at the start.
    pub base: [f32; 3],
    /// The change per second, or the width of a swing.
    pub delta: [f32; 3],
    /// The downward acceleration of a falling object, `0` for the default of the game.
    pub gravity: i32,
}

impl Trajectory {
    /// The value at a server time.
    pub fn at(&self, time: i32) -> [f32; 3] {
        let seconds = |until: i32| until.saturating_sub(self.start) as f32 / 1000.0;
        let moved =
            |factor: f32| core::array::from_fn(|axis| self.base[axis] + self.delta[axis] * factor);
        match self.kind {
            Some(TrajectoryKind::Linear) => moved(seconds(time)),
            Some(TrajectoryKind::LinearStop) => {
                let end = self.start.saturating_add(self.duration);
                moved(seconds(time.min(end)).max(0.0))
            }
            Some(TrajectoryKind::Sine) if self.duration != 0 => {
                let swings = time.saturating_sub(self.start) as f32 / self.duration as f32;
                moved((swings * core::f32::consts::TAU).sin())
            }
            Some(TrajectoryKind::Gravity) => {
                let elapsed = seconds(time);
                let gravity = match self.gravity {
                    0 => DEFAULT_GRAVITY,
                    gravity => gravity,
                };
                let [x, y, z] = moved(elapsed);
                [x, y, z - 0.5 * gravity as f32 * elapsed * elapsed]
            }
            _ => self.base,
        }
    }
}

/// The names of the fields of a trajectory of an entity.
struct TrajectoryFields {
    kind: usize,
    start: usize,
    duration: usize,
    base: [usize; 3],
    delta: [usize; 3],
    gravity: usize,
}

macro_rules! trajectory_fields {
    ($prefix:literal) => {
        const {
            TrajectoryFields {
                kind: entity_field(concat!($prefix, ".trType")),
                start: entity_field(concat!($prefix, ".trTime")),
                duration: entity_field(concat!($prefix, ".trDuration")),
                base: [
                    entity_field(concat!($prefix, ".trBase[0]")),
                    entity_field(concat!($prefix, ".trBase[1]")),
                    entity_field(concat!($prefix, ".trBase[2]")),
                ],
                delta: [
                    entity_field(concat!($prefix, ".trDelta[0]")),
                    entity_field(concat!($prefix, ".trDelta[1]")),
                    entity_field(concat!($prefix, ".trDelta[2]")),
                ],
                gravity: entity_field(concat!($prefix, ".gravity")),
            }
        }
    };
}

macro_rules! entity_value {
    ($(#[$meta:meta])* $name:ident, $field:literal) => {
        $(#[$meta])*
        pub fn $name(&self) -> u32 {
            self.fields[const { entity_field($field) }]
        }
    };
}

impl EntityState {
    entity_value!(
        /// The client the entity belongs to.
        client_number,
        "clientNum"
    );

    entity_value!(
        /// The detail of an event, like the way a player died.
        event_parameter,
        "eventParm"
    );

    entity_value!(
        /// The first other entity involved.
        other_entity,
        "otherEntityNum"
    );

    entity_value!(
        /// The second other entity involved.
        second_other_entity,
        "otherEntityNum2"
    );

    entity_value!(
        /// The flags of the game.
        flags,
        "eFlags"
    );

    entity_value!(
        /// The model, which for an item tells which one it is.
        model_index,
        "modelindex"
    );

    /// What the entity is.
    pub fn kind(&self) -> EntityKind {
        match self.fields[const { entity_field("eType") }] {
            0 => EntityKind::General,
            1 => EntityKind::Player,
            2 => EntityKind::Item,
            3 => EntityKind::Missile,
            4 => EntityKind::Mover,
            5 => EntityKind::Beam,
            6 => EntityKind::Portal,
            7 => EntityKind::Speaker,
            8 => EntityKind::PushTrigger,
            9 => EntityKind::TeleportTrigger,
            10 => EntityKind::Invisible,
            11 => EntityKind::Grapple,
            12 => EntityKind::Team,
            number => EntityKind::Event(number - FIRST_EVENT_TYPE),
        }
    }

    /// How the position changes.
    pub fn trajectory(&self) -> Trajectory {
        self.read_trajectory(&trajectory_fields!("pos"))
    }

    /// How the angles (pitch, yaw, roll) change.
    pub fn angle_trajectory(&self) -> Trajectory {
        self.read_trajectory(&trajectory_fields!("apos"))
    }

    /// The position at a server time.
    pub fn position_at(&self, time: i32) -> [f32; 3] {
        self.trajectory().at(time)
    }

    /// The fixed point of entities that do not move, like the place an event happens at.
    pub fn origin(&self) -> [f32; 3] {
        self.floats([
            const { entity_field("origin[0]") },
            const { entity_field("origin[1]") },
            const { entity_field("origin[2]") },
        ])
    }

    /// A second point, like the end of a rail trail.
    pub fn second_origin(&self) -> [f32; 3] {
        self.floats([
            const { entity_field("origin2[0]") },
            const { entity_field("origin2[1]") },
            const { entity_field("origin2[2]") },
        ])
    }

    /// The weapon held or fired.
    pub fn weapon(&self) -> Option<Weapon> {
        Weapon::from_number(self.fields[const { entity_field("weapon") }])
    }

    /// The event attached to an entity that is not an event itself, `0` without one.
    pub fn attached_event(&self) -> u32 {
        self.fields[const { entity_field("event") }] & EVENT_NUMBER_MASK
    }

    fn read_trajectory(&self, fields: &TrajectoryFields) -> Trajectory {
        Trajectory {
            kind: TrajectoryKind::from_number(self.fields[fields.kind]),
            start: self.fields[fields.start].cast_signed(),
            duration: self.fields[fields.duration].cast_signed(),
            base: self.floats(fields.base),
            delta: self.floats(fields.delta),
            gravity: self.fields[fields.gravity].cast_signed(),
        }
    }

    fn floats(&self, indices: [usize; 3]) -> [f32; 3] {
        indices.map(|index| f32::from_bits(self.fields[index]))
    }
}

impl PlayerState {
    /// The player whose view this is, who differs from the owner of the state while spectating.
    pub fn client_number(&self) -> i32 {
        self.fields[const { player_field("clientNum") }].cast_signed()
    }

    /// The position.
    pub fn origin(&self) -> [f32; 3] {
        self.floats([
            const { player_field("origin[0]") },
            const { player_field("origin[1]") },
            const { player_field("origin[2]") },
        ])
    }

    /// The speed in units per second.
    pub fn velocity(&self) -> [f32; 3] {
        self.floats([
            const { player_field("velocity[0]") },
            const { player_field("velocity[1]") },
            const { player_field("velocity[2]") },
        ])
    }

    /// Pitch, yaw and roll of the view in degrees.
    pub fn view_angles(&self) -> [f32; 3] {
        self.floats([
            const { player_field("viewangles[0]") },
            const { player_field("viewangles[1]") },
            const { player_field("viewangles[2]") },
        ])
    }

    /// The height of the eyes above the position.
    pub fn view_height(&self) -> i32 {
        self.fields[const { player_field("viewheight") }].cast_signed()
    }

    /// The health, `0` or less for the dead.
    pub fn health(&self) -> i16 {
        self.stats[STAT_HEALTH]
    }

    /// The armor.
    pub fn armor(&self) -> i16 {
        self.stats[STAT_ARMOR]
    }

    /// The weapon held.
    pub fn weapon(&self) -> Option<Weapon> {
        Weapon::from_number(self.fields[const { player_field("weapon") }])
    }

    /// Whether the player carries a weapon.
    pub fn has_weapon(&self, weapon: Weapon) -> bool {
        self.stats[STAT_WEAPONS].cast_unsigned() & (1 << weapon.number()) != 0
    }

    /// The ammunition of a weapon, negative if it needs none.
    pub fn ammo_of(&self, weapon: Weapon) -> Option<i16> {
        let index = usize::try_from(weapon.number()).ok()?;
        (index < ARRAY_LENGTH).then(|| self.ammo[index])
    }

    fn floats(&self, indices: [usize; 3]) -> [f32; 3] {
        indices.map(|index| f32::from_bits(self.fields[index]))
    }
}

#[cfg(test)]
mod view_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::*;

    fn trajectory(kind: TrajectoryKind) -> Trajectory {
        Trajectory {
            kind: Some(kind),
            start: 1000,
            duration: 2000,
            base: [1.0, 2.0, 3.0],
            delta: [10.0, 0.0, 400.0],
            gravity: 0,
        }
    }

    #[rstest]
    #[case::stationary(TrajectoryKind::Stationary, 2000, [1.0, 2.0, 3.0])]
    #[case::linear(TrajectoryKind::Linear, 2000, [11.0, 2.0, 403.0])]
    #[case::stopped(TrajectoryKind::LinearStop, 9000, [21.0, 2.0, 803.0])]
    #[case::not_started(TrajectoryKind::LinearStop, 0, [1.0, 2.0, 3.0])]
    #[case::falling(TrajectoryKind::Gravity, 2000, [11.0, 2.0, 3.0])]
    fn trajectories_give_the_value_at_a_time(
        #[case] kind: TrajectoryKind,
        #[case] time: i32,
        #[case] expected: [f32; 3],
    ) {
        assert_eq!(trajectory(kind).at(time), expected);
    }

    #[test]
    fn an_entity_is_read_by_the_meaning_of_its_fields() {
        let mut entity = EntityState::default();
        let mut set = |name, value| entity.fields[entity_field(name)] = value;
        set("eType", 13 + 58);
        set("weapon", 5);
        set("event", 0x314);
        set("pos.trType", 2);
        set("pos.trDelta[0]", 8.0f32.to_bits());
        set("otherEntityNum2", 4);

        assert_eq!(entity.kind(), EntityKind::Event(58));
        assert_eq!(entity.weapon(), Some(Weapon::RocketLauncher));
        assert_eq!(entity.attached_event(), 20);
        assert_eq!(entity.position_at(500), [4.0, 0.0, 0.0]);
        assert_eq!(entity.second_other_entity(), 4);
    }

    #[test]
    fn a_player_is_read_by_the_meaning_of_its_fields() {
        let mut player = PlayerState::default();
        player.fields[player_field("origin[2]")] = 64.0f32.to_bits();
        player.fields[player_field("weapon")] = 7;
        player.stats[STAT_HEALTH] = 90;
        player.stats[STAT_ARMOR] = 50;
        player.stats[STAT_WEAPONS] = 1 << 7;
        player.ammo[7] = 12;

        assert_eq!(player.origin(), [0.0, 0.0, 64.0]);
        assert_eq!((player.health(), player.armor()), (90, 50));
        assert_eq!(player.weapon(), Some(Weapon::Railgun));
        assert!(player.has_weapon(Weapon::Railgun));
        assert!(!player.has_weapon(Weapon::Gauntlet));
        assert_eq!(player.ammo_of(Weapon::Railgun), Some(12));
    }

    #[rstest]
    #[case(0)]
    #[case(16)]
    fn numbers_without_a_weapon_are_none(#[case] number: u32) {
        assert_eq!(Weapon::from_number(number), None);
    }

    #[test]
    fn the_numbers_of_the_variants_round_trip() {
        for number in 0..40 {
            if let Some(means) = MeansOfDeath::from_number(number) {
                assert_eq!(means.number(), number);
            }
        }
    }
}
