########
Commands
########

.. _commands:
.. currentmodule:: shinqlx

.. class:: Command(plugin, name, handler, permission, channels, exclude_channels, client_cmd_pass, client_cmd_perm, prefix, usage)

   :param Plugin plugin: The plugin that created this command.
   :param str | Iterable[str] name: The name or names for the added commands.
   :param Callable handler: The handler for the command. See below for a prototype.
   :param int permission: The minimum permission a player trying to invoke this command needs.
   :param Iterable[AbstractChannel] channels: The channels this command can be triggered from.
   :param Iterable[Abstractchannel] exclude_channels: Explicit channels this command cannot be triggered from.
   :param int priority: The priority for this command, valid values: :const:`PRI_LOWEST <shinqlx.PRI_LOWEST>`, :const:`PRI_LOW <shinqlx.PRI_LOW>`, :const:`PRI_NORMAL <shinqlx.PRI_NORMAL>`, :const:`PRI_HIGH <shinqlx.PRI_HIGH>`, :const:`PRI_HIGHEST <shinqlx.PRI_HIGHEST>`.
   :param bool client_cmd_pass: Flag whether this command should be passed to ``client_command`` and the general quake live engine.
   :param int client_cmd_perm: The minimum permission level needed when triggering this command via the ``client_command`` channel.
   :param bool prefix: Flag indicating whether this commands needs to be prefixed with the character in ``qlx_commandPrefix``
   :param str usage: Usage message shown to a player trying to invoke this command when the ``handler`` returns ``RET_USAGE``

   A class representing an input-triggered command.

   Has information about the command itself, its usage, when and who to call when action should be taken.

   .. hint::
      Prototype for the ``handler`` Callable:

      .. code-block:: python

         def handle_cmd(
             self,
             player: Player,
             msg: str,
             channel: Abstractchannel
           ) ->  None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL | RET_USAGE

   .. method:: execute(player, msg, channel) -> None | RET_NONE | RET_STOP | RET_STOP_EVENT | RET_STOP_ALL | RET_USAGE

      Execute this command.

      :param Player player: The player to pass to the ``handler``.
      :param str msg: The message to pass to the ``handler``.
      :param AbstractChannel channel: The channel to pass to the ``handler``.
      :return: forwards what ``handler`` returned.

   .. method:: is_eligible_name(name) -> bool

      Checks whether the given ``name`` is eligible for this command to trigger.

      :param str name: The name to check for eligibility.
      :return: whether the name is eligible to trigger this command to execute.

   .. method:: is_eligible_channel(channel) -> bool

      Check if a chat channel is one this command should execute in. Exclude channels take precedence.

      :param AbstractChannel channel: The channel to check for eligibility.
      :return: whether the channel is eligible to trigger this command to execute.

   .. method:: is_eligible_player(player, is_client_cmd) -> bool

      Check if a player has the rights to execute the command.

      :param Player player: The player to check for eligibility.
      :param bool is_client_cmd: Whether the command was triggered via :class:`ClientCommandChannel`, rather than the general :class:`ChatChannel`.
      :return: whether the player is eligible to trigger this command to execute.

.. class:: CommandInvoker()

   Holds all commands and executes them whenever we get input and should execute.

   .. property:: commands() -> list[Command]

      The commands configured in this CommandInnvoker. **Read-only**.

   .. method:: add_command(command, priority)

      Adds a command with the given priority.

      :param Command command: The command to add.
      :param int priority: The priority for the added command.
      :raises ValueError: if a command with the same name is already registered.

   .. method:: remove_command(command)

      Removes a command that was added with :meth:`add_command`.

      :param Command command: The command to remove.
      :raises ValueError: if the command was not previously registered.

   .. method:: is_registered(command) -> bool

      Check if a command is already registed. Commands are unique by (command.name, command.handler).

      :param Command command: The command to check.
      :return: whether the command is already registered.

   .. method:: handle_input(player, msg, channel) -> bool

      Checks all registered commands, and calls their :meth:`Command.execute` function if they are eligible.

      :param Player player: The player that put the message in.
      :param str msg: The message the player sent.
      :param AbstractChannel channel: The channel where the player sent the message.
      :return: whether to pass the command on to the underlying quake live engine (``True``), or not (``False``).

.. data:: COMMANDS
   :type: CommandInvoker

   The command invoker used through-out the server holding all registered commands.
