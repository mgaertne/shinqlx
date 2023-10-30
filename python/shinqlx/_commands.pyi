from abc import abstractmethod
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Pattern, Callable, Iterable
    from shinqlx import Player, Plugin

MAX_MSG_LENGTH: int
re_color_tag: Pattern

class AbstractChannel:
    _name: str

    def __init__(self, name: str) -> None: ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    def __ne__(self, other: object) -> bool: ...
    @property
    def name(self) -> str: ...
    def reply(self, msg: str, limit: int = ..., delimiter: str = ...) -> None: ...
    def split_long_lines(
        self, msg: str, limit: int = ..., delimiter: str = ...
    ) -> list[str]: ...

class ChatChannel(AbstractChannel):
    fmt: str

    def __init__(self, name: str = ..., fmt: str = ...) -> None: ...
    @abstractmethod
    def receipients(self) -> list[int] | None: ...
    def reply(self, msg: str, limit: int = ..., delimiter: str = ...) -> None: ...

class TeamChatChannel(ChatChannel):
    team: str
    def __init__(self, team: str = ..., name: str = ..., fmt: str = ...) -> None: ...
    def receipients(self) -> list[int] | None: ...

class TellChannel(ChatChannel):
    recipient: str | int | Player

    def __init__(self, player: str | int | Player) -> None: ...
    def __repr__(self) -> str: ...
    def receipients(self) -> list[int] | None: ...

class ConsoleChannel(AbstractChannel):
    def __init__(self) -> None: ...
    def reply(self, msg: str, limit: int = ..., delimiter: str = ...) -> None: ...

class ClientCommandChannel(AbstractChannel):
    recipient: Player
    tell_channel: ChatChannel

    def __init__(self, player: Player) -> None: ...
    def __repr__(self) -> str: ...
    def reply(self, msg: str, limit: int = ..., delimiter: str = ...) -> None: ...

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
CHAT_CHANNEL: AbstractChannel
RED_TEAM_CHAT_CHANNEL: AbstractChannel
BLUE_TEAM_CHAT_CHANNEL: AbstractChannel
FREE_CHAT_CHANNEL: AbstractChannel
SPECTATOR_CHAT_CHANNEL: AbstractChannel
CONSOLE_CHANNEL: AbstractChannel
