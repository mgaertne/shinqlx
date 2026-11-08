import builtins
from collections.abc import Iterable
from enum import Enum
from typing import (
    Callable,
    ClassVar,
    Literal,
    SupportsIndex,
    final,
    overload,
)

from typing_extensions import Self

# Variables with simple values
RET_NONE: int
RET_STOP: int
RET_STOP_EVENT: int
RET_STOP_ALL: int
RET_USAGE: int

PRI_HIGHEST: int
PRI_HIGH: int
PRI_NORMAL: int
PRI_LOW: int
PRI_LOWEST: int

# Cvar flags
CVAR_ARCHIVE: int
CVAR_USERINFO: int
CVAR_SERVERINFO: int
CVAR_SYSTEMINFO: int
CVAR_INIT: int
CVAR_LATCH: int
CVAR_ROM: int
CVAR_USER_CREATED: int
CVAR_TEMP: int
CVAR_CHEAT: int
CVAR_NORESTART: int

# Privileges
PRIV_NONE: int
PRIV_MOD: int
PRIV_ADMIN: int
PRIV_ROOT: int
PRIV_BANNED: int

# Connection states
CS_FREE: int
CS_ZOMBIE: int
CS_CONNECTED: int
CS_PRIMED: int
CS_ACTIVE: int

# Teams
TEAM_FREE: int
TEAM_RED: int
TEAM_BLUE: int
TEAM_SPECTATOR: int

# Means of death
MOD_UNKNOWN: int
MOD_SHOTGUN: int
MOD_GAUNTLET: int
MOD_MACHINEGUN: int
MOD_GRENADE: int
MOD_GRENADE_SPLASH: int
MOD_ROCKET: int
MOD_ROCKET_SPLASH: int
MOD_PLASMA: int
MOD_PLASMA_SPLASH: int
MOD_RAILGUN: int
MOD_LIGHTNING: int
MOD_BFG: int
MOD_BFG_SPLASH: int
MOD_WATER: int
MOD_SLIME: int
MOD_LAVA: int
MOD_CRUSH: int
MOD_TELEFRAG: int
MOD_FALLING: int
MOD_SUICIDE: int
MOD_TARGET_LASER: int
MOD_TRIGGER_HURT: int
MOD_NAIL: int
MOD_CHAINGUN: int
MOD_PROXIMITY_MINE: int
MOD_KAMIKAZE: int
MOD_JUICED: int
MOD_GRAPPLE: int
MOD_SWITCH_TEAMS: int
MOD_THAW: int
MOD_LIGHTNING_DISCHARGE: int
MOD_HMG: int
MOD_RAILGUN_HEADSHOT: int

# damage flags
DAMAGE_RADIUS: int
DAMAGE_NO_ARMOR: int
DAMAGE_NO_KNOCKBACK: int
DAMAGE_NO_PROTECTION: int
DAMAGE_NO_TEAM_PROTECTION: int

@final
class Vector3(tuple):
    def __new__(cls, values: tuple[int, int, int]) -> Self: ...
    x: int
    y: int
    z: int

@final
class ConfigString:
    SERVERINFO: ClassVar[ConfigString]
    SYSTEMINFO: ClassVar[ConfigString]
    MUSIC: ClassVar[ConfigString]
    MESSAGE: ClassVar[ConfigString]
    MOTD: ClassVar[ConfigString]
    WARMUP: ClassVar[ConfigString]
    SCORES1: ClassVar[ConfigString]
    SCORES2: ClassVar[ConfigString]
    VOTE_TIME: ClassVar[ConfigString]
    VOTE_STRING: ClassVar[ConfigString]
    VOTE_YES: ClassVar[ConfigString]
    VOTE_NO: ClassVar[ConfigString]
    GAME_VERSION: ClassVar[ConfigString]
    LEVEL_START_TIME: ClassVar[ConfigString]
    INTERMISSION: ClassVar[ConfigString]
    ITEMS: ClassVar[ConfigString]
    MODELS: ClassVar[ConfigString]
    SOUNDS: ClassVar[ConfigString]
    PLAYERS: ClassVar[ConfigString]
    LOCATIONS: ClassVar[ConfigString]
    PARTICLES: ClassVar[ConfigString]
    FLAGSTATUS: ClassVar[ConfigString]
    FIRSTPLACE: ClassVar[ConfigString]
    SECONDPLACE: ClassVar[ConfigString]
    ROUND_STATUS: ClassVar[ConfigString]
    ROUND_TIME: ClassVar[ConfigString]
    RED_PLAYERS_LEFT: ClassVar[ConfigString]
    BLUE_PLAYERS_LEFT: ClassVar[ConfigString]
    SHADERSTATE: ClassVar[ConfigString]
    NEXTMAP: ClassVar[ConfigString]
    PRACTICE: ClassVar[ConfigString]
    FREECAM: ClassVar[ConfigString]
    PAUSE_START_TIME: ClassVar[ConfigString]
    PAUSE_END_TIME: ClassVar[ConfigString]
    TIMEOUTS_RED: ClassVar[ConfigString]
    TIMEOUTS_BLUE: ClassVar[ConfigString]
    MODEL_OVERRIDE: ClassVar[ConfigString]
    PLAYER_CYLINDERS: ClassVar[ConfigString]
    DEBUGFLAGS: ClassVar[ConfigString]
    ENABLEBREATH: ClassVar[ConfigString]
    DMGTHROUGHDEPTH: ClassVar[ConfigString]
    AUTHOR: ClassVar[ConfigString]
    AUTHOR2: ClassVar[ConfigString]
    ADVERT_DELAY: ClassVar[ConfigString]
    PMOVEINFO: ClassVar[ConfigString]
    ARMORINFO: ClassVar[ConfigString]
    WEAPONINFO: ClassVar[ConfigString]
    PLAYERINFO: ClassVar[ConfigString]
    SCORE1STPLAYER: ClassVar[ConfigString]
    SCORE2NDPLAYER: ClassVar[ConfigString]
    CLIENTNUM1STPLAYER: ClassVar[ConfigString]
    CLIENTNUM2NDPLAYER: ClassVar[ConfigString]
    NAME1STPLAYER: ClassVar[ConfigString]
    NAME2NDPLAYER: ClassVar[ConfigString]
    ATMOSEFFECT: ClassVar[ConfigString]
    MOST_DAMAGEDEALT_PLYR: ClassVar[ConfigString]
    MOST_ACCURATE_PLYR: ClassVar[ConfigString]
    REDTEAMBASE: ClassVar[ConfigString]
    BLUETEAMBASE: ClassVar[ConfigString]
    BEST_ITEMCONTROL_PLYR: ClassVar[ConfigString]
    MOST_VALUABLE_OFFENSIVE_PLYR: ClassVar[ConfigString]
    MOST_VALUABLE_DEFENSIVE_PLYR: ClassVar[ConfigString]
    MOST_VALUABLE_PLYR: ClassVar[ConfigString]
    GENERIC_COUNT_RED: ClassVar[ConfigString]
    GENERIC_COUNT_BLUE: ClassVar[ConfigString]
    AD_SCORES: ClassVar[ConfigString]
    ROUND_WINNER: ClassVar[ConfigString]
    CUSTOM_SETTINGS: ClassVar[ConfigString]
    ROTATIONMAPS: ClassVar[ConfigString]
    ROTATIONVOTES: ClassVar[ConfigString]
    DISABLE_VOTE_UI: ClassVar[ConfigString]
    ALLREADY_TIME: ClassVar[ConfigString]
    INFECTED_SURVIVOR_MINSPEED: ClassVar[ConfigString]
    RACE_POINTS: ClassVar[ConfigString]
    DISABLE_LOADOUT: ClassVar[ConfigString]
    MATCH_GUID: ClassVar[ConfigString]
    STARTING_WEAPONS: ClassVar[ConfigString]
    STEAM_ID: ClassVar[ConfigString]
    STEAM_WORKSHOP_IDS: ClassVar[ConfigString]
    def __index__(self) -> int: ...

@final
class Teams:
    FREE: ClassVar[Teams]
    RED: ClassVar[Teams]
    BLUE: ClassVar[Teams]
    SPECTATOR: ClassVar[Teams]
    def __index__(self) -> int: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...

@final
class Privileges:
    NONE: ClassVar[Privileges]
    MOD: ClassVar[Privileges]
    ADMIN: ClassVar[Privileges]
    ROOT: ClassVar[Privileges]
    BANNED: ClassVar[Privileges]
    def __index__(self) -> int: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...

@final
class ConnectionStates:
    FREE: ClassVar[ConnectionStates]
    ZOMBIE: ClassVar[ConnectionStates]
    CONNECTED: ClassVar[ConnectionStates]
    PRIMED: ClassVar[ConnectionStates]
    ACTIVE: ClassVar[ConnectionStates]
    def __index__(self) -> int: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...

@final
class Priorities:
    HIGHEST: ClassVar[Priorities]
    HIGH: ClassVar[Priorities]
    NORMAL: ClassVar[Priorities]
    LOW: ClassVar[Priorities]
    LOWEST: ClassVar[Priorities]
    def __index__(self) -> int: ...

@final
class ReturnCodes:
    NONE: ClassVar[ReturnCodes]
    STOP: ClassVar[ReturnCodes]
    STOP_EVENT: ClassVar[ReturnCodes]
    STOP_ALL: ClassVar[ReturnCodes]
    USAGE: ClassVar[ReturnCodes]
    def __index__(self) -> int: ...
    def __bool__(self) -> bool: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...

@final
class VoteStates:
    NONE: ClassVar[VoteStates]
    PENDING: ClassVar[VoteStates]
    YES: ClassVar[VoteStates]
    NO: ClassVar[VoteStates]
    FORCE_PASS: ClassVar[VoteStates]
    FORCE_FAIL: ClassVar[VoteStates]
    EXPIRED: ClassVar[VoteStates]
    def __index__(self) -> int: ...

@final
class Weapon:
    NONE: ClassVar[Weapon]
    GAUNTLET: ClassVar[Weapon]
    MACHINEGUN: ClassVar[Weapon]
    SHOTGUN: ClassVar[Weapon]
    GRENADE_LAUNCHER: ClassVar[Weapon]
    ROCKET_LAUNCHER: ClassVar[Weapon]
    LIGHTNING: ClassVar[Weapon]
    RAILGUN: ClassVar[Weapon]
    PLASMAGUN: ClassVar[Weapon]
    BFG: ClassVar[Weapon]
    GRAPPLING_HOOK: ClassVar[Weapon]
    NAILGUN: ClassVar[Weapon]
    PROX_LAUNCHER: ClassVar[Weapon]
    CHAINGUN: ClassVar[Weapon]
    HMG: ClassVar[Weapon]
    HANDS: ClassVar[Weapon]
    def __index__(self) -> int: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...

@final
class Permissions:
    PLAYER: ClassVar[Permissions]
    MODERATOR: ClassVar[Permissions]
    GAME_ADMIN: ClassVar[Permissions]
    COMMUNITY_ADMIN: ClassVar[Permissions]
    SERVER_ADMIN: ClassVar[Permissions]
    GOD: ClassVar[Permissions]
    def __index__(self) -> int: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...
    def __lt__(self, other: object, /) -> bool: ...
    def __le__(self, other: object, /) -> bool: ...
    def __gt__(self, other: object, /) -> bool: ...
    def __ge__(self, other: object, /) -> bool: ...

@final
class Holdable:
    NONE: ClassVar[Holdable]
    TELEPORTER: ClassVar[Holdable]
    MEDKIT: ClassVar[Holdable]
    FLIGHT: ClassVar[Holdable]
    KAMIKAZE: ClassVar[Holdable]
    PORTAL: ClassVar[Holdable]
    INVULNERABILITY: ClassVar[Holdable]
    UNKNOWN: ClassVar[Holdable]
    def __index__(self) -> int: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...

class Events(str, Enum):
    CHAT = "chat"
    CLIENT_COMMAND = "client_command"
    COMMAND = "command"
    CONSOLE_PRINT = "console_print"
    DAMAGE = "damage"
    DEATH = "death"
    FRAME = "frame"
    GAME_COUNTDOWN = "game_countdown"
    GAME_END = "game_end"
    GAME_START = "game_start"
    KAMIKAZE_EXPLODE = "kamikaze_explode"
    KAMIKAZE_USE = "kamikaze_use"
    KILL = "kill"
    MAP = "map"
    NEW_GAME = "new_game"
    PLAYER_CONNECT = "player_connect"
    PLAYER_DISCONNECT = "player_disconnect"
    PLAYER_LOADED = "player_loaded"
    PLAYER_SPAWN = "player_spawn"
    ROUND_COUNTDOWN = "round_countdown"
    ROUND_END = "round_end"
    ROUND_START = "round_start"
    SERVER_COMMAND = "server_command"
    SET_CONFIGSTRING = "set_configstring"
    STATS = "stats"
    TEAM_SWITCH = "team_switch"
    TEAM_SWITCH_ATTEMPT = "team_switch_attempt"
    UNLOAD = "unload"
    USERINFO = "userinfo"
    VOTE = "vote"
    VOTE_CALLED = "vote_called"
    VOTE_ENDED = "vote_ended"
    VOTE_STARTED = "vote_started"

@final
class GameStates:
    WARMUP: ClassVar[GameStates]
    COUNTDOWN: ClassVar[GameStates]
    IN_PROGRESS: ClassVar[GameStates]
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...

@final
class CvarFlags:
    ARCHIVE: ClassVar[CvarFlags]
    USERINFO: ClassVar[CvarFlags]
    SERVERINFO: ClassVar[CvarFlags]
    SYSTEMINFO: ClassVar[CvarFlags]
    INIT: ClassVar[CvarFlags]
    LATCH: ClassVar[CvarFlags]
    ROM: ClassVar[CvarFlags]
    USER_CREATED: ClassVar[CvarFlags]
    TEMP: ClassVar[CvarFlags]
    CHEAT: ClassVar[CvarFlags]
    NORESTART: ClassVar[CvarFlags]
    def __index__(self) -> int: ...
    def __or__(self, other: SupportsIndex, /) -> int: ...
    def __ror__(self, other: SupportsIndex, /) -> int: ...
    def __and__(self, other: SupportsIndex, /) -> int: ...
    def __rand__(self, other: SupportsIndex, /) -> int: ...

@final
class DamageFlags:
    RADIUS: ClassVar[DamageFlags]
    NO_ARMOR: ClassVar[DamageFlags]
    NO_KNOCKBACK: ClassVar[DamageFlags]
    NO_PROTECTION: ClassVar[DamageFlags]
    NO_TEAM_PROTECTION: ClassVar[DamageFlags]
    def __index__(self) -> int: ...
    def __or__(self, other: SupportsIndex, /) -> int: ...
    def __ror__(self, other: SupportsIndex, /) -> int: ...
    def __and__(self, other: SupportsIndex, /) -> int: ...
    def __rand__(self, other: SupportsIndex, /) -> int: ...

@final
class MeansOfDeath:
    UNKNOWN: ClassVar[MeansOfDeath]
    SHOTGUN: ClassVar[MeansOfDeath]
    GAUNTLET: ClassVar[MeansOfDeath]
    MACHINEGUN: ClassVar[MeansOfDeath]
    GRENADE: ClassVar[MeansOfDeath]
    GRENADE_SPLASH: ClassVar[MeansOfDeath]
    ROCKET: ClassVar[MeansOfDeath]
    ROCKET_SPLASH: ClassVar[MeansOfDeath]
    PLASMA: ClassVar[MeansOfDeath]
    PLASMA_SPLASH: ClassVar[MeansOfDeath]
    RAILGUN: ClassVar[MeansOfDeath]
    LIGHTNING: ClassVar[MeansOfDeath]
    BFG: ClassVar[MeansOfDeath]
    BFG_SPLASH: ClassVar[MeansOfDeath]
    WATER: ClassVar[MeansOfDeath]
    SLIME: ClassVar[MeansOfDeath]
    LAVA: ClassVar[MeansOfDeath]
    CRUSH: ClassVar[MeansOfDeath]
    TELEFRAG: ClassVar[MeansOfDeath]
    FALLING: ClassVar[MeansOfDeath]
    SUICIDE: ClassVar[MeansOfDeath]
    TARGET_LASER: ClassVar[MeansOfDeath]
    TRIGGER_HURT: ClassVar[MeansOfDeath]
    NAIL: ClassVar[MeansOfDeath]
    CHAINGUN: ClassVar[MeansOfDeath]
    PROXIMITY_MINE: ClassVar[MeansOfDeath]
    KAMIKAZE: ClassVar[MeansOfDeath]
    JUICED: ClassVar[MeansOfDeath]
    GRAPPLE: ClassVar[MeansOfDeath]
    SWITCH_TEAMS: ClassVar[MeansOfDeath]
    THAW: ClassVar[MeansOfDeath]
    LIGHTNING_DISCHARGE: ClassVar[MeansOfDeath]
    HMG: ClassVar[MeansOfDeath]
    RAILGUN_HEADSHOT: ClassVar[MeansOfDeath]
    def __index__(self) -> int: ...

@final
class Flight(tuple):
    def __new__(cls, values: tuple[int, int, int, int]) -> Self: ...
    fuel: int
    max_fuel: int
    thrust: int
    refuel: int

@final
class Powerups(tuple):
    def __new__(cls, values: tuple[int, int, int, int, int, int]) -> Self: ...
    quad: int
    battlesuit: int
    haste: int
    invisibility: int
    regeneration: int
    invulnerability: int

@final
class Weapons(tuple):
    def __new__(
        cls, values: tuple[int, int, int, int, int, int, int, int, int, int, int, int, int, int, int]
    ) -> Self: ...
    g: int
    mg: int
    sg: int
    gl: int
    rl: int
    lg: int
    rg: int
    pg: int
    bfg: int
    gh: int
    ng: int
    pl: int
    cg: int
    hmg: int
    hands: int

@final
class PlayerInfo(tuple):
    def __new__(cls, tuple: builtins.tuple[int, str, int, str, int, int, int]) -> Self: ...
    @property
    def client_id(self) -> int: ...
    @property
    def name(self) -> str: ...
    @property
    def connection_state(self) -> int: ...
    @property
    def userinfo(self) -> str: ...
    @property
    def steam_id(self) -> int: ...
    @property
    def team(self) -> int: ...
    @property
    def privileges(self) -> int: ...

@final
class PlayerState(tuple):
    is_alive: bool
    position: Vector3
    velocity: Vector3
    health: int
    armor: int
    noclip: bool
    weapon: int
    weapons: Weapons
    ammo: Weapons
    powerups: Powerups
    holdable: Holdable
    flight: Flight
    is_chatting: bool
    is_frozen: bool

@final
class PlayerStats(tuple):
    score: int
    kills: int
    deaths: int
    damage_dealt: int
    damage_taken: int
    time: int
    ping: int

def player_info(client_id: int) -> PlayerInfo | None: ...
def players_info() -> Iterable[PlayerInfo]: ...
def get_userinfo(client_id: int) -> str | None: ...
def send_server_command(client_id: int | None, cmd: str) -> bool | None: ...
def client_command(client_id: int, cmd: str) -> bool | None: ...
def console_command(cmd: str) -> None: ...
def get_cvar(cvar: str) -> str | None: ...
def set_cvar(cvar: str, value: str, flags: SupportsIndex | None = ...) -> bool: ...
def set_cvar_limit(cvar: str, value: float, min: float, max: float, flags: SupportsIndex | None = ...) -> None: ...
def kick(client_id: int, reason: str | None = None) -> None: ...
def console_print(text: str) -> None: ...
def get_configstring(config_id: SupportsIndex) -> str: ...
def set_configstring(config_id: SupportsIndex, value: str) -> None: ...
def force_vote(_pass: bool) -> bool: ...
def add_console_command(command: str) -> None: ...
@overload
def register_handler(event: Literal["rcon"], handler: Callable[[str], bool | None] | None = ...) -> None: ...
@overload
def register_handler(
    event: Literal["client_command"],
    handler: Callable[[int, str], bool | str] | None = ...,
) -> None: ...
@overload
def register_handler(
    event: Literal["server_command"],
    handler: Callable[[int, str], bool | str] | None = ...,
) -> None: ...
@overload
def register_handler(event: Literal["new_game"], handler: Callable[[bool], bool | None] | None = ...) -> None: ...
@overload
def register_handler(
    event: Literal["set_configstring"],
    handler: Callable[[int, str], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    event: Literal["console_print"],
    handler: Callable[[str | None], bool | str | None] | None = ...,
) -> None: ...
@overload
def register_handler(event: Literal["frame"], handler: Callable[[], bool | None] | None = ...) -> None: ...
@overload
def register_handler(
    event: Literal["player_connect"],
    handler: Callable[[int, bool], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    event: Literal["player_loaded"],
    handler: Callable[[int], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    event: Literal["player_disconnect"],
    handler: Callable[[int, str | None], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(event: Literal["player_spawn"], handler: Callable[[int], bool | None] | None = ...) -> None: ...
@overload
def register_handler(event: Literal["kamikaze_use"], handler: Callable[[int], bool | None] | None = ...) -> None: ...
@overload
def register_handler(
    event: Literal["kamikaze_explode"],
    handler: Callable[[int, bool], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    event: Literal["damage"],
    handler: Callable[[int, int | None, int, int, int], bool | None] | None = ...,
) -> None: ...
def player_state(client_id: int) -> PlayerState | None: ...
def player_stats(client_id: int) -> PlayerStats | None: ...
def set_position(client_id: int, position: Vector3) -> bool: ...
def set_velocity(client_id: int, velocity: Vector3) -> bool: ...
def noclip(client_id: int, activate: bool) -> bool: ...
def set_health(client_id: int, health: int) -> bool: ...
def set_armor(client_id: int, armor: int) -> bool: ...
def set_weapons(client_id: int, weapons: Weapons) -> bool: ...
def set_weapon(client_id: int, weapon: int | Weapon) -> bool: ...
def set_ammo(client_id: int, ammos: Weapons) -> bool: ...
def set_powerups(client_id: int, powerups: Powerups) -> bool: ...
def set_holdable(client_id: int, holdable: SupportsIndex) -> bool: ...
def drop_holdable(client_id: int) -> bool: ...
def set_flight(client_id: int, flight: Flight) -> bool: ...
def set_invulnerability(client_id: int, time: int) -> bool: ...
def set_score(client_id: int, score: int) -> bool: ...
def callvote(vote: str, vote_disp: str, vote_time: int | None = ...) -> None: ...
def allow_single_player(allow: bool) -> None: ...
def player_spawn(client_id: int) -> bool: ...
def set_privileges(client_id: int, privileges: int) -> bool: ...
def destroy_kamikaze_timers() -> bool: ...
def spawn_item(item_id: int, x: int, y: int, z: int) -> bool: ...
def remove_dropped_items() -> bool: ...
def slay_with_mod(client_id: int, mean_of_death: SupportsIndex) -> bool: ...
def replace_items(item1: int | str, item2: int | str) -> bool: ...
def dev_print_items() -> None: ...
def force_weapon_respawn_time(respawn_time: int) -> bool: ...
def get_targetting_entities(entity_id: int) -> list[int]: ...
def set_cvar_once(cvar: str, value: str, flags: SupportsIndex = ...) -> bool: ...
def set_cvar_limit_once(
    cvar: str,
    value: float,
    min: float,
    max: float,
    flags: SupportsIndex = ...,
) -> bool: ...
