.. _core:
.. currentmodule:: shinqlx

##############
Core functions
##############

Classes
=======

.. class:: PlayerInfo()

   Information about a player, such as Steam ID, name, client ID, and whatnot.

   .. property:: client_id
      :type: int

      The player's client ID.

   .. property:: name
      :type: str

      The player's name.

   .. property:: connection_state
      :type: int

      The player's connection state.

      .. seealso::
         :ref:`connection_states` for the different connection states.

   .. property:: userinfo
      :type: str

      The player's userinfo.

   .. property:: steam_id
      :type: int

      The player's 64-bit representation of the Steam ID.

   .. property:: team
      :type: int

      The player's team.

      .. seealso::
         :ref:`teams` for the different team values.

   .. property:: privileges
      :type: int

      The player's privileges.

      .. seealso::
         :ref:`privileges` for the different privilege values.

.. class:: PlayerState()

   Information about a player's state in the game.

   .. property:: is_alive
      :type: bool

      Whether the player is alive or not.

   .. property:: position
      :type: Vector3

      The player's position.

   .. property:: velocity
      :type: Vector3

      The player's velocity.

   .. property:: health
      :type: int

      The player's health.

   .. property:: armor
      :type: int

      The player's armor.

   .. property:: noclip
      :type: bool

      Whether the player has noclip or not.

   .. property:: weapon
      :type: int

      The weapon the player is currently using.

   .. property:: weapons
      :type: Weapons

      The player's weapons.

   .. property:: ammo
      :type: Weapons

      The player's weapon ammo.

   .. property:: powerups
      :type: Powerups

      The player's powerups.

   .. property:: holdable
      :type: int

      The player's holdable item.

   .. property:: flight
      :type: Flight

      A struct sequence with flight parameters.

   .. property:: is_chatting
      :type: bool

      Whether the player is currently chatting.

   .. property:: is_frozen
      :type: bool

      Whether the player is frozen(freezetag).

.. class:: PlayerStats()

   A player's score and some basic stats.

   .. property:: score
      :type: int

      The player's primary score.

   .. property:: kills
      :type: int

      The player's number of kills.

   .. property:: deaths
      :type: int

      The player's number of deaths.

   .. property:: damage_dealt
      :type: int

      The player's total damage dealt.

   .. property:: damage_taken
      :type: int

      The player's total damage taken.

   .. property:: time
      :type: int

      The time in milliseconds the player has on a team since the game started.

   .. property:: ping
      :type: int

      The player's ping.

(Helper) Data-classes
---------------------

.. class:: Vector3()

   A three-dimensional vector.

   .. property:: x
      :type: int

   .. property:: y
      :type: int

   .. property:: z
      :type: int

.. class:: Flight()

   A struct sequence containing parameters for the flight holdable item.

   .. property:: fuel
      :type: int

   .. property:: max_fuel
      :type: int

   .. property:: thrust
      :type: int

   .. property:: refuel
      :type: int

.. class:: Powerups()

   A struct sequence containing all the powerups in the game.

   .. property:: quad
      :type: int

   .. property:: battlesuit
      :type: int

   .. property:: haste
      :type: int

   .. property:: invisibility
      :type: int

   .. property:: regeneration
      :type: int

   .. property:: invulnerability
      :type: int

.. class:: Weapons()

   A struct sequence containing all the weapons in the game.

   .. property:: g
      :type: int

      Gauntlet

   .. property:: mg
      :type: int

      Machine-gun

   .. property:: sg
      :type: int

      Shotgun

   .. property:: gl
      :type: int

      Grenade launcher

   .. property:: rl
      :type: int

      Rocket launcher

   .. property:: lg
      :type: int

      Lighting gun

   .. property:: rg
      :type: int

      Railgun

   .. property:: pg
      :type: int

      Plasma gun

   .. property:: bfg
      :type: int

      BFG

   .. property:: gh
      :type: int

      Grappling hook

   .. property:: ng
      :type: int

      Nailgun

   .. property:: pl
      :type: int

      Proximity-mine launcher

   .. property:: cg
      :type: int

      Chaingun

   .. property:: hmg
      :type: int

      Heavy machinegun

   .. property:: hands
      :type: int

      Hands

Functions
=========

.. function:: player_info(client_id) -> PlayerInfo | None

   Returns a :class:`PlayerInfo` with information about a player by ID.

   :param int client_id: The ``client_id`` to retrieve the information from.
   :return: The :class:`PlayerInfo` for the given player, or ``None`` if no player could be found.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: players_info() -> list[PlayerInfo]

   Returns a list with :class:`PlayerInfo` s with information about all the players on the server.

   :return: a list with :class:`PlayerInfo` s with information about all the players on the server

.. function:: get_userinfo(client_id) -> str | None

   Returns a string with a player's userinfo.

   :param int client: The ``client_id`` to retrieve the userinfo from.
   :return: The userinfo string for the given player, or ``None`` if no player could be found.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: send_server_command(client_id, cmd) -> bool

   Sends a server command to either one specific client or all the clients.

   :param int | None client_id: The (optional) ``client_id`` to send the server command to. If ``None``, the server command is sent to all players.
   :param str cmd: The server command to send.
   :return: ``False`` if the client for the ``client_id`` is not an active player, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` not ``None`` and not in the range between ``0`` and ``sv_maxclients``.

.. function:: client_command(client_id, cmd) -> bool

   Tells the server to process a command from a specific client.

   :param int client_id: The ``client_id`` to send the client command to.
   :param str cmd: The client command to send.
   :return: ``False`` if the client for the ``client_id`` is not an active player, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: console_command(cmd)

   Executes a command as if it was executed from the server console.

   :param str cmd: The command to execute.

.. function:: get_cvar(cvar) -> str | None

   Gets a cvar.

   :param str cvar: The name of the cvar to get.
   :return: The cvar string, or ``None`` if the cvar is not set.

.. function:: set_cvar(cvar, value, flags = None) -> bool

   Sets a cvar.

   .. seealso::
      :ref:`cvar_flags` for an explanation of the different flag values.

   :param str cvar: The name of the cvar to set.
   :param str value: The value to set the cvar to.
   :param int | None flags: The flags to set if, and only if, the cvar does not exist and has to be created. (default: ``None``)
   :return: ``True`` if a new cvar was created, ``False`` if an existing cvar was overwritten.

.. function:: set_cvar_limit(cvar, value, min, max, flags = None)

   Sets a cvar with minimum and maximum values.

   .. seealso::
      :ref:`cvar_flags` for an explanation of the different flag values.

   :param str cvar: The name of the cvar to set.
   :param str value: The value to set the cvar to.
   :param int | float min: The minimum value of the cvar.
   :param int | float max: The maximum value of the cvar.
   :param int | None flags: The flags to set if, and only if, the cvar does not exist and has to be created. (default: ``None``)

.. function:: kick(client_id, reason = None)

   Kick a player and allowing the admin to supply a reason for it.

   :param int client_id: The ``client_id`` of the player to kick.
   :param str | None reason: The reason for the kick. (default: ``None``)
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: console_print(text)

   Prints text on the console. If used during an RCON command, it will be printed in the player's console.

   :param str text: The text to print.

.. function:: get_configstring(config_id) -> str

   Get a configstring.

   :param int config_id: The id of the configstring to get.
   :return: The value of the configstring.
   :rasies ValueError: if the ``config_id`` is not between ``0`` and ``MAX_CONFISTRINGS`` (``1024``).

.. function:: set_configstring(config_id, value)

   Sets a configstring and sends it to all the players on the server.

   :param int config_id: The id of the configstring to set.
   :param str value: The value the configstring is set to.
   :rasies ValueError: if the ``config_id`` is not between ``0`` and ``MAX_CONFISTRINGS`` (``1024``).

.. function:: force_vote(pass) -> bool

   Forces the current vote to either fail or pass.

   :param bool pass: Whether to pass (``True``) or fail (``False``) the vote.
   :return: ``False``, if no vote is in progress, ``True`` otherwise.

.. function:: add_console_command(command)

   Adds a console command that will be handled by Python code.

   :param str command: The command trigger to add.

.. function:: register_handler(event, handler = None)

   Register an event handler. Can be called more than once per event, but only the last one will work.

   :param str event: The event to register the handler for. Valid values: ``"rcon"``, ``"client_command"``, ``"server_command"``, ``"new_game"``, ``"set_configstring"``, ``"console_print"``, ``"frame"``, ``"player_connect"``, ``"player_loaded"``, ``"player_disconnect"``, ``"player_spawn"``, ``"kamikaze_use"``, ``"kamikaze_explode"``, ``"damage"``.
   :param Callable | None handler: The handler for the event. If ``None``, the handler will be removed and no related events triggered anymore in any plugins. (default: ``None``)
   :raises ValueError: if the event is neither of the supported values.
   :raises TypeError: if the handler is not ``None`` and not callable.

.. function:: player_state(client_id) -> PlayerState | None

   Get information about the player's state in the game.

   :param int client_id: The ``client_id`` of the player to gather the player's state from.
   :return: the :class:`PlayerState` of the player, or ``None``, if no player exists or is (not yet?) valid.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: player_stats(client_id) -> PlayerStats | None

   Get some player stats.

   :param int client_id: The ``client_id`` of the player to gather the player's stats from.
   :return: the :class:`PlayerStats` of the player, or ``None``, if no player exists or is (not yet?) valid.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_position(client_id, position) -> bool

   Sets a player's position vector.

   :param int client_id: The ``client_id`` of the player to set their position to.
   :param Vector3 position: The new position for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_velocity(client_id, velocity) -> bool

   Sets a player's velocity vector.

   :param int client_id: The ``client_id`` of the player to set their velocitry to.
   :param Vector3 velocity: The new velocity for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: noclip(client_id, activate) -> bool

   Sets noclip for a player.

   :param int client_id: The ``client_id`` of the player to set their noclip flag.
   :param bool activate: Whether to activate (``True``) or deactivate (``False``) for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_health(client_id, health) -> bool

   Sets a player's health.

   :param int client_id: The ``client_id`` of the player to set their health value.
   :param int health: The new health value.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_armor(client_id, armor) -> bool

   Sets a player's armor.

   :param int client_id: The ``client_id`` of the player to set their armor value.
   :param int health: The new armor value.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_weapons(client_id, weapons) -> bool

   Sets a player's weapons.

   :param int client_id: The ``client_id`` of the player to set their weapons.
   :param Weapons weapons: The weapons to set for the player. Any weapon with a non-zero value will be given to the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_weapon(client_id, weapon) -> bool

   Sets a player's weapon.

   .. seealso::
      :data:`WEAPONS` for the different weapon values.

   :param int client_id: The ``client_id`` of the player to set their weapon.
   :param int weapon: The weapon to set for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_ammo(client_id, ammo) -> bool

   Sets a player's ammo.

   :param int client_id: The ``client_id`` of the player to set their ammo values.
   :param Weapons ammo: The ammo values to set for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_powerups(client_id, powerups) -> bool

   Sets a player's powerups.

   :param int client_id: The ``client_id`` of the player to set their powerups.
   :param Powerups powerups: The powerups to set for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_holdable(client_id, holdable) -> bool

   Sets a player's holdable item.

   :param int client_id: The ``client_id`` of the player to set their holdable.
   :param int holdable: The holdable to set for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: drop_holdable(client_id) -> bool

   Drops a player's holdable item.

   :param int client_id: The ``client_id`` of the player to set their holdable.
   :return: ``False`` if no valid player could be determined or the player does not hold a holdable item, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_flight(client_id, flight) -> bool

   Sets a player's flight parameters, such as current fuel, max fuel and, so on.

   :param int client_id: The ``client_id`` of the player to set their flight values.
   :param Flight flight: The flight values to set for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_invulnerability(client_id, time) -> bool

   Makes player invulnerable for limited time.

   :param int client_id: The ``client_id`` of the player to set their invulnerability time.
   :param int time: Time in seconds the invulnerability will be active.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_score(client_id, score) -> bool

   Sets a player's score.

   :param int client_id: The ``client_id`` of the player to set their new score value.
   :param int score: The new score value for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: callvote(vote, vote_display, vote_time = None)

   Calls a vote as if started by the server and not a player.

   :param str vote: The vote command that will be executed when the vote is passed.
   :param str vote_display: The text to prompt player's during the voting period.
   :param int | None time: The vote time. (default: ``None``)

.. function:: allow_single_player(allow)

   Allows or disallows a game with only a single player in it to go on without forfeiting. Useful for race.

   :param bool allow: Whether to allow single player, or not.

.. function:: player_spawn(client_id) -> bool

   Spawn a player.

   :param int client_id: The ``client_id`` of the player to spawn.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: set_privileges(client_id, privileges) -> bool

   Sets a player's privileges. Does not persist.

   .. seealso::
      :ref:`privileges` for the different privilege levels.

   :param int client_id: The ``client_id`` of the player to set their privileges.
   :param int privileges: The new privileges for the player.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: destroy_kamikaze_timers() -> bool

   Removes all current kamikaze timers.

   :return: ``True``.

.. function:: spawn_item(item_id, x, y, z) -> bool

   Spawns item with specified coordinates.

   .. seealso::
      :meth:`dev_print_items` for the different items in the current map.

   :param int item_id: The id of the item to spawn.
   :param int x: The x-coordinate of the spawned item.
   :param int y: The y-coordinate of the spawned item.
   :param int z: The z-coordinate of the spawned item.
   :return: ``True``.
   :raises ValueError: if the ``item_id`` is not in the range between ``0`` and the maximum number of items on the current map and gamemode.

.. function:: remove_dropped_item() -> bool

   Removes all dropped items.

   :return: ``True``.

.. function:: slay_with_mod(client_id, mod) -> bool

   Slay player with means of death.

   .. seealso::
      :ref:`means_of_death` for the different means of death value.

   :param int client_id: The ``client_id`` of the player to slay.
   :param int mod: The means of death to slay the player with.
   :return: ``False`` if no valid player could be determined, ``True`` otherwise.
   :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

.. function:: replace_items(item1, item2) -> bool

   Replaces target entity's item with specified one.

   .. seealso::
      :meth:`dev_print_items` for the different items in the current map.

   :param int | str item1: The item to be replaced. This can be an item id or classname for replacing several items of the same classname.
   :param int | str item2: The item to replace ``item1`` with. This can either be given by id or classname. If ``0``, ``item1`` is removed.
   :return: ``False`` if no valid item for ``item1`` could be determined, ``True`` otherwise.
   :raises ValueError: if the ``item1`` or ``item2`` are invalid, i.e. invalid item_id or classname or of improper types.

.. function:: dev_print_items()

   Prints all items and entity numbers to server console.

.. function:: force_weapon_respawn_time(respawn_time) -> bool

   Force all weapons to have a specified respawn time, overriding custom map respawn times set for them.

   :param int respawn_time: The new respawn time. Must be ``0`` or greater.
   :return: ``True``.
   :raises ValueError: if ``respawn_time`` is negative.

.. function:: get_targetting_entities(entity_id) -> list[int]

   get a list of entities that target a given entity

   .. seealso::
      :meth:`dev_print_items` for the different items in the current map.

   :param int entity_id: The entity to determine entities targetting it from.
   :return: The list of entity_ids for the entites that target the given ``entity_id``.

.. function:: parse_variables(varstr, ordered = False) -> dict[str, str]

   Parses strings of key-value pairs delimited by "\\" and puts them into a dictionary.

   :param str varstr: The string with variables.
   :param bool ordered: Whether it should use :class:`collections.OrderedDict` or not. (default: ``False``)

      .. deprecated:: Python3.7
         no longer necessary since Python changed its default behavior.

   :return: A dictionary with the variables added as key-value pairs.

.. function:: get_logger(plugin = None) -> logging.Logger

   Provides a logger that should be used by your plugin for debugging, info and error reporting. It will automatically output to both the server console as well as to a file.

   :param Plugin | str | None plugin: The plugin to get the logger for, or get the module logger if ``None``. (default: ``None``)
   :return: The logger in question.

.. function:: log_exception(plugin = None)

   Logs an exception using :func:`get_logger`. Call this in an except block.

   :param Plugin | str | None plugin: The plugin that is using the logger.

.. function:: handle_exception(exc_type, exc_value, exc_traceback)

   A handler for unhandled exceptions. Replaces :func:`sys.excepthook`.

.. function:: threading_excepthook(args)

   A handler for unhadled exceptions in threads. Replaces :func:`threading.excepthook`.

.. function:: uptime() -> datetime.timedelta

   Returns a :class:`datetime.timedelta` instance of the time since initialized.

   :return: :class:`datetime.timedelta` of the time since initialization.

.. function:: owner() -> int | None

   Returns the SteamID64 of the owner. This is set in the config.

   :return: the SteamID64 of the owner, or ``None`` if none is set or set to an invalid value.

.. function:: stats_listener() -> StatsListener

   Returns the :class:`shinqlx.StatsListener` instance used to listen for stats.

   :return: :class:`shinqlx.StatsListener` instance used to listen for stats from the server.

.. function:: set_cvar_once(name, value, flags = 0) -> bool

   Sets a cvar if no cvar is already set.

   .. seealso::
      :ref:`cvar_flags` for an explanation of the different flag values.

   :param str cvar: The name of the cvar to set.
   :param str value: The value to set the cvar to.
   :param int | None flags: The flags to set if, and only if, the cvar does not exist and has to be created. (default: ``None``)
   :return: ``True`` if a new cvar was created, ``False`` if an existing cvar was left unchanged.

.. function:: set_cvar_limit_once(name, value, minimum, maximum, flags = 0) -> bool

   Sets a cvar with minimum and maximum values if no cvar is already set.

   .. seealso::
      :ref:`cvar_flags` for an explanation of the different flag values.

   :param str cvar: The name of the cvar to set.
   :param str value: The value to set the cvar to.
   :param int | float minimum: The minimum value of the cvar.
   :param int | float maximum: The maximum value of the cvar.
   :param int | None flags: The flags to set if, and only if, the cvar does not exist and has to be created. (default: ``None``)
   :return: ``True`` if a new cvar was created, ``False`` if an existing cvar was left unchanged.

.. function:: set_plugins_version(path)

   Gathers and sets :data:`__plugins_version__` internally.

   :param str path: Path to the plugins-folder.

.. function:: set_map_subtitles()

   Internally sets the long map name, map author and second subline. Called every time a map change occurs.

.. decorator:: next_frame

   Decorator for functions that shall be processed when the next game frame is handled, i.e. when the current event has been processed by the engine.

.. decorator:: delay(time)

   Delay a function call a certain amount of time.

   .. note::
      It cannot guarantee you that it will be called right as the timer
      expires, but unless some plugin is for some reason blocking, then
      you can expect it to be called practically as soon as it expires.

   :param float time: The number of seconds before the function should be called.

.. decorator:: thread

   Starts a thread with the function passed as its target. If a function decorated with this is called within a function also decorated, it will **not** create a second thread.

.. function:: load_preset_plugins()

   Load the preset plugins. Used internally during intialization.

.. exception:: PluginLoadError

.. function:: load_plugin(plugin)

   Load a plugin.

   :param str plugin: The name of the plugin to load.
   :raises PluginLoadError: if the plugin could not be loaded.

.. exception:: PluginUnloadError

.. function:: unload_plugin(plugin)

   Unload a plugin.

   :param str plugin: The plugin to unload.
   :raises PluginUnloadError: if the plugin was not loaded before or another problem occured.

.. function:: reload_plugin(plugin)

   Reload a plugin.

   :param str plugin: The name of the plugin to reload.
   :raises PluginLoadError: if the plugin could not be loaded after being unloaded.

.. function:: initialize_cvars()

   Called intnerally after the server has been loaded up to initialize a couple of default cvars.

.. function:: initialize()

   Called internally when the server starts up.

.. function:: late_init()

   Called internally when everythhing else has been set up.

Constants
=========

.. data:: __version__
   :type: str

   :class:`str` of the currently installed version representation.

.. data:: __version_info__
   :type: tuple[int, int, int]

   Version info tuple.

.. data:: __plugins_version__
   :type: str

   :class:`str` of the currently installed plugins basis.

.. data:: DEBUG
   :type: bool

   Flag whether shinqlx is running in debug mode.

.. data:: GAMETYPES
   :type: dict[int, str]

   dictionary mapping integer gametypes to more readable strings.

.. data:: GAMETYPES_SHORT
   :type: dict[int, str]

   dictionary mapping integer gametypes to short gametype names.

.. data:: WEAPONS
   :type: dict[int, str]

   dictionary mapping integer weapons to their short names.

.. data:: DEFAULT_PLUGINS
   :type: tuple[str, ...]

   Names of the default plugins being loaded if none are configured otherwise, or ``DEFAULT`` is used.

Return values for hooks and commands
------------------------------------

.. data:: RET_NONE
   :type: int

   Equal to ``None`` return for hooks and commands.

.. data:: RET_STOP
   :type: int

   Stop execution of event handlers within Python.

.. data:: RET_STOP_EVENT
   :type: int

   Only stop the event, but let other handlers process it.

.. data:: RET_STOP_ALL
   :type: int

   Stop execution at an engine level. SCARY STUFF!

.. data:: RET_USAGE
   :type: int

   Used for commands. Replies to the channel with a command's usage.

Priority values for hooks and commands
--------------------------------------

.. data:: PRI_HIGHEST
   :type: int

.. data:: PRI_HIGH
   :type: int

.. data:: PRI_NORMAL
   :type: int

.. data:: PRI_LOW
   :type: int

.. data:: PRI_LOWEST
   :type: int

.. _cvar_flags:

CVar flags
----------

Descriptions from the quake live C-source comments.

.. data:: CVAR_ARCHIVE
   :type: int

   set to cause it to be saved to vars.rc used for system variables, not for player specific configurations

.. data:: CVAR_USERINFO
   :type: int

   sent to server on connect or change

.. data:: CVAR_SERVERINFO
   :type: int

   sent in response to front end requests

.. data:: CVAR_SYSTEMINFO
   :type: int

   these cvars will be duplicated on all clients

.. data:: CVAR_INIT
   :type: int

   don't allow change from console at all, but can be set from the command line

.. data:: CVAR_LATCH
   :type: int

   will only change when C code next does a Cvar_Get(), so it can't be changed without proper initialization.  modified will be set, even though the value hasn't changed yet

.. data:: CVAR_ROM
   :type: int

   display only, cannot be set by user at all

.. data:: CVAR_USER_CREATED
   :type: int

   created by a set command

.. data:: CVAR_TEMP
   :type: int

   can be set even when cheats are disabled, but is not archived

.. data:: CVAR_CHEAT
   :type: int

   can not be changed if cheats are disabled

.. data:: CVAR_NORESTART
   :type: int

   do not clear when a cvar_restart is issued

.. _privileges:

Privileges
----------

.. data:: PRIV_NONE
   :type: int

.. data:: PRIV_MOD
   :type: int

.. data:: PRIV_ADMIN
   :type: int

.. data:: PRIV_ROOT
   :type: int

.. data:: PRIV_BANNED
   :type: int

.. _connection_states:

Connection states
-----------------

Descriptions from the quake live C-source comments.

.. data:: CS_FREE
   :type: int

   can be reused for a new connection

.. data:: CS_ZOMBIE
   :type: int

   client has been disconnected, but don't reuse connection for a couple seconds

.. data:: CS_CONNECTED
   :type: int

   has been assigned to a client_t, but no gamestate yet

.. data:: CS_PRIMED
   :type: int

   gamestate has been sent, but client hasn't sent a usercmd

.. data:: CS_ACTIVE
   :type: int

   client is fully in game

.. data:: CONNECTION_STATES
   :type: dict[int, str]

   dictionary mapping integer connection states to more readable strings.

.. _teams:

Teams
-----

.. data:: TEAM_FREE
   :type: int

.. data:: TEAM_RED
   :type: int

.. data:: TEAM_BLUE
   :type: int

.. data:: TEAM_SPECTATOR
   :type: int

.. data:: TEAMS
   :type: dict[int, str]

   dictionary mapping integer teams to more readable strings.

.. _means_of_death:

Means of Death
--------------

.. data:: MOD_UNKNOWN
   :type: int

.. data:: MOD_SHOTGUN
   :type: int

.. data:: MOD_GAUNTLET
   :type: int

.. data:: MOD_MACHINEGUN
   :type: int

.. data:: MOD_GRENADE
   :type: int

.. data:: MOD_GRENADE_SPLASH
   :type: int

.. data:: MOD_ROCKET
   :type: int

.. data:: MOD_ROCKET_SPLASH
   :type: int

.. data:: MOD_PLASMA
   :type: int

.. data:: MOD_PLASMA_SPLASH
   :type: int

.. data:: MOD_RAILGUN
   :type: int

.. data:: MOD_LIGHTNING
   :type: int

.. data:: MOD_BFG
   :type: int

.. data:: MOD_BFG_SPLASH
   :type: int

.. data:: MOD_WATER
   :type: int

.. data:: MOD_SLIME
   :type: int

.. data:: MOD_LAVA
   :type: int

.. data:: MOD_CRUSH
   :type: int

.. data:: MOD_TELEFRAG
   :type: int

.. data:: MOD_FALLING
   :type: int

.. data:: MOD_SUICIDE
   :type: int

.. data::MOD_TARGET_LASER
   :type: int

.. data:: MOD_TRIGGER_HURT
   :type: int

.. data:: MOD_NAIL
   :type: int

.. data:: MOD_CHAINGUN
   :type: int

.. data:: MOD_PROXIMITY_MINE
   :type: int

.. data:: MOD_KAMIKAZE
   :type: int

.. data:: MOD_JUICED
   :type: int

.. data:: MOD_GRAPPLE
   :type: int

.. data:: MOD_SWITCH_TEAMS
   :type: int

.. data:: MOD_THAW
   :type: int

.. data:: MOD_LIGHTNING_DISCHARGE
   :type: int

.. data:: MOD_HMG
   :type: int

.. data:: MOD_RAILGUN_HEADSHOT
   :type: int

.. _damage_flags:

Damage flags
------------

Descriptions from the quake live C-source comments.

.. data:: DAMAGE_RADIUS
   :type: int

   damage was indirect

.. data:: DAMAGE_NO_ARMOR
   :type: int

   armour does not protect from this damage

.. data:: DAMAGE_NO_KNOCKBACK
   :type: int

   do not affect velocity, just view angles

.. data:: DAMAGE_NO_PROTECTION
   :type: int

   armor, shields, invulnerability, and godmode have no effect

.. data:: DAMAGE_NO_TEAM_PROTECTION
   :type: int

   armor, shields, invulnerability, and godmode have no effect
