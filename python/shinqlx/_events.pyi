from typing import TYPE_CHECKING, overload

if TYPE_CHECKING:
    from typing import Type, Literal
    from shinqlx import (
        EventDispatcher,
        ConsolePrintDispatcher,
        CommandDispatcher,
        ClientCommandDispatcher,
        ServerCommandDispatcher,
        FrameEventDispatcher,
        SetConfigstringDispatcher,
        ChatEventDispatcher,
        UnloadDispatcher,
        PlayerConnectDispatcher,
        PlayerLoadedDispatcher,
        PlayerDisconnectDispatcher,
        PlayerSpawnDispatcher,
        StatsDispatcher,
        VoteCalledDispatcher,
        VoteStartedDispatcher,
        VoteEndedDispatcher,
        VoteDispatcher,
        GameCountdownDispatcher,
        GameStartDispatcher,
        GameEndDispatcher,
        RoundCountdownDispatcher,
        RoundStartDispatcher,
        RoundEndDispatcher,
        TeamSwitchDispatcher,
        TeamSwitchAttemptDispatcher,
        MapDispatcher,
        NewGameDispatcher,
        KillDispatcher,
        DeathDispatcher,
        UserinfoDispatcher,
        KamikazeUseDispatcher,
        KamikazeExplodeDispatcher,
        DamageDispatcher,
    )

UncancellableEventReturn = Literal[0] | None
CancellableEventReturn = Literal[0, 1, 2, 3] | None

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
