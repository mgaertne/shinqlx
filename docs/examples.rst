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

The command can leave the checking of the arguments to the decorator. With ``args``, the words after the command
name are converted, and the method receives them after ``channel``. A missing or invalid argument returns
:attr:`ReturnCodes.USAGE` without calling the method, and without a ``usage`` the reply is made from the parameter
names. :func:`game` skips the method unless a game is in the given state:

.. code-block:: python

   import shinqlx


   class slapper(shinqlx.Plugin):
       @shinqlx.command("slap", permission=shinqlx.Permissions.MODERATOR, args=(shinqlx.Player, int), min_args=1)
       @shinqlx.game(state=shinqlx.GameStates.IN_PROGRESS, message="There is no match to slap anyone in.")
       def cmd_slap(self, player, msg, channel, target, damage=1):
           target.slap(damage)

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

Every read of the attribute asks the server for the current value, so a change by an admin applies at once and
the plugin does not need to be reloaded. A read costs a fraction of a microsecond. Whoever needs the value many times
in a loop keeps it in a local variable.

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

Deferring calls
===============

Plugins must not block the frame. :func:`later` defers a call by a number of seconds or by a number of game frames,
and the decorated method returns at once. Pass exactly one of ``seconds`` and ``frames``.

.. code-block:: python

   import shinqlx


   class countdown(shinqlx.Plugin):
       @shinqlx.command("go", permission=shinqlx.Permissions.PLAYER)
       def cmd_go(self, player, msg, channel):
           self.msg("Ready ...")
           self.announce("Go!")
           self.follow_up()

       @shinqlx.later(seconds=3)
       def announce(self, text):
           self.msg(text)

       @shinqlx.later(frames=2)
       def follow_up(self):
           self.msg("Two frames later.")

``later(frames=1)`` is the same as :func:`next_frame`, and ``later(seconds=3)`` is the same as ``delay(3)``. Whoever sends
a long text to one player spreads it over several calls with ``later`` instead of sending it in one frame.

Listing the installed maps
==========================

.. shinqlx-only::

:func:`maps` lists the maps that are installed on the server, and :func:`map_info` tells what the files say about one
of them. The server reads them in the background, so a plugin must not rely on them while it loads: until the first
scan has finished, the list is empty. The plugin below asks a few seconds after loading and then answers its commands
from the list. Names and authors can contain colour codes, which :meth:`Plugin.clean_text` removes. The objects only
have the name of the archive, never the path on the server.

.. code-block:: python

   import shinqlx


   class maplist(shinqlx.Plugin):
       def __init__(self):
           super().__init__()
           self.maps = ()
           self.add_command("maplist", self.cmd_maplist, usage="[gametype]")
           self.add_command("mapinfo", self.cmd_mapinfo, usage="<map>")
           self.load_maps()

       @shinqlx.delay(10)
       def load_maps(self):
           self.maps = shinqlx.maps(shinqlx.GameTypes.CLAN_ARENA)
           if not self.maps:
               self.load_maps()

       def cmd_maplist(self, player, msg, channel):
           if not self.maps:
               player.tell("The maps are not read yet, try again later.")
               return

           names = ", ".join(info.name for info in self.maps if info.has_aas)
           player.tell(f"Clan Arena maps with bots: {names}")

       def cmd_mapinfo(self, player, msg, channel):
           if len(msg) < 2:
               return shinqlx.ReturnCodes.USAGE

           info = shinqlx.map_info(msg[1])
           if info is None:
               player.tell(f"{msg[1]} is not installed.")
               return

           kinds = ", ".join(source.kind for source in info.sources)
           text = f"{self.clean_text(info.longname)} ({info.name}): {', '.join(info.gametypes)}; from {kinds}"
           if not info.arenas:
               text += "; no arena block, so every game type"
           player.tell(text)

:attr:`MapInfo.gametypes` contains members of :class:`GameTypes` and :attr:`MapSource.kind` one of
:class:`MapSourceKind`; both are strings, so they can be joined and compared with their short names. A factory of the
base game has a member of :class:`Factories` as its id:

.. code-block:: python

   factory = shinqlx.factory_info(shinqlx.Factories.CLAN_ARENA)
   if factory is not None:
       shinqlx.console_print(f"{factory.id} is based on {factory.basegt}")

A running server learns of a workshop item that was added later with :func:`reload_maps`, which asks for a new scan in
the background (with the search paths read anew) and returns at once.
