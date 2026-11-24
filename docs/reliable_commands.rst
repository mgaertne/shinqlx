.. _reliable_commands:

*********************************
Reliable commands and scoreboards
*********************************

The server sends texts and statistics to a client as *reliable commands*: ``print``, ``chat``, ``tchat``, ``cp``, the
scoreboards (``scores_ca`` and the like) and, at the end of a Clan Arena match, one ``castats`` command per player. The
engine keeps 64 of them per client until the client acknowledges them. A client for which more than 64 commands wait is
dropped from the server with the message *a reliable command was cycled out*. The end of a match with many players
is the typical case: with 47 players the server sends every client 47 ``castats`` commands in the same frame, and the
scoreboard, the prints and the chat come on top.

The reliable guard of shinqlx spreads such a burst over several frames, so that the ring of a client does not run
over. It is switched on by default and needs no plugin.

What the guard does
===================

* It only holds back commands whose delay does no harm: ``castats``, ``print``, ``chat``, ``tchat``, ``cp``,
  ``scores`` and ``scores_*``. Everything the client needs in the right order, for example configstrings (``cs``),
  goes out immediately.
* A command of this kind waits when the ring of its client holds ``qlx_reliableWatermark`` or more unacknowledged
  commands, when something is already waiting for that client (to keep the order), or when the global limit of the
  frame is used up. The commands are handed out again in the following frames as soon as the client has acknowledged
  enough.
* Per frame, at most ``qlx_reliableBurst`` commands go to one client and at most ``qlx_reliableGlobalBurst`` to all
  clients together. The release starts with a different client every frame.
* A command that waited for 40 frames goes out regardless of the backlog, so that a client which stopped
  acknowledging does not keep its commands forever.
* At most 128 commands wait for one client. Further commands are sent straight through, never dropped, so the guard
  cannot lose a command.
* The ``server_command`` event is dispatched when the command is sent by the game, not when the guard hands it out.
  A plugin that changes or stops a command sees it at the same moment as without the guard.

The one change the guard makes: ``print`` commands that wait for the same client one after the other are merged into a
single ``print`` command as long as the text of both fits into one command (980 characters). A merged command takes one
slot of the ring instead of several, so a burst of console lines costs the client little. The text arrives as before; a
plugin that listens to ``server_command`` still sees every ``print`` on its own, because the event comes before the guard.
``qlx_reliableMergePrints 0`` switches the merging off. A ``print`` that has a quote in its text is never merged.

The guard does not change the other commands. It cannot pace the commands it does not hold back: a flood of configstrings
or of broadcasts to all clients (``None`` as the client) still fills the ring.

Cvars
=====

The cvars are read every 40 frames, so a change takes effect within a second. Values outside the range are limited to
it.

.. list-table::
   :header-rows: 1
   :widths: 30 12 14 44

   * - Cvar
     - Default
     - Range
     - Meaning
   * - ``qlx_reliableGuard``
     - ``1``
     -
     - ``0`` switches the guard off. The counters keep running.
   * - ``qlx_reliableWatermark``
     - ``32``
     - 8 to ring - 8
     - Backlog of a client from which commands wait.
   * - ``qlx_reliableBurst``
     - ``8``
     - 1 to 32
     - Commands that one client gets in a frame from the waiting ones.
   * - ``qlx_reliableGlobalBurst``
     - ``64``
     - 8 to 1024
     - Deferrable commands that go to all clients together in a frame.
   * - ``qlx_reliableMergePrints``
     - ``1``
     -
     - ``0`` keeps every held ``print`` as its own command, see below.
   * - ``qlx_reliableRing``
     - ``64``
     - 16 to 64
     - The ring size the guard assumes. The engine's ring is fixed at 64; a smaller value is for test rigs, where
       bots hardly ever fall behind, and makes the guard and the counter react earlier.

Console command ``reliable``
============================

``reliable`` prints the counters since the start or since the last reset::

    Reliable guard: on (watermark 32, burst 8, global burst 64, merge prints on)
    held 940, released 940, passed through 0, forced 12, peak backlog 36 of 64, peak queue 20, commands at ring size 0, held by the global limit 0, prints merged 310

``reliable reset`` sets them to zero. The counters:

* ``held`` and ``released``: commands that waited and commands that were handed out again.
* ``passed through``: commands sent straight through because 128 waited already for the client.
* ``forced``: commands that were handed out after 40 frames regardless of the backlog.
* ``peak backlog``: the highest number of unacknowledged commands that was seen on a client.
* ``peak queue``: the most commands that waited for one client at the same time.
* ``commands at ring size``: commands that were sent to a client whose backlog had reached ``qlx_reliableRing``.
  With a ring of 64 each of them was close to costing the client its connection.
* ``held by the global limit``: commands that waited because ``qlx_reliableGlobalBurst`` was used up.
* ``prints merged``: ``print`` commands that were added to a waiting ``print`` instead of taking a place of their own.

Team scoreboards
================

The engine throws away a server command of 1023 bytes or more without a message. The game builds a team scoreboard
(``scores_ca``) from the rows of all players and only counts the rows against its own limit, not the header, so with
about 20 to 25 players a scoreboard can still be too long for the engine. The game then sends a stub that shows no
players. The client can only show 8 players per team anyway.

The scoreboard trim cuts a scoreboard back before it is sent, only as far as it needs to, and only if the whole
command is longer than ``qlx_scoreboardBudget`` bytes. It works in steps and stops as soon as the scoreboard fits:

1. The values of spectators are set to zero.
2. The values of ranks beyond ``qlx_scoreboardFullPerTeam`` of each team are set to zero.
3. Ranks beyond ``qlx_scoreboardLightPerTeam`` of each team are dropped.
4. Spectator rows are dropped.
5. Rows are dropped from the bottom of the larger team, one after the other, down to ``qlx_scoreboardMinPerTeam``.

The row of the player who gets the scoreboard always stays. The number of rows in the header is adjusted, the scores of
the teams in the header are not touched. The trim knows ``scores_ca``, ``scores_tdm``, ``scores_ctf``, ``scores_ft``, ``scores_ffa`` and ``scores_rr``. In ``scores_ffa`` and ``scores_rr`` there are no teams: all rows count as one group, in the order the game sorted them, and ``qlx_scoreboardFullPerTeam`` and the other limits apply to that group. ``scores_duel`` and ``scores_ad`` cannot get long and are sent unchanged. The
``server_command`` event sees the scoreboard before it is cut back.

.. list-table::
   :header-rows: 1
   :widths: 32 12 14 42

   * - Cvar
     - Default
     - Range
     - Meaning
   * - ``qlx_scoreboardTrim``
     - ``1``
     -
     - ``0`` switches the trim off.
   * - ``qlx_scoreboardBudget``
     - ``1000``
     - 200 to 1022
     - Size in bytes of the whole command from which it is cut back.
   * - ``qlx_scoreboardFullPerTeam``
     - ``8``
     - 1 to 32
     - Ranks per team that keep their values.
   * - ``qlx_scoreboardLightPerTeam``
     - ``12``
     - full to 64
     - Ranks per team that stay, with their values set to zero.
   * - ``qlx_scoreboardMinPerTeam``
     - ``4``
     - 1 to 16
     - Ranks per team that stay in the last step.

The console command ``scoreboard`` shows the settings, the longest scoreboard seen and how often each step was the last
one that was needed. ``scoreboard reset`` clears the counters.

Statistics on the scoreboard
============================

The guard keeps the ``castats`` commands instead of dropping them, so players see the damage and weapon statistics on
the scoreboard also with 20 and more players. A plugin that stops ``castats`` itself still works, but the statistics
stay empty.

Test rig with bots
==================

Bots hardly ever fall behind with their acknowledgements, so a test rig with bots only shows the effect with a smaller
ring. A server with 48 bots in Clan Arena, ``qlx_reliableRing 16`` and a short match gave:

.. list-table::
   :header-rows: 1

   * - Guard
     - Commands at ring size
     - Peak backlog
   * - off
     - 3970 and 4071
     - 59
   * - on, default limits
     - 923 and 907
     - 48 and 44

All 2145 held commands arrived, none was dropped.
