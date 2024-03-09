from typing import TYPE_CHECKING, overload

if TYPE_CHECKING:
    from typing import Type, Callable, Iterable, Pattern, Mapping, Literal
    from shinqlx import (
        Plugin,
        Player,
        AbstractChannel,
        Command,
        StatsData,
        GameStartData,
        GameEndData,
        RoundEndData,
        KillData,
        DeathData,
        UserInfoEventInput,
    )

_re_vote: Pattern

UncancellableEventReturn = Literal["RET_NONE"] | None
CancellableEventReturn = Literal[0, 1, 2, 3] | None

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
    _kwargs: Mapping[str, str] | None
    _return_value: str | bool | Iterable | None
    no_debug: Iterable[str]
    need_zmq_stats_enabled: bool

    def __init__(self) -> None: ...
    @property
    def args(self) -> Iterable[str]: ...
    @args.setter
    def args(self, value: Iterable[str]) -> None: ...
    @property
    def kwargs(self) -> Mapping[str, str]: ...
    @kwargs.setter
    def kwargs(self, value: Mapping[str, str]) -> None: ...
    @property
    def return_value(self) -> str | bool | Iterable | None: ...
    @return_value.setter
    def return_value(self, value: str | bool | Iterable | None) -> None: ...
    def dispatch(self, *args, **kwargs) -> str | bool | Iterable | None: ...  # type: ignore
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...
    def add_hook(
        self, plugin: Plugin | str, handler: Callable, priority: int = ...
    ) -> None: ...
    def remove_hook(
        self, plugin: Plugin | str, handler: Callable, priority: int = ...
    ) -> None: ...

class EventDispatcherManager:
    _dispatchers: dict[str, EventDispatcher]

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
    ) -> PlayerDisonnectDispatcher: ...
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
    def __getitem__(
        self, key: Literal["player_items_toss"]
    ) -> PlayerItemsTossDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["damage"]) -> DamageDispatcher: ...
    def __contains__(self, key: str) -> bool: ...
    def add_dispatcher(self, dispatcher: Type[EventDispatcher]) -> None: ...
    def remove_dispatcher(self, dispatcher: Type[EventDispatcher]) -> None: ...
    def remove_dispatcher_by_name(self, event_name: str) -> None: ...

class ConsolePrintDispatcher(EventDispatcher):
    def dispatch(self, text: str) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class CommandDispatcher(EventDispatcher):
    def dispatch(self, caller: Player, command: Command, args: str) -> None: ...

class ClientCommandDispatcher(EventDispatcher):
    def dispatch(self, player: Player, cmd: str) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class ServerCommandDispatcher(EventDispatcher):
    def dispatch(self, player: Player, cmd: str) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class FrameEventDispatcher(EventDispatcher):
    def dispatch(self) -> str | bool | Iterable | None: ...

class SetConfigstringDispatcher(EventDispatcher):
    def dispatch(self, index: int, value: str) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class ChatEventDispatcher(EventDispatcher):
    def dispatch(
        self, player: Player, msg: str, channel: AbstractChannel
    ) -> str | bool | Iterable | None: ...

class UnloadDispatcher(EventDispatcher):
    def dispatch(self, plugin: Plugin | str) -> None: ...

class PlayerConnectDispatcher(EventDispatcher):
    def dispatch(self, player: Player) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class PlayerLoadedDispatcher(EventDispatcher):
    def dispatch(self, player: Player) -> str | bool | Iterable | None: ...

class PlayerDisonnectDispatcher(EventDispatcher):
    def dispatch(
        self, player: Player, reason: str | None
    ) -> str | bool | Iterable | None: ...

class PlayerSpawnDispatcher(EventDispatcher):
    def dispatch(self, player: Player) -> str | bool | Iterable | None: ...

class StatsDispatcher(EventDispatcher):
    def dispatch(self, stats: StatsData) -> str | bool | Iterable | None: ...

class VoteCalledDispatcher(EventDispatcher):
    def dispatch(
        self, player: Player, vote: str, args: str | None
    ) -> str | bool | Iterable | None: ...

class VoteStartedDispatcher(EventDispatcher):
    _caller: Player | None

    def __init__(self) -> None: ...
    def dispatch(self, vote: str, args: str | None) -> str | bool | Iterable | None: ...
    def caller(self, player: Player | None) -> None: ...

class VoteEndedDispatcher(EventDispatcher):
    def dispatch(self, passed: bool) -> None: ...

class VoteDispatcher(EventDispatcher):
    def dispatch(self, player: Player, yes: bool) -> str | bool | Iterable | None: ...

class GameCountdownDispatcher(EventDispatcher):
    def dispatch(self) -> str | bool | Iterable | None: ...

class GameStartDispatcher(EventDispatcher):
    def dispatch(self, data: GameStartData) -> str | bool | Iterable | None: ...

class GameEndDispatcher(EventDispatcher):
    def dispatch(self, data: GameEndData) -> str | bool | Iterable | None: ...

class RoundCountdownDispatcher(EventDispatcher):
    def dispatch(self, round_number: int) -> str | bool | Iterable | None: ...

class RoundStartDispatcher(EventDispatcher):
    def dispatch(self, round_number: int) -> str | bool | Iterable | None: ...

class RoundEndDispatcher(EventDispatcher):
    def dispatch(self, data: RoundEndData) -> str | bool | Iterable | None: ...

class TeamSwitchDispatcher(EventDispatcher):
    def dispatch(
        self, player: Player, old_team: str, new_team: str
    ) -> str | bool | Iterable | None: ...

class TeamSwitchAttemptDispatcher(EventDispatcher):
    def dispatch(
        self, player: Player, old_team: str, new_team: str
    ) -> str | bool | Iterable | None: ...

class MapDispatcher(EventDispatcher):
    def dispatch(self, mapname: str, factory: str) -> str | bool | Iterable | None: ...

class NewGameDispatcher(EventDispatcher):
    def dispatch(self) -> str | bool | Iterable | None: ...

class KillDispatcher(EventDispatcher):
    def dispatch(
        self, victim: Player, killer: Player | None, data: KillData
    ) -> str | bool | Iterable | None: ...

class DeathDispatcher(EventDispatcher):
    def dispatch(
        self, victim: Player, killer: Player | None, data: DeathData
    ) -> str | bool | Iterable | None: ...

class UserinfoDispatcher(EventDispatcher):
    def dispatch(
        self, playe: Player, changed: UserInfoEventInput
    ) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class KamikazeUseDispatcher(EventDispatcher):
    def dispatch(self, player: Player) -> str | bool | Iterable | None: ...

class KamikazeExplodeDispatcher(EventDispatcher):
    def dispatch(
        self, player: Player, is_used_on_demand: bool
    ) -> str | bool | Iterable | None: ...

class PlayerItemsTossDispatcher(EventDispatcher):
    def dispatch(self, player: Player) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class DamageDispatcher(EventDispatcher):
    def dispatch(
        self,
        target: Player | int | None,
        attacker: Player | int | None,
        damage: int,
        dflags: int,
        means_of_death: int,
    ) -> str | bool | Iterable | None: ...

EVENT_DISPATCHERS: EventDispatcherManager
