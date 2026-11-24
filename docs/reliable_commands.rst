.. _reliable_commands:

*******************
Reliable commands
*******************

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

The guard does not change the commands. It cannot pace the commands it does not hold back: a flood of configstrings
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
   * - ``qlx_reliableRing``
     - ``64``
     - 16 to 64
     - The ring size the guard assumes. The engine's ring is fixed at 64; a smaller value is for test rigs, where
       bots hardly ever fall behind, and makes the guard and the counter react earlier.

Console command ``reliable``
============================

``reliable`` prints the counters since the start or since the last reset::

    Reliable guard: on (watermark 32, burst 8, global burst 64)
    held 940, released 940, passed through 0, forced 12, peak backlog 36 of 64, peak queue 20, commands at ring size 0, held by the global limit 0

``reliable reset`` sets them to zero. The counters:

* ``held`` and ``released``: commands that waited and commands that were handed out again.
* ``passed through``: commands sent straight through because 128 waited already for the client.
* ``forced``: commands that were handed out after 40 frames regardless of the backlog.
* ``peak backlog``: the highest number of unacknowledged commands that was seen on a client.
* ``peak queue``: the most commands that waited for one client at the same time.
* ``commands at ring size``: commands that were sent to a client whose backlog had reached ``qlx_reliableRing``.
  With a ring of 64 each of them was close to costing the client its connection.
* ``held by the global limit``: commands that waited because ``qlx_reliableGlobalBurst`` was used up.

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
