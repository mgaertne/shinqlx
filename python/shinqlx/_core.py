import subprocess
import threading
import traceback
import importlib
import os
import os.path
import shlex
import sys
from contextlib import suppress

import shinqlx
from shinqlx import PluginLoadError, PluginUnloadError, get_logger, log_exception, _configure_logger
import shinqlx.database

if sys.version_info < (3, 7):
    raise AssertionError("Only python 3.7 and later is supported by shinqlx")


# ====================================================================
#                               HELPERS
# ====================================================================
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


_stats = None


def stats_listener():
    """Returns the :class:`shinqlx.StatsListener` instance used to listen for stats."""
    return _stats


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
                setattr(shinqlx, "__plugins_version__", "NOT_SET")
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
                setattr(shinqlx, "__plugins_version__", version)
                return

            if p.stdout:
                branch = p.stdout.read().decode().strip()
    except (FileNotFoundError, subprocess.TimeoutExpired):
        setattr(shinqlx, "__plugins_version__", "NOT_SET")
        return

    setattr(shinqlx, "__plugins_version__", f"{version}-{branch}")


# ====================================================================
#                       CONFIG AND PLUGIN LOADING
# ====================================================================
# We need to keep track of module instances for use with importlib.reload.
_modules = {}


def load_preset_plugins():
    plugins_temp = []
    plugins_cvar = shinqlx.Plugin.get_cvar("qlx_plugins", list)
    if plugins_cvar is None:
        return
    for p in plugins_cvar:
        if p == "DEFAULT":
            plugins_temp += list(shinqlx.DEFAULT_PLUGINS)
        else:
            plugins_temp.append(p)

    plugins = []
    for p in plugins_temp:
        if p not in plugins:
            plugins.append(p)

    plugins_path_cvar = shinqlx.get_cvar("qlx_pluginsPath")
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
    plugins = shinqlx.Plugin._loaded_plugins
    plugins_path_cvar = shinqlx.get_cvar("qlx_pluginsPath")
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
        if not issubclass(plugin_class, shinqlx.Plugin):
            raise PluginLoadError(
                "Attempted to load a plugin that is not a subclass of 'shinqlx.Plugin'."
            )
        plugins[plugin] = plugin_class()
    except:
        log_exception(plugin)
        raise


def unload_plugin(plugin):
    logger = get_logger(None)
    logger.info("Unloading plugin '%s'...", plugin)
    # noinspection PyProtectedMember
    plugins = shinqlx.Plugin._loaded_plugins
    if plugin not in plugins:
        raise PluginUnloadError("Attempted to unload a plugin that is not loaded.")

    try:
        shinqlx.EVENT_DISPATCHERS["unload"].dispatch(plugin)

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


# ====================================================================
#                                 MAIN
# ====================================================================
def initialize():
    shinqlx.register_handlers()


def late_init():
    """Initialization that needs to be called after QLDS has finished
    its own initialization.

    """
    shinqlx.initialize_cvars()

    # Set the default database plugins should use.
    # TODO: Make Plugin.database setting generic.
    database_cvar = shinqlx.get_cvar("qlx_database")
    if database_cvar is not None and database_cvar.lower() == "redis":
        shinqlx.Plugin.database = shinqlx.database.Redis

    # Get the plugins path and set shinqlx.__plugins_version__.
    plugins_path_cvar = shinqlx.get_cvar("qlx_pluginsPath")
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

    stats_enable_cvar = shinqlx.get_cvar("zmq_stats_enable")
    if stats_enable_cvar is not None and bool(int(stats_enable_cvar)):
        global _stats
        _stats = shinqlx.StatsListener()
        logger.info("Stats listener started on %s.", _stats.address)
        # Start polling. Not blocking due to decorator magic. Aw yeah.
        _stats.keep_receiving()

    logger.info("We're good to go!")
