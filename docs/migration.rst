.. _migration:
.. currentmodule:: shinqlx

#####################
Migration from minqlx
#####################

Most plugins written for `minqlx <https://github.com/MinoMino/minqlx>`_ run on shinqlx without a change, because the
``minqlx`` module is still importable and provides the same names. A few things behave differently and a few things
were added, which makes shorter and better checked plugins possible. This page lists both, and describes the tool that
does most of the work for a whole folder of plugins.

The changes fall into three groups:

* :ref:`Required changes <migration_required>`: the plugin breaks without them.
* :ref:`Changes in behaviour <migration_behaviour>`: the plugin runs, but something works differently.
* :ref:`Optional modernisation <migration_optional>`: the plugin runs unchanged, and gets simpler with them.

.. _migration_tool:

The migration tool
==================

``plugin_migration`` takes a folder with minqlx plugins and writes the migrated plugins to another folder:

.. code-block:: console

   plugin_migration [-n] [-j N] <input> <output>

The output folder must not exist yet or must be empty, and it must not be inside the input folder. A file that needs no
change, and every file that is not Python, is copied as it is. A file that is changed keeps its formatting and its
comments, only the affected places are rewritten. Files are processed in parallel (``-j`` sets the number of threads)
and a progress bar shows how far the tool is. With ``-n`` nothing is written and the tool only reports what it would do.

Places that need a look by hand are reported as warnings with the file and the line, for example::

   warning: battleroyale.py:246: the registration was kept as it is: the argument permission is not a constant

The tool is built in the ``build-binaries`` job of the CI and is a member of the cargo workspace
(``cargo run -p plugin_migration --release -- <input> <output>``). The migrated plugins run on shinqlx only, so keep the
original folder as long as you also run minqlx somewhere.

What the tool does
------------------

The rules run in this order, each one on the result of the previous one:

=========================== ===============================================================================
rule                        change
=========================== ===============================================================================
``redis-keywords``          ``db.setex``, ``db.lrem`` and ``db.zincrby`` get keyword arguments, see
                            :ref:`migration_redis`. Where the order of the arguments of ``zincrby`` cannot be
                            determined, the call is left as it is and reported.
``slay-with-mod``           ``mod=`` becomes ``means_of_death=``, see :ref:`migration_slay`.
``default-plugin-access``   Only warnings, see :ref:`migration_default_plugins`.
``event-names``             The event name in ``add_hook`` and ``remove_hook`` becomes ``Events.X``. Names the
                            :class:`Events` enumeration does not know stay strings.
``enum-constants``          ``RET_*``, ``PRI_*``, ``CVAR_*``, ``TEAM_*``, ``CS_*``, ``MOD_*``, ``PRIV_*`` and
                            ``DAMAGE_*`` become members of :class:`ReturnCodes`, :class:`Priorities`,
                            :class:`CvarFlags`, :class:`Teams`, :class:`ConnectionStates`, :class:`MeansOfDeath`,
                            :class:`Privileges` and :class:`DamageFlags`. This only happens where a member works
                            the same as the integer: in ``return``, in comparisons with ``==``, ``!=`` and ``in``,
                            and in the arguments that take the enumeration. Arithmetic, ``is`` and dictionary keys
                            keep the constant.
``enum-imports``            The same for constants imported with ``from minqlx import RET_STOP``. The use becomes
                            ``ReturnCodes.STOP`` and the import brings in ``ReturnCodes``. A constant stays
                            imported as long as it is still used somewhere else. Imports that span several lines
                            and imports with ``as`` are left as they are.
``permission-levels``       A permission level from 0 to 5 in ``add_command`` becomes :class:`Permissions`.
``decorators``              Registrations in ``__init__`` become decorators, see below.
``module-rename``           ``minqlx`` becomes ``shinqlx`` in imports and in uses of the module. Strings and
                            comments stay as they are, so database keys like ``"minqlx:players:{}"`` keep working
                            with the data of an existing server. If a file binds the name ``minqlx`` to something
                            else, it is left alone.
=========================== ===============================================================================

The enumerations and event names are read from the type stub ``_shinqlx.pyi`` when the tool is built, so the tool and the
module cannot drift apart.

Decorators
^^^^^^^^^^

A call ``self.add_hook(...)`` or ``self.add_command(...)`` as a statement of its own directly in ``__init__`` becomes
:func:`hook` or :func:`command` on the handler, if the handler is a method of the same class and all arguments are
constants. The handler may already be decorated with :func:`thread`, :func:`next_frame` or :func:`delay`. Otherwise the
registration stays in ``__init__`` untouched, and the tool names the reason in a warning:

* The permission, the name or another argument is not a constant, for example a permission that is read from a cvar.
  Registrations like this are meant to stay in ``__init__``: the code after the call is the plugin author's choice,
  and a decorator cannot express it.
* The handler is not a method of the class, for example a lambda or a function of another object.
* The class calls ``remove_hook`` or ``remove_command``, so the registrations are changed at runtime.
* The handler has other decorators.

An ``__init__`` that did nothing but register is removed completely. If a class has both kinds of registrations, the
tool reports it, because hooks of the same priority run in the order of registration, which can change when the
decorated ones are registered first.

Checked against real plugins
^^^^^^^^^^^^^^^^^^^^^^^^^^^^

The tool was run over the plugins of
`MinoMino/minqlx-plugins <https://github.com/MinoMino/minqlx-plugins>`_ and the repositories of the
`Useful Plugins <https://github.com/MinoMino/minqlx/wiki/Useful-Plugins>`_ list. Every migrated file is valid Python, and
every class registers the same hooks and commands, with the same handlers, priorities and options, as before.

.. _migration_required:

Required changes
================

.. _migration_redis:

Redis calls
-----------

``setex``, ``lrem`` and ``zincrby`` of the database take their value arguments by keyword. The redis client has two
different orders for them, depending on the version (redis-py 2.x and 3), and plugins never knew which one they got.
Calls with positional arguments raise a :class:`TypeError`.

.. code-block:: python

   # minqlx
   db.setex(key, 3600, value)
   db.lrem(key, 0, value)
   db.zincrby(key, value, 1)

   # shinqlx
   db.setex(key, time=3600, value=value)
   db.lrem(key, count=0, value=value)
   db.zincrby(key, value=value, amount=1)

``amount`` of ``zincrby`` defaults to 1 and ``count`` of ``lrem`` to 0. ``Redis.setx``, which a version of the port
offered by mistake, never existed in redis-py.

.. _migration_slay:

``Plugin.slay_with_mod``
------------------------

The keyword of the means of death is called ``means_of_death`` instead of ``mod``, because ``mod`` is a reserved word in
Rust. Calls with a positional argument keep working. The same goes for the function :func:`slay_with_mod` of the module,
whose keyword is ``mean_of_death``.

.. _migration_default_plugins:

Members of the default plugins
------------------------------

The default plugins are built into shinqlx as Rust plugins, see :ref:`native_plugins`. Plugins that call methods of the
default plugins with ``self.plugins["name"]`` find the ones that other community plugins commonly use: ``ban`` offers
``is_banned``, ``leave_status``, ``warn_player``, ``deliver_warning`` and ``remember_players``, ``silence`` offers
``is_silenced``, and a few more. Members that were only used inside of a default plugin are not rebuilt, for example
``update_player`` and ``force`` of ``essentials``, or ``clean_tag`` of ``clan``. The tool warns for every access to a
member that is missing. Fix those by hand, by copying the original default plugin to the plugins directory under the
same name: A Python plugin of a name wins over the built-in plugin of that name, so nothing else has to be configured.

``next_frame_tasks``
--------------------

``next_frame_tasks`` is a ``queue.SimpleQueue`` and has ``put`` and ``get_nowait`` instead of the methods of a
``collections.deque``. Plugins should use the :func:`next_frame` decorator instead of touching the queue. The tool
warns for every use.

.. _migration_behaviour:

Changes in behaviour
====================

Commands and hooks
------------------

* ``Command.execute`` calls the handler with ``msg.split()`` like minqlx, so arguments are separated by any amount of
  whitespace and are never empty.
* ``team <letter>`` is evaluated in lower case, so ``team R`` triggers ``team_switch_attempt`` like it does in minqlx.
* The ``say`` and ``say_team`` handlers replace a ``"`` in the message with ``'``, minqlx removed it.
* A ``serverinfo`` configstring without ``g_gameState`` raises a :class:`KeyError` like in minqlx. The error is logged and
  the event passes.
* ``handle_player_disconnect`` receives the reason as an optional argument.
* The ``frame``, ``server_command``, ``set_configstring``, ``console_print`` and ``damage`` events are only dispatched
  if a hook is registered. Plugins notice nothing, the server saves the work.
* Tasks queued with :func:`next_frame` are also taken over if the dispatch of the frame raised an exception.

Texts
-----

* :func:`send_server_command` replaces ``%`` with the full width percent sign U+FF05 in the commands that show text
  (``print``, ``chat``, ``tchat`` and ``cp``), because the server takes ``%`` for a placeholder. Plugins which did
  that themselves keep working.
* The built-in Rust plugins keep the texts of the Python plugins. The deviations are listed with each plugin in
  :ref:`native_plugins`: ``ban``, ``silence`` and ``essentials`` accept names where the original took only client ids,
  answer with the usage on an invalid length, and a name that matches nobody or several players ends the processing
  with ``RET_STOP_ALL``.

Loading plugins
---------------

* ``DEFAULT`` in ``qlx_plugins`` is replaced by the default plugins at its position and every plugin loads once, at its
  first occurrence. minqlx put ``DEFAULT`` first or last, depending on the order of an unordered set. With
  ``qlx_plugins "myplugin, DEFAULT"`` the plugin ``myplugin`` is loaded before the default plugins, which matters for
  plugins that replace a default plugin. See :data:`DEFAULT_PLUGINS`.
* ``p!name`` loads the Python plugin and ``r!name`` the built-in Rust plugin. A name without prefix is the Python plugin
  if the file exists, so a plugin in the plugins directory that replaces a default plugin needs no change. Plugins
  are known by the bare name: ``!unload`` and ``self.plugins`` do not take the prefix. See :func:`load_plugin`.
* A Python plugin of a replaced default plugin, for example ``myban`` or ``myessentials``, unloads it, as before.

Removed
-------

* ``Plugin.cointoss`` and the command ``!cointoss`` of ``essentials`` are gone. They were unused and did nothing in
  minqlx.

.. _migration_optional:

Optional modernisation
======================

The tool makes all of these changes, but they can also be made by hand, one plugin at a time. They are only available in
the ``shinqlx`` module, the ``minqlx`` module does not provide them.

``shinqlx`` instead of ``minqlx``
---------------------------------

``import shinqlx`` is the module of shinqlx, ``import minqlx`` the compatibility module. Plugins that use both are
possible, but a plugin is easier to read with one name. Replace the name in imports and uses, and nothing else:
strings stay as they are.

Enumerations
------------

The enumerations replace the integer constants. Their members compare equal to the integers they stand for and are
accepted wherever the API expects the integer, and type checkers know them.

.. code-block:: python

   # minqlx
   return minqlx.RET_STOP_ALL
   self.add_hook("chat", self.handle_chat, priority=minqlx.PRI_HIGH)
   minqlx.set_cvar("qlx_foo", "1", minqlx.CVAR_ARCHIVE | minqlx.CVAR_ROM)

   # shinqlx
   return shinqlx.ReturnCodes.STOP_ALL
   self.add_hook(shinqlx.Events.CHAT, self.handle_chat, priority=shinqlx.Priorities.HIGH)
   shinqlx.set_cvar("qlx_foo", "1", shinqlx.CvarFlags.ARCHIVE | shinqlx.CvarFlags.ROM)

Do not replace a constant in arithmetic or as the key of a dictionary: the tool does not either. See :ref:`enums` for
all enumerations. A plugin that imports the constants with ``from minqlx import RET_STOP`` imports the enumeration
instead, ``from shinqlx import ReturnCodes``, and uses ``ReturnCodes.STOP``. The rules for events, permission levels
and decorators need ``import minqlx`` and do not change plugins that only use ``from minqlx import ...``.

Decorators for hooks and commands
---------------------------------

.. code-block:: python

   # minqlx
   class chatlog(minqlx.Plugin):
       def __init__(self):
           self.add_hook("chat", self.handle_chat)
           self.add_command("chatlog", self.cmd_chatlog, 3)

   # shinqlx
   class chatlog(shinqlx.Plugin):
       @shinqlx.hook(shinqlx.Events.CHAT)
       def handle_chat(self, player, msg, channel):
           ...

       @shinqlx.command("chatlog", permission=shinqlx.Permissions.COMMUNITY_ADMIN)
       def cmd_chatlog(self, player, msg, channel):
           ...

The signature of the handler is checked against the arguments of the event when the plugin is loaded, so a wrong
handler is found at once and not when the event first fires. See :func:`hook` and :func:`command`. Registrations that
depend on something computed in ``__init__`` stay as they are.

Migrating by hand
=================

If you do not use the tool, go through the plugins in this order:

#. Search for ``setex``, ``lrem`` and ``zincrby`` and add the keywords.
#. Search for ``slay_with_mod`` with ``mod=``, and for ``next_frame_tasks``.
#. Search for ``self.plugins[`` and check that each accessed member exists in the plugin. For the Rust plugins see
   :ref:`native_plugins`. If one is missing, put the original default plugin into the plugins directory.
#. Check plugins which replace a default plugin for the order in ``qlx_plugins``.
#. Start the server and read the log. A plugin which cannot be loaded names the reason, and a handler with a wrong
   signature is reported when the decorator is applied.
#. Change to ``shinqlx``, the enumerations and the decorators when you touch a plugin anyway. Nothing forces you to.
