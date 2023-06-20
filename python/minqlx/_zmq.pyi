from typing import TYPE_CHECKING

if TYPE_CHECKING:
    # noinspection PyPackageRequirements
    from zmq import Context, Socket

class StatsListener:
    done: bool
    address: str
    password: str | None

    context: Context
    socket: Socket
    _in_progress: bool

    def __init__(self) -> None: ...
    def keep_receiving(self) -> None: ...
    def stop(self) -> None: ...
