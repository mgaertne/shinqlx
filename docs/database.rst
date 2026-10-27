.. _database:
.. module:: shinqlx.database

########
Database
########

.. class:: AbstractDatabase(plugin)

   :param Plugin plugin: The plugin for the database instance.

   .. property:: logger
      :type: logging.Logger

      The logging instance for this database instance. **Read-only**.

   .. method:: set_permission(player, level)
      :abstractmethod:

      Should set the permission of a player.

      :param Player | int | str player: The player to set the permission for.
      :param int level: The permission level to grant to the given player.

   .. method:: get_permission(player) -> int
      :abstractmethod:

      Should return the permission of a player.

      :param Player | int | str player: The player to get the permission from.
      :return: The permission level of the player.

   .. method:: has_permission(player, level = 5) -> bool
      :abstractmethod:

      Should return whether or not a player has more than or equal to a certain permission level. Should only take a value of 0 to 5, where 0 is always True.

      :param Player | int | str player: The player to check the permission from.
      :param int level: The permission level to check for. (default: ``5``)
      :return: Whether the given player has at least the given permission level.

   .. method:: set_flag(player, flag, value = True)
      :abstractmethod:

      Should set specified player flag to value.

      :param Player | int | str: The player to set the flag for.
      :param str flag: The flag to set.
      :param bool value: The value to set the flag to. (default: ``True``)

   .. method:: clear_flag(player, flag)

      Clears the specified player flag.

      :param Player | int | str: The player to clear the flag for.
      :param str flag: The flag to clear.

   .. method:: get_flag(player, flag, default = False) -> bool
      :abstractmethod:

      Should get specified player flag to value.

      :param Player | int | str: The player to get the flag for.
      :param str flag: The flag to get.
      :param bool default: The default value to return if the flag is not set for the player. (default: ``False``)
      :return: The flag value for the player, or the default value.

   .. method:: connect() -> redis.Redis | None
      :abstractmethod:

      Should return a connection to the database. Exactly what a "connection" obviously depends on the database, so the specifics will be up to the implementation.

      :return: The database instance after connecting.

   .. method:: close()
      :abstractmethod:

      If the database has a connection state, this method should close the connection

.. class:: Redis(plugin)

   :param Plugin plugin: The plugin for the database instance.

   A subclass of :class:`shinqlx.database.AbstractDatabase` providing support for Redis.

   .. property:: r
      :type: redis.Redis

      Access to the underlying redis instance. **Read-only**.

   .. method:: set_permission(player, level)

      Sets the permission of a player.

      :param Player | int | str player: The player to set the permission for.
      :param int level: The permission level to grant to the given player.

   .. method:: get_permission(player) -> int

      Gets the permission of a player.

      :param Player | int | str player: The player to get the permission from.
      :return: The permission level of the player.

      :raises ValueError: if the ``player`` is not an instance of Player, int, or str.

   .. method:: has_permission(player, level = 5) -> bool

      Checks if the player has higher than or equal to ``level``.

      :param Player | int | str player: The player to check the permission from.
      :param int level: The permission level to check for. (default: ``5``)
      :return: Whether the given player has at least the given permission level.

      :raises ValueError: if the ``player`` is not an instance of Player, int, or str.

   .. method:: set_flag(player, flag, value = True)

      Sets specified player flag to value.

      :param Player | int | str: The player to set the flag for.
      :param str flag: The flag to set.
      :param bool value: The value to set the flag to. (default: ``True``)

   .. method:: get_flag(player, flag, default = False) -> bool

      Gets the specified player flag, or the default value.

      :param Player | int | str: The player to get the flag for.
      :param str flag: The flag to get.
      :param bool default: The default value to return if the flag is not set for the player. (default: ``False``)
      :return: The flag value for the player, or the default value.

   .. method:: connect() -> redis.Redis | None
               connect(host = None, database = 0, unix_socket = False, password = None) -> redis.Redis | None

      Returns a connection to a Redis database.

      If ``host`` is None, it will fall back to the settings in the config (``qlx_redisAddress``, ``qlx_redisDatabase``, ``qlx_redisUnixSocket``, and ``qlx_redisPassword``) and ignore the rest of the arguments. It will also share the connection across any plugins using the default configuration.

      Passing ``host`` will make it connect to a specific database that is not shared at all. Subsequent calls to this will return the connection initialized the first call unless it has been closed.

      :param str | None host: The host name. If no port is specified, it will use ``6379``. Ex.: ``localhost:1234``. (default: ``None``)
      :param int database: The database number that should be used. (default: ``0``)
      :param bool unix_socket: Whether or not ``host`` should be interpreted as a unix socket path. (default: ``False``)
      :param str | None password: The password to use for the database connection. (default: ``None``)

      :return: The database instance after connecting.
      :raises ValueError: if the database connection is misconfigured.

   .. method:: close()

      Close the Redis connection if the config was overridden. Otherwise only do so if this is the last plugin using the default connection.

   .. method:: __contains__(key) -> bool

      Allows ``key in self.db``. Checks whether the key exists in the database.

   .. method:: __getitem__(key) -> str

      Allows ``self.db[key]``. Returns the string value stored for the key.

      :raises KeyError: if the key is not present in the database.

   .. method:: __setitem__(key, item)

      Allows ``self.db[key] = item``. Stores the value for the key.

      :raises RuntimeError: if the database assignment failed.

   .. method:: __delitem__(key)

      Allows ``del self.db[key]``. Removes the key from the database.

      :raises KeyError: if the key is not present in the database.

   .. method:: __getattr__(attr)

      Every other attribute is looked up on the underlying redis connection :attr:`r`. That way commands like ``self.db.smembers(key)`` or ``self.db.lrange(key, 0, -1)`` work as they would on a redis connection.

   The following commands behave the same with redis-py 2.x and 3.x or later. Where redis-py changed the order of the positional arguments between those versions, the arguments are accepted by keyword only.

   .. method:: mset(mapping = None, **kwargs)
               msetnx(mapping = None, **kwargs)

      Set multiple keys at once, ``msetnx`` only if none of the keys exist yet. The pairs are given either as a single :class:`dict` or as keyword arguments, or both.

      Raises a ``redis.RedisError`` if more than one positional argument or a positional argument that is not a :class:`dict` is given.

   .. method:: zadd(name, *args, **kwargs)
               zadd(name, mapping, nx = False, xx = False, ch = False, incr = False, gt = None, lt = None)

      Add members to the sorted set ``name``. Accepts the redis-py 3.x style with a ``mapping`` of members to scores, as well as the redis-py 2.x style with alternating ``member, score`` arguments, which is converted to a mapping.

      Raises a ``redis.RedisError`` if the number of members and scores does not match.

   .. method:: zincrby(name, *, value, amount = 1)

      Increment the score of ``value`` in the sorted set ``name`` by ``amount``.

   .. method:: setex(name, *, value, time)

      Set the ``value`` of the key ``name`` that expires after ``time``, given in seconds or as a :class:`datetime.timedelta`.

   .. method:: lrem(name, *, value, count = 0)

      Remove occurrences of ``value`` from the list ``name``. A ``count`` of ``0`` removes all of them.

   .. note::

      Old minqlx plugins call ``setex``, ``zincrby`` and ``lrem`` with positional arguments in the redis-py 2.x order, for example ``self.db.setex(key, time, value)``. Those calls raise a :class:`TypeError` here and have to pass ``value``, ``time``, ``amount`` and ``count`` as keyword arguments.
