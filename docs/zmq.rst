.. _zmq:
.. currentmodule:: shinqlx

#####
Stats
#####

.. _zmq_enable:

Stats without the ZMQ socket
============================

No cvar is needed for the stats events. The game module hands every stats message, and the report of a match, to
functions of the engine, and shinqlx hooks them: ``stats``, ``game_start``, ``game_end``, ``round_end``,
``team_switch``, ``kill``, ``death``, ``player_connect``, ``player_disconnect``, ``medal`` and ``flag_status`` see the
same messages as the ZMQ stats socket would carry, handed to the plugins in the next frame. The data that the events
pass is described in :ref:`event_data`.

If the hook cannot find the functions in a game module (after a Quake Live update), shinqlx logs an error and the stats
events are not dispatched.

The stats listener
==================

shinqlx can still read the stats from the ZMQ socket of the server. That code is not part of the default build, it is
behind the Cargo feature ``zmq-stats``. With the feature, the stats listener starts during the late initialization if the
cvar ``zmq_stats_enable`` is not ``0``, and the hook of the engine stays silent so that no message is dispatched twice.
The listener is the :class:`StatsListener` of the module ``_zmq``, which ``shinqlx.stats_listener()`` returns. The
module attribute ``_stats`` is ``None`` without the feature.

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Cvar
     - Meaning
   * - ``zmq_stats_enable``
     - ``1`` to read the stats from the socket with the stats listener.
   * - ``zmq_stats_ip``
     - The host to connect to. Default: ``127.0.0.1``
   * - ``zmq_stats_port``
     - The port to connect to. Default: the value of ``net_port``
   * - ``zmq_stats_password``
     - The password of the stats socket

.. class:: StatsListener

   Only with the Cargo feature ``zmq-stats``. Listens for zmq stats from the server in a thread and forwards the
   according events to the :class:`event dispatchers <EventDispatcher>`.

   .. property:: done
      :type: bool

      Whether this stats listener is shutting down.

   .. property:: address
      :type: str

      The ZMQ address this listener is connected to.

   .. property:: password
      :type: str

      The ZMQ password to use.

   .. method:: keep_receiving()

      Receives until 'self.done' is set to True.

   .. method:: stop()

      Stops the stats listener.
