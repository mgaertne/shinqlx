import shinqlx
from shinqlx import Command


# ====================================================================
#                              COMMANDS
# ====================================================================
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
