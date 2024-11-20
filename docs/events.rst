.. _events:
.. currentmodule:: shinqlx

######
Events
######

.. data:: EVENT_DISPATCHERS
   :type: EventDispatcherManager

   The central event dispatcher manager that holds all registered event dispatchers.

.. class:: EventDispatcherManager

   Holds all the event dispatchers and provides a way to access the dispatcher instances by accessing it like a dictionary using the event name as a key. Only one dispatcher can be used per event.

   .. method:: add_dispatcher(dispatcher)

      Adds a dispatcher.

      :param Type[EventDispatcher] dispatcher: The class of the dispatcher to be added.
      :raises ValueError: if the event name is already taken or the dispatcher is not a subclass of :class:`EventDispatcher`.

   .. method:: remove_dispatcher(dispatcher)

      Removes a dispatcher.

      :param Type[EventDispatcher] dispatcher: The class of the dispatcher to be removed.
      :raises ValueError: if the event name was not added.

   .. method:: remove_dispatcher_by_name(event_name)

      Removes a dispatcher by their event_name.

      :param str event_name: The event_name of the dispatcher to be removed.
      :raises ValueError: if the event name was not added.

.. class:: EventDispatcher

   The base event dispatcher. Each event should inherit this and provides a way to hook into events by registering an event handler.

   .. property:: no_debug
      :type: tuple[str, ...]

      Events for which no debug messages are produced in the logfile.

      By default this is set to: ``"frame"``, ``"set_configstring"``, ``"stats"``, ``"server_command"``, ``"death"``, ``"kill"``, ``"command"``, ``"console_print"``, ``"damage"``

      **Read-only**.

   .. property:: name
      :type: str

      The name of the event dispatcher. **Read-only**.

   .. property:: need_zmq_stats_enabled
      :type: bool

      Whether the event dispatcher needs ``zmq_stats_enables`` to be enabled. **Read-only**.

   .. property:: plugins()
      :type: dict[Plugin, tuple[list[Callable], list[Callable], list[Callable], list[Callable], list[Callable]]]

      Dictionary of the registered plugins for this event dispatcher. **Read-only**.

   .. method:: handle_return(handler, value) -> str | None

      Handles non-standard return values in subclasses. The default implementation will simply log a warning.

      :param Callable handler: The handler for the event.
      :param int | str | None value: The non-standard return value that was returned
      :return: A changed return value, or ``None``.

   .. method:: dispatch(*args, **kwargs) -> str | bool | Iterable | None

      Calls all the handlers that have been registered when hooking this event.

      The recommended way to use this for events that inherit this class is to override the method with explicit arguments (as opposed to this one's) and call this method by using ``super().dispatch()``.

      Handlers have several options for return values that can affect the flow:

         * shinqlx.RET_NONE or None -- Continue execution normally.
         * shinqlx.RET_STOP -- Stop any further handlers from being called.
         * shinqlx.RET_STOP_EVENT -- Let handlers process it, but stop the event at the engine-level.
         * shinqlx.RET_STOP_ALL -- Stop handlers **and** the event.
         * Any other value -- Passed on to :meth:`handle_return`, which will by default simply send a warning to the logger about an unknown value being returned. Can be overridden so that events can have their own special return values.

      :param args: Any arguments.
      :param kwargs: Any keyword arguments.
      :return: Whether to pass the event to the engine, or a changed event value that is passed on to the engine.

   .. method:: add_hook(plugin, handler, priority=PRI_NORMAL)

      Hook the event, making the handler get called with relevant arguments whenever the event is takes place.

      :param Plugin | str plugin: The plugin that's hooking the event.
      :param Callable handler: The handler to be called when the event takes place.
      :param int priority: The priority of the hook. Determines the order the handlers are called in. valid values: :const:`PRI_LOWEST <shinqlx.PRI_LOWEST>`, :const:`PRI_LOW <shinqlx.PRI_LOW>`, :const:`PRI_NORMAL <shinqlx.PRI_NORMAL>`, :const:`PRI_HIGH <shinqlx.PRI_HIGH>`, :const:`PRI_HIGHEST <shinqlx.PRI_HIGHEST>` (default: ``PRI_NORMAL``)
      :raises ValueError: when passed an invalid priority, or the event is already registered with the same handler.
      :raises AssertionError: if the hook requires ``zmq_stats_enables`` cvar to be set to ``1``, but it was disabled.

   .. method:: remove_hook(plugin, handler, priority = PRI_NORMAL)

      Removes a previously hooked event.

      :param Plugin | str plugin: The plugin that hooked the event.
      :param Callable handler: The handler used when hooked.
      :param int priority: The priority of the hook when hooked.
      :raises ValueError: if the event has not been hooks up with the handler provided.

Concrete Event dispatchers
--------------------------

.. class:: ConsolePrintDispatcher

   ``name = "console_print"``

   Event that goes off whenever the console prints something, including those with :func:`shinqlx.console_print`

   .. method:: dispatch(text) -> str | bool

      :param str text: The text to print
      :return: Whether to pass on the event to the engine, or a changed text to print to the console.

.. class:: CommandDispatcher

   ``name = "command"``

   Event that goes off when a command is executed. This can be used to for instance keep a log of all the commands admins have used.

   .. method:: dispatch(caller, command, args)

      :param Player caller: The player that issued the command.
      :param Command command: The command that was triggered.
      :param str args: Any additional arguments to the command.

.. class:: ClientCommandDispatcher

   ``name = "client_command"``

   Event that triggers with any client command. This overlaps with other events, such as "chat".

   .. method:: dispatch(player, cmd) -> str | bool

      :param Player player: The player that issued the command.
      :param str cmd: The command the player issued.
      :return: Whether to pass on the event to the engine, or a changed command.

.. class:: ServerCommandDispatcher

   ``name = "server_command"``

   Event that triggers with any server command sent by the server, including :func:`shinqlx.send_server_command`. Can be cancelled.

   .. method:: dispatch(player, cmd) -> str | bool

      :param Player | None player: The player (if any) the server command is issued for.
      :param str cmd: The command that was issued.
      :return: Whether to pass on the event to the engine, or a changed command.

.. class:: FrameEventDispatcher

   ``name = "frame"``

   Event that triggers every frame. Cannot be cancelled.

   .. method:: dispatch() -> bool

      :return: Whether to pass on the event to the engine.

.. class:: SetConfigstringDispatcher

   ``name = "set_configstring"``

   Event that triggers when the server tries to set a configstring. You can stop this event and use :func:`shinqlx.set_configstring` to modify it, but a more elegant way to do it is simply returning the new configstring in the handler, and the modified one will go down the plugin chain instead.

   .. method:: dispatch(index, value) -> str | bool

      :param int index: The configstring index to be set.
      :param str value: The value to set the configstring index to.
      :return: Whether to pass on the event to the engine, or a changed configstring value.

.. class:: ChatEventDispatcher

   ``name = "chat"``

   Event that triggers with the "say" command. If the handler cancels it, the message will also be cancelled.

   .. method:: dispatch(player, msg, channel) -> str | bool

      :param Player player: The player that issued the chat event.
      :param str msg: The chat message that was sent.
      :param AbstractChannel channel: The channel the chat message was sent to.
      :return: Whether to pass on the event to the engine, or a chat message.

.. class:: UnloadDispatcher:

   ``name = "unload"``

   Event that triggers whenever a plugin is unloaded. Cannot be cancelled.

   .. method:: dispatch(plugin)

      :param Plugin | str plugin: The plugin that was about to be unloaded.

.. class:: PlayerConnectDispatcher

   ``name = "player_connect"``

   Event that triggers whenever a player tries to connect. If the event is not stopped, it will let the player connect as usual. If it is stopped it will either display a generic ban message, or whatever string is returned by the handler.

   .. method:: dispatch(player) -> str | bool

      :param Player player: The player that is trying to connect.
      :return: Whether to pass on the event to the engine, or the message to show the connecting player.

.. class:: PlayerLoadedDispatcher

   ``name = "player_loaded"``

   Event that triggers whenever a player connects *and* finishes loading. This means it'll trigger later than the "X connected" messages in-game, and it will also trigger when a map changes and players finish loading it.

   .. method:: dispatch(player) -> bool

      :param Player player: The player that finished connecting to the server.
      :return: Whether to pass on the event to the engine.

.. class:: PlayerDisconnectDispatcher

   ``name = "player_disconnect"``

   Event that triggers whenever a player disconnects. Cannot be cancelled.

   .. method:: dispatch(player, reason) -> bool

      :param Player player: The player that is disconnecting.
      :param str | None reason: The reason why the player disconnects.
      :return: Whether to pass on the event to the engine.

.. class:: PlayerSpawnDispatcher

   ``name = "player_spawn"``

   Event that triggers when a player spawns. Cannot be cancelled.

   .. method:: dispatch(player) -> bool

      :param Player player: The player that is spawning.
      :return: Whether to pass on the event to the engine.

.. class:: StatsDispatcher

   ``name = "stats"``

   ``needs_zmq_stats_enabled = True``

   Event that triggers whenever the server sends stats over ZMQ.

   .. method:: dispatch(stats) -> bool

      :param dict stats: The raw stats event that was sent via ZMQ.
      :return: Whether to pass on the event to the engine.

.. class:: VoteCalledDispatcher

   ``name = "vote_called"``

   Event that goes off whenever a player tries to call a vote. Note that this goes off even if it's a vote command that is invalid. Use vote_started if you only need votes that actually go through. Use this one for custom votes.

   .. method:: dispatch(player, vote, args) -> bool

      :param Player player: The player that called the vote.
      :param str vote: The vote that was called.
      :param str | None args: Additional arguments to the vote.
      :return: Whether to pass on the event to the engine.

.. class:: VoteStartedDispatcher

   ``name = "vote_started"``

   Event that goes off whenever a vote starts. A vote started with :meth:`Plugin.callvote` will have the caller set to ``None``.

   .. method:: dispatch(vote, args) -> bool

      :param str vote: The vote that is started.
      :param str | None args: Additional arguments to the vote.
      :return: Whether to pass on the event to the engine.

   .. method:: caller(player)

      Sets the caller for the next ``"vote_started"`` event.

      :param Player | None player: The player that shall start the next ``"vote_started"`` event.

.. class:: VoteEndedDispatcher

   ``name = "vote_ended"``

   Event that goes off whenever a vote either passes or fails.

   .. method:: dispatch(passed)

      :param bool passed: Whether or not the vote succeeded.

.. class:: VoteDispatcher

   ``name = "vote"``

   Event that goes off whenever someone tries to vote either yes or no.

   .. method:: dispatch(player, yes) -> bool

      :param Player player: The player that voted.
      :param bool yes: Whether the player voted in favor of the vote.
      :return: Whether to pass on the event to the engine.

.. class:: GameCountdownDispatcher

   ``name = "game_countdown"``

   Event that goes off when the countdown before a game starts.

   .. method:: dispatch() -> bool

      :return: Whether to pass on the event to the engine.

.. class:: GameStartDispatcher

   ``name = "game_start"``

   ``need_zmq_stats_enabled = True``

   Event that goes off when a game starts.

   .. method:: dispatch(data) -> bool

      :param dict data: The raw game start event data received via ZMQ.
      :return: Whether to pass on the event to the engine.

.. class:: GameEndDispatcher

   ``name = "game_end"``

   ``need_zmq_stats_enabled = True``

   Event that goes off when a game ends.

   .. method:: dispatch(data) -> bool

      :param dict data: The raw game end event data received via ZMQ.
      :return: Whether to pass on the event to the engine.

.. class:: RoundCountdownDispatcher

   ``name = "round_countdown"``

   Event that goes off when the countdown before a round starts.

   .. method:: dipatch(round_number) -> bool

      :param int round_number: The round number that is about to start.
      :return: Whether to pass on the event to the engine.

.. class:: RoundStartDispatcher

   ``name = "round_start"``

   Event that goes off when a round starts.

   .. method:: dipatch(round_number) -> bool

      :param int round_number: The round number that is about to start.
      :return: Whether to pass on the event to the engine.

.. class:: RoundEndDispatcher

   ``name = "round_end"``

   ``need_zmq_stats_enabled = True``

   Event that goes off when a round ends.

   .. method:: dipatch(data) -> bool

      :param dict data: The raw round end event data received via ZMQ.
      :return: Whether to pass on the event to the engine.

.. class:: TeamSwitchDispatcher

   ``name = "team_switch"``

   ``need_zmq_stats_enabled = True``

   For when a player switches teams. If cancelled, simply put the player back in the old team.

   If possible, consider using :class ``"team_switch_attempt"`` for a cleaner solution if you need to cancel the event.

   .. method:: dispatch(player, old_team, new_team) -> bool

      :param Player player: The player that was switched.
      :param str old_team: The old team of the player.
      :param str new_team: The team the player switched to.
      :return: Whether to pass on the event to the engine.

.. class:: TeamSwitchAttemptDispatcher

   ``name = "team_switch"``

   For when a player attempts to join a team. Prevents the player from doing it when cancelled.

   When players click the Join Match button, it sends "team a" (with the "a" being "any", presumably), meaning the new_team argument can also be "any" in addition to all the other teams.For when a player switches teams. If cancelled, simply put the player back in the old team.

   .. method:: dispatch(player, old_team, new_team) -> bool

      :param Player player: The player that was switched.
      :param str old_team: The old team of the player.
      :param str new_team: The team the player switched to.
      :return: Whether to pass on the event to the engine.

.. class:: MapDispatcher

   ``name = "map"``

   Event that goes off when a map is loaded, even if the same map is loaded again.

   .. method:: dispatch(mapname, factory) -> bool

      :param str mapname: The name of the new map.
      :param str factory: The factory for the map change.
      :return: Whether to pass on the event to the engine.

.. class:: NewGameDispatcher

   ``name = "new_game"``

   Event that goes off when the game module is initialized. This happens when new maps are loaded, a game is aborted, a game ends but stays on the same map, or when the game itself starts.

   .. method:: dispatch() -> bool

      :return: Whether to pass on the event to the engine.

.. class:: KillDispatcher

   ``name = "kill"``

   ``need_zmq_stats_enabled = True``

   Event that goes off when someone is killed.

   .. method:: dispatch(victim, killer, data) -> bool

      :param Player victim: The player that was killed.
      :param Player killer: The player that killed the victim.
      :param dict data: The raw kill event data received via ZMQ.
      :return: Whether to pass on the event to the engine.

.. class:: DeathDispatcher

   ``name = "death"``

   ``need_zmq_stats_enabled = True``

   Event that goes off when someone dies.

   .. method:: dispatch(victim, killer, data) -> bool

      :param Player victim: The player that was killed.
      :param Player | None killer: The player that killed the victim, or ``None`` if victim died from an environmental hazard.
      :param dict data: The raw kill event data received via ZMQ.
      :return: Whether to pass on the event to the engine.

.. class:: UserinfoDispatcher

   ``name = "userinfo"``

   Event for clients changing their userinfo.

   .. method:: dispatch(player, changed) -> bool | dict

      :param Player player: The player that changed their userinfo.
      :param dict changed: The changed userinfo values.
      :return: Whether to pass on the event to the engine, or the new changed userinfo values.

.. class:: KamikazeUseDispatcher

   ``name = "kamikaze_use"``

   Event that goes off when player uses kamikaze item.

   .. method:: dispatch(player) -> bool

      :param Player player: The player that used their kamikaze holdable.
      :return: Whether to pass on the event to the engine.

.. class:: KamikazeUExplodeDispatcher

   ``name = "kamikaze_explode"``

   Event that goes off when player uses kamikaze item.

   .. method:: dispatch(player, is_used_on_demand) -> bool

      :param Player player: The player that had heir kamikaze holdable explode.
      :param bool is_used_on_demand: Whether the player use the kamikaze on their demand, or not.
      :return: Whether to pass on the event to the engine.

.. class:: DamageDispatcher

   ``name = "damage"``

   Event that goes off when someone is inflicted with damage.

   .. method:: dispatch(target, attacker, damage, dflags, means_of_death) -> bool

      :param Player | int | None target: The target of the inflicted damage.
      :param Player | int | None attacker: The attacker for the inflicted damage.
      :param int damage: The raw damage amount before applying handicaps, etc.
      :param int dflags: The damage flags. See :ref:`damage_flags`.
      :param int means_of_death: The means of death used. See :ref:`means_of_death`
      :return: Whether to pass on the event to the engine.
