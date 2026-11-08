from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import sys
    from typing import Literal, TypedDict

    if sys.version_info >= (3, 11):
        from typing import NotRequired
    else:
        from typing_extensions import NotRequired

from . import database
from ._commands import (
    BLUE_TEAM_CHAT_CHANNEL,
    CHAT_CHANNEL,
    COMMANDS,
    CONSOLE_CHANNEL,
    FREE_CHAT_CHANNEL,
    MAX_MSG_LENGTH,
    RED_TEAM_CHAT_CHANNEL,
    SPECTATOR_CHAT_CHANNEL,
    AbstractChannel,
    ChatChannel,
    ClientCommandChannel,
    Command,
    CommandInvoker,
    ConsoleChannel,
    TeamChatChannel,
    TellChannel,
    re_color_tag,
)
from ._core import (
    DEFAULT_PLUGINS,
    PluginLoadError,
    PluginUnloadError,
    _configure_logger,
    _modules,
    _stats,
    _thread_count,
    _thread_name,
    delay,
    get_logger,
    handle_exception,
    initialize,
    initialize_cvars,
    late_init,
    load_plugin,
    load_preset_plugins,
    log_exception,
    next_frame,
    owner,
    parse_variables,
    reload_plugin,
    set_map_subtitles,
    set_plugins_version,
    stats_listener,
    thread,
    threading_excepthook,
    unload_plugin,
    uptime,
)
from ._events import (
    EVENT_DISPATCHERS,
    ChatEventDispatcher,
    ClientCommandDispatcher,
    CommandDispatcher,
    ConsolePrintDispatcher,
    DamageDispatcher,
    DeathDispatcher,
    EventDispatcher,
    EventDispatcherManager,
    FrameEventDispatcher,
    GameCountdownDispatcher,
    GameEndDispatcher,
    GameStartDispatcher,
    KamikazeExplodeDispatcher,
    KamikazeUseDispatcher,
    KillDispatcher,
    MapDispatcher,
    NewGameDispatcher,
    PlayerConnectDispatcher,
    PlayerDisconnectDispatcher,
    PlayerLoadedDispatcher,
    PlayerSpawnDispatcher,
    RoundCountdownDispatcher,
    RoundEndDispatcher,
    RoundStartDispatcher,
    ServerCommandDispatcher,
    SetConfigstringDispatcher,
    StatsDispatcher,
    TeamSwitchAttemptDispatcher,
    TeamSwitchDispatcher,
    UnloadDispatcher,
    UserinfoDispatcher,
    VoteCalledDispatcher,
    VoteDispatcher,
    VoteEndedDispatcher,
    VoteStartedDispatcher,
    _re_vote,
)
from ._game import (
    Game,
    NonexistentGameError,
)
from ._handlers import (
    frame_tasks,
    handle_client_command,
    handle_console_print,
    handle_damage,
    handle_frame,
    handle_kamikaze_explode,
    handle_kamikaze_use,
    handle_new_game,
    handle_player_connect,
    handle_player_disconnect,
    handle_player_loaded,
    handle_player_spawn,
    handle_rcon,
    handle_server_command,
    handle_set_configstring,
    next_frame_tasks,
    redirect_print,
    register_handlers,
)
from ._player import (
    AbstractDummyPlayer,
    NonexistentPlayerError,
    Player,
    RconDummyPlayer,
)
from ._plugin import Plugin, command, cvar, hook
from ._shinqlx import (
    CS_ACTIVE,
    CS_CONNECTED,
    CS_FREE,
    CS_PRIMED,
    CS_ZOMBIE,
    CVAR_ARCHIVE,
    CVAR_CHEAT,
    CVAR_INIT,
    CVAR_LATCH,
    CVAR_NORESTART,
    CVAR_ROM,
    CVAR_SERVERINFO,
    CVAR_SYSTEMINFO,
    CVAR_TEMP,
    CVAR_USER_CREATED,
    CVAR_USERINFO,
    DAMAGE_NO_ARMOR,
    DAMAGE_NO_KNOCKBACK,
    DAMAGE_NO_PROTECTION,
    DAMAGE_NO_TEAM_PROTECTION,
    DAMAGE_RADIUS,
    MOD_BFG,
    MOD_BFG_SPLASH,
    MOD_CHAINGUN,
    MOD_CRUSH,
    MOD_FALLING,
    MOD_GAUNTLET,
    MOD_GRAPPLE,
    MOD_GRENADE,
    MOD_GRENADE_SPLASH,
    MOD_HMG,
    MOD_JUICED,
    MOD_KAMIKAZE,
    MOD_LAVA,
    MOD_LIGHTNING,
    MOD_LIGHTNING_DISCHARGE,
    MOD_MACHINEGUN,
    MOD_NAIL,
    MOD_PLASMA,
    MOD_PLASMA_SPLASH,
    MOD_PROXIMITY_MINE,
    MOD_RAILGUN,
    MOD_RAILGUN_HEADSHOT,
    MOD_ROCKET,
    MOD_ROCKET_SPLASH,
    MOD_SHOTGUN,
    MOD_SLIME,
    MOD_SUICIDE,
    MOD_SWITCH_TEAMS,
    MOD_TARGET_LASER,
    MOD_TELEFRAG,
    MOD_THAW,
    MOD_TRIGGER_HURT,
    MOD_UNKNOWN,
    MOD_WATER,
    PRI_HIGH,
    PRI_HIGHEST,
    PRI_LOW,
    PRI_LOWEST,
    PRI_NORMAL,
    PRIV_ADMIN,
    PRIV_BANNED,
    PRIV_MOD,
    PRIV_NONE,
    PRIV_ROOT,
    RET_NONE,
    RET_STOP,
    RET_STOP_ALL,
    RET_STOP_EVENT,
    RET_USAGE,
    TEAM_BLUE,
    TEAM_FREE,
    TEAM_RED,
    TEAM_SPECTATOR,
    ConfigString,
    ConnectionStates,
    CvarFlags,
    DamageFlags,
    Flight,
    MeansOfDeath,
    Permissions,
    PlayerInfo,
    PlayerState,
    PlayerStats,
    Powerups,
    Priorities,
    Privileges,
    ReturnCodes,
    Teams,
    Vector3,
    VoteStates,
    Weapon,
    Weapons,
    add_console_command,
    allow_single_player,
    callvote,
    client_command,
    console_command,
    console_print,
    destroy_kamikaze_timers,
    dev_print_items,
    drop_holdable,
    force_vote,
    force_weapon_respawn_time,
    get_configstring,
    get_cvar,
    get_targetting_entities,
    get_userinfo,
    kick,
    noclip,
    player_info,
    player_spawn,
    player_state,
    player_stats,
    players_info,
    register_handler,
    remove_dropped_items,
    replace_items,
    send_server_command,
    set_ammo,
    set_armor,
    set_configstring,
    set_cvar,
    set_cvar_limit,
    set_cvar_limit_once,
    set_cvar_once,
    set_flight,
    set_health,
    set_holdable,
    set_invulnerability,
    set_position,
    set_powerups,
    set_privileges,
    set_score,
    set_velocity,
    set_weapon,
    set_weapons,
    slay_with_mod,
    spawn_item,
)
from ._zmq import StatsListener

_map_title: str | None
_map_subtitle1: str | None
_map_subtitle2: str | None

__version__: str
__version_info__: tuple[int, int, int]
__plugins_version__: str
DEBUG: bool

TEAMS: dict[int, str]

# game types
GAMETYPES: dict[int, str]
GAMETYPES_SHORT: dict[int, str]

CONNECTION_STATES: dict[int, str]
WEAPONS: dict[int, str]

class UserInfo(TypedDict):
    ip: str
    ui_singlePlayerActive: str
    cg_autoAction: str
    cg_autoHop: str
    cg_predictItems: str
    model: str
    headmodel: str
    cl_anonymous: str
    country: str
    color1: str
    rate: str
    color2: str
    sex: str
    teamtask: str
    name: str
    handicap: str
    password: NotRequired[str]

UncancellableEventReturn = Literal[0] | None
CancellableEventReturn = Literal[0, 1, 2, 3] | None

class PlayerSummaryData(TypedDict):
    NAME: str
    STEAM_ID: str
    TEAM: int

class GameStartData(TypedDict):
    CAPTURE_LIMIT: int
    FACTORY: str
    FACTORY_TITLE: str
    FRAG_LIMIT: int
    GAME_TYPE: str
    INFECTED: int
    INSTAGIB: int
    MAP: str
    MATCH_GUID: str
    MERCY_LIMIT: int
    PLAYERS: list[PlayerSummaryData]
    QUADHOG: int
    ROUND_LIMIT: int
    SCORE_LIMIT: int
    SERVER_TITLE: str
    TIME_LIMIT: int
    TRAINING: int

class GameEndData(TypedDict):
    ABORTED: bool
    CAPTURE_LIMIT: int
    EXIT_MSG: str
    FACTORY: str
    FACTORY_TITLE: str
    FIRST_SCORER: str
    FRAG_LIMIT: int
    GAME_LENGTH: int
    GAME_TYPE: str
    INFECTED: int
    INSTAGIB: int
    LAST_LEAD_CHANGE_TIME: int
    LAST_SCORER: str
    LAST_TEAMSCORER: str
    MAP: str
    MATCH_GUID: str
    MERCY_LIMIT: int
    QUADHOG: int
    RESTARTED: int
    ROUND_LIMIT: int
    SCORE_LIMIT: int
    SERVER_TITLE: str
    TIME_LIMIT: int
    TRAINING: int
    TSCORE0: int
    TSCORE1: int

class RoundEndData(TypedDict):
    MATCH_GUID: str
    ROUND: int
    TEAM_WON: Literal["RED", "BLUE", "DRAW"]
    TIME: int
    WARMUP: bool

class Vector(TypedDict):
    x: float
    y: float
    z: float

PowerUps = Literal["QUAD", "BATTLESUIT", "HASTE", "INVISIBILITY", "REGENERATION", "INVULNERABILITY"]
Holdable = Literal["TELEPORTER", "MEDKIT", "FLIGHT", "KAMIKAZE", "PORTAL", "INVULNERABILITY"]
StatsWeapon = Literal[
    "GAUNTLET",
    "MACHINEGUN",
    "SHOTGUN",
    "GRENADE",
    "ROCKET",
    "LIGHTNING",
    "RAILGUN",
    "PLASMA",
    "BFG",
    "GRAPPLE",
    "NAIL",
    "PROXIMITY",
    "CHAINGUN",
    "HMG",
    "HANDS",
]

class PlayerData(TypedDict):
    AIRBORNE: bool
    AMMO: int
    ARMOR: int
    BOT: bool
    BOT_SKILL: int | None
    HEALTH: int
    HOLDABLE: Holdable | None
    NAME: str
    POSITION: Vector
    POWERUPS: list[PowerUps] | None
    SPEED: float
    STEAM_ID: str
    SUBMERGED: bool
    TEAM: int
    VIEW: Vector
    WEAPON: StatsWeapon

StatsMeansOfDeath = Literal[
    "UNKNOWN",
    "SHOTGUN",
    "GAUNTLET",
    "MACHINEGUN",
    "GRENADE",
    "GRENADE_SPLASH",
    "ROCKET",
    "ROCKET_SPLASH",
    "PLASMA",
    "PLASMA_SPLASH",
    "RAILGUN",
    "LIGHTNING",
    "BFG",
    "BFG_SPLASH",
    "WATER",
    "SLIME",
    "LAVA",
    "CRUSH",
    "TELEFRAG",
    "FALLING",
    "SUICIDE",
    "TARGET_LASER",
    "HURT",
    "NAIL",
    "CHAINGUN",
    "PROXIMITY_MINE",
    "KAMIKAZE",
    "JUICED",
    "GRAPPLE",
    "SWITCHTEAM",
    "THAW",
    "LIGHTNING_DISCHARGE",
    "HMG",
    "RAILGUN_HEADSHOT",
]

class KillData(TypedDict):
    KILLER: PlayerData
    VICTIM: PlayerData
    MATCH_GUID: str
    MOD: StatsMeansOfDeath
    OTHER_TEAM_ALIVE: int
    OTHER_TEAM_DEAD: int
    ROUND: int
    SUICIDE: bool
    TEAMKILL: bool
    TEAM_ALIVE: int
    TEAM_DEAD: int
    TIME: int
    WARMUP: bool

class DeathData(TypedDict):
    KILLER: PlayerData | None
    VICTIM: PlayerData
    MATCH_GUID: str
    MOD: StatsMeansOfDeath
    OTHER_TEAM_ALIVE: int
    OTHER_TEAM_DEAD: int
    ROUND: int
    SUICIDE: bool
    TEAMKILL: bool
    TEAM_ALIVE: int
    TEAM_DEAD: int
    TIME: int
    WARMUP: bool

UserinfoEventInput = TypedDict(
    "UserinfoEventInput",
    {
        "ip": str,
        "ui_singlePlayerActive": str,
        "cg_autoAction": str,
        "cg_autoHop": str,
        "cg_predictItems": str,
        "model": str,
        "headmodel": str,
        "cl_anonymous": str,
        "countr<": str,
        "color1": str,
        "rate": str,
        "color2": str,
        "sex": str,
        "teamtask": str,
        "name": str,
        "handicap": str,
        "password": str,
    },
    total=False,
)

class PlayerKillStats(TypedDict):
    DATA: KillData
    TYPE: Literal["PLAYER_KILL"]

class PlayerDeathStats(TypedDict):
    DATA: DeathData
    TYPE: Literal["PLAYER_DEATH"]

class MedalData(TypedDict):
    MATCH_GUID: str
    MEDAL: Literal[
        "ACCURACY",
        "ASSISTS",
        "CAPTURES",
        "COMBOKILL",
        "DEFENDS",
        "EXCELLENT",
        "FIRSTFRAG",
        "HEADSHOT",
        "HUMILIATION",
        "IMPRESSIVE",
        "MIDAIR",
        "PERFECT",
        "PERFORATED",
        "QUADGOD",
        "RAMPAGE",
        "REVENGE",
    ]
    NAME: str
    STEAM_ID: str
    TIME: int
    TOTAL: int
    WARMUP: bool

class PlayerMedalStats(TypedDict):
    DATA: MedalData
    TYPE: Literal["PLAYER_MEDAL"]

class RoundOverStats(TypedDict):
    DATA: RoundEndData
    TYPE: Literal["ROUND_OVER"]

class PlayerGameData(TypedDict):
    MATCH_GUID: str
    NAME: str
    STEAM_ID: str
    TIME: int
    WARMUP: bool

class PlayerConnectStats(TypedDict):
    DATA: PlayerGameData
    TYPE: Literal["PLAYER_CONNECT"]

class PlayerDisconnectStats(TypedDict):
    DATA: PlayerGameData
    TYPE: Literal["PLAYER_DICCONNECT"]

class TeamSwitchEvent(TypedDict):
    NAME: str
    OLD_TEAM: str
    STEAM_ID: str
    TEAM: str

class TeamSwitchGameData(TypedDict):
    KILLER: TeamSwitchEvent
    MATCH_GUID: str
    TIME: int
    WARMUP: bool

class PlayerSwitchTeamStats(TypedDict):
    DATA: TeamSwitchGameData
    TYPE: Literal["PLAYER_SWITCHTEAM"]

class MatchStartedStats(TypedDict):
    DATA: GameStartData
    TYPE: Literal["MATCH_STARTED"]

class MatchReportStats(TypedDict):
    DATA: GameEndData
    TYPE: Literal["MATCH_REPORT"]

class DamageEntry(TypedDict):
    DEALT: int
    TAKEN: int

class MedalsEntry(TypedDict):
    ACCURACY: int
    ASSISTS: int
    CAPTURES: int
    COMBOKILL: int
    DEFENDS: int
    EXCELLENT: int
    FIRSTFRAG: int
    HEADSHOT: int
    HUMILIATION: int
    IMPRESSIVE: int
    MIDAIR: int
    PERFECT: int
    PERFORATED: int
    QUADGOD: int
    RAMPAGE: int
    REVENGE: int

class PickupsEntry(TypedDict):
    AMMO: int
    ARMOR: int
    ARMOR_REGEN: int
    BATTLESUIT: int
    DOUBLER: int
    FLIGHT: int
    GREEN_ARMOR: int
    GUARD: int
    HASTE: int
    HEALTH: int
    INVIS: int
    INVULNERABILITY: int
    KAMIKAZE: int
    MEDKIT: int
    MEGA_HEALTH: int
    OTHER_HOLDABLE: int
    OTHER_POWERUP: int
    PORTAL: int
    QUAD: int
    RED_ARMOR: int
    REGEN: int
    SCOUT: int
    TELEPORTER: int
    YELLOW_ARMOR: int

class SingleWeaponStatsEntry(TypedDict):
    D: int
    DG: int
    DR: int
    H: int
    K: int
    P: int
    S: int
    T: int

class WeaponsStatsEntry(TypedDict):
    BFG: SingleWeaponStatsEntry
    CHAINGUN: SingleWeaponStatsEntry
    GAUNTLET: SingleWeaponStatsEntry
    GRENADE: SingleWeaponStatsEntry
    HMG: SingleWeaponStatsEntry
    LIGHTNING: SingleWeaponStatsEntry
    MACHINEGUN: SingleWeaponStatsEntry
    NAILGUN: SingleWeaponStatsEntry
    OTHER_WEAPON: SingleWeaponStatsEntry
    PLASMA: SingleWeaponStatsEntry
    PROXMINE: SingleWeaponStatsEntry
    RAILGUN: SingleWeaponStatsEntry
    ROCKET: SingleWeaponStatsEntry
    SHOTGUN: SingleWeaponStatsEntry

class PlayerStatsEntry(TypedDict):
    ABORTED: bool
    BLUE_FLAG_PICKUPS: int
    DAMAGE: DamageEntry
    DEATHS: int
    HOLY_SHITS: int
    KILLS: int
    LOSE: int
    MATCH_GUID: str
    MAX_STREAK: int
    MEDALS: MedalsEntry
    MODEL: str
    NAME: str
    NEUTRAL_FLAG_PICKUPS: int
    PICKUPS: PickupsEntry
    PLAY_TIME: int
    QUIT: int
    RANK: int
    RED_FLAG_PICKUPS: int
    SCORE: int
    STEAM_ID: str
    TEAM: int
    TEAM_JOIN_TIME: int
    TEAM_RANK: int
    TIED_RANK: int
    TIED_TEAM_RANK: int
    WARMUP: bool
    WEAPONS: WeaponsStatsEntry
    WIN: int

class PlayerStatsStats(TypedDict):
    DATA: PlayerStatsEntry
    TYPE: Literal["PLAYER_STATS"]

StatsData = (
    PlayerKillStats
    | PlayerDeathStats
    | PlayerMedalStats
    | RoundOverStats
    | PlayerConnectStats
    | PlayerDisconnectStats
    | PlayerSwitchTeamStats
    | MatchStartedStats
    | MatchReportStats
    | PlayerStatsStats
)

__all__ = [
    "BLUE_TEAM_CHAT_CHANNEL",
    "CHAT_CHANNEL",
    "COMMANDS",
    "CONNECTION_STATES",
    "CONSOLE_CHANNEL",
    "CS_ACTIVE",
    "CS_CONNECTED",
    "CS_FREE",
    "CS_PRIMED",
    "CS_ZOMBIE",
    "CVAR_ARCHIVE",
    "CVAR_CHEAT",
    "CVAR_INIT",
    "CVAR_LATCH",
    "CVAR_NORESTART",
    "CVAR_ROM",
    "CVAR_SERVERINFO",
    "CVAR_SYSTEMINFO",
    "CVAR_TEMP",
    "CVAR_USERINFO",
    "CVAR_USER_CREATED",
    "DAMAGE_NO_ARMOR",
    "DAMAGE_NO_KNOCKBACK",
    "DAMAGE_NO_PROTECTION",
    "DAMAGE_NO_TEAM_PROTECTION",
    "DAMAGE_RADIUS",
    "DEBUG",
    "DEFAULT_PLUGINS",
    "EVENT_DISPATCHERS",
    "FREE_CHAT_CHANNEL",
    "GAMETYPES",
    "GAMETYPES_SHORT",
    "MAX_MSG_LENGTH",
    "MOD_BFG",
    "MOD_BFG_SPLASH",
    "MOD_CHAINGUN",
    "MOD_CRUSH",
    "MOD_FALLING",
    "MOD_GAUNTLET",
    "MOD_GRAPPLE",
    "MOD_GRENADE",
    "MOD_GRENADE_SPLASH",
    "MOD_HMG",
    "MOD_JUICED",
    "MOD_KAMIKAZE",
    "MOD_LAVA",
    "MOD_LIGHTNING",
    "MOD_LIGHTNING_DISCHARGE",
    "MOD_MACHINEGUN",
    "MOD_NAIL",
    "MOD_PLASMA",
    "MOD_PLASMA_SPLASH",
    "MOD_PROXIMITY_MINE",
    "MOD_RAILGUN",
    "MOD_RAILGUN_HEADSHOT",
    "MOD_ROCKET",
    "MOD_ROCKET_SPLASH",
    "MOD_SHOTGUN",
    "MOD_SLIME",
    "MOD_SUICIDE",
    "MOD_SWITCH_TEAMS",
    "MOD_TARGET_LASER",
    "MOD_TELEFRAG",
    "MOD_THAW",
    "MOD_TRIGGER_HURT",
    "MOD_UNKNOWN",
    "MOD_WATER",
    "PRIV_ADMIN",
    "PRIV_BANNED",
    "PRIV_MOD",
    "PRIV_NONE",
    "PRIV_ROOT",
    "PRI_HIGH",
    "PRI_HIGHEST",
    "PRI_LOW",
    "PRI_LOWEST",
    "PRI_NORMAL",
    "RED_TEAM_CHAT_CHANNEL",
    "RET_NONE",
    "RET_STOP",
    "RET_STOP_ALL",
    "RET_STOP_EVENT",
    "RET_USAGE",
    "SPECTATOR_CHAT_CHANNEL",
    "TEAMS",
    "TEAM_BLUE",
    "TEAM_FREE",
    "TEAM_RED",
    "TEAM_SPECTATOR",
    "WEAPONS",
    "AbstractChannel",
    "AbstractDummyPlayer",
    "CancellableEventReturn",
    "ChatChannel",
    "ChatEventDispatcher",
    "ClientCommandChannel",
    "ClientCommandDispatcher",
    "Command",
    "CommandDispatcher",
    "CommandInvoker",
    "ConfigString",
    "ConnectionStates",
    "ConsoleChannel",
    "ConsolePrintDispatcher",
    "CvarFlags",
    "DamageDispatcher",
    "DamageEntry",
    "DamageFlags",
    "DeathData",
    "DeathDispatcher",
    "EventDispatcher",
    "EventDispatcherManager",
    "Flight",
    "FrameEventDispatcher",
    "Game",
    "GameCountdownDispatcher",
    "GameEndData",
    "GameEndDispatcher",
    "GameStartData",
    "GameStartDispatcher",
    "Holdable",
    "KamikazeExplodeDispatcher",
    "KamikazeUseDispatcher",
    "KillData",
    "KillDispatcher",
    "MapDispatcher",
    "MatchReportStats",
    "MatchStartedStats",
    "MeansOfDeath",
    "MedalData",
    "MedalsEntry",
    "NewGameDispatcher",
    "NonexistentGameError",
    "NonexistentPlayerError",
    "Permissions",
    "PickupsEntry",
    "Player",
    "PlayerConnectDispatcher",
    "PlayerConnectStats",
    "PlayerData",
    "PlayerDeathStats",
    "PlayerDisconnectDispatcher",
    "PlayerDisconnectStats",
    "PlayerGameData",
    "PlayerInfo",
    "PlayerKillStats",
    "PlayerLoadedDispatcher",
    "PlayerMedalStats",
    "PlayerSpawnDispatcher",
    "PlayerState",
    "PlayerStats",
    "PlayerStatsEntry",
    "PlayerStatsStats",
    "PlayerSummaryData",
    "PlayerSwitchTeamStats",
    "Plugin",
    "PluginLoadError",
    "PluginUnloadError",
    "PowerUps",
    "Powerups",
    "Priorities",
    "Privileges",
    "RconDummyPlayer",
    "ReturnCodes",
    "RoundCountdownDispatcher",
    "RoundEndData",
    "RoundEndDispatcher",
    "RoundOverStats",
    "RoundStartDispatcher",
    "ServerCommandDispatcher",
    "SetConfigstringDispatcher",
    "SingleWeaponStatsEntry",
    "StatsData",
    "StatsDispatcher",
    "StatsListener",
    "StatsMeansOfDeath",
    "StatsWeapon",
    "TeamChatChannel",
    "TeamSwitchAttemptDispatcher",
    "TeamSwitchDispatcher",
    "TeamSwitchGameData",
    "Teams",
    "TellChannel",
    "UncancellableEventReturn",
    "UnloadDispatcher",
    "UserInfo",
    "UserinfoDispatcher",
    "UserinfoEventInput",
    "Vector",
    "Vector3",
    "VoteCalledDispatcher",
    "VoteDispatcher",
    "VoteEndedDispatcher",
    "VoteStartedDispatcher",
    "VoteStates",
    "Weapon",
    "Weapons",
    "WeaponsStatsEntry",
    "__plugins_version__",
    "__version__",
    "__version_info__",
    "_configure_logger",
    "_map_subtitle1",
    "_map_subtitle2",
    "_map_title",
    "_modules",
    "_re_vote",
    "_stats",
    "_thread_count",
    "_thread_name",
    "add_console_command",
    "allow_single_player",
    "callvote",
    "client_command",
    "command",
    "console_command",
    "console_print",
    "cvar",
    "database",
    "delay",
    "destroy_kamikaze_timers",
    "dev_print_items",
    "drop_holdable",
    "force_vote",
    "force_weapon_respawn_time",
    "frame_tasks",
    "get_configstring",
    "get_cvar",
    "get_logger",
    "get_targetting_entities",
    "get_userinfo",
    "handle_client_command",
    "handle_console_print",
    "handle_damage",
    "handle_exception",
    "handle_frame",
    "handle_kamikaze_explode",
    "handle_kamikaze_use",
    "handle_new_game",
    "handle_player_connect",
    "handle_player_disconnect",
    "handle_player_loaded",
    "handle_player_spawn",
    "handle_rcon",
    "handle_server_command",
    "handle_set_configstring",
    "hook",
    "initialize",
    "initialize_cvars",
    "kick",
    "late_init",
    "load_plugin",
    "load_preset_plugins",
    "log_exception",
    "next_frame",
    "next_frame_tasks",
    "noclip",
    "owner",
    "parse_variables",
    "player_info",
    "player_spawn",
    "player_state",
    "player_stats",
    "players_info",
    "re_color_tag",
    "redirect_print",
    "register_handler",
    "register_handlers",
    "reload_plugin",
    "remove_dropped_items",
    "replace_items",
    "send_server_command",
    "set_ammo",
    "set_armor",
    "set_configstring",
    "set_cvar",
    "set_cvar_limit",
    "set_cvar_limit_once",
    "set_cvar_once",
    "set_flight",
    "set_health",
    "set_holdable",
    "set_invulnerability",
    "set_map_subtitles",
    "set_plugins_version",
    "set_position",
    "set_powerups",
    "set_privileges",
    "set_score",
    "set_velocity",
    "set_weapon",
    "set_weapons",
    "slay_with_mod",
    "spawn_item",
    "stats_listener",
    "thread",
    "threading_excepthook",
    "unload_plugin",
    "uptime",
]
