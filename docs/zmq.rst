.. _zmq:
.. currentmodule:: shinqlx

#############
StatsListener
#############

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