####
Game
####

.. _game:
.. currentmodule:: shinqlx

.. exception:: NonexistentGameError

   An exception raised when accessing properties on an invalid game.

.. class:: Game([cached = True])

   :param bool cached:
      .. deprecated:: 0.5.11
         will be removed in future versions

   A class representing the game. That is, stuff like what map is being played, if it's in warmup, and so on. It also has methods to call in timeins, aborts, pauses, and so on.

   .. property:: cvars
      :type: dict[str, str]

      A dictionary of unprocessed cvars. Use attributes whenever possible, but since some cvars might not have attributes on this class, this could be useful. **Read-only**.

   .. property:: type
      :type: str

      The (long) gametype of this game. **Read-only**.

      .. seealso::
         :data:`GAMETYPES <shinqlx.GAMETYPES>` for the different game types.

   .. property:: type_short
      :type: str

      The (short) gametype of this game. **Read-only**.

   .. property:: map
      :type: str

      The (short) map name of the game.

   .. property:: map_title
      :type: str

      The full name of the map. Ex.: ``Longest Yard``. **Read-only**.

   .. property:: map_subtitle1
      :type: str

      The map's subtitle. Usually either empty or has the author's name. **Read-only**.

   .. property:: map_subtitle2
      :type: str

      The map's second subtitle. Usually either empty or has the author's name. **Read-only**.

   .. property:: red_score
      :type: int

      The current score for the red team. **Read-only**.

   .. property:: blue_score
      :type: int

      The current score for the blue team. **Read-only**.

   .. property:: state
      :type: str

      A string describing the state of the game. **Read-only**.

      Possible values:

      * ``"warmup"`` -- The game has yet to start and is waiting for players to ready up.
      * ``"countdown"`` -- Players recently readied up, and it's counting down until the game starts.
      * ``"in_progress"`` -- The game is in progress.

   .. property:: factory
      :type: str

      The (short) factory the current game runs on.

   .. property:: factory_title
      :type: str

      The (long) factory title the current game runs on. **Read-only**.

   .. property:: hostname
      :type: str

      The hostname as set in ``sv_hostname``. **Read-only**.

   .. property:: instagib
      :type: bool

      The instagib setting in the current game.

   .. property:: loadout
      :type: bool

      The loadout setting in the current game.

   .. property:: maxclients
      :type: int

      The maximum amount of players allowed on the server.

   .. property:: timelimit
      :type: int

      The time limit for the current game.

   .. property:: fraglimit
      :type: int

      The frag limit for the current game.

   .. property:: roundlimit
      :type: int

      The round limit for the current game.

   .. property:: roundtimelimit
      :type: int

      The round time limit for the current game.

   .. property:: scorelimit
      :type: int

      The score limit for the current game.

   .. property:: capturelimit
      :type: int

      The capture limit for the current game.

   .. property:: teamsize
      :type: int

      The team size for the current game.

   .. property:: tags
      :type: list[str]

      The tags for the current game.

   .. property:: workshop_items
      :type: list[int]

      The workshop items currently in use.

   .. method:: shuffle()
      :classmethod:

      Shuffle the players.

   Admin commands
   --------------

   .. method:: timeout()
      :classmethod:

      Time-out the game.

   .. method:: timein()
      :classmethod:

      Time-in a game that was :meth:`timeout` ed.

   .. method:: allready()
      :classmethod:

      Set all players to ready-up.

   .. method:: pause()
      :classmethod:

      Pause the current game.

   .. method:: unpause()
      :classmethod:

      Unpause a :meth:`pause` ed game.

   .. method:: lock(team=None)
      :classmethod:

      Lock all teams or just the given team.

      :param str | None team: The team to lock. If ``None``, all teams will be locked. (default: ``None``)

   .. method:: unlock(team=None)
      :classmethod:

      Unlock all teams or just the given team.

      :param str | None team: The team to unlock. If ``None``, all teams will be unlocked. (default: ``None``)

   .. method:: put(player, team)
      :classmethod:

      Put a player on a specific team.

      :param Player player: The player to move.
      :param str team: The team the player should be put onto.

   .. method:: mute(player)
      :classmethod:

      Mute the given player. Chat events will still be triggered, but chat messages will be blocked by the quake live engine.

      :param Player player: The player to mute.

   .. method:: unmute(player)
      :classmethod:

      Unmute a :meth:`mute` ed player.

      :param Player player: The player to unmute.

   .. method:: tempban(player)
      :classmethod:

      Temporarily ban a player from the server. Upon map change, the player will be allowed to connect again.

      :param Player player: The player to temporarily ban.

   .. method:: ban(player)
      :classmethod:

      Ban a player (permanently) from the server.

      :param Player player: The player to ban.

   .. method:: unban(player)
      :classmethod:

      Unban a player from the server that was :meth:`ban` ed before.

      :param Player player: The player to unban.

   .. method:: opsay(msg)
      :classmethod:

      Send a message as an operator of the server.

      :param str msg: The message to deliver.

   .. method:: addadmin(player)
      :classmethod:

      Grant the given player admin permissions.

      :param Player player: The player to promote to an admin.

   .. method:: addmod(player)
      :classmethod:

      Grant the given player moderation permissions.

      :param Player player: The player to promote to a moderator.

   .. method:: demote(player)
      :classmethod:

      Demote a player, i.e. stripping away all their permissions.

      :param Player player: The player to demote.

   .. method:: abort()
      :classmethod:

      Abort the current game, setting it back to warm-up.

   .. method:: addscore(player, score)
      :classmethod:

      Add the given score to the given player.

      :param Player player: The player to add score points to.
      :param int score: The amount of score points to add to the player. Can be negative.

   .. method:: addteamscore(team, score)
      :classmethod:

      Add the given score to the given team.

      :param str team: The team to add score points to.
      :param int score: The amount of score points to add to the team. Can be negative.

   .. method:: setmatchtime(time)
      :classmethod:

      Set the match time to the one provided.

      :param int time: The new match time.
