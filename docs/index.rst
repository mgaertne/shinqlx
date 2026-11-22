.. highlight: python
.. module:: shinqlx

.. shinqlx documentation

#####################
shinqlx documentation
#####################

********************************
High-level classes and functions
********************************

* :ref:`Plugins <plugin>`
* :ref:`Player <player>`
* :ref:`Game <game>`
* :ref:`Channels <channels>`
* :ref:`Database <database>`
* :ref:`Built-in Rust plugins <native_plugins>`
* :ref:`Plugin examples <examples>`
* :ref:`Logging <logging>`
* :ref:`Migration from minqlx <migration>`

*******************************
Low-level classes and functions
*******************************

* :ref:`Core functions <core>`
* :ref:`Enumerations <enums>`
* :ref:`Commands <commands>`
* :ref:`EventDispatcher <events>`
* :ref:`Event data <event_data>`
* :ref:`Handlers <handlers>`
* :ref:`StatsListener <zmq>`
* `Changelog <https://github.com/mgaertne/shinqlx/blob/main/CHANGELOG.md>`_
* `Source code on GitHub <https://github.com/mgaertne/shinqlx>`_
* `License (BSD-3-Clause) <https://github.com/mgaertne/shinqlx/blob/main/LICENSE>`_

*****************************
shinqlx and minqlx
*****************************

shinqlx is compatible with minqlx. Whatever is not marked is available in both. What only shinqlx offers is marked with
a badge:

.. shinqlx-only::

For the places where shinqlx differs from minqlx, see :ref:`Migration from minqlx <migration>`.

******************
Indices and tables
******************
* :ref:`genindex`

.. toctree::
   :hidden:

   plugin
   player
   game
   channels
   database
   native_plugins
   examples
   logging
   migration
   shinqlx
   enums
   commands
   events
   event_data
   handlers
   zmq

   genindex
   modindex
   search
