from typing import TYPE_CHECKING
from shinqlx import AbstractChannel

if TYPE_CHECKING:
    from shinqlx import Player, Command

class CommandInvoker:
    _commands: tuple[
        list[Command], list[Command], list[Command], list[Command], list[Command]
    ]

    def __init__(self) -> None: ...
    @property
    def commands(self) -> list[Command]: ...
    def add_command(self, command: Command, priority: int) -> None: ...
    def remove_command(self, command: Command) -> None: ...
    def is_registered(self, command: Command) -> bool: ...
    def handle_input(
        self, player: Player, msg: str, channel: AbstractChannel
    ) -> bool: ...

COMMANDS: CommandInvoker
