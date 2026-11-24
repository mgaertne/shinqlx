.. _zmq:
.. currentmodule:: shinqlx

#############
StatsListener
#############

.. _zmq_enable:

Enabling the stats
==================

The stats listener starts during the late initialization if the cvar ``zmq_stats_enable`` is not ``0``. Set the cvar on
the command line of the server, together with the cvars that select where the listener connects to:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Cvar
     - Meaning
   * - ``zmq_stats_enable``
     - ``1`` to enable the stats. Without it no :class:`StatsListener` runs. ``game_end`` is then not dispatched, and
       hooking it raises an :exc:`AssertionError`.
   * - ``zmq_stats_ip``
     - The host to connect to. Default: ``127.0.0.1``
   * - ``zmq_stats_port``
     - The port to connect to. Default: the value of ``net_port``
   * - ``zmq_stats_password``
     - The password of the stats socket

The only event that still needs the stats of the ZMQ socket is ``game_end``, listed in :ref:`events` with
``need_zmq_stats_enabled = True``. The engine builds the ``MATCH_REPORT`` message of the end of a match itself. All other
stats messages come from the game module, and shinqlx hooks the engine function that they pass through: ``stats``,
``game_start``, ``round_end``, ``team_switch``, ``kill`` and ``death`` work without ``zmq_stats_enable`` when it is ``0``.
They then see the same messages as from the socket, handed to the plugins in the next frame. With ``zmq_stats_enable``
set the messages come from the socket, as before. The data that the events pass is described in :ref:`event_data`.

.. class:: StatsListener

   A :class:`threading.Thread` sub-class to listen for zmq stats from the server and forward the according events to the :class:`event dispatchers <EventDispatcher>`. Those are the ones that need zmq to be enabled.

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

   .. method:: run()

      Starts the stats listener.

   .. method:: stop()

      Stops the stats listener.
