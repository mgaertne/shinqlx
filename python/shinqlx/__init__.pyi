from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Literal, TypedDict
    import sys

    if sys.version_info >= (3, 11):
        from typing import NotRequired
    else:
        from typing_extensions import NotRequired

from ._commands import (
    MAX_MSG_LENGTH,
    re_color_tag,
    AbstractChannel,
    ConsoleChannel,
    ChatChannel,
    TellChannel,
    ClientCommandChannel,
    TeamChatChannel,
    CHAT_CHANNEL,
    RED_TEAM_CHAT_CHANNEL,
    BLUE_TEAM_CHAT_CHANNEL,
    FREE_CHAT_CHANNEL,
    SPECTATOR_CHAT_CHANNEL,
    CONSOLE_CHANNEL,
    Command,
    CommandInvoker,
    COMMANDS,
)
from ._core import (
    _thread_name,
    _thread_count,
    DEFAULT_PLUGINS,
    set_plugins_version,
    set_map_subtitles,
    parse_variables,
    get_logger,
    _configure_logger,
    handle_exception,
    log_exception,
    threading_excepthook,
    next_frame,
    delay,
    thread,
    uptime,
    owner,
    _stats,
    stats_listener,
    _modules,
    load_preset_plugins,
    load_plugin,
    unload_plugin,
    reload_plugin,
    initialize_cvars,
    initialize,
    late_init,
    PluginLoadError,
    PluginUnloadError,
)
from ._events import (
    _re_vote,
    EventDispatcher,
    ConsolePrintDispatcher,
    CommandDispatcher,
    ClientCommandDispatcher,
    ServerCommandDispatcher,
    FrameEventDispatcher,
    SetConfigstringDispatcher,
    ChatEventDispatcher,
    UnloadDispatcher,
    PlayerConnectDispatcher,
    PlayerLoadedDispatcher,
    PlayerDisconnectDispatcher,
    PlayerSpawnDispatcher,
    StatsDispatcher,
    VoteCalledDispatcher,
    VoteStartedDispatcher,
    VoteEndedDispatcher,
    VoteDispatcher,
    GameCountdownDispatcher,
    GameStartDispatcher,
    GameEndDispatcher,
    RoundCountdownDispatcher,
    RoundStartDispatcher,
    RoundEndDispatcher,
    TeamSwitchDispatcher,
    TeamSwitchAttemptDispatcher,
    MapDispatcher,
    NewGameDispatcher,
    KillDispatcher,
    DeathDispatcher,
    UserinfoDispatcher,
    KamikazeUseDispatcher,
    KamikazeExplodeDispatcher,
    DamageDispatcher,
    EventDispatcherManager,
    EVENT_DISPATCHERS,
)
from ._game import (
    NonexistentGameError,
    Game,
)
from ._handlers import (
    frame_tasks,
    next_frame_tasks,
    handle_rcon,
    handle_client_command,
    handle_server_command,
    handle_frame,
    handle_new_game,
    handle_set_configstring,
    handle_player_connect,
    handle_player_loaded,
    handle_player_spawn,
    handle_player_disconnect,
    handle_kamikaze_use,
    handle_kamikaze_explode,
    handle_damage,
    handle_console_print,
    redirect_print,
    register_handlers,
)
from ._player import (
    Player,
    NonexistentPlayerError,
    AbstractDummyPlayer,
    RconDummyPlayer,
)
from ._plugin import Plugin
from ._shinqlx import (
    RET_NONE,
    RET_STOP,
    RET_STOP_EVENT,
    RET_STOP_ALL,
    RET_USAGE,
    PRI_HIGHEST,
    PRI_HIGH,
    PRI_NORMAL,
    PRI_LOW,
    PRI_LOWEST,
    CVAR_ARCHIVE,
    CVAR_USERINFO,
    CVAR_SERVERINFO,
    CVAR_SYSTEMINFO,
    CVAR_INIT,
    CVAR_LATCH,
    CVAR_ROM,
    CVAR_USER_CREATED,
    CVAR_TEMP,
    CVAR_CHEAT,
    CVAR_NORESTART,
    PRIV_NONE,
    PRIV_MOD,
    PRIV_ADMIN,
    PRIV_ROOT,
    PRIV_BANNED,
    CS_FREE,
    CS_ZOMBIE,
    CS_CONNECTED,
    CS_PRIMED,
    CS_ACTIVE,
    TEAM_FREE,
    TEAM_RED,
    TEAM_BLUE,
    TEAM_SPECTATOR,
    MOD_UNKNOWN,
    MOD_SHOTGUN,
    MOD_GAUNTLET,
    MOD_MACHINEGUN,
    MOD_GRENADE,
    MOD_GRENADE_SPLASH,
    MOD_ROCKET,
    MOD_ROCKET_SPLASH,
    MOD_PLASMA,
    MOD_PLASMA_SPLASH,
    MOD_RAILGUN,
    MOD_LIGHTNING,
    MOD_BFG,
    MOD_BFG_SPLASH,
    MOD_WATER,
    MOD_SLIME,
    MOD_LAVA,
    MOD_CRUSH,
    MOD_TELEFRAG,
    MOD_FALLING,
    MOD_SUICIDE,
    MOD_TARGET_LASER,
    MOD_TRIGGER_HURT,
    MOD_NAIL,
    MOD_CHAINGUN,
    MOD_PROXIMITY_MINE,
    MOD_KAMIKAZE,
    MOD_JUICED,
    MOD_GRAPPLE,
    MOD_SWITCH_TEAMS,
    MOD_THAW,
    MOD_LIGHTNING_DISCHARGE,
    MOD_HMG,
    MOD_RAILGUN_HEADSHOT,
    DAMAGE_RADIUS,
    DAMAGE_NO_ARMOR,
    DAMAGE_NO_KNOCKBACK,
    DAMAGE_NO_PROTECTION,
    DAMAGE_NO_TEAM_PROTECTION,
    Vector3,
    Flight,
    Powerups,
    Weapons,
    PlayerInfo,
    PlayerState,
    PlayerStats,
    player_info,
    players_info,
    get_userinfo,
    send_server_command,
    client_command,
    console_command,
    get_cvar,
    set_cvar,
    set_cvar_limit,
    kick,
    console_print,
    get_configstring,
    set_configstring,
    force_vote,
    add_console_command,
    player_state,
    player_stats,
    set_position,
    set_velocity,
    noclip,
    set_health,
    set_armor,
    set_weapons,
    set_weapon,
    set_ammo,
    set_powerups,
    set_holdable,
    drop_holdable,
    set_flight,
    set_invulnerability,
    set_score,
    callvote,
    allow_single_player,
    player_spawn,
    set_privileges,
    destroy_kamikaze_timers,
    spawn_item,
    remove_dropped_items,
    slay_with_mod,
    replace_items,
    dev_print_items,
    force_weapon_respawn_time,
    register_handler,
    get_targetting_entities,
    set_cvar_once,
    set_cvar_limit_once,
)
from ._zmq import StatsListener

import database

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

UserInfo = TypedDict(
    "UserInfo",
    {
        "ip": str,
        "ui_singlePlayerActive": str,
        "cg_autoAction": str,
        "cg_autoHop": str,
        "cg_predictItems": str,
        "model": str,
        "headmodel": str,
        "cl_anonymous": str,
        "country": str,
        "color1": str,
        "rate": str,
        "color2": str,
        "sex": str,
        "teamtask": str,
        "name": str,
        "handicap": str,
        "password": NotRequired[str],
    },
)

UncancellableEventReturn = Literal[0] | None
CancellableEventReturn = Literal[0, 1, 2, 3] | None

PlayerSummaryData = TypedDict(
    "PlayerSummaryData",
    {
        "NAME": str,
        "STEAM_ID": str,
        "TEAM": int,
    },
)
GameStartData = TypedDict(
    "GameStartData",
    {
        "CAPTURE_LIMIT": int,
        "FACTORY": str,
        "FACTORY_TITLE": str,
        "FRAG_LIMIT": int,
        "GAME_TYPE": str,
        "INFECTED": int,
        "INSTAGIB": int,
        "MAP": str,
        "MATCH_GUID": str,
        "MERCY_LIMIT": int,
        "PLAYERS": list[PlayerSummaryData],
        "QUADHOG": int,
        "ROUND_LIMIT": int,
        "SCORE_LIMIT": int,
        "SERVER_TITLE": str,
        "TIME_LIMIT": int,
        "TRAINING": int,
    },
)
GameEndData = TypedDict(
    "GameEndData",
    {
        "ABORTED": bool,
        "CAPTURE_LIMIT": int,
        "EXIT_MSG": str,
        "FACTORY": str,
        "FACTORY_TITLE": str,
        "FIRST_SCORER": str,
        "FRAG_LIMIT": int,
        "GAME_LENGTH": int,
        "GAME_TYPE": str,
        "INFECTED": int,
        "INSTAGIB": int,
        "LAST_LEAD_CHANGE_TIME": int,
        "LAST_SCORER": str,
        "LAST_TEAMSCORER": str,
        "MAP": str,
        "MATCH_GUID": str,
        "MERCY_LIMIT": int,
        "QUADHOG": int,
        "RESTARTED": int,
        "ROUND_LIMIT": int,
        "SCORE_LIMIT": int,
        "SERVER_TITLE": str,
        "TIME_LIMIT": int,
        "TRAINING": int,
        "TSCORE0": int,
        "TSCORE1": int,
    },
)
RoundEndData = TypedDict(
    "RoundEndData",
    {
        "MATCH_GUID": str,
        "ROUND": int,
        "TEAM_WON": Literal["RED", "BLUE", "DRAW"],
        "TIME": int,
        "WARMUP": bool,
    },
)
Vector = TypedDict("Vector", {"x": float, "y": float, "z": float})
PowerUps = Literal[
    "QUAD", "BATTLESUIT", "HASTE", "INVISIBILITY", "REGENERATION", "INVULNERABILITY"
]
Holdable = Literal[
    "TELEPORTER", "MEDKIT", "FLIGHT", "KAMIKAZE", "PORTAL", "INVULNERABILITY"
]
Weapon = Literal[
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
PlayerData = TypedDict(
    "PlayerData",
    {
        "AIRBORNE": bool,
        "AMMO": int,
        "ARMOR": int,
        "BOT": bool,
        "BOT_SKILL": int | None,
        "HEALTH": int,
        "HOLDABLE": Holdable | None,
        "NAME": str,
        "POSITION": Vector,
        "POWERUPS": list[PowerUps] | None,
        "SPEED": float,
        "STEAM_ID": str,
        "SUBMERGED": bool,
        "TEAM": int,
        "VIEW": Vector,
        "WEAPON": Weapon,
    },
)
MeansOfDeath = Literal[
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
    "SWITCH_TEAMS",
    "THAW",
    "LIGHTNING_DISCHARGE",
    "HMG",
    "RAILGUN_HEADSHOT",
]
KillData = TypedDict(
    "KillData",
    {
        "KILLER": PlayerData,
        "VICTIM": PlayerData,
        "MATCH_GUID": str,
        "MOD": MeansOfDeath,
        "OTHER_TEAM_ALIVE": int,
        "OTHER_TEAM_DEAD": int,
        "ROUND": int,
        "SUICIDE": bool,
        "TEAMKILL": bool,
        "TEAM_ALIVE": int,
        "TEAM_DEAD": int,
        "TIME": int,
        "WARMUP": bool,
    },
)
DeathData = TypedDict(
    "DeathData",
    {
        "KILLER": PlayerData | None,
        "VICTIM": PlayerData,
        "MATCH_GUID": str,
        "MOD": MeansOfDeath,
        "OTHER_TEAM_ALIVE": int,
        "OTHER_TEAM_DEAD": int,
        "ROUND": int,
        "SUICIDE": bool,
        "TEAMKILL": bool,
        "TEAM_ALIVE": int,
        "TEAM_DEAD": int,
        "TIME": int,
        "WARMUP": bool,
    },
)
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
PlayerKillStats = TypedDict(
    "PlayerKillStats", {"DATA": KillData, "TYPE": Literal["PLAYER_KILL"]}
)
PlayerDeathStats = TypedDict(
    "PlayerDeathStats", {"DATA": DeathData, "TYPE": Literal["PLAYER_DEATH"]}
)
MedalData = TypedDict(
    "MedalData",
    {
        "MATCH_GUID": str,
        "MEDAL": Literal[
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
        ],
        "NAME": str,
        "STEAM_ID": str,
        "TIME": int,
        "TOTAL": int,
        "WARMUP": bool,
    },
)
PlayerMedalStats = TypedDict(
    "PlayerMedalStats", {"DATA": MedalData, "TYPE": Literal["PLAYER_MEDAL"]}
)
RoundOverStats = TypedDict(
    "RoundOverStats", {"DATA": RoundEndData, "TYPE": Literal["ROUND_OVER"]}
)
PlayerGameData = TypedDict(
    "PlayerGameData",
    {"MATCH_GUID": str, "NAME": str, "STEAM_ID": str, "TIME": int, "WARMUP": bool},
)
PlayerConnectStats = TypedDict(
    "PlayerConnectStats", {"DATA": PlayerGameData, "TYPE": Literal["PLAYER_CONNECT"]}
)
PlayerDisconnectStats = TypedDict(
    "PlayerDisconnectStats",
    {"DATA": PlayerGameData, "TYPE": Literal["PLAYER_DICCONNECT"]},
)
TeamSwitchEvent = TypedDict(
    "TeamSwitchEvent", {"NAME": str, "OLD_TEAM": str, "STEAM_ID": str, "TEAM": str}
)
TeamSwitchGameData = TypedDict(
    "TeamSwitchGameData",
    {"KILLER": TeamSwitchEvent, "MATCH_GUID": str, "TIME": int, "WARMUP": bool},
)
PlayerSwitchTeamStats = TypedDict(
    "PlayerSwitchTeamStats",
    {"DATA": TeamSwitchGameData, "TYPE": Literal["PLAYER_SWITCHTEAM"]},
)
MatchStartedStats = TypedDict(
    "MatchStartedStats", {"DATA": GameStartData, "TYPE": Literal["MATCH_STARTED"]}
)
MatchReportStats = TypedDict(
    "MatchReportStats", {"DATA": GameEndData, "TYPE": Literal["MATCH_REPORT"]}
)
DamageEntry = TypedDict("DamageEntry", {"DEALT": int, "TAKEN": int})
MedalsEntry = TypedDict(
    "MedalsEntry",
    {
        "ACCURACY": int,
        "ASSISTS": int,
        "CAPTURES": int,
        "COMBOKILL": int,
        "DEFENDS": int,
        "EXCELLENT": int,
        "FIRSTFRAG": int,
        "HEADSHOT": int,
        "HUMILIATION": int,
        "IMPRESSIVE": int,
        "MIDAIR": int,
        "PERFECT": int,
        "PERFORATED": int,
        "QUADGOD": int,
        "RAMPAGE": int,
        "REVENGE": int,
    },
)
PickupsEntry = TypedDict(
    "PickupsEntry",
    {
        "AMMO": int,
        "ARMOR": int,
        "ARMOR_REGEN": int,
        "BATTLESUIT": int,
        "DOUBLER": int,
        "FLIGHT": int,
        "GREEN_ARMOR": int,
        "GUARD": int,
        "HASTE": int,
        "HEALTH": int,
        "INVIS": int,
        "INVULNERABILITY": int,
        "KAMIKAZE": int,
        "MEDKIT": int,
        "MEGA_HEALTH": int,
        "OTHER_HOLDABLE": int,
        "OTHER_POWERUP": int,
        "PORTAL": int,
        "QUAD": int,
        "RED_ARMOR": int,
        "REGEN": int,
        "SCOUT": int,
        "TELEPORTER": int,
        "YELLOW_ARMOR": int,
    },
)
SingleWeaponStatsEntry = TypedDict(
    "SingleWeaponStatsEntry",
    {"D": int, "DG": int, "DR": int, "H": int, "K": int, "P": int, "S": int, "T": int},
)
WeaponsStatsEntry = TypedDict(
    "WeaponsStatsEntry",
    {
        "BFG": SingleWeaponStatsEntry,
        "CHAINGUN": SingleWeaponStatsEntry,
        "GAUNTLET": SingleWeaponStatsEntry,
        "GRENADE": SingleWeaponStatsEntry,
        "HMG": SingleWeaponStatsEntry,
        "LIGHTNING": SingleWeaponStatsEntry,
        "MACHINEGUN": SingleWeaponStatsEntry,
        "NAILGUN": SingleWeaponStatsEntry,
        "OTHER_WEAPON": SingleWeaponStatsEntry,
        "PLASMA": SingleWeaponStatsEntry,
        "PROXMINE": SingleWeaponStatsEntry,
        "RAILGUN": SingleWeaponStatsEntry,
        "ROCKET": SingleWeaponStatsEntry,
        "SHOTGUN": SingleWeaponStatsEntry,
    },
)
PlayerStatsEntry = TypedDict(
    "PlayerStatsEntry",
    {
        "ABORTED": bool,
        "BLUE_FLAG_PICKUPS": int,
        "DAMAGE": DamageEntry,
        "DEATHS": int,
        "HOLY_SHITS": int,
        "KILLS": int,
        "LOSE": int,
        "MATCH_GUID": str,
        "MAX_STREAK": int,
        "MEDALS": MedalsEntry,
        "MODEL": str,
        "NAME": str,
        "NEUTRAL_FLAG_PICKUPS": int,
        "PICKUPS": PickupsEntry,
        "PLAY_TIME": int,
        "QUIT": int,
        "RANK": int,
        "RED_FLAG_PICKUPS": int,
        "SCORE": int,
        "STEAM_ID": str,
        "TEAM": int,
        "TEAM_JOIN_TIME": int,
        "TEAM_RANK": int,
        "TIED_RANK": int,
        "TIED_TEAM_RANK": int,
        "WARMUP": bool,
        "WEAPONS": WeaponsStatsEntry,
        "WIN": int,
    },
)
PlayerStatsStats = TypedDict(
    "PlayerStatsStats", {"DATA": PlayerStatsEntry, "TYPE": Literal["PLAYER_STATS"]}
)
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
    "__version__",
    "__version_info__",
    "__plugins_version__",
    "_map_title",
    "_map_subtitle1",
    "_map_subtitle2",
    "UncancellableEventReturn",
    "CancellableEventReturn",
    "GameStartData",
    "GameEndData",
    "RoundEndData",
    "DeathData",
    "KillData",
    "UserinfoEventInput",
    "Weapon",
    "PowerUps",
    "MeansOfDeath",
    "PlayerSummaryData",
    "Vector",
    "Holdable",
    "PlayerData",
    "PlayerKillStats",
    "PlayerDeathStats",
    "PlayerMedalStats",
    "MedalData",
    "RoundOverStats",
    "PlayerConnectStats",
    "PlayerDisconnectStats",
    "PlayerGameData",
    "PlayerSwitchTeamStats",
    "TeamSwitchGameData",
    "MatchStartedStats",
    "MatchReportStats",
    "PlayerStatsStats",
    "PlayerStatsEntry",
    "WeaponsStatsEntry",
    "PickupsEntry",
    "MedalsEntry",
    "DamageEntry",
    "SingleWeaponStatsEntry",
    "StatsData",
    "UserInfo",
    # from _commands.pyi
    "MAX_MSG_LENGTH",
    "re_color_tag",
    "AbstractChannel",
    "ConsoleChannel",
    "ChatChannel",
    "TellChannel",
    "ClientCommandChannel",
    "TeamChatChannel",
    "CHAT_CHANNEL",
    "RED_TEAM_CHAT_CHANNEL",
    "BLUE_TEAM_CHAT_CHANNEL",
    "FREE_CHAT_CHANNEL",
    "SPECTATOR_CHAT_CHANNEL",
    "CONSOLE_CHANNEL",
    "Command",
    "CommandInvoker",
    "COMMANDS",
    # from _core.pyi
    "_thread_name",
    "_thread_count",
    "DEFAULT_PLUGINS",
    "set_cvar_once",
    "set_cvar_limit_once",
    "set_plugins_version",
    "set_map_subtitles",
    "parse_variables",
    "get_logger",
    "_configure_logger",
    "log_exception",
    "handle_exception",
    "threading_excepthook",
    "next_frame",
    "delay",
    "thread",
    "uptime",
    "owner",
    "_stats",
    "stats_listener",
    "_modules",
    "load_preset_plugins",
    "load_plugin",
    "unload_plugin",
    "reload_plugin",
    "initialize_cvars",
    "initialize",
    "late_init",
    "PluginLoadError",
    "PluginUnloadError",
    # from _events.pyi
    "_re_vote",
    "EventDispatcher",
    "ConsolePrintDispatcher",
    "CommandDispatcher",
    "ClientCommandDispatcher",
    "ServerCommandDispatcher",
    "FrameEventDispatcher",
    "SetConfigstringDispatcher",
    "ChatEventDispatcher",
    "UnloadDispatcher",
    "PlayerConnectDispatcher",
    "PlayerLoadedDispatcher",
    "PlayerDisconnectDispatcher",
    "PlayerSpawnDispatcher",
    "StatsDispatcher",
    "VoteCalledDispatcher",
    "VoteStartedDispatcher",
    "VoteEndedDispatcher",
    "VoteDispatcher",
    "GameCountdownDispatcher",
    "GameStartDispatcher",
    "GameEndDispatcher",
    "RoundCountdownDispatcher",
    "RoundStartDispatcher",
    "RoundEndDispatcher",
    "TeamSwitchDispatcher",
    "TeamSwitchAttemptDispatcher",
    "MapDispatcher",
    "NewGameDispatcher",
    "KillDispatcher",
    "DeathDispatcher",
    "UserinfoDispatcher",
    "KamikazeUseDispatcher",
    "KamikazeExplodeDispatcher",
    "DamageDispatcher",
    "EventDispatcherManager",
    "EVENT_DISPATCHERS",
    # from _game.pyi
    "Game",
    "NonexistentGameError",
    # from _handlers.pyi
    "frame_tasks",
    "next_frame_tasks",
    "handle_rcon",
    "handle_client_command",
    "handle_server_command",
    "handle_frame",
    "handle_new_game",
    "handle_set_configstring",
    "handle_player_connect",
    "handle_player_loaded",
    "handle_player_spawn",
    "handle_player_disconnect",
    "handle_kamikaze_use",
    "handle_kamikaze_explode",
    "handle_damage",
    "handle_console_print",
    "redirect_print",
    "register_handlers",
    # from _player.pyi
    "Player",
    "NonexistentPlayerError",
    "AbstractDummyPlayer",
    "RconDummyPlayer",
    # from _plugin.pyi
    "Plugin",
    # _shinqlx
    "DEBUG",
    "RET_NONE",
    "RET_STOP",
    "RET_STOP_EVENT",
    "RET_STOP_ALL",
    "RET_USAGE",
    "PRI_HIGHEST",
    "PRI_HIGH",
    "PRI_NORMAL",
    "PRI_LOW",
    "PRI_LOWEST",
    "CVAR_ARCHIVE",
    "CVAR_USERINFO",
    "CVAR_SERVERINFO",
    "CVAR_SYSTEMINFO",
    "CVAR_INIT",
    "CVAR_LATCH",
    "CVAR_ROM",
    "CVAR_USER_CREATED",
    "CVAR_TEMP",
    "CVAR_CHEAT",
    "CVAR_NORESTART",
    "GAMETYPES",
    "GAMETYPES_SHORT",
    "PRIV_NONE",
    "PRIV_MOD",
    "PRIV_ADMIN",
    "PRIV_ROOT",
    "PRIV_BANNED",
    "CS_FREE",
    "CS_ZOMBIE",
    "CS_CONNECTED",
    "CS_PRIMED",
    "CS_ACTIVE",
    "CONNECTION_STATES",
    "TEAM_FREE",
    "TEAM_RED",
    "TEAM_BLUE",
    "TEAM_SPECTATOR",
    "TEAMS",
    "MOD_UNKNOWN",
    "MOD_SHOTGUN",
    "MOD_GAUNTLET",
    "MOD_MACHINEGUN",
    "MOD_GRENADE",
    "MOD_GRENADE_SPLASH",
    "MOD_ROCKET",
    "MOD_ROCKET_SPLASH",
    "MOD_PLASMA",
    "MOD_PLASMA_SPLASH",
    "MOD_RAILGUN",
    "MOD_LIGHTNING",
    "MOD_BFG",
    "MOD_BFG_SPLASH",
    "MOD_WATER",
    "MOD_SLIME",
    "MOD_LAVA",
    "MOD_CRUSH",
    "MOD_TELEFRAG",
    "MOD_FALLING",
    "MOD_SUICIDE",
    "MOD_TARGET_LASER",
    "MOD_TRIGGER_HURT",
    "MOD_NAIL",
    "MOD_CHAINGUN",
    "MOD_PROXIMITY_MINE",
    "MOD_KAMIKAZE",
    "MOD_JUICED",
    "MOD_GRAPPLE",
    "MOD_SWITCH_TEAMS",
    "MOD_THAW",
    "MOD_LIGHTNING_DISCHARGE",
    "MOD_HMG",
    "MOD_RAILGUN_HEADSHOT",
    "WEAPONS",
    "DAMAGE_RADIUS",
    "DAMAGE_NO_ARMOR",
    "DAMAGE_NO_KNOCKBACK",
    "DAMAGE_NO_PROTECTION",
    "DAMAGE_NO_TEAM_PROTECTION",
    "Vector3",
    "Flight",
    "Powerups",
    "Weapons",
    "PlayerInfo",
    "PlayerState",
    "PlayerStats",
    "player_info",
    "players_info",
    "get_userinfo",
    "send_server_command",
    "client_command",
    "console_command",
    "get_cvar",
    "set_cvar",
    "set_cvar_limit",
    "kick",
    "console_print",
    "get_configstring",
    "set_configstring",
    "force_vote",
    "add_console_command",
    "player_state",
    "player_stats",
    "set_position",
    "set_velocity",
    "noclip",
    "set_health",
    "set_armor",
    "set_weapons",
    "set_weapon",
    "set_ammo",
    "set_powerups",
    "set_holdable",
    "drop_holdable",
    "set_flight",
    "set_invulnerability",
    "set_score",
    "callvote",
    "allow_single_player",
    "player_spawn",
    "set_privileges",
    "destroy_kamikaze_timers",
    "spawn_item",
    "remove_dropped_items",
    "slay_with_mod",
    "replace_items",
    "dev_print_items",
    "force_weapon_respawn_time",
    "register_handler",
    "get_targetting_entities",
    # from _zmq.pyi
    "StatsListener",
    # from database.pyi
    "database",
]
