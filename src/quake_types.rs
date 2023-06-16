#![allow(dead_code)]
#![allow(clippy::upper_case_acronyms)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
use derive_builder::Builder;
use std::ffi::{c_char, c_float, c_int, c_uchar, c_uint, c_ushort};

// these are the only configstrings that the system reserves, all the
// other ones are strictly for servergame to clientgame communication
pub const CS_SERVERINFO: u32 = 0; // an info string with all the serverinfo cvars
pub const CS_SYSTEMINFO: u32 = 1; // an info string for server system to client system configuration (timescale, etc)
pub const CS_MUSIC: u32 = 2;
pub const CS_MESSAGE: u32 = 3; // from the map worldspawn's message field
pub const CS_MOTD: u32 = 4; // g_motd string for server message of the day
pub const CS_WARMUP: u32 = 5; // server time when the match will be restarted
pub const CS_SCORES1: u32 = 6;
pub const CS_SCORES2: u32 = 7;
pub const CS_VOTE_TIME: u32 = 8;
pub const CS_VOTE_STRING: u32 = 9;
pub const CS_VOTE_YES: u32 = 10;
pub const CS_VOTE_NO: u32 = 11;
pub const CS_GAME_VERSION: u32 = 12;
pub const CS_LEVEL_START_TIME: u32 = 13; // so the timer only shows the current level
pub const CS_INTERMISSION: u32 = 14; // when 1, fraglimit/timelimit has been hit and intermissionwill start in a second or two
pub const CS_ITEMS: u32 = 15; // string of 0's and 1's that tell which items are present
pub const CS_MODELS: u32 = 17; // same as CS_SOUNDS where it is being indexed from 1 so 17 is empty and first model is 18
pub const CS_SOUNDS: u32 = CS_MODELS + MAX_MODELS;
pub const CS_PLAYERS: u32 = CS_SOUNDS + MAX_SOUNDS;
pub const CS_LOCATIONS: u32 = CS_PLAYERS + MAX_CLIENTS;
pub const CS_PARTICLES: u32 = CS_LOCATIONS + MAX_LOCATIONS;

pub const CS_FLAGSTATUS: u32 = 658; // string indicating flag status in CTF

pub const CS_FIRSTPLACE: u32 = 659;
pub const CS_SECONDPLACE: u32 = 660;

pub const CS_ROUND_STATUS: u32 = 661; // also used for freezetag
pub const CS_ROUND_TIME: u32 = 662; // when -1 round is over, also used for freezetag

pub const CS_SHADERSTATS: u32 = 665;

pub const CS_TIMEOUT_BEGIN_TIME: u32 = 669;
pub const CS_TIMEOUT_END_TIME: u32 = 670;
pub const CS_RED_TEAM_TIMEOUT_LEFT: u32 = 671;
pub const CS_BLUE_TEAM_TIMEOUT_LEFT: u32 = 672;

pub const CS_MAP_CREATOR: u32 = 678;
pub const CS_ORIGINAL_MAP_CREATOR: u32 = 679;

pub const CS_PMOVE_SETTING: u32 = 681;
pub const CS_ARMOR_TIERED: u32 = 682;
pub const CS_WEAPON_SETTINGS: u32 = 683;

pub const MAX_CLIENTS: u32 = 64;
pub const MAX_LOCATIONS: u32 = 64;
pub const MAX_CHALLENGES: u32 = 1024;
pub const MAX_MSGLEN: u32 = 16384;
pub const MAX_PS_EVENTS: u32 = 2;
pub const MAX_MAP_AREA_BYTES: u32 = 32; // bit vector of area visibility
pub const MAX_INFO_STRING: u32 = 1024;
pub const MAX_RELIABLE_COMMANDS: u32 = 64; // max string commands buffered for restransmit
pub const MAX_STRING_CHARS: u32 = 1024; // max length of a string passed to Cmd_TokenizeString
pub const MAX_NAME_LENGTH: u32 = 32; // max length of a client name
pub const MAX_QPATH: u32 = 64; // max length of a quake game pathname
pub const MAX_DOWNLOAD_WINDOW: u32 = 8; // max of eight download frames
pub const MAX_NETNAME: u32 = 36;
pub const PACKET_BACKUP: u32 = 32; // number of old messages that must be kept on client and
                                   // server for delta comrpession and ping estimation
pub const PACKET_MASK: u32 = PACKET_BACKUP - 1;
pub const MAX_ENT_CLUSTERS: u32 = 16;
pub const MAX_MODELS: u32 = 256; // these are sent over the net as 8 bits
pub const MAX_SOUNDS: u32 = 256; // so they cannot be blindly increased
pub const MAX_CONFIGSTRINGS: u32 = 1024;
pub const GENTITYNUM_BITS: u32 = 10; // don't need to send any more
pub const MAX_GENTITIES: u32 = 1 << GENTITYNUM_BITS;
pub const MAX_ITEM_MODELS: u32 = 4;
pub const MAX_SPAWN_VARS: u32 = 64;
pub const MAX_SPAWN_VARS_CHARS: u32 = 4096;
pub const BODY_QUEUE_SIZE: u32 = 8;

// bit field limits
pub const MAX_STATS: u32 = 16;
pub const MAX_PERSISTANT: u32 = 16;
pub const MAX_POWERUPS: u32 = 16;
pub const MAX_WEAPONS: u32 = 16;

// Button flags
pub const BUTTON_ATTACK: u32 = 1;
pub const BUTTON_TALK: u32 = 2; // displkays talk balloon and disables actions
pub const BUTTON_USE_HOLDABLE: u32 = 4; // Mino +button2
pub const BUTTON_GESTURE: u32 = 8; // Mino: +button3
pub const BUTTON_WALKING: u32 = 16;
// Block of unused button flags, or at least flags I couldn't trigger.
// Q3 used them for bot commands, so probably unused in QL.
pub const BUTTON_UNUSED1: u32 = 32;
pub const BUTTON_UNUSED2: u32 = 64;
pub const BUTTON_UNUSED3: u32 = 128;
pub const BUTTON_UNUSED4: u32 = 256;
pub const BUTTON_UNUSED5: u32 = 512;
pub const BUTTON_UNUSED6: u32 = 1024;
pub const BUTTON_UPMOVE: u32 = 2048; // Mino: Not in Q3. I'm guessing it's for cg_autohop.
pub const BUTTON_ANY: u32 = 4096; // any key whatsoever
pub const BUTTON_IS_ACTIVE: u32 = 65536; // Mino: No idea what it is, but it goes off after a while of being
                                         //       AFK, then goes on after being active for a while.

// eflags
pub const EF_DEAD: u32 = 1; // don't draw a foe marker over players with EF_DEAD
pub const EF_TICKING: u32 = 2; // used to make players play the prox mine ticking sound
pub const EF_TELEPORT_BIT: u32 = 4; // toggled every time the origin abruptly changes
pub const EF_AWARD_EXCELLENT: u32 = 8; // draw an excellent sprite
pub const EF_PLAYER_EVENT: u32 = 16;
pub const EF_BOUNCE: u32 = 16; // for missiles
pub const EF_BOUNCE_HALF: u32 = 32; // for missiles
pub const EF_AWARD_GAUNTLET: u32 = 64; // draw a gauntlet sprite
pub const EF_NODRAW: u32 = 128; // may have an event, but no model (unspawned items)
pub const EF_FIRING: u32 = 256; // for lightning gun
pub const EF_KAMIKAZE: u32 = 512;
pub const EF_MOVER_STOP: u32 = 1024; // will push otherwise
pub const EF_AWARD_CAP: u32 = 2048; // draw the capture sprite
pub const EF_TALK: u32 = 4096; // draw a talk balloon
pub const EF_CONNECTION: u32 = 8192; // draw a connection trouble sprite
pub const EF_VOTED: u32 = 16384; // already cast a vote
pub const EF_AWARD_IMPRESSIVE: u32 = 32768; // draw an impressive sprite
pub const EF_AWARD_DEFEND: u32 = 65536; // draw a defend sprite
pub const EF_AWARD_ASSIST: u32 = 131072; // draw a assist sprite
pub const EF_AWARD_DENIED: u32 = 262144; // denied
pub const EF_TEAMVOTED: u32 = 524288; // already cast a team vote

// gentity->flags
pub const FL_GODMODE: u32 = 16;
pub const FL_NOTARGET: u32 = 32;
pub const FL_TEAMSLAVE: u32 = 1024; // not the first on the team
pub const FL_NO_KNOCKBACK: u32 = 2048;
pub const FL_DROPPED_ITEM: u32 = 4096;
pub const FL_NO_BOTS: u32 = 8192; // spawn point not for bot use
pub const FL_NO_HUMANS: u32 = 16384; // spawn point just for bots
pub const FL_FORCE_GESTURE: u32 = 32768; // force gesture on client

// damage flags
pub const DAMAGE_RADIUS: u32 = 1; // damage was indirect
pub const DAMAGE_NO_ARMOR: u32 = 2; // armor does not protect from this damage
pub const DAMAGE_NO_KNOCKBACK: u32 = 4; // do not affect velocity, just view angles
pub const DAMAGE_NO_PROTECTION: u32 = 8; // armor, shields, invulnerability, and godmode have no effect
pub const DAMAGE_NO_TEAM_PROTECTION: u32 = 16; // armor, shields, invulnerability, and godmode have no effect

pub const MODELINDEX_ARMORSHARD: u32 = 0;
pub const MODELINDEX_ARMORCOMBAT: u32 = 1;
pub const MODELINDEX_ARMORBODY: u32 = 2;
pub const MODELINDEX_HEALTHSMALL: u32 = 3;
pub const MODELINDEX_HEALTH: u32 = 4;
pub const MODELINDEX_HEALTHLARGE: u32 = 5;
pub const MODELINDEX_HEALTHMEGA: u32 = 6;
pub const MODELINDEX_GAUNTLET: u32 = 7;
pub const MODELINDEX_SHOTGUN: u32 = 8;
pub const MODELINDEX_MACHINEGUN: u32 = 9;
pub const MODELINDEX_GRENADELAUNCHER: u32 = 10;
pub const MODELINDEX_ROCKETLAUNCHER: u32 = 11;
pub const MODELINDEX_LIGHTNING: u32 = 12;
pub const MODELINDEX_RAILGUN: u32 = 13;
pub const MODELINDEX_PLASMAGUN: u32 = 14;
pub const MODELINDEX_BFG10K: u32 = 15;
pub const MODELINDEX_GRAPPLINGHOOK: u32 = 16;
pub const MODELINDEX_SHELLS: u32 = 17;
pub const MODELINDEX_BULLETS: u32 = 18;
pub const MODELINDEX_GRENADES: u32 = 19;
pub const MODELINDEX_CELLS: u32 = 20;
pub const MODELINDEX_LIGHTNINGAMMO: u32 = 21;
pub const MODELINDEX_ROCKETS: u32 = 22;
pub const MODELINDEX_SLUGS: u32 = 23;
pub const MODELINDEX_BFGAMMO: u32 = 24;
pub const MODELINDEX_TELEPORTER: u32 = 25;
pub const MODELINDEX_MEDKIT: u32 = 26;
pub const MODELINDEX_QUAD: u32 = 27;
pub const MODELINDEX_ENVIRONMENTSUIT: u32 = 28;
pub const MODELINDEX_HASTE: u32 = 29;
pub const MODELINDEX_INVISIBILITY: u32 = 30;
pub const MODELINDEX_REGEN: u32 = 31;
pub const MODELINDEX_FLIGHT: u32 = 32;
pub const MODELINDEX_REDFLAG: u32 = 33;
pub const MODELINDEX_BLUEFLAG: u32 = 34;
pub const MODELINDEX_KAMIKAZE: u32 = 35;
pub const MODELINDEX_PORTAL: u32 = 36;
pub const MODELINDEX_INVULNERABILITY: u32 = 37;
pub const MODELINDEX_NAILS: u32 = 38;
pub const MODELINDEX_MINES: u32 = 39;
pub const MODELINDEX_BELT: u32 = 40;
pub const MODELINDEX_SCOUT: u32 = 41;
pub const MODELINDEX_GUARD: u32 = 42;
pub const MODELINDEX_DOUBLER: u32 = 43;
pub const MODELINDEX_AMMOREGEN: u32 = 44;
pub const MODELINDEX_NEUTRALFLAG: u32 = 45;
pub const MODELINDEX_REDCUBE: u32 = 46;
pub const MODELINDEX_BLUECUBE: u32 = 47;
pub const MODELINDEX_NAILGUN: u32 = 48;
pub const MODELINDEX_PROXLAUNCHER: u32 = 49;
pub const MODELINDEX_CHAINGUN: u32 = 50;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum qboolean {
    qfalse = 0,
    qtrue = 1,
}

pub type byte = c_uchar;
pub type gentity_t = gentity_s;
pub type gclient_t = gclient_s;
pub type vec_t = f32;
pub type vec3_t = [c_float; 3];
pub type fileHandle_t = c_int;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum privileges_t {
    PRIV_BANNED = 4294967295,
    PRIV_NONE = 0,
    PRIV_MOD = 1,
    PRIV_ADMIN = 2,
    PRIV_ROOT = 3,
}

#[repr(u32)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum clientState_t {
    CS_FREE = 0,   // can be reused for a new connection
    CS_ZOMBIE = 1, // client has been disconnected, but don't reuse
    // connection for a couple seconds
    CS_CONNECTED = 2, // has been assigned to a client_t, but no gamestate yet
    CS_PRIMED = 3,    // gamestate has been sent, but client hasn't sent a usercmd
    CS_ACTIVE = 4,    // client is fully in game
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum roundStateState_t {
    PREGAME = 0,
    ROUND_WARMUP = 1,
    ROUND_SHUFFLE = 2,
    ROUND_BEGUN = 3,
    ROUND_OVER = 4,
    POSTGAME = 5,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

impl powerup_t {
    pub const PW_SPAWNARMOR: powerup_t = powerup_t::PW_NONE;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum weaponstate_t {
    WEAPON_READY = 0,
    WEAPON_RAISING = 1,
    WEAPON_DROPPING = 2,
    WEAPON_FIRING = 3,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum rune_t {
    RUNE_NONE = 0,
    RUNE_SCOUT = 1,
    RUNE_GUARD = 2,
    RUNE_DAMAGE = 3,
    RUNE_ARMORREGEN = 4,
    RUNE_MAX = 5,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum playerTeamStateState_t {
    TEAM_BEGIN = 0,  // Beginning a team game, spawn at base
    TEAM_ACTIVE = 1, // Now actively playing
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum team_t {
    TEAM_FREE = 0,
    TEAM_RED = 1,
    TEAM_BLUE = 2,
    TEAM_SPECTATOR = 3,
    TEAM_NUM_TEAMS = 4,
}

// https://github.com/brugal/wolfcamql/blob/73e2d707e5dd1fb0fc50d4ad9f00940909c4b3ec/code/game/bg_public.h#L1142-L1188
// means of death
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum spectatorState_t {
    SPECTATOR_NOT = 0,
    SPECTATOR_FREE = 1,
    SPECTATOR_FOLLOW = 2,
    SPECTATOR_SCOREBOARD = 3,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum clientConnected_t {
    CON_DISCONNECTED = 0,
    CON_CONNECTING = 1,
    CON_CONNECTED = 2,
}

// movers are things like doors, plats, buttons, etc
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum moverState_t {
    MOVER_POS1 = 0,
    MOVER_POS2 = 1,
    MOVER_1TO2 = 2,
    MOVER_2TO1 = 3,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum persistantFields_t {
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum cbufExec_t {
    EXEC_NOW = 0, // don't return until completed, a VM should NEVER use this,
    // because some commands might cause the VM to be unloaded...
    EXEC_INSERT = 1, // insert at current position, but don't run yet
    EXEC_APPEND = 2, // add to end of the command buffer (normal case)
}

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

pub type cvar_t = cvar_s;

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

#[repr(C)]
#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "UserCmdBuilder")]
pub struct usercmd_s {
    #[builder(default)]
    pub serverTime: c_int,
    #[builder(default)]
    pub angles: [c_int; 3usize],
    #[builder(default)]
    pub buttons: c_int,
    #[builder(default)]
    pub weapon: byte,
    #[builder(default)]
    pub weaponPrimary: byte,
    #[builder(default)]
    pub fov: byte,
    #[builder(default)]
    pub forwardmove: c_char,
    #[builder(default)]
    pub rightmove: c_char,
    #[builder(default)]
    pub upmove: c_char,
}

pub type usercmd_t = usercmd_s;

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum netsrc_t {
    NS_CLIENT = 0,
    NS_SERVER = 1,
}

#[repr(u32)]
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

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum trType_t {
    TR_STATIONARY = 0,
    TR_INTERPOLATE = 1, // non-parametric, but interpolate between snapshots
    TR_LINEAR = 2,
    TR_LINEAR_STOP = 3,
    TR_SINE = 4, // value = base + sin( time / duration ) * delta
    TR_GRAVITY = 5,
}

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct netadr_t {
    pub type_: netadrtype_t,
    pub ip: [byte; 4usize],
    pub ipx: [byte; 10usize],
    pub port: c_ushort,
}

#[repr(C)]
#[derive(Debug, PartialEq)]
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
    pub fragmentBuffer: [byte; MAX_MSGLEN as usize],
    // outgoing fragment buffer
    // we need to space out the sending of large fragmented messages
    pub unsentFragments: qboolean,
    pub unsentFragmentStart: c_int,
    pub unsentLength: c_int,
    pub unsentBuffer: [byte; MAX_MSGLEN as usize],
}

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct cplane_s {
    pub normal: vec3_t,
    pub dist: f32,
    pub type_: byte,
    pub signbits: byte,
    pub pad: [byte; 2usize],
}

pub type cplane_t = cplane_s;

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
#[repr(C)]
#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "PlayerStateBuilder")]
pub struct playerState_s {
    #[builder(default)]
    pub commandTime: c_int,
    #[builder(default = "pmtype_t::PM_NORMAL")]
    pub pm_type: pmtype_t,
    #[builder(default)]
    pub bobCycle: c_int,
    #[builder(default)]
    pub pm_flags: c_int,
    #[builder(default)]
    pub pm_time: c_int,
    #[builder(default)]
    pub origin: vec3_t,
    #[builder(default)]
    pub velocity: vec3_t,
    #[builder(default)]
    pub weaponTime: c_int,
    #[builder(default)]
    pub gravity: c_int,
    #[builder(default)]
    pub speed: c_int,
    #[builder(default)]
    pub delta_angles: [c_int; 3usize],
    #[builder(default)]
    pub groundEntityNum: c_int,
    #[builder(default)]
    pub legsTimer: c_int,
    #[builder(default)]
    pub legsAnim: c_int,
    #[builder(default)]
    pub torsoTimer: c_int,
    #[builder(default)]
    pub torsoAnim: c_int,
    #[builder(default)]
    pub movementDir: c_int,
    #[builder(default)]
    pub grapplePoint: vec3_t,
    #[builder(default)]
    pub eFlags: c_int,
    #[builder(default)]
    pub eventSequence: c_int,
    #[builder(default)]
    pub events: [c_int; 2usize],
    #[builder(default)]
    pub eventParms: [c_int; 2usize],
    #[builder(default)]
    pub externalEvent: c_int,
    #[builder(default)]
    pub externalEventParm: c_int,
    #[builder(default)]
    pub clientNum: c_int,
    #[builder(default)]
    pub location: c_int,
    #[builder(default)]
    pub weapon: c_int,
    #[builder(default)]
    pub weaponPrimary: c_int,
    #[builder(default)]
    pub weaponstate: c_int,
    #[builder(default)]
    pub fov: c_int,
    #[builder(default)]
    pub viewangles: vec3_t,
    #[builder(default)]
    pub viewheight: c_int,
    #[builder(default)]
    pub damageEvent: c_int,
    #[builder(default)]
    pub damageYaw: c_int,
    #[builder(default)]
    pub damagePitch: c_int,
    #[builder(default)]
    pub damageCount: c_int,
    #[builder(default)]
    pub stats: [c_int; 16usize],
    #[builder(default)]
    pub persistant: [c_int; 16usize],
    #[builder(default)]
    pub powerups: [c_int; 16usize],
    #[builder(default)]
    pub ammo: [c_int; 16usize],
    #[builder(default)]
    pub generic1: c_int,
    #[builder(default)]
    pub loopSound: c_int,
    #[builder(default)]
    pub jumppad_ent: c_int,
    #[builder(default)]
    pub jumpTime: c_int,
    #[builder(default)]
    pub doubleJumped: c_int,
    #[builder(default)]
    pub crouchTime: c_int,
    #[builder(default)]
    pub crouchSlideTime: c_int,
    #[builder(default)]
    pub forwardmove: c_char,
    #[builder(default)]
    pub rightmove: c_char,
    #[builder(default)]
    pub upmove: c_char,
    #[builder(default)]
    pub ping: c_int,
    #[builder(default)]
    pub pmove_framecount: c_int,
    #[builder(default)]
    pub jumppad_frame: c_int,
    #[builder(default)]
    pub entityEventSequence: c_int,
    #[builder(default)]
    pub freezetime: c_int,
    #[builder(default)]
    pub thawtime: c_int,
    #[builder(default)]
    pub thawClientNum_valid: c_int,
    #[builder(default)]
    pub thawClientNum: c_int,
    #[builder(default)]
    pub respawnTime: c_int,
    #[builder(default)]
    pub localPersistant: [c_int; 16usize],
    #[builder(default)]
    pub roundDamage: c_int,
    #[builder(default)]
    pub roundShots: c_int,
    #[builder(default)]
    pub roundHits: c_int,
}

pub type playerState_t = playerState_s;

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

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct clientSnapshot_t {
    pub areabytes: c_int,
    pub areabits: [byte; MAX_MAP_AREA_BYTES as usize], // portalarea visibility bits
    pub ps: playerState_t,
    pub num_entities: c_int,
    pub first_entity: c_int, // into the circular sv_packet_entities[]
    // the entities MUST be in increasing state number
    // order, otherwise the delta compression will fail
    pub messageSent: c_int,  // time the message was transmitted
    pub messageAcked: c_int, // time the message was acked
    pub messageSize: c_int,  // used to rate drop packets
}

#[repr(C)]
pub struct netchan_buffer_s {
    pub msg: msg_t,
    pub msgBuffer: [byte; MAX_MSGLEN as usize],
    pub next: *mut netchan_buffer_s,
}

pub type netchan_buffer_t = netchan_buffer_s;

#[repr(C)]
#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "TrajectoryBuilder")]
pub struct trajectory_t {
    #[builder(default = "trType_t::TR_STATIONARY")]
    pub trType: trType_t,
    #[builder(default = "0")]
    pub trTime: c_int,
    #[builder(default = "0")]
    pub trDuration: c_int,
    #[builder(default = "[0.0; 3]")]
    pub trBase: vec3_t,
    #[builder(default = "[0.0; 3]")]
    pub trDelta: vec3_t,
    #[builder(default = "0.0")]
    pub gravity: f32,
}

#[repr(C)]
#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "EntityStateBuilder")]
pub struct entityState_s {
    #[builder(default = "0")]
    pub number: c_int,
    #[builder(default = "0")]
    pub eType: c_int,
    #[builder(default = "0")]
    pub eFlags: c_int,
    pub pos: trajectory_t,
    pub apos: trajectory_t,
    #[builder(default = "0")]
    pub time: c_int,
    #[builder(default = "0")]
    pub time2: c_int,
    #[builder(default = "[0.0; 3]")]
    pub origin: vec3_t,
    #[builder(default = "[0.0; 3]")]
    pub origin2: vec3_t,
    #[builder(default = "[0.0; 3]")]
    pub angles: vec3_t,
    #[builder(default = "[0.0; 3]")]
    pub angles2: vec3_t,
    #[builder(default = "0")]
    pub otherEntityNum: c_int,
    #[builder(default = "0")]
    pub otherEntityNum2: c_int,
    #[builder(default = "0")]
    pub groundEntityNum: c_int,
    #[builder(default = "0")]
    pub constantLight: c_int,
    #[builder(default = "0")]
    pub loopSound: c_int,
    #[builder(default = "0")]
    pub modelindex: c_int,
    #[builder(default = "0")]
    pub modelindex2: c_int,
    #[builder(default = "0")]
    pub clientNum: c_int,
    #[builder(default = "0")]
    pub frame: c_int,
    #[builder(default = "0")]
    pub solid: c_int,
    #[builder(default = "0")]
    pub event: c_int,
    #[builder(default = "0")]
    pub eventParm: c_int,
    #[builder(default = "0")]
    pub powerups: c_int,
    #[builder(default = "0")]
    pub health: c_int,
    #[builder(default = "0")]
    pub armor: c_int,
    #[builder(default = "0")]
    pub weapon: c_int,
    #[builder(default = "0")]
    pub location: c_int,
    #[builder(default = "0")]
    pub legsAnim: c_int,
    #[builder(default = "0")]
    pub torsoAnim: c_int,
    #[builder(default = "0")]
    pub generic1: c_int,
    #[builder(default = "0")]
    pub jumpTime: c_int,
    #[builder(default = "0")]
    pub doubleJumped: c_int,
}

pub type entityState_t = entityState_s;

#[repr(C)]
#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "EntitySharedbuilder")]
pub struct entityShared_t {
    pub s: entityState_t,
    #[builder(default = "qboolean::qfalse")]
    pub linked: qboolean,
    #[builder(default = "0")]
    pub linkcount: c_int,
    #[builder(default = "0")]
    pub svFlags: c_int,
    #[builder(default = "0")]
    pub singleClient: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub bmodel: qboolean,
    #[builder(default = "[0.0; 3]")]
    pub mins: vec3_t,
    #[builder(default = "[0.0; 3]")]
    pub maxs: vec3_t,
    #[builder(default = "0")]
    pub contents: c_int,
    #[builder(default = "[0.0; 3]")]
    pub absmin: vec3_t,
    #[builder(default = "[0.0; 3]")]
    pub absmax: vec3_t,
    #[builder(default = "[0.0; 3]")]
    pub currentOrigin: vec3_t,
    #[builder(default = "[0.0; 3]")]
    pub currentAngles: vec3_t,
    #[builder(default = "0")]
    pub ownerNum: c_int,
}

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct sharedEntity_t {
    pub s: entityState_t,  // communicated by server to clients
    pub r: entityShared_t, // shared by both the server system and game
}

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct client_s {
    pub state: clientState_t,
    pub userinfo: [c_char; MAX_INFO_STRING as usize], // name, etc
    pub reliableCommands: [[c_char; MAX_STRING_CHARS as usize]; MAX_RELIABLE_COMMANDS as usize],
    pub reliableSequence: c_int, // last added reliable message, not necesarily sent or acknowledged yet
    pub reliableAcknowledge: c_int, // last acknowledged reliable message
    pub reliableSent: c_int,     // last sent reliable message, not necesarily acknowledged yet
    pub messageAcknowledge: c_int,
    pub gamestateMessageNum: c_int, // netchan->outgoingSequence of gamestate
    pub challenge: c_int,
    pub lastUsercmd: usercmd_t,
    pub lastMessageNum: c_int,    // for delta compression
    pub lastClientCommand: c_int, // reliable client message sequence
    pub lastClientCommandString: [c_char; MAX_STRING_CHARS as usize],
    pub gentity: *mut sharedEntity_t, // SV_GentityNum(clientnum)
    pub name: [c_char; MAX_NAME_LENGTH as usize], // extracted from userinfo, high bits masked

    // Mino: I think everything above this is correct. Below is a mess.

    // downloading
    pub downloadName: [c_char; MAX_QPATH as usize], // if not empty string, we are downloading
    pub download: fileHandle_t,                     // file being downloaded
    pub downloadSize: c_int,                        // total bytes (can't use EOF because of paks)
    pub downloadCount: c_int,                       // bytes sent
    pub downloadClientBlock: c_int, // last block we sent to the client, awaiting ack
    pub downloadCurrentBlock: c_int, // current block number
    pub downloadXmitBlock: c_int,   // last block we xmited
    pub downloadBlocks: [*mut c_uchar; MAX_DOWNLOAD_WINDOW as usize], // the buffers for the download blocks
    pub downloadBlockSize: [c_int; MAX_DOWNLOAD_WINDOW as usize],
    pub downloadEOF: qboolean,   // We have sent the EOF block
    pub downloadSendTime: c_int, // time we last got an ack from the client
    pub deltaMessage: c_int,     // frame last client usercmd message
    pub nextReliableTime: c_int, // svs.time when another reliable command will be allowed
    pub lastPacketTime: c_int,   // svs.time when packet was last received
    pub lastConnectTime: c_int,  // svs.time when connection started
    pub nextSnapshotTime: c_int, // send another snapshot when svs.time >= nextSnapshotTime
    pub rateDelayed: qboolean, // true if nextSnapshotTime was set based on rate instead of snapshotMsec
    pub timeoutCount: c_int,   // must timeout a few frames in a row so debugging doesn't break
    pub frames: [clientSnapshot_t; PACKET_BACKUP as usize], // updates can be delta'd from here
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
    pub _unknown2: [u8; 36836usize], // TODO: Outdated.
    // Mino: Woohoo! How nice of them to put the SteamID last.
    pub steam_id: u64,
}

pub type client_t = client_s;

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
    pub challenges: [challenge_t; MAX_CHALLENGES as usize], // to prevent invalid IPs from connecting
    pub redirectAddress: netadr_t,                          // for rcon return messages
    pub authorizeAddress: netadr_t,                         // for rcon return messages
}

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct svEntity_s {
    pub worldSector: *mut worldSector_s,
    pub nextEntityInWorldSector: *mut svEntity_s,
    pub baseline: entityState_t, // for delta compression of initial sighting
    pub numClusters: c_int,      // if -1, use headnode instead
    pub clusternums: [c_int; MAX_ENT_CLUSTERS as usize],
    pub lastCluster: c_int, // if all the clusters don't fit in clusternums
    pub areanum: c_int,
    pub areanum2: c_int,
    pub snapshotCounter: c_int, // used to prevent double adding from portal views
}

pub type svEntity_t = svEntity_s;

#[repr(C)]
#[derive(Debug, PartialEq)]
pub struct worldSector_s {
    pub axis: c_int, // -1 = leaf node
    pub dist: f32,
    pub children: [*mut worldSector_s; 2usize],
    pub entities: *mut svEntity_t,
}

pub type worldSector_t = worldSector_s;

#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum serverState_t {
    SS_DEAD = 0,    // no map loaded
    SS_LOADING = 1, // spawning level entities
    SS_GAME = 2,    // actively running
}

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
    pub models: [*mut cmodel_s; MAX_MODELS as usize],
    pub configstrings: [*mut c_char; MAX_CONFIGSTRINGS as usize],
    pub svEntities: [svEntity_t; MAX_GENTITIES as usize],
    pub entityParsePoint: *mut c_char,
    pub gentities: *mut sharedEntity_t,
    pub gentitySize: c_int,
    pub num_entities: c_int,
    pub gameClients: *mut playerState_t,
    pub gameClientSize: c_int,
    pub restartTime: c_int,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone, Builder)]
#[builder(name = "PlayerTeamStateBuilder")]
pub struct playerTeamState_t {
    #[builder(default = "playerTeamStateState_t::TEAM_ACTIVE")]
    pub state: playerTeamStateState_t,
    #[builder(default)]
    pub captures: c_int,
    #[builder(default)]
    pub basedefense: c_int,
    #[builder(default)]
    pub carrierdefense: c_int,
    #[builder(default)]
    pub flagrecovery: c_int,
    #[builder(default)]
    pub fragcarrier: c_int,
    #[builder(default)]
    pub assists: c_int,
    #[builder(default)]
    pub flagruntime: c_int,
    #[builder(default)]
    pub flagrunrelays: c_int,
    #[builder(default)]
    pub lasthurtcarrier: c_int,
    #[builder(default)]
    pub lastreturnedflag: c_int,
    #[builder(default)]
    pub lastfraggedcarrier: c_int,
}

#[repr(C)]
#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "ExpandedStatsBuilder")]
pub struct expandedStatObj_t {
    #[builder(default)]
    pub statId: c_uint,
    #[builder(default)]
    pub lastThinkTime: c_int,
    #[builder(default)]
    pub teamJoinTime: c_int,
    #[builder(default)]
    pub totalPlayTime: c_int,
    #[builder(default)]
    pub serverRank: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub serverRankIsTied: qboolean,
    #[builder(default)]
    pub teamRank: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub teamRankIsTied: qboolean,
    #[builder(default)]
    pub numKills: c_int,
    #[builder(default)]
    pub numDeaths: c_int,
    #[builder(default)]
    pub numSuicides: c_int,
    #[builder(default)]
    pub numTeamKills: c_int,
    #[builder(default)]
    pub numTeamKilled: c_int,
    #[builder(default)]
    pub numWeaponKills: [c_int; 16usize],
    #[builder(default)]
    pub numWeaponDeaths: [c_int; 16usize],
    #[builder(default)]
    pub shotsFired: [c_int; 16usize],
    #[builder(default)]
    pub shotsHit: [c_int; 16usize],
    #[builder(default)]
    pub damageDealt: [c_int; 16usize],
    #[builder(default)]
    pub damageTaken: [c_int; 16usize],
    #[builder(default)]
    pub powerups: [c_int; 16usize],
    #[builder(default)]
    pub holdablePickups: [c_int; 7usize],
    #[builder(default)]
    pub weaponPickups: [c_int; 16usize],
    #[builder(default)]
    pub weaponUsageTime: [c_int; 16usize],
    #[builder(default)]
    pub numCaptures: c_int,
    #[builder(default)]
    pub numAssists: c_int,
    #[builder(default)]
    pub numDefends: c_int,
    #[builder(default)]
    pub numHolyShits: c_int,
    #[builder(default)]
    pub totalDamageDealt: c_int,
    #[builder(default)]
    pub totalDamageTaken: c_int,
    #[builder(default)]
    pub previousHealth: c_int,
    #[builder(default)]
    pub previousArmor: c_int,
    #[builder(default)]
    pub numAmmoPickups: c_int,
    #[builder(default)]
    pub numFirstMegaHealthPickups: c_int,
    #[builder(default)]
    pub numMegaHealthPickups: c_int,
    #[builder(default)]
    pub megaHealthPickupTime: c_int,
    #[builder(default)]
    pub numHealthPickups: c_int,
    #[builder(default)]
    pub numFirstRedArmorPickups: c_int,
    #[builder(default)]
    pub numRedArmorPickups: c_int,
    #[builder(default)]
    pub redArmorPickupTime: c_int,
    #[builder(default)]
    pub numFirstYellowArmorPickups: c_int,
    #[builder(default)]
    pub numYellowArmorPickups: c_int,
    #[builder(default)]
    pub yellowArmorPickupTime: c_int,
    #[builder(default)]
    pub numFirstGreenArmorPickups: c_int,
    #[builder(default)]
    pub numGreenArmorPickups: c_int,
    #[builder(default)]
    pub greenArmorPickupTime: c_int,
    #[builder(default)]
    pub numQuadDamagePickups: c_int,
    #[builder(default)]
    pub numQuadDamageKills: c_int,
    #[builder(default)]
    pub numBattleSuitPickups: c_int,
    #[builder(default)]
    pub numRegenerationPickups: c_int,
    #[builder(default)]
    pub numHastePickups: c_int,
    #[builder(default)]
    pub numInvisibilityPickups: c_int,
    #[builder(default)]
    pub numRedFlagPickups: c_int,
    #[builder(default)]
    pub numBlueFlagPickups: c_int,
    #[builder(default)]
    pub numNeutralFlagPickups: c_int,
    #[builder(default)]
    pub numMedkitPickups: c_int,
    #[builder(default)]
    pub numArmorPickups: c_int,
    #[builder(default)]
    pub numDenials: c_int,
    #[builder(default)]
    pub killStreak: c_int,
    #[builder(default)]
    pub maxKillStreak: c_int,
    #[builder(default)]
    pub xp: c_int,
    #[builder(default)]
    pub domThreeFlagsTime: c_int,
    #[builder(default)]
    pub numMidairShotgunKills: c_int,
}

// client data that stays across multiple respawns, but is cleared
// on each level change or team change at ClientBegin()
#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "ClientPersistantBuilder")]
#[repr(C, align(8))]
pub struct clientPersistant_t {
    #[builder(default = "clientConnected_t::CON_CONNECTED")]
    pub connected: clientConnected_t,
    pub cmd: usercmd_t,
    #[builder(default = "qboolean::qfalse")]
    pub localClient: qboolean,
    #[builder(default = "qboolean::qfalse")]
    pub initialSpawn: qboolean,
    #[builder(default = "qboolean::qfalse")]
    pub predictItemPickup: qboolean,
    #[builder(
        default = "[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]"
    )]
    pub netname: [c_char; 40usize],
    #[builder(default)]
    pub country: [c_char; 24usize],
    #[builder(default)]
    pub steamId: u64,
    #[builder(default)]
    pub maxHealth: c_int,
    #[builder(default)]
    pub voteCount: c_int,
    #[builder(default = "voteState_t::VOTE_NONE")]
    pub voteState: voteState_t,
    #[builder(default)]
    pub complaints: c_int,
    #[builder(default)]
    pub complaintClient: c_int,
    #[builder(default)]
    pub complaintEndTime: c_int,
    #[builder(default)]
    pub damageFromTeammates: c_int,
    #[builder(default)]
    pub damageToTeammates: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub ready: qboolean,
    #[builder(default)]
    pub autoaction: c_int,
    #[builder(default)]
    pub timeouts: c_int,
    #[builder(default)]
    pub enterTime: c_int,
    pub teamState: playerTeamState_t,
    #[builder(default)]
    pub damageResidual: c_int,
    #[builder(default)]
    pub inactivityTime: c_int,
    #[builder(default)]
    pub inactivityWarning: c_int,
    #[builder(default)]
    pub lastUserinfoUpdate: c_int,
    #[builder(default)]
    pub userInfoFloodInfractions: c_int,
    #[builder(default)]
    pub lastMapVoteTime: c_int,
    #[builder(default)]
    pub lastMapVoteIndex: c_int,
}

// client data that stays across multiple levels or tournament restarts
// this is achieved by writing all the data to cvar strings at game shutdown
// time and reading them back at connection time.  Anything added here
// MUST be dealt with in G_InitSessionData() / G_ReadSessionData() / G_WriteSessionData()
#[repr(C)]
#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "ClientSessionBuilder")]
pub struct clientSession_t {
    #[builder(default = "team_t::TEAM_SPECTATOR")]
    pub sessionTeam: team_t,
    #[builder(default)]
    pub spectatorTime: c_int,
    #[builder(default = "spectatorState_t::SPECTATOR_FREE")]
    pub spectatorState: spectatorState_t,
    #[builder(default)]
    pub spectatorClient: c_int,
    #[builder(default)]
    pub weaponPrimary: c_int,
    #[builder(default)]
    pub wins: c_int,
    #[builder(default)]
    pub losses: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub teamLeader: qboolean,
    #[builder(default = "privileges_t::PRIV_NONE")]
    pub privileges: privileges_t,
    #[builder(default)]
    pub specOnly: c_int,
    #[builder(default)]
    pub playQueue: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub updatePlayQueue: qboolean,
    #[builder(default)]
    pub muted: c_int,
    #[builder(default)]
    pub prevScore: c_int,
}

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

#[repr(C)]
#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "GEntityBuilder")]
pub struct gentity_s {
    pub s: entityState_t,
    pub r: entityShared_t,
    #[builder(default = "std::ptr::null_mut()")]
    pub client: *mut gclient_s,
    #[builder(default = "qboolean::qtrue")]
    pub inuse: qboolean,
    #[builder(default = "std::ptr::null_mut()")]
    pub classname: *mut c_char,
    #[builder(default = "0")]
    pub spawnflags: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub neverFree: qboolean,
    #[builder(default = "0")]
    pub flags: c_int,
    #[builder(default = "std::ptr::null_mut()")]
    pub model: *mut c_char,
    #[builder(default = "std::ptr::null_mut()")]
    pub model2: *mut c_char,
    #[builder(default = "0")]
    pub freetime: c_int,
    #[builder(default = "0")]
    pub eventTime: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub freeAfterEvent: qboolean,
    #[builder(default = "qboolean::qfalse")]
    pub unlinkAfterEvent: qboolean,
    #[builder(default = "qboolean::qfalse")]
    pub physicsObject: qboolean,
    #[builder(default = "0.0")]
    pub physicsBounce: f32,
    #[builder(default = "0")]
    pub clipmask: c_int,
    #[builder(default = "moverState_t::MOVER_POS1")]
    pub moverState: moverState_t,
    #[builder(default = "0")]
    pub soundPos1: c_int,
    #[builder(default = "0")]
    pub sound1to2: c_int,
    #[builder(default = "0")]
    pub sound2to1: c_int,
    #[builder(default = "0")]
    pub soundPos2: c_int,
    #[builder(default = "0")]
    pub soundLoop: c_int,
    #[builder(default = "std::ptr::null_mut()")]
    pub parent: *mut gentity_t,
    #[builder(default = "std::ptr::null_mut()")]
    pub nextTrain: *mut gentity_t,
    #[builder(default = "std::ptr::null_mut()")]
    pub prevTrain: *mut gentity_t,
    #[builder(default = "[0.0; 3]")]
    pub pos1: vec3_t,
    #[builder(default = "[0.0; 3]")]
    pub pos2: vec3_t,
    #[builder(default = "std::ptr::null_mut()")]
    pub message: *mut c_char,
    #[builder(default = "std::ptr::null_mut()")]
    pub cvar: *mut c_char,
    #[builder(default = "std::ptr::null_mut()")]
    pub tourPointTarget: *mut c_char,
    #[builder(default = "std::ptr::null_mut()")]
    pub tourPointTargetName: *mut c_char,
    #[builder(default = "std::ptr::null_mut()")]
    pub noise: *mut c_char,
    #[builder(default = "0")]
    pub timestamp: c_int,
    #[builder(default = "0.0")]
    pub angle: f32,
    #[builder(default = "std::ptr::null_mut()")]
    pub target: *mut c_char,
    #[builder(default = "std::ptr::null_mut()")]
    pub targetname: *mut c_char,
    #[builder(default = "std::ptr::null_mut()")]
    pub targetShaderName: *mut c_char,
    #[builder(default = "std::ptr::null_mut()")]
    pub targetShaderNewName: *mut c_char,
    #[builder(default = "std::ptr::null_mut()")]
    pub target_ent: *mut gentity_t,
    #[builder(default = "0.0")]
    pub speed: f32,
    #[builder(default = "[0.0; 3]")]
    pub movedir: vec3_t,
    #[builder(default = "0")]
    pub nextthink: c_int,
    #[builder(default = "None")]
    pub think: Option<unsafe extern "C" fn(arg1: *mut gentity_t)>,
    #[builder(default = "None")]
    pub framethink: Option<unsafe extern "C" fn(arg1: *mut gentity_t)>,
    #[builder(default = "None")]
    pub reached: Option<unsafe extern "C" fn(arg1: *mut gentity_t)>,
    #[builder(default = "None")]
    pub blocked: Option<unsafe extern "C" fn(arg1: *mut gentity_t, arg2: *mut gentity_t)>,
    #[builder(default = "None")]
    pub touch: Option<
        unsafe extern "C" fn(arg1: *mut gentity_t, arg2: *mut gentity_t, arg3: *mut trace_t),
    >,
    #[builder(default = "None")]
    pub use_: Option<
        unsafe extern "C" fn(arg1: *mut gentity_t, arg2: *mut gentity_t, arg3: *mut gentity_t),
    >,
    #[builder(default = "None")]
    pub pain: Option<unsafe extern "C" fn(arg1: *mut gentity_t, arg2: *mut gentity_t, arg3: c_int)>,
    #[builder(default = "None")]
    pub die: Option<
        unsafe extern "C" fn(
            arg1: *mut gentity_t,
            arg2: *mut gentity_t,
            arg3: *mut gentity_t,
            arg4: c_int,
            arg5: c_int,
        ),
    >,
    #[builder(default = "0")]
    pub pain_debounce_time: c_int,
    #[builder(default = "0")]
    pub fly_sound_debounce_time: c_int,
    #[builder(default = "0")]
    pub health: c_int,
    #[builder(default = "qboolean::qtrue")]
    pub takedamage: qboolean,
    #[builder(default = "0")]
    pub damage: c_int,
    #[builder(default = "0")]
    pub damageFactor: c_int,
    #[builder(default = "0")]
    pub splashDamage: c_int,
    #[builder(default = "0")]
    pub splashRadius: c_int,
    #[builder(default = "0")]
    pub methodOfDeath: c_int,
    #[builder(default = "0")]
    pub splashMethodOfDeath: c_int,
    #[builder(default = "0")]
    pub count: c_int,
    #[builder(default = "std::ptr::null_mut() as *mut gentity_t")]
    pub enemy: *mut gentity_t,
    #[builder(default = "std::ptr::null_mut() as *mut gentity_t")]
    pub activator: *mut gentity_t,
    #[builder(default = "std::ptr::null()")]
    pub team: *const c_char,
    #[builder(default = "std::ptr::null_mut() as *mut gentity_t")]
    pub teammaster: *mut gentity_t,
    #[builder(default = "std::ptr::null_mut() as *mut gentity_t")]
    pub teamchain: *mut gentity_t,
    #[builder(default = "0")]
    pub kamikazeTime: c_int,
    #[builder(default = "0")]
    pub kamikazeShockTime: c_int,
    #[builder(default = "0")]
    pub watertype: c_int,
    #[builder(default = "0")]
    pub waterlevel: c_int,
    #[builder(default = "0")]
    pub noise_index: c_int,
    #[builder(default = "0")]
    pub bouncecount: c_int,
    #[builder(default = "0.0")]
    pub wait: f32,
    #[builder(default = "0.0")]
    pub random: f32,
    #[builder(default = "0")]
    pub spawnTime: c_int,
    #[builder(default = "std::ptr::null()")]
    pub item: *const gitem_t,
    #[builder(default = "0")]
    pub pickupCount: c_int,
}

#[derive(Debug, PartialEq, Clone, Builder)]
#[builder(name = "RaceInfoBuilder")]
#[repr(C)]
pub struct raceInfo_t {
    #[builder(default = "qboolean::qfalse")]
    pub racingActive: qboolean,
    #[builder(default)]
    pub startTime: c_int,
    #[builder(default)]
    pub lastTime: c_int,
    #[builder(
        default = "[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]"
    )]
    pub best_race: [c_int; 64usize],
    #[builder(
        default = "[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]"
    )]
    pub current_race: [c_int; 64usize],
    #[builder(default)]
    pub currentCheckPoint: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub weaponUsed: qboolean,
    #[builder(default = "std::ptr::null_mut()")]
    pub nextRacePoint: *mut gentity_t,
    #[builder(default = "std::ptr::null_mut()")]
    pub nextRacePoint2: *mut gentity_t,
}

// this structure is cleared on each ClientSpawn(),
// except for 'client->pers' and 'client->sess'
#[derive(Debug, PartialEq, Builder)]
#[builder(name = "GClientBuilder")]
#[repr(C, align(8))]
pub struct gclient_s {
    pub ps: playerState_t,
    pub pers: clientPersistant_t,
    pub sess: clientSession_t,
    #[builder(default = "qboolean::qfalse")]
    pub noclip: qboolean,
    #[builder(default)]
    pub lastCmdTime: c_int,
    #[builder(default)]
    pub buttons: c_int,
    #[builder(default)]
    pub oldbuttons: c_int,
    #[builder(default)]
    pub damage_armor: c_int,
    #[builder(default)]
    pub damage_blood: c_int,
    #[builder(default)]
    pub damage_from: vec3_t,
    #[builder(default = "qboolean::qfalse")]
    pub damage_fromWorld: qboolean,
    #[builder(default)]
    pub impressiveCount: c_int,
    #[builder(default)]
    pub accuracyCount: c_int,
    #[builder(default)]
    pub accuracy_shots: c_int,
    #[builder(default)]
    pub accuracy_hits: c_int,
    #[builder(default)]
    pub lastClientKilled: c_int,
    #[builder(default)]
    pub lastKilledClient: c_int,
    #[builder(default)]
    pub lastHurtClient: [c_int; 2usize],
    #[builder(default)]
    pub lastHurtMod: [c_int; 2usize],
    #[builder(default)]
    pub lastHurtTime: [c_int; 2usize],
    #[builder(default)]
    pub lastKillTime: c_int,
    #[builder(default)]
    pub lastGibTime: c_int,
    #[builder(default)]
    pub rampageCounter: c_int,
    #[builder(
        default = "[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]"
    )]
    pub revengeCounter: [c_int; 64usize],
    #[builder(default)]
    pub respawnTime: c_int,
    #[builder(default)]
    pub rewardTime: c_int,
    #[builder(default)]
    pub airOutTime: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub fireHeld: qboolean,
    #[builder(default = "std::ptr::null_mut()")]
    pub hook: *mut gentity_t,
    #[builder(default)]
    pub switchTeamTime: c_int,
    #[builder(default)]
    pub timeResidual: c_int,
    #[builder(default)]
    pub timeResidualScout: c_int,
    #[builder(default)]
    pub timeResidualArmor: c_int,
    #[builder(default)]
    pub timeResidualHealth: c_int,
    #[builder(default)]
    pub timeResidualPingPOI: c_int,
    #[builder(default)]
    pub timeResidualSpecInfo: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub healthRegenActive: qboolean,
    #[builder(default = "qboolean::qfalse")]
    pub armorRegenActive: qboolean,
    #[builder(default = "std::ptr::null_mut()")]
    pub persistantPowerup: *mut gentity_t,
    #[builder(default)]
    pub portalID: c_int,
    #[builder(default)]
    pub ammoTimes: [c_int; 16usize],
    #[builder(default)]
    pub invulnerabilityTime: c_int,
    pub expandedStats: expandedStatObj_t,
    #[builder(default)]
    pub ignoreChatsTime: c_int,
    #[builder(default)]
    pub lastUserCmdTime: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub freezePlayer: qboolean,
    #[builder(default)]
    pub deferredSpawnTime: c_int,
    #[builder(default)]
    pub deferredSpawnCount: c_int,
    pub race: raceInfo_t,
    #[builder(
        default = "[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]"
    )]
    pub shotgunDmg: [c_int; 64usize],
    #[builder(default)]
    pub round_shots: c_int,
    #[builder(default)]
    pub round_hits: c_int,
    #[builder(default)]
    pub round_damage: c_int,
    #[builder(default = "qboolean::qfalse")]
    pub queuedSpectatorFollow: qboolean,
    #[builder(default)]
    pub queuedSpectatorClient: c_int,
}

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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum healthPickup_t {
    H_NONE = 0,
    H_MEGA = 1,
    H_LARGE = 2,
    H_MEDIUM = 3,
    H_SMALL = 4,
    H_NUM_HEALTHS = 5,
}
