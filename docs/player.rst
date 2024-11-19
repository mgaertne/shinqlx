#######
Players
#######

.. _player:
.. currentmodule:: shinqlx

.. exception:: NonexistentPlayerError

   An exception that is raised when a player that disconnected is being used as if the player were still present.

.. class:: Player(client_id[, info = None])

   :param int client_id: the ``client_id`` of the player
   :param PlayerInfo | None info: the player's pre-determined player info. (default: ``None``)

   A class that represents a player on the server. As opposed to minqlbot, attributes are all the values from when the class was instantiated. This means for instance if a player is on the blue team when you check, but then moves to red, it will still be blue when you check a second time. To update it, use :meth:`~.Player.update`. Note that if you update it and the player has disconnected, it will raise a :exc:`NonexistentPlayerError` exception.

   .. property:: cvars
      :type: dict[str, str | int]

      The cvars the player has set.

   .. property:: steam_id
      :type: int

      The ``steam_id`` of the player. **Read-only**.

   .. property:: id
      :type: int

      The ``id`` on the server of the player. **Read-only**.

   .. property:: ip
      :type: str

      The ip of the player. **Read-only**.

   .. property:: clan
      :type: str

      The clan of the player.

   .. property:: name
      :type: str

      The name of the player.

   .. property:: clean_name
      :type: str

      The cleaned name of the player, stripped by color codes. **Read-only**.

   .. property:: qport
      :type: int

      The port the player is connecting from. **Read-only**.

   .. property:: team
      :type: str

      The team of the player.

   .. property:: colors
      :type: tuple[float, float]

      The values of ``color1`` and ``color2`` in the player's :data:`cvars`.

   .. property:: model
      :type: str

      The model of the player.

   .. property:: headmodel
      :type: str

      The headmodel of the player.

   .. property:: handicap
      :type: int

      The handicap of the player.

   .. property:: autohop
      :type: bool

      The autohop setting of the player.

   .. property:: autoaction
      :type: bool

      The autoaction setting of the player.

   .. property:: predictitems
      :type: bool

      The predictitems setting of the player.

   .. property:: connection_state
      :type: int

      The connection state of the player.

      .. seealso::
         :ref:`connection_states` for the different connection states.

   .. property:: state
      :type: PlayerState | None

      The :class:`PlayerState` of the player. **Read-only**.

   .. property:: privileges
      :type: str

      The privileges of the player.

   .. property:: country
      :type: str

      The country of the player.

   .. property:: valid
      :type: bool

      Whether the player is valid. **Read-only**.

   .. property:: stats
      :type: PlayerStats | None

      The player's stats. **Read-only**.

   .. property:: ping
      :type: int

      The player's current ping. **Read-only**.

   .. property:: holdable
      :type: int

      The holdable of the player.

   .. property:: noclip
      :type: bool

      The noclip setting of the player.

   .. property:: health
      :type: int

      The health value of the player.

   .. property:: armor
      :type: int

      The armor value of the player.

   .. property:: is_alive
      :type: bool

      Whether the player is alive or not.

   .. property:: is_frozen
      :type: bool

      Whether the player is frozen.

   .. property:: score
      :type: int

      The player's score.

   .. property:: channel
      :type: Abstractchannel

      The player's client channel. **Read-only**.

   .. method:: update()

      Update the player information with the latest data. If the player disconnected it will raise an exception and invalidates a player.

      The player's name and Steam ID can still be accessed after being invalidated, but anything else will make it throw an exception too.

      :raises NonexistentPlayerError: if the player has become invalid.

   .. method:: position() -> Vector3
               position(reset = False, *, x = None, y = None, z = None) -> bool

      Gather or set the player's position.

      When called without any arguments, it returns the current position of the player.

      When ``reset`` is set to ``True``, the player position is set to `0`` for any coordinates that were not provided. Otherwise the current position coordinates are used to position the player.

      :param bool reset: Whether to reset the player's position. (default: ``False``)
      :param int | None x: The x-coordinate to set the player to. Keyword-only argument. (default = ``None``)
      :param int | None y: The y-coordinate to set the player to. Keyword-only argument. (default = ``None``)
      :param int | None z: The z-coordinate to set the player to. Keyword-only argument. (default = ``None``)
      :return:
         When called without any arguments, the current position of the player is returned.
         Otherwise returns ``False`` if the player is no longer valid, ``True`` otherwise.

   .. method:: velocity() -> Vector3
               velocity(reset = False, *, x = None, y = None, z = None) -> bool

      Gather or set the player's velocity.

      When called without any arguments, it returns the current velocity of the player.

      When ``reset`` is set to ``True``, the player's velocity is set to `0`` for any coordinates that were not provided. Otherwise the current velocity coordinates are used to set the player's velocity.

      :param bool reset: Whether to reset the player's velocity. (default: ``False``)
      :param int | None x: The x-coordinate to set the player's velocity. Keyword-only argument. (default = ``None``)
      :param int | None y: The y-coordinate to set the player's velocity. Keyword-only argument. (default = ``None``)
      :param int | None z: The z-coordinate to set the player's velocity. Keyword-only argument. (default = ``None``)
      :return:
         When called without any arguments, the current velocity of the player is returned.
         Otherwise returns ``False`` if the player is no longer valid, ``True`` otherwise.

   .. method:: weapons() -> Weapons
               weapons(reset = False, *, g = None, mg = None, sg = None, gl = None, rl = None, lg = None, rg = None, pg = None, bfg = None, gh = None, ng = None, pl = None, cg = None, hmg = None, hands = None) -> bool

      Gather or set the player's weapons.

      When called without any arguments, it returns the current weapons of the player.

      When ``reset`` is set to ``True``, the player's weapons are reset for any weapon not provided. Otherwise the current weapons the player is holding will be used as a basis.

      :param bool reset: Whether to reset player's weapons. (default: ``False``)
      :param bool | None g: Gauntlet. Keyword-only argument. (default: ``None``)
      :param bool | None mg: Machine-gun. Keyword-only argument. (default: ``None``)
      :param bool | None sg: Shotgun. Keyword-only argument. (default: ``None``)
      :param bool | None gl: Grenade launcher. Keyword-only argument. (default: ``None``)
      :param bool | None rl: Rocket launcher. Keyword-only argument. (default: ``None``)
      :param bool | None lg: Lighting gun. Keyword-only argument. (default: ``None``)
      :param bool | None rg: Railgun. Keyword-only argument. (default: ``None``)
      :param bool | None pg: Plasma gun. Keyword-only argument. (default: ``None``)
      :param bool | None bfg: BFG. Keyword-only argument. (default: ``None``)
      :param bool | None gh: Grappling hook. Keyword-only argument. (default: ``None``)
      :param bool | None ng: Nailgun. Keyword-only argument. (default: ``None``)
      :param bool | None pl: Proximity-mine launcher. Keyword-only argument. (default: ``None``)
      :param bool | None cg: Chaingun. Keyword-only argument. (default: ``None``)
      :param bool | None hmg: Heavy machinegun. Keyword-only argument. (default: ``None``)
      :param bool | None hands: Hands. Keyword-only argument. (default: ``None``)
      :return:
         When called without any arguments, the current weapons of the player are returned.
         Otherwise returns ``False`` if the player is no longer valid, ``True`` otherwise.

   .. method:: weapon() -> int
               weapon(new_weapon) -> bool

      Gather or set the player's currently held weapon.

      When called without any arguments, it returns the weapon the player is currently holding.

      When ``new_weapon`` is provided, the player's weapon is switched.

      .. seealso::
         :data:`WEAPONS <shinqlx.WEAPONS>` for the different weapon integer and string values.

      :param int | str new_weapon: The new weapons to switch to.
      :return:
         When called without any arguments, the currently held weapon of the player is returned.
         Otherwise returns ``False`` if the player is no longer valid, ``True`` otherwise.

   .. method:: ammo() -> Weapons
               ammo(reset = False, *, g = None, mg = None, sg = None, gl = None, rl = None, lg = None, rg = None, pg = None, bfg = None, gh = None, ng = None, pl = None, cg = None, hmg = None, hands = None) -> bool

      Gather or set the player's ammos.

      When called without any arguments, it returns the current ammos of the player.

      When ``reset`` is set to ``True``, the player's ammos are reset for any weapon not provided. Otherwise the current ammos the player is holding will be used as a basis.

      :param bool reset: Whether to reset player's ammos. (default: ``False``)
      :param bool | None g: Gauntlet. Keyword-only argument. (default: ``None``)
      :param bool | None mg: Machine-gun. Keyword-only argument. (default: ``None``)
      :param bool | None sg: Shotgun. Keyword-only argument. (default: ``None``)
      :param bool | None gl: Grenade launcher. Keyword-only argument. (default: ``None``)
      :param bool | None rl: Rocket launcher. Keyword-only argument. (default: ``None``)
      :param bool | None lg: Lighting gun. Keyword-only argument. (default: ``None``)
      :param bool | None rg: Railgun. Keyword-only argument. (default: ``None``)
      :param bool | None pg: Plasma gun. Keyword-only argument. (default: ``None``)
      :param bool | None bfg: BFG. Keyword-only argument. (default: ``None``)
      :param bool | None gh: Grappling hook. Keyword-only argument. (default: ``None``)
      :param bool | None ng: Nailgun. Keyword-only argument. (default: ``None``)
      :param bool | None pl: Proximity-mine launcher. Keyword-only argument. (default: ``None``)
      :param bool | None cg: Chaingun. Keyword-only argument. (default: ``None``)
      :param bool | None hmg: Heavy machinegun. Keyword-only argument. (default: ``None``)
      :param bool | None hands: Hands. Keyword-only argument. (default: ``None``)
      :return:
         When called without any arguments, the current ammos of the player are returned.
         Otherwise returns ``False`` if the player is no longer valid, ``True`` otherwise.
   .. method:: powerups() -> Powerups
               powerups(reset = False, *, quad = None, battlesuit = None, haste = None, invisibility = None, regeneration = None, invulnerability = None) -> bool

      Gather or set the player's powerups.

      When called without any arguments, it returns the current powerups of the player.

      When ``reset`` is set to ``True``, the player's powerups are reset for any powerups not provided. Otherwise the current powerups the player is holding will be used as a basis.

      :param bool reset: Whether to reset player's powerups. (default: ``False``)
      :param int | None quad: Quad damage duration in seconds. Keyword-only argument. (default = ``None``)
      :param int | None battlesuit: Battlesuit duration in seconds. Keyword-only argument. (default = ``None``)
      :param int | None haste: Haste duration in seconds. Keyword-only argument. (default = ``None``)
      :param int | None invisibility: Invisibility duration in seconds. Keyword-only argument. (default = ``None``)
      :param int | None regeneration: Regeneration duration in seconds. Keyword-only argument. (default = ``None``)
      :param int | None invulnerability: Invulnerability duration in seconds. Keyword-only argument. (default = ``None``)
      :return:
         When called without any arguments, the current powerups of the player are returned.
         Otherwise returns ``False`` if the player is no longer valid, ``True`` otherwise.

   .. method:: drop_holdable()

      Drop the player's holdable.

   .. method:: flight() -> Flight
               flight(reset = False, *, fuel = None, max_fuel = None, thrust = None, refuel = None) -> bool

      Gather or set the player's flight parameters.

      When called without any arguments, it returns the current flight parameters of the player.

      When ``reset`` is set to ``True``, the player's flight parameters are reset for any non-provided value. Otherwise the current flight parameters of the player will be used as a basis.

      :param bool reset: Whether to reset player's flight parameters. (default: ``False``)
      :param int | None fuel: Fuel. Keyword-only argument. (default = ``None``)
      :param int | None max_fuel: Maximum fuel. Keyword-only argument. (default = ``None``)
      :param int | None thrust: Thrust. Keyword-only argument. (default = ``None``)
      :param int | None refuel: Refuel. Keyword-only argument. (default = ``None``)
      :return:
         When called without any arguments, the current flight parameters of the player are returned.
         Otherwise returns ``False`` if the player is no longer valid, ``True`` otherwise.

   .. method:: center_print(msg)

      Center print the provided ``msg`` to this player.

      :param str msg: The message to center print.

   .. method:: tell(msg)

      Send this player a message only visible to them.

      :param str msg: The message to send.

   .. method:: kick(reason = "")

      Kick the player with an optional reason.

      :param str reason: The reason to display. (default: ``""``)

   .. method:: ban()

      Ban the player (permanently) from the server.

   .. method:: tempban()

      Temporarily ban the player from the server. Upon map change, the player will be allowed to connect again.

   .. method:: addadmin()

      Grant the player admin permissions.

   .. method:: addmod()

      Grant the player moderation permissions.

   .. method:: demote()

      Demote the player, i.e. stripping away all their permissions.

   .. method:: mute()

      Mute the player. Chat events will still be triggered, but chat messages will be blocked by the quake live engine.

   .. method:: unmute()

      Unmute the player.

   .. method:: put(team)

      Put a player on a specific team.

      :param str team: The team the player should be put onto.

   .. method:: addscore(score)

      Add the given score to the player.

      :param int score: The amount of score points to add to the player. Can be negative.

   .. method:: switch(other_player)
      Switch the player with ``other_player`` on the respective teams.

      :param Player other_player: the other player to switch.
      :raises ValueError: if either player is invalid or both players are on the same team.

   .. method:: slap(damage = 0)

      Slap the player with and deal the provided amount of damage.

      :param int damage: The amount of damage to deal to the player when slapping. (default: ``0``)

   .. method:: slay()
      :classmethod:

      Slay (kill) a player instantly.

   .. method:: slay_with_mod(mod) -> bool

      Slay the player with means of death.

      .. seealso::
         :ref:`means_of_death` for the different means of death value.

      :param int mod: The means of death to slay the player with.
      :return: ``False`` if the player was invalid, ``True`` otherwise.
      :raises ValueError: if the ``client_id`` is not in the range between ``0`` and ``sv_maxclients``.

   .. method:: all_players() -> list[Player]
      :classmethod:

      Retrieve all players.

      :return: a list of all players currently connected.

.. class:: AbstractDummyPlayer(name = "DummyPlayer")

   :param str name: The name for the dummy player. (default: ``"DummyPlayer"``)

.. class:: RconDummyPlayer()

   A dummy player that is used for rcon interactions with the server and reflects owner status, i.e. :attr:`steam_id <Player.steam_id>` is set to the server owner. Handle with care.
