from typing import TYPE_CHECKING, Generic, SupportsIndex, TypeVar, overload

if TYPE_CHECKING:
    import builtins
    from collections.abc import Iterable, Mapping
    from logging import Logger
    from typing import Any, Callable, ClassVar, Literal, NoReturn

    from shinqlx import (
        AbstractChannel,
        CancellableEventReturn,
        Command,
        DeathData,
        Game,
        GameEndData,
        GameStartData,
        KillData,
        Player,
        RoundEndData,
        StatsData,
        UncancellableEventReturn,
        UserInfo,
        UserinfoEventInput,
    )
    from shinqlx.database import Redis

    from ._shinqlx import Events, Teams

_PluginT = TypeVar("_PluginT", bound="Plugin")
_CvarT = TypeVar("_CvarT")

_ConsolePrintHookHandler = Callable[[str], str | CancellableEventReturn]
_ConsolePrintHookMethod = Callable[[_PluginT, str], str | CancellableEventReturn]
_CommandHookHandler = Callable[[Player, Command, str], CancellableEventReturn]
_CommandHookMethod = Callable[[_PluginT, Player, Command, str], CancellableEventReturn]
_ClientCommandHookHandler = Callable[[Player | None, str], str | bool | CancellableEventReturn]
_ClientCommandHookMethod = Callable[[_PluginT, Player | None, str], str | bool | CancellableEventReturn]
_ServerCommandHookHandler = Callable[[Player | None, str], str | CancellableEventReturn]
_ServerCommandHookMethod = Callable[[_PluginT, Player | None, str], str | CancellableEventReturn]
_FrameHookHandler = Callable[[], UncancellableEventReturn]
_FrameHookMethod = Callable[[_PluginT], UncancellableEventReturn]
_SetConfigstringHookHandler = Callable[[int, str], str | CancellableEventReturn]
_SetConfigstringHookMethod = Callable[[_PluginT, int, str], str | CancellableEventReturn]
_ChatHookHandler = Callable[[Player, str, AbstractChannel], str | CancellableEventReturn]
_ChatHookMethod = Callable[[_PluginT, Player, str, AbstractChannel], str | CancellableEventReturn]
_UnloadHookHandler = Callable[[Plugin], UncancellableEventReturn]
_UnloadHookMethod = Callable[[_PluginT, Plugin], UncancellableEventReturn]
_PlayerConnectHookHandler = Callable[[Player], str | CancellableEventReturn]
_PlayerConnectHookMethod = Callable[[_PluginT, Player], str | CancellableEventReturn]
_PlayerLoadedHookHandler = Callable[[Player], UncancellableEventReturn]
_PlayerLoadedHookMethod = Callable[[_PluginT, Player], UncancellableEventReturn]
_PlayerDisconnectHookHandler = Callable[[Player, str | None], UncancellableEventReturn]
_PlayerDisconnectHookMethod = Callable[[_PluginT, Player, str | None], UncancellableEventReturn]
_PlayerSpawnHookHandler = Callable[[Player], UncancellableEventReturn]
_PlayerSpawnHookMethod = Callable[[_PluginT, Player], UncancellableEventReturn]
_StatsHookHandler = Callable[[StatsData], UncancellableEventReturn]
_StatsHookMethod = Callable[[_PluginT, StatsData], UncancellableEventReturn]
_VoteCalledHookHandler = Callable[[Player, str, str | None], CancellableEventReturn]
_VoteCalledHookMethod = Callable[[_PluginT, Player, str, str | None], CancellableEventReturn]
_VoteStartedHookHandler = Callable[[Player, str, str | None], CancellableEventReturn]
_VoteStartedHookMethod = Callable[[_PluginT, Player, str, str | None], CancellableEventReturn]
_VoteEndedHookHandler = Callable[[tuple[int, int], str, str | None, bool], CancellableEventReturn]
_VoteEndedHookMethod = Callable[[_PluginT, tuple[int, int], str, str | None, bool], CancellableEventReturn]
_VoteHookHandler = Callable[[Player, bool], CancellableEventReturn]
_VoteHookMethod = Callable[[_PluginT, Player, bool], CancellableEventReturn]
_GameCountdownHookHandler = Callable[[], CancellableEventReturn]
_GameCountdownHookMethod = Callable[[_PluginT], CancellableEventReturn]
_GameStartHookHandler = Callable[[GameStartData], CancellableEventReturn]
_GameStartHookMethod = Callable[[_PluginT, GameStartData], CancellableEventReturn]
_GameEndHookHandler = Callable[[GameEndData], CancellableEventReturn]
_GameEndHookMethod = Callable[[_PluginT, GameEndData], CancellableEventReturn]
_RoundCountdownHookHandler = Callable[[int], CancellableEventReturn]
_RoundCountdownHookMethod = Callable[[_PluginT, int], CancellableEventReturn]
_RoundStartHookHandler = Callable[[int], CancellableEventReturn]
_RoundStartHookMethod = Callable[[_PluginT, int], CancellableEventReturn]
_RoundEndHookHandler = Callable[[RoundEndData], CancellableEventReturn]
_RoundEndHookMethod = Callable[[_PluginT, RoundEndData], CancellableEventReturn]
_TeamSwitchHookHandler = Callable[[Player, str, str], CancellableEventReturn]
_TeamSwitchHookMethod = Callable[[_PluginT, Player, str, str], CancellableEventReturn]
_TeamSwitchAttemptHookHandler = Callable[[Player, str, str], CancellableEventReturn]
_TeamSwitchAttemptHookMethod = Callable[[_PluginT, Player, str, str], CancellableEventReturn]
_MapHookHandler = Callable[[str, str], CancellableEventReturn]
_MapHookMethod = Callable[[_PluginT, str, str], CancellableEventReturn]
_NewGameHookHandler = Callable[[], CancellableEventReturn]
_NewGameHookMethod = Callable[[_PluginT], CancellableEventReturn]
_KillHookHandler = Callable[[Player, Player, KillData], CancellableEventReturn]
_KillHookMethod = Callable[[_PluginT, Player, Player, KillData], CancellableEventReturn]
_DeathHookHandler = Callable[[Player, Player | None, DeathData], CancellableEventReturn]
_DeathHookMethod = Callable[[_PluginT, Player, Player | None, DeathData], CancellableEventReturn]
_UserinfoHookHandler = Callable[[Player, UserinfoEventInput], UserInfo | CancellableEventReturn]
_UserinfoHookMethod = Callable[[_PluginT, Player, UserinfoEventInput], UserInfo | CancellableEventReturn]
_KamikazeUseHookHandler = Callable[[Player], CancellableEventReturn]
_KamikazeUseHookMethod = Callable[[_PluginT, Player], CancellableEventReturn]
_KamikazeExplodeHookHandler = Callable[[Player, bool], CancellableEventReturn]
_KamikazeExplodeHookMethod = Callable[[_PluginT, Player, bool], CancellableEventReturn]
_DamageHookHandler = Callable[[Player | int | None, Player | int | None, int, int, int], UncancellableEventReturn]
_DamageHookMethod = Callable[
    [_PluginT, Player | int | None, Player | int | None, int, int, int], UncancellableEventReturn
]

_CommandHandler = Callable[[Player, list[str], AbstractChannel], int | None]
_CommandMethod = Callable[[_PluginT, Player, list[str], AbstractChannel], int | None]

@overload
def hook(
    event: Literal["console_print", Events.CONSOLE_PRINT], priority: SupportsIndex | None = ...
) -> Callable[[_ConsolePrintHookMethod[_PluginT]], _ConsolePrintHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["command", Events.COMMAND], priority: SupportsIndex | None = ...
) -> Callable[[_CommandHookMethod[_PluginT]], _CommandHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["client_command", Events.CLIENT_COMMAND], priority: SupportsIndex | None = ...
) -> Callable[[_ClientCommandHookMethod[_PluginT]], _ClientCommandHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["server_command", Events.SERVER_COMMAND], priority: SupportsIndex | None = ...
) -> Callable[[_ServerCommandHookMethod[_PluginT]], _ServerCommandHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["frame", Events.FRAME], priority: SupportsIndex | None = ...
) -> Callable[[_FrameHookMethod[_PluginT]], _FrameHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["set_configstring", Events.SET_CONFIGSTRING], priority: SupportsIndex | None = ...
) -> Callable[[_SetConfigstringHookMethod[_PluginT]], _SetConfigstringHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["chat", Events.CHAT], priority: SupportsIndex | None = ...
) -> Callable[[_ChatHookMethod[_PluginT]], _ChatHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["unload", Events.UNLOAD], priority: SupportsIndex | None = ...
) -> Callable[[_UnloadHookMethod[_PluginT]], _UnloadHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["player_connect", Events.PLAYER_CONNECT], priority: SupportsIndex | None = ...
) -> Callable[[_PlayerConnectHookMethod[_PluginT]], _PlayerConnectHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["player_loaded", Events.PLAYER_LOADED], priority: SupportsIndex | None = ...
) -> Callable[[_PlayerLoadedHookMethod[_PluginT]], _PlayerLoadedHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["player_disconnect", Events.PLAYER_DISCONNECT], priority: SupportsIndex | None = ...
) -> Callable[[_PlayerDisconnectHookMethod[_PluginT]], _PlayerDisconnectHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["player_spawn", Events.PLAYER_SPAWN], priority: SupportsIndex | None = ...
) -> Callable[[_PlayerSpawnHookMethod[_PluginT]], _PlayerSpawnHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["stats", Events.STATS], priority: SupportsIndex | None = ...
) -> Callable[[_StatsHookMethod[_PluginT]], _StatsHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["vote_called", Events.VOTE_CALLED], priority: SupportsIndex | None = ...
) -> Callable[[_VoteCalledHookMethod[_PluginT]], _VoteCalledHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["vote_started", Events.VOTE_STARTED], priority: SupportsIndex | None = ...
) -> Callable[[_VoteStartedHookMethod[_PluginT]], _VoteStartedHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["vote_ended", Events.VOTE_ENDED], priority: SupportsIndex | None = ...
) -> Callable[[_VoteEndedHookMethod[_PluginT]], _VoteEndedHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["vote", Events.VOTE], priority: SupportsIndex | None = ...
) -> Callable[[_VoteHookMethod[_PluginT]], _VoteHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["game_countdown", Events.GAME_COUNTDOWN], priority: SupportsIndex | None = ...
) -> Callable[[_GameCountdownHookMethod[_PluginT]], _GameCountdownHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["game_start", Events.GAME_START], priority: SupportsIndex | None = ...
) -> Callable[[_GameStartHookMethod[_PluginT]], _GameStartHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["game_end", Events.GAME_END], priority: SupportsIndex | None = ...
) -> Callable[[_GameEndHookMethod[_PluginT]], _GameEndHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["round_countdown", Events.ROUND_COUNTDOWN], priority: SupportsIndex | None = ...
) -> Callable[[_RoundCountdownHookMethod[_PluginT]], _RoundCountdownHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["round_start", Events.ROUND_START], priority: SupportsIndex | None = ...
) -> Callable[[_RoundStartHookMethod[_PluginT]], _RoundStartHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["round_end", Events.ROUND_END], priority: SupportsIndex | None = ...
) -> Callable[[_RoundEndHookMethod[_PluginT]], _RoundEndHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["team_switch", Events.TEAM_SWITCH], priority: SupportsIndex | None = ...
) -> Callable[[_TeamSwitchHookMethod[_PluginT]], _TeamSwitchHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["team_switch_attempt", Events.TEAM_SWITCH_ATTEMPT], priority: SupportsIndex | None = ...
) -> Callable[[_TeamSwitchAttemptHookMethod[_PluginT]], _TeamSwitchAttemptHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["map", Events.MAP], priority: SupportsIndex | None = ...
) -> Callable[[_MapHookMethod[_PluginT]], _MapHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["new_game", Events.NEW_GAME], priority: SupportsIndex | None = ...
) -> Callable[[_NewGameHookMethod[_PluginT]], _NewGameHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["kill", Events.KILL], priority: SupportsIndex | None = ...
) -> Callable[[_KillHookMethod[_PluginT]], _KillHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["death", Events.DEATH], priority: SupportsIndex | None = ...
) -> Callable[[_DeathHookMethod[_PluginT]], _DeathHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["userinfo", Events.USERINFO], priority: SupportsIndex | None = ...
) -> Callable[[_UserinfoHookMethod[_PluginT]], _UserinfoHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["kamikaze_use", Events.KAMIKAZE_USE], priority: SupportsIndex | None = ...
) -> Callable[[_KamikazeUseHookMethod[_PluginT]], _KamikazeUseHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["kamikaze_explode", Events.KAMIKAZE_EXPLODE], priority: SupportsIndex | None = ...
) -> Callable[[_KamikazeExplodeHookMethod[_PluginT]], _KamikazeExplodeHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["damage", Events.DAMAGE], priority: SupportsIndex | None = ...
) -> Callable[[_DamageHookMethod[_PluginT]], _DamageHookMethod[_PluginT]]: ...

class Cvar(Generic[_CvarT]):
    """Declares a cvar of a plugin as a read-only attribute of the type ``_CvarT``."""

    @overload
    def __init__(
        self: Cvar[str],
        name: str,
        default: str | None = ...,
        *,
        type: None = ...,
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[bool],
        name: str,
        default: bool,
        *,
        type: None = ...,
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[int],
        name: str,
        default: int,
        *,
        type: None = ...,
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[float],
        name: str,
        default: float,
        *,
        type: None = ...,
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[str],
        name: str,
        default: object = ...,
        *,
        type: builtins.type[str],
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[int],
        name: str,
        default: object = ...,
        *,
        type: builtins.type[int],
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[float],
        name: str,
        default: object = ...,
        *,
        type: builtins.type[float],
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[bool],
        name: str,
        default: object = ...,
        *,
        type: builtins.type[bool],
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[list[str]],
        name: str,
        default: object = ...,
        *,
        type: builtins.type[list[Any]],
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[set[str]],
        name: str,
        default: object = ...,
        *,
        type: builtins.type[set[Any]],
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __init__(
        self: Cvar[tuple[str, ...]],
        name: str,
        default: object = ...,
        *,
        type: builtins.type[tuple[Any, ...]],
        flags: SupportsIndex = ...,
        minimum: float | None = ...,
        maximum: float | None = ...,
    ) -> None: ...
    @overload
    def __get__(self, instance: None, owner: builtins.type[object]) -> Cvar[_CvarT]: ...
    @overload
    def __get__(self, instance: object, owner: builtins.type[object]) -> _CvarT: ...
    def __set__(self, instance: object, value: NoReturn) -> None: ...

def command(
    name: str | Iterable[str],
    *,
    permission: SupportsIndex = ...,
    channels: Iterable[AbstractChannel] | None = ...,
    exclude_channels: Iterable[AbstractChannel] = ...,
    priority: SupportsIndex = ...,
    client_cmd_pass: bool = ...,
    client_cmd_perm: SupportsIndex = ...,
    prefix: bool = ...,
    usage: str = ...,
) -> Callable[[_CommandMethod[_PluginT]], _CommandMethod[_PluginT]]: ...

class Plugin:
    _loaded_plugins: ClassVar[dict[str, Plugin]] = ...
    database: ClassVar[type[Redis] | None] = ...

    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: type[str] = ...) -> str | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: type[bool]) -> bool | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: type[int]) -> int | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: type[float]) -> float | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: type[list]) -> list[str] | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: type[set]) -> set[str] | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: type[tuple]) -> tuple[str, ...] | None: ...
    @classmethod
    def set_cvar(
        cls,
        name: str,
        value: str | bool | int | float | list | set | tuple,
        flags: SupportsIndex = ...,
    ) -> bool: ...
    @classmethod
    def set_cvar_limit(
        cls,
        name: str,
        value: int | float,
        minimum: int | float,
        maximum: int | float,
        flags: SupportsIndex = ...,
    ) -> bool: ...
    @classmethod
    def set_cvar_once(
        cls,
        name: str,
        value: str | bool | int | float | list | set | tuple,
        flags: SupportsIndex = ...,
    ) -> bool: ...
    @classmethod
    def set_cvar_limit_once(
        cls,
        name: str,
        value: int | float,
        minimum: int | float,
        maximum: int | float,
        flags: SupportsIndex = ...,
    ) -> bool: ...
    @classmethod
    def players(cls) -> list[Player]: ...
    @classmethod
    def player(cls, name: str | int | Player, player_list: Iterable[Player] | None = ...) -> Player | None: ...
    @classmethod
    def msg(cls, msg: str, chat_channel: str = ..., **kwargs: str) -> None: ...
    @classmethod
    def console(cls, text: str) -> None: ...
    @classmethod
    def clean_text(cls, text: str) -> str: ...
    @classmethod
    def colored_name(cls, name: str | Player, player_list: Iterable[Player] | None = ...) -> str | None: ...
    @classmethod
    def client_id(cls, name: str | int | Player, player_list: Iterable[Player] | None = ...) -> int | None: ...
    @classmethod
    def find_player(cls, name: str, player_list: Iterable[Player] | None = ...) -> list[Player]: ...
    @classmethod
    def find_by_name_or_id(
        cls,
        player: Player | None,
        target: str,
        player_list: Iterable[Player] | None = ...,
    ) -> Player | None: ...
    @classmethod
    def teams(cls, player_list: Iterable[Player] | None = ...) -> Mapping[str, list[Player]]: ...
    @classmethod
    def center_print(cls, msg: str, recipient: str | int | Player | None = ...) -> None: ...
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
    def lock(cls, team: str | Teams | None = ...) -> None: ...
    @classmethod
    def unlock(cls, team: str | Teams | None = ...) -> None: ...
    @classmethod
    def put(cls, player: Player, team: str | Teams) -> None: ...
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
    def addteamscore(cls, team: str | Teams, score: int) -> None: ...
    @classmethod
    def setmatchtime(cls, time: int) -> None: ...
    _hooks: list[tuple[str, Callable, int]]
    _commands: list[Command]
    _db_instance: Redis | None

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
        event: Literal["console_print", Events.CONSOLE_PRINT],
        handler: _ConsolePrintHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["command", Events.COMMAND],
        handler: _CommandHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["client_command", Events.CLIENT_COMMAND],
        handler: _ClientCommandHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["server_command", Events.SERVER_COMMAND],
        handler: _ServerCommandHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["frame", Events.FRAME],
        handler: _FrameHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["set_configstring", Events.SET_CONFIGSTRING],
        handler: _SetConfigstringHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["chat", Events.CHAT],
        handler: _ChatHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["unload", Events.UNLOAD],
        handler: _UnloadHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_connect", Events.PLAYER_CONNECT],
        handler: _PlayerConnectHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_loaded", Events.PLAYER_LOADED],
        handler: _PlayerLoadedHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_disconnect", Events.PLAYER_DISCONNECT],
        handler: _PlayerDisconnectHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_spawn", Events.PLAYER_SPAWN],
        handler: _PlayerSpawnHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["stats", Events.STATS],
        handler: _StatsHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote_called", Events.VOTE_CALLED],
        handler: _VoteCalledHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote_started", Events.VOTE_STARTED],
        handler: _VoteStartedHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote_ended", Events.VOTE_ENDED],
        handler: _VoteEndedHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote", Events.VOTE],
        handler: _VoteHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["game_countdown", Events.GAME_COUNTDOWN],
        handler: _GameCountdownHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["game_start", Events.GAME_START],
        handler: _GameStartHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["game_end", Events.GAME_END],
        handler: _GameEndHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["round_countdown", Events.ROUND_COUNTDOWN],
        handler: _RoundCountdownHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["round_start", Events.ROUND_START],
        handler: _RoundStartHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["round_end", Events.ROUND_END],
        handler: _RoundEndHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["team_switch", Events.TEAM_SWITCH],
        handler: _TeamSwitchHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["team_switch_attempt", Events.TEAM_SWITCH_ATTEMPT],
        handler: _TeamSwitchAttemptHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["map", Events.MAP],
        handler: _MapHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["new_game", Events.NEW_GAME],
        handler: _NewGameHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["kill", Events.KILL],
        handler: _KillHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["death", Events.DEATH],
        handler: _DeathHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["userinfo", Events.USERINFO],
        handler: _UserinfoHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["kamikaze_use", Events.KAMIKAZE_USE],
        handler: _KamikazeUseHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["kamikaze_explode", Events.KAMIKAZE_EXPLODE],
        handler: _KamikazeExplodeHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["damage", Events.DAMAGE],
        handler: _DamageHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["console_print", Events.CONSOLE_PRINT],
        handler: _ConsolePrintHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["command", Events.COMMAND],
        handler: _CommandHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["client_command", Events.CLIENT_COMMAND],
        handler: _ClientCommandHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["server_command", Events.SERVER_COMMAND],
        handler: _ServerCommandHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["frame", Events.FRAME],
        handler: _FrameHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["set_configstring", Events.SET_CONFIGSTRING],
        handler: _SetConfigstringHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["chat", Events.CHAT],
        handler: _ChatHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["unload", Events.UNLOAD],
        handler: _UnloadHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_connect", Events.PLAYER_CONNECT],
        handler: _PlayerConnectHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_loaded", Events.PLAYER_LOADED],
        handler: _PlayerLoadedHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_disconnect", Events.PLAYER_DISCONNECT],
        handler: _PlayerDisconnectHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_spawn", Events.PLAYER_SPAWN],
        handler: _PlayerSpawnHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["stats", Events.STATS],
        handler: _StatsHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote_called", Events.VOTE_CALLED],
        handler: _VoteCalledHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote_started", Events.VOTE_STARTED],
        handler: _VoteStartedHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote_ended", Events.VOTE_ENDED],
        handler: _VoteEndedHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote", Events.VOTE],
        handler: _VoteHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["game_countdown", Events.GAME_COUNTDOWN],
        handler: _GameCountdownHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["game_start", Events.GAME_START],
        handler: _GameStartHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["game_end", Events.GAME_END],
        handler: _GameEndHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["round_countdown", Events.ROUND_COUNTDOWN],
        handler: _RoundCountdownHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["round_start", Events.ROUND_START],
        handler: _RoundStartHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["round_end", Events.ROUND_END],
        handler: _RoundEndHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["team_switch", Events.TEAM_SWITCH],
        handler: _TeamSwitchHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["team_switch_attempt", Events.TEAM_SWITCH_ATTEMPT],
        handler: _TeamSwitchAttemptHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["map", Events.MAP],
        handler: _MapHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["new_game", Events.NEW_GAME],
        handler: _NewGameHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["kill", Events.KILL],
        handler: _KillHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["death", Events.DEATH],
        handler: _DeathHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["userinfo", Events.USERINFO],
        handler: _UserinfoHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["kamikaze_use", Events.KAMIKAZE_USE],
        handler: _KamikazeUseHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["kamikaze_explode", Events.KAMIKAZE_EXPLODE],
        handler: _KamikazeExplodeHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["damage", Events.DAMAGE],
        handler: _DamageHookHandler,
        priority: SupportsIndex = ...,
    ) -> None: ...
    def add_command(
        self,
        name: str | Iterable[str],
        handler: _CommandHandler,
        permission: SupportsIndex = ...,
        channels: Iterable[AbstractChannel] | None = ...,
        exclude_channels: Iterable[AbstractChannel] = ...,
        priority: SupportsIndex = ...,
        client_cmd_pass: bool = ...,
        client_cmd_perm: SupportsIndex = ...,
        prefix: bool = ...,
        usage: str = ...,
    ) -> None: ...
    def remove_command(
        self,
        name: Iterable[str],
        handler: _CommandHandler,
    ) -> None: ...
