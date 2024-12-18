from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from typing import Type, Callable, Iterable, Pattern, Mapping, ClassVar
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
hot_plugged_events: Iterable[str]

UncancellableEventReturn = Literal["RET_NONE"] | None
CancellableEventReturn = Literal[0, 1, 2, 3] | None

class EventDispatcher:
    name: ClassVar[str]

    def __init__(self) -> None:
        self.plugins: dict[
            Plugin,
            tuple[
                Iterable[Callable],
                Iterable[Callable],
                Iterable[Callable],
                Iterable[Callable],
                Iterable[Callable],
            ],
        ] = ...
        self._args: Iterable[str] | None = ...
        self._kwargs: Mapping[str, str] | None = ...
        self._return_value: str | bool | Iterable | None = ...
        self.no_debug: Iterable[str] = ...
        self.need_zmq_stats_enabled: bool = ...
        ...

    @property
    def args(self) -> Iterable[str]: ...

    # noinspection PyUnresolvedReferences
    @args.setter
    def args(self, value: Iterable[str]) -> None: ...
    @property
    def kwargs(self) -> Mapping[str, str]: ...

    # noinspection PyUnresolvedReferences
    @kwargs.setter
    def kwargs(self, value: Mapping[str, str]) -> None: ...
    @property
    def return_value(self) -> str | bool | Iterable | None: ...

    # noinspection PyUnresolvedReferences
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
    def __init__(self) -> None:
        self._dispatchers: dict[str, EventDispatcher] = ...
        ...

    def __getitem__(self, key: str) -> EventDispatcher: ...
    def __contains__(self, key: str) -> bool: ...
    def add_dispatcher(self, dispatcher: Type[EventDispatcher]) -> None: ...
    def remove_dispatcher(self, dispatcher: Type[EventDispatcher]) -> None: ...
    def remove_dispatcher_by_name(self, event_name: str) -> None: ...

class ConsolePrintDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, text: str) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class CommandDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, caller: Player, command: Command, args: str) -> None: ...

class ClientCommandDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, player: Player, cmd: str) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class ServerCommandDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, player: Player, cmd: str) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class FrameEventDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self) -> str | bool | Iterable | None: ...

class SetConfigstringDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, index: int, value: str) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class ChatEventDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self, player: Player, msg: str, channel: AbstractChannel
    ) -> str | bool | Iterable | None: ...

class UnloadDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, plugin: Plugin) -> None: ...

class PlayerConnectDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, player: Player) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class PlayerLoadedDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, player: Player) -> str | bool | Iterable | None: ...

class PlayerDisonnectDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self, player: Player, reason: str | None
    ) -> str | bool | Iterable | None: ...

class PlayerSpawnDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, player: Player) -> str | bool | Iterable | None: ...

class StatsDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, stats: StatsData) -> str | bool | Iterable | None: ...

class VoteCalledDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self, player: Player, vote: str, args: str | None
    ) -> str | bool | Iterable | None: ...

class VoteStartedDispatcher(EventDispatcher):
    name: ClassVar[str]

    def __init__(self) -> None:
        self._caller: Player | None = ...
        ...

    def dispatch(self, vote: str, args: str | None) -> str | bool | Iterable | None: ...
    def caller(self, player: Player) -> None: ...

class VoteEndedDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, passed: bool) -> None: ...

class VoteDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, player: Player, yes: bool) -> str | bool | Iterable | None: ...

class GameCountdownDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self) -> str | bool | Iterable | None: ...

class GameStartDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, data: GameStartData) -> str | bool | Iterable | None: ...

class GameEndDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, data: GameEndData) -> str | bool | Iterable | None: ...

class RoundCountdownDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, round_number: int) -> str | bool | Iterable | None: ...

class RoundStartDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, round_number: int) -> str | bool | Iterable | None: ...

class RoundEndDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, data: RoundEndData) -> str | bool | Iterable | None: ...

class TeamSwitchDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self, player: Player, old_team: str, new_team: str
    ) -> str | bool | Iterable | None: ...

class TeamSwitchAttemptDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self, player: Player, old_team: str, new_team: str
    ) -> str | bool | Iterable | None: ...

class MapDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, mapname: str, factory: str) -> str | bool | Iterable | None: ...

class NewGameDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self) -> str | bool | Iterable | None: ...

class KillDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self, victim: Player, killer: Player | None, data: KillData
    ) -> str | bool | Iterable | None: ...

class DeathDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self, victim: Player, killer: Player | None, data: DeathData
    ) -> str | bool | Iterable | None: ...

class UserinfoDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self, playe: Player, changed: UserInfoEventInput
    ) -> str | bool | Iterable | None: ...
    def handle_return(
        self, handler: Callable, value: int | str | None
    ) -> str | None: ...

class KamikazeUseDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(self, player: Player) -> str | bool | Iterable | None: ...

class KamikazeExplodeDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self, player: Player, is_used_on_demand: bool
    ) -> str | bool | Iterable | None: ...

class DamageDispatcher(EventDispatcher):
    name: ClassVar[str]

    def dispatch(
        self,
        target: Player | int | None,
        attacker: Player | int | None,
        damage: int,
        dflags: int,
        means_of_death: int,
    ) -> str | bool | Iterable | None: ...

EVENT_DISPATCHERS: EventDispatcherManager
