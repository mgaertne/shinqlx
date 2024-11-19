#######
Plugins
#######

.. _plugin:
.. currentmodule:: shinqlx

.. class:: Plugin()

   The base plugin class.

   Every plugin must inherit this or a subclass of this. It does not support any database
   by itself, but it has a *database* static variable that must be a subclass of the
   abstract class :class:`shinqlx.database.AbstractDatabase`. This abstract class requires
   a few methods that deal with permissions. This will make sure that simple plugins that
   only care about permissions can work on any database. Abstraction beyond that is hard,
   so any use of the database past that point will be uncharted territory, meaning the
   plugin will likely be database-specific unless you abstract it yourself.

   Permissions for commands can be overriden in the config. If you have a plugin called
   ``my_plugin`` with a command ``my_command``, you could override its permission
   requirement by adding ``perm_my_command: 3`` under a ``[my_plugin]`` header.
   This allows users to set custom permissions without having to edit the scripts.

   .. warning::
      I/O is the bane of single-threaded applications. You do **not** want blocking operations
      in code called by commands or events. That could make players lag. Helper decorators
      like :func:`shinqlx.thread` can be useful.

   .. property:: database
      :type: shinqlx.database.Redis | None
      :classmethod:

      The database driver class the plugin should use.

   .. property:: db
      :type: shinqlx.database.Redis | None

      Read-only accessor for the database instance.

   .. property:: name
      :type: str

      Read-only property to the name of this plugin

   .. property:: plugins
      :type: dict[str, Plugin]

      Read-only property to the dictionary containing plugin names as keys and plugin instances as values of all currently loaded plugins.

   .. property:: hooks
      :type: list[tuple[str, Callable, int]]

      Read-only property to the list of all the hooks this plugin has via :meth:`add_hook`

   .. property:: commands
      :type: list[Command]

      Read-only property to the list of all the commands this plugin has registered via :meth:`add_command`

   .. property:: game
      :type: Game | None

      Read-only property to the current Game instance. Might be None if between matches.

   .. property:: logger
      :type: logging.Logger

      Read-only property to get access to the instance of :class:`logging.Logger`, but initialized for this plugin.

   .. method:: add_hook(event, handler, priority = PRI_NORMAL)

      Add a hook for this plugin. The supported events with their respective prototype handler is listed in the table below.

      :param str event: The event to hook the ``handler`` up for. See valid values below.
      :param Callable handler: The handler for the ``event`` of this hook. See prototypes below
      :param int priority: The priority for this hook, valid values: :const:`PRI_LOWEST <shinqlx.PRI_LOWEST>`, :const:`PRI_LOW <shinqlx.PRI_LOW>`, :const:`PRI_NORMAL <shinqlx.PRI_NORMAL>`, :const:`PRI_HIGH <shinqlx.PRI_HIGH>`, :const:`PRI_HIGHEST <shinqlx.PRI_HIGHEST>` (default: ``PRI_NORMAL``)

      .. _event_hooks:

      ========================= =================
      event                     handler prototype
      ========================= =================
      ``"console_print"``       .. code-block:: python

                                   def handle_console_print(
                                       self,
                                       text: str
                                     ) -> str | None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"command"``             .. code-block:: python

                                   def handle_command(
                                       self,
                                       player: Player,
                                       cmd: Command,
                                       args: str
                                     ) -> None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"client_command"``      .. code-block:: python

                                   def handle_client_command(
                                       self,
                                       player: Player | None,
                                       cmd: str
                                     ) ->  str | None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"server_command"``      .. code-block:: python

                                   def handle_server_command(
                                       self,
                                       player: Player | None,
                                       cmd: str
                                     ) ->  str | None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"frame"``               .. code-block:: python

                                   def handle_frame(
                                       self
                                     ) ->  None | RET_NONE
      ``"set_configstring"``    .. code-block:: python

                                   def handle_set_configstring(
                                       self,
                                       index: int,
                                       value: str
                                     ) ->  str | None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"chat"``                .. code-block:: python

                                   def handle_chat(
                                       self,
                                       player: Player,
                                       msg: str,
                                       channel: AbstractChannel
                                     ) ->  str | None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"unload"``              .. code-block:: python

                                   def handle_unload(
                                       self,
                                       plugin: Plugin
                                     ) ->  None | RET_NONE
      ``"player_connect"``      .. code-block:: python

                                   def handle_player_connect(
                                       self,
                                       player: Player
                                     ) ->  str | None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"player_loaded"``       .. code-block:: python

                                   def handle_player_loaded(
                                       self,
                                       player: Player
                                     ) ->  None | RET_NONE
      ``"player_disconnect"``   .. code-block:: python

                                   def handle_player_disconnect(
                                       self,
                                       player: Player,
                                       reason: str
                                     ) ->  None | RET_NONE
      ``"player_spawn"``        .. code-block:: python

                                   def handle_player_spawn(
                                       self,
                                       player: Player
                                     ) ->  None | RET_NONE
      ``"stats"``               .. code-block:: python

                                   def handle_stats(
                                       self,
                                       stats
                                     ) ->  None | RET_NONE
      ``"vote_called"``         .. code-block:: python

                                   def handle_vote_called(
                                       self,
                                       player: Player,
                                       vote: str,
                                       args: str | None
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"vote_started"``        .. code-block:: python

                                   def handle_vote_started(
                                       self,
                                       player: Player,
                                       vote: str,
                                       args: str | None
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"vote_ended"``          .. code-block:: python

                                   def handle_vote_ended(
                                       self,
                                       votes: tuple[int, int],
                                       vote: str,
                                       args: str | None,
                                       passed: bool
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"vote"``                .. code-block:: python

                                   def handle_vote(
                                       self,
                                       player: Player,
                                       yes: bool
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"game_countdown"``      .. code-block:: python

                                   def handle_game_countdown(
                                       self
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"game_start"``          .. code-block:: python

                                   def handle_game_start(
                                       self,
                                       data,
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"game_end"``            .. code-block:: python

                                   def handle_game_end(
                                       self,
                                       data
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"round_countdown"``     .. code-block:: python

                                   def handle_round_countdown(
                                       self,
                                       round_number: int
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"round_start"``         .. code-block:: python

                                   def handle_round_start(
                                       self,
                                       round_number: int
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"round_end"``           .. code-block:: python

                                   def handle_round_end
                                       self,
                                       data
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"team_switch"``         .. code-block:: python

                                   def handle_team_switch(
                                       self,
                                       player: Player,
                                       old_team: str,
                                       new_team: str
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"team_switch_attempt"`` .. code-block:: python

                                   def handle_team_switch_attempt(
                                       self,
                                       player: Player,
                                       old_team: str,
                                       new_team: str
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"map"``                 .. code-block:: python

                                   def handle_map(
                                       self,
                                       mapname: str,
                                       factory: str
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"new_game"``            .. code-block:: python

                                   def handle_new_game(
                                       self
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"kill"``                .. code-block:: python

                                   def handle_kill(
                                       self,
                                       victim: Player,
                                       killer: Player,
                                       data
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"death"``               .. code-block:: python

                                   def handle_death(
                                       self,
                                       victim: Player,
                                       killer: Player | None,
                                       data
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"userinfo"``            .. code-block:: python

                                   def handle_userinfo(
                                       self,
                                       player: Player,
                                       changed: dict[str, str]
                                     ) ->  dict[str, str] | None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"kamikaze_use"``        .. code-block:: python

                                   def handle_kamikaze_use(
                                       self,
                                       player: Player
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"kamikaze_explode"``    .. code-block:: python

                                   def handle_kamikaze_explode(
                                       self,
                                       player: Player,
                                       is_used_on_demand: bool
                                     ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL
      ``"damage"``              .. code-block:: python

                                   def handle_damage(
                                       self,
                                       player: Player | int | None,
                                       attacker: Player | int | None,
                                       damage: int,
                                       dflags: int,
                                       means_of_death: int
                                     ) ->  None | RET_NONE
      ========================= =================

   .. method:: remove_hook(event, handler, priority = PRI_NORMAL)

      Remove a hook from this plugin. The supported events with their respective prototype handler are the same as listed in :meth:`add_hook`.

      :param str event: The event to remove the hook for ``handler``
      :param Callable handler: The handler for the ``event`` that should be removed from  this hook. See prototypes in :meth:`add_hook`
      :param int priority: The priority for this hook, valid values: :const:`PRI_LOWEST <shinqlx.PRI_LOWEST>`, :const:`PRI_LOW <shinqlx.PRI_LOW>`, :const:`PRI_NORMAL <shinqlx.PRI_NORMAL>`, :const:`PRI_HIGH <shinqlx.PRI_HIGH>`, :const:`PRI_HIGHEST <shinqlx.PRI_HIGHEST>` (default: ``PRI_NORMAL``)

   .. method:: add_command(name, handler, permission = PRIV_NONE, channels = None, exclude_channels = (), priority = PRI_NORMAL, client_cmd_pass = False, client_cmd_perm = 3, prefix = True, usage = "")

      Add a command for this plugin.

      .. seealso::
         :ref:`privileges` for the different privilege levels.

      :param str | Iterable[str] name: The name or names for the added commands.
      :param Callable handler: The handler for the command. See below for a prototype.
      :param int permission: The minimum permission a player trying to invoke this command needs. (default: ``PRIV_NONE``, i.e. no special permissions needed)
      :param Iterable[AbstractChannel] | None channels: The channels this command can be triggered from. (default: ``None``, command can be triggered from any channel)
      :param Iterable[Abstractchannel] exclude_channels: Explicit channels this command cannot be triggered from. (default: ``()``)
      :param int priority: The priority for this command, valid values: :const:`PRI_LOWEST <shinqlx.PRI_LOWEST>`, :const:`PRI_LOW <shinqlx.PRI_LOW>`, :const:`PRI_NORMAL <shinqlx.PRI_NORMAL>`, :const:`PRI_HIGH <shinqlx.PRI_HIGH>`, :const:`PRI_HIGHEST <shinqlx.PRI_HIGHEST>` (default: ``PRI_NORMAL``)
      :param bool client_cmd_pass: Flag whether this command should be passed to ``client_command`` and the general quake live engine. (default: ``False``)
      :param int client_cmd_perm: The minimum permission level needed when triggering this command via the ``client_command`` channel. (default; 5, i.e. the owner and super-admins.)
      :param bool prefix: Flag indicating whether this commands needs to be prefixed with the character in ``qlx_commandPrefix``, (default: ``True``)
      :param str usage: Usage message shown to a player trying to invoke this command when the ``handler`` returns ``RET_USAGE``. (default: ``""``)

      .. hint::
         Prototype for the ``handler`` Callable:

         .. code-block:: python

            def handle_cmd(
                self,
                player: Player,
                msg: str,
                channel: Abstractchannel
              ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL | RET_USAGE

   .. method:: remove_command(name: str | Iterable[str], handler: Callable)

      Remove a command from this plugin.

      :param str | Iterable[str] name: The name or names for the removed commands. Has to be the same as used in :meth:`add_command`.
      :param Callable handler: The handler for the command. Has to be the same as used in :meth:`add_command`.

   .. method:: __str__() -> str

   Class methods
   =============

   .. method:: get_cvar(name, str) -> str
               get_cvar(name, bool) -> bool
               get_cvar(name, int) -> int
               get_cvar(name, float) -> float
               get_cvar(name, list) -> list[str]
               get_cvar(name, set) -> set[str]
               get_cvar(name, tuple) -> tuple[str, ...]
               get_cvar(name, return_type = str) -> str | bool | int | float | list | set | tuple
      :classmethod:

      Retrieve a cvar from the server and convert it to the ``return_type``. For ``list``, ``set`` and ``tuple`` the original cvar is split along commas.

      :param str name: The name of the cvar.
      :param  return_type: The return_type of the function (default: ``str``)
      :type return_type: :class:`str` | :class:`bool` | :class:`int` | :class:`float` | :class:`list` | :class:`set` | :class:`tuple`
      :return: the converted cvar string.
      :raises ValueError: if ``return_type`` is neither :class:`str`, :class:`bool`, :class:`int`, :class:`float`, :class:`list`, :class:`set`, :class:`tuple`

   .. method:: set_cvar(name, value, flags = 0) -> bool
      :classmethod:

      Sets a cvar. If the cvar exists, it will be set as if set from the console, otherwise create it.

      .. seealso::
         :ref:`cvar_flags` for an explanation of the different flag values.

      :param str name: The name of the cvar
      :param value: The value to set the cvar to. Type can be anything that supports __str__()
      :param int flags: The flags to set if, and only if, the cvar does not exist and has to be created. (default: ``0``)
      :return: ``True`` if a new cvar was created, ``False`` if an existing cvar was overwritten.

   .. method:: set_cvar_limit(name, value, minimum, maximum, flags = 0)
      :classmethod:

      Sets a cvar with upper and lower limits. If the cvar exists, it will be set as if set from the console, otherwise create it.

      .. seealso::
         :ref:`cvar_flags` for an explanation of the different flag values.

      :param str name: The name of the cvar
      :param int | float value: The value to set the cvar to.
      :param int | float minimum: The minimum value of the cvar.
      :param int | float maximum: The maximum value of the cvar.
      :param int flags: The flags to set if, and only if, the cvar does not exist and has to be created. (default: ``0``)

   .. method:: set_cvar_once(name, value, flags = 0) -> bool
      :classmethod:

      Sets a cvar. If the cvar exists, do nothing.

      .. seealso::
         :ref:`cvar_flags` for an explanation of the different flag values.

      :param str name: The name of the cvar
      :param value: The value to set the cvar to. Type can be anything that supports __str__()
      :param int flags: The flags to set if, and only if, the cvar does not exist and has to be created. (default: ``0``)
      :return: ``True`` if a new cvar was created, ``False`` if an existing cvar already existed.

   .. method:: set_cvar_limit_once(name, value, minimum, maximum, flags = 0) -> bool
      :classmethod:

      Sets a cvar with upper and lower limits. If the cvar exists, not do anything.

      .. seealso::
         :ref:`cvar_flags` for an explanation of the different flag values.

      :param str name: The name of the cvar
      :param int | float value: The value to set the cvar to.
      :param int | float minimum: The minimum value of the cvar.
      :param int | float maximum: The maximum value of the cvar.
      :param int flags: The flags to set if, and only if, the cvar does not exist and has to be created. (default: ``0``)
      :return: ``True`` if a new cvar was created, ``False`` if an existing cvar already existed.

   .. method:: players() -> list[Player]
      :classmethod:

      Get a list of all the players on the server.

      :return: a list of all players

   .. method:: player(name, player_list = None) -> Player | None
      :classmethod:

      Get a Player instance from the name, client ID, or Steam ID. Assumes [0, 64) to be a client ID and [64, inf) to be a Steam ID.

      :param str | int | Player name: The name, client ID, or Steam ID used to gather the player.
      :param Iterable[Player] | None player_list: the list of players to search for. If ``None``, :meth:`players` is used. (default: ``None``)
      :return: The found player or ``None`` if none can be found

   .. method:: msg(msg, chat_channel = "chat")
      :classmethod:

      Send a message to the chat, or any other channel.

      :param str msg: The message to send.
      :param str chat_channel: The channel to send the message to. Valid string values are ``"chat"``, ``"red_team_chant"``, ``"blue_team_chant"``, ``"console"``. (default: ``"chat"``)
      :raises ValueError: if the provided channel is not supported.

   .. method:: console(text)
      :classmethod:

      Prints text in the console.

      :param str text: The text to print in the console.

   .. method:: clean_text(text) -> str
      :classmethod:

      Removes color tags from text.

      :param str text: The text that should be cleaned from color codes.
      :return: The cleaned text

   .. method:: colored_name(name, player_list = None) -> str | None
      :classmethod:

      Get the colored name of a decolored name if the player can be found.

      :param str | Player name: The name of the player to search.
      :param Iterable[Player] | None player_list:  the list of players to search for. If ``None``. :meth:`players` is used. (default: ``None``)
      :return: The colored name of the first found player in ``player_list`` or ``None`` if no matching player could be found.

   .. method:: client_id(name, player_list = None) -> int | None
      :classmethod:

      Get a player's client id from the name, client ID, Player instance, or Steam ID. Assumes [0, 64) to be a client ID and [64, inf) to be a Steam ID.

      :param str | int | Player name: The name, client ID, or Player of the player to search.
      :param Iterable[Player] | None player_list:  the list of players to search for. If ``None``, :meth:`players` is used. (default: ``None``)
      :return: The client_id of the first found player in ``player_list`` or ``None`` if no matching player could be found.

   .. method:: find_player(name, player_list = None) -> list[Player]
      :classmethod:

      Find a player based on part of a players name.

      :param str | None name: Name or part of the name of the players to search.
      :param Iterable[Player] | None player_list:  the list of players to search for. If ``None``, :meth:`players` is used. (default: ``None``)
      :return: The list of matching players founds in ``player_list``.

   .. method:: teams(player_list = None) -> dict[str, Player]
      :classmethod:

      Get a dictionary with the teams as keys and players as values.

      :param Iterable[Player] | None player_list:  the list of players to search for. If ``None``, :meth:`players` is used. (default: ``None``)
      :return: a dictionary mapping teams ``"red"``, ``"blue"``, ``"free"``, ``"spectator"`` to players

   .. method:: center_print(msg, recipient = None)
      :classmethod:

      Center print the provided ``msg`` to all players or the provided ``recipient``.

      :param str msg: The message to center print.
      :param str | int | Player | None recipient: The recipient of the center print. Center prints to all players if ``None``. (default: ``None``)

   .. method:: tell(msg, recipient)
      :classmethod:

      Send a private message (tell) to someone.

      :param str msg: The message to be sent.
      :param str | int | Player recipient: The player that should receive the message.

   .. method:: is_vote_active() -> bool
      :classmethod:

      Determines whether a vote is currently active on the server.

      :return: Whether a vote is currently active.

   .. method:: current_vote_count() -> tuple[int, int] | None
      :classmethod:

      Retrieve the current vote count. The resulting tuple will contain ``(yes_votes, no_votes)`` or ``None`` if no vote is currently active.

      :return: The current vote count or ``None`` if no vote is active.

   .. method:: callvote(vote, display, time = 30) -> bool
      :classmethod:

      Starts a new vote if none is active at the moment.

      :param str vote: The vote to execute.
      :param str display: How the vote is displayed for all players.
      :param int time: The vote time. (default: ``30``)
      :return: ``True`` if a new vote was started, ``False`` if a vote is currently running.

   .. method:: force_vote(pass_it) -> bool
      :classmethod:

      Forces a vote to pass or fail.

      :param bool pass_it: Whether to force a pass (``True``) or fail (``False``) for the current vote.
      :return: ``False`` if not vote is currently running, ``True`` if forcing the vote succeeded.
      :raises ValueError: if ``pass_it`` is neither ``True`` nor ``False``.

   .. method:: teamsize(size)
      :classmethod:

      Changes the team size to the given value.

      :param int size: the new teamsize to set.

   .. method:: kick(player, reason="")
      :classmethod:

      Kicks the given player with the given reason.

      :param str | int | Player player: the player to kick.
      :param str reason: the reason to kick the player for. (default: ``""``)
      :raises ValueError: if the player can not be identified.

   .. method:: shuffle()
      :classmethod:

      Shuffle the players.

   .. method:: change_map(new_map, factory = None)
      :classmethod:

      Change the current map.

      .. seealso::
         :data:`GAMETYPES_SHORT` for the default quake live factory values.

      :param str new_map: The short name of the map to change to.
      :param str | None factory: The game factory to change to. If ``None`` use the same factory as the current map. (default: ``None``)

   .. method:: switch(player, other_player)
      :classmethod:

      Switch ``player`` with ``other_player`` on the respective teams.

      :param Player player: the first player to switch.
      :param Player other_player: the second player to switch.
      :raises ValueError: if either player is invalid or both players are on the same team.

   .. method:: play_sound(sound_path, player = None) -> bool
      :classmethod:

      Play a sound to one player or all players.

      :param str sound_path: the path to the sound that shall be played.
      :param Player | None player: the player to play the sound to. If ``None``, the sound is played for all players. (default: ``None``)
      :return: ``False`` if ``sound_path`` starts with ``"music/"``, ``True`` otherwise.

   .. method:: play_music(music_path, player = None) -> bool
      :classmethod:

      Play a music to one player or all players.

      :param str music_path: the path to the music file that shall be played.
      :param Player | None player: the player to play the music to. If ``None``, the music is played for all players. (default: ``None``)
      :return: ``False`` if ``music_path`` starts with ``"sound/"``, ``True`` otherwise.

   .. method:: stop_sound(player = None)
      :classmethod:

      Stop sounds playing for one or all players.

      :param Player | None player: the player to stop playing sounds to. If ``None``, sound playback is stopped for all players. (default: ``None``)

   .. method:: stop_music(player = None)
      :classmethod:

      Stop music playing for one or all players.

      :param Player | None player: the player to stop playing music to. If ``None``, music playback is stopped for all players. (default: ``None``)

   .. method:: slap(player, damage = 0)
      :classmethod:

      Slap a player with and deal the provided amount of damage.

      :param str | int | Player player: The player to slap.
      :param int damage: The amount of damage to deal to the player when slapping. (default: ``0``)
      :raises ValueError: if an invalid player is provided.

   .. method:: slay(player)
      :classmethod:

      Slay (kill) a player instantly.

      :param str | int | Player player: The player to slay.
      :raises ValueError: if an invalid player is provided.

   Admin commands
   --------------

   .. method:: timeout()
      :classmethod:

      Time-out the game.

   .. method:: timein()
      :classmethod:

      Time-in a game that was :meth:`timeout` ed.

   .. method:: allready()
      :classmethod:

      Set all players to ready-up.

   .. method:: pause()
      :classmethod:

      Pause the current game.

   .. method:: unpause()
      :classmethod:

      Unpause a :meth:`pause` ed game.

   .. method:: lock(team=None)
      :classmethod:

      Lock all teams or just the given team.

      :param str | None team: The team to lock. If ``None``, all teams will be locked. (default: ``None``)

   .. method:: unlock(team=None)
      :classmethod:

      Unlock all teams or just the given team.

      :param str | None team: The team to unlock. If ``None``, all teams will be unlocked. (default: ``None``)

   .. method:: put(player, team)
      :classmethod:

      Put a player on a specific team.

      :param Player player: The player to move.
      :param str team: The team the player should be put onto.

   .. method:: mute(player)
      :classmethod:

      Mute the given player. Chat events will still be triggered, but chat messages will be blocked by the quake live engine.

      :param Player player: The player to mute.

   .. method:: unmute(player)
      :classmethod:

      Unmute a :meth:`mute` ed player.

      :param Player player: The player to unmute.

   .. method:: tempban(player)
      :classmethod:

      Temporarily ban a player from the server. Upon map change, the player will be allowed to connect again.

      :param Player player: The player to temporarily ban.

   .. method:: ban(player)
      :classmethod:

      Ban a player (permanently) from the server.

      :param Player player: The player to ban.

   .. method:: unban(player)
      :classmethod:

      Unban a player from the server that was :meth:`ban` ed before.

      :param Player player: The player to unban.

   .. method:: opsay(msg)
      :classmethod:

      Send a message as an operator of the server.

      :param str msg: The message to deliver.

   .. method:: addadmin(player)
      :classmethod:

      Grant the given player admin permissions.

      :param Player player: The player to promote to an admin.

   .. method:: addmod(player)
      :classmethod:

      Grant the given player moderation permissions.

      :param Player player: The player to promote to a moderator.

   .. method:: demote(player)
      :classmethod:

      Demote a player, i.e. stripping away all their permissions.

      :param Player player: The player to demote.

   .. method:: abort()
      :classmethod:

      Abort the current game, setting it back to warm-up.

   .. method:: addscore(player, score)
      :classmethod:

      Add the given score to the given player.

      :param Player player: The player to add score points to.
      :param int score: The amount of score points to add to the player. Can be negative.

   .. method:: addteamscore(team, score)
      :classmethod:

      Add the given score to the given team.

      :param str team: The team to add score points to.
      :param int score: The amount of score points to add to the team. Can be negative.

   .. method:: setmatchtime(time)
      :classmethod:

      Set the match time to the one provided.

      :param int time: The new match time.
