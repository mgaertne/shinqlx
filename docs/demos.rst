.. _demos:
.. currentmodule:: shinqlx

#################
Recording matches
#################

.. shinqlx-only::

   The whole page: minqlx does not record demos.

shinqlx can record the matches of a server on the server itself, with the view of every player, for example so that an
admin can look at a match again after someone has reported a suspected cheater. The recordings are converted to Quake
Live demos (``.dm_91``) afterwards.

The feature is experimental. It has been tried with bots and exported demos have been played in wolfcamql, but not yet
verified in the game client, and the format of the recordings may still change.

Setting it up
=============

#. Build shinqlx with the Cargo feature ``demo-recording``, which is off by default. A plain ``pip install shinqlx``
   does not contain it. In the source tree, and in the Python environment of the server:

   .. code-block:: shell

      maturin build --release --features demo-recording
      pip install --force-reinstall target/wheels/shinqlx-*.whl

#. Switch recording on in the server configuration:

   .. code-block:: none

      set qlx_demoRecord "1"

#. To let players ask for a match to be kept, add the built-in plugin ``demos`` to ``qlx_plugins``:

   .. code-block:: none

      set qlx_plugins "..., r!demos"

The server logs ``Recording to <file>`` when a recording starts, and ``!demostatus`` shows what the recorder is doing.

What is recorded
================

One file holds the warmup and the match that follows it. Its name is ``YYYYMMDD_HHmmSS_mapname.dm_sqx``, after its
start in UTC. A recording ends

* when the game returns to the warmup after a match, finished or aborted,
* when the map ends,
* when the last human leaves (see ``qlx_demoRequireHuman``),
* after ``qlx_demoMaxMinutes``, continuing in a new file,
* when recording is switched off or there is no disk space left,
* when the server cannot write fast enough; the next attempt follows a minute later, as after a failed start.

A recording holds what the server knows in every frame: all players and the other entities of the game, with the
messages sent to the players. It does not hold the inputs of the players, so the view angles are only known as often as
the server runs frames (``sv_fps``, 40 by default).

Next to each recording the server keeps a file of the same name ending in ``.json`` with the map, the factory, start
and end, whether a match was played, who was in the game with their SteamIDs, and the requests to keep the recording.
Who is in the game is looked up once a second.

Cvars
=====

They are read once a second while the server runs.

================================ ======================= ==============================================================
cvar                             default                 description
================================ ======================= ==============================================================
``qlx_demoRecord``               ``0``                   ``1`` records matches.
``qlx_demoDir``                  ``<fs_homepath>/demos`` Where the recordings are stored. Give every server its own.
``qlx_demoRequireHuman``         ``1``                   Record only while someone other than a bot is connected,
                                                         spectators included.
``qlx_demoMaxMinutes``           ``90``                  Continue in a new file after this time.
``qlx_demoKeepUnrequestedHours`` ``24``                  Delete recordings nobody asked for after this time.
``qlx_demoKeepRequestedDays``    ``30``                  Delete requested recordings after this time.
``qlx_demoMaxTotalMB``           ``5120``                Delete the oldest recordings once all together are larger.
``qlx_demoMinFreeMB``            ``1024``                Delete the oldest recordings while the disk has less free
                                                         space.
``qlx_demoRequestPermission``    ``1``                   Permission level for ``!request demo`` of the plugin
                                                         ``demos``, read when the plugin is loaded. ``0`` lets
                                                         everyone ask.
================================ ======================= ==============================================================

For the times and sizes, ``0`` means no limit.

Keeping and deleting
====================

A recording is deleted ``qlx_demoKeepUnrequestedHours`` after its end, unless someone has asked to keep it. A player
asks with ``!request demo [reason]``, a plugin with :func:`request_demo`. In the warmup after a match the request means
that match.

The size limit and the free space delete the oldest recordings nobody asked for first, requested ones only when no
others are left; the log warns about each of those. If the disk runs full, recordings are deleted even when other files
fill it. If that is not enough, recording pauses until there is room again.

The limits are applied about once a minute and are applied in a server with the feature even while ``qlx_demoRecord``
is ``0``. Only files named like recordings and their ``.json`` files are ever deleted, but all of those in the
directory: copy a recording elsewhere to keep it for good.

Disk space and server load
==========================

In tests with 12 to 16 bots a recording grew by 16 to 20 KB per second, about 40 to 55 MB for 45 minutes. A demo
exported from it takes about 10 KB per second for each player.

The state is collected at the end of each game frame and written by a separate thread. In the same tests, collecting
took 40 to 100 microseconds per frame on average and a few hundred at most; starting a recording took about 2
milliseconds. Matches with human players have not been measured. When a recording ends, the log shows these times for
your server.

Getting demos
=============

The tool ``ql_demo`` converts a copy of a recording on any machine, without involving the server. Build it in the source
tree with ``cargo build --release -p ql_demo``:

.. code-block:: shell

   ql_demo info 20260101_200000_campgrounds.dm_sqx
   ql_demo export -p alice -d demos 20260101_200000_campgrounds.dm_sqx

Its `README <https://github.com/mgaertne/shinqlx/blob/main/ql_demo/README.md>`_ describes all options. Play a demo by
copying it into the ``demos`` folder of the client and entering ``demo <name>`` in the console. The ``ql_demo`` of
another build than the server may not be able to read a recording.

Plugins can do the same on the server with :func:`export_demo`, and the event ``demo_finished``
(:class:`DemoFinishedDispatcher`) tells them about each finished recording:

.. code-block:: python

   import os

   import shinqlx


   class demo_export(shinqlx.Plugin):
       def __init__(self):
           super().__init__()
           self.add_hook("demo_finished", self.handle_demo_finished)

       def handle_demo_finished(self, recording):
           if recording.requests:
               self.export(recording.name)

       @shinqlx.thread
       def export(self, name):
           directory = os.path.join(self.get_cvar("fs_homepath"), "exported_demos")
           shinqlx.export_demo(name, directory, include_private=False)

The plugin exports the demos of a recording that was requested while it ran. Cleaning up ``exported_demos`` is its own
task; the limits above do not apply there.

Privacy
=======

A recording contains the messages of *all* players, including team chat and tells, and its ``.json`` file their
SteamIDs. Treat both as confidential, restrict who can read the directory, and tell your players that the server
records. ``ql_demo export --no-private`` and ``include_private=False`` leave team chat and tells out of a demo. Other
messages sent to a single player, such as the answers of server plugins, stay in it, so look at a demo before passing it
on.
