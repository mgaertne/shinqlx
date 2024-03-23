import subprocess
import importlib
import os
import os.path
import shlex
import sys
from contextlib import suppress

import shinqlx
from shinqlx import (
    PluginLoadError,
    PluginUnloadError,
    get_logger,
    log_exception,
)
import shinqlx.database

if sys.version_info < (3, 7):
    raise AssertionError("Only python 3.7 and later is supported by shinqlx")


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
