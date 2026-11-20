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
     - ``1`` to enable the stats. Without it no :class:`StatsListener` runs and the events that need the stats are not
       dispatched. Hooking such an event then raises an :exc:`AssertionError`.
   * - ``zmq_stats_ip``
     - The host to connect to. Default: ``127.0.0.1``
   * - ``zmq_stats_port``
     - The port to connect to. Default: the value of ``net_port``
   * - ``zmq_stats_password``
     - The password of the stats socket

The events that need the stats are listed in :ref:`events` with ``need_zmq_stats_enabled = True``. The data that they
pass is described in :ref:`event_data`.

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
