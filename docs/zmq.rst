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
     - ``1`` to read the stats from the socket with a :class:`StatsListener`. With ``0`` the events get the same
       messages from the hooks of shinqlx.
   * - ``zmq_stats_ip``
     - The host to connect to. Default: ``127.0.0.1``
   * - ``zmq_stats_port``
     - The port to connect to. Default: the value of ``net_port``
   * - ``zmq_stats_password``
     - The password of the stats socket

No event needs the stats socket any more. The game module hands every stats message, and the report of a match, to
functions of the engine, and shinqlx hooks them: ``stats``, ``game_start``, ``game_end``, ``round_end``,
``team_switch``, ``kill`` and ``death`` work without ``zmq_stats_enable`` when it is ``0``. They then see the same
messages as from the socket, handed to the plugins in the next frame. With ``zmq_stats_enable`` set the messages come
from the socket, as before, and the :class:`StatsListener` runs. The data that the events pass is described in
:ref:`event_data`.

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
