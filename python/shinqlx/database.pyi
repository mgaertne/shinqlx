from shinqlx import AbstractDatabase
from typing import overload, TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Mapping, ClassVar
    from datetime import timedelta

    from redis import Redis as redisRedis, ConnectionPool
    from shinqlx import Plugin, Player


class Redis(AbstractDatabase):
    _counter: ClassVar[int]
    _conn: ClassVar[redisRedis | None]
    _pool: ClassVar[ConnectionPool | None]

    def __init__(self, plugin: Plugin) -> None: ...

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

    @overload
    def zincrby(
            self, name: str, value_or_amount: str, amount_or_value: int | float = ...
    ) -> float: ...

    @overload
    def zincrby(
            self, name: str, value_or_amount: int | float, amount_or_value: str
    ) -> float: ...

    @overload
    def setex(
            self, name: str, value_or_time: str, time_or_value: int | timedelta
    ) -> bool: ...

    @overload
    def setex(
            self, name: str, value_or_time: int | timedelta, time_or_value: str
    ) -> bool: ...

    @overload
    def lrem(self, name: str, value_or_count: str, num_or_value: int = ...) -> int: ...

    @overload
    def lrem(self, name: str, value_or_count: int, num_or_value: str) -> int: ...
