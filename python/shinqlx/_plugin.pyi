from typing import TYPE_CHECKING, TypeVar, overload

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping
    from logging import Logger
    from typing import Callable, ClassVar, Literal

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

_PluginT = TypeVar("_PluginT", bound="Plugin")

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
    event: Literal["console_print"], priority: int | None = ...
) -> Callable[[_ConsolePrintHookMethod[_PluginT]], _ConsolePrintHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["command"], priority: int | None = ...
) -> Callable[[_CommandHookMethod[_PluginT]], _CommandHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["client_command"], priority: int | None = ...
) -> Callable[[_ClientCommandHookMethod[_PluginT]], _ClientCommandHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["server_command"], priority: int | None = ...
) -> Callable[[_ServerCommandHookMethod[_PluginT]], _ServerCommandHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["frame"], priority: int | None = ...
) -> Callable[[_FrameHookMethod[_PluginT]], _FrameHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["set_configstring"], priority: int | None = ...
) -> Callable[[_SetConfigstringHookMethod[_PluginT]], _SetConfigstringHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["chat"], priority: int | None = ...
) -> Callable[[_ChatHookMethod[_PluginT]], _ChatHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["unload"], priority: int | None = ...
) -> Callable[[_UnloadHookMethod[_PluginT]], _UnloadHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["player_connect"], priority: int | None = ...
) -> Callable[[_PlayerConnectHookMethod[_PluginT]], _PlayerConnectHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["player_loaded"], priority: int | None = ...
) -> Callable[[_PlayerLoadedHookMethod[_PluginT]], _PlayerLoadedHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["player_disconnect"], priority: int | None = ...
) -> Callable[[_PlayerDisconnectHookMethod[_PluginT]], _PlayerDisconnectHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["player_spawn"], priority: int | None = ...
) -> Callable[[_PlayerSpawnHookMethod[_PluginT]], _PlayerSpawnHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["stats"], priority: int | None = ...
) -> Callable[[_StatsHookMethod[_PluginT]], _StatsHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["vote_called"], priority: int | None = ...
) -> Callable[[_VoteCalledHookMethod[_PluginT]], _VoteCalledHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["vote_started"], priority: int | None = ...
) -> Callable[[_VoteStartedHookMethod[_PluginT]], _VoteStartedHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["vote_ended"], priority: int | None = ...
) -> Callable[[_VoteEndedHookMethod[_PluginT]], _VoteEndedHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["vote"], priority: int | None = ...
) -> Callable[[_VoteHookMethod[_PluginT]], _VoteHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["game_countdown"], priority: int | None = ...
) -> Callable[[_GameCountdownHookMethod[_PluginT]], _GameCountdownHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["game_start"], priority: int | None = ...
) -> Callable[[_GameStartHookMethod[_PluginT]], _GameStartHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["game_end"], priority: int | None = ...
) -> Callable[[_GameEndHookMethod[_PluginT]], _GameEndHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["round_countdown"], priority: int | None = ...
) -> Callable[[_RoundCountdownHookMethod[_PluginT]], _RoundCountdownHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["round_start"], priority: int | None = ...
) -> Callable[[_RoundStartHookMethod[_PluginT]], _RoundStartHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["round_end"], priority: int | None = ...
) -> Callable[[_RoundEndHookMethod[_PluginT]], _RoundEndHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["team_switch"], priority: int | None = ...
) -> Callable[[_TeamSwitchHookMethod[_PluginT]], _TeamSwitchHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["team_switch_attempt"], priority: int | None = ...
) -> Callable[[_TeamSwitchAttemptHookMethod[_PluginT]], _TeamSwitchAttemptHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["map"], priority: int | None = ...
) -> Callable[[_MapHookMethod[_PluginT]], _MapHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["new_game"], priority: int | None = ...
) -> Callable[[_NewGameHookMethod[_PluginT]], _NewGameHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["kill"], priority: int | None = ...
) -> Callable[[_KillHookMethod[_PluginT]], _KillHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["death"], priority: int | None = ...
) -> Callable[[_DeathHookMethod[_PluginT]], _DeathHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["userinfo"], priority: int | None = ...
) -> Callable[[_UserinfoHookMethod[_PluginT]], _UserinfoHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["kamikaze_use"], priority: int | None = ...
) -> Callable[[_KamikazeUseHookMethod[_PluginT]], _KamikazeUseHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["kamikaze_explode"], priority: int | None = ...
) -> Callable[[_KamikazeExplodeHookMethod[_PluginT]], _KamikazeExplodeHookMethod[_PluginT]]: ...
@overload
def hook(
    event: Literal["damage"], priority: int | None = ...
) -> Callable[[_DamageHookMethod[_PluginT]], _DamageHookMethod[_PluginT]]: ...
def command(
    name: str | Iterable[str],
    *,
    permission: int = ...,
    channels: Iterable[AbstractChannel] | None = ...,
    exclude_channels: Iterable[AbstractChannel] = ...,
    priority: int = ...,
    client_cmd_pass: bool = ...,
    client_cmd_perm: int = ...,
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
        event: Literal["console_print"],
        handler: _ConsolePrintHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["command"],
        handler: _CommandHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["client_command"],
        handler: _ClientCommandHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["server_command"],
        handler: _ServerCommandHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["frame"],
        handler: _FrameHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["set_configstring"],
        handler: _SetConfigstringHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["chat"],
        handler: _ChatHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["unload"],
        handler: _UnloadHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_connect"],
        handler: _PlayerConnectHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_loaded"],
        handler: _PlayerLoadedHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_disconnect"],
        handler: _PlayerDisconnectHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_spawn"],
        handler: _PlayerSpawnHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["stats"],
        handler: _StatsHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote_called"],
        handler: _VoteCalledHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote_started"],
        handler: _VoteStartedHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote_ended"],
        handler: _VoteEndedHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote"],
        handler: _VoteHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["game_countdown"],
        handler: _GameCountdownHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["game_start"],
        handler: _GameStartHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["game_end"],
        handler: _GameEndHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["round_countdown"],
        handler: _RoundCountdownHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["round_start"],
        handler: _RoundStartHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["round_end"],
        handler: _RoundEndHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["team_switch"],
        handler: _TeamSwitchHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["team_switch_attempt"],
        handler: _TeamSwitchAttemptHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["map"],
        handler: _MapHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["new_game"],
        handler: _NewGameHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["kill"],
        handler: _KillHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["death"],
        handler: _DeathHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["userinfo"],
        handler: _UserinfoHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["kamikaze_use"],
        handler: _KamikazeUseHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["kamikaze_explode"],
        handler: _KamikazeExplodeHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["damage"],
        handler: _DamageHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["console_print"],
        handler: _ConsolePrintHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["command"],
        handler: _CommandHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["client_command"],
        handler: _ClientCommandHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["server_command"],
        handler: _ServerCommandHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["frame"],
        handler: _FrameHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["set_configstring"],
        handler: _SetConfigstringHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["chat"],
        handler: _ChatHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["unload"],
        handler: _UnloadHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_connect"],
        handler: _PlayerConnectHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_loaded"],
        handler: _PlayerLoadedHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_disconnect"],
        handler: _PlayerDisconnectHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_spawn"],
        handler: _PlayerSpawnHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["stats"],
        handler: _StatsHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote_called"],
        handler: _VoteCalledHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote_started"],
        handler: _VoteStartedHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote_ended"],
        handler: _VoteEndedHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote"],
        handler: _VoteHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["game_countdown"],
        handler: _GameCountdownHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["game_start"],
        handler: _GameStartHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["game_end"],
        handler: _GameEndHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["round_countdown"],
        handler: _RoundCountdownHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["round_start"],
        handler: _RoundStartHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["round_end"],
        handler: _RoundEndHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["team_switch"],
        handler: _TeamSwitchHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["team_switch_attempt"],
        handler: _TeamSwitchAttemptHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["map"],
        handler: _MapHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["new_game"],
        handler: _NewGameHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["kill"],
        handler: _KillHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["death"],
        handler: _DeathHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["userinfo"],
        handler: _UserinfoHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["kamikaze_use"],
        handler: _KamikazeUseHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["kamikaze_explode"],
        handler: _KamikazeExplodeHookHandler,
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["damage"],
        handler: _DamageHookHandler,
        priority: int = ...,
    ) -> None: ...
    def add_command(
        self,
        name: str | Iterable[str],
        handler: _CommandHandler,
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
        handler: _CommandHandler,
    ) -> None: ...
