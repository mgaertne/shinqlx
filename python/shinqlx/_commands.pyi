from typing import TYPE_CHECKING
from shinqlx import AbstractChannel

if TYPE_CHECKING:
    from typing import Callable, Iterable
    from shinqlx import Player, Plugin

class Command:
    name: list[str]
    plugin: Plugin
    handler: Callable
    permission: int
    channels: list[AbstractChannel]
    exclude_channels: list[AbstractChannel]
    client_cmd_pass: bool
    client_cmd_perm: int
    prefix: bool
    usage: str

    def __init__(
        self,
        plugin: Plugin,
        name: str | Iterable[str],
        handler: Callable,
        permission: int,
        channels: Iterable[AbstractChannel] | None,
        exclude_channels: Iterable[AbstractChannel] | None,
        client_cmd_pass: bool,
        client_cmd_perm: int,
        prefix: bool,
        usage: str,
    ) -> None: ...
    def execute(
        self, player: Player, msg: str, channel: AbstractChannel
    ) -> int | None: ...
    def is_eligible_name(self, name: str) -> bool: ...
    def is_eligible_channel(self, channel: AbstractChannel) -> bool: ...
    def is_eligible_player(self, player: Player, is_client_cmd: bool) -> bool: ...

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
