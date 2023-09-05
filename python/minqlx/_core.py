# minqlx - Extends Quake Live's dedicated server with extra functionality and scripting.
# Copyright (C) 2015 Mino <mino@minomino.org>

# This file is part of minqlx.

# minqlx is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.

# minqlx is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.

# You should have received a copy of the GNU General Public License
# along with minqlx. If not, see <http://www.gnu.org/licenses/>.


# Since this isn't the actual module, we define it here and export
# it later so that it can be accessed with minqlx.__doc__ by Sphinx.
import collections
import multiprocessing.pool
import subprocess
import threading
import traceback
import importlib
import datetime
import os
import os.path
import logging
import shlex
import sys
from contextlib import suppress
from functools import wraps

from logging.handlers import RotatingFileHandler

import minqlx
import minqlx.database

# em92: reasons not to support older than 3.5
# https://docs.python.org/3.5/whatsnew/3.5.html#whatsnew-ordereddict
# plugins already assume, that they are running on python >= 3.5
if sys.version_info < (3, 5):
    raise AssertionError("Only python 3.5 and later is supported by minqlx")

# Team number -> string
TEAMS = {0: "free", 1: "red", 2: "blue", 3: "spectator"}

# Game type number -> string
GAMETYPES = {
    0: "Free for All",
    1: "Duel",
    2: "Race",
    3: "Team Deathmatch",
    4: "Clan Arena",
    5: "Capture the Flag",
    6: "One Flag",
    8: "Harvester",
    9: "Freeze Tag",
    10: "Domination",
    11: "Attack and Defend",
    12: "Red Rover",
}

# Game type number -> short string
GAMETYPES_SHORT = {
    0: "ffa",
    1: "duel",
    2: "race",
    3: "tdm",
    4: "ca",
    5: "ctf",
    6: "1f",
    8: "har",
    9: "ft",
    10: "dom",
    11: "ad",
    12: "rr",
}

# Connection states.
CONNECTION_STATES = {0: "free", 1: "zombie", 2: "connected", 3: "primed", 4: "active"}

WEAPONS = {
    1: "g",
    2: "mg",
    3: "sg",
    4: "gl",
    5: "rl",
    6: "lg",
    7: "rg",
    8: "pg",
    9: "bfg",
    10: "gh",
    11: "ng",
    12: "pl",
    13: "cg",
    14: "hmg",
    15: "hands",
}

DEFAULT_PLUGINS = (
    "plugin_manager",
    "essentials",
    "motd",
    "permission",
    "ban",
    "silence",
    "clan",
    "names",
    "log",
    "workshop",
)


# ====================================================================
#                               HELPERS
# ====================================================================
def parse_variables(varstr, ordered=False):
    """
    Parses strings of key-value pairs delimited by "\\" and puts
    them into a dictionary.

    :param: varstr: The string with variables.
    :type: varstr: str
    :param: ordered: Whether it should use :class:`collections.OrderedDict` or not.
    :type: ordered: bool
    :returns: dict -- A dictionary with the variables added as key-value pairs.
    """
    res = collections.OrderedDict() if ordered else {}  # type: ignore
    if not varstr.strip():
        return res

    _vars = varstr.lstrip("\\").split("\\")
    try:
        for i in range(0, len(_vars), 2):
            res[_vars[i]] = _vars[i + 1]
    except IndexError:
        # Log and return incomplete dict.
        logger = minqlx.get_logger()
        logger.warning("Uneven number of keys and values: %s", varstr)

    return res


def get_logger(plugin=None):
    """
    Provides a logger that should be used by your plugin for debugging, info
    and error reporting. It will automatically output to both the server console
    as well as to a file.

    :param: plugin: The plugin that is using the logger.
    :type: plugin: minqlx.Plugin
    :returns: logging.Logger -- The logger in question.
    """
    if plugin:
        return logging.getLogger("minqlx." + str(plugin))
    return logging.getLogger("minqlx")


def _configure_logger():
    logger = logging.getLogger("minqlx")
    logger.setLevel(logging.DEBUG)

    # Console
    console_fmt = logging.Formatter(
        "[%(name)s.%(funcName)s] %(levelname)s: %(message)s", "%H:%M:%S"
    )
    console_handler = logging.StreamHandler()
    console_handler.setLevel(logging.INFO)
    console_handler.setFormatter(console_fmt)
    logger.addHandler(console_handler)

    # File
    homepath_cvar = minqlx.get_cvar("fs_homepath")
    if homepath_cvar is None:
        return
    file_path = os.path.join(homepath_cvar, "minqlx.log")
    maxlogs = minqlx.Plugin.get_cvar("qlx_logs", int)
    if maxlogs is None:
        return
    maxlogsize = minqlx.Plugin.get_cvar("qlx_logsSize", int)
    if maxlogsize is None:
        return
    file_fmt = logging.Formatter(
        "(%(asctime)s) [%(levelname)s @ %(name)s.%(funcName)s] %(message)s", "%H:%M:%S"
    )
    file_handler = RotatingFileHandler(
        file_path, encoding="utf-8", maxBytes=maxlogsize, backupCount=maxlogs
    )
    file_handler.setLevel(logging.DEBUG)
    file_handler.setFormatter(file_fmt)
    logger.addHandler(file_handler)
    logger.info(
        "============================= minqlx run @ %s =============================",
        datetime.datetime.now(),
    )


def log_exception(plugin=None):
    """
    Logs an exception using :func:`get_logger`. Call this in an except block.

    :param: plugin: The plugin that is using the logger.
    :type: plugin: minqlx.Plugin
    """
    # TODO: Remove plugin arg and make it automatic.
    logger = get_logger(plugin)
    e = traceback.format_exc().rstrip("\n")
    for line in e.split("\n"):
        logger.error(line)


def handle_exception(exc_type, exc_value, exc_traceback):
    """A handler for unhandled exceptions."""
    # TODO: If exception was raised within a plugin, detect it and pass to log_exception()
    logger = get_logger(None)
    e = "".join(traceback.format_exception(exc_type, exc_value, exc_traceback)).rstrip(
        "\n"
    )
    for line in e.split("\n"):
        logger.error(line)


def threading_excepthook(args):
    handle_exception(args.exc_type, args.exc_value, args.exc_traceback)


_init_time: datetime.datetime = datetime.datetime.now()


def uptime():
    """Returns a :class:`datetime.timedelta` instance of the time since initialized."""
    return datetime.datetime.now() - _init_time


def owner():
    """Returns the SteamID64 of the owner. This is set in the config."""
    # noinspection PyBroadException
    try:
        owner_cvar = minqlx.get_cvar("qlx_owner")
        if owner_cvar is None:
            raise RuntimeError
        sid = int(owner_cvar)
        if sid == -1:
            raise RuntimeError
        return sid
    except:  # noqa: E722
        logger = minqlx.get_logger()
        logger.error(
            "Failed to parse the Owner Steam ID. Make sure it's in SteamID64 format."
        )
    return None


_stats = None


def stats_listener():
    """Returns the :class:`minqlx.StatsListener` instance used to listen for stats."""
    return _stats


def set_cvar_once(name, value, flags=0):
    if minqlx.get_cvar(name) is None:
        minqlx.set_cvar(name, value, flags)
        return True

    return False


def set_cvar_limit_once(name, value, minimum, maximum, flags=0):
    if minqlx.get_cvar(name) is None:
        minqlx.set_cvar_limit(name, value, minimum, maximum, flags)
        return True

    return False


def set_plugins_version(path) -> None:
    args_version = shlex.split("git describe --long --tags --dirty --always")
    args_branch = shlex.split("git rev-parse --abbrev-ref HEAD")

    # We keep environment variables, but remove LD_PRELOAD to avoid a warning the OS might throw.
    env = dict(os.environ)
    del env["LD_PRELOAD"]
    try:
        # Get the version using git describe.
        with subprocess.Popen(
            args_version,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=path,
            env=env,
        ) as p:
            p.wait(timeout=1)
            if p.returncode != 0:
                setattr(minqlx, "__plugins_version__", "NOT_SET")
                return

            if p.stdout:
                version = p.stdout.read().decode().strip()

        # Get the branch using git rev-parse.
        with subprocess.Popen(
            args_branch,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=path,
            env=env,
        ) as p:
            p.wait(timeout=1)
            if p.returncode != 0:
                setattr(minqlx, "__plugins_version__", version)
                return

            if p.stdout:
                branch = p.stdout.read().decode().strip()
    except (FileNotFoundError, subprocess.TimeoutExpired):
        setattr(minqlx, "__plugins_version__", "NOT_SET")
        return

    setattr(minqlx, "__plugins_version__", f"{version}-{branch}")


def set_map_subtitles() -> None:
    # We save the actual values before setting them so that we can retrieve them in Game.
    setattr(minqlx, "_map_title", minqlx.get_configstring(3))
    setattr(minqlx, "_map_subtitle1", minqlx.get_configstring(678))
    setattr(minqlx, "_map_subtitle2", minqlx.get_configstring(679))

    cs = minqlx.get_configstring(678)
    if cs:
        cs += " - "
    minqlx.set_configstring(
        678,
        cs + f"Running minqlx ^6{minqlx.__version__}^7 "
        f"with plugins ^6{getattr(minqlx, '__plugins_version__', 'NOT_SET')}^7.",
    )
    cs = minqlx.get_configstring(679)
    if cs:
        cs += " - "
    minqlx.set_configstring(
        679, cs + "Check ^6http://github.com/MinoMino/minqlx^7 for more details."
    )


# ====================================================================
#                              DECORATORS
# ====================================================================
def next_frame(func):
    @wraps(func)
    def f(*args, **kwargs):
        minqlx.next_frame_tasks.put((func, args, kwargs), block=False)

    return f


def delay(time):
    """Delay a function call a certain amount of time.

    .. note::
        It cannot guarantee you that it will be called right as the timer
        expires, but unless some plugin is for some reason blocking, then
        you can expect it to be called practically as soon as it expires.

    :param: func: The function to be called.
    :type: func: callable
    :param: time: The number of seconds before the function should be called.
    :type: time: float

    """

    def wrap(func):
        @wraps(func)
        def f(*args, **kwargs):
            minqlx.frame_tasks.enter(time, 1, func, args, kwargs)

        return f

    return wrap


def _thread_init():
    current_thread = threading.current_thread()
    current_thread.name = f"{_thread_name}-{current_thread.name}"


_thread_name = "minqlxthread"
_threadpool = multiprocessing.pool.ThreadPool(initializer=_thread_init)


def thread(func, force=False):
    """Starts a thread with the function passed as its target. If a function decorated
    with this is called within a function also decorated, it will **not** create a second
    thread unless told to do so with the *force* keyword.

    :param: func: The function to be run in a thread.
    :type: func: callable
    :param: force: Force it to create a new thread even if already in one created by this decorator.
    :type: force: bool
    :returns: threading.Thread

    """

    @wraps(func)
    def f(*args, **kwargs):
        if not force and threading.current_thread().name.startswith(_thread_name):
            func(*args, **kwargs)
        else:
            _threadpool.apply_async(func, args, kwargs)

    return f


# ====================================================================
#                       CONFIG AND PLUGIN LOADING
# ====================================================================
# We need to keep track of module instances for use with importlib.reload.
_modules = {}


class PluginLoadError(Exception):
    pass


class PluginUnloadError(Exception):
    pass


def load_preset_plugins():
    plugins_temp = []
    plugins_cvar = minqlx.Plugin.get_cvar("qlx_plugins", list)
    if plugins_cvar is None:
        return
    for p in plugins_cvar:
        if p == "DEFAULT":
            plugins_temp += list(DEFAULT_PLUGINS)
        else:
            plugins_temp.append(p)

    plugins = []
    for p in plugins_temp:
        if p not in plugins:
            plugins.append(p)

    plugins_path_cvar = minqlx.get_cvar("qlx_pluginsPath")
    if plugins_path_cvar is None:
        raise PluginLoadError("cvar qlx_pluginsPath misconfigured")

    plugins_path = os.path.abspath(plugins_path_cvar)
    plugins_dir = os.path.basename(plugins_path)

    if not os.path.isdir(plugins_path):
        raise PluginLoadError(
            f"Cannot find the plugins directory '{os.path.abspath(plugins_path)}'."
        )

    plugins = [p for p in plugins if f"{plugins_dir}.{p}"]
    for p in plugins:
        load_plugin(p)


def load_plugin(plugin):
    logger = get_logger(None)
    logger.info("Loading plugin '%s'...", plugin)
    # noinspection PyProtectedMember
    plugins = minqlx.Plugin._loaded_plugins
    plugins_path_cvar = minqlx.get_cvar("qlx_pluginsPath")
    if plugins_path_cvar is None:
        raise PluginLoadError("cvar qlx_pluginsPath misconfigured")

    plugins_path = os.path.abspath(plugins_path_cvar)
    plugins_dir = os.path.basename(plugins_path)

    if not os.path.isfile(os.path.join(plugins_path, plugin + ".py")):
        raise PluginLoadError("No such plugin exists.")
    if plugin in plugins:
        reload_plugin(plugin)
        return
    try:
        module = importlib.import_module(f"{plugins_dir}.{plugin}")
        # We add the module regardless of whether it fails or not, otherwise we can't reload later.
        _modules[plugin] = module

        if not hasattr(module, plugin):
            raise PluginLoadError(
                "The plugin needs to have a class with the exact name as the file, minus the .py."
            )

        plugin_class = getattr(module, plugin)
        if not issubclass(plugin_class, minqlx.Plugin):
            raise PluginLoadError(
                "Attempted to load a plugin that is not a subclass of 'minqlx.Plugin'."
            )
        plugins[plugin] = plugin_class()
    except:
        log_exception(plugin)
        raise


def unload_plugin(plugin):
    logger = get_logger(None)
    logger.info("Unloading plugin '%s'...", plugin)
    # noinspection PyProtectedMember
    plugins = minqlx.Plugin._loaded_plugins
    if plugin not in plugins:
        raise PluginUnloadError("Attempted to unload a plugin that is not loaded.")

    try:
        minqlx.EVENT_DISPATCHERS["unload"].dispatch(plugin)

        # Unhook its hooks.
        for hook in plugins[plugin].hooks:
            plugins[plugin].remove_hook(*hook)

        # Unregister commands.
        for cmd in plugins[plugin].commands:
            plugins[plugin].remove_command(cmd.name, cmd.handler)

        del plugins[plugin]
    except:
        log_exception(plugin)
        raise


def reload_plugin(plugin):
    with suppress(PluginUnloadError):
        unload_plugin(plugin)

    try:
        if plugin in _modules:  # Unloaded previously?
            importlib.reload(_modules[plugin])
        load_plugin(plugin)
    except:
        log_exception(plugin)
        raise


def initialize_cvars():
    # Core
    minqlx.set_cvar_once("qlx_owner", "-1")
    minqlx.set_cvar_once("qlx_plugins", ", ".join(DEFAULT_PLUGINS))
    minqlx.set_cvar_once("qlx_pluginsPath", "minqlx-plugins")
    minqlx.set_cvar_once("qlx_database", "Redis")
    minqlx.set_cvar_once("qlx_commandPrefix", "!")
    minqlx.set_cvar_once("qlx_logs", "2")
    minqlx.set_cvar_once("qlx_logsSize", str(3 * 10**6))  # 3 MB
    # Redis
    minqlx.set_cvar_once("qlx_redisAddress", "127.0.0.1")
    minqlx.set_cvar_once("qlx_redisDatabase", "0")
    minqlx.set_cvar_once("qlx_redisUnixSocket", "0")
    minqlx.set_cvar_once("qlx_redisPassword", "")


# ====================================================================
#                                 MAIN
# ====================================================================
def initialize():
    minqlx.register_handlers()


def late_init():
    """Initialization that needs to be called after QLDS has finished
    its own initialization.

    """
    minqlx.initialize_cvars()

    # Set the default database plugins should use.
    # TODO: Make Plugin.database setting generic.
    database_cvar = minqlx.get_cvar("qlx_database")
    if database_cvar is not None and database_cvar.lower() == "redis":
        minqlx.Plugin.database = minqlx.database.Redis

    # Get the plugins path and set minqlx.__plugins_version__.
    plugins_path_cvar = minqlx.get_cvar("qlx_pluginsPath")
    if plugins_path_cvar is not None:
        plugins_path = os.path.abspath(plugins_path_cvar)
        set_plugins_version(plugins_path)

        # Add the plugins path to PATH so that we can load plugins later.
        sys.path.append(os.path.dirname(plugins_path))

    # Initialize the logger now that we have fs_basepath.
    _configure_logger()
    logger = get_logger()
    # Set our own exception handler so that we can log them if unhandled.
    sys.excepthook = handle_exception

    if sys.version_info >= (3, 8):
        threading.excepthook = threading_excepthook

    logger.info("Loading preset plugins...")
    load_preset_plugins()

    stats_enable_cvar = minqlx.get_cvar("zmq_stats_enable")
    if stats_enable_cvar is not None and bool(int(stats_enable_cvar)):
        global _stats
        _stats = minqlx.StatsListener()
        logger.info("Stats listener started on %s.", _stats.address)
        # Start polling. Not blocking due to decorator magic. Aw yeah.
        _stats.keep_receiving()

    logger.info("We're good to go!")
