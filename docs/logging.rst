.. _logging:
.. currentmodule:: shinqlx

*******
Logging
*******

shinqlx writes one log for the messages of its Rust core, the Python plugins and the built-in Rust plugins. Plugins log
through :func:`get_logger` as before; the Python logging module is the interface, and the messages end up in the
console of the server and in the file ``shinqlx.log``.

Console and file
================

* The console shows messages of the level ``info`` and above as ``[shinqlx.function] INFO: text``.
* ``shinqlx.log`` lies in the directory of the cvar ``fs_homepath`` and contains everything from ``debug`` up, as
  ``(HH:MM:SS) [INFO @ shinqlx.plugin.function] text``. The tracebacks of exceptions are part of the message.
* The file rotates: ``shinqlx.log.1`` and ``shinqlx.log.2`` are the older ones. The cvar ``qlx_logsSize`` is the size
  of a file in bytes (default 3000000), ``qlx_logs`` the number of old files to keep (default 2). The size is counted in
  bytes and not in characters.
* The messages of the start-up phase, before ``fs_homepath`` is known, are written to the file as soon as it is open.
* The file is written by a thread of its own. A slow disk does not slow down the server; if more than 8192 lines
  pile up, debug and info lines are dropped and a line ``N log lines dropped`` says how many. Warnings and errors wait
  briefly to be written.
* Every event that is dispatched to the plugins writes one ``debug`` line with its arguments to the file, except the
  noisy ones. The cvar ``qlx_noDebugEvents`` lists the events without such a line, separated by commas or spaces. It
  starts with ``frame``, ``set_configstring``, ``stats``, ``server_command``, ``death``, ``kill``, ``command``,
  ``console_print``, ``damage``, ``cvar_changed``, ``item_touch`` and ``weapon_fired``. The list replaces the default completely: set it
  to an empty string to log all events, or to the names you want to silence. It is read once while shinqlx starts, so
  put it into the server configuration with ``set``. The configuration file below cannot do this, it knows loggers and
  not events.
* The chat log of the built-in plugin ``log`` is separate, it keeps its own file ``chatlogs/chat.log`` and its cvars
  ``qlx_chatlogs`` and ``qlx_chatlogsSize``.

Configuration
=============

The file ``shinqlx_log.yml`` that comes with shinqlx shows the defaults and the options. It is optional; without it
shinqlx logs as described above. shinqlx looks for it in ``fs_homepath`` first and in the working directory of the
server second. A file that cannot be used is reported on the console and the defaults apply.

The file is a `log4rs <https://docs.rs/log4rs/latest/log4rs/config/index.html>`_ configuration with two additions:

* The appender kind ``shinqlx_file`` is the rotating log file. Its keys ``path`` (default ``shinqlx.log``, relative to
  ``fs_homepath``), ``max_size``, ``count`` and ``pattern`` are optional; ``max_size`` and ``count`` default to the cvars
  above.
* The encoder kind ``shinqlx_pattern`` is the log4rs pattern encoder that prints ``::`` in targets as ``.`` and the
  level ``WARN`` as ``WARNING``, as Python does.
  ``{K(func)}`` in a pattern is the function a message was logged from.

The target of a message is the name of its logger: ``shinqlx`` for the core and ``shinqlx::<plugin>`` for the plugin
``<plugin>``, written with ``::`` in the file. Each appender takes a ``threshold`` filter for its own level, and the
``loggers`` section sets levels per target, for example to silence one plugin:

.. code-block:: yaml

   loggers:
     shinqlx::balance:
       level: warn

Changes of the file apply after a restart.
