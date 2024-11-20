.. _handlers:
.. currentmodule:: shinqlx

########
Handlers
########

Low-level handlers. These are all called by the Rust-code, not within Python.

.. function:: handle_rcon(cmd) -> bool | None

   Console commands that are to be processed as regular pyshinqlx commands as if the owner executes it. This allows the owner to interact with the Python part of shinqlx without having to connect.

   :param str cmd:
   :return: ``True``

.. function:: handle_client_command(client_id, cmd) -> bool | str

   Client commands are commands such as ```"say"``, ```"say_team"``, ```"scores"``, ```"disconnect"`` and so on. This function parses those and passes it on to the event dispatcher.

   :param int client_id: The client identifier.
   :param str cmd: The command being run by the client.
   :return: Whether to pass on the client command, or a changed client command that will be passed on.

.. function:: handle_server_command(client_id, cmd) -> bool | str

   Handles commands sent by the server.

   :param int client_id: The client identifier.
   :param str cmd: The command being run.
   :return: Whether to pass on the client command, or a changed client command that will be passed on.

.. function::  handle_frame() -> bool | None

   This will be called every frame. To allow threads to call stuff from the main thread, tasks can be scheduled using the :func:`shinqlx.next_frame` decorator and have it be executed here.

   :return: ``True`` if an exception occurred during handling, ``None`` otherwise.

.. function:: handle_new_game(is_restart) -> bool | None

   This handler is called whenever a new game is initialized.

   :param bool is_restart: Whether the map is just restarted (``True``) or the whole VM has been re-initialized (``False``).
   :return: ``True`` if an exception occurred during handling, ``None`` otherwise.

.. function:: handle_set_configstring(client_id, value) -> bool | None

   Called whenever the server tries to set a configstring. Can return ``False`` to stop the event.

   :param int index: The configstring index to set.
   :param str value: The value the configstring should be set to.
   :return: ``True`` if an exception occurred during handling, ``None`` otherwise.

.. function:: handle_player_connect(cilent_id, is_bot) -> bool | str | None

   This will be called whenever a player tries to connect. If the dispatcher returns False, it will not allow the player to connect and instead show them a message explaining why. The default message is "You are banned from this server.".

   :param int client_id: The client identifier.
   :param bool is_bot: Whether or not the player is a bot.
   :return: ``True`` if an exception occurred during handling, a changed value, if a handler returned a different value while processing, ``None`` otherwise.

.. function:: handple_player_loaded(client_id) -> bool | None

   This will be called whenever a player has connected and finished loading, meaning it'll go off a bit later than the usual "X connected" messages. This will not trigger on bots.

   :param int client_id: The client identifier.
   :return: ``True`` if an exception occurred during handling, ``None`` otherwise.

.. function:: handle_player_disconnect(client_id, reason) -> bool | None

   This will be called whenever a player disconnects.

   :param int client_id: The client identifier.
   :param str reason: The reason for the disconnect
   :return: ``True`` if an exception occurred during handling, ``None`` otherwise.

.. function:: handle_player_spawn(client_id) -> bool | None

   Called when a player spawns. Note that a spectator going in free spectate mode makes the client spawn, so you'll want to check for that if you only want "actual" spawns.

   :param int client_id: The client identifier.
   :return: ``True`` if an exception occurred during handling, ``None`` otherwise.

.. function:: handle_kamikaze_use(client_id) -> bool | None

   This will be called whenever player uses kamikaze item.

   :param int client_id: The client identifier.
   :return: ``True`` if an exception occurred during handling, ``None`` otherwise.

.. function:: handle_kamikaze_explode(client_id, is_used_on_demand) -> bool | None

   This will be called whenever kamikaze explodes.

   :param int client_id: The client identifier.
   :param bool is_used_on_demand: Whether kamikaze is used on demand.
   :return: ``True`` if an exception occurred during handling, ``None`` otherwise.

.. function:: handle_damage(target_id, attacker_id, damage, dflags, mod) -> bool | None

   This will be called whenever damage is happening in the game.

   :param int target_id: The target identifier of the inflicted damage
   :param int attacker_id: The attacker identifier of the inflicted damage
   :param int damage: The raw damage amount before applying handicaps, etc.
   :param int dflags: The damage flags. See :ref:`damage_flags`.
   :param int mod: The means of death used. See :ref:`means_of_death`
   :return: ``True`` if an exception occurred during handling, ``None`` otherwise.

.. function:: handle_console_print(text) -> bool | str | None

   Called whenever the server prints something to the console and when rcon is used.

   :param str text: The text to be printed.
   :return: ``True`` if an exception occurred during handling, a changed value, if a handler returned a different value while processing, ``None`` otherwise.

.. function:: redirect_print(channel)

   Redirects print output to a channel. Useful for commands that execute console commands and want to redirect the output to the channel instead of letting it go to the console.

   To use it, use the return value with the "with" statement.

   .. code-block:: python

       def cmd_echo(self, player, msg, channel):
           with shinqlx.redirect_print(channel):
               shinqlx.console_command("echo {}".format(" ".join(msg)))

   :param AbstractChannel channel: The channel to redirect printed messages to.

.. function:: register_handlers()

   Registers the main handlers once the server has been initialized properly.

.. class:: PrintRedirector(channel)

   A helper :class:`ContextManager <contextlib.AbstractContextManager>` for :func:`redirect_print`

   :param AbstractChannel channel: The channel to redirect printed messages to.

   .. method:: flush

      Flushes the print queue.
