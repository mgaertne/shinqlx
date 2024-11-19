########
Channels
########

.. _channels:
.. currentmodule:: shinqlx

.. class:: AbstractChannel(name)

   :param str name: The name of the channel.

   An abstract class of a chat channel. A chat channel being a source of a message.

   Chat channels must implement reply(), since that's the whole point of having a chat channel as a class. Makes it quite convenient when dealing with commands and such, while allowing people to implement their own channels, opening the possibilites for communication with the bot through other means than just chat and console (e.g. web interface).

   Say "ChatChannelA" and "ChatChannelB" are both subclasses of this, and "cca" and "ccb" are instances, the default implementation of "cca == ccb" is comparing __repr__(). However, when you register a command and list what channels you want it to work with, it'll use this class' __str__(). It's important to keep this in mind if you make a subclass. Say you have a web interface that supports multiple users on it simulaneously. The right way would be to set "name" to something like "webinterface", and then implement a __repr__() to return something like "webinterface user1".

   .. property:: name
      :type: str

      The name of the channel. **Read-only**.

   .. method:: reply(msg, limit = 100, delimiter = " ")
      :abstractmethod:

      Send a message to this channel. The message is split along ``delimiter`` if it exceeds ``limit`` characters.

      :param str msg: The message to send.
      :param int limit: The maximum of characters for each message part if ``msg`` is longer. (default: ``100``)
      :param str delimiter: The delimiter to use to split longer messages. (default: ``" "``)

   .. method:: split_long_lines(msg, limit = 100, delimiter = " ") -> list[str]

      Helper function to split up longer messages. The msg is split along ``delimiter`` and returned in chunks of maximum ``limit`` characters each.

      :param str msg: The message to split.
      :param int limit: The maximum of characters for each message part. (default: ``100``)
      :param str delimiter: The delimiter to use to split longer messages. (default: ``" "``)
      :return: ``msg`` split up properly in a list of :class:`str`.

.. data:: MAX_MSG_LENGTH
   :type: int

   The maximum message length. Messages that are longer are split accordingly for :class:`Chatchannel` s.

.. class:: ChatChannel(name = "chat", fmt = 'print "{}\n"\n')

   :param str name: The name of the channel.
   :param str fmt: Format string to be used when issuing :meth:`send_server_command` to send the message.

   An abstract subclass of :class:`shinqlx.AbstractChannel` for chat to and from the server.

   .. method:: recipients() -> list[int] | None
      :abstractmethod:

      The ids of the recipients for this channel. Used by :meth:`reply`.

      :return: a list of ids of the recipients for this channel, or ``None`` if the message should go to all players.

   .. method:: reply(msg, limit = 100, delimiter = " ")

      Send a message to this channel. The message is split along ``delimiter`` if it exceeds ``limit`` characters.

      :param str msg: The message to send.
      :param int limit: The maximum of characters for each message part if ``msg`` is longer. (default: ``100``)
      :param str delimiter: The delimiter to use to split longer messages. (default: ``" "``)

.. class:: TeamChatChannel(team = "all"", name = "chat", fmt = 'print "{}\n"\n')

   :param str team: The name of the team for this channel.
   :param str name: The name of the channel.
   :param str fmt: Format string to be used when issuing :meth:`send_server_command` to send the message.

   A subclass of :class:`shinqlx.ChatChannel` for team chat to and from the server.

   .. method:: recipients() -> list[int] | None

      The ids of the recipients for this channel. Used by :meth:`ChatChannel.reply()`.

      :return: a list of ids of the recipients for this channel, or ``None`` if the message should go to all players.

.. class:: TellChannel(player)

   :param str | int | Player player: The player to interact with.

   A subclass of :class:`shinqlx.ChatChannel` private in-game messages..

   .. method:: recipients() -> list[int] | None

      The ids of the recipients for this channel. Used by :meth:`ChatChannel.reply()`.

      :return: a list of ids of the recipients for this channel.

.. class:: ConsoleChannel()

   A subclass of :class:`shinqlx.AbstractChannel` that prints to the console.

   .. method:: reply(msg)

      Send a message to this channel.

      :param str msg: The message to send.

.. class:: ClientCommandChannel(player)

   :param str | int | Player player: The player to interact with.

   A subclass of :class:`shinqlx.AbstractChannel` that wraps a TellChannel, but with its own name.

   .. method:: reply(msg, limit = 100, delimiter = " ")

      Send a message to this channel. The message is split along ``delimiter`` if it exceeds ``limit`` characters.

      :param str msg: The message to send.
      :param int limit: The maximum of characters for each message part if ``msg`` is longer. (default: ``100``)
      :param str delimiter: The delimiter to use to split longer messages. (default: ``" "``)


Constants
=========

.. data:: CHAT_CHANNEL
   :type: TeamChatChannel

   General chat channel. Messages are sent to all players on the server.

.. data:: RED_TEAM_CHAT_CHANNEL
   :type: TeamChatChannel

   Chat channel for messages to the red team.

.. data:: BLUE_TEAM_CHAT_CHANNEL
   :type: TeamChatChannel

   Chat channel for messages to the blue team.

.. data:: FREE_TEAM_CHAT_CHANNEL
   :type: TeamChatChannel

   Chat channel for messages to the free team.

.. data:: SPECTATOR_TEAM_CHAT_CHANNEL
   :type: TeamChatChannel

   Chat channel for messages to spectators only.

.. data:: CONSOLE_CHANNEL
   :type: ConsoleChannel

   Chat channel for console messages.
