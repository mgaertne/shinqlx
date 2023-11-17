from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Callable, Any, Iterable, Mapping
    from shinqlx import Player

__version__: str
_map_title: str | None
_map_subtitle1: str | None
_map_subtitle2: str | None
_thread_count: int
_thread_name: str

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

# classes
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
def register_handler(
    _event: str, _handler: Callable[[Any], Any] | None = ...
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
def set_cvar_once(name: str, value: str, flags: int = ...) -> bool: ...
def set_cvar_limit_once(
        name: str,
        value: int | float,
        minimum: int | float,
        maximum: int | float,
        flags: int = ...,
) -> bool: ...
def set_map_subtitles() -> None: ...
def next_frame(func: Callable) -> Callable: ...
def delay(time: float) -> Callable: ...
def thread(func: Callable, force: bool = ...) -> Callable: ...

class PluginLoadError(Exception): ...
class PluginUnloadError(Exception): ...
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