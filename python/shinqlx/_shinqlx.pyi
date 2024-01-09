from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Callable, Any, Iterable

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
