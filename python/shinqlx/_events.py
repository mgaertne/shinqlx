from shinqlx import (
    EventDispatcher,
    ClientCommandDispatcher,
    CommandDispatcher,
    ConsolePrintDispatcher,
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


# ====================================================================
#                               EVENTS
# ====================================================================
class EventDispatcherManager:
    """Holds all the event dispatchers and provides a way to access the dispatcher
    instances by accessing it like a dictionary using the event name as a key.
    Only one dispatcher can be used per event.

    """

    def __init__(self):
        self._dispatchers = {}

    def __getitem__(self, key):
        return self._dispatchers[key]

    def __contains__(self, key):
        return key in self._dispatchers

    def add_dispatcher(self, dispatcher):
        if dispatcher.name in self:
            raise ValueError("Event name already taken.")
        if not issubclass(dispatcher, EventDispatcher):
            raise ValueError(
                "Cannot add an event dispatcher not based on EventDispatcher."
            )
        print(dispatcher.name)
        self._dispatchers[dispatcher.name] = dispatcher()

    def remove_dispatcher(self, dispatcher) -> None:
        if dispatcher.name not in self:
            raise ValueError("Event name not found.")

        del self._dispatchers[dispatcher.name]

    def remove_dispatcher_by_name(self, event_name) -> None:
        if event_name not in self:
            raise ValueError("Event name not found.")

        del self._dispatchers[event_name]


EVENT_DISPATCHERS = EventDispatcherManager()
EVENT_DISPATCHERS.add_dispatcher(ConsolePrintDispatcher)
EVENT_DISPATCHERS.add_dispatcher(CommandDispatcher)
EVENT_DISPATCHERS.add_dispatcher(ClientCommandDispatcher)
EVENT_DISPATCHERS.add_dispatcher(ServerCommandDispatcher)
EVENT_DISPATCHERS.add_dispatcher(FrameEventDispatcher)
EVENT_DISPATCHERS.add_dispatcher(SetConfigstringDispatcher)
EVENT_DISPATCHERS.add_dispatcher(ChatEventDispatcher)
EVENT_DISPATCHERS.add_dispatcher(UnloadDispatcher)
EVENT_DISPATCHERS.add_dispatcher(PlayerConnectDispatcher)
EVENT_DISPATCHERS.add_dispatcher(PlayerLoadedDispatcher)
EVENT_DISPATCHERS.add_dispatcher(PlayerDisconnectDispatcher)
EVENT_DISPATCHERS.add_dispatcher(PlayerSpawnDispatcher)
EVENT_DISPATCHERS.add_dispatcher(KamikazeUseDispatcher)
EVENT_DISPATCHERS.add_dispatcher(KamikazeExplodeDispatcher)
EVENT_DISPATCHERS.add_dispatcher(StatsDispatcher)
EVENT_DISPATCHERS.add_dispatcher(VoteCalledDispatcher)
EVENT_DISPATCHERS.add_dispatcher(VoteStartedDispatcher)
EVENT_DISPATCHERS.add_dispatcher(VoteEndedDispatcher)
EVENT_DISPATCHERS.add_dispatcher(VoteDispatcher)
EVENT_DISPATCHERS.add_dispatcher(GameCountdownDispatcher)
EVENT_DISPATCHERS.add_dispatcher(GameStartDispatcher)
EVENT_DISPATCHERS.add_dispatcher(GameEndDispatcher)
EVENT_DISPATCHERS.add_dispatcher(RoundCountdownDispatcher)
EVENT_DISPATCHERS.add_dispatcher(RoundStartDispatcher)
EVENT_DISPATCHERS.add_dispatcher(RoundEndDispatcher)
EVENT_DISPATCHERS.add_dispatcher(TeamSwitchDispatcher)
EVENT_DISPATCHERS.add_dispatcher(TeamSwitchAttemptDispatcher)
EVENT_DISPATCHERS.add_dispatcher(MapDispatcher)
EVENT_DISPATCHERS.add_dispatcher(NewGameDispatcher)
EVENT_DISPATCHERS.add_dispatcher(KillDispatcher)
EVENT_DISPATCHERS.add_dispatcher(DeathDispatcher)
EVENT_DISPATCHERS.add_dispatcher(UserinfoDispatcher)
EVENT_DISPATCHERS.add_dispatcher(DamageDispatcher)
