.. _event_data:
.. currentmodule:: shinqlx

##########
Event data
##########

The events ``stats``, ``game_start``, ``game_end``, ``round_end``, ``kill`` and ``death`` pass the data that the server
sends over ZMQ to the handlers as a :class:`dict` decoded from JSON. The keys are written in capitals and the values are
the ones of the server, so they can change with a Quake Live update. The type stubs of shinqlx describe the keys as
``TypedDict`` classes of the same names (``StatsData``, ``GameStartData``, ``GameEndData``, ``RoundEndData``,
``KillData`` and ``DeathData``).

All of them need ``zmq_stats_enable``, see :ref:`zmq_enable`.

.. _event_data_stats:

``stats``
=========

The data of the ``stats`` event is the envelope of every message of the server. ``DATA`` holds the payload described on
this page for the ``TYPE`` of the message.

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - ``TYPE``
     - ``DATA``
   * - ``"MATCH_STARTED"``
     - :ref:`GameStartData <event_data_game_start>`
   * - ``"ROUND_OVER"``
     - :ref:`RoundEndData <event_data_round_end>`
   * - ``"PLAYER_KILL"``
     - :ref:`KillData <event_data_kill>`
   * - ``"PLAYER_DEATH"``
     - :ref:`DeathData <event_data_kill>` (``KILLER`` may be ``None``)
   * - ``"PLAYER_MEDAL"``
     - ``MATCH_GUID``, ``MEDAL``, ``NAME``, ``STEAM_ID``, ``TIME``, ``TOTAL``, ``WARMUP``
   * - ``"PLAYER_CONNECT"``
     - ``MATCH_GUID``, ``NAME``, ``STEAM_ID``, ``TIME``, ``WARMUP``
   * - ``"PLAYER_SWITCHTEAM"``
     - ``KILLER`` (``NAME``, ``OLD_TEAM``, ``STEAM_ID``, ``TEAM``), ``MATCH_GUID``, ``TIME``, ``WARMUP``
   * - ``"MATCH_REPORT"``, ``"PLAYER_STATS"``
     - see the stubs of ``MatchReportStats`` and ``PlayerStatsStats``

.. _event_data_game_start:

``game_start``
==============

``GameStartData``, sent when the match starts.

.. list-table::
   :header-rows: 1
   :widths: 30 20 50

   * - Key
     - Type
     - Meaning
   * - ``MATCH_GUID``
     - ``str``
     - The identifier of the match.
   * - ``MAP``, ``FACTORY``, ``FACTORY_TITLE``
     - ``str``
     - The map and the game factory with its title.
   * - ``GAME_TYPE``
     - ``str``
     - The short name of the game type, for example ``"CA"``.
   * - ``SERVER_TITLE``
     - ``str``
     - The title of the server.
   * - ``CAPTURE_LIMIT``, ``FRAG_LIMIT``, ``MERCY_LIMIT``, ``ROUND_LIMIT``, ``SCORE_LIMIT``, ``TIME_LIMIT``
     - ``int``
     - The limits of the match.
   * - ``INFECTED``, ``INSTAGIB``, ``QUADHOG``, ``TRAINING``
     - ``int``
     - ``1`` if the mode is active, otherwise ``0``.
   * - ``PLAYERS``
     - ``list[dict]``
     - One entry per player with ``NAME``, ``STEAM_ID`` (a string) and ``TEAM`` (an integer).

.. _event_data_game_end:

``game_end``
============

``GameEndData``, sent when the match ends. It contains the keys of ``GameStartData`` except ``PLAYERS`` and these:

.. list-table::
   :header-rows: 1
   :widths: 30 20 50

   * - Key
     - Type
     - Meaning
   * - ``ABORTED``
     - ``bool``
     - Whether the match was aborted.
   * - ``RESTARTED``
     - ``int``
     - ``1`` if the match was restarted.
   * - ``EXIT_MSG``
     - ``str``
     - The message that the game shows at the end of the match.
   * - ``GAME_LENGTH``, ``LAST_LEAD_CHANGE_TIME``
     - ``int``
     - Times in milliseconds.
   * - ``FIRST_SCORER``, ``LAST_SCORER``, ``LAST_TEAMSCORER``
     - ``str``
     - The names of the scorers.
   * - ``TSCORE0``, ``TSCORE1``
     - ``int``
     - The scores of the red and the blue team.

.. _event_data_round_end:

``round_end``
=============

``RoundEndData``, sent at the end of a round.

.. list-table::
   :header-rows: 1
   :widths: 30 20 50

   * - Key
     - Type
     - Meaning
   * - ``MATCH_GUID``
     - ``str``
     - The identifier of the match.
   * - ``ROUND``
     - ``int``
     - The number of the round.
   * - ``TEAM_WON``
     - ``str``
     - ``"RED"``, ``"BLUE"`` or ``"DRAW"``.
   * - ``TIME``
     - ``int``
     - The time in the match in milliseconds.
   * - ``WARMUP``
     - ``bool``
     - Whether the event happened in the warmup.

.. _event_data_kill:

``kill`` and ``death``
======================

``KillData`` for ``kill`` and ``DeathData`` for ``death``. The two have the same keys. In ``DeathData`` ``KILLER`` is
``None`` if the player died from the environment.

.. list-table::
   :header-rows: 1
   :widths: 30 20 50

   * - Key
     - Type
     - Meaning
   * - ``KILLER``, ``VICTIM``
     - ``dict``
     - The state of the players when it happened, see below.
   * - ``MOD``
     - ``str``
     - The means of death without the prefix, for example ``"ROCKET_SPLASH"`` or ``"TELEFRAG"``. Note that these are
       the names of the server, not the ones of :class:`MeansOfDeath`: ``"SWITCHTEAM"`` is :data:`MOD_SWITCH_TEAMS`.
   * - ``SUICIDE``, ``TEAMKILL``, ``WARMUP``
     - ``bool``
     - Whether the kill was a suicide, a team kill, or happened in the warmup.
   * - ``MATCH_GUID``
     - ``str``
     - The identifier of the match.
   * - ``ROUND``, ``TIME``
     - ``int``
     - The round and the time in the match in milliseconds.
   * - ``TEAM_ALIVE``, ``TEAM_DEAD``, ``OTHER_TEAM_ALIVE``, ``OTHER_TEAM_DEAD``
     - ``int``
     - The number of living and dead players of the team of the victim and of the other team.

The players in ``KILLER`` and ``VICTIM`` have these keys:

.. list-table::
   :header-rows: 1
   :widths: 30 20 50

   * - Key
     - Type
     - Meaning
   * - ``NAME``, ``STEAM_ID``
     - ``str``
     - The name and the SteamID64 as a string.
   * - ``TEAM``
     - ``int``
     - The team, see :data:`TEAMS`.
   * - ``HEALTH``, ``ARMOR``, ``AMMO``, ``SPEED``
     - ``int``, ``float``
     - The state of the player (``SPEED`` is a ``float``).
   * - ``WEAPON``, ``HOLDABLE``, ``POWERUPS``
     - ``str``, ``str | None``, ``list[str] | None``
     - The weapon, the holdable item and the active powerups.
   * - ``POSITION``, ``VIEW``
     - ``dict``
     - A dictionary with ``x``, ``y`` and ``z`` as floats.
   * - ``AIRBORNE``, ``SUBMERGED``, ``BOT``
     - ``bool``
     - The state of the player.
   * - ``BOT_SKILL``
     - ``int | None``
     - The skill of a bot, otherwise ``None``.
