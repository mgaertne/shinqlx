import builtins
from collections.abc import Iterable, Iterator, Mapping
from enum import Enum
from os import PathLike
from typing import (
    Callable,
    ClassVar,
    Literal,
    SupportsIndex,
    TypedDict,
    final,
    overload,
)

from typing_extensions import Self, TypeAlias

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

class Objective(str, Enum):
    CAPTURE = "capture"
    RETURN = "return"
    ASSIST = "assist"
    BASE_DEFENSE = "base_defense"
    CARRIER_DEFENSE = "carrier_defense"
    FRAG_CARRIER = "frag_carrier"
    UNKNOWN = "unknown"

class FlagTeam(str, Enum):
    RED = "red"
    BLUE = "blue"
    NEUTRAL = "neutral"

class Medal(str, Enum):
    ACCURACY = "ACCURACY"
    ASSIST = "ASSIST"
    COMBOKILL = "COMBOKILL"
    DEFENSE = "DEFENSE"
    EXCELLENT = "EXCELLENT"
    FIRSTFRAG = "FIRSTFRAG"
    HUMILIATION = "HUMILIATION"
    IMPRESSIVE = "IMPRESSIVE"
    MIDAIR = "MIDAIR"
    PERFECT = "PERFECT"
    PERFORATED = "PERFORATED"
    QUADGOD = "QUADGOD"
    RAMPAGE = "RAMPAGE"
    REVENGE = "REVENGE"

class FlagStatus(str, Enum):
    AT_BASE = "at_base"
    TAKEN = "taken"
    DROPPED = "dropped"

class GameTypes(str, Enum):
    FREE_FOR_ALL = "ffa"
    DUEL = "duel"
    RACE = "race"
    TEAM_DEATHMATCH = "tdm"
    CLAN_ARENA = "ca"
    CAPTURE_THE_FLAG = "ctf"
    ONE_FLAG = "1f"
    HARVESTER = "har"
    FREEZE_TAG = "ft"
    DOMINATION = "dom"
    ATTACK_AND_DEFEND = "ad"
    RED_ROVER = "rr"
    UNKNOWN = "N/A"
    @property
    def long_name(self) -> str: ...

class VoteTypes(str, Enum):
    MAP = "map"
    MAP_RESTART = "map_restart"
    NEXTMAP = "nextmap"
    KICK = "kick"
    CLIENTKICK = "clientkick"
    SHUFFLE = "shuffle"
    TEAMSIZE = "teamsize"
    TIMELIMIT = "timelimit"
    FRAGLIMIT = "fraglimit"
    ROUNDLIMIT = "roundlimit"
    SCORELIMIT = "scorelimit"
    CAPTURELIMIT = "capturelimit"
    WEAPRESPAWN = "weaprespawn"
    COINTOSS = "cointoss"
    ALLREADY = "allready"
    ABORT = "abort"
    LOCK = "lock"
    UNLOCK = "unlock"
    PAUSE = "pause"
    UNPAUSE = "unpause"
    FORFEIT = "forfeit"

class StatsTypes(str, Enum):
    FLAG_STATUS = "FLAG_STATUS"
    MATCH_REPORT = "MATCH_REPORT"
    MATCH_STARTED = "MATCH_STARTED"
    PLAYER_CONNECT = "PLAYER_CONNECT"
    PLAYER_DEATH = "PLAYER_DEATH"
    PLAYER_DISCONNECT = "PLAYER_DISCONNECT"
    PLAYER_KILL = "PLAYER_KILL"
    PLAYER_MEDAL = "PLAYER_MEDAL"
    PLAYER_STATS = "PLAYER_STATS"
    PLAYER_SWITCHTEAM = "PLAYER_SWITCHTEAM"
    ROUND_OVER = "ROUND_OVER"

class ChatChannels(str, Enum):
    CONSOLE = "console"
    CHAT = "chat"
    TELL = "tell"
    RED_TEAM_CHAT = "red_team_chat"
    BLUE_TEAM_CHAT = "blue_team_chat"
    FREE_CHAT = "free_chat"
    SPECTATOR_CHAT = "spectator_chat"

class EntityTypes(str, Enum):
    GENERAL = "general"
    PLAYER = "player"
    ITEM = "item"
    MISSILE = "missile"
    MOVER = "mover"
    BEAM = "beam"
    PORTAL = "portal"
    SPEAKER = "speaker"
    PUSH_TRIGGER = "push_trigger"
    TELEPORT_TRIGGER = "teleport_trigger"
    INVISIBLE = "invisible"
    GRAPPLE = "grapple"
    TEAM = "team"
    EVENTS = "events"

class Factories(str, Enum):
    TRAINING = "_training"
    RJ = "_rj"
    SJ = "_sj"
    ACTF = "actf"
    ATTACK_AND_DEFEND = "ad"
    CLAN_ARENA = "ca"
    CAPTURE_THE_FLAG = "ctf"
    DOMINATION = "dom"
    DUEL = "duel"
    ONE_FLAG = "oneflag"
    FREE_FOR_ALL = "ffa"
    FREEZE_TAG = "ft"
    HARVESTER = "har"
    INSTAGIB_CTF = "ictf"
    INSTAGIB_FFA = "iffa"
    INSTAGIB_FREEZE_TAG = "ift"
    INFECTED = "infected"
    QUADHOG = "quadhog"
    RACE = "race"
    RED_ROVER = "rr"
    TEAM_DEATHMATCH = "tdm"
    VCA = "vca"

class MapSourceKind(str, Enum):
    BASE = "base"
    HOME = "home"
    WORKSHOP = "workshop"

class DemoStates(str, Enum):
    UNAVAILABLE = "unavailable"
    DISABLED = "disabled"
    WAITING = "waiting"
    RECORDING = "recording"
    DISK_FULL = "disk_full"

class Events(str, Enum):
    CHAT = "chat"
    CLIENT_COMMAND = "client_command"
    COMMAND = "command"
    CONSOLE_PRINT = "console_print"
    DAMAGE = "damage"
    DEATH = "death"
    DEMO_FINISHED = "demo_finished"
    FRAME = "frame"
    GAME_COUNTDOWN = "game_countdown"
    GAME_END = "game_end"
    GAME_START = "game_start"
    OBJECTIVE = "objective"
    MEDAL = "medal"
    FLAG_STATUS = "flag_status"
    WEAPON_FIRED = "weapon_fired"
    CVAR_CHANGED = "cvar_changed"
    ITEM_PICKUP = "item_pickup"
    ITEM_TOUCH = "item_touch"
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
class RoundStates:
    PREGAME: ClassVar[RoundStates]
    ROUND_WARMUP: ClassVar[RoundStates]
    ROUND_SHUFFLE: ClassVar[RoundStates]
    ROUND_BEGUN: ClassVar[RoundStates]
    ROUND_OVER: ClassVar[RoundStates]
    POSTGAME: ClassVar[RoundStates]
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
    def __new__(cls, value: SupportsIndex = 0) -> Self: ...
    def __index__(self) -> int: ...
    def __int__(self) -> int: ...
    def __bool__(self) -> bool: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...
    def __or__(self, other: SupportsIndex, /) -> CvarFlags: ...
    def __ror__(self, other: SupportsIndex, /) -> CvarFlags: ...
    def __and__(self, other: SupportsIndex, /) -> CvarFlags: ...
    def __rand__(self, other: SupportsIndex, /) -> CvarFlags: ...
    def __xor__(self, other: SupportsIndex, /) -> CvarFlags: ...
    def __rxor__(self, other: SupportsIndex, /) -> CvarFlags: ...
    def __invert__(self) -> CvarFlags: ...
    def __contains__(self, other: SupportsIndex, /) -> bool: ...
    def __iter__(self) -> Iterator[CvarFlags]: ...

@final
class DamageFlags:
    RADIUS: ClassVar[DamageFlags]
    NO_ARMOR: ClassVar[DamageFlags]
    NO_KNOCKBACK: ClassVar[DamageFlags]
    NO_PROTECTION: ClassVar[DamageFlags]
    NO_TEAM_PROTECTION: ClassVar[DamageFlags]
    def __new__(cls, value: SupportsIndex = 0) -> Self: ...
    def __index__(self) -> int: ...
    def __int__(self) -> int: ...
    def __bool__(self) -> bool: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...
    def __or__(self, other: SupportsIndex, /) -> DamageFlags: ...
    def __ror__(self, other: SupportsIndex, /) -> DamageFlags: ...
    def __and__(self, other: SupportsIndex, /) -> DamageFlags: ...
    def __rand__(self, other: SupportsIndex, /) -> DamageFlags: ...
    def __xor__(self, other: SupportsIndex, /) -> DamageFlags: ...
    def __rxor__(self, other: SupportsIndex, /) -> DamageFlags: ...
    def __invert__(self) -> DamageFlags: ...
    def __contains__(self, other: SupportsIndex, /) -> bool: ...
    def __iter__(self) -> Iterator[DamageFlags]: ...

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
class EntityInfo:
    @property
    def id(self) -> int: ...
    @property
    def classname(self) -> str: ...
    @property
    def etype(self) -> str: ...
    @property
    def type_id(self) -> int: ...
    @property
    def position(self) -> Vector3: ...
    @property
    def in_use(self) -> bool: ...

@final
class DemoPlayer:
    @property
    def steam_id(self) -> int: ...
    @property
    def name(self) -> str: ...
    @property
    def client_id(self) -> int: ...

@final
class DemoRequest:
    @property
    def requested_by(self) -> int | None: ...
    @property
    def reason(self) -> str: ...
    @property
    def time(self) -> int: ...

@final
class DemoResult:
    @property
    def aborted(self) -> bool: ...
    @property
    def red_score(self) -> int: ...
    @property
    def blue_score(self) -> int: ...

@final
class DemoScore:
    @property
    def steam_id(self) -> int: ...
    @property
    def name(self) -> str: ...
    @property
    def team(self) -> str: ...
    @property
    def score(self) -> int: ...

@final
class Recording:
    @property
    def name(self) -> str: ...
    @property
    def path(self) -> str: ...
    @property
    def map(self) -> str: ...
    @property
    def factory(self) -> str: ...
    @property
    def gametype(self) -> str: ...
    @property
    def started(self) -> int: ...
    @property
    def ended(self) -> int | None: ...
    @property
    def size(self) -> int: ...
    @property
    def match_played(self) -> bool: ...
    @property
    def players(self) -> tuple[DemoPlayer, ...]: ...
    @property
    def requests(self) -> tuple[DemoRequest, ...]: ...
    @property
    def match_guid(self) -> str | None: ...
    @property
    def result(self) -> DemoResult | None: ...
    @property
    def scores(self) -> tuple[DemoScore, ...]: ...

@final
class DemoStatus:
    @property
    def state(self) -> DemoStates: ...
    @property
    def current(self) -> Recording | None: ...

@final
class ExportedDemo:
    @property
    def path(self) -> str: ...
    @property
    def player(self) -> DemoPlayer: ...

@final
class MapSource:
    @property
    def kind(self) -> MapSourceKind: ...
    @property
    def pk3_path(self) -> str: ...
    @property
    def workshop_id(self) -> int | None: ...
    @property
    def has_aas(self) -> bool: ...
    @property
    def has_levelshot(self) -> bool: ...
    @property
    def has_preview(self) -> bool: ...

@final
class ArenaInfo:
    @property
    def map(self) -> str: ...
    @property
    def longname(self) -> str | None: ...
    @property
    def author(self) -> str | None: ...
    @property
    def author2(self) -> str | None: ...
    @property
    def gametypes(self) -> tuple[GameTypes, ...]: ...
    @property
    def type_tokens(self) -> tuple[str, ...]: ...
    @property
    def fraglimit(self) -> int | None: ...
    @property
    def timelimit(self) -> int | None: ...
    @property
    def capturelimit(self) -> int | None: ...
    @property
    def raw(self) -> Mapping[str, str]: ...
    @property
    def source(self) -> MapSource | None: ...

@final
class MapInfo:
    @property
    def name(self) -> str: ...
    @property
    def longname(self) -> str: ...
    @property
    def author(self) -> str | None: ...
    @property
    def author2(self) -> str | None: ...
    @property
    def gametypes(self) -> tuple[GameTypes, ...]: ...
    @property
    def type_tokens(self) -> tuple[str, ...]: ...
    @property
    def arenas(self) -> tuple[ArenaInfo, ...]: ...
    @property
    def sources(self) -> tuple[MapSource, ...]: ...
    @property
    def has_levelshot(self) -> bool: ...
    @property
    def has_preview(self) -> bool: ...
    @property
    def has_aas(self) -> bool: ...

@final
class FactoryInfo:
    @property
    def id(self) -> Factories | str: ...
    @property
    def title(self) -> str | None: ...
    @property
    def author(self) -> str | None: ...
    @property
    def description(self) -> str | None: ...
    @property
    def basegt(self) -> GameTypes | str | None: ...
    @property
    def cvars(self) -> Mapping[str, str]: ...
    @property
    def source(self) -> MapSource | None: ...

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
def register_handler(
    event: Literal["objective"], handler: Callable[[int, str, int], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(event: Literal["medal"], handler: Callable[[int, str, int], bool | None] | None = ...) -> None: ...
@overload
def register_handler(
    event: Literal["flag_status"], handler: Callable[[int, int, int | None], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(
    event: Literal["weapon_fired"], handler: Callable[[int, int], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(
    event: Literal["cvar_changed"], handler: Callable[[str, str, str], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(
    event: Literal["item_pickup"], handler: Callable[[int, int, str], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(
    event: Literal["item_touch"], handler: Callable[[int, int, str], bool | None] | None = ...
) -> None: ...
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
def entities(in_use: bool = True, etype: str | EntityTypes | None = None) -> list[EntityInfo]: ...
def remove_entity(entity_id: int) -> bool: ...
def maps(gametype: str | GameTypes | None = None) -> tuple[MapInfo, ...]: ...
def map_info(name: str) -> MapInfo | None: ...
def factories() -> tuple[FactoryInfo, ...]: ...
def factory_info(factory_id: str | Factories) -> FactoryInfo | None: ...
def reload_maps() -> bool: ...
def demo_status() -> DemoStatus: ...
def recordings() -> tuple[Recording, ...]: ...
def recording(name: str) -> Recording | None: ...
def request_demo(name: str | None = None, *, requested_by: int | None = None, reason: str = "") -> Recording | None: ...
def export_demo(
    name: str,
    directory: str | PathLike[str],
    *,
    steam_ids: Iterable[int] | None = None,
    include_private: bool = True,
) -> tuple[ExportedDemo, ...]: ...
def move_entity(entity_id: int, x: float, y: float, z: float) -> bool: ...

# BEGIN spawn_entity (generated from entity_classes.rs, see its test)
_SpawnValue: TypeAlias = str | int | float

class _CommonSpawnKeys(TypedDict, total=False):
    origin: _SpawnValue
    angle: _SpawnValue
    angles: _SpawnValue
    spawnflags: int
    target: _SpawnValue
    targetname: _SpawnValue
    model: _SpawnValue
    model2: _SpawnValue
    team: _SpawnValue
    speed: _SpawnValue
    message: _SpawnValue
    wait: _SpawnValue
    random: _SpawnValue
    count: _SpawnValue
    health: _SpawnValue
    light: _SpawnValue
    dmg: _SpawnValue
    gametype: _SpawnValue
    not_gametype: _SpawnValue
    notfree: _SpawnValue
    notteam: _SpawnValue
    notsingle: _SpawnValue
    notbot: _SpawnValue
    nohumans: _SpawnValue
    nobots: _SpawnValue
    targetShaderName: _SpawnValue
    targetShaderNewName: _SpawnValue

class _AdvertisementKeys(_CommonSpawnKeys, total=False):
    cellId: _SpawnValue
    color: _SpawnValue

class _FuncBobbingKeys(_CommonSpawnKeys, total=False):
    color: _SpawnValue
    height: _SpawnValue
    noise: _SpawnValue
    phase: _SpawnValue

class _FuncButtonKeys(_CommonSpawnKeys, total=False):
    color: _SpawnValue
    lip: _SpawnValue

class _FuncDoorKeys(_CommonSpawnKeys, total=False):
    color: _SpawnValue
    endsound: _SpawnValue
    lip: _SpawnValue
    startsound: _SpawnValue

class _FuncPendulumKeys(_CommonSpawnKeys, total=False):
    color: _SpawnValue
    noise: _SpawnValue
    phase: _SpawnValue

class _FuncPlatKeys(_CommonSpawnKeys, total=False):
    color: _SpawnValue
    height: _SpawnValue
    lip: _SpawnValue

class _FuncRotatingKeys(_CommonSpawnKeys, total=False):
    color: _SpawnValue
    noise: _SpawnValue

class _FuncStaticKeys(_CommonSpawnKeys, total=False):
    color: _SpawnValue

class _FuncTrainKeys(_CommonSpawnKeys, total=False):
    color: _SpawnValue

class _InfoCampKeys(_CommonSpawnKeys, total=False):
    range: _SpawnValue
    weight: _SpawnValue

class _InfoTourPointKeys(_CommonSpawnKeys, total=False):
    cvar: _SpawnValue
    cvarValue: _SpawnValue
    ignorePlayerLocation: _SpawnValue
    printChatTextTime: _SpawnValue
    radius: _SpawnValue
    tourPointTarget: _SpawnValue
    tourPointTargetName: _SpawnValue

class _ItemBotroamKeys(_CommonSpawnKeys, total=False):
    weight: _SpawnValue

class _MiscPortalCameraKeys(_CommonSpawnKeys, total=False):
    roll: _SpawnValue

class _TargetDelayKeys(_CommonSpawnKeys, total=False):
    delay: _SpawnValue

class _TargetSpeakerKeys(_CommonSpawnKeys, total=False):
    noise: _SpawnValue

class _TargetCvarKeys(_CommonSpawnKeys, total=False):
    cvar: _SpawnValue
    cvarValue: _SpawnValue

class _TeamDomPointKeys(_CommonSpawnKeys, total=False):
    identifier: _SpawnValue

@overload
def spawn_entity(
    classname: Literal["advertisement"],
    keys: _AdvertisementKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_pack"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_bfg"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_belt"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_hmg"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_bullets"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_cells"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_grenades"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_lightning"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_nails"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_mines"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_rockets"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_shells"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["ammo_slugs"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["func_bobbing"],
    keys: _FuncBobbingKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["func_button"],
    keys: _FuncButtonKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["func_door"],
    keys: _FuncDoorKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["func_pendulum"],
    keys: _FuncPendulumKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["func_plat"],
    keys: _FuncPlatKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["func_rotating"],
    keys: _FuncRotatingKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["func_static"],
    keys: _FuncStaticKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["func_timer"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["func_train"],
    keys: _FuncTrainKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["holdable_invulnerability"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["holdable_kamikaze"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["holdable_medkit"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["holdable_teleporter"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["info_camp"],
    keys: _InfoCampKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["info_tour_point"],
    keys: _InfoTourPointKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["info_notnull"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["info_null"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["info_player_deathmatch"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["info_player_intermission"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["info_player_start"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_armor_body"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_armor_combat"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_armor_jacket"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_armor_shard"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_botroam"],
    keys: _ItemBotroamKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_enviro"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_flight"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_haste"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_health"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_health_large"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_health_mega"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_health_small"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_key_silver"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_key_gold"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_key_master"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_invis"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_quad"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_regen"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["misc_portal_camera"],
    keys: _MiscPortalCameraKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["misc_portal_surface"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["misc_teleporter_dest"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["path_corner"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_scout"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_guard"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_doubler"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["item_ammoregen"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["race_point"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["shooter_grenade"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["shooter_plasma"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["shooter_rocket"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_delay"],
    keys: _TargetDelayKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_give"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_kill"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_location"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_position"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_print"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_push"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_relay"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_remove_powerups"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_score"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_speaker"],
    keys: _TargetSpeakerKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_teleporter"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["target_cvar"],
    keys: _TargetCvarKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_dom_point"],
    keys: _TeamDomPointKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_blueobelisk"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_CTF_blueflag"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_CTF_blueplayer"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_CTF_bluespawn"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_CTF_neutralflag"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_CTF_redflag"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_CTF_redplayer"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_CTF_redspawn"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_neutralobelisk"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["team_redobelisk"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["trigger_always"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["trigger_hurt"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["trigger_multiple"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["trigger_push"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["trigger_teleport"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_bfg"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_chaingun"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_gauntlet"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_grapplinghook"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_grenadelauncher"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_lightning"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_machinegun"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_nailgun"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_plasmagun"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_prox_launcher"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_railgun"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_rocketlauncher"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_shotgun"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: Literal["weapon_hmg"],
    keys: _CommonSpawnKeys | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...
@overload
def spawn_entity(
    classname: str,
    keys: dict[str, _SpawnValue] | None = None,
    *,
    strict: bool = True,
) -> EntityInfo | None: ...

# END spawn_entity
def set_cvar_once(name: str, value: str, flags: SupportsIndex = ...) -> bool: ...
def set_cvar_limit_once(
    name: str,
    value: float,
    minimum: float,
    maximum: float,
    flags: SupportsIndex = ...,
) -> bool: ...
