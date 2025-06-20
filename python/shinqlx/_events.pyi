from typing import TYPE_CHECKING, overload

if TYPE_CHECKING:
    from typing import Iterable, Callable, Literal, Type

    from re import Pattern

    from shinqlx import (
        Plugin,
        Player,
        Command,
        AbstractChannel,
        StatsData,
        GameStartData,
        GameEndData,
        RoundEndData,
        KillData,
        DeathData,
        UserinfoEventInput,
    )

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
    def dispatch(self, *args, **kwargs) -> str | bool | Iterable | None: ...  # type: ignore
    def add_hook(self, plugin: str, handler: Callable, priority: int = ...) -> None: ...
    def remove_hook(self, plugin: str, handler: Callable, priority: int = ...) -> None: ...

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
    def dispatch(self, player: Player, msg: str, channel: AbstractChannel) -> str | bool: ...

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
    def dispatch(self, victim: Player, killer: Player | None, data: DeathData) -> bool: ...

class UserinfoDispatcher(EventDispatcher):
    def dispatch(self, player: Player, changed: UserinfoEventInput) -> bool | UserinfoEventInput: ...

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
    def __getitem__(self, key: Literal["client_command"]) -> ClientCommandDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["server_command"]) -> ServerCommandDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["frame"]) -> FrameEventDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["set_configstring"]) -> SetConfigstringDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["chat"]) -> ChatEventDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["unload"]) -> UnloadDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["player_connect"]) -> PlayerConnectDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["player_loaded"]) -> PlayerLoadedDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["player_disconnect"]) -> PlayerDisconnectDispatcher: ...
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
    def __getitem__(self, key: Literal["game_countdown"]) -> GameCountdownDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["game_start"]) -> GameStartDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["game_end"]) -> GameEndDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["round_countdown"]) -> RoundCountdownDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["round_start"]) -> RoundStartDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["round_end"]) -> RoundEndDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["team_switch"]) -> TeamSwitchDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["team_switch_attempt"]) -> TeamSwitchAttemptDispatcher: ...
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
    def __getitem__(self, key: Literal["kamikaze_explode"]) -> KamikazeExplodeDispatcher: ...
    @overload
    def __getitem__(self, key: Literal["damage"]) -> DamageDispatcher: ...
    def __contains__(self, key: str) -> bool: ...
    def add_dispatcher(self, dispatcher: Type[EventDispatcher]) -> None: ...
    def remove_dispatcher(self, dispatcher: Type[EventDispatcher]) -> None: ...
    def remove_dispatcher_by_name(self, event_name: str) -> None: ...

EVENT_DISPATCHERS: EventDispatcherManager
