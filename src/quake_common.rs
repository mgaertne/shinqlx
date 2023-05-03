use crate::hooks::shinqlx_set_configstring;
use crate::quake_common::clientConnected_t::CON_DISCONNECTED;
use crate::quake_common::persistantFields_t::PERS_ROUND_SCORE;
use crate::quake_common::powerup_t::{
    PW_BATTLESUIT, PW_HASTE, PW_INVIS, PW_INVULNERABILITY, PW_QUAD, PW_REGEN,
};
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
pub(crate) const CS_SCORES1: i32 = 6;
#[allow(dead_code)]
pub(crate) const CS_SCORES2: i32 = 7;
#[allow(dead_code)]
pub(crate) const CS_VOTE_TIME: i32 = 8;
#[allow(dead_code)]
pub(crate) const CS_VOTE_STRING: i32 = 9;
#[allow(dead_code)]
pub(crate) const CS_VOTE_YES: i32 = 10;
#[allow(dead_code)]
pub(crate) const CS_VOTE_NO: i32 = 11;
#[allow(dead_code)]
pub(crate) const CS_ITEMS: i32 = 15;

#[allow(dead_code)]
pub(crate) const MAX_CLIENTS: isize = 64;
pub(crate) const MAX_CHALLENGES: usize = 1024;
pub(crate) const MAX_MSGLEN: usize = 16384;
#[allow(dead_code)]
pub(crate) const MAX_PS_EVENTS: usize = 2;
pub(crate) const MAX_MAP_AREA_BYTES: usize = 32; // bit vector of area visibility
pub(crate) const MAX_INFO_STRING: usize = 1024;
pub(crate) const MAX_RELIABLE_COMMANDS: usize = 64; // max string commands buffered for restransmit
pub(crate) const MAX_STRING_CHARS: usize = 1024; // max length of a string passed to Cmd_TokenizeString
pub(crate) const MAX_NAME_LENGTH: usize = 32; // max length of a client name
pub(crate) const MAX_QPATH: usize = 64; // max length of a quake game pathname
pub(crate) const MAX_DOWNLOAD_WINDOW: usize = 8; // max of eight download frames
#[allow(dead_code)]
pub(crate) const MAX_NETNAME: usize = 36;
pub(crate) const PACKET_BACKUP: usize = 32; // number of old messages that must be kept on client and
                                            // server for delta comrpession and ping estimation
#[allow(dead_code)]
pub(crate) const PACKET_MASK: usize = PACKET_BACKUP - 1;
#[allow(dead_code)]
pub(crate) const MAX_ENT_CLUSTERS: usize = 16;
#[allow(dead_code)]
pub(crate) const MAX_MODELS: usize = 256; // these are sent over the net as 8 bits
pub(crate) const MAX_CONFIGSTRINGS: usize = 1024;
#[allow(dead_code)]
pub(crate) const GENTITYNUM_BITS: usize = 10; // don't need to send any more
#[allow(dead_code)]
pub(crate) const MAX_GENTITIES: usize = 1 << GENTITYNUM_BITS;
#[allow(dead_code)]
pub(crate) const MAX_ITEM_MODELS: usize = 4;
#[allow(dead_code)]
pub(crate) const MAX_SPAWN_VARS: usize = 64;
#[allow(dead_code)]
pub(crate) const MAX_SPAWN_VARS_CHARS: usize = 4096;
#[allow(dead_code)]
pub(crate) const BODY_QUEUE_SIZE: usize = 8;

// bit field limits
#[allow(dead_code)]
pub(crate) const MAX_STATS: usize = 16;
#[allow(dead_code)]
pub(crate) const MAX_PERSISTANT: usize = 16;
#[allow(dead_code)]
pub(crate) const MAX_POWERUPS: usize = 16;
#[allow(dead_code)]
pub(crate) const MAX_WEAPONS: usize = 16;

// Button flags
#[allow(dead_code)]
pub(crate) const BUTTON_ATTACK: usize = 1;
#[allow(dead_code)]
pub(crate) const BUTTON_TALK: usize = 2; // displkays talk balloon and disables actions
#[allow(dead_code)]
pub(crate) const BUTTON_USE_HOLDABLE: usize = 4; // Mino +button2
#[allow(dead_code)]
pub(crate) const BUTTON_GESTURE: usize = 8; // Mino: +button3
#[allow(dead_code)]
pub(crate) const BUTTON_WALKING: usize = 16;
// Block of unused button flags, or at least flags I couldn't trigger.
// Q3 used them for bot commands, so probably unused in QL.
#[allow(dead_code)]
pub(crate) const BUTTON_UNUSED1: usize = 32;
#[allow(dead_code)]
pub(crate) const BUTTON_UNUSED2: usize = 64;
#[allow(dead_code)]
pub(crate) const BUTTON_UNUSED3: usize = 128;
#[allow(dead_code)]
pub(crate) const BUTTON_UNUSED4: usize = 256;
#[allow(dead_code)]
pub(crate) const BUTTON_UNUSED5: usize = 512;
#[allow(dead_code)]
pub(crate) const BUTTON_UNUSED6: usize = 1024;
#[allow(dead_code)]
pub(crate) const BUTTON_UPMOVE: usize = 2048; // Mino: Not in Q3. I'm guessing it's for cg_autohop.
#[allow(dead_code)]
pub(crate) const BUTTON_ANY: usize = 4096; // any key whatsoever
#[allow(dead_code)]
pub(crate) const BUTTON_IS_ACTIVE: usize = 65536; // Mino: No idea what it is, but it goes off after a while of being
                                                  //       AFK, then goes on after being active for a while.

// eflags
#[allow(dead_code)]
pub(crate) const EF_DEAD: c_int = 0x00000001; // don't draw a foe marker over players with EF_DEAD
#[allow(dead_code)]
pub(crate) const EF_TICKING: c_int = 0x00000002; // used to make players play the prox mine ticking sound
#[allow(dead_code)]
pub(crate) const EF_TELEPORT_BIT: c_int = 0x00000004; // toggled every time the origin abruptly changes
#[allow(dead_code)]
pub(crate) const EF_AWARD_EXCELLENT: c_int = 0x00000008; // draw an excellent sprite
#[allow(dead_code)]
pub(crate) const EF_PLAYER_EVENT: c_int = 0x00000010;
#[allow(dead_code)]
pub(crate) const EF_BOUNCE: c_int = 0x00000010; // for missiles
#[allow(dead_code)]
pub(crate) const EF_BOUNCE_HALF: c_int = 0x00000020; // for missiles
#[allow(dead_code)]
pub(crate) const EF_AWARD_GAUNTLET: c_int = 0x00000040; // draw a gauntlet sprite
#[allow(dead_code)]
pub(crate) const EF_NODRAW: c_int = 0x00000080; // may have an event, but no model (unspawned items)
#[allow(dead_code)]
pub(crate) const EF_FIRING: c_int = 0x00000100; // for lightning gun
pub(crate) const EF_KAMIKAZE: c_int = 0x00000200;
#[allow(dead_code)]
pub(crate) const EF_MOVER_STOP: c_int = 0x0000400; // will push otherwise
#[allow(dead_code)]
pub(crate) const EF_AWARD_CAP: c_int = 0x00000800; // draw the capture sprite
#[allow(dead_code)]
pub(crate) const EF_TALK: c_int = 0x00001000; // draw a talk balloon
#[allow(dead_code)]
pub(crate) const EF_CONNECTION: c_int = 0x00002000; // draw a connection trouble sprite
#[allow(dead_code)]
pub(crate) const EF_VOTED: c_int = 0x00004000; // already cast a vote
#[allow(dead_code)]
pub(crate) const EF_AWARD_IMPRESSIVE: c_int = 0x00008000; // draw an impressive sprite
#[allow(dead_code)]
pub(crate) const EF_AWARD_DEFEND: c_int = 0x00010000; // draw a defend sprite
#[allow(dead_code)]
pub(crate) const EF_AWARD_ASSIST: c_int = 0x00020000; // draw a assist sprite
#[allow(dead_code)]
pub(crate) const EF_AWARD_DENIED: c_int = 0x00040000; // denied
#[allow(dead_code)]
pub(crate) const EF_TEAMVOTED: c_int = 0x00080000; // already cast a team vote

#[allow(dead_code)]
pub(crate) const FL_DROPPED_ITEM: c_int = 0x00001000;

#[allow(dead_code)]
pub(crate) const DAMAGE_NO_PROTECTION: c_int = 0x00000008;

#[allow(non_camel_case_types)]
pub type byte = u8;

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[repr(C)]
pub enum qboolean {
    qfalse,
    qtrue,
}

impl From<qboolean> for c_int {
    fn from(value: qboolean) -> Self {
        match value {
            qboolean::qtrue => 1,
            qboolean::qfalse => 0,
        }
    }
}

impl From<qboolean> for bool {
    fn from(value: qboolean) -> Self {
        match value {
            qboolean::qtrue => true,
            qboolean::qfalse => false,
        }
    }
}

impl From<bool> for qboolean {
    fn from(value: bool) -> Self {
        match value {
            true => qboolean::qtrue,
            false => qboolean::qfalse,
        }
    }
}

impl Not for qboolean {
    type Output = qboolean;

    fn not(self) -> Self::Output {
        match self {
            qboolean::qtrue => qboolean::qfalse,
            qboolean::qfalse => qboolean::qtrue,
        }
    }
}

// paramters for command buffer stuffing
#[allow(non_camel_case_types)]
#[derive(PartialEq, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum cbufExec_t {
    EXEC_NOW, // don't return until completed, a VM should NEVER use this,
    // because some commands might cause the VM to be unloaded...
    EXEC_INSERT, // insert at current position, but don't run yet
    EXEC_APPEND, // add to end of the command buffer (normal case)
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum clientState_t {
    CS_FREE,   // can be reused for a new connection
    CS_ZOMBIE, // client has been disconnected, but don't reuse
    // connection for a couple seconds
    CS_CONNECTED, // has been assigned to a client_t, but no gamestate yet
    CS_PRIMED,    // gamestate has been sent, but client hasn't sent a usercmd
    CS_ACTIVE,    // client is fully in game
}

#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum roundStateState_t {
    PREGAME = 0x0,
    ROUND_WARMUP = 0x1,
    ROUND_SHUFFLE = 0x2,
    ROUND_BEGUN = 0x3,
    ROUND_OVER = 0x4,
    POSTGAME = 0x5,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum statIndex_t {
    STAT_HEALTH,
    STAT_HOLDABLE_ITEM,
    STAT_RUNE,
    STAT_WEAPONS,
    STAT_ARMOR,
    STAT_BSKILL,
    STAT_CLIENTS_READY,
    STAT_MAX_HEALTH,
    STAT_SPINUP,
    STAT_FLIGHT_THRUST,
    STAT_MAX_FLIGHT_FUEL,
    STAT_CUR_FLIGHT_FUEL,
    STAT_FLIGHT_REFUEL,
    STAT_QUADKILLS,
    STAT_ARMORTYPE,
    STAT_KEY,
    STAT_OTHER_HEALTH,
    STAT_OTHER_ARMOR,
}

// movers are things like doors, plats, buttons, etc
#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum moverState_t {
    MOVER_POS1,
    MOVER_POS2,
    MOVER_1TO2,
    MOVER_2TO1,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub(crate) enum persistantFields_t {
    PERS_ROUND_SCORE = 0x0,
    PERS_COMBOKILL_COUNT = 0x1,
    PERS_RAMPAGE_COUNT = 0x2,
    PERS_MIDAIR_COUNT = 0x3,
    PERS_REVENGE_COUNT = 0x4,
    PERS_PERFORATED_COUNT = 0x5,
    PERS_HEADSHOT_COUNT = 0x6,
    PERS_ACCURACY_COUNT = 0x7,
    PERS_QUADGOD_COUNT = 0x8,
    PERS_FIRSTFRAG_COUNT = 0x9,
    PERS_PERFECT_COUNT = 0xA,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum entity_event_t {
    EV_NONE = 0x0,
    EV_FOOTSTEP = 0x1,
    EV_FOOTSTEP_METAL = 0x2,
    EV_FOOTSPLASH = 0x3,
    EV_FOOTWADE = 0x4,
    EV_SWIM = 0x5,
    EV_FALL_SHORT = 0x6,
    EV_FALL_MEDIUM = 0x7,
    EV_FALL_FAR = 0x8,
    EV_JUMP_PAD = 0x9,
    EV_JUMP = 0xA,
    EV_WATER_TOUCH = 0xB,
    EV_WATER_LEAVE = 0xC,
    EV_WATER_UNDER = 0xD,
    EV_WATER_CLEAR = 0xE,
    EV_ITEM_PICKUP = 0xF,
    EV_GLOBAL_ITEM_PICKUP = 0x10,
    EV_NOAMMO = 0x11,
    EV_CHANGE_WEAPON = 0x12,
    EV_DROP_WEAPON = 0x13,
    EV_FIRE_WEAPON = 0x14,
    EV_USE_ITEM0 = 0x15,
    EV_USE_ITEM1 = 0x16,
    EV_USE_ITEM2 = 0x17,
    EV_USE_ITEM3 = 0x18,
    EV_USE_ITEM4 = 0x19,
    EV_USE_ITEM5 = 0x1A,
    EV_USE_ITEM6 = 0x1B,
    EV_USE_ITEM7 = 0x1C,
    EV_USE_ITEM8 = 0x1D,
    EV_USE_ITEM9 = 0x1E,
    EV_USE_ITEM10 = 0x1F,
    EV_USE_ITEM11 = 0x20,
    EV_USE_ITEM12 = 0x21,
    EV_USE_ITEM13 = 0x22,
    EV_USE_ITEM14 = 0x23,
    EV_USE_ITEM15 = 0x24,
    EV_ITEM_RESPAWN = 0x25,
    EV_ITEM_POP = 0x26,
    EV_PLAYER_TELEPORT_IN = 0x27,
    EV_PLAYER_TELEPORT_OUT = 0x28,
    EV_GRENADE_BOUNCE = 0x29,
    EV_GENERAL_SOUND = 0x2A,
    EV_GLOBAL_SOUND = 0x2B,
    EV_GLOBAL_TEAM_SOUND = 0x2C,
    EV_BULLET_HIT_FLESH = 0x2D,
    EV_BULLET_HIT_WALL = 0x2E,
    EV_MISSILE_HIT = 0x2F,
    EV_MISSILE_MISS = 0x30,
    EV_MISSILE_MISS_METAL = 0x31,
    EV_RAILTRAIL = 0x32,
    EV_SHOTGUN = 0x33,
    EV_BULLET = 0x34,
    EV_PAIN = 0x35,
    EV_DEATH1 = 0x36,
    EV_DEATH2 = 0x37,
    EV_DEATH3 = 0x38,
    EV_DROWN = 0x39,
    EV_OBITUARY = 0x3A,
    EV_POWERUP_QUAD = 0x3B,
    EV_POWERUP_BATTLESUIT = 0x3C,
    EV_POWERUP_REGEN = 0x3D,
    EV_POWERUP_ARMORREGEN = 0x3E,
    EV_GIB_PLAYER = 0x3F,
    EV_SCOREPLUM = 0x40,
    EV_PROXIMITY_MINE_STICK = 0x41,
    EV_PROXIMITY_MINE_TRIGGER = 0x42,
    EV_KAMIKAZE = 0x43,
    EV_OBELISKEXPLODE = 0x44,
    EV_OBELISKPAIN = 0x45,
    EV_INVUL_IMPACT = 0x46,
    EV_JUICED = 0x47,
    EV_LIGHTNINGBOLT = 0x48,
    EV_DEBUG_LINE = 0x49,
    EV_TAUNT = 0x4A,
    EV_TAUNT_YES = 0x4B,
    EV_TAUNT_NO = 0x4C,
    EV_TAUNT_FOLLOWME = 0x4D,
    EV_TAUNT_GETFLAG = 0x4E,
    EV_TAUNT_GUARDBASE = 0x4F,
    EV_TAUNT_PATROL = 0x50,
    EV_FOOTSTEP_SNOW = 0x51,
    EV_FOOTSTEP_WOOD = 0x52,
    EV_ITEM_PICKUP_SPEC = 0x53,
    EV_OVERTIME = 0x54,
    EV_GAMEOVER = 0x55,
    EV_MISSILE_MISS_DMGTHROUGH = 0x56,
    EV_THAW_PLAYER = 0x57,
    EV_THAW_TICK = 0x58,
    EV_SHOTGUN_KILL = 0x59,
    EV_POI = 0x5A,
    EV_DEBUG_HITBOX = 0x5B,
    EV_LIGHTNING_DISCHARGE = 0x5C,
    EV_RACE_START = 0x5D,
    EV_RACE_CHECKPOINT = 0x5E,
    EV_RACE_FINISH = 0x5F,
    EV_DAMAGEPLUM = 0x60,
    EV_AWARD = 0x61,
    EV_INFECTED = 0x62,
    EV_NEW_HIGH_SCORE = 0x63,
    EV_NUM_ETYPES = 0x64,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum itemType_t {
    IT_BAD,
    IT_WEAPON,  // EFX: rotate + upscale + minlight
    IT_AMMO,    // EFX: rotate
    IT_ARMOR,   // EFX: rotate + minlight
    IT_HEALTH,  // EFX: static external sphere + rotating internal
    IT_POWERUP, // instant on, timer based
    // EFX: rotate + external ring that rotates
    IT_HOLDABLE, // single use, holdable item
    // EFX: rotate + bob
    IT_PERSISTANT_POWERUP,
    IT_TEAM,
}

//noinspection ALL
#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum powerup_t {
    PW_NONE = 0x0,
    //PW_SPAWNARMOR = 0x0,
    PW_REDFLAG = 0x1,
    PW_BLUEFLAG = 0x2,
    PW_NEUTRALFLAG = 0x3,
    PW_QUAD = 0x4,
    PW_BATTLESUIT = 0x5,
    PW_HASTE = 0x6,
    PW_INVIS = 0x7,
    PW_REGEN = 0x8,
    PW_FLIGHT = 0x9,
    PW_INVULNERABILITY = 0xA,
    NOTPW_SCOUT = 0xB,
    NOTPW_GUARD = 0xC,
    NOTPW_DOUBLER = 0xD,
    NOTPW_ARMORREGEN = 0xE,
    PW_FREEZE = 0xF,
    PW_NUM_POWERUPS = 0x10,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct gitem_t {
    classname: *const c_char,
    pickup_sound: *const c_char,
    world_model: [*const c_char; 4],
    premium_model: [*const c_char; 4],
    icon: *const c_char,
    pickup_name: *const c_char,
    quantity: c_int,
    giType: itemType_t,
    giTag: c_int,
    itemTimer: qboolean,
    maskGametypeRenderSkip: c_uint,
    maskGametypeForceSpawn: c_uint,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum clientConnected_t {
    CON_DISCONNECTED,
    CON_CONNECTING,
    CON_CONNECTED,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum voteState_t {
    VOTE_NONE,
    VOTE_PENDING,
    VOTE_YES,
    VOTE_NO,
    VOTE_FORCE_PASS,
    VOTE_FORCE_FAIL,
    VOTE_EXPIRED,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum playerTeamStateState_t {
    TEAM_BEGIN,  // Beginning a team game, spawn at base
    TEAM_ACTIVE, // Now actively playing
}

#[allow(non_camel_case_types)]
#[repr(C)]
struct playerTeamState_t {
    state: playerTeamStateState_t,
    captures: c_int,
    basedefense: c_int,
    carrierdefense: c_int,
    flagrecovery: c_int,
    fragcarrier: c_int,
    assists: c_int,
    flagruntime: c_int,
    flagrunrelays: c_int,
    lasthurtcarrier: c_int,
    lastreturnedflag: c_int,
    lastfraggedcarrier: c_int,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum team_t {
    TEAM_FREE,
    TEAM_RED,
    TEAM_BLUE,
    TEAM_SPECTATOR,

    TEAM_NUM_TEAMS,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum spectatorState_t {
    SPECTATOR_NOT,
    SPECTATOR_FREE,
    SPECTATOR_FOLLOW,
    SPECTATOR_SCOREBOARD,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C, align(8))]
struct clientPersistant_t {
    connected: clientConnected_t,
    cmd: usercmd_t,
    localClient: qboolean,
    initialSpawn: qboolean,
    predictItemPickup: qboolean,
    netname: [c_char; 40],
    country: [c_char; 24],
    steamId: u64,
    maxHealth: c_int,
    voteCount: c_int,
    voteState: voteState_t,
    complaints: c_int,
    complaintClient: c_int,
    complaintEndTime: c_int,
    damageFromTeammates: c_int,
    damageToTeammates: c_int,
    ready: qboolean,
    autoaction: c_int,
    timeouts: c_int,
    enterTime: c_int,
    teamState: playerTeamState_t,
    damageResidual: c_int,
    inactivityTime: c_int,
    inactivityWarning: c_int,
    lastUserinfoUpdate: c_int,
    userInfoFloodInfractions: c_int,
    lastMapVoteTime: c_int,
    lastMapVoteIndex: c_int,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct clientSession_t {
    sessionTeam: team_t,
    spectatorTime: c_int,
    spectatorState: spectatorState_t,
    spectatorClient: c_int,
    weaponPrimary: c_int,
    wins: c_int,
    losses: c_int,
    teamLeader: qboolean,
    privileges: privileges_t,
    specOnly: c_int,
    playQueue: c_int,
    updatePlayQueue: qboolean,
    muted: c_int,
    prevScore: c_int,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum privileges_t {
    PRIV_BANNED = -1,
    PRIV_NONE = 0x0,
    PRIV_MOD = 0x1,
    PRIV_ADMIN = 0x2,
    PRIV_ROOT = 0x3,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct expandedStatObj_t {
    statId: c_uint,
    lastThinkTime: c_int,
    teamJoinTime: c_int,
    totalPlayTime: c_int,
    serverRank: c_int,
    serverRankIsTied: qboolean,
    teamRank: c_int,
    teamRankIsTied: qboolean,
    numKills: c_int,
    numDeaths: c_int,
    numSuicides: c_int,
    numTeamKills: c_int,
    numTeamKilled: c_int,
    numWeaponKills: [c_int; 16],
    numWeaponDeaths: [c_int; 16],
    shotsFired: [c_int; 16],
    shotsHit: [c_int; 16],
    damageDealt: [c_int; 16],
    damageTaken: [c_int; 16],
    powerups: [c_int; 16],
    holdablePickups: [c_int; 7],
    weaponPickups: [c_int; 16],
    weaponUsageTime: [c_int; 16],
    numCaptures: c_int,
    numAssists: c_int,
    numDefends: c_int,
    numHolyShits: c_int,
    totalDamageDealt: c_int,
    totalDamageTaken: c_int,
    previousHealth: c_int,
    previousArmor: c_int,
    numAmmoPickups: c_int,
    numFirstMegaHealthPickups: c_int,
    numMegaHealthPickups: c_int,
    megaHealthPickupTime: c_int,
    numHealthPickups: c_int,
    numFirstRedArmorPickups: c_int,
    numRedArmorPickups: c_int,
    redArmorPickupTime: c_int,
    numFirstYellowArmorPickups: c_int,
    numYellowArmorPickups: c_int,
    yellowArmorPickupTime: c_int,
    numFirstGreenArmorPickups: c_int,
    numGreenArmorPickups: c_int,
    greenArmorPickupTime: c_int,
    numQuadDamagePickups: c_int,
    numQuadDamageKills: c_int,
    numBattleSuitPickups: c_int,
    numRegenerationPickups: c_int,
    numHastePickups: c_int,
    numInvisibilityPickups: c_int,
    numRedFlagPickups: c_int,
    numBlueFlagPickups: c_int,
    numNeutralFlagPickups: c_int,
    numMedkitPickups: c_int,
    numArmorPickups: c_int,
    numDenials: c_int,
    killStreak: c_int,
    maxKillStreak: c_int,
    xp: c_int,
    domThreeFlagsTime: c_int,
    numMidairShotgunKills: c_int,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct raceInfo_t {
    racingActive: qboolean,
    startTime: c_int,
    lastTime: c_int,
    best_race: [c_int; 64],
    current_race: [c_int; 64],
    currentCheckPoint: c_int,
    weaponUsed: qboolean,
    nextRacePoint: *const gentity_t,
    nextRacePoint2: *const gentity_t,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C, align(8))]
struct gclient_t {
    ps: playerState_t,
    pers: clientPersistant_t,
    sess: clientSession_t,
    noclip: qboolean,
    lastCmdTime: c_int,
    buttons: c_int,
    oldbuttons: c_int,
    damage_armor: c_int,
    damage_blood: c_int,
    damage_from: vec3_t,
    damage_fromWorld: qboolean,
    impressiveCount: c_int,
    accuracyCount: c_int,
    accuracy_shots: c_int,
    accuracy_hits: c_int,
    lastClientKilled: c_int,
    lastKilledClient: c_int,
    lastHurtClient: [c_int; 2],
    lastHurtMod: [c_int; 2],
    lastHurtTime: [c_int; 2],
    lastKillTime: c_int,
    lastGibTime: c_int,
    rampageCounter: c_int,
    revengeCounter: [c_int; 64],
    respawnTime: c_int,
    rewardTime: c_int,
    airOutTime: c_int,
    fireHeld: qboolean,
    hook: *const gentity_t,
    switchTeamTime: c_int,
    timeResidual: c_int,
    timeResidualScout: c_int,
    timeResidualArmor: c_int,
    timeResidualHealth: c_int,
    timeResidualPingPOI: c_int,
    timeResidualSpecInfo: c_int,
    healthRegenActive: qboolean,
    armorRegenActive: qboolean,
    persistantPowerup: *const gentity_t,
    portalID: c_int,
    ammoTimes: [c_int; 16],
    invulnerabilityTime: c_int,
    expandedStats: expandedStatObj_t,
    ignoreChatsTime: c_int,
    lastUserCmdTime: c_int,
    freezePlayer: qboolean,
    deferredSpawnTime: c_int,
    deferredSpawnCount: c_int,
    race: raceInfo_t,
    shotgunDmg: [c_int; 64],
    round_shots: c_int,
    round_hits: c_int,
    round_damage: c_int,
    queuedSpectatorFollow: qboolean,
    queuedSpectatorClient: c_int,
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
        self.game_client.ps.eFlags.bitand_assign(!EF_KAMIKAZE);
    }

    pub(crate) fn get_connection_state(&self) -> clientConnected_t {
        self.game_client.pers.connected
    }

    pub(crate) fn get_team(&self) -> i32 {
        self.game_client.sess.sessionTeam as i32
    }

    pub(crate) fn get_player_name(&self) -> Cow<'static, str> {
        unsafe { CStr::from_ptr(self.game_client.pers.netname.as_ptr()).to_string_lossy() }
    }

    pub(crate) fn get_privileges(&self) -> i32 {
        self.game_client.sess.privileges as i32
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
            self.game_client.ps.eFlags.bitor_assign(EF_KAMIKAZE);
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
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct gentity_t {
    s: entityState_t,
    r: entityShared_t,
    client: *mut gclient_t,
    inuse: qboolean,
    classname: *const c_char,
    spawnflags: c_int,
    neverFree: qboolean,
    flags: c_int,
    model: *const c_char,
    model2: *const c_char,
    freetime: c_int,
    eventTime: c_int,
    freeAfterEvent: qboolean,
    unlinkAfterEvent: qboolean,
    physicsObject: qboolean,
    physicsBounce: c_float,
    clipmask: c_int,
    moverState: moverState_t,
    soundPos1: c_int,
    sound1to2: c_int,
    sound2to1: c_int,
    soundPos2: c_int,
    soundLoop: c_int,
    parent: *const gentity_t,
    nextTrain: *const gentity_t,
    prevTrain: *const gentity_t,
    pos1: vec3_t,
    pos2: vec3_t,
    message: *const c_char,
    cvar: *const c_char,
    tourPointTarget: *const c_char,
    tourPointTargetName: *const c_char,
    noise: *const c_char,
    timestamp: c_int,
    angle: c_float,
    target: *const c_char,
    targetname: *const c_char,
    targetShaderName: *const c_char,
    targetShaderNewName: *const c_char,
    target_ent: *const gentity_t,
    speed: c_float,
    movedir: vec3_t,
    nextthink: c_int,
    think: extern "C" fn(*mut gentity_t),
    framethink: extern "C" fn(*const gentity_t) -> c_void,
    reached: extern "C" fn(*const gentity_t) -> c_void,
    blocked: extern "C" fn(*const gentity_t, *const gentity_t) -> c_void,
    touch: extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t),
    _use: extern "C" fn(*const gentity_t, *const gentity_t, *const gentity_t) -> c_void,
    pain: extern "C" fn(*const gentity_t, c_int) -> c_void,
    die:
        extern "C" fn(*const gentity_t, *const gentity_t, *const gentity_t, c_int, c_int) -> c_void,
    pain_debounce_time: c_int,
    fly_sound_debounce_time: c_int,
    health: c_int,
    takedamage: qboolean,
    damage: c_int,
    damageFactor: c_int,
    splashDamage: c_int,
    splashRadius: c_int,
    methodOfDeath: c_int,
    splashMethodOfDeath: c_int,
    count: c_int,
    enemy: *const gentity_t,
    activator: *const gentity_t,
    team: *const c_char,
    teammaster: *const gentity_t,
    teamchain: *const gentity_t,
    kamikazeTime: c_int,
    kamikazeShockTime: c_int,
    watertype: c_int,
    waterlevel: c_int,
    noise_index: c_int,
    bouncecount: c_int,
    wait: c_float,
    random: c_float,
    spawnTime: c_int,
    item: *const gitem_t,
    pickupCount: c_int,
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
    static bg_itemList: *const gitem_t;
    static LaunchItem: extern "C" fn(*const gitem_t, vec3_t, vec3_t) -> *const gentity_t;
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn ShiNQlx_Touch_Item(
    ent: *mut gentity_t,
    other: *mut gentity_t,
    trace: *mut trace_t,
) {
    unsafe {
        if (*ent).parent == other {
            return;
        }
        Touch_Item(ent, other, trace);
    }
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn ShiNQlx_Switch_Touch_Item(ent: *mut gentity_t) {
    unsafe {
        (*ent).touch = Touch_Item;
        (*ent).think = G_FreeEntity;
        (*ent).nextthink = CurrentLevel::default().get_leveltime() + 29000;
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

    pub fn in_use(&self) -> bool {
        self.gentity_t.inuse.into()
    }

    pub fn get_client_number(&self) -> i32 {
        self.gentity_t.s.clientNum
    }

    pub(crate) fn drop_holdable(&mut self) {
        let angle = self.gentity_t.s.apos.trBase[1] * (PI * 2.0 / 360.0);
        let velocity = [150.0 * angle.cos(), 150.0 * angle.sin(), 250.0];
        unsafe {
            let mut entity = LaunchItem(
                bg_itemList.offset(
                    (*self.gentity_t.client).ps.stats[STAT_HOLDABLE_ITEM as usize] as isize,
                ),
                self.gentity_t.s.pos.trBase,
                velocity,
            )
            .cast_mut();
            (*entity).touch = ShiNQlx_Touch_Item;
            (*entity).parent = self.gentity_t;
            (*entity).think = ShiNQlx_Switch_Touch_Item;
            let current_level = CurrentLevel::default();
            (*entity).nextthink = current_level.get_leveltime() + 1000;
            (*entity).s.pos.trTime = current_level.get_leveltime() - 500;

            (*self.gentity_t.client).ps.stats[STAT_HOLDABLE_ITEM as usize] = 0;
        }
    }
}

pub(crate) struct Activator {
    activator: &'static gentity_t,
}

impl TryFrom<*const gentity_t> for Activator {
    type Error = &'static str;

    fn try_from(game_entity: *const gentity_t) -> Result<Self, Self::Error> {
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

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C, align(4))]
pub struct usercmd_t {
    serverTime: c_int,
    angles: [c_int; 3],
    buttons: c_int,
    weapon: byte,
    weaponPrimary: byte,
    fov: byte,
    forwardmove: byte,
    rightmove: byte,
    upmove: byte,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum trType_t {
    TR_STATIONARY,
    TR_INTERPOLATE, // non-parametric, but interpolate between snapshots
    TR_LINEAR,
    TR_LINEAR_STOP,
    TR_SINE, // value = base + sin( time / duration ) * delta
    TR_GRAVITY,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct trajectory_t {
    trType: trType_t,
    trTime: c_int,
    trDuration: c_int,
    trBase: vec3_t,
    trDelta: vec3_t,
    gravity: c_float,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct entityState_t {
    number: c_int,
    eType: c_int,
    eFlags: c_int,
    pos: trajectory_t,
    apos: trajectory_t,
    time: c_int,
    time2: c_int,
    origin: vec3_t,
    origin2: vec3_t,
    angles: vec3_t,
    angles2: vec3_t,
    otherEntityNum: c_int,
    otherEntityNum2: c_int,
    groundEntityNum: c_int,
    constantLight: c_int,
    loopSound: c_int,
    modelindex: c_int,
    modelindex2: c_int,
    clientNum: c_int,
    frame: c_int,
    solid: c_int,
    event: c_int,
    eventParm: c_int,
    powerups: c_int,
    health: c_int,
    armor: c_int,
    weapon: c_int,
    location: c_int,
    legsAnim: c_int,
    torsoAnim: c_int,
    generic1: c_int,
    jumpTime: c_int,
    doubleJumped: c_int,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct entityShared_t {
    s: entityState_t,
    linked: qboolean,
    linkcount: c_int,
    svFlags: c_int,
    singleClient: c_int,
    bmodel: qboolean,
    mins: vec3_t,
    maxs: vec3_t,
    contents: c_int,
    absmin: vec3_t,
    absmax: vec3_t,
    currentOrigin: vec3_t,
    currentAngles: vec3_t,
    ownerNum: c_int,
}

#[allow(non_camel_case_types)]
#[repr(C)]
struct sharedEntity_t {
    s: entityState_t,  // communicated by server to clients
    r: entityShared_t, // shared by both the server system and game
}

#[allow(non_camel_case_types)]
type fileHandle_t = c_int;

#[allow(non_camel_case_types)]
type vec3_t = [c_float; 3];

#[allow(non_camel_case_types)]
#[repr(C)]
struct cplane_t {
    normal: vec3_t,
    dist: c_float,
    ctype: byte,
    signbits: byte,
    pad: [byte; 2],
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct trace_t {
    allsolid: qboolean,
    startsolid: qboolean,
    fraction: c_float,
    endpos: vec3_t,
    plane: cplane_t,
    surfaceFlags: c_int,
    contents: c_int,
    entityNum: c_int,
}

// playerState_t is a full superset of entityState_t as it is used by players,
// so if a playerState_t is transmitted, the entityState_t can be fully derived
// from it.
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct playerState_t {
    commandTime: c_int,
    pm_type: c_int,
    bobCycle: c_int,
    pm_flags: c_int,
    pm_time: c_int,
    origin: vec3_t,
    velocity: vec3_t,
    weaponTime: c_int,
    gravity: c_int,
    speed: c_int,
    delta_angles: [c_int; 3],
    groundEntityNum: c_int,
    legsTimer: c_int,
    legsAnim: c_int,
    torsoTimer: c_int,
    torsoAnim: c_int,
    movementDir: c_int,
    grapplePoint: vec3_t,
    eFlags: c_int,
    eventSequence: c_int,
    events: [c_int; 2],
    eventParms: [c_int; 2],
    externalEvent: c_int,
    externalEventParm: c_int,
    clientNum: c_int,
    location: c_int,
    weapon: c_int,
    weaponPrimary: c_int,
    weaponstate: c_int,
    fov: c_int,
    viewangles: vec3_t,
    viewheight: c_int,
    damageEvent: c_int,
    damageYaw: c_int,
    damagePitch: c_int,
    damageCount: c_int,
    stats: [c_int; 16],
    persistant: [c_int; 16],
    powerups: [c_int; 16],
    ammo: [c_int; 16],
    generic1: c_int,
    loopSound: c_int,
    jumppad_ent: c_int,
    jumpTime: c_int,
    doubleJumped: c_int,
    crouchTime: c_int,
    crouchSlideTime: c_int,
    forwardmove: c_char,
    rightmove: c_char,
    upmove: c_char,
    ping: c_int,
    pmove_framecount: c_int,
    jumppad_frame: c_int,
    entityEventSequence: c_int,
    freezetime: c_int,
    thawtime: c_int,
    thawClientNum_valid: c_int,
    thawClientNum: c_int,
    respawnTime: c_int,
    localPersistant: [c_int; 16],
    roundDamage: c_int,
    roundShots: c_int,
    roundHits: c_int,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct clientSnapshot_t {
    areabytes: c_int,
    areabits: [byte; MAX_MAP_AREA_BYTES], // portalarea visibility bits
    ps: playerState_t,
    num_entities: c_int,
    first_entity: c_int, // into the circular sv_packet_entities[]
    // the entities MUST be in increasing state number
    // order, otherwise the delta compression will fail
    messageSent: c_int,  // time the message was transmitted
    messageAcked: c_int, // time the message was acked
    messageSize: c_int,  // used to rate drop packets
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
enum netsrc_t {
    NS_CLIENT,
    NS_SERVER,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq, Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
enum netadrtype_t {
    NA_BOT,
    NA_BAD, // an address lookup failed
    NA_LOOPBACK,
    NA_BROADCAST,
    NA_IP,
    NA_IPX,
    NA_BROADCAST_IPX,
}
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct netadr_t {
    pub adrtype: netadrtype_t,

    pub ip: [byte; 4],
    pub ipx: [byte; 10],

    pub port: c_ushort,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct netchan_t {
    sock: netsrc_t,

    dropped: c_int, // between last packet and previous

    remoteAddress: netadr_t,
    qport: c_int, // qport value to write when transmitting

    // sequencing variables
    incomingSequence: c_int,
    outgoingSequence: c_int,

    // incoming fragment assembly buffer
    fragmentSequence: c_int,
    fragmentLength: c_int,
    fragmentBuffer: [byte; MAX_MSGLEN],

    // outgoing fragment buffer
    // we need to space out the sending of large fragmented messages
    unsentFragments: qboolean,
    unsentFragmentStart: c_int,
    unsentLength: c_int,
    unsentBuffer: [byte; MAX_MSGLEN],
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[derive(Debug)]
#[repr(C)]
pub struct cvar_t {
    name: *const c_char,
    string: *const c_char,
    resetString: *const c_char,   // cvar_restart will reset to this value
    latchedString: *const c_char, // for CVAR_LATCH vars
    defaultString: *const c_char,
    minimumString: *const c_char,
    maximumString: *const c_char,
    flags: c_int,
    modified: qboolean,
    _unknown2: [u8; 4],
    modificationCount: c_int, // incremented each time the cvar is changed
    value: c_float,           // atof( string )
    integer: c_int,           // atoi( string )
    _unknown3: [u8; 8],
    next: *const cvar_t,
    hashNext: *const cvar_t,
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

    pub(crate) fn get_cvar(&self) -> &cvar_t {
        self.cvar
    }
}

#[allow(non_camel_case_types)]
#[repr(C)]
struct msg_t {
    allowoverflow: qboolean,
    // if false, do a Com_Error
    overflowed: qboolean,
    // set to true if the buffer size failed (with allowoverflow set)
    oob: qboolean,
    // set to true if the buffer size failed (with allowoverflow set)
    data: *const byte,
    maxsize: c_int,
    cursize: c_int,
    readcount: c_int,
    bit: c_int, // for bitwise reads and writes
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
struct netchan_buffer_t {
    msg: msg_t,
    msgBuffer: [byte; MAX_MSGLEN],
    next: *const netchan_buffer_t,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct client_t {
    state: clientState_t,
    userinfo: [c_char; MAX_INFO_STRING], // name, etc

    reliableCommands: [[c_char; MAX_STRING_CHARS]; MAX_RELIABLE_COMMANDS],
    reliableSequence: c_int, // last added reliable message, not necesarily sent or acknowledged yet
    reliableAcknowledge: c_int, // last acknowledged reliable message
    reliableSent: c_int,     // last sent reliable message, not necesarily acknowledged yet
    messageAcknowledge: c_int,

    gamestateMessageNum: c_int, // netchan->outgoingSequence of gamestate
    challenge: c_int,

    lastUsercmd: usercmd_t,
    lastMessageNum: c_int,    // for delta compression
    lastClientCommand: c_int, // reliable client message sequence
    lastClientCommandString: [c_char; MAX_STRING_CHARS],
    gentity: *const sharedEntity_t,  // SV_GentityNum(clientnum)
    name: [c_char; MAX_NAME_LENGTH], // extracted from userinfo, high bits masked

    // Mino: I think everything above this is correct. Below is a mess.

    // downloading
    downloadName: [c_char; MAX_QPATH], // if not empty string, we are downloading
    download: fileHandle_t,            // file being downloaded
    downloadSize: c_int,               // total bytes (can't use EOF because of paks)
    downloadCount: c_int,              // bytes sent
    downloadClientBlock: c_int,        // last block we sent to the client, awaiting ack
    downloadCurrentBlock: c_int,       // current block number
    downloadXmitBlock: c_int,          // last block we xmited
    downloadBlocks: *const [c_uchar; MAX_DOWNLOAD_WINDOW], // the buffers for the download blocks
    downloadBlockSize: [c_int; MAX_DOWNLOAD_WINDOW],
    downloadEOF: qboolean,   // We have sent the EOF block
    downloadSendTime: c_int, // time we last got an ack from the client

    deltaMessage: c_int,     // frame last client usercmd message
    nextReliableTime: c_int, // svs.time when another reliable command will be allowed
    lastPacketTime: c_int,   // svs.time when packet was last received
    lastConnectTime: c_int,  // svs.time when connection started
    nextSnapshotTime: c_int, // send another snapshot when svs.time >= nextSnapshotTime
    rateDelayed: qboolean, // true if nextSnapshotTime was set based on rate instead of snapshotMsec
    timeoutCount: c_int,   // must timeout a few frames in a row so debugging doesn't break
    frames: [clientSnapshot_t; PACKET_BACKUP], // updates can be delta'd from here
    ping: c_int,
    rate: c_int,         // bytes / second
    snapshotMsec: c_int, // requests a snapshot every snapshotMsec unless rate choked
    pureAuthentic: c_int,
    gotCP: qboolean, // TTimo - additional flag to distinguish between a bad pure checksum, and no cp command at all
    netchan: netchan_t,
    netchan_start_queue: *const netchan_buffer_t,
    netchan_end_queue: *const *const netchan_buffer_t,

    // Mino: Holy crap. A bunch of data was added. I have no idea where it actually goes,
    // but this will at least correct sizeof(client_t).
    #[cfg(target_pointer_width = "64")]
    _unknown2: [u8; 36808],
    #[cfg(target_pointer_width = "32")]
    _unknown2: [u8; 36836], // TODO: Outdated.

    // Mino: Woohoo! How nice of them to put the SteamID last.
    steam_id: u64,
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
    static svs: *const serverStatic_t;
}

impl TryFrom<i32> for Client {
    type Error = &'static str;

    fn try_from(client_id: i32) -> Result<Self, Self::Error> {
        unsafe {
            (*svs)
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
        // we really should be using .offset_from here, but rust's optimizations above level 0 led to some mis-calculations, so we mimic the raw C-calculation.
        unsafe {
            (((self.client_t as *const client_t as usize) - ((*svs).clients as usize))
                / mem::size_of::<client_t>()) as i32
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
        unsafe { CStr::from_ptr(self.client_t.name.as_ptr()).to_string_lossy() }
    }

    pub(crate) fn get_steam_id(&self) -> u64 {
        self.client_t.steam_id
    }

    fn is_connected(&self) -> bool {
        let Ok(game_entity) = GameEntity::try_from(self.get_client_id()) else {
            return false;
        };
        let Some(game_client) = game_entity.get_game_client() else {
            return false;
        };
        game_client.get_connection_state() == CON_DISCONNECTED
    }

    pub(crate) fn get_player_name(&self) -> Cow<'static, str> {
        if !self.is_connected() {
            return Cow::from("");
        }
        let game_entity = GameEntity::try_from(self.get_client_id()).unwrap();
        let game_client = game_entity.get_game_client().unwrap();
        game_client.get_player_name()
    }

    pub(crate) fn get_team(&self) -> i32 {
        if !self.is_connected() {
            return TEAM_SPECTATOR as i32;
        }

        let game_entity = GameEntity::try_from(self.get_client_id()).unwrap();
        let game_client = game_entity.get_game_client().unwrap();
        game_client.get_team()
    }

    pub(crate) fn get_privileges(&self) -> i32 {
        if !self.is_connected() {
            return -1;
        }

        let game_entity = GameEntity::try_from(self.get_client_id()).unwrap();
        let game_client = game_entity.get_game_client().unwrap();
        game_client.get_privileges()
    }

    pub(crate) fn get_user_info(&self) -> Cow<'static, str> {
        unsafe { CStr::from_ptr(self.client_t.userinfo.as_ptr()).to_string_lossy() }
    }

    pub(crate) fn set_vote(&self, yes_or_no: bool) {
        if let Ok(game_entity) = GameEntity::try_from(self.get_client_id()) {
            unsafe {
                (*game_entity.gentity_t.client).pers.voteState =
                    if yes_or_no { VOTE_YES } else { VOTE_NO };
            }
        };
    }
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[repr(C)]
struct challenge_t {
    adr: netadr_t,
    challenge: c_int,
    time: c_int,      // time the last packet was sent to the autherize server
    pingTime: c_int,  // time the challenge response was sent to client
    firstTime: c_int, // time the adr was first used, for authorize timeout checks
    connected: qboolean,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[repr(C)]
struct roundState_t {
    eCurrent: roundStateState_t,
    eNext: roundStateState_t,
    tNext: c_int,
    startTime: c_int,
    turn: c_int,
    round: c_int,
    prevRoundWinningTeam: team_t,
    touch: qboolean,
    capture: qboolean,
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[repr(C, align(8))]
struct level_locals_t {
    clients: *const gclient_t,
    gentities: *const gentity_t,
    gentitySize: c_int,
    num_entities: c_int,
    warmupTime: c_int,
    logFile: fileHandle_t,
    maxclients: c_int,
    time: c_int,
    frametime: c_int,
    startTime: c_int,
    teamScores: [c_int; 4],
    nextTeamInfoTime: c_int,
    newSession: qboolean,
    restarted: qboolean,
    shufflePending: qboolean,
    shuffleReadyTime: c_int,
    numConnectedClients: c_int,
    numNonSpectatorClients: c_int,
    numPlayingClients: c_int,
    numReadyClients: c_int,
    numReadyHumans: c_int,
    numStandardClients: c_int,
    sortedClients: [c_int; 64],
    follow1: c_int,
    follow2: c_int,
    snd_fry: c_int,
    warmupModificationCount: c_int,
    voteString: [c_char; 1024],
    voteDisplayString: [c_char; 1024],
    voteExecuteTime: c_int,
    voteTime: c_int,
    voteYes: c_int,
    voteNo: c_int,
    pendingVoteCaller: c_int,
    spawning: qboolean,
    numSpawnVars: c_int,
    spawnVars: [[*const c_char; 2]; 64],
    numSpawnVarChars: c_int,
    spawnVarChars: [c_char; 4096],
    intermissionQueued: c_int,
    intermissionTime: c_int,
    readyToExit: qboolean,
    votingEnded: qboolean,
    exitTime: c_int,
    intermission_origin: vec3_t,
    intermission_angle: vec3_t,
    locationLinked: qboolean,
    locationHead: *const gentity_t,
    timePauseBegin: c_int,
    timeOvertime: c_int,
    timeInitialPowerupSpawn: c_int,
    bodyQueIndex: c_int,
    bodyQue: [*const gentity_t; 8],
    portalSequence: c_int,
    gameStatsReported: qboolean,
    mapIsTrainingMap: qboolean,
    clientNum1stPlayer: c_int,
    clientNum2ndPlayer: c_int,
    scoreboardArchive1: [c_char; 1024],
    scoreboardArchive2: [c_char; 1024],
    firstScorer: [c_char; 40],
    lastScorer: [c_char; 40],
    lastTeamScorer: [c_char; 40],
    firstFrag: [c_char; 40],
    red_flag_origin: vec3_t,
    blue_flag_origin: vec3_t,
    spawnCount: [c_int; 4],
    runeSpawns: [c_int; 5],
    itemCount: [c_int; 60],
    suddenDeathRespawnDelay: c_int,
    suddenDeathRespawnDelayLastAnnounced: c_int,
    numRedArmorPickups: [c_int; 4],
    numYellowArmorPickups: [c_int; 4],
    numGreenArmorPickups: [c_int; 4],
    numMegaHealthPickups: [c_int; 4],
    numQuadDamagePickups: [c_int; 4],
    numBattleSuitPickups: [c_int; 4],
    numRegenerationPickups: [c_int; 4],
    numHastePickups: [c_int; 4],
    numInvisibilityPickups: [c_int; 4],
    quadDamagePossessionTime: [c_int; 4],
    battleSuitPossessionTime: [c_int; 4],
    regenerationPossessionTime: [c_int; 4],
    hastePossessionTime: [c_int; 4],
    invisibilityPossessionTime: [c_int; 4],
    numFlagPickups: [c_int; 4],
    numMedkitPickups: [c_int; 4],
    flagPossessionTime: [c_int; 4],
    dominationPoints: [*const gentity_t; 5],
    dominationPointCount: c_int,
    dominationPointsTallied: c_int,
    racePointCount: c_int,
    disableDropWeapon: qboolean,
    teamShuffleActive: qboolean,
    lastTeamScores: [c_int; 4],
    lastTeamRoundScores: [c_int; 4],
    attackingTeam: team_t,
    roundState: roundState_t,
    lastTeamCountSent: c_int,
    infectedConscript: c_int,
    lastZombieSurvivor: c_int,
    zombieScoreTime: c_int,
    lastInfectionTime: c_int,
    intermissionMapNames: [[c_char; 1024]; 3],
    intermissionMapTitles: [[c_char; 1024]; 3],
    intermissionMapConfigs: [[c_char; 1024]; 3],
    intermissionMapVotes: [c_int; 3],
    matchForfeited: qboolean,
    allReadyTime: c_int,
    notifyCvarChange: qboolean,
    notifyCvarChangeTime: c_int,
    lastLeadChangeTime: c_int,
    lastLeadChangeClient: c_int,
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

    pub(crate) fn set_vote_time(&mut self, new_vote_time: i32) {
        self.level.voteTime = new_vote_time as c_int;
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

        let maxclients = *SV_MAXCLIENTS.lock().unwrap();
        for client_id in 0..maxclients {
            if let Ok(game_entity) = GameEntity::try_from(client_id) {
                let mut game_client = game_entity.get_game_client().unwrap();
                game_client.set_vote_pending();
            }
        }

        shinqlx_set_configstring(CS_VOTE_STRING, vote_disp);
        shinqlx_set_configstring(CS_VOTE_TIME, format!("{}", self.level.voteTime).as_str());
        shinqlx_set_configstring(CS_VOTE_YES, "0");
        shinqlx_set_configstring(CS_VOTE_NO, "0");
    }
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[repr(C)]
struct serverStatic_t {
    initialized: qboolean,                  // sv_init has completed
    time: c_int,                            // will be strictly increasing across level changes
    snapFlagServerBit: c_int,               // ^= SNAPFLAG_SERVERCOUNT every SV_SpawnServer()
    clients: *mut client_t,                 // [sv_maxclients->integer];
    numSnapshotEntities: c_int, // sv_maxclients->integer*PACKET_BACKUP*MAX_PACKET_ENTITIES
    nextSnapshotEntities: c_int, // next snapshotEntities to use
    snapshotEntities: *const entityState_t, // [numSnapshotEntities]
    nextHeartbeatTime: c_int,
    challenges: [challenge_t; MAX_CHALLENGES], // to prevent invalid IPs from connecting
    redirectAddress: netadr_t,                 // for rcon return messages
    authorizeAddress: netadr_t,                // for rcon return messages
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
        let c_value = CString::new(value).unwrap().into_raw();
        unsafe { SV_SetConfigstring(*index, c_value) }
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
    fn client_connect(client_num: i32, first_time: bool, is_bot: bool)
        -> Option<Cow<'static, str>>;
}

impl ClientConnect for QuakeLiveEngine {
    fn client_connect(
        client_num: i32,
        first_time: bool,
        is_bot: bool,
    ) -> Option<Cow<'static, str>> {
        unsafe {
            let c_return = ClientConnect(client_num, first_time.into(), is_bot.into());
            if c_return.is_null() {
                return None;
            }
            Some(CStr::from_ptr(c_return).to_string_lossy())
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
        String::from_utf8_lossy(&buffer).to_string()
    }
}
