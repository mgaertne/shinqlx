from typing import overload, TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Mapping
    from datetime import timedelta
    from logging import Logger

    from redis import Redis as redisRedis, ConnectionPool
    from shinqlx import Plugin, Player

class AbstractDatabase:
    _counter: int
    plugin: Plugin

    def __init__(self, plugin: Plugin) -> None: ...
    def __del__(self) -> None: ...
    @property
    def logger(self) -> Logger: ...
    def set_permission(self, player: Player | int | str, level: int) -> None: ...
    def get_permission(self, player: Player | int | str) -> int: ...
    def has_permission(self, player: Player | int | str, level: int = ...) -> bool: ...
    def set_flag(
        self, player: Player | int | str, flag: str, value: bool = ...
    ) -> None: ...
    def clear_flag(self, player: Player | int | str, flag: str) -> None: ...
    def get_flag(
        self, player: Player | int | str, flag: str, default: bool = ...
    ) -> bool: ...
    def connect(self) -> redisRedis | None: ...
    def close(self) -> None: ...

class Redis(AbstractDatabase):
    _conn: redisRedis | None
    _pool: ConnectionPool | None
    _pass: str

    def __del__(self) -> None: ...
    def __contains__(self, key: str) -> bool: ...
    def __getitem__(self, key: str) -> str: ...
    def __setitem__(self, key: str, item: str | int) -> None: ...
    def __delitem__(self, key: str) -> None: ...
    def __getattr__(self, attr: str) -> str: ...
    @property
    def r(self) -> redisRedis: ...
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
    ) -> redisRedis | None: ...
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
    def zincrby(self, name: str, *, value: str, amount: int | float = ...) -> float: ...
    def setex(self, name: str, *, value: str, time: int | timedelta) -> bool: ...
    def lrem(self, name: str, *, value: str, count: int = ...) -> int: ...
