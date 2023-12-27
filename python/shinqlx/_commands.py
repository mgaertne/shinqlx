import re

import shinqlx
from shinqlx import AbstractChannel, ChatChannel, ConsoleChannel

re_color_tag = re.compile(r"\^[0-7]")


# ====================================================================
#                             CHANNELS
# ====================================================================
class TeamChatChannel(ChatChannel):
    """A channel for chat to and from the server."""

    def __new__(cls, team="all", name="chat", fmt='print "{}\n"\n'):
        return super().__new__(cls, name=name, fmt=fmt)

    def __init__(self, team="all", name="chat", fmt='print "{}\n"\n'):
        super().__init__()
        self.team = team

    def receipients(self):
        if self.team == "all":
            return None

        return [
            player.id
            for player in shinqlx.Player.all_players()
            if player.team == self.team
        ]


class TellChannel(ChatChannel):
    """A channel for private in-game messages."""

    def __new__(cls, player):
        return super().__new__(cls, name="tell", fmt='print "{}\n"\n')

    def __init__(self, player):
        super().__init__()
        self.recipient = player

    def __repr__(self):
        player = shinqlx.Plugin.player(self.recipient)
        if player is None:
            return ""
        return f"tell {player.steam_id}"

    def receipients(self):
        cid = shinqlx.Plugin.client_id(self.recipient)
        if cid is None:
            raise ValueError("Invalid recipient.")
        return [cid]


class ClientCommandChannel(AbstractChannel):
    """Wraps a TellChannel, but with its own name."""

    def __new__(cls, player):
        return super().__new__(cls, "client_command")

    def __init__(self, player):
        super().__init__()
        self.recipient = player
        self.tell_channel = TellChannel(player)

    def __repr__(self):
        player = shinqlx.Plugin.player(self.recipient)
        if player is None:
            return ""

        return f"client_command {player.id}"

    def reply(self, msg, limit=100, delimiter=" "):
        self.tell_channel.reply(msg, limit, delimiter)


# ====================================================================
#                              COMMANDS
# ====================================================================
class Command:
    """A class representing an input-triggered command.

    Has information about the command itself, its usage, when and who to call when
    action should be taken.

    """

    def __init__(
        self,
        plugin,
        name,
        handler,
        permission,
        channels,
        exclude_channels,
        client_cmd_pass,
        client_cmd_perm,
        prefix,
        usage,
    ):
        if not (channels is None or hasattr(channels, "__iter__")):
            raise ValueError("'channels' must be a finite iterable or None.")
        if not (channels is None or hasattr(exclude_channels, "__iter__")):
            raise ValueError("'exclude_channels' must be a finite iterable or None.")
        self.plugin = plugin  # Instance of the owner.

        # Allow a command to have alternative names.
        if isinstance(name, (list, tuple)):
            self.name = [n.lower() for n in name]
        else:
            self.name = [name]
        self.handler = handler
        self.permission = permission
        self.channels = list(channels) if channels is not None else []
        self.exclude_channels = (
            list(exclude_channels) if exclude_channels is not None else []
        )
        self.client_cmd_pass = client_cmd_pass
        self.client_cmd_perm = client_cmd_perm
        self.prefix = prefix
        self.usage = usage

    def execute(self, player, msg, channel):
        logger = shinqlx.get_logger(self.plugin)
        logger.debug(
            "%s executed: %s @ %s -> %s",
            player.steam_id,
            self.name[0],
            self.plugin.name,
            channel,
        )
        return self.handler(player, msg.split(), channel)

    def is_eligible_name(self, name):
        if self.prefix:
            prefix = shinqlx.get_cvar("qlx_commandPrefix")
            if prefix is None:
                return False
            if not name.startswith(prefix):
                return False
            name = name[len(prefix):]

        return name.lower() in self.name

    def is_eligible_channel(self, channel):
        """Check if a chat channel is one this command should execute in.

        Exclude takes precedence.

        """
        if channel in self.exclude_channels:
            return False
        return not self.channels or channel.name in self.channels

    def is_eligible_player(self, player, is_client_cmd):
        """Check if a player has the rights to execute the command."""
        # Check if config overrides permission.
        perm = self.permission
        client_cmd_perm = self.client_cmd_perm

        if is_client_cmd:
            cvar_client_cmd = shinqlx.get_cvar("qlx_ccmd_perm_" + self.name[0])
            if cvar_client_cmd:
                client_cmd_perm = int(cvar_client_cmd)
        else:
            cvar = shinqlx.get_cvar("qlx_perm_" + self.name[0])
            if cvar:
                perm = int(cvar)

        if (
            player.steam_id == shinqlx.owner()
            or (not is_client_cmd and perm == 0)
            or (is_client_cmd and client_cmd_perm == 0)
        ):
            return True

        if self.plugin.db is None:
            return False

        player_perm = self.plugin.db.get_permission(player)
        if is_client_cmd:
            return player_perm >= client_cmd_perm
        return player_perm >= perm


class CommandInvoker:
    """Holds all commands and executes them whenever we get input and should execute."""

    def __init__(self):
        self._commands: tuple[
            list[Command], list[Command], list[Command], list[Command], list[Command]
        ] = (
            [],
            [],
            [],
            [],
            [],
        )

    @property
    def commands(self):
        c = []
        for cmds in self._commands:
            c.extend(cmds)

        return c

    def add_command(self, command, priority):
        if self.is_registered(command):
            raise ValueError("Attempted to add an already registered command.")

        self._commands[priority].append(command)

    def remove_command(self, command):
        if not self.is_registered(command):
            raise ValueError("Attempted to remove a command that was never added.")

        for priority_level in self._commands:
            for cmd in priority_level.copy():
                if cmd == command:
                    priority_level.remove(cmd)
                    return

    def is_registered(self, command):
        """Check if a command is already registed.

        Commands are unique by (command.name, command.handler).

        """
        for priority_level in self._commands:
            for cmd in priority_level:
                if command.name == cmd.name and command.handler == cmd.handler:
                    return True

        return False

    def handle_input(self, player, msg, channel):
        if not msg.strip():
            return False

        name = msg.strip().split(" ", 1)[0].lower()
        is_client_cmd = channel == "client_command"
        pass_through = True

        for priority_level in self._commands:
            for cmd in priority_level:
                if (
                    cmd.is_eligible_name(name)
                    and cmd.is_eligible_channel(channel)
                    and cmd.is_eligible_player(player, is_client_cmd)
                ):
                    # Client commands will not pass through to the engine unless told to explicitly.
                    # This is to avoid having to return RET_STOP_EVENT just to not get the "unknown cmd" msg.
                    if is_client_cmd:
                        pass_through = cmd.client_cmd_pass

                    # Dispatch "command" and allow people to stop it from being executed.
                    if (
                        shinqlx.EVENT_DISPATCHERS["command"].dispatch(player, cmd, msg)
                        is False
                    ):
                        return True

                    res = cmd.execute(player, msg, channel)
                    if res == shinqlx.RET_STOP:
                        return False
                    if res == shinqlx.RET_STOP_EVENT:
                        pass_through = False
                    elif res == shinqlx.RET_STOP_ALL:
                        # C-level dispatchers expect False if it shouldn't go to the engine.
                        return False
                    elif res == shinqlx.RET_USAGE and cmd.usage:
                        channel.reply(f"^7Usage: ^6{name} {cmd.usage}")
                    elif res is not None and res != shinqlx.RET_NONE:
                        logger = shinqlx.get_logger(None)
                        logger.warning(
                            "Command '%s' with handler '%s' returned an unknown return value: %s",
                            cmd.name,
                            cmd.handler.__name__,
                            res,
                        )

        return pass_through


# ====================================================================
#                          MODULE CONSTANTS
# ====================================================================
COMMANDS = CommandInvoker()
CHAT_CHANNEL = TeamChatChannel(team="all", name="chat")
RED_TEAM_CHAT_CHANNEL = TeamChatChannel(team="red", name="red_team_chat")
BLUE_TEAM_CHAT_CHANNEL = TeamChatChannel(team="blue", name="blue_team_chat")
FREE_CHAT_CHANNEL = TeamChatChannel(team="free", name="free_chat")
SPECTATOR_CHAT_CHANNEL = TeamChatChannel(team="spectator", name="spectator_chat")
CONSOLE_CHANNEL = ConsoleChannel()
