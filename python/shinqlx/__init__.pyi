from ._shinqlx import (
    DEBUG,
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
    GAMETYPES,
    GAMETYPES_SHORT,
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
    CONNECTION_STATES,
    TEAM_FREE,
    TEAM_RED,
    TEAM_BLUE,
    TEAM_SPECTATOR,
    TEAMS,
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
    WEAPONS,
    DAMAGE_RADIUS,
    DAMAGE_NO_ARMOR,
    DAMAGE_NO_KNOCKBACK,
    DAMAGE_NO_PROTECTION,
    DAMAGE_NO_TEAM_PROTECTION,
    DEFAULT_PLUGINS,
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
    set_map_subtitles,
    next_frame,
    delay,
    thread,
    uptime,
    owner,
    initialize_cvars,
    Game,
    NonexistentGameError,
    PluginLoadError,
    PluginUnloadError,
    _map_title,
    _map_subtitle1,
    _map_subtitle2,
    _thread_name,
    _thread_count,
)
from ._core import (
    parse_variables,
    get_logger,
    log_exception,
    handle_exception,
    threading_excepthook,
    stats_listener,
    set_plugins_version,
    load_preset_plugins,
    load_plugin,
    unload_plugin,
    reload_plugin,
    initialize,
    late_init,
)
from ._player import (
    Player,
    NonexistentPlayerError,
    AbstractDummyPlayer,
    RconDummyPlayer,
    UserInfo,
)
from ._plugin import (
    Plugin,
    GameStartData,
    GameEndData,
    RoundEndData,
    DeathData,
    KillData,
    UserInfoEventInput,
    Weapon,
    PowerUps,
    MeansOfDeath,
    PlayerSummaryData,
    Vector,
    Holdable,
    PlayerData,
    PlayerKillStats,
    PlayerDeathStats,
    PlayerMedalStats,
    MedalData,
    RoundOverStats,
    PlayerConnectStats,
    PlayerDisconnectStats,
    PlayerGameData,
    PlayerSwitchTeamStats,
    TeamSwitchGameData,
    MatchStartedStats,
    MatchReportStats,
    PlayerStatsStats,
    PlayerStatsEntry,
    WeaponsStatsEntry,
    PickupsEntry,
    MedalsEntry,
    DamageEntry,
    SingleWeaponStatsEntry,
    StatsData,
)
from ._events import (
    EventDispatcher,
    EventDispatcherManager,
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
    PlayerDisonnectDispatcher,
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
    PlayerItemsTossDispatcher,
    DamageDispatcher,
    EVENT_DISPATCHERS,
    UncancellableEventReturn,
    CancellableEventReturn,
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
    handle_player_disconnect,
    handle_player_spawn,
    handle_kamikaze_use,
    handle_kamikaze_explode,
    handle_console_print,
    handle_damage,
    redirect_print,
    register_handlers,
)
from ._commands import (
    MAX_MSG_LENGTH,
    re_color_tag,
    AbstractChannel,
    ChatChannel,
    TeamChatChannel,
    TellChannel,
    ConsoleChannel,
    ClientCommandChannel,
    Command,
    CommandInvoker,
    COMMANDS,
    CHAT_CHANNEL,
    RED_TEAM_CHAT_CHANNEL,
    BLUE_TEAM_CHAT_CHANNEL,
    FREE_CHAT_CHANNEL,
    SPECTATOR_CHAT_CHANNEL,
    CONSOLE_CHANNEL,
)
from ._zmq import StatsListener

__version__: str
__plugins_version__: str

__version_info__: tuple[int, int, int]

__all__ = [
    "__version__",
    "__version_info__",
    "__plugins_version__",
    "_map_title",
    "_map_subtitle1",
    "_map_subtitle2",
    "_thread_name",
    "_thread_count",
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
    "DEFAULT_PLUGINS",
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
    "set_cvar_once",
    "set_cvar_limit_once",
    "next_frame",
    "delay",
    "thread",
    "uptime",
    "owner",
    "initialize_cvars",
    "Game",
    "NonexistentGameError",
    "PluginLoadError",
    "PluginUnloadError",
    # _core
    "parse_variables",
    "get_logger",
    "log_exception",
    "handle_exception",
    "threading_excepthook",
    "stats_listener",
    "set_plugins_version",
    "set_map_subtitles",
    "load_preset_plugins",
    "load_plugin",
    "unload_plugin",
    "reload_plugin",
    "initialize",
    "late_init",
    # _plugin
    "Plugin",
    "GameStartData",
    "GameEndData",
    "RoundEndData",
    "DeathData",
    "KillData",
    "UserInfoEventInput",
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
    # _player
    "Player",
    "NonexistentPlayerError",
    "AbstractDummyPlayer",
    "RconDummyPlayer",
    "UserInfo",
    # _commands
    "MAX_MSG_LENGTH",
    "re_color_tag",
    "AbstractChannel",
    "ChatChannel",
    "TeamChatChannel",
    "TellChannel",
    "ConsoleChannel",
    "ClientCommandChannel",
    "Command",
    "CommandInvoker",
    "COMMANDS",
    "CHAT_CHANNEL",
    "RED_TEAM_CHAT_CHANNEL",
    "BLUE_TEAM_CHAT_CHANNEL",
    "FREE_CHAT_CHANNEL",
    "SPECTATOR_CHAT_CHANNEL",
    "CONSOLE_CHANNEL",
    # _events
    "EventDispatcher",
    "EventDispatcherManager",
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
    "PlayerDisonnectDispatcher",
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
    "PlayerItemsTossDispatcher",
    "DamageDispatcher",
    "EVENT_DISPATCHERS",
    "UncancellableEventReturn",
    "CancellableEventReturn",
    # _handlers
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
    "handle_player_disconnect",
    "handle_player_spawn",
    "handle_kamikaze_use",
    "handle_kamikaze_explode",
    "handle_console_print",
    "handle_damage",
    "redirect_print",
    "register_handlers",
    # _zmq
    "StatsListener",
]
