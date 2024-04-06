from abc import abstractmethod
from typing import TYPE_CHECKING, overload, ClassVar

if TYPE_CHECKING:
    import sys
    from logging import Logger

    if sys.version_info >= (3, 11):
        from typing import NotRequired, Unpack
    else:
        from typing_extensions import NotRequired, Unpack
    from typing import (
        Pattern,
        Callable,
        Iterable,
        Mapping,
        TypedDict,
        Literal,
        Type,
        Protocol,
    )
    from datetime import timedelta
    from queue import Queue
    from sched import scheduler

    from types import TracebackType, ModuleType

    from shinqlx.database import Redis

# from __init__.pyi
_map_title: str | None
_map_subtitle1: str | None
_map_subtitle2: str | None

# from _shinqlx.pyi
__version__: str
DEBUG: bool

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

class Vector3(tuple):
    x: int
    y: int
    z: int

class Flight(tuple):
    fuel: int
    max_fuel: int
    thrust: int
    refuel: int

class Powerups(tuple):
    quad: int
    battlesuit: int
    haste: int
    invisibility: int
    regeneration: int
    invulnerability: int

class Weapons(tuple):
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

class PlayerInfo(tuple):
    def __init__(self, tuple: tuple[int, str, int, str, int, int, int]) -> None: ...
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
    holdable: int
    flight: Flight
    is_chatting: bool
    is_frozen: bool

class PlayerStats(tuple):
    score: int
    kills: int
    deaths: int
    damage_dealt: int
    damage_taken: int
    time: int
    ping: int

def player_info(_client_id: int) -> PlayerInfo | None: ...
def players_info() -> Iterable[PlayerInfo]: ...
def get_userinfo(_client_id: int) -> str | None: ...
def send_server_command(_client_id: int | None, _cmd: str) -> bool | None: ...
def client_command(_client_id: int, _cmd: str) -> bool | None: ...
def console_command(_cmd: str) -> None: ...
def get_cvar(_cvar: str) -> str | None: ...
def set_cvar(_cvar: str, _value: str, _flags: int | None = ...) -> bool: ...
def set_cvar_limit(
    _name: str, _value: int | float, _min: int | float, _max: int | float, _flags: int
) -> None: ...
def kick(_client_id: int, _reason: str | None = None) -> None: ...
def console_print(_text: str) -> None: ...
def get_configstring(_config_id: int) -> str: ...
def set_configstring(_config_id: int, _value: str) -> None: ...
def force_vote(_pass: bool) -> bool: ...
def add_console_command(_command: str) -> None: ...
@overload
def register_handler(
    _event: Literal["rcon"], _handler: Callable[[str], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(
    _event: Literal["client_command"],
    _handler: Callable[[int, str], bool | str] | None = ...,
) -> None: ...
@overload
def register_handler(
    _event: Literal["server_command"],
    _handler: Callable[[int, str], bool | str] | None = ...,
) -> None: ...
@overload
def register_handler(
    _event: Literal["new_game"], _handler: Callable[[bool], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(
    _event: Literal["set_configstring"],
    _handler: Callable[[int, str], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    _event: Literal["console_print"],
    _handler: Callable[[str | None], bool | str | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    _event: Literal["frame"], _handler: Callable[[], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(
    _event: Literal["player_connect"],
    _handler: Callable[[int, bool], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    _event: Literal["player_loaded"],
    _handler: Callable[[int], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    _event: Literal["player_disconnect"],
    _handler: Callable[[int, str | None], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    _event: Literal["player_spawn"], _handler: Callable[[int], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(
    _event: Literal["kamikaze_use"], _handler: Callable[[int], bool | None] | None = ...
) -> None: ...
@overload
def register_handler(
    _event: Literal["kamikaze_explode"],
    _handler: Callable[[int, bool], bool | None] | None = ...,
) -> None: ...
@overload
def register_handler(
    _event: Literal["damage"],
    _handler: Callable[[int, int | None, int, int, int], bool | None] | None = ...,
) -> None: ...
def player_state(_client_id: int) -> PlayerState | None: ...
def player_stats(_client_id: int) -> PlayerStats | None: ...
def set_position(_client_id: int, _position: Vector3) -> bool: ...
def set_velocity(_client_id: int, _velocity: Vector3) -> bool: ...
def noclip(_client_id: int, _activate: bool) -> bool: ...
def set_health(_client_id: int, _health: int) -> bool: ...
def set_armor(_client_id: int, _armor: int) -> bool: ...
def set_weapons(_client_id: int, _weapons: Weapons) -> bool: ...
def set_weapon(_client_id: int, _weapon: int) -> bool: ...
def set_ammo(_client_id: int, _ammo: Weapons) -> bool: ...
def set_powerups(_client_id: int, _powerups: Powerups) -> bool: ...
def set_holdable(_client_id: int, _powerup: int) -> bool: ...
def drop_holdable(_client_id: int) -> bool: ...
def set_flight(_client_id: int, _flight: Flight) -> bool: ...
def set_invulnerability(_client_id: int, _time: int) -> bool: ...
def set_score(_client_id: int, _score: int) -> bool: ...
def callvote(_vote: str, _vote_display: str, _vote_time: int | None = ...) -> None: ...
def allow_single_player(_allow: bool) -> None: ...
def player_spawn(_client_id: int) -> bool: ...
def set_privileges(_client_id: int, _privileges: int) -> bool: ...
def destroy_kamikaze_timers() -> bool: ...
def spawn_item(_item_id: int, _x: int, _y: int, _z: int) -> bool: ...
def remove_dropped_items() -> bool: ...
def slay_with_mod(_client_id: int, _mod: int) -> bool: ...
def replace_items(_item1: int | str, _item2: int | str) -> bool: ...
def dev_print_items() -> None: ...
def force_weapon_respawn_time(_respawn_time: int) -> bool: ...
def get_targetting_entities(_entity_id: int) -> list[int]: ...

# from _core.pyi
class PluginLoadError(Exception): ...
class PluginUnloadError(Exception): ...

TEAMS: dict[int, str]

# game types
GAMETYPES: dict[int, str]
GAMETYPES_SHORT: dict[int, str]

CONNECTION_STATES: dict[int, str]
WEAPONS: dict[int, str]

DEFAULT_PLUGINS: tuple[str, ...]

_thread_count: int
_thread_name: str

def parse_variables(varstr: str, ordered: bool = False) -> dict[str, str]: ...
def get_logger(plugin: Plugin | str | None = ...) -> Logger: ...
def _configure_logger() -> None: ...
def log_exception(plugin: Plugin | str | None = ...) -> None: ...
def handle_exception(
    exc_type: Type[BaseException],
    exc_value: BaseException,
    exc_traceback: TracebackType | None,
) -> None: ...

class ExceptHookArgs(Protocol):
    exc_traceback: TracebackType
    exc_type: Type[BaseException]
    exc_value: BaseException

def threading_excepthook(args: ExceptHookArgs) -> None: ...
def uptime() -> timedelta: ...
def owner() -> int | None: ...

_stats: StatsListener | None

def stats_listener() -> StatsListener: ...
def set_cvar_once(name: str, value: str, flags: int = ...) -> bool: ...
def set_cvar_limit_once(
    name: str,
    value: int | float,
    minimum: int | float,
    maximum: int | float,
    flags: int = ...,
) -> bool: ...
def set_plugins_version(path: str) -> None: ...
def set_map_subtitles() -> None: ...
def next_frame(func: Callable) -> Callable: ...
def delay(time: float) -> Callable: ...
def thread(func: Callable, force: bool = ...) -> Callable: ...

_modules: dict[str, ModuleType]

def load_preset_plugins() -> None: ...
def load_plugin(plugin: str) -> None: ...
def unload_plugin(plugin: str) -> None: ...
def reload_plugin(plugin: str) -> None: ...
def initialize_cvars() -> None: ...
def initialize() -> None: ...
def late_init() -> None: ...

# from _game.pyi
class NonexistentGameError(Exception): ...

class Game:
    cached: bool
    _valid: bool

    def __init__(self, cached: bool = ...) -> None: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def __contains__(self, key: str) -> bool: ...
    def __getitem__(self, key: str) -> str: ...
    @property
    def cvars(self) -> Mapping[str, str]: ...
    @property
    def type(self) -> str: ...
    @property
    def type_short(self) -> str: ...
    @property
    def map(self) -> str: ...
    @map.setter
    def map(self, value: str) -> None: ...
    @property
    def map_title(self) -> str | None: ...
    @property
    def map_subtitle1(self) -> str | None: ...
    @property
    def map_subtitle2(self) -> str | None: ...
    @property
    def red_score(self) -> int: ...
    @property
    def blue_score(self) -> int: ...
    @property
    def state(self) -> str: ...
    @property
    def factory(self) -> str: ...
    @factory.setter
    def factory(self, value: str) -> None: ...
    @property
    def factory_title(self) -> str: ...
    @property
    def hostname(self) -> str: ...
    @hostname.setter
    def hostname(self, value: str) -> None: ...
    @property
    def instagib(self) -> bool: ...
    @instagib.setter
    def instagib(self, value: bool | int) -> None: ...
    @property
    def loadout(self) -> bool: ...
    @loadout.setter
    def loadout(self, value: bool | int) -> None: ...
    @property
    def maxclients(self) -> int: ...
    @maxclients.setter
    def maxclients(self, new_limit: int) -> None: ...
    @property
    def timelimit(self) -> int: ...
    @timelimit.setter
    def timelimit(self, new_limit: int) -> None: ...
    @property
    def fraglimit(self) -> int: ...
    @fraglimit.setter
    def fraglimit(self, new_limit: int) -> None: ...
    @property
    def roundlimit(self) -> int: ...
    @roundlimit.setter
    def roundlimit(self, new_limit: int) -> None: ...
    @property
    def roundtimelimit(self) -> int: ...
    @roundtimelimit.setter
    def roundtimelimit(self, new_limit: int) -> None: ...
    @property
    def scorelimit(self) -> int: ...
    @scorelimit.setter
    def scorelimit(self, new_limit: int) -> None: ...
    @property
    def capturelimit(self) -> int: ...
    @capturelimit.setter
    def capturelimit(self, new_limit: int) -> None: ...
    @property
    def teamsize(self) -> int: ...
    @teamsize.setter
    def teamsize(self, new_size: int) -> None: ...
    @property
    def tags(self) -> Iterable[str]: ...
    @tags.setter
    def tags(self, new_tags: str | Iterable[str]) -> None: ...
    @property
    def workshop_items(self) -> Iterable[int]: ...
    @workshop_items.setter
    def workshop_items(self, new_items: Iterable[int]) -> None: ...
    @classmethod
    def shuffle(cls) -> None: ...
    @classmethod
    def timeout(cls) -> None: ...
    @classmethod
    def timein(cls) -> None: ...
    @classmethod
    def allready(cls) -> None: ...
    @classmethod
    def pause(cls) -> None: ...
    @classmethod
    def unpause(cls) -> None: ...
    @classmethod
    def lock(cls, team: str | None = ...) -> None: ...
    @classmethod
    def unlock(cls, team: str | None = ...) -> None: ...
    @classmethod
    def put(cls, player: Player, team: str) -> None: ...
    @classmethod
    def mute(cls, player: Player) -> None: ...
    @classmethod
    def unmute(cls, player: Player) -> None: ...
    @classmethod
    def tempban(cls, player: Player) -> None: ...
    @classmethod
    def ban(cls, player: Player) -> None: ...
    @classmethod
    def unban(cls, player: Player) -> None: ...
    @classmethod
    def opsay(cls, msg: str) -> None: ...
    @classmethod
    def addadmin(cls, player: Player) -> None: ...
    @classmethod
    def addmod(cls, player: Player) -> None: ...
    @classmethod
    def demote(cls, player: Player) -> None: ...
    @classmethod
    def abort(cls) -> None: ...
    @classmethod
    def addscore(cls, player: Player, score: int) -> None: ...
    @classmethod
    def addteamscore(cls, team: str, score: int) -> None: ...
    @classmethod
    def setmatchtime(cls, time: int) -> None: ...

# from _player.pyi
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

Vector3Kwargs = TypedDict(
    "Vector3Kwargs",
    {
        "x": NotRequired[int | float],
        "y": NotRequired[int | float],
        "z": NotRequired[int | float],
    },
)

WeaponsKwargs = TypedDict(
    "WeaponsKwargs",
    {
        "g": NotRequired[bool | int],
        "mg": NotRequired[bool | int],
        "sg": NotRequired[bool | int],
        "gl": NotRequired[bool | int],
        "rl": NotRequired[bool | int],
        "lg": NotRequired[bool | int],
        "rg": NotRequired[bool | int],
        "pg": NotRequired[bool | int],
        "bfg": NotRequired[bool | int],
        "gh": NotRequired[bool | int],
        "ng": NotRequired[bool | int],
        "pl": NotRequired[bool | int],
        "cg": NotRequired[bool | int],
        "hmg": NotRequired[bool | int],
        "hands": NotRequired[bool | int],
    },
)

AmmoKwargs = TypedDict(
    "AmmoKwargs",
    {
        "g": NotRequired[int],
        "mg": NotRequired[int],
        "sg": NotRequired[int],
        "gl": NotRequired[int],
        "rl": NotRequired[int],
        "lg": NotRequired[int],
        "rg": NotRequired[int],
        "pg": NotRequired[int],
        "bfg": NotRequired[int],
        "gh": NotRequired[int],
        "ng": NotRequired[int],
        "pl": NotRequired[int],
        "cg": NotRequired[int],
        "hmg": NotRequired[int],
        "hands": NotRequired[int],
    },
)

PowerupsKwargs = TypedDict(
    "PowerupsKwargs",
    {
        "quad": NotRequired[str | float],
        "battlesuit": NotRequired[str | float],
        "haste": NotRequired[str | float],
        "invisibility": NotRequired[str | float],
        "regeneration": NotRequired[str | float],
        "invulnerability": NotRequired[str | float],
    },
)

FlightKwargs = TypedDict(
    "FlightKwargs",
    {
        "fuel": NotRequired[int],
        "max_fuel": NotRequired[int],
        "thrust": NotRequired[int],
        "refuel": NotRequired[int],
    },
)

class NonexistentPlayerError(Exception): ...

class Player:
    @classmethod
    def all_players(cls) -> list[Player]: ...

    _valid: bool
    _id: int
    _info: PlayerInfo | None
    _userinfo: UserInfo | None
    _steam_id = int
    _name: str

    def __init__(self, client_id: int, info: PlayerInfo | None = ...) -> None: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def __contains__(self, key: str) -> bool: ...
    def __getitem__(self, key: str) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    def __ne__(self, other: object) -> bool: ...
    def update(self) -> None: ...
    def _invalidate(self, e: str = ...) -> None: ...
    @property
    def cvars(self) -> dict[str, str | int]: ...
    @cvars.setter
    def cvars(self, new_cvars: dict[str, str | int]) -> None: ...
    @property
    def steam_id(self) -> int: ...
    @property
    def id(self) -> int: ...
    @property
    def ip(self) -> str: ...
    @property
    def clan(self) -> str: ...
    @clan.setter
    def clan(self, tag: str) -> None: ...
    @property
    def name(self) -> str: ...
    @name.setter
    def name(self, value: str) -> None: ...
    @property
    def clean_name(self) -> str: ...
    @property
    def qport(self) -> int: ...
    @property
    def team(self) -> Literal["free", "red", "blue", "spectator"]: ...
    @team.setter
    def team(self, new_team: Literal["free", "red", "blue", "spectator"]) -> None: ...
    @property
    def colors(self) -> tuple[float, float]: ...
    @colors.setter
    def colors(self, value: tuple[float, float]) -> None: ...
    @property
    def model(self) -> str: ...
    @model.setter
    def model(self, value: str) -> None: ...
    @property
    def headmodel(self) -> str: ...
    @headmodel.setter
    def headmodel(self, value: str) -> None: ...
    @property
    def handicap(self) -> str: ...
    @handicap.setter
    def handicap(self, value: str) -> None: ...
    @property
    def autohop(self) -> bool: ...
    @autohop.setter
    def autohop(self, value: bool) -> None: ...
    @property
    def autoaction(self) -> bool: ...
    @autoaction.setter
    def autoaction(self, value: bool) -> None: ...
    @property
    def predictitems(self) -> bool: ...
    @predictitems.setter
    def predictitems(self, value: bool) -> None: ...
    @property
    def connection_state(
        self,
    ) -> Literal["free", "zombie", "connected", "primed", "active"]: ...
    @property
    def state(self) -> PlayerState | None: ...
    @property
    def privileges(self) -> Literal["mod", "admin", "root", "banned"]: ...
    @privileges.setter
    def privileges(self, value: None | Literal["none", "mod", "admin"]) -> None: ...
    @property
    def country(self) -> str: ...
    @country.setter
    def country(self, value: str) -> None: ...
    @property
    def valid(self) -> bool: ...
    @property
    def stats(self) -> PlayerStats | None: ...
    @property
    def ping(self) -> int: ...
    def position(
        self, reset: bool = ..., **kwargs: Unpack[Vector3Kwargs]
    ) -> Vector3 | bool: ...
    def velocity(
        self, reset: bool = ..., **kwargs: Unpack[Vector3Kwargs]
    ) -> bool | Vector3: ...
    def weapons(
        self, reset: bool = ..., **kwargs: Unpack[WeaponsKwargs]
    ) -> bool | Weapons: ...
    def weapon(
        self,
        new_weapon: (
            Literal[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
            | Literal[
                "g",
                "mg",
                "sg",
                "gl",
                "rl",
                "lg",
                "rg",
                "pg",
                "bfg",
                "gh",
                "ng",
                "pl",
                "cg",
                "hmg",
                "hands",
            ]
            | None
        ) = ...,
    ) -> bool | Literal[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]: ...
    def ammo(
        self, reset: bool = ..., **kwargs: Unpack[AmmoKwargs]
    ) -> bool | Weapons: ...
    def powerups(
        self, reset: bool = ..., **kwargs: Unpack[PowerupsKwargs]
    ) -> bool | Powerups: ...
    @property
    def holdable(
        self,
    ) -> (
        None
        | Literal[
            "teleporter", "medkit", "flight", "kamikaze", "portal", "invulnerability"
        ]
    ): ...
    @holdable.setter
    def holdable(
        self,
        value: (
            None
            | Literal[
                "teleporter",
                "medkit",
                "flight",
                "kamikaze",
                "portal",
                "invulnerability",
                "none",
            ]
        ),
    ) -> None: ...
    def drop_holdable(self) -> None: ...
    def flight(
        self, reset: bool = ..., **kwargs: Unpack[FlightKwargs]
    ) -> bool | Flight: ...
    @property
    def noclip(self) -> bool: ...
    @noclip.setter
    def noclip(self, value: bool | int | str) -> None: ...
    @property
    def health(self) -> int: ...
    @health.setter
    def health(self, value: int) -> None: ...
    @property
    def armor(self) -> int: ...
    @armor.setter
    def armor(self, value: int) -> None: ...
    @property
    def is_alive(self) -> bool: ...
    @is_alive.setter
    def is_alive(self, value: bool) -> None: ...
    @property
    def is_frozen(self) -> bool: ...
    @property
    def is_chatting(self) -> bool: ...
    @property
    def score(self) -> int: ...
    @score.setter
    def score(self, value: int) -> None: ...
    @property
    def channel(self) -> AbstractChannel: ...
    def center_print(self, msg: str) -> None: ...
    def tell(self, msg: str, **kwargs: str) -> None: ...
    def kick(self, reason: str = ...) -> None: ...
    def ban(self) -> None: ...
    def tempban(self) -> None: ...
    def addadmin(self) -> None: ...
    def addmod(self) -> None: ...
    def demote(self) -> None: ...
    def mute(self) -> None: ...
    def unmute(self) -> None: ...
    def put(self, team: Literal["free", "red", "blue", "spectator"]) -> None: ...
    def addscore(self, score: int) -> None: ...
    def switch(self, other_player: Player) -> None: ...
    def slap(self, damage: int = ...) -> None: ...
    def slay(self) -> None: ...
    def slay_with_mod(self, mod: int) -> bool: ...

_DUMMY_USERINFO: Iterable[str]

class AbstractDummyPlayer(Player):
    def __init__(self, name: str = ...) -> None: ...
    @property
    def id(self) -> int: ...
    @property
    def steam_id(self) -> int: ...
    def update(self) -> None: ...
    @property
    def channel(self) -> AbstractChannel: ...
    def tell(self, msg: str, **kwargs: str) -> None: ...

class RconDummyPlayer(AbstractDummyPlayer):
    def __init__(self) -> None: ...
    @property
    def steam_id(self) -> int: ...
    @property
    def channel(self) -> AbstractChannel: ...
    def tell(self, msg: str, **kwargs: str) -> None: ...

# from _commands.pyi
MAX_MSG_LENGTH: int

re_color_tag: Pattern

class AbstractChannel:
    _name: str

    def __init__(self, name: str) -> None: ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    def __ne__(self, other: object) -> bool: ...
    @property
    def name(self) -> str: ...
    def reply(self, msg: str, limit: int = ..., delimiter: str = ...) -> None: ...
    def split_long_lines(
        self, msg: str, limit: int = ..., delimiter: str = ...
    ) -> list[str]: ...

class ChatChannel(AbstractChannel):
    fmt: str

    def __init__(self, name: str = ..., fmt: str = ...) -> None: ...
    @abstractmethod
    def recipients(self) -> list[int] | None: ...
    def reply(self, msg: str, limit: int = ..., delimiter: str = ...) -> None: ...

class TeamChatChannel(ChatChannel):
    team: str

    def __init__(self, team: str = ..., name: str = ..., fmt: str = ...) -> None: ...
    def recipients(self) -> list[int] | None: ...

class TellChannel(ChatChannel):
    recipient: str | int | Player

    def __init__(self, player: str | int | Player) -> None: ...
    def __repr__(self) -> str: ...
    def recipients(self) -> list[int] | None: ...

class ConsoleChannel(AbstractChannel):
    def __init__(self) -> None: ...
    def reply(self, msg: str, limit: int = ..., delimiter: str = ...) -> None: ...

class ClientCommandChannel(AbstractChannel):
    recipient: Player
    tell_channel: ChatChannel

    def __init__(self, player: Player) -> None: ...
    def __repr__(self) -> str: ...
    def reply(self, msg: str, limit: int = ..., delimiter: str = ...) -> None: ...

CHAT_CHANNEL: AbstractChannel
RED_TEAM_CHAT_CHANNEL: AbstractChannel
BLUE_TEAM_CHAT_CHANNEL: AbstractChannel
FREE_CHAT_CHANNEL: AbstractChannel
SPECTATOR_CHAT_CHANNEL: AbstractChannel
CONSOLE_CHANNEL: AbstractChannel

class Command:
    name: list[str]
    plugin: Plugin
    handler: Callable
    permission: int
    channels: list[AbstractChannel]
    exclude_channels: list[AbstractChannel]
    client_cmd_pass: bool
    client_cmd_perm: int
    prefix: bool
    usage: str

    def __init__(
        self,
        plugin: Plugin,
        name: str | Iterable[str],
        handler: Callable,
        permission: int,
        channels: Iterable[AbstractChannel] | None,
        exclude_channels: Iterable[AbstractChannel] | None,
        client_cmd_pass: bool,
        client_cmd_perm: int,
        prefix: bool,
        usage: str,
    ) -> None: ...
    def execute(
        self, player: Player, msg: str, channel: AbstractChannel
    ) -> int | None: ...
    def is_eligible_name(self, name: str) -> bool: ...
    def is_eligible_channel(self, channel: AbstractChannel) -> bool: ...
    def is_eligible_player(self, player: Player, is_client_cmd: bool) -> bool: ...

class CommandInvoker:
    _commands: tuple[
        list[Command], list[Command], list[Command], list[Command], list[Command]
    ]

    def __init__(self) -> None: ...
    @property
    def commands(self) -> list[Command]: ...
    def add_command(self, command: Command, priority: int) -> None: ...
    def remove_command(self, command: Command) -> None: ...
    def is_registered(self, command: Command) -> bool: ...
    def handle_input(
        self, player: Player, msg: str, channel: AbstractChannel
    ) -> bool: ...

COMMANDS: CommandInvoker

# from _zmq.pyi
class StatsListener:
    done: bool
    address: str
    password: str | None

    def __init__(self) -> None: ...
    def keep_receiving(self) -> None: ...
    def stop(self) -> None: ...

# from _handlers.pyi
frame_tasks: scheduler
next_frame_tasks: Queue

def handle_rcon(cmd: str) -> bool | None: ...
def handle_client_command(client_id: int, cmd: str) -> bool | str: ...
def handle_server_command(client_id: int, cmd: str) -> bool | str: ...
def handle_frame() -> bool | None: ...
def handle_new_game(is_restart: bool) -> bool | None: ...
def handle_set_configstring(index: int, value: str) -> bool | None: ...
def handle_player_connect(client_id: int, _is_bot: bool) -> bool | str | None: ...
def handle_player_loaded(client_id: int) -> bool | None: ...
def handle_player_disconnect(client_id: int, reason: str | None) -> bool | None: ...
def handle_player_spawn(client_id: int) -> bool | None: ...
def handle_kamikaze_use(client_id: int) -> bool | None: ...
def handle_kamikaze_explode(client_id: int, is_used_on_demand: bool) -> bool | None: ...
def handle_damage(
    target_id: int, attacker_id: int | None, damage: int, dflags: int, mod: int
) -> bool | None: ...
def handle_console_print(text: str | None) -> bool | str | None: ...
def redirect_print(channel: AbstractChannel) -> PrintRedirector: ...
def register_handlers() -> None: ...

class PrintRedirector:
    channel: AbstractChannel

    def __init__(self, _channel: AbstractChannel) -> None: ...
    def __enter__(self) -> None: ...
    def __exit__(
        self,
        exc_type: Type[BaseException],
        exc_value: BaseException,
        exc_traceback: TracebackType | None,
    ) -> None: ...
    def flush(self) -> None: ...
    def append(self, text: str) -> None: ...

# from _events.pyi
_re_vote: Pattern

class EventDispatcher:
    name: str
    plugins: dict[
        Plugin,
        tuple[
            Iterable[Callable],
            Iterable[Callable],
            Iterable[Callable],
            Iterable[Callable],
            Iterable[Callable],
        ],
    ]
    _args: Iterable[str] | None
    _return_value: str | bool | Iterable | None
    no_debug: Iterable[str]
    need_zmq_stats_enabled: bool

    def __init__(self) -> None: ...
    @property
    def args(self) -> Iterable[str]: ...
    @args.setter
    def args(self, value: Iterable[str]) -> None: ...
    @property
    def return_value(self) -> str | bool | Iterable | None: ...
    @return_value.setter
    def return_value(self, value: str | bool | Iterable | None) -> None: ...
    def dispatch(self, *args, **kwargs) -> str | bool | Iterable | None: ...  # type: ignore
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...
    def add_hook(self, plugin: str, handler: Callable, priority: int = ...) -> None: ...
    def remove_hook(
        self, plugin: str, handler: Callable, priority: int = ...
    ) -> None: ...

class ConsolePrintDispatcher(EventDispatcher):
    def dispatch(self, text: str) -> str | bool: ...

class CommandDispatcher(EventDispatcher):
    def dispatch(self, caller: Player, command: Command, args: str) -> None: ...

class ClientCommandDispatcher(EventDispatcher):
    def dispatch(self, player: Player, cmd: str) -> str | bool: ...

class ServerCommandDispatcher(EventDispatcher):
    def dispatch(self, player: Player | None, cmd: str) -> str | bool: ...

class FrameEventDispatcher(EventDispatcher):
    def dispatch(self) -> bool: ...

class SetConfigstringDispatcher(EventDispatcher):
    def dispatch(self, index: int, value: str) -> str | bool: ...

class ChatEventDispatcher(EventDispatcher):
    def dispatch(
        self, player: Player, msg: str, channel: AbstractChannel
    ) -> str | bool: ...

class UnloadDispatcher(EventDispatcher):
    def dispatch(self, plugin: Plugin | str) -> None: ...

class PlayerConnectDispatcher(EventDispatcher):
    def dispatch(self, player: Player) -> str | bool: ...

class PlayerLoadedDispatcher(EventDispatcher):
    def dispatch(self, player: Player) -> bool: ...

class PlayerDisconnectDispatcher(EventDispatcher):
    def dispatch(self, player: Player, reason: str | None) -> bool: ...

class PlayerSpawnDispatcher(EventDispatcher):
    def dispatch(self, player: Player) -> bool: ...

class StatsDispatcher(EventDispatcher):
    def dispatch(self, stats: StatsData) -> bool: ...

class VoteCalledDispatcher(EventDispatcher):
    def dispatch(self, player: Player, vote: str, args: str | None) -> bool: ...

class VoteStartedDispatcher(EventDispatcher):
    _caller: Player | None

    def __init__(self) -> None: ...
    def dispatch(self, vote: str, args: str | None) -> bool: ...
    def caller(self, player: Player | None) -> None: ...

class VoteEndedDispatcher(EventDispatcher):
    def dispatch(self, passed: bool) -> None: ...

class VoteDispatcher(EventDispatcher):
    def dispatch(self, player: Player, yes: bool) -> bool: ...

class GameCountdownDispatcher(EventDispatcher):
    def dispatch(self) -> bool: ...

class GameStartDispatcher(EventDispatcher):
    def dispatch(self, data: GameStartData) -> bool: ...

class GameEndDispatcher(EventDispatcher):
    def dispatch(self, data: GameEndData) -> bool: ...

class RoundCountdownDispatcher(EventDispatcher):
    def dispatch(self, round_number: int) -> bool: ...

class RoundStartDispatcher(EventDispatcher):
    def dispatch(self, round_number: int) -> bool: ...

class RoundEndDispatcher(EventDispatcher):
    def dispatch(self, data: RoundEndData) -> bool: ...

class TeamSwitchDispatcher(EventDispatcher):
    def dispatch(self, player: Player, old_team: str, new_team: str) -> bool: ...

class TeamSwitchAttemptDispatcher(EventDispatcher):
    def dispatch(self, player: Player, old_team: str, new_team: str) -> bool: ...

class MapDispatcher(EventDispatcher):
    def dispatch(self, mapname: str, factory: str) -> bool: ...

class NewGameDispatcher(EventDispatcher):
    def dispatch(self) -> bool: ...

class KillDispatcher(EventDispatcher):
    def dispatch(self, victim: Player, killer: Player, data: KillData) -> bool: ...

class DeathDispatcher(EventDispatcher):
    def dispatch(
        self, victim: Player, killer: Player | None, data: DeathData
    ) -> bool: ...

class UserinfoDispatcher(EventDispatcher):
    def dispatch(
        self, playe: Player, changed: UserinfoEventInput
    ) -> bool | UserinfoEventInput: ...

class KamikazeUseDispatcher(EventDispatcher):
    def dispatch(self, player: Player) -> bool: ...

class KamikazeExplodeDispatcher(EventDispatcher):
    def dispatch(self, player: Player, is_used_on_demand: bool) -> bool: ...

class DamageDispatcher(EventDispatcher):
    def dispatch(
        self,
        target: Player | int | None,
        attacker: Player | int | None,
        damage: int,
        dflags: int,
        means_of_death: int,
    ) -> bool: ...

class EventDispatcherManager:
    def __init__(self) -> None: ...
    @overload
    def __getitem__(self, key: Literal["console_print"]) -> ConsolePrintDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["command"]) -> CommandDispatcher: ...
    @overload
    def __getitem__(
        self, key: Literal["client_command"]
    ) -> ClientCommandDispatcher: ...
    @overload
    def __getitem__(
        self, key: Literal["server_command"]
    ) -> ServerCommandDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["frame"]) -> FrameEventDispatcher: ...
    @overload
    def __getitem__(
        self, key: Literal["set_configstring"]
    ) -> SetConfigstringDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["chat"]) -> ChatEventDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["unload"]) -> UnloadDispatcher: ...
    @overload
    def __getitem__(
        self, key: Literal["player_connect"]
    ) -> PlayerConnectDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["player_loaded"]) -> PlayerLoadedDispatcher: ...
    @overload
    def __getitem__(
        self, key: Literal["player_disconnect"]
    ) -> PlayerDisconnectDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["player_spawn"]) -> PlayerSpawnDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["stats"]) -> StatsDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["vote_called"]) -> VoteCalledDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["vote_started"]) -> VoteStartedDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["vote_ended"]) -> VoteEndedDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["vote"]) -> VoteDispatcher: ...
    @overload
    def __getitem__(
        self, key: Literal["game_countdown"]
    ) -> GameCountdownDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["game_start"]) -> GameStartDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["game_end"]) -> GameEndDispatcher: ...
    @overload
    def __getitem__(
        self, key: Literal["round_countdown"]
    ) -> RoundCountdownDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["round_start"]) -> RoundStartDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["round_end"]) -> RoundEndDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["team_switch"]) -> TeamSwitchDispatcher: ...
    @overload
    def __getitem__(
        self, key: Literal["team_switch_attempt"]
    ) -> TeamSwitchAttemptDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["map"]) -> MapDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["new_game"]) -> NewGameDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["kill"]) -> KillDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["death"]) -> DeathDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["userinfo"]) -> UserinfoDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["kamikaze_use"]) -> KamikazeUseDispatcher: ...
    @overload
    def __getitem__(
        self, key: Literal["kamikaze_explode"]
    ) -> KamikazeExplodeDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["damage"]) -> DamageDispatcher: ...
    def __contains__(self, key: str) -> bool: ...
    def add_dispatcher(self, dispatcher: Type[EventDispatcher]) -> None: ...
    def remove_dispatcher(self, dispatcher: Type[EventDispatcher]) -> None: ...
    def remove_dispatcher_by_name(self, event_name: str) -> None: ...

EVENT_DISPATCHERS: EventDispatcherManager

UncancellableEventReturn = Literal[0] | None
CancellableEventReturn = Literal[0, 1, 2, 3] | None

# from _plugin.py
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

class Plugin:
    _loaded_plugins: ClassVar[dict[str, Plugin]] = ...
    database: Type[Redis] | None = ...
    _hooks: list[tuple[str, Callable, int]]
    _commands: list[Command]
    _db_instance: Redis | None = ...

    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[str] = ...) -> str | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[bool]) -> bool | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[int]) -> int | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[float]) -> float | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[list]) -> list[str] | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[set]) -> set[str] | None: ...
    @classmethod
    @overload
    def get_cvar(
            cls, name: str, return_type: Type[tuple]
    ) -> tuple[str, ...] | None: ...
    @classmethod
    def set_cvar(
            cls,
            name: str,
            value: str | bool | int | float | list | set | tuple,
            flags: int = ...,
    ) -> bool: ...
    @classmethod
    def set_cvar_limit(
            cls,
            name: str,
            value: int | float,
            minimum: int | float,
            maximum: int | float,
            flags: int = ...,
    ) -> bool: ...
    @classmethod
    def set_cvar_once(
            cls,
            name: str,
            value: str | bool | int | float | list | set | tuple,
            flags: int = ...,
    ) -> bool: ...
    @classmethod
    def set_cvar_limit_once(
            cls,
            name: str,
            value: int | float,
            minimum: int | float,
            maximum: int | float,
            flags: int = ...,
    ) -> bool: ...
    @classmethod
    def players(cls) -> list[Player]: ...
    @classmethod
    def player(
            cls, name: str | int | Player, player_list: Iterable[Player] | None = ...
    ) -> Player | None: ...
    @classmethod
    def msg(cls, msg: str, chat_channel: str = ..., **kwargs: str) -> None: ...
    @classmethod
    def console(cls, text: str) -> None: ...
    @classmethod
    def clean_text(cls, text: str) -> str: ...
    @classmethod
    def colored_name(
            cls, name: str | Player, player_list: Iterable[Player] | None = ...
    ) -> str | None: ...
    @classmethod
    def client_id(
            cls, name: str | int | Player, player_list: Iterable[Player] | None = ...
    ) -> int | None: ...
    @classmethod
    def find_player(
            cls, name: str, player_list: Iterable[Player] | None = ...
    ) -> list[Player]: ...
    @classmethod
    def teams(
            cls, player_list: Iterable[Player] | None = ...
    ) -> Mapping[str, list[Player]]: ...
    @classmethod
    def center_print(
            cls, msg: str, recipient: str | int | Player | None = ...
    ) -> None: ...
    @classmethod
    def tell(cls, msg: str, recipient: str | int | Player, **kwargs: str) -> None: ...
    @classmethod
    def is_vote_active(cls) -> bool: ...
    @classmethod
    def current_vote_count(cls) -> tuple[int, int] | None: ...
    @classmethod
    def callvote(cls, vote: str, display: str, time: int = ...) -> bool: ...
    @classmethod
    def force_vote(cls, pass_it: bool) -> bool: ...
    @classmethod
    def teamsize(cls, size: int) -> None: ...
    @classmethod
    def kick(cls, player: str | int | Player, reason: str = ...) -> None: ...
    @classmethod
    def shuffle(cls) -> None: ...
    @classmethod
    def cointoss(cls) -> None: ...
    @classmethod
    def change_map(cls, new_map: str, factory: str | None = ...) -> None: ...
    @classmethod
    def switch(cls, player: Player, other_player: Player) -> None: ...
    @classmethod
    def play_sound(cls, sound_path: str, player: Player | None = ...) -> bool: ...
    @classmethod
    def play_music(cls, music_path: str, player: Player | None = ...) -> bool: ...
    @classmethod
    def stop_sound(cls, player: Player | None = ...) -> None: ...
    @classmethod
    def stop_music(cls, player: Player | None = ...) -> None: ...
    @classmethod
    def slap(cls, player: str | int | Player, damage: int = ...) -> None: ...
    @classmethod
    def slay(cls, player: str | int | Player) -> None: ...
    @classmethod
    def timeout(cls) -> None: ...
    @classmethod
    def timein(cls) -> None: ...
    @classmethod
    def allready(cls) -> None: ...
    @classmethod
    def pause(cls) -> None: ...
    @classmethod
    def unpause(cls) -> None: ...
    @classmethod
    def lock(cls, team: str | None = ...) -> None: ...
    @classmethod
    def unlock(cls, team: str | None = ...) -> None: ...
    @classmethod
    def put(cls, player: Player, team: str) -> None: ...
    @classmethod
    def mute(cls, player: Player) -> None: ...
    @classmethod
    def unmute(cls, player: Player) -> None: ...
    @classmethod
    def tempban(cls, player: Player) -> None: ...
    @classmethod
    def ban(cls, player: Player) -> None: ...
    @classmethod
    def unban(cls, player: Player) -> None: ...
    @classmethod
    def opsay(cls, msg: str) -> None: ...
    @classmethod
    def addadmin(cls, player: Player) -> None: ...
    @classmethod
    def addmod(cls, player: Player) -> None: ...
    @classmethod
    def demote(cls, player: Player) -> None: ...
    @classmethod
    def abort(cls) -> None: ...
    @classmethod
    def addscore(cls, player: Player, score: int) -> None: ...
    @classmethod
    def addteamscore(cls, team: str, score: int) -> None: ...
    @classmethod
    def setmatchtime(cls, time: int) -> None: ...
    def __init__(self) -> None: ...
    def __str__(self) -> str: ...
    @property
    def db(self) -> Redis | None: ...
    @property
    def name(self) -> str: ...
    @property
    def plugins(self) -> Mapping[str, Plugin]: ...
    @property
    def hooks(self) -> Iterable[tuple[str, Callable, int]]: ...
    @property
    def commands(self) -> Iterable[Command]: ...
    @property
    def game(self) -> Game | None: ...
    @property
    def logger(self) -> Logger: ...
    @overload
    def add_hook(
            self,
            event: Literal["console_print"],
            handler: Callable[
                [str],
                str | CancellableEventReturn,
            ],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["command"],
            handler: Callable[
                [Player, Command, str],
                CancellableEventReturn,
            ],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["client_command"],
            handler: Callable[[Player | None, str], str | bool | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["server_command"],
            handler: Callable[[Player | None, str], str | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["frame"],
            handler: Callable[[], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["set_configstring"],
            handler: Callable[[int, str], str | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["chat"],
            handler: Callable[[Player, str, AbstractChannel], str | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["unload"],
            handler: Callable[[Plugin], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["player_connect"],
            handler: Callable[[Player], str | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["player_loaded"],
            handler: Callable[[Player], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["player_disconnect"],
            handler: Callable[[Player, str | None], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["player_spawn"],
            handler: Callable[[Player], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["stats"],
            handler: Callable[[StatsData], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["vote_called"],
            handler: Callable[[Player, str, str | None], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["vote_started"],
            handler: Callable[[Player, str, str | None], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["vote_ended"],
            handler: Callable[
                [tuple[int, int], str, str | None, bool], CancellableEventReturn
            ],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["vote"],
            handler: Callable[[Player, bool], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["game_countdown"],
            handler: Callable[[], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["game_start"],
            handler: Callable[[GameStartData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["game_end"],
            handler: Callable[[GameEndData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["round_countdown"],
            handler: Callable[[int], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["round_start"],
            handler: Callable[[int], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["round_end"],
            handler: Callable[[RoundEndData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["team_switch"],
            handler: Callable[[Player, str, str], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["team_switch_attempt"],
            handler: Callable[[Player, str, str], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["map"],
            handler: Callable[[str, str], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["new_game"],
            handler: Callable[[], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["kill"],
            handler: Callable[[Player, Player, KillData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["death"],
            handler: Callable[[Player, Player | None, DeathData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["userinfo"],
            handler: Callable[
                [Player, UserinfoEventInput], UserInfo | CancellableEventReturn
            ],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["kamikaze_use"],
            handler: Callable[[Player], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["kamikaze_explde"],
            handler: Callable[[Player, bool], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
            self,
            event: Literal["damage"],
            handler: Callable[
                [Player | int | None, Player | int | None, int, int, int],
                UncancellableEventReturn,
            ],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["console_print"],
            handler: Callable[
                [str],
                str | CancellableEventReturn,
            ],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["command"],
            handler: Callable[
                [Player, Command, str],
                CancellableEventReturn,
            ],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["client_command"],
            handler: Callable[[Player | None, str], str | bool | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["server_command"],
            handler: Callable[[Player | None, str], str | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["frame"],
            handler: Callable[[], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["set_configstring"],
            handler: Callable[[int, str], str | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["chat"],
            handler: Callable[[Player, str, AbstractChannel], str | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["unload"],
            handler: Callable[[Plugin], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["player_connect"],
            handler: Callable[[Player], str | CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["player_loaded"],
            handler: Callable[[Player], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["player_disconnect"],
            handler: Callable[[Player, str | None], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["player_spawn"],
            handler: Callable[[Player], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["stats"],
            handler: Callable[[StatsData], UncancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["vote_called"],
            handler: Callable[[Player, str, str | None], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["vote_started"],
            handler: Callable[[Player, str, str | None], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["vote_ended"],
            handler: Callable[
                [tuple[int, int], str, str | None, bool], CancellableEventReturn
            ],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["vote"],
            handler: Callable[[Player, bool], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["game_countdown"],
            handler: Callable[[], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["game_start"],
            handler: Callable[[GameStartData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["game_end"],
            handler: Callable[[GameEndData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["round_countdown"],
            handler: Callable[[int], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["round_start"],
            handler: Callable[[int], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["round_end"],
            handler: Callable[[RoundEndData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["team_switch"],
            handler: Callable[[Player, str, str], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["team_switch_attempt"],
            handler: Callable[[Player, str, str], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["map"],
            handler: Callable[[str, str], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["new_game"],
            handler: Callable[[], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["kill"],
            handler: Callable[[Player, Player, KillData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["death"],
            handler: Callable[[Player, Player | None, DeathData], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["userinfo"],
            handler: Callable[
                [Player, UserinfoEventInput], UserInfo | CancellableEventReturn
            ],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["kamikaze_use"],
            handler: Callable[[Player], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["kamikaze_explde"],
            handler: Callable[[Player, bool], CancellableEventReturn],
            priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
            self,
            event: Literal["damage"],
            handler: Callable[
                [Player | int | None, Player | int | None, int, int, int],
                UncancellableEventReturn,
            ],
            priority: int = ...,
    ) -> None: ...
    def add_command(
            self,
            name: str | Iterable[str],
            handler: Callable[[Player, str | list[str], AbstractChannel], CancellableEventReturn],
            permission: int = ...,
            channels: Iterable[AbstractChannel] | None = ...,
            exclude_channels: Iterable[AbstractChannel] = ...,
            priority: int = ...,
            client_cmd_pass: bool = ...,
            client_cmd_perm: int = ...,
            prefix: bool = ...,
            usage: str = ...,
    ) -> None: ...
    def remove_command(
            self,
            name: Iterable[str],
            handler: Callable[[Player, str, AbstractChannel], CancellableEventReturn],
    ) -> None: ...
