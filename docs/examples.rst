.. _examples:
.. currentmodule:: shinqlx

#################
Plugin examples
#################

The examples show the typical building blocks of a plugin. A plugin is a Python file in the directory of
``qlx_pluginsPath`` whose class has the same name as the file, e.g. ``greeter.py`` contains ``class greeter``.
Add the name to ``qlx_plugins`` to load it at startup, or load it at runtime with ``!load greeter``.

The events are named with the members of :class:`Events`, e.g. ``shinqlx.Events.PLAYER_LOADED``. They are strings, so
the plain event name ``"player_loaded"`` keeps working, which is what you find in older plugins.

More plugins to learn from can be found in the
`minqlx plugin repository <https://github.com/MinoMino/minqlx-plugins>`_.

.. note::
   The examples use the :ref:`enumerations <enums>` of ``shinqlx``. A plugin that has to run on ``minqlx`` as well
   uses the integer constants like :data:`PRI_LOW` and the strings ``"red"`` instead; their values are the same.

A greeting
==========

Hooks react to events of the server, commands react to chat messages and console input. Both can be declared on the
methods that handle them with the :ref:`decorators <plugin>`.

.. code-block:: python

   import shinqlx


   class greeter(shinqlx.Plugin):
       @shinqlx.hook(shinqlx.Events.PLAYER_LOADED, priority=shinqlx.Priorities.LOW)
       def greet_new_player(self, player):
           player.tell(f"Welcome, {player.name}^7!")

       @shinqlx.command("hello", permission=shinqlx.Permissions.PLAYER, usage="<name>")
       def cmd_hello(self, player, msg, channel):
           if len(msg) < 2:
               return shinqlx.ReturnCodes.USAGE

           channel.reply(f"Hello {msg[1]}^7!")

Returning :attr:`ReturnCodes.USAGE` from a command replies with its ``usage``. A hook that returns
:attr:`ReturnCodes.STOP_ALL` stops the event for the engine as well, which is shown in the next example.

Keeping teams even
==================

The ``team_switch_attempt`` event happens before the player changes the team, so a handler can veto it. The teams
are given as strings, and the :class:`Teams` members compare equal to them.

.. code-block:: python

   import shinqlx


   class team_guard(shinqlx.Plugin):
       @shinqlx.hook(shinqlx.Events.TEAM_SWITCH_ATTEMPT)
       def keep_teams_even(self, player, old_team, new_team):
           if new_team not in (shinqlx.Teams.RED, shinqlx.Teams.BLUE):
               return shinqlx.ReturnCodes.NONE

           teams = self.teams()
           other = shinqlx.Teams.BLUE if new_team == shinqlx.Teams.RED else shinqlx.Teams.RED
           if len(teams[new_team]) > len(teams[str(other)]):
               player.tell("The teams would be uneven, please wait.")
               return shinqlx.ReturnCodes.STOP_ALL

A command to even out the teams by hand can use :meth:`Plugin.put`, which accepts the members as well:

.. code-block:: python

       @shinqlx.command("spec", permission=shinqlx.Permissions.MODERATOR, usage="<id>")
       def cmd_spec(self, player, msg, channel):
           target = self.player(msg[1]) if len(msg) > 1 else None
           if target is None:
               return shinqlx.ReturnCodes.USAGE

           self.put(target, shinqlx.Teams.SPECTATOR)

Weapons
=======

:class:`Weapon` names the weapon, :class:`Weapons` holds the amount per weapon. The string of a member is the field
name of :class:`Weapons`.

.. code-block:: python

   import shinqlx


   class railgun_start(shinqlx.Plugin):
       def __init__(self):
           super().__init__()
           self.add_hook(shinqlx.Events.PLAYER_SPAWN, self.give_railgun)

       def give_railgun(self, player):
           player.weapons(rg=1, mg=0)
           player.ammo(rg=25)
           player.weapon(shinqlx.Weapon.RAILGUN)

       @shinqlx.command("rockets", permission=shinqlx.Permissions.PLAYER)
       def cmd_rockets(self, player, msg, channel):
           rockets = getattr(player.ammo(), str(shinqlx.Weapon.ROCKET_LAUNCHER))
           channel.reply(f"{player.name}^7 has {rockets} rockets.")

Cvars and flags
===============

The :class:`Cvar` declaration creates the cvar of a plugin when it is loaded, but only if the cvar does not exist yet,
so a value from the server configuration always wins. The attribute returns the current value in the type of the
default, and type checkers know that type.

.. code-block:: python

   import shinqlx


   class announcer(shinqlx.Plugin):
       text = shinqlx.Cvar("qlx_announcerText", "Have fun!", flags=shinqlx.CvarFlags.ARCHIVE)
       repeat = shinqlx.Cvar("qlx_announcerRepeat", 3, minimum=1, maximum=10)

       def __init__(self):
           super().__init__()
           self.add_hook(shinqlx.Events.GAME_COUNTDOWN, self.announce)

       def announce(self):
           for _ in range(self.repeat):
               self.msg(self.text)

Without the declaration, the same is done with :meth:`Plugin.set_cvar_once` and :meth:`Plugin.get_cvar`:

.. code-block:: python

   flags = shinqlx.CvarFlags.ARCHIVE | shinqlx.CvarFlags.LATCH
   self.set_cvar_once("qlx_announcerText", "Have fun!", flags)
   text = self.get_cvar("qlx_announcerText")

Connection states and slaying
=============================

A player in the list of players is not necessarily in the game yet. :attr:`Player.connection_state` tells how far the
connection is, and compares equal to the members of :class:`ConnectionStates`.

.. code-block:: python

   import shinqlx


   class leave_notice(shinqlx.Plugin):
       def __init__(self):
           super().__init__()
           self.add_hook(shinqlx.Events.PLAYER_DISCONNECT, self.announce_leave)

       def announce_leave(self, player, reason):
           if player.connection_state == shinqlx.ConnectionStates.ACTIVE:
               self.msg(f"{player.name}^7 left the game.")

To kill a player in the game, :func:`slay_with_mod` takes a member of :class:`MeansOfDeath` as the cause of the death.
The player stays connected to the server and respawns as usual, which is what distinguishes slaying from kicking
(:meth:`Player.kick`):

.. code-block:: python

   import shinqlx


   class slayer(shinqlx.Plugin):
       @shinqlx.command("slay", permission=shinqlx.Permissions.GAME_ADMIN, usage="<id>")
       def cmd_slay(self, player, msg, channel):
           target = self.player(msg[1]) if len(msg) > 1 else None
           if target is None:
               return shinqlx.ReturnCodes.USAGE

           shinqlx.slay_with_mod(target.id, shinqlx.MeansOfDeath.SUICIDE)
