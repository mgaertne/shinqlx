from typing import TYPE_CHECKING, overload
from abc import ABC, abstractmethod

if TYPE_CHECKING:
    from typing import ClassVar, Mapping

    from datetime import timedelta
    from logging import Logger

    from shinqlx import Plugin, Player

    from redis import StrictRedis, ConnectionPool


class AbstractDatabase(ABC):
    plugin: Plugin

    def __new__(cls, plugin: Plugin) -> AbstractDatabase: ...

    @property
    def logger(self) -> Logger: ...

    @abstractmethod
    def set_permission(self, player: Player | int | str, level: int) -> None: ...

    @abstractmethod
    def get_permission(self, player: Player | int | str) -> int: ...

    @abstractmethod
    def has_permission(self, player: Player | int | str, level: int = ...) -> bool: ...

    @abstractmethod
    def set_flag(
            self, player: Player | int | str, flag: str, value: bool = ...
    ) -> None: ...

    def clear_flag(self, player: Player | int | str, flag: str) -> None: ...

    @abstractmethod
    def get_flag(
            self, player: Player | int | str, flag: str, default: bool = ...
    ) -> bool: ...

    @abstractmethod
    def connect(self) -> None: ...

    @abstractmethod
    def close(self) -> None: ...


class Redis(AbstractDatabase):
    _counter: ClassVar[int]

    _conn: StrictRedis | None
    _pool: ConnectionPool | None

    def __init__(self, plugin: Plugin) -> None: ...

    def __del__(self) -> None: ...

    def __contains__(self, key: str) -> bool: ...

    def __getitem__(self, key: str) -> str: ...

    def __setitem__(self, key: str, item: str | int) -> None: ...

    def __delitem__(self, key: str) -> None: ...

    def __getattr__(self, attr: str) -> str: ...

    def set_permission(self, player: Player | int | str, level: int) -> None: ...

    def get_permission(self, player: Player | int | str) -> int: ...

    def has_permission(self, player: Player | int | str, level: int = ...) -> bool: ...

    def set_flag(
            self, player: Player | int | str, flag: str, value: bool = ...
    ) -> None: ...

    def get_flag(
            self, player: Player | int | str, flag: str, default: bool = False
    ) -> bool: ...

    def connect(
            self,
            host: str | None = ...,
            database: int = ...,
            unix_socket: bool = ...,
            password: str | None = ...,
    ) -> None: ...

    def close(self) -> None: ...

    def mset(self, *args: dict, **kwargs: str | int | float | bool) -> bool: ...

    def msetnx(self, *args: dict, **kwargs: str | int | float | bool) -> bool: ...

    @overload
    async def zadd(
            self,
            name: str,
            *args: str | int | float,
            **kwargs: int | float,
    ) -> int: ...

    @overload
    async def zadd(
            self,
            name: str,
            mapping: Mapping[str, int | float],
            nx: bool = ...,
            xx: bool = ...,
            ch: bool = ...,
            incr: bool = ...,
            gt: int | float | None = ...,
            lt: int | float | None = ...,
    ) -> int: ...

    def zincrby(self, name: str, *, value: str, amount: int | float) -> float: ...

    def setex(self, name: str, *, value: str, time: int | timedelta) -> bool: ...

    def lrem(self, name: str, *, value: str, count: int) -> int: ...
