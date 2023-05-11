#![allow(non_camel_case_types)]
#![allow(non_camel_case_types)]

use crate::hooks::shinqlx_set_configstring;
use crate::quake_common::clientConnected_t::CON_DISCONNECTED;
use crate::quake_common::entityType_t::ET_ITEM;
use crate::quake_common::entity_event_t::EV_ITEM_RESPAWN;
use crate::quake_common::itemType_t::IT_WEAPON;
use crate::quake_common::meansOfDeath_t::MOD_KAMIKAZE;
use crate::quake_common::persistantFields_t::PERS_ROUND_SCORE;
use crate::quake_common::pmtype_t::PM_NORMAL;
use crate::quake_common::powerup_t::{
    PW_BATTLESUIT, PW_HASTE, PW_INVIS, PW_INVULNERABILITY, PW_QUAD, PW_REGEN,
};
use crate::quake_common::privileges_t::{PRIV_ADMIN, PRIV_BANNED, PRIV_MOD, PRIV_NONE, PRIV_ROOT};
use crate::quake_common::statIndex_t::{
    STAT_ARMOR, STAT_CUR_FLIGHT_FUEL, STAT_FLIGHT_REFUEL, STAT_FLIGHT_THRUST, STAT_HOLDABLE_ITEM,
    STAT_MAX_FLIGHT_FUEL, STAT_WEAPONS,
};
use crate::quake_common::team_t::TEAM_SPECTATOR;
use crate::quake_common::voteState_t::{VOTE_NO, VOTE_PENDING, VOTE_YES};
use crate::SV_MAXCLIENTS;
use lazy_static::lazy_static;
use std::borrow::Cow;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::ffi::{c_char, c_float, c_int, c_uchar, c_uint, c_ushort, c_void, CStr, CString};
use std::mem;
use std::ops::{BitAnd, BitAndAssign, BitOrAssign, Not};

#[allow(dead_code)]
pub(crate) const DEBUG_PRINT_PREFIX: &str = "[shinqlx]";

pub(crate) const SV_TAGS_PREFIX: &str = "shinqlx";

#[allow(dead_code)]
pub const CS_SCORES1: u32 = 6;
#[allow(dead_code)]
pub const CS_SCORES2: u32 = 7;
#[allow(dead_code)]
pub const CS_VOTE_TIME: u32 = 8;
#[allow(dead_code)]
pub const CS_VOTE_STRING: u32 = 9;
#[allow(dead_code)]
pub const CS_VOTE_YES: u32 = 10;
#[allow(dead_code)]
pub const CS_VOTE_NO: u32 = 11;
#[allow(dead_code)]
pub const CS_ITEMS: u32 = 15;

#[allow(dead_code)]
pub const MAX_CLIENTS: u32 = 64;
#[allow(dead_code)]
pub const MAX_CHALLENGES: u32 = 1024;
#[allow(dead_code)]
pub const MAX_MSGLEN: u32 = 16384;
#[allow(dead_code)]
pub const MAX_PS_EVENTS: u32 = 2;
#[allow(dead_code)]
pub const MAX_MAP_AREA_BYTES: u32 = 32; // bit vector of area visibility
#[allow(dead_code)]
pub const MAX_INFO_STRING: u32 = 1024;
#[allow(dead_code)]
pub const MAX_RELIABLE_COMMANDS: u32 = 64; // max string commands buffered for restransmit
#[allow(dead_code)]
pub const MAX_STRING_CHARS: u32 = 1024; // max length of a string passed to Cmd_TokenizeString
#[allow(dead_code)]
pub const MAX_NAME_LENGTH: u32 = 32; // max length of a client name
#[allow(dead_code)]
pub const MAX_QPATH: u32 = 64; // max length of a quake game pathname
#[allow(dead_code)]
pub const MAX_DOWNLOAD_WINDOW: u32 = 8; // max of eight download frames
#[allow(dead_code)]
pub const MAX_NETNAME: u32 = 36;
pub const PACKET_BACKUP: u32 = 32; // number of old messages that must be kept on client and
                                   // server for delta comrpession and ping estimation
#[allow(dead_code)]
pub(crate) const PACKET_MASK: u32 = PACKET_BACKUP - 1;
#[allow(dead_code)]
pub const MAX_ENT_CLUSTERS: u32 = 16;
#[allow(dead_code)]
pub const MAX_MODELS: u32 = 256; // these are sent over the net as 8 bits
pub const MAX_CONFIGSTRINGS: u32 = 1024;
#[allow(dead_code)]
pub const GENTITYNUM_BITS: u32 = 10; // don't need to send any more
#[allow(dead_code)]
pub const MAX_GENTITIES: u32 = 1 << GENTITYNUM_BITS;
#[allow(dead_code)]
pub const MAX_ITEM_MODELS: u32 = 4;
#[allow(dead_code)]
pub const MAX_SPAWN_VARS: u32 = 64;
#[allow(dead_code)]
pub const MAX_SPAWN_VARS_CHARS: u32 = 4096;
#[allow(dead_code)]
pub const BODY_QUEUE_SIZE: u32 = 8;

// bit field limits
#[allow(dead_code)]
pub const MAX_STATS: u32 = 16;
#[allow(dead_code)]
pub const MAX_PERSISTANT: u32 = 16;
#[allow(dead_code)]
pub const MAX_POWERUPS: u32 = 16;
#[allow(dead_code)]
pub const MAX_WEAPONS: u32 = 16;

// Button flags
#[allow(dead_code)]
pub const BUTTON_ATTACK: u32 = 1;
#[allow(dead_code)]
pub const BUTTON_TALK: u32 = 2; // displkays talk balloon and disables actions
#[allow(dead_code)]
pub const BUTTON_USE_HOLDABLE: u32 = 4; // Mino +button2
#[allow(dead_code)]
pub const BUTTON_GESTURE: u32 = 8; // Mino: +button3
#[allow(dead_code)]
pub const BUTTON_WALKING: u32 = 16;
// Block of unused button flags, or at least flags I couldn't trigger.
// Q3 used them for bot commands, so probably unused in QL.
#[allow(dead_code)]
pub const BUTTON_UNUSED1: u32 = 32;
#[allow(dead_code)]
pub const BUTTON_UNUSED2: u32 = 64;
#[allow(dead_code)]
pub const BUTTON_UNUSED3: u32 = 128;
#[allow(dead_code)]
pub const BUTTON_UNUSED4: u32 = 256;
#[allow(dead_code)]
pub const BUTTON_UNUSED5: u32 = 512;
#[allow(dead_code)]
pub const BUTTON_UNUSED6: u32 = 1024;
#[allow(dead_code)]
pub const BUTTON_UPMOVE: u32 = 2048; // Mino: Not in Q3. I'm guessing it's for cg_autohop.
#[allow(dead_code)]
pub const BUTTON_ANY: u32 = 4096; // any key whatsoever
#[allow(dead_code)]
pub const BUTTON_IS_ACTIVE: u32 = 65536; // Mino: No idea what it is, but it goes off after a while of being
                                         //       AFK, then goes on after being active for a while.

// eflags
#[allow(dead_code)]
pub const EF_DEAD: u32 = 1; // don't draw a foe marker over players with EF_DEAD
#[allow(dead_code)]
pub const EF_TICKING: u32 = 2; // used to make players play the prox mine ticking sound
#[allow(dead_code)]
pub const EF_TELEPORT_BIT: u32 = 4; // toggled every time the origin abruptly changes
#[allow(dead_code)]
pub const EF_AWARD_EXCELLENT: u32 = 8; // draw an excellent sprite
#[allow(dead_code)]
pub const EF_PLAYER_EVENT: u32 = 16;
#[allow(dead_code)]
pub const EF_BOUNCE: u32 = 16; // for missiles
#[allow(dead_code)]
pub const EF_BOUNCE_HALF: u32 = 32; // for missiles
#[allow(dead_code)]
pub const EF_AWARD_GAUNTLET: u32 = 64; // draw a gauntlet sprite
#[allow(dead_code)]
pub const EF_NODRAW: u32 = 128; // may have an event, but no model (unspawned items)
#[allow(dead_code)]
pub const EF_FIRING: u32 = 256; // for lightning gun
pub const EF_KAMIKAZE: u32 = 512;
#[allow(dead_code)]
pub const EF_MOVER_STOP: u32 = 1024; // will push otherwise
#[allow(dead_code)]
pub const EF_AWARD_CAP: u32 = 2048; // draw the capture sprite
#[allow(dead_code)]
pub const EF_TALK: u32 = 4096; // draw a talk balloon
#[allow(dead_code)]
pub const EF_CONNECTION: u32 = 8192; // draw a connection trouble sprite
#[allow(dead_code)]
pub const EF_VOTED: u32 = 16384; // already cast a vote
#[allow(dead_code)]
pub const EF_AWARD_IMPRESSIVE: u32 = 32768; // draw an impressive sprite
#[allow(dead_code)]
pub const EF_AWARD_DEFEND: u32 = 65536; // draw a defend sprite
#[allow(dead_code)]
pub const EF_AWARD_ASSIST: u32 = 131072; // draw a assist sprite
#[allow(dead_code)]
pub const EF_AWARD_DENIED: u32 = 262144; // denied
#[allow(dead_code)]
pub const EF_TEAMVOTED: u32 = 524288; // already cast a team vote

#[allow(dead_code)]
pub const FL_DROPPED_ITEM: u32 = 4096;

#[allow(dead_code)]
pub const DAMAGE_NO_PROTECTION: u32 = 8;

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum qboolean {
    qfalse = 0,
    qtrue = 1,
}

impl From<qboolean> for c_int {
    fn from(value: qboolean) -> Self {
        match value {
            qboolean::qtrue => 1,
            _ => 0,
        }
    }
}

impl From<qboolean> for bool {
    fn from(value: qboolean) -> Self {
        matches!(value, qboolean::qtrue)
    }
}

impl From<bool> for qboolean {
    fn from(value: bool) -> Self {
        match value {
            true => qboolean::qtrue,
            _ => qboolean::qfalse,
        }
    }
}

impl Not for qboolean {
    type Output = qboolean;

    fn not(self) -> Self::Output {
        match self {
            qboolean::qtrue => qboolean::qfalse,
            _ => qboolean::qtrue,
        }
    }
}

pub type byte = c_uchar;
pub type gentity_t = gentity_s;
#[allow(dead_code)]
pub type gclient_t = gclient_s;
pub type vec_t = f32;
type vec3_t = [c_float; 3];
pub type fileHandle_t = c_int;

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum privileges_t {
    PRIV_BANNED = 4294967295,
    PRIV_NONE = 0,
    PRIV_MOD = 1,
    PRIV_ADMIN = 2,
    PRIV_ROOT = 3,
}

impl From<i32> for privileges_t {
    fn from(value: i32) -> Self {
        match value {
            -1 => PRIV_BANNED,
            0x1 => PRIV_MOD,
            0x2 => PRIV_ADMIN,
            0x3 => PRIV_ROOT,
            _ => PRIV_NONE,
        }
    }
}

#[repr(u32)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum voteState_t {
    VOTE_NONE = 0,
    VOTE_PENDING = 1,
    VOTE_YES = 2,
    VOTE_NO = 3,
    VOTE_FORCE_PASS = 4,
    VOTE_FORCE_FAIL = 5,
    VOTE_EXPIRED = 6,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum clientState_t {
    CS_FREE = 0,   // can be reused for a new connection
    CS_ZOMBIE = 1, // client has been disconnected, but don't reuse
    // connection for a couple seconds
    CS_CONNECTED = 2, // has been assigned to a client_t, but no gamestate yet
    CS_PRIMED = 3,    // gamestate has been sent, but client hasn't sent a usercmd
    CS_ACTIVE = 4,    // client is fully in game
}

#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum roundStateState_t {
    PREGAME = 0,
    ROUND_WARMUP = 1,
    ROUND_SHUFFLE = 2,
    ROUND_BEGUN = 3,
    ROUND_OVER = 4,
    POSTGAME = 5,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum statIndex_t {
    STAT_HEALTH = 0,
    STAT_HOLDABLE_ITEM = 1,
    STAT_RUNE = 2,
    STAT_WEAPONS = 3,
    STAT_ARMOR = 4,
    STAT_BSKILL = 5,
    STAT_CLIENTS_READY = 6,
    STAT_MAX_HEALTH = 7,
    STAT_SPINUP = 8,
    STAT_FLIGHT_THRUST = 9,
    STAT_MAX_FLIGHT_FUEL = 10,
    STAT_CUR_FLIGHT_FUEL = 11,
    STAT_FLIGHT_REFUEL = 12,
    STAT_QUADKILLS = 13,
    STAT_ARMORTYPE = 14,
    STAT_KEY = 15,
    STAT_OTHER_HEALTH = 16,
    STAT_OTHER_ARMOR = 17,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum gameExport_t {
    GAME_INIT = 0, // ( int levelTime, int randomSeed, int restart );
    // init and shutdown will be called every single level
    // The game should call G_GET_ENTITY_TOKEN to parse through all the
    // entity configuration text and spawn gentities.
    GAME_SHUTDOWN = 1,       // (void);
    GAME_CLIENT_CONNECT = 2, // ( int clientNum, qboolean firstTime, qboolean isBot );
    // return NULL if the client is allowed to connect, otherwise return
    // a text string with the reason for denial
    GAME_CLIENT_BEGIN = 3,            // ( int clientNum );
    GAME_CLIENT_USERINFO_CHANGED = 4, // ( int clientNum );
    GAME_CLIENT_DISCONNECT = 5,       // ( int clientNum );
    GAME_CLIENT_COMMAND = 6,          // ( int clientNum );
    GAME_CLIENT_THINK = 7,            // ( int clientNum );
    GAME_RUN_FRAME = 8,               // ( int clientNum );
    GAME_CONSOLE_COMMAND = 9,         // ( void );
    // ConsoleCommand will be called when a command has been issued
    // that is not recognized as a builtin function.
    // The game can issue trap_argc() / trap_argv() commands to get the command
    // and parameters.  Return qfalse if the game doesn't recognize it as a command.
    BOTAI_START_FRAME = 10, // ( int time );
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum pmtype_t {
    PM_NORMAL = 0,
    PM_NOCLIP = 1,
    PM_SPECTATOR = 2,
    PM_DEAD = 3,
    PM_FREEZE = 4,
    PM_INTERMISSION = 5,
    PM_TUTORIAL = 6,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum entity_event_t {
    EV_NONE = 0,
    EV_FOOTSTEP = 1,
    EV_FOOTSTEP_METAL = 2,
    EV_FOOTSPLASH = 3,
    EV_FOOTWADE = 4,
    EV_SWIM = 5,
    EV_FALL_SHORT = 6,
    EV_FALL_MEDIUM = 7,
    EV_FALL_FAR = 8,
    EV_JUMP_PAD = 9,
    EV_JUMP = 10,
    EV_WATER_TOUCH = 11,
    EV_WATER_LEAVE = 12,
    EV_WATER_UNDER = 13,
    EV_WATER_CLEAR = 14,
    EV_ITEM_PICKUP = 15,
    EV_GLOBAL_ITEM_PICKUP = 16,
    EV_NOAMMO = 17,
    EV_CHANGE_WEAPON = 18,
    EV_DROP_WEAPON = 19,
    EV_FIRE_WEAPON = 20,
    EV_USE_ITEM0 = 21,
    EV_USE_ITEM1 = 22,
    EV_USE_ITEM2 = 23,
    EV_USE_ITEM3 = 24,
    EV_USE_ITEM4 = 25,
    EV_USE_ITEM5 = 26,
    EV_USE_ITEM6 = 27,
    EV_USE_ITEM7 = 28,
    EV_USE_ITEM8 = 29,
    EV_USE_ITEM9 = 30,
    EV_USE_ITEM10 = 31,
    EV_USE_ITEM11 = 32,
    EV_USE_ITEM12 = 33,
    EV_USE_ITEM13 = 34,
    EV_USE_ITEM14 = 35,
    EV_USE_ITEM15 = 36,
    EV_ITEM_RESPAWN = 37,
    EV_ITEM_POP = 38,
    EV_PLAYER_TELEPORT_IN = 39,
    EV_PLAYER_TELEPORT_OUT = 40,
    EV_GRENADE_BOUNCE = 41,
    EV_GENERAL_SOUND = 42,
    EV_GLOBAL_SOUND = 43,
    EV_GLOBAL_TEAM_SOUND = 44,
    EV_BULLET_HIT_FLESH = 45,
    EV_BULLET_HIT_WALL = 46,
    EV_MISSILE_HIT = 47,
    EV_MISSILE_MISS = 48,
    EV_MISSILE_MISS_METAL = 49,
    EV_RAILTRAIL = 50,
    EV_SHOTGUN = 51,
    EV_BULLET = 52,
    EV_PAIN = 53,
    EV_DEATH1 = 54,
    EV_DEATH2 = 55,
    EV_DEATH3 = 56,
    EV_DROWN = 57,
    EV_OBITUARY = 58,
    EV_POWERUP_QUAD = 59,
    EV_POWERUP_BATTLESUIT = 60,
    EV_POWERUP_REGEN = 61,
    EV_POWERUP_ARMORREGEN = 62,
    EV_GIB_PLAYER = 63,
    EV_SCOREPLUM = 64,
    EV_PROXIMITY_MINE_STICK = 65,
    EV_PROXIMITY_MINE_TRIGGER = 66,
    EV_KAMIKAZE = 67,
    EV_OBELISKEXPLODE = 68,
    EV_OBELISKPAIN = 69,
    EV_INVUL_IMPACT = 70,
    EV_JUICED = 71,
    EV_LIGHTNINGBOLT = 72,
    EV_DEBUG_LINE = 73,
    EV_TAUNT = 74,
    EV_TAUNT_YES = 75,
    EV_TAUNT_NO = 76,
    EV_TAUNT_FOLLOWME = 77,
    EV_TAUNT_GETFLAG = 78,
    EV_TAUNT_GUARDBASE = 79,
    EV_TAUNT_PATROL = 80,
    EV_FOOTSTEP_SNOW = 81,
    EV_FOOTSTEP_WOOD = 82,
    EV_ITEM_PICKUP_SPEC = 83,
    EV_OVERTIME = 84,
    EV_GAMEOVER = 85,
    EV_MISSILE_MISS_DMGTHROUGH = 86,
    EV_THAW_PLAYER = 87,
    EV_THAW_TICK = 88,
    EV_SHOTGUN_KILL = 89,
    EV_POI = 90,
    EV_DEBUG_HITBOX = 91,
    EV_LIGHTNING_DISCHARGE = 92,
    EV_RACE_START = 93,
    EV_RACE_CHECKPOINT = 94,
    EV_RACE_FINISH = 95,
    EV_DAMAGEPLUM = 96,
    EV_AWARD = 97,
    EV_INFECTED = 98,
    EV_NEW_HIGH_SCORE = 99,
    EV_NUM_ETYPES = 100,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum itemType_t {
    IT_BAD = 0,
    IT_WEAPON = 1,  // EFX: rotate + upscale + minlight
    IT_AMMO = 2,    // EFX: rotate
    IT_ARMOR = 3,   // EFX: rotate + minlight
    IT_HEALTH = 4,  // EFX: static external sphere + rotating internal
    IT_POWERUP = 5, // instant on, timer based
    // EFX: rotate + external ring that rotates
    IT_HOLDABLE = 6, // single use, holdable item
    // EFX: rotate + bob
    IT_PERSISTANT_POWERUP = 7,
    IT_TEAM = 8,
}

#[allow(dead_code)]
impl powerup_t {
    pub const PW_SPAWNARMOR: powerup_t = powerup_t::PW_NONE;
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum powerup_t {
    PW_NONE = 0,
    PW_REDFLAG = 1,
    PW_BLUEFLAG = 2,
    PW_NEUTRALFLAG = 3,
    PW_QUAD = 4,
    PW_BATTLESUIT = 5,
    PW_HASTE = 6,
    PW_INVIS = 7,
    PW_REGEN = 8,
    PW_FLIGHT = 9,
    PW_INVULNERABILITY = 10,
    NOTPW_SCOUT = 11,
    NOTPW_GUARD = 12,
    NOTPW_DOUBLER = 13,
    NOTPW_ARMORREGEN = 14,
    PW_FREEZE = 15,
    PW_NUM_POWERUPS = 16,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum holdable_t {
    HI_NONE = 0,
    HI_TELEPORTER = 1,
    HI_MEDKIT = 2,
    HI_KAMIKAZE = 3,
    HI_PORTAL = 4,
    HI_INVULNERABILITY = 5,
    HI_FLIGHT = 6,
    HI_NUM_HOLDABLE = 7,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum weapon_t {
    WP_NONE = 0,
    WP_GAUNTLET = 1,
    WP_MACHINEGUN = 2,
    WP_SHOTGUN = 3,
    WP_GRENADE_LAUNCHER = 4,
    WP_ROCKET_LAUNCHER = 5,
    WP_LIGHTNING = 6,
    WP_RAILGUN = 7,
    WP_PLASMAGUN = 8,
    WP_BFG = 9,
    WP_GRAPPLING_HOOK = 10,
    WP_NAILGUN = 11,
    WP_PROX_LAUNCHER = 12,
    WP_CHAINGUN = 13,
    WP_HMG = 14,
    WP_HANDS = 15,
    WP_NUM_WEAPONS = 16,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum weaponstate_t {
    WEAPON_READY = 0,
    WEAPON_RAISING = 1,
    WEAPON_DROPPING = 2,
    WEAPON_FIRING = 3,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum rune_t {
    RUNE_NONE = 0,
    RUNE_SCOUT = 1,
    RUNE_GUARD = 2,
    RUNE_DAMAGE = 3,
    RUNE_ARMORREGEN = 4,
    RUNE_MAX = 5,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum playerTeamStateState_t {
    TEAM_BEGIN = 0,  // Beginning a team game, spawn at base
    TEAM_ACTIVE = 1, // Now actively playing
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum team_t {
    TEAM_FREE = 0,
    TEAM_RED = 1,
    TEAM_BLUE = 2,
    TEAM_SPECTATOR = 3,
    TEAM_NUM_TEAMS = 4,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum meansOfDeath_t {
    MOD_UNKNOWN = 0,
    MOD_SHOTGUN = 1,
    MOD_GAUNTLET = 2,
    MOD_MACHINEGUN = 3,
    MOD_GRENADE = 4,
    MOD_GRENADE_SPLASH = 5,
    MOD_ROCKET = 6,
    MOD_ROCKET_SPLASH = 7,
    MOD_PLASMA = 8,
    MOD_PLASMA_SPLASH = 9,
    MOD_RAILGUN = 10,
    MOD_LIGHTNING = 11,
    MOD_BFG = 12,
    MOD_BFG_SPLASH = 13,
    MOD_WATER = 14,
    MOD_SLIME = 15,
    MOD_LAVA = 16,
    MOD_CRUSH = 17,
    MOD_TELEFRAG = 18,
    MOD_FALLING = 19,
    MOD_SUICIDE = 20,
    MOD_TARGET_LASER = 21,
    MOD_TRIGGER_HURT = 22,
    MOD_NAIL = 23,
    MOD_CHAINGUN = 24,
    MOD_PROXIMITY_MINE = 25,
    MOD_KAMIKAZE = 26,
    MOD_JUICED = 27,
    MOD_GRAPPLE = 28,
    MOD_SWITCH_TEAMS = 29,
    MOD_THAW = 30,
    MOD_LIGHTNING_DISCHARGE = 31,
    MOD_HMG = 32,
    MOD_RAILGUN_HEADSHOT = 33,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum spectatorState_t {
    SPECTATOR_NOT = 0,
    SPECTATOR_FREE = 1,
    SPECTATOR_FOLLOW = 2,
    SPECTATOR_SCOREBOARD = 3,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum clientConnected_t {
    CON_DISCONNECTED = 0,
    CON_CONNECTING = 1,
    CON_CONNECTED = 2,
}

// movers are things like doors, plats, buttons, etc
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum moverState_t {
    MOVER_POS1 = 0,
    MOVER_POS2 = 1,
    MOVER_1TO2 = 2,
    MOVER_2TO1 = 3,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub(crate) enum persistantFields_t {
    PERS_ROUND_SCORE = 0,
    PERS_COMBOKILL_COUNT = 1,
    PERS_RAMPAGE_COUNT = 2,
    PERS_MIDAIR_COUNT = 3,
    PERS_REVENGE_COUNT = 4,
    PERS_PERFORATED_COUNT = 5,
    PERS_HEADSHOT_COUNT = 6,
    PERS_ACCURACY_COUNT = 7,
    PERS_QUADGOD_COUNT = 8,
    PERS_FIRSTFRAG_COUNT = 9,
    PERS_PERFECT_COUNT = 10,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum cvar_flags {
    CVAR_ARCHIVE = 1,
    CVAR_USERINFO = 2,
    CVAR_SERVERINFO = 4,
    CVAR_SYSTEMINFO = 8,
    CVAR_INIT = 16,
    CVAR_LATCH = 32,
    CVAR_ROM = 64,
    CVAR_USER_CREATED = 128,
    CVAR_TEMP = 256,
    CVAR_CHEAT = 512,
    CVAR_NORESTART = 1024,
    CVAR_UNKOWN1 = 2048,
    CVAR_UNKOWN2 = 4096,
    CVAR_UNKOWN3 = 8192,
    CVAR_UNKOWN4 = 16384,
    CVAR_UNKOWN5 = 32768,
    CVAR_UNKOWN6 = 65536,
    CVAR_UNKOWN7 = 131072,
    CVAR_UNKOWN8 = 262144,
    CVAR_UNKOWN9 = 524288,
    CVAR_UNKOWN10 = 1048576,
}

// paramters for command buffer stuffing
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(u32)]
pub enum cbufExec_t {
    EXEC_NOW = 0, // don't return until completed, a VM should NEVER use this,
    // because some commands might cause the VM to be unloaded...
    EXEC_INSERT = 1, // insert at current position, but don't run yet
    EXEC_APPEND = 2, // add to end of the command buffer (normal case)
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct cvar_s {
    pub name: *mut c_char,
    pub string: *mut c_char,
    pub resetString: *mut c_char, // cvar_restart will reset to this value
    pub latchedString: *mut c_char, // for CVAR_LATCH vars
    pub defaultString: *mut c_char,
    pub minimumString: *mut c_char,
    pub maximumString: *mut c_char,
    pub flags: c_int,
    pub modified: qboolean,
    pub _unknown2: [u8; 4usize],
    pub modificationCount: c_int, // incremented each time the cvar is changed
    pub value: f32,               // atof( string )
    pub integer: c_int,           // atof( string )
    pub _unknown3: [u8; 8usize],
    pub next: *mut cvar_s,
    pub hashNext: *mut cvar_s,
}

#[allow(non_camel_case_types)]
pub type cvar_t = cvar_s;

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct msg_t {
    pub allowoverflow: qboolean, // if false, do a Com_Error
    pub overflowed: qboolean,    // set to true if the buffer size failed (with allowoverflow set)
    pub oob: qboolean,           // set to true if the buffer size failed (with allowoverflow set)
    pub data: *mut byte,
    pub maxsize: c_int,
    pub cursize: c_int,
    pub readcount: c_int,
    pub bit: c_int, // for bitwise reads and writes
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct usercmd_s {
    pub serverTime: c_int,
    pub angles: [c_int; 3usize],
    pub buttons: c_int,
    pub weapon: byte,
    pub weaponPrimary: byte,
    pub fov: byte,
    pub forwardmove: c_char,
    pub rightmove: c_char,
    pub upmove: c_char,
}

#[allow(non_camel_case_types)]
pub type usercmd_t = usercmd_s;

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum netsrc_t {
    NS_CLIENT = 0,
    NS_SERVER = 1,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(u32)]
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum netadrtype_t {
    NA_BOT = 0,
    NA_BAD = 1, // an address lookup failed
    NA_LOOPBACK = 2,
    NA_BROADCAST = 3,
    NA_IP = 4,
    NA_IPX = 5,
    NA_BROADCAST_IPX = 6,
}

#[allow(non_camel_case_types)]
#[repr(u32)]
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum trType_t {
    TR_STATIONARY = 0,
    TR_INTERPOLATE = 1, // non-parametric, but interpolate between snapshots
    TR_LINEAR = 2,
    TR_LINEAR_STOP = 3,
    TR_SINE = 4, // value = base + sin( time / duration ) * delta
    TR_GRAVITY = 5,
}

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct netadr_t {
    pub type_: netadrtype_t,
    pub ip: [byte; 4usize],
    pub ipx: [byte; 10usize],
    pub port: c_ushort,
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
pub struct netchan_t {
    pub sock: netsrc_t,
    pub dropped: c_int, // between last packet and previous
    pub remoteAddress: netadr_t,
    pub qport: c_int, // qport value to write when transmitting
    // sequencing variables
    pub incomingSequence: c_int,
    pub outgoingSequence: c_int,
    // incoming fragment assembly buffer
    pub fragmentSequence: c_int,
    pub fragmentLength: c_int,
    pub fragmentBuffer: [byte; 16384usize],
    // outgoing fragment buffer
    // we need to space out the sending of large fragmented messages
    pub unsentFragments: qboolean,
    pub unsentFragmentStart: c_int,
    pub unsentLength: c_int,
    pub unsentBuffer: [byte; 16384usize],
}

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct cplane_s {
    pub normal: vec3_t,
    pub dist: f32,
    pub type_: byte,
    pub signbits: byte,
    pub pad: [byte; 2usize],
}

#[allow(non_camel_case_types)]
pub type cplane_t = cplane_s;

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct trace_t {
    pub allsolid: qboolean,
    pub startsolid: qboolean,
    pub fraction: f32,
    pub endpos: vec3_t,
    pub plane: cplane_t,
    pub surfaceFlags: c_int,
    pub contents: c_int,
    pub entityNum: c_int,
}

// playerState_t is a full superset of entityState_t as it is used by players,
// so if a playerState_t is transmitted, the entityState_t can be fully derived
// from it.
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct playerState_s {
    pub commandTime: c_int,
    pub pm_type: c_int,
    pub bobCycle: c_int,
    pub pm_flags: c_int,
    pub pm_time: c_int,
    pub origin: vec3_t,
    pub velocity: vec3_t,
    pub weaponTime: c_int,
    pub gravity: c_int,
    pub speed: c_int,
    pub delta_angles: [c_int; 3usize],
    pub groundEntityNum: c_int,
    pub legsTimer: c_int,
    pub legsAnim: c_int,
    pub torsoTimer: c_int,
    pub torsoAnim: c_int,
    pub movementDir: c_int,
    pub grapplePoint: vec3_t,
    pub eFlags: c_int,
    pub eventSequence: c_int,
    pub events: [c_int; 2usize],
    pub eventParms: [c_int; 2usize],
    pub externalEvent: c_int,
    pub externalEventParm: c_int,
    pub clientNum: c_int,
    pub location: c_int,
    pub weapon: c_int,
    pub weaponPrimary: c_int,
    pub weaponstate: c_int,
    pub fov: c_int,
    pub viewangles: vec3_t,
    pub viewheight: c_int,
    pub damageEvent: c_int,
    pub damageYaw: c_int,
    pub damagePitch: c_int,
    pub damageCount: c_int,
    pub stats: [c_int; 16usize],
    pub persistant: [c_int; 16usize],
    pub powerups: [c_int; 16usize],
    pub ammo: [c_int; 16usize],
    pub generic1: c_int,
    pub loopSound: c_int,
    pub jumppad_ent: c_int,
    pub jumpTime: c_int,
    pub doubleJumped: c_int,
    pub crouchTime: c_int,
    pub crouchSlideTime: c_int,
    pub forwardmove: c_char,
    pub rightmove: c_char,
    pub upmove: c_char,
    pub ping: c_int,
    pub pmove_framecount: c_int,
    pub jumppad_frame: c_int,
    pub entityEventSequence: c_int,
    pub freezetime: c_int,
    pub thawtime: c_int,
    pub thawClientNum_valid: c_int,
    pub thawClientNum: c_int,
    pub respawnTime: c_int,
    pub localPersistant: [c_int; 16usize],
    pub roundDamage: c_int,
    pub roundShots: c_int,
    pub roundHits: c_int,
}

#[allow(non_camel_case_types)]
pub type playerState_t = playerState_s;

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct pmove_t {
    pub ps: *mut playerState_t,
    pub cmd: usercmd_t,
    pub tracemask: c_int,
    pub debugLevel: c_int,
    pub noFootsteps: c_int,
    pub gauntletHit: qboolean,
    pub numtouch: c_int,
    pub touchents: [c_int; 32usize],
    pub mins: vec3_t,
    pub maxs: vec3_t,
    pub watertype: c_int,
    pub waterlevel: c_int,
    pub xyspeed: f32,
    pub stepHeight: f32,
    pub stepTime: c_int,
    pub trace: Option<
        unsafe extern "C" fn(
            arg1: *mut trace_t,
            arg2: *const vec_t,
            arg3: *const vec_t,
            arg4: *const vec_t,
            arg5: *const vec_t,
            arg6: c_int,
            arg7: c_int,
        ),
    >,
    pub pointcontents: Option<unsafe extern "C" fn(arg1: *const vec_t, arg2: c_int) -> c_int>,
    pub hookEnemy: qboolean,
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct clientSnapshot_t {
    pub areabytes: c_int,
    pub areabits: [byte; 32usize], // portalarea visibility bits
    pub ps: playerState_t,
    pub num_entities: c_int,
    pub first_entity: c_int, // into the circular sv_packet_entities[]
    // the entities MUST be in increasing state number
    // order, otherwise the delta compression will fail
    pub messageSent: c_int,  // time the message was transmitted
    pub messageAcked: c_int, // time the message was acked
    pub messageSize: c_int,  // used to rate drop packets
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
pub struct netchan_buffer_s {
    pub msg: msg_t,
    pub msgBuffer: [byte; 16384usize],
    pub next: *mut netchan_buffer_s,
}

#[allow(non_camel_case_types)]
pub type netchan_buffer_t = netchan_buffer_s;

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct trajectory_t {
    pub trType: trType_t,
    pub trTime: c_int,
    pub trDuration: c_int,
    pub trBase: vec3_t,
    pub trDelta: vec3_t,
    pub gravity: f32,
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct entityState_s {
    pub number: c_int,
    pub eType: c_int,
    pub eFlags: c_int,
    pub pos: trajectory_t,
    pub apos: trajectory_t,
    pub time: c_int,
    pub time2: c_int,
    pub origin: vec3_t,
    pub origin2: vec3_t,
    pub angles: vec3_t,
    pub angles2: vec3_t,
    pub otherEntityNum: c_int,
    pub otherEntityNum2: c_int,
    pub groundEntityNum: c_int,
    pub constantLight: c_int,
    pub loopSound: c_int,
    pub modelindex: c_int,
    pub modelindex2: c_int,
    pub clientNum: c_int,
    pub frame: c_int,
    pub solid: c_int,
    pub event: c_int,
    pub eventParm: c_int,
    pub powerups: c_int,
    pub health: c_int,
    pub armor: c_int,
    pub weapon: c_int,
    pub location: c_int,
    pub legsAnim: c_int,
    pub torsoAnim: c_int,
    pub generic1: c_int,
    pub jumpTime: c_int,
    pub doubleJumped: c_int,
}

#[allow(non_camel_case_types)]
pub type entityState_t = entityState_s;

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct entityShared_t {
    pub s: entityState_t,
    pub linked: qboolean,
    pub linkcount: c_int,
    pub svFlags: c_int,
    pub singleClient: c_int,
    pub bmodel: qboolean,
    pub mins: vec3_t,
    pub maxs: vec3_t,
    pub contents: c_int,
    pub absmin: vec3_t,
    pub absmax: vec3_t,
    pub currentOrigin: vec3_t,
    pub currentAngles: vec3_t,
    pub ownerNum: c_int,
}

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct sharedEntity_t {
    pub s: entityState_t,  // communicated by server to clients
    pub r: entityShared_t, // shared by both the server system and game
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
pub struct client_s {
    pub state: clientState_t,
    pub userinfo: [c_char; 1024usize], // name, etc
    pub reliableCommands: [[c_char; 1024usize]; 64usize],
    pub reliableSequence: c_int, // last added reliable message, not necesarily sent or acknowledged yet
    pub reliableAcknowledge: c_int, // last acknowledged reliable message
    pub reliableSent: c_int,     // last sent reliable message, not necesarily acknowledged yet
    pub messageAcknowledge: c_int,
    pub gamestateMessageNum: c_int, // netchan->outgoingSequence of gamestate
    pub challenge: c_int,
    pub lastUsercmd: usercmd_t,
    pub lastMessageNum: c_int,    // for delta compression
    pub lastClientCommand: c_int, // reliable client message sequence
    pub lastClientCommandString: [c_char; 1024usize],
    pub gentity: *mut sharedEntity_t, // SV_GentityNum(clientnum)
    pub name: [c_char; 32usize],      // extracted from userinfo, high bits masked

    // Mino: I think everything above this is correct. Below is a mess.

    // downloading
    pub downloadName: [c_char; 64usize], // if not empty string, we are downloading
    pub download: fileHandle_t,          // file being downloaded
    pub downloadSize: c_int,             // total bytes (can't use EOF because of paks)
    pub downloadCount: c_int,            // bytes sent
    pub downloadClientBlock: c_int,      // last block we sent to the client, awaiting ack
    pub downloadCurrentBlock: c_int,     // current block number
    pub downloadXmitBlock: c_int,        // last block we xmited
    pub downloadBlocks: [*mut c_uchar; 8usize], // the buffers for the download blocks
    pub downloadBlockSize: [c_int; 8usize],
    pub downloadEOF: qboolean,               // We have sent the EOF block
    pub downloadSendTime: c_int,             // time we last got an ack from the client
    pub deltaMessage: c_int,                 // frame last client usercmd message
    pub nextReliableTime: c_int, // svs.time when another reliable command will be allowed
    pub lastPacketTime: c_int,   // svs.time when packet was last received
    pub lastConnectTime: c_int,  // svs.time when connection started
    pub nextSnapshotTime: c_int, // send another snapshot when svs.time >= nextSnapshotTime
    pub rateDelayed: qboolean, // true if nextSnapshotTime was set based on rate instead of snapshotMsec
    pub timeoutCount: c_int,   // must timeout a few frames in a row so debugging doesn't break
    pub frames: [clientSnapshot_t; 32usize], // updates can be delta'd from here
    pub ping: c_int,
    pub rate: c_int,         // bytes / second
    pub snapshotMsec: c_int, // requests a snapshot every snapshotMsec unless rate choked
    pub pureAuthentic: c_int,
    pub gotCP: qboolean, // TTimo - additional flag to distinguish between a bad pure checksum, and no cp command at all
    pub netchan: netchan_t,
    pub netchan_start_queue: *mut netchan_buffer_t,
    pub netchan_end_queue: *mut *mut netchan_buffer_t,
    // Mino: Holy crap. A bunch of data was added. I have no idea where it actually goes,
    // but this will at least correct sizeof(client_t).
    #[cfg(target_pointer_width = "64")]
    pub _unknown2: [u8; 36808usize],
    #[cfg(target_pointer_width = "32")]
    _unknown2: [u8; 36836usize], // TODO: Outdated.
    // Mino: Woohoo! How nice of them to put the SteamID last.
    pub steam_id: u64,
}

#[allow(non_camel_case_types)]
pub type client_t = client_s;

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct challenge_t {
    pub adr: netadr_t,
    pub challenge: c_int,
    pub time: c_int,      // time the last packet was sent to the autherize server
    pub pingTime: c_int,  // time the challenge response was sent to client
    pub firstTime: c_int, // time the adr was first used, for authorize timeout checks
    pub connected: qboolean,
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
pub struct serverStatic_t {
    pub initialized: qboolean,                // sv_init has completed
    pub time: c_int,                          // will be strictly increasing across level changes
    pub snapFlagServerBit: c_int,             // ^= SNAPFLAG_SERVERCOUNT every SV_SpawnServer()
    pub clients: *mut client_t,               // [sv_maxclients->integer];
    pub numSnapshotEntities: c_int, // sv_maxclients->integer*PACKET_BACKUP*MAX_PACKET_ENTITIES
    pub nextSnapshotEntities: c_int, // next snapshotEntities to use
    pub snapshotEntities: *mut entityState_t, // [numSnapshotEntities]
    pub nextHeartbeatTime: c_int,
    pub challenges: [challenge_t; 1024usize], // to prevent invalid IPs from connecting
    pub redirectAddress: netadr_t,            // for rcon return messages
    pub authorizeAddress: netadr_t,           // for rcon return messages
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct svEntity_s {
    pub worldSector: *mut worldSector_s,
    pub nextEntityInWorldSector: *mut svEntity_s,
    pub baseline: entityState_t, // for delta compression of initial sighting
    pub numClusters: c_int,      // if -1, use headnode instead
    pub clusternums: [c_int; 16usize],
    pub lastCluster: c_int, // if all the clusters don't fit in clusternums
    pub areanum: c_int,
    pub areanum2: c_int,
    pub snapshotCounter: c_int, // used to prevent double adding from portal views
}

#[allow(non_camel_case_types)]
pub type svEntity_t = svEntity_s;

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct worldSector_s {
    pub axis: c_int, // -1 = leaf node
    pub dist: f32,
    pub children: [*mut worldSector_s; 2usize],
    pub entities: *mut svEntity_t,
}

#[allow(dead_code)]
#[allow(non_camel_case_types)]
pub type worldSector_t = worldSector_s;

#[repr(u32)]
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum serverState_t {
    SS_DEAD = 0,    // no map loaded
    SS_LOADING = 1, // spawning level entities
    SS_GAME = 2,    // actively running
}

#[allow(non_snake_case)]
#[allow(dead_code)]
#[repr(C)]
pub struct server_t {
    pub state: serverState_t,
    pub restarting: qboolean,
    pub serverId: c_int,
    pub restartedServerId: c_int,
    pub checksumFeed: c_int,
    pub checksumFeedServerId: c_int,
    pub snapshotCounter: c_int,
    pub timeResidual: c_int,
    pub nextFrameTime: c_int,
    pub models: [*mut cmodel_s; 256usize],
    pub configstrings: [*mut c_char; 1024usize],
    pub svEntities: [svEntity_t; 1024usize],
    pub entityParsePoint: *mut c_char,
    pub gentities: *mut sharedEntity_t,
    pub gentitySize: c_int,
    pub num_entities: c_int,
    pub gameClients: *mut playerState_t,
    pub gameClientSize: c_int,
    pub restartTime: c_int,
}

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct playerTeamState_t {
    pub state: playerTeamStateState_t,
    pub captures: c_int,
    pub basedefense: c_int,
    pub carrierdefense: c_int,
    pub flagrecovery: c_int,
    pub fragcarrier: c_int,
    pub assists: c_int,
    pub flagruntime: c_int,
    pub flagrunrelays: c_int,
    pub lasthurtcarrier: c_int,
    pub lastreturnedflag: c_int,
    pub lastfraggedcarrier: c_int,
}

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct expandedStatObj_t {
    pub statId: c_uint,
    pub lastThinkTime: c_int,
    pub teamJoinTime: c_int,
    pub totalPlayTime: c_int,
    pub serverRank: c_int,
    pub serverRankIsTied: qboolean,
    pub teamRank: c_int,
    pub teamRankIsTied: qboolean,
    pub numKills: c_int,
    pub numDeaths: c_int,
    pub numSuicides: c_int,
    pub numTeamKills: c_int,
    pub numTeamKilled: c_int,
    pub numWeaponKills: [c_int; 16usize],
    pub numWeaponDeaths: [c_int; 16usize],
    pub shotsFired: [c_int; 16usize],
    pub shotsHit: [c_int; 16usize],
    pub damageDealt: [c_int; 16usize],
    pub damageTaken: [c_int; 16usize],
    pub powerups: [c_int; 16usize],
    pub holdablePickups: [c_int; 7usize],
    pub weaponPickups: [c_int; 16usize],
    pub weaponUsageTime: [c_int; 16usize],
    pub numCaptures: c_int,
    pub numAssists: c_int,
    pub numDefends: c_int,
    pub numHolyShits: c_int,
    pub totalDamageDealt: c_int,
    pub totalDamageTaken: c_int,
    pub previousHealth: c_int,
    pub previousArmor: c_int,
    pub numAmmoPickups: c_int,
    pub numFirstMegaHealthPickups: c_int,
    pub numMegaHealthPickups: c_int,
    pub megaHealthPickupTime: c_int,
    pub numHealthPickups: c_int,
    pub numFirstRedArmorPickups: c_int,
    pub numRedArmorPickups: c_int,
    pub redArmorPickupTime: c_int,
    pub numFirstYellowArmorPickups: c_int,
    pub numYellowArmorPickups: c_int,
    pub yellowArmorPickupTime: c_int,
    pub numFirstGreenArmorPickups: c_int,
    pub numGreenArmorPickups: c_int,
    pub greenArmorPickupTime: c_int,
    pub numQuadDamagePickups: c_int,
    pub numQuadDamageKills: c_int,
    pub numBattleSuitPickups: c_int,
    pub numRegenerationPickups: c_int,
    pub numHastePickups: c_int,
    pub numInvisibilityPickups: c_int,
    pub numRedFlagPickups: c_int,
    pub numBlueFlagPickups: c_int,
    pub numNeutralFlagPickups: c_int,
    pub numMedkitPickups: c_int,
    pub numArmorPickups: c_int,
    pub numDenials: c_int,
    pub killStreak: c_int,
    pub maxKillStreak: c_int,
    pub xp: c_int,
    pub domThreeFlagsTime: c_int,
    pub numMidairShotgunKills: c_int,
}

// client data that stays across multiple respawns, but is cleared
// on each level change or team change at ClientBegin()
#[allow(non_snake_case)]
#[repr(C, align(8))]
pub struct clientPersistant_t {
    pub connected: clientConnected_t,
    pub cmd: usercmd_t,
    pub localClient: qboolean,
    pub initialSpawn: qboolean,
    pub predictItemPickup: qboolean,
    pub netname: [c_char; 40usize],
    pub country: [c_char; 24usize],
    pub steamId: u64,
    pub maxHealth: c_int,
    pub voteCount: c_int,
    pub voteState: voteState_t,
    pub complaints: c_int,
    pub complaintClient: c_int,
    pub complaintEndTime: c_int,
    pub damageFromTeammates: c_int,
    pub damageToTeammates: c_int,
    pub ready: qboolean,
    pub autoaction: c_int,
    pub timeouts: c_int,
    pub enterTime: c_int,
    pub teamState: playerTeamState_t,
    pub damageResidual: c_int,
    pub inactivityTime: c_int,
    pub inactivityWarning: c_int,
    pub lastUserinfoUpdate: c_int,
    pub userInfoFloodInfractions: c_int,
    pub lastMapVoteTime: c_int,
    pub lastMapVoteIndex: c_int,
}

// client data that stays across multiple levels or tournament restarts
// this is achieved by writing all the data to cvar strings at game shutdown
// time and reading them back at connection time.  Anything added here
// MUST be dealt with in G_InitSessionData() / G_ReadSessionData() / G_WriteSessionData()
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct clientSession_t {
    pub sessionTeam: team_t,
    pub spectatorTime: c_int,
    pub spectatorState: spectatorState_t,
    pub spectatorClient: c_int,
    pub weaponPrimary: c_int,
    pub wins: c_int,
    pub losses: c_int,
    pub teamLeader: qboolean,
    pub privileges: privileges_t,
    pub specOnly: c_int,
    pub playQueue: c_int,
    pub updatePlayQueue: qboolean,
    pub muted: c_int,
    pub prevScore: c_int,
}

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct gitem_s {
    pub classname: *mut c_char,
    pub pickup_sound: *const c_char,
    pub world_model: [*const c_char; 4usize],
    pub premium_model: [*const c_char; 4usize],
    pub icon: *const c_char,
    pub pickup_name: *const c_char,
    pub quantity: c_int,
    pub giType: itemType_t,
    pub giTag: c_int,
    pub itemTimer: qboolean,
    pub maskGametypeRenderSkip: c_uint,
    pub maskGametypeForceSpawn: c_uint,
}

pub type gitem_t = gitem_s;

#[repr(u32)]
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum entityType_t {
    ET_GENERAL = 0,
    ET_PLAYER = 1,
    ET_ITEM = 2,
    ET_MISSILE = 3,
    ET_MOVER = 4,
    ET_BEAM = 5,
    ET_PORTAL = 6,
    ET_SPEAKER = 7,
    ET_PUSH_TRIGGER = 8,
    ET_TELEPORT_TRIGGER = 9,
    ET_INVISIBLE = 10,
    ET_GRAPPLE = 11, // grapple hooked on wall
    ET_TEAM = 12,
    ET_EVENTS = 13, // any of the EV_* events can be added freestanding
                    // by setting eType to ET_EVENTS + eventNum
                    // this avoids having to set eFlags and eventNum
}

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct gentity_s {
    pub s: entityState_t,
    pub r: entityShared_t,
    pub client: *mut gclient_s,
    pub inuse: qboolean,
    pub classname: *mut c_char,
    pub spawnflags: c_int,
    pub neverFree: qboolean,
    pub flags: c_int,
    pub model: *mut c_char,
    pub model2: *mut c_char,
    pub freetime: c_int,
    pub eventTime: c_int,
    pub freeAfterEvent: qboolean,
    pub unlinkAfterEvent: qboolean,
    pub physicsObject: qboolean,
    pub physicsBounce: f32,
    pub clipmask: c_int,
    pub moverState: moverState_t,
    pub soundPos1: c_int,
    pub sound1to2: c_int,
    pub sound2to1: c_int,
    pub soundPos2: c_int,
    pub soundLoop: c_int,
    pub parent: *mut gentity_t,
    pub nextTrain: *mut gentity_t,
    pub prevTrain: *mut gentity_t,
    pub pos1: vec3_t,
    pub pos2: vec3_t,
    pub message: *mut c_char,
    pub cvar: *mut c_char,
    pub tourPointTarget: *mut c_char,
    pub tourPointTargetName: *mut c_char,
    pub noise: *mut c_char,
    pub timestamp: c_int,
    pub angle: f32,
    pub target: *mut c_char,
    pub targetname: *mut c_char,
    pub targetShaderName: *mut c_char,
    pub targetShaderNewName: *mut c_char,
    pub target_ent: *mut gentity_t,
    pub speed: f32,
    pub movedir: vec3_t,
    pub nextthink: c_int,
    pub think: Option<unsafe extern "C" fn(arg1: *mut gentity_t)>,
    pub framethink: Option<unsafe extern "C" fn(arg1: *mut gentity_t)>,
    pub reached: Option<unsafe extern "C" fn(arg1: *mut gentity_t)>,
    pub blocked: Option<unsafe extern "C" fn(arg1: *mut gentity_t, arg2: *mut gentity_t)>,
    pub touch: Option<
        unsafe extern "C" fn(arg1: *mut gentity_t, arg2: *mut gentity_t, arg3: *mut trace_t),
    >,
    pub use_: Option<
        unsafe extern "C" fn(arg1: *mut gentity_t, arg2: *mut gentity_t, arg3: *mut gentity_t),
    >,
    pub pain: Option<unsafe extern "C" fn(arg1: *mut gentity_t, arg2: *mut gentity_t, arg3: c_int)>,
    pub die: Option<
        unsafe extern "C" fn(
            arg1: *mut gentity_t,
            arg2: *mut gentity_t,
            arg3: *mut gentity_t,
            arg4: c_int,
            arg5: c_int,
        ),
    >,
    pub pain_debounce_time: c_int,
    pub fly_sound_debounce_time: c_int,
    pub health: c_int,
    pub takedamage: qboolean,
    pub damage: c_int,
    pub damageFactor: c_int,
    pub splashDamage: c_int,
    pub splashRadius: c_int,
    pub methodOfDeath: c_int,
    pub splashMethodOfDeath: c_int,
    pub count: c_int,
    pub enemy: *mut gentity_t,
    pub activator: *mut gentity_t,
    pub team: *const c_char,
    pub teammaster: *mut gentity_t,
    pub teamchain: *mut gentity_t,
    pub kamikazeTime: c_int,
    pub kamikazeShockTime: c_int,
    pub watertype: c_int,
    pub waterlevel: c_int,
    pub noise_index: c_int,
    pub bouncecount: c_int,
    pub wait: f32,
    pub random: f32,
    pub spawnTime: c_int,
    pub item: *const gitem_t,
    pub pickupCount: c_int,
}

#[allow(non_snake_case)]
#[repr(C)]
pub struct raceInfo_t {
    pub racingActive: qboolean,
    pub startTime: c_int,
    pub lastTime: c_int,
    pub best_race: [c_int; 64usize],
    pub current_race: [c_int; 64usize],
    pub currentCheckPoint: c_int,
    pub weaponUsed: qboolean,
    pub nextRacePoint: *mut gentity_t,
    pub nextRacePoint2: *mut gentity_t,
}

// this structure is cleared on each ClientSpawn(),
// except for 'client->pers' and 'client->sess'
#[allow(non_snake_case)]
#[repr(C, align(8))]
pub struct gclient_s {
    pub ps: playerState_t,
    pub pers: clientPersistant_t,
    pub sess: clientSession_t,
    pub noclip: qboolean,
    pub lastCmdTime: c_int,
    pub buttons: c_int,
    pub oldbuttons: c_int,
    pub damage_armor: c_int,
    pub damage_blood: c_int,
    pub damage_from: vec3_t,
    pub damage_fromWorld: qboolean,
    pub impressiveCount: c_int,
    pub accuracyCount: c_int,
    pub accuracy_shots: c_int,
    pub accuracy_hits: c_int,
    pub lastClientKilled: c_int,
    pub lastKilledClient: c_int,
    pub lastHurtClient: [c_int; 2usize],
    pub lastHurtMod: [c_int; 2usize],
    pub lastHurtTime: [c_int; 2usize],
    pub lastKillTime: c_int,
    pub lastGibTime: c_int,
    pub rampageCounter: c_int,
    pub revengeCounter: [c_int; 64usize],
    pub respawnTime: c_int,
    pub rewardTime: c_int,
    pub airOutTime: c_int,
    pub fireHeld: qboolean,
    pub hook: *mut gentity_t,
    pub switchTeamTime: c_int,
    pub timeResidual: c_int,
    pub timeResidualScout: c_int,
    pub timeResidualArmor: c_int,
    pub timeResidualHealth: c_int,
    pub timeResidualPingPOI: c_int,
    pub timeResidualSpecInfo: c_int,
    pub healthRegenActive: qboolean,
    pub armorRegenActive: qboolean,
    pub persistantPowerup: *mut gentity_t,
    pub portalID: c_int,
    pub ammoTimes: [c_int; 16usize],
    pub invulnerabilityTime: c_int,
    pub expandedStats: expandedStatObj_t,
    pub ignoreChatsTime: c_int,
    pub lastUserCmdTime: c_int,
    pub freezePlayer: qboolean,
    pub deferredSpawnTime: c_int,
    pub deferredSpawnCount: c_int,
    pub race: raceInfo_t,
    pub shotgunDmg: [c_int; 64usize],
    pub round_shots: c_int,
    pub round_hits: c_int,
    pub round_damage: c_int,
    pub queuedSpectatorFollow: qboolean,
    pub queuedSpectatorClient: c_int,
}

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct roundState_t {
    pub eCurrent: roundStateState_t,
    pub eNext: roundStateState_t,
    pub tNext: c_int,
    pub startTime: c_int,
    pub turn: c_int,
    pub round: c_int,
    pub prevRoundWinningTeam: team_t,
    pub touch: qboolean,
    pub capture: qboolean,
}

#[allow(non_snake_case)]
#[repr(C)]
pub struct level_locals_t {
    pub clients: *mut gclient_s,
    pub gentities: *mut gentity_s,
    pub gentitySize: c_int,
    pub num_entities: c_int,
    pub warmupTime: c_int,
    pub logFile: fileHandle_t,
    pub maxclients: c_int,
    pub time: c_int,
    pub frametime: c_int,
    pub startTime: c_int,
    pub teamScores: [c_int; 4usize],
    pub nextTeamInfoTime: c_int,
    pub newSession: qboolean,
    pub restarted: qboolean,
    pub shufflePending: qboolean,
    pub shuffleReadyTime: c_int,
    pub numConnectedClients: c_int,
    pub numNonSpectatorClients: c_int,
    pub numPlayingClients: c_int,
    pub numReadyClients: c_int,
    pub numReadyHumans: c_int,
    pub numStandardClients: c_int,
    pub sortedClients: [c_int; 64usize],
    pub follow1: c_int,
    pub follow2: c_int,
    pub snd_fry: c_int,
    pub warmupModificationCount: c_int,
    pub voteString: [c_char; 1024usize],
    pub voteDisplayString: [c_char; 1024usize],
    pub voteExecuteTime: c_int,
    pub voteTime: c_int,
    pub voteYes: c_int,
    pub voteNo: c_int,
    pub pendingVoteCaller: c_int,
    pub spawning: qboolean,
    pub numSpawnVars: c_int,
    pub spawnVars: [[*mut c_char; 2usize]; 64usize],
    pub numSpawnVarChars: c_int,
    pub spawnVarChars: [c_char; 4096usize],
    pub intermissionQueued: c_int,
    pub intermissionTime: c_int,
    pub readyToExit: qboolean,
    pub votingEnded: qboolean,
    pub exitTime: c_int,
    pub intermission_origin: vec3_t,
    pub intermission_angle: vec3_t,
    pub locationLinked: qboolean,
    pub locationHead: *mut gentity_t,
    pub timePauseBegin: c_int,
    pub timeOvertime: c_int,
    pub timeInitialPowerupSpawn: c_int,
    pub bodyQueIndex: c_int,
    pub bodyQue: [*mut gentity_t; 8usize],
    pub portalSequence: c_int,
    pub gameStatsReported: qboolean,
    pub mapIsTrainingMap: qboolean,
    pub clientNum1stPlayer: c_int,
    pub clientNum2ndPlayer: c_int,
    pub scoreboardArchive1: [c_char; 1024usize],
    pub scoreboardArchive2: [c_char; 1024usize],
    pub firstScorer: [c_char; 40usize],
    pub lastScorer: [c_char; 40usize],
    pub lastTeamScorer: [c_char; 40usize],
    pub firstFrag: [c_char; 40usize],
    pub red_flag_origin: vec3_t,
    pub blue_flag_origin: vec3_t,
    pub spawnCount: [c_int; 4usize],
    pub runeSpawns: [c_int; 5usize],
    pub itemCount: [c_int; 60usize],
    pub suddenDeathRespawnDelay: c_int,
    pub suddenDeathRespawnDelayLastAnnounced: c_int,
    pub numRedArmorPickups: [c_int; 4usize],
    pub numYellowArmorPickups: [c_int; 4usize],
    pub numGreenArmorPickups: [c_int; 4usize],
    pub numMegaHealthPickups: [c_int; 4usize],
    pub numQuadDamagePickups: [c_int; 4usize],
    pub numBattleSuitPickups: [c_int; 4usize],
    pub numRegenerationPickups: [c_int; 4usize],
    pub numHastePickups: [c_int; 4usize],
    pub numInvisibilityPickups: [c_int; 4usize],
    pub quadDamagePossessionTime: [c_int; 4usize],
    pub battleSuitPossessionTime: [c_int; 4usize],
    pub regenerationPossessionTime: [c_int; 4usize],
    pub hastePossessionTime: [c_int; 4usize],
    pub invisibilityPossessionTime: [c_int; 4usize],
    pub numFlagPickups: [c_int; 4usize],
    pub numMedkitPickups: [c_int; 4usize],
    pub flagPossessionTime: [c_int; 4usize],
    pub dominationPoints: [*mut gentity_t; 5usize],
    pub dominationPointCount: c_int,
    pub dominationPointsTallied: c_int,
    pub racePointCount: c_int,
    pub disableDropWeapon: qboolean,
    pub teamShuffleActive: qboolean,
    pub lastTeamScores: [c_int; 4usize],
    pub lastTeamRoundScores: [c_int; 4usize],
    pub attackingTeam: team_t,
    pub roundState: roundState_t,
    pub lastTeamCountSent: c_int,
    pub infectedConscript: c_int,
    pub lastZombieSurvivor: c_int,
    pub zombieScoreTime: c_int,
    pub lastInfectionTime: c_int,
    pub intermissionMapNames: [[c_char; 1024usize]; 3usize],
    pub intermissionMapTitles: [[c_char; 1024usize]; 3usize],
    pub intermissionMapConfigs: [[c_char; 1024usize]; 3usize],
    pub intermissionMapVotes: [c_int; 3usize],
    pub matchForfeited: qboolean,
    pub allReadyTime: c_int,
    pub notifyCvarChange: qboolean,
    pub notifyCvarChangeTime: c_int,
    pub lastLeadChangeTime: c_int,
    pub lastLeadChangeClient: c_int,
}

// Some extra stuff that's not in the Q3 source. These are the commands you
// get when you type ? in the console. The array has a sentinel struct, so
// check "cmd" == NULL.
#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct adminCmd_t {
    pub needed_privileges: privileges_t,
    pub unknown1: c_int,
    pub cmd: *mut c_char, // The command name, e.g. "tempban".
    pub admin_func: Option<unsafe extern "C" fn(ent: *mut gentity_t)>,
    pub unknown2: c_int,
    pub unknown3: c_int,
    pub description: *mut c_char, // Command description that gets printed when you do "?".
}

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct cmodel_s {
    pub _address: u8,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
#[repr(C)]
pub enum healthPickup_t {
    H_NONE = 0,
    H_MEGA = 1,
    H_LARGE = 2,
    H_MEDIUM = 3,
    H_SMALL = 4,
    H_NUM_HEALTHS = 5,
}

pub(crate) struct GameClient {
    game_client: &'static mut gclient_t,
}

impl TryFrom<*mut gclient_t> for GameClient {
    type Error = &'static str;

    fn try_from(game_client: *mut gclient_t) -> Result<Self, Self::Error> {
        unsafe {
            game_client
                .as_mut()
                .map(|gclient_t| Self {
                    game_client: gclient_t,
                })
                .ok_or("null pointer passed")
        }
    }
}

lazy_static! {
    static ref POWERUP_INDEX_LOOKUP: HashMap<i32, usize> = HashMap::from([
        (0, PW_QUAD as usize),
        (1, PW_BATTLESUIT as usize),
        (2, PW_HASTE as usize),
        (3, PW_INVIS as usize),
        (4, PW_REGEN as usize),
        (5, PW_INVULNERABILITY as usize),
    ]);
}

impl GameClient {
    pub(crate) fn get_client_num(&self) -> i32 {
        self.game_client.ps.clientNum
    }

    pub(crate) fn remove_kamikaze_flag(&mut self) {
        self.game_client
            .ps
            .eFlags
            .bitand_assign(!EF_KAMIKAZE as i32);
    }

    pub(crate) fn set_privileges(&mut self, privileges: i32) {
        self.game_client.sess.privileges = privileges_t::from(privileges);
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.game_client.ps.pm_type == 0
    }

    pub(crate) fn get_position(&self) -> (f32, f32, f32) {
        (
            self.game_client.ps.origin[0],
            self.game_client.ps.origin[1],
            self.game_client.ps.origin[2],
        )
    }

    pub(crate) fn set_position(&mut self, position: (f32, f32, f32)) {
        self.game_client.ps.origin[0] = position.0;
        self.game_client.ps.origin[1] = position.1;
        self.game_client.ps.origin[2] = position.2;
    }

    pub(crate) fn get_velocity(&self) -> (f32, f32, f32) {
        (
            self.game_client.ps.velocity[0],
            self.game_client.ps.velocity[1],
            self.game_client.ps.velocity[2],
        )
    }

    pub(crate) fn set_velocity(&mut self, velocity: (f32, f32, f32)) {
        self.game_client.ps.velocity[0] = velocity.0 as c_float;
        self.game_client.ps.velocity[1] = velocity.1 as c_float;
        self.game_client.ps.velocity[2] = velocity.2 as c_float;
    }

    pub(crate) fn get_armor(&self) -> i32 {
        self.game_client.ps.stats[STAT_ARMOR as usize]
    }

    pub(crate) fn set_armor(&mut self, armor: i32) {
        self.game_client.ps.stats[STAT_ARMOR as usize] = armor;
    }

    pub(crate) fn get_noclip(&self) -> bool {
        self.game_client.noclip.into()
    }

    pub(crate) fn set_noclip(&mut self, activate: bool) {
        self.game_client.noclip = activate.into();
    }

    pub(crate) fn get_weapon(&self) -> i32 {
        self.game_client.ps.weapon
    }

    pub(crate) fn set_weapon(&mut self, weapon: i32) {
        self.game_client.ps.weapon = weapon;
    }

    pub(crate) fn get_weapons(&self) -> [bool; 15] {
        let mut returned = [false; 15];
        let weapon_stats = self.game_client.ps.stats[STAT_WEAPONS as usize];
        for (i, item) in returned.iter_mut().enumerate() {
            *item = weapon_stats.bitand(1 << (i + 1)) != 0;
        }
        returned
    }

    pub(crate) fn set_weapons(&mut self, weapons: [bool; 15]) {
        let mut weapon_flags = 0;
        for (i, &item) in weapons.iter().enumerate() {
            let modifier = if item { 1 << (i + 1) } else { 0 };
            weapon_flags.bitor_assign(modifier);
        }

        self.game_client.ps.stats[STAT_WEAPONS as usize] = weapon_flags;
    }

    pub(crate) fn get_ammo(&self) -> [i32; 15] {
        let mut returned = [0; 15];
        let ammos = self.game_client.ps.ammo;
        for (i, item) in returned.iter_mut().enumerate() {
            *item = ammos[i + 1];
        }
        returned
    }

    pub(crate) fn set_ammos(&mut self, ammos: [i32; 15]) {
        for (i, &item) in ammos.iter().enumerate() {
            self.game_client.ps.ammo[i + 1] = item;
        }
    }

    pub(crate) fn get_powerups(&self) -> [i32; 6] {
        let mut returned = [0; 6];
        let current_level = CurrentLevel::default();
        for (powerup, item) in returned.iter_mut().enumerate() {
            let powerup_index = *POWERUP_INDEX_LOOKUP.get(&(powerup as i32)).unwrap();
            *item = self.game_client.ps.powerups[powerup_index] - current_level.get_leveltime();
        }
        returned
    }

    pub(crate) fn set_powerups(&mut self, powerups: [i32; 6]) {
        let current_level = CurrentLevel::default();
        for (powerup, &item) in powerups.iter().enumerate() {
            let powerup_index = *POWERUP_INDEX_LOOKUP.get(&(powerup as i32)).unwrap();
            if item == 0 {
                self.game_client.ps.powerups[powerup_index] = 0;
            } else {
                let level_time = current_level.get_leveltime();
                self.game_client.ps.powerups[powerup_index] =
                    level_time - (level_time % 1000) + item;
            }
        }
    }

    pub(crate) fn get_holdable(&self) -> i32 {
        self.game_client.ps.stats[STAT_HOLDABLE_ITEM as usize]
    }

    pub(crate) fn set_holdable(&mut self, holdable: i32) {
        // 37 - kamikaze
        if holdable == 37 {
            self.game_client.ps.eFlags.bitor_assign(EF_KAMIKAZE as i32);
        } else {
            self.remove_kamikaze_flag();
        }
        self.game_client.ps.stats[STAT_HOLDABLE_ITEM as usize] = holdable;
    }

    pub(crate) fn get_current_flight_fuel(&self) -> i32 {
        self.game_client.ps.stats[STAT_CUR_FLIGHT_FUEL as usize]
    }

    pub(crate) fn get_max_flight_fuel(&self) -> i32 {
        self.game_client.ps.stats[STAT_MAX_FLIGHT_FUEL as usize]
    }

    pub(crate) fn get_flight_thrust(&self) -> i32 {
        self.game_client.ps.stats[STAT_FLIGHT_THRUST as usize]
    }

    pub(crate) fn get_flight_refuel(&self) -> i32 {
        self.game_client.ps.stats[STAT_FLIGHT_REFUEL as usize]
    }

    pub(crate) fn set_flight(&mut self, flight_params: (i32, i32, i32, i32)) {
        self.game_client.ps.stats[STAT_CUR_FLIGHT_FUEL as usize] = flight_params.0;
        self.game_client.ps.stats[STAT_MAX_FLIGHT_FUEL as usize] = flight_params.1;
        self.game_client.ps.stats[STAT_FLIGHT_THRUST as usize] = flight_params.2;
        self.game_client.ps.stats[STAT_FLIGHT_REFUEL as usize] = flight_params.3;
    }

    pub(crate) fn set_invulnerability(&mut self, time: i32) {
        self.game_client.invulnerabilityTime = CurrentLevel::default().get_leveltime() + time;
    }

    pub(crate) fn is_frozen(&self) -> bool {
        self.game_client.ps.pm_type == 4
    }

    pub(crate) fn get_score(&self) -> i32 {
        if self.game_client.sess.sessionTeam == TEAM_SPECTATOR {
            0
        } else {
            self.game_client.ps.persistant[PERS_ROUND_SCORE as usize]
        }
    }

    pub(crate) fn set_score(&mut self, score: i32) {
        self.game_client.ps.persistant[PERS_ROUND_SCORE as usize] = score;
    }

    pub(crate) fn get_kills(&self) -> i32 {
        self.game_client.expandedStats.numKills
    }

    pub(crate) fn get_deaths(&self) -> i32 {
        self.game_client.expandedStats.numDeaths
    }

    pub(crate) fn get_damage_dealt(&self) -> i32 {
        self.game_client.expandedStats.totalDamageDealt
    }

    pub(crate) fn get_damage_taken(&self) -> i32 {
        self.game_client.expandedStats.totalDamageTaken
    }

    pub(crate) fn get_time_on_team(&self) -> i32 {
        CurrentLevel::default().level.time - self.game_client.pers.enterTime
    }

    pub(crate) fn get_ping(&self) -> i32 {
        self.game_client.ps.ping
    }

    pub(crate) fn set_vote_pending(&mut self) {
        self.game_client.pers.voteState = VOTE_PENDING;
    }

    pub(crate) fn spawn(&mut self) {
        self.game_client.ps.pm_type = PM_NORMAL as c_int;
    }
}

pub(crate) struct GameEntity {
    gentity_t: &'static mut gentity_t,
}

impl TryFrom<*mut gentity_t> for GameEntity {
    type Error = &'static str;

    fn try_from(game_entity: *mut gentity_t) -> Result<Self, Self::Error> {
        unsafe {
            game_entity
                .as_mut()
                .map(|gentity| Self { gentity_t: gentity })
                .ok_or("null pointer passed")
        }
    }
}

extern "C" {
    static g_entities: *mut gentity_t;
}

impl TryFrom<i32> for GameEntity {
    type Error = &'static str;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        if client_id < 0 {
            return Err("invalid client_id");
        }
        unsafe {
            g_entities
                .offset(client_id as isize)
                .as_mut()
                .map(|gentity| Self { gentity_t: gentity })
                .ok_or("client not found")
        }
    }
}

extern "C" {
    static G_StartKamikaze: extern "C" fn(*const gentity_t);
    static Touch_Item: extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t);
    static G_FreeEntity: extern "C" fn(*mut gentity_t);
    static bg_itemlist: *const gitem_t;
    static LaunchItem: extern "C" fn(*const gitem_t, vec3_t, vec3_t) -> *const gentity_t;
    static G_Damage: extern "C" fn(
        *const gentity_t,
        *const gentity_t,
        *const gentity_t,
        *const c_float, // oritinal: vec3_t
        *const c_float, // original: vec3_t
        c_int,
        c_int,
        c_int,
    );
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn ShiNQlx_Touch_Item(
    ent: *mut gentity_t,
    other: *mut gentity_t,
    trace: *mut trace_t,
) {
    unsafe {
        if ent.as_ref().unwrap().parent == other {
            return;
        }
        Touch_Item(ent, other, trace);
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn ShiNQlx_Switch_Touch_Item(ent: *mut gentity_t) {
    unsafe {
        let ref_mut_ent = ent.as_mut().unwrap();
        ref_mut_ent.touch = Some(Touch_Item);
        ref_mut_ent.think = Some(G_FreeEntity);
        ref_mut_ent.nextthink = CurrentLevel::default().get_leveltime() + 29000;
    }
}

impl GameEntity {
    pub fn get_client_id(&self) -> i32 {
        // we really should be using .offset_from here, but rust's optimizations above level 0 led to some mis-calculations, so we mimic the raw C-calculation.
        unsafe {
            (((self.gentity_t as *const gentity_t as usize) - (g_entities as usize))
                / mem::size_of::<gentity_t>()) as i32
        }
    }

    pub fn start_kamikaze(&self) {
        unsafe { G_StartKamikaze(self.gentity_t as *const gentity_t) }
    }

    pub(crate) fn get_player_name(&self) -> String {
        if self.gentity_t.client.is_null() {
            return "".into();
        }
        if unsafe { (*self.gentity_t.client).pers.connected } == CON_DISCONNECTED {
            return "".into();
        }

        unsafe {
            CStr::from_ptr(
                self.gentity_t
                    .client
                    .as_ref()
                    .unwrap()
                    .pers
                    .netname
                    .as_ptr(),
            )
            .to_string_lossy()
            .to_string()
        }
    }

    pub(crate) fn get_team(&self) -> i32 {
        if self.gentity_t.client.is_null() {
            return TEAM_SPECTATOR as i32;
        }
        if unsafe { (*self.gentity_t.client).pers.connected } == CON_DISCONNECTED {
            return TEAM_SPECTATOR as i32;
        }

        unsafe { self.gentity_t.client.as_ref().unwrap().sess.sessionTeam as i32 }
    }

    pub(crate) fn get_privileges(&self) -> i32 {
        if self.gentity_t.client.is_null() {
            return -1;
        }

        unsafe { self.gentity_t.client.as_ref().unwrap().sess.privileges as i32 }
    }

    pub fn get_game_client(&self) -> Option<GameClient> {
        self.gentity_t.client.try_into().ok()
    }

    pub fn get_activator(&self) -> Option<Activator> {
        self.gentity_t.activator.try_into().ok()
    }

    pub fn get_health(&self) -> i32 {
        self.gentity_t.health
    }

    pub fn set_health(&mut self, new_health: i32) {
        self.gentity_t.health = new_health as c_int;
    }

    pub(crate) fn slay_with_mod(&mut self, mean_of_death: i32) {
        let damage = self.get_health()
            + if mean_of_death == MOD_KAMIKAZE as i32 {
                100000
            } else {
                0
            };

        self.get_game_client().unwrap().set_armor(0);
        unsafe {
            G_Damage(
                self.gentity_t,
                self.gentity_t,
                self.gentity_t,
                std::ptr::null(),
                std::ptr::null(),
                damage * 2,
                DAMAGE_NO_PROTECTION as c_int,
                mean_of_death,
            );
        }
    }

    pub fn in_use(&self) -> bool {
        self.gentity_t.inuse.into()
    }

    pub(crate) fn is_respawning_weapon(&self) -> bool {
        unsafe {
            self.gentity_t.s.eType == ET_ITEM as i32
                && !self.gentity_t.item.is_null()
                && self.gentity_t.item.as_ref().unwrap().giType == IT_WEAPON
        }
    }

    pub(crate) fn set_respawn_time(&mut self, respawn_time: i32) {
        self.gentity_t.wait = respawn_time as c_float;
    }

    pub fn is_dropped_item(&self) -> bool {
        self.gentity_t.flags.bitand(FL_DROPPED_ITEM as i32) == 1
    }

    pub fn get_client_number(&self) -> i32 {
        self.gentity_t.s.clientNum
    }

    pub(crate) fn drop_holdable(&mut self) {
        let angle = self.gentity_t.s.apos.trBase[1] * (PI * 2.0 / 360.0);
        let velocity = [150.0 * angle.cos(), 150.0 * angle.sin(), 250.0];
        unsafe {
            let entity = LaunchItem(
                bg_itemlist.offset(
                    self.gentity_t.client.as_ref().unwrap().ps.stats[STAT_HOLDABLE_ITEM as usize]
                        as isize,
                ),
                self.gentity_t.s.pos.trBase,
                velocity,
            )
            .cast_mut();
            let mut_ref_entity = entity.as_mut().unwrap();
            mut_ref_entity.touch = Some(ShiNQlx_Touch_Item);
            mut_ref_entity.parent = self.gentity_t;
            mut_ref_entity.think = Some(ShiNQlx_Switch_Touch_Item);
            let current_level = CurrentLevel::default();
            mut_ref_entity.nextthink = current_level.get_leveltime() + 1000;
            mut_ref_entity.s.pos.trTime = current_level.get_leveltime() - 500;

            self.gentity_t.client.as_mut().unwrap().ps.stats[STAT_HOLDABLE_ITEM as usize] = 0;
        }
    }

    pub(crate) fn is_kamikaze_timer(&self) -> bool {
        unsafe {
            CStr::from_ptr(self.gentity_t.classname)
                == CString::new("kamikaze timer").unwrap().as_c_str()
        }
    }

    pub(crate) fn free_entity(&mut self) {
        unsafe {
            G_FreeEntity(self.gentity_t);
        }
    }

    pub(crate) fn spawn_item(item_id: i32, origin: (i32, i32, i32)) {
        let origin_vec = [
            origin.0 as c_float,
            origin.1 as c_float,
            origin.2 as c_float,
        ];
        let velocity = [0.0, 0.0, 0.9];

        #[allow(clippy::zero_ptr)]
        unsafe {
            let ent = LaunchItem(bg_itemlist.offset(item_id as isize), origin_vec, velocity)
                as *mut gentity_t;
            let mut_ref_ent = ent.as_mut().unwrap();
            mut_ref_ent.nextthink = 0;
            mut_ref_ent.think = None;
            G_AddEvent(ent, EV_ITEM_RESPAWN, 0); // make item be scaled up
        }
    }
}

pub(crate) struct Activator {
    activator: &'static gentity_t,
}

impl TryFrom<*mut gentity_t> for Activator {
    type Error = &'static str;

    fn try_from(game_entity: *mut gentity_t) -> Result<Self, Self::Error> {
        unsafe {
            game_entity
                .as_ref()
                .map(|gentity| Self { activator: gentity })
                .ok_or("null pointer passed")
        }
    }
}

impl Activator {
    pub fn get_owner_num(&self) -> i32 {
        self.activator.r.ownerNum
    }
}

pub(crate) struct CVar {
    cvar: &'static cvar_t,
}

impl TryFrom<*const cvar_t> for CVar {
    type Error = &'static str;

    fn try_from(cvar: *const cvar_t) -> Result<Self, Self::Error> {
        unsafe {
            cvar.as_ref()
                .map(|cvar| Self { cvar })
                .ok_or("null pointer passed")
        }
    }
}

impl CVar {
    pub(crate) fn get_string(&self) -> Cow<'_, str> {
        unsafe { CStr::from_ptr(self.cvar.string).to_string_lossy() }
    }

    pub(crate) fn get_integer(&self) -> i32 {
        self.cvar.integer
    }

    #[cfg(feature = "cembed")]
    pub(crate) fn get_cvar(&self) -> &cvar_t {
        self.cvar
    }
}

pub(crate) struct Client {
    client_t: &'static client_t,
}

impl TryFrom<*const client_t> for Client {
    type Error = &'static str;

    fn try_from(client: *const client_t) -> Result<Self, Self::Error> {
        unsafe {
            client
                .as_ref()
                .map(|client_t| Self { client_t })
                .ok_or("null pointer passed")
        }
    }
}

extern "C" {
    static svs: *mut serverStatic_t;
}

impl TryFrom<i32> for Client {
    type Error = &'static str;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        if client_id < 0 {
            return Err("invalid client_id");
        }
        unsafe {
            svs.as_ref()
                .unwrap()
                .clients
                .offset(client_id as isize)
                .as_ref()
                .map(|client| Self { client_t: client })
                .ok_or("client not found")
        }
    }
}

extern "C" {
    static SV_DropClient: extern "C" fn(*const client_t, *const c_char);
}

impl Client {
    pub(crate) fn get_client_id(&self) -> i32 {
        unsafe {
            (self.client_t as *const client_t).offset_from(svs.as_ref().unwrap().clients) as i32
        }
    }

    pub(crate) fn get_state(&self) -> i32 {
        self.client_t.state as i32
    }

    pub(crate) fn has_gentity(&self) -> bool {
        !self.client_t.gentity.is_null()
    }

    pub(crate) fn disconnect(&self, reason: &str) {
        let c_reason = CString::new(reason).unwrap().into_raw();
        unsafe {
            SV_DropClient(self.client_t, c_reason);
        }
    }

    pub(crate) fn get_name(&self) -> Cow<'static, str> {
        unsafe { CStr::from_ptr(&self.client_t.name as *const c_char).to_string_lossy() }
    }

    pub(crate) fn get_user_info(&self) -> Cow<str> {
        unsafe { CStr::from_ptr(self.client_t.userinfo.as_ptr()).to_string_lossy() }
    }

    pub(crate) fn get_steam_id(&self) -> u64 {
        self.client_t.steam_id
    }

    pub(crate) fn set_vote(&self, yes_or_no: bool) {
        if let Ok(game_entity) = GameEntity::try_from(self.get_client_id()) {
            unsafe {
                game_entity
                    .gentity_t
                    .client
                    .as_mut()
                    .unwrap()
                    .pers
                    .voteState = if yes_or_no { VOTE_YES } else { VOTE_NO };
            }
        };
    }
}

extern "C" {
    static level: *mut level_locals_t;
}

pub(crate) struct CurrentLevel {
    level: &'static mut level_locals_t,
}

impl Default for CurrentLevel {
    fn default() -> Self {
        unsafe {
            Self {
                level: level.as_mut().unwrap(),
            }
        }
    }
}

impl CurrentLevel {
    pub(crate) fn get_vote_time(&self) -> Option<i32> {
        if self.level.voteTime <= 0 {
            None
        } else {
            Some(self.level.voteTime)
        }
    }

    pub(crate) fn get_leveltime(&self) -> i32 {
        self.level.time
    }

    pub(crate) fn callvote(&mut self, vote: &str, vote_disp: &str) {
        let vote_time = 30;
        for (dest, src) in self
            .level
            .voteString
            .iter_mut()
            .zip(CString::new(vote).unwrap().as_bytes_with_nul().iter())
        {
            *dest = *src as _;
        }
        for (dest, src) in self
            .level
            .voteDisplayString
            .iter_mut()
            .zip(CString::new(vote_disp).unwrap().as_bytes_with_nul().iter())
        {
            *dest = *src as _;
        }
        self.level.voteTime = self.level.time - 30000 + vote_time * 1000;
        self.level.voteYes = 0;
        self.level.voteNo = 0;

        let maxclients = unsafe { SV_MAXCLIENTS };
        for client_id in 0..maxclients {
            if let Ok(game_entity) = GameEntity::try_from(client_id) {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_vote_pending();
            }
        }

        shinqlx_set_configstring(CS_VOTE_STRING as i32, vote_disp);
        shinqlx_set_configstring(
            CS_VOTE_TIME as i32,
            format!("{}", self.level.voteTime).as_str(),
        );
        shinqlx_set_configstring(CS_VOTE_YES as i32, "0");
        shinqlx_set_configstring(CS_VOTE_NO as i32, "0");
    }

    pub(crate) fn set_training_map(&mut self, is_training_map: bool) {
        self.level.mapIsTrainingMap = is_training_map.into();
    }
}

pub(crate) struct QuakeLiveEngine {}

extern "C" {
    static Cvar_FindVar: extern "C" fn(*const c_char) -> *const cvar_t;
}

pub(crate) trait FindCVar {
    fn find_cvar(name: &str) -> Option<CVar>;
}

impl FindCVar for QuakeLiveEngine {
    fn find_cvar(name: &str) -> Option<CVar> {
        let c_name = CString::new(name).unwrap().into_raw();
        unsafe { CVar::try_from(Cvar_FindVar(c_name)).ok() }
    }
}

extern "C" {
    static Cbuf_ExecuteText: extern "C" fn(cbufExec_t, *const c_char);
}

pub(crate) trait CbufExecuteText {
    fn cbuf_execute_text(exec_t: cbufExec_t, new_tags: &str);
}

impl CbufExecuteText for QuakeLiveEngine {
    fn cbuf_execute_text(exec_t: cbufExec_t, new_tags: &str) {
        let c_tags = CString::new(new_tags).unwrap().into_raw();
        unsafe { Cbuf_ExecuteText(exec_t, c_tags) }
    }
}

extern "C" {
    static Cmd_AddCommand: extern "C" fn(*const c_char, *const c_void);
}

pub(crate) trait AddCommand {
    fn add_command(cmd: &str, func: unsafe extern "C" fn());
}

impl AddCommand for QuakeLiveEngine {
    fn add_command(cmd: &str, func: unsafe extern "C" fn()) {
        let c_cmd = CString::new(cmd).unwrap().into_raw();
        unsafe { Cmd_AddCommand(c_cmd, func as *const c_void) }
    }
}

extern "C" {
    static Sys_SetModuleOffset: extern "C" fn(*const c_char, *const c_void);
}

pub(crate) trait SetModuleOffset {
    fn set_module_offset(module_name: &str, offset: unsafe extern "C" fn());
}

impl SetModuleOffset for QuakeLiveEngine {
    fn set_module_offset(module_name: &str, offset: unsafe extern "C" fn()) {
        let c_module_name = CString::new(module_name).unwrap().into_raw();
        unsafe { Sys_SetModuleOffset(c_module_name, offset as *const c_void) }
    }
}

extern "C" {
    static G_InitGame: extern "C" fn(c_int, c_int, c_int);
}

pub(crate) trait InitGame {
    fn init_game(level_time: i32, random_seed: i32, restart: i32);
}

impl InitGame for QuakeLiveEngine {
    fn init_game(level_time: i32, random_seed: i32, restart: i32) {
        unsafe { G_InitGame(level_time, random_seed, restart) }
    }
}

extern "C" {
    static SV_ExecuteClientCommand: extern "C" fn(*const client_t, *const c_char, qboolean);
}

pub(crate) trait ExecuteClientCommand {
    fn execute_client_command(client: Option<&Client>, cmd: &str, client_ok: bool);
}

impl ExecuteClientCommand for QuakeLiveEngine {
    fn execute_client_command(client: Option<&Client>, cmd: &str, client_ok: bool) {
        let command_native = CString::new(cmd).unwrap().into_raw();
        match client {
            Some(safe_client) => unsafe {
                SV_ExecuteClientCommand(safe_client.client_t, command_native, client_ok.into())
            },
            None => unsafe {
                SV_ExecuteClientCommand(std::ptr::null(), command_native, client_ok.into())
            },
        }
    }
}

extern "C" {
    static SV_SendServerCommand: extern "C" fn(*const client_t, *const c_char);
}

pub(crate) trait SendServerCommand {
    fn send_server_command(client: Option<Client>, command: &str);
}

impl SendServerCommand for QuakeLiveEngine {
    fn send_server_command(client: Option<Client>, command: &str) {
        let command_native = CString::new(command).unwrap().into_raw();
        match client {
            Some(safe_client) => unsafe {
                SV_SendServerCommand(safe_client.client_t, command_native)
            },
            None => unsafe { SV_SendServerCommand(std::ptr::null(), command_native) },
        }
    }
}

extern "C" {
    static SV_ClientEnterWorld: extern "C" fn(*const client_t, *const usercmd_t);
}

pub(crate) trait ClientEnterWorld {
    fn client_enter_world(client: &Client, cmd: *const usercmd_t);
}

impl ClientEnterWorld for QuakeLiveEngine {
    fn client_enter_world(client: &Client, cmd: *const usercmd_t) {
        unsafe { SV_ClientEnterWorld(client.client_t, cmd) }
    }
}

extern "C" {
    static SV_SetConfigstring: extern "C" fn(c_int, *const c_char);
}

pub(crate) trait SetConfigstring {
    fn set_configstring(index: &i32, value: &str);
}

impl SetConfigstring for QuakeLiveEngine {
    fn set_configstring(index: &i32, value: &str) {
        if let Ok(c_value) = CString::new(value) {
            unsafe { SV_SetConfigstring(index.to_owned(), c_value.into_raw()) }
        }
    }
}

extern "C" {
    static Com_Printf: extern "C" fn(*const c_char);
}

pub(crate) trait ComPrintf {
    fn com_printf(msg: &str);
}

impl ComPrintf for QuakeLiveEngine {
    fn com_printf(msg: &str) {
        let c_msg = CString::new(msg).unwrap().into_raw();
        unsafe { Com_Printf(c_msg) }
    }
}

extern "C" {
    static SV_SpawnServer: extern "C" fn(*const c_char, qboolean);
}

pub(crate) trait SpawnServer {
    fn spawn_server(server: &str, kill_bots: bool);
}

impl SpawnServer for QuakeLiveEngine {
    fn spawn_server(server: &str, kill_bots: bool) {
        let c_server = CString::new(server).unwrap().into_raw();
        unsafe { SV_SpawnServer(c_server, kill_bots.into()) }
    }
}

extern "C" {
    static G_RunFrame: extern "C" fn(c_int);
}

pub(crate) trait RunFrame {
    fn run_frame(time: i32);
}

impl RunFrame for QuakeLiveEngine {
    fn run_frame(time: i32) {
        unsafe {
            G_RunFrame(time);
        }
    }
}

extern "C" {
    static ClientConnect: extern "C" fn(c_int, qboolean, qboolean) -> *const c_char;
}

pub(crate) trait ClientConnect {
    fn client_connect(client_num: i32, first_time: bool, is_bot: bool) -> Option<String>;
}

impl ClientConnect for QuakeLiveEngine {
    fn client_connect(client_num: i32, first_time: bool, is_bot: bool) -> Option<String> {
        unsafe {
            let c_return = ClientConnect(client_num, first_time.into(), is_bot.into());
            if c_return.is_null() {
                return None;
            }
            Some(CStr::from_ptr(c_return).to_string_lossy().to_string())
        }
    }
}

extern "C" {
    static ClientSpawn: extern "C" fn(*const gentity_t);
}

pub(crate) trait ClientSpawn {
    fn client_spawn(ent: &GameEntity);
}

impl ClientSpawn for QuakeLiveEngine {
    fn client_spawn(ent: &GameEntity) {
        unsafe {
            ClientSpawn(ent.gentity_t);
        }
    }
}

extern "C" {
    static Cmd_Args: extern "C" fn() -> *const c_char;
}

pub(crate) trait CmdArgs {
    fn cmd_args() -> Option<&'static str>;
}

impl CmdArgs for QuakeLiveEngine {
    fn cmd_args() -> Option<&'static str> {
        unsafe { CStr::from_ptr(Cmd_Args()).to_str().ok() }
    }
}

extern "C" {
    static Cmd_Argc: extern "C" fn() -> c_int;
}

pub(crate) trait CmdArgc {
    fn cmd_argc() -> i32;
}

impl CmdArgc for QuakeLiveEngine {
    fn cmd_argc() -> i32 {
        unsafe { Cmd_Argc() }
    }
}

extern "C" {
    static Cmd_Argv: extern "C" fn(c_int) -> *const c_char;
}

pub(crate) trait CmdArgv {
    fn cmd_argv(argno: i32) -> Option<&'static str>;
}

impl CmdArgv for QuakeLiveEngine {
    fn cmd_argv(argno: i32) -> Option<&'static str> {
        if argno < 0 {
            None
        } else {
            unsafe { CStr::from_ptr(Cmd_Argv(argno)).to_str().ok() }
        }
    }
}

extern "C" {
    static G_AddEvent: extern "C" fn(*const gentity_t, entity_event_t, c_int);
}

pub(crate) trait GameAddEvent {
    fn game_add_event(game_entity: &GameEntity, event: entity_event_t, event_param: i32);
}

impl GameAddEvent for QuakeLiveEngine {
    fn game_add_event(game_entity: &GameEntity, event: entity_event_t, event_param: i32) {
        unsafe {
            G_AddEvent(
                game_entity.gentity_t as *const gentity_t,
                event,
                event_param,
            )
        }
    }
}

extern "C" {
    static Cmd_ExecuteString: extern "C" fn(*const c_char);
}

pub(crate) trait ConsoleCommand {
    fn execute_console_command(cmd: &str);
}

impl ConsoleCommand for QuakeLiveEngine {
    fn execute_console_command(cmd: &str) {
        let c_cmd = CString::new(cmd).unwrap().into_raw();
        unsafe { Cmd_ExecuteString(c_cmd) }
    }
}

extern "C" {
    static Cvar_Get: extern "C" fn(*const c_char, *const c_char, c_int) -> *const cvar_t;
}

pub(crate) trait SetCVar {
    fn set_cvar(name: &str, value: &str, flags: Option<i32>) -> Option<CVar>;
}

impl SetCVar for QuakeLiveEngine {
    fn set_cvar(name: &str, value: &str, flags: Option<i32>) -> Option<CVar> {
        let c_name = CString::new(name).unwrap().into_raw();
        let c_value = CString::new(value).unwrap().into_raw();
        let flags_value = flags.unwrap_or_default();
        unsafe { CVar::try_from(Cvar_Get(c_name, c_value, flags_value)).ok() }
    }
}

extern "C" {
    static Cvar_Set2: extern "C" fn(*const c_char, *const c_char, qboolean) -> *const cvar_t;
}

pub(crate) trait SetCVarForced {
    fn set_cvar_forced(name: &str, value: &str, forced: bool) -> Option<CVar>;
}

impl SetCVarForced for QuakeLiveEngine {
    fn set_cvar_forced(name: &str, value: &str, forced: bool) -> Option<CVar> {
        let c_name = CString::new(name).unwrap().into_raw();
        let c_value = CString::new(value).unwrap().into_raw();
        unsafe { CVar::try_from(Cvar_Set2(c_name, c_value, forced.into())).ok() }
    }
}

extern "C" {
    static Cvar_GetLimit: extern "C" fn(
        *const c_char,
        *const c_char,
        *const c_char,
        *const c_char,
        c_int,
    ) -> *const cvar_t;
}

pub(crate) trait SetCVarLimit {
    fn set_cvar_limit(
        name: &str,
        value: &str,
        min: &str,
        max: &str,
        flags: Option<i32>,
    ) -> Option<CVar>;
}

impl SetCVarLimit for QuakeLiveEngine {
    fn set_cvar_limit(
        name: &str,
        value: &str,
        min: &str,
        max: &str,
        flags: Option<i32>,
    ) -> Option<CVar> {
        let c_name = CString::new(name).unwrap().into_raw();
        let c_value = CString::new(value).unwrap().into_raw();
        let c_min = CString::new(min).unwrap().into_raw();
        let c_max = CString::new(max).unwrap().into_raw();
        let flags_value = flags.unwrap_or_default();
        unsafe { CVar::try_from(Cvar_GetLimit(c_name, c_value, c_min, c_max, flags_value)).ok() }
    }
}

extern "C" {
    static SV_GetConfigstring: extern "C" fn(c_int, *mut c_char, c_int);
}

pub(crate) trait GetConfigstring {
    fn get_configstring(index: i32) -> String;
}

impl GetConfigstring for QuakeLiveEngine {
    fn get_configstring(index: i32) -> String {
        let mut buffer: [u8; 4096] = [0; 4096];
        unsafe {
            SV_GetConfigstring(
                index,
                buffer.as_mut_ptr() as *mut c_char,
                buffer.len() as c_int,
            );
        }
        CStr::from_bytes_until_nul(&buffer)
            .unwrap()
            .to_string_lossy()
            .to_string()
    }
}
