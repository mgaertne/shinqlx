from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Type
    from sched import scheduler
    from queue import Queue
    from types import TracebackType

    from shinqlx import AbstractChannel

frame_tasks: scheduler
next_frame_tasks: Queue

_zmq_warning_issued: bool
_first_game: bool
_ad_round_number: int

_print_redirection: AbstractChannel | None
_print_buffer: str

def handle_frame() -> bool | None: ...
def handle_new_game(is_restart: bool) -> bool | None: ...
def handle_set_configstring(index: int, value: str) -> bool | None: ...
def handle_console_print(text: str | None) -> bool | str | None: ...
def redirect_print(channel: AbstractChannel) -> PrintRedirector: ...
def register_handlers() -> None: ...

class PrintRedirector:
    channel: AbstractChannel

    def __init__(self, _channel: AbstractChannel) -> None: ...
    def __enter__(self) -> None: ...
    def __exit__(
        self,
        exc_type: Type[BaseException],
        exc_value: BaseException,
        exc_traceback: TracebackType | None,
    ) -> None: ...
    def flush(self) -> None: ...
