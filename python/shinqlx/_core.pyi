from typing import Protocol, TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Type, Callable
    from types import TracebackType, ModuleType
    from datetime import datetime, timedelta
    from logging import Logger

    from shinqlx import StatsListener, Plugin

class ExceptHookArgs(Protocol):
    exc_traceback: TracebackType
    exc_type: Type[BaseException]
    exc_value: BaseException

TEAMS: dict[int, str]
GAMETYPES: dict[int, str]
GAMETYPES_SHORT: dict[int, str]
CONNECTION_STATES: dict[int, str]
WEAPONS: dict[int, str]
DEFAULT_PLUGINS: tuple[str, ...]

_init_time: datetime
_stats: StatsListener
_modules: dict[str, ModuleType]

def parse_variables(varstr: str, ordered: bool = False) -> dict[str, str]: ...
def get_logger(plugin: Plugin | str | None = ...) -> Logger: ...
def _configure_logger() -> None: ...
def log_exception(plugin: Plugin | str | None = ...) -> None: ...
def handle_exception(
    exc_type: Type[BaseException],
    exc_value: BaseException,
    exc_traceback: TracebackType | None,
) -> None: ...
def threading_excepthook(args: ExceptHookArgs) -> None: ...
def uptime() -> timedelta: ...
def owner() -> int | None: ...
def stats_listener() -> StatsListener: ...
def set_plugins_version(path: str) -> None: ...
def load_preset_plugins() -> None: ...
def load_plugin(plugin: str) -> None: ...
def unload_plugin(plugin: str) -> None: ...
def reload_plugin(plugin: str) -> None: ...
def initialize_cvars() -> None: ...
def initialize() -> None: ...
def late_init() -> None: ...