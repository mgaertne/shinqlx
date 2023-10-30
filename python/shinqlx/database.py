from datetime import timedelta
import redis

import shinqlx


# ====================================================================
#                          AbstractDatabase
# ====================================================================
# noinspection PyProtectedMember
class AbstractDatabase:
    # An instance counter. Useful for closing connections.
    _counter = 0

    def __init__(self, plugin):
        self.plugin = plugin
        self.__class__._counter += 1

    def __del__(self):
        self.__class__._counter -= 1

    @property
    def logger(self):
        return shinqlx.get_logger(self.plugin)

    def set_permission(self, player, level):
        """Abstract method. Should set the permission of a player.

        :raises: NotImplementedError

        """
        raise NotImplementedError("The base plugin can't do database actions.")

    def get_permission(self, player):
        """Abstract method. Should return the permission of a player.

        :returns: int
        :raises: NotImplementedError

        """
        raise NotImplementedError("The base plugin can't do database actions.")

    def has_permission(self, player, level=5):
        """Abstract method. Should return whether or not a player has more than or equal
        to a certain permission level. Should only take a value of 0 to 5, where 0 is
        always True.

        :returns: bool
        :raises: NotImplementedError

        """
        raise NotImplementedError("The base plugin can't do database actions.")

    def set_flag(self, player, flag, value=True):
        """Abstract method. Should set specified player flag to value.

        :raises: NotImplementedError

        """
        raise NotImplementedError("The base plugin can't do database actions.")

    def clear_flag(self, player, flag):
        """Should clear specified player flag."""
        return self.set_flag(player, flag, False)

    def get_flag(self, player, flag, default=False):
        """Abstract method. Should return specified player flag

        :returns: bool
        :raises: NotImplementedError

        """
        raise NotImplementedError("The base plugin can't do database actions.")

    def connect(self):
        """Abstract method. Should return a connection to the database. Exactly what a
        "connection" obviously depends on the database, so the specifics will be up
        to the implementation.

        A :class:`shinqlx.Plugin` subclass can set

        :raises: NotImplementedError

        """
        raise NotImplementedError("The base plugin can't do database actions.")

    def close(self):
        """Abstract method. If the database has a connection state, this method should
        close the connection.

        :raises: NotImplementedError

        """
        raise NotImplementedError("The base plugin can't do database actions.")


# ====================================================================
#                               Redis
# ====================================================================
# noinspection PyProtectedMember
class Redis(AbstractDatabase):
    """A subclass of :class:`shinqlx.AbstractDatabase` providing support for Redis."""

    # We only use the instance-level ones if we override the URI from the config.
    _conn = None
    _pool = None
    _pass = ""

    def __del__(self):
        super().__del__()
        self.close()

    def __contains__(self, key):
        return self.r.exists(key)

    def __getitem__(self, key):
        res = self.r.get(key)
        if res is None:
            raise KeyError(f"The key '{key}' is not present in the database.")
        return res

    def __setitem__(self, key, item):
        res = self.r.set(key, item)
        if res is False:
            raise RuntimeError("The database assignment failed.")

    def __delitem__(self, key):
        res = self.r.delete(key)
        if res == 0:
            raise KeyError(f"The key '{key}' is not present in the database.")

    def __getattr__(self, attr):
        return getattr(self.r, attr)

    @property
    def r(self):
        return self.connect()

    def set_permission(self, player, level):
        """Sets the permission of a player.

        :param: player: The player in question.
        :type: player: shinqlx.Player

        """
        if isinstance(player, shinqlx.Player):
            key = f"minqlx:players:{player.steam_id}:permission"
        else:
            key = f"minqlx:players:{player}:permission"

        self[key] = level

    def get_permission(self, player):
        """Gets the permission of a player.

        :param: player: The player in question.
        :type: player: shinqlx.Player, int
        :returns: int

        """
        if isinstance(player, shinqlx.Player):
            steam_id = player.steam_id
        elif isinstance(player, int):
            steam_id = player
        elif isinstance(player, str):
            steam_id = int(player)
        else:
            raise ValueError(
                "Invalid player. Use either a shinqlx.Player instance or a SteamID64."
            )

        # If it's the owner, treat it like a 5.
        if steam_id == shinqlx.owner():
            return 5

        key = f"minqlx:players:{steam_id}:permission"
        try:
            perm = self[key]
        except KeyError:
            perm = "0"

        return int(perm)

    def has_permission(self, player, level=5):
        """Checks if the player has higher than or equal to *level*.

        :param: player: The player in question.
        :type: player: shinqlx.Player
        :param: level: The permission level to check for.
        :type: level: int
        :returns: bool

        """
        return self.get_permission(player) >= level

    def set_flag(self, player, flag, value=True):
        """Sets specified player flag

        :param: player: The player in question.
        :type: player: shinqlx.Player
        :param: flag: The flag to set.
        :type: flag: string
        :param: value: (optional, default=True) Value to set
        :type: value: bool

        """
        if isinstance(player, shinqlx.Player):
            key = f"minqlx:players:{player.steam_id}:flags:{flag}"
        else:
            key = f"minqlx:players:{player}:flags:{flag}"

        self[key] = 1 if value else 0

    def get_flag(self, player, flag, default=False):
        """Clears the specified player flag

        :param: player: The player in question.
        :type: player: shinqlx.Player
        :param: flag: The flag to get
        :type: flag: string
        :param: default: (optional, default=False) The value to return if the flag is unknown
        :type: default: bool

        """
        if isinstance(player, shinqlx.Player):
            key = f"minqlx:players:{player.steam_id}:flags:{flag}"
        else:
            key = f"minqlx:players:{player}:flags:{flag}"

        try:
            return bool(int(self[key]))
        except KeyError:
            return default

    def connect(self, host=None, database=0, unix_socket=False, password=None):
        """Returns a connection to a Redis database. If *host* is None, it will
        fall back to the settings in the config and ignore the rest of the arguments.
        It will also share the connection across any plugins using the default
        configuration. Passing *host* will make it connect to a specific database
        that is not shared at all. Subsequent calls to this will return the connection
        initialized the first call unless it has been closed.

        :param: host: The host name. If no port is specified, it will use 6379. Ex.: ``localhost:1234``.
        :type: host: str
        :param: database: The database number that should be used.
        :type: database: int
        :param: unix_socket: Whether or not *host* should be interpreted as a unix socket path.
        :type: unix_socket: bool
        :raises: RuntimeError

        """
        if not host and not self._conn:  # Resort to default settings in config?
            if not Redis._conn:
                cvar_host = shinqlx.get_cvar("qlx_redisAddress")
                if cvar_host is None:
                    raise ValueError("cvar qlx_redisAddress misconfigured")
                redis_db_cvar = shinqlx.get_cvar("qlx_redisDatabase")
                if redis_db_cvar is None:
                    raise ValueError("cvar qlx_redisDatabase misconfigured.")
                cvar_db = int(redis_db_cvar)
                unix_socket_cvar = shinqlx.get_cvar("qlx_redisUnixSocket")
                if unix_socket_cvar is None:
                    raise ValueError("cvar qlx_redisUnixSocket misconfigured")
                cvar_unixsocket = bool(int(unix_socket_cvar))
                password_cvar = shinqlx.get_cvar("qlx_redisPassword")
                if password_cvar is None:
                    raise ValueError("cvar qlx_redisPassword misconfigured")
                Redis._pass = password_cvar
                if cvar_unixsocket:
                    Redis._conn = redis.StrictRedis(
                        unix_socket_path=cvar_host,
                        db=cvar_db,
                        password=Redis._pass,
                        decode_responses=True,
                    )
                else:
                    split_host = cvar_host.split(":")
                    port = int(split_host[1]) if len(split_host) > 1 else 6379
                    Redis._pool = redis.ConnectionPool(
                        host=split_host[0],
                        port=port,
                        db=cvar_db,
                        password=Redis._pass,
                        decode_responses=True,
                    )
                    Redis._conn = redis.StrictRedis(
                        connection_pool=Redis._pool, decode_responses=True
                    )
                    # TODO: Why does self._conn get set when doing Redis._conn?
                    self._conn = None
            return Redis._conn
        if not self._conn:
            if host is None:
                raise ValueError("wrong host")
            split_host = host.split(":")
            port = int(split_host[1]) if len(split_host) > 1 else 6379

            if unix_socket:
                self._conn = redis.StrictRedis(
                    unix_socket_path=host,
                    db=database,
                    password=password,
                    decode_responses=True,
                )
            else:
                self._pool = redis.ConnectionPool(
                    host=split_host[0],
                    port=port,
                    db=database,
                    password=password,
                    decode_responses=True,
                )
                self._conn = redis.StrictRedis(
                    connection_pool=self._pool, decode_responses=True
                )
        return self._conn

    def close(self):
        """Close the Redis connection if the config was overridden. Otherwise only do so
        if this is the last plugin using the default connection.

        """
        if self._conn:
            self._conn = None
            if self._pool:
                self._pool.disconnect()
                self._pool = None

        if Redis._counter <= 1 and Redis._conn:
            Redis._conn = None
            if Redis._pool:
                Redis._pool.disconnect()
                Redis._pool = None

    def mset(self, *args, **kwargs):
        mapping = {}
        if args:
            if len(args) != 1 or not isinstance(args[0], dict):
                raise redis.RedisError("MSET requires **kwargs or a single dict arg")
            mapping.update(args[0])

        if kwargs:
            mapping.update(kwargs)

        return self.r.mset(mapping)

    def msetnx(self, *args, **kwargs):
        mapping = {}
        if args:
            if len(args) != 1 or not isinstance(args[0], dict):
                raise redis.RedisError("MSETNX requires **kwargs or a single dict arg")
            mapping.update(args[0])

        if kwargs:
            mapping.update(kwargs)

        return self.r.msetnx(mapping)

    def zadd(self, name, *args, **kwargs):
        if redis.VERSION < (3, 0):
            return self.r.zadd(name, *args, **kwargs)

        if isinstance(args[0], dict):
            return self.r.zadd(name, *args, **kwargs)

        mapping = {}
        if len(args) > 0 and len(args) % 2 != 0:
            raise redis.RedisError("ZADD requires an equal number of values and scores")

        for i in range(0, len(args), 2):
            mapping[args[i + 1]] = args[i]

        return self.r.zadd(name, mapping, **kwargs)

    def zincrby(self, name, value_or_amount, amount_or_value=1):
        if not isinstance(value_or_amount, (int, float)):
            value = value_or_amount
            amount = amount_or_value
        else:
            value = amount_or_value
            amount = value_or_amount

        if redis.VERSION < (3, 0):
            return self.r.zincrby(name, value, amount)
        return self.r.zincrby(name, amount, value)

    def setex(self, name, value_or_time, time_or_value):
        if not isinstance(value_or_time, (int, timedelta)):
            value = value_or_time
            time = time_or_value
        else:
            value = time_or_value
            time = value_or_time

        if redis.VERSION < (3, 0):
            return self.r.setex(name, time, value)
        return self.r.setex(name, value, time)

    def lrem(self, name, value_or_count, num_or_value=0):
        if not isinstance(value_or_count, int):
            value = value_or_count
            count = num_or_value
        else:
            value = num_or_value
            count = value_or_count

        if redis.VERSION < (3, 0):
            return self.r.lrem(name, count, value)
        return self.r.lrem(name, value, count)
