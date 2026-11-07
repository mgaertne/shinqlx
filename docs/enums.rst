.. _enums:
.. currentmodule:: shinqlx

############
Enumerations
############

.. note::
   The enumerations are only available in ``shinqlx``, they are not provided by the ``minqlx`` compatibility
   layer. The old constants like :data:`TEAM_RED` or :data:`PRI_LOW` are still there and have the values of the
   members, so existing plugins keep working.

The enumerations replace the loose integer constants and the magic strings of the API by names that can be found
by the editor and the type checker. They are plain Python classes whose members behave like the values they stand for:

.. code-block:: python

   import shinqlx

   # a member is usable wherever the API expects the integer ...
   self.add_hook("damage", self.handle_damage, priority=shinqlx.Priorities.LOW)
   shinqlx.set_weapon(player.id, shinqlx.Weapon.RAILGUN)

   # ... compares equal to it ...
   assert shinqlx.Priorities.LOW == shinqlx.PRI_LOW
   assert player.team == shinqlx.Teams.RED

   # ... and, for teams, privileges, connection states and weapons, to the string that the API used before.
   assert shinqlx.Teams.RED == "red"
   assert str(shinqlx.Weapon.ROCKET_LAUNCHER) == "rl"

Members of different enumerations are never equal, even if their integer values are: ``Teams.RED != Privileges.MOD``.
All members are immutable and hashable. The hash is the one of the integer value, so a dictionary with members as keys
cannot be searched with the string.

The enumerations that stand for a team or a privilege level are also accepted where the API took a string, e.g.
``player.team = Teams.BLUE``, ``player.put(Teams.SPECTATOR)``, ``Plugin.lock(Teams.RED)`` or
``player.privileges = Privileges.MOD``.

Teams
-----

.. class:: Teams

   The teams of Quake Live. :class:`str` of a member is the name that :attr:`Player.team` returns.

   .. attribute:: FREE
      :value: 0

      Equal to :data:`TEAM_FREE`, ``"free"``.

   .. attribute:: RED
      :value: 1

      Equal to :data:`TEAM_RED`, ``"red"``.

   .. attribute:: BLUE
      :value: 2

      Equal to :data:`TEAM_BLUE`, ``"blue"``.

   .. attribute:: SPECTATOR
      :value: 3

      Equal to :data:`TEAM_SPECTATOR`, ``"spectator"``.

Privileges
----------

.. class:: Privileges

   The privilege levels of a player. :class:`str` of a member is the name that :attr:`Player.privileges` returns.

   .. attribute:: NONE
      :value: 0

      Equal to :data:`PRIV_NONE`, ``"none"``.

   .. attribute:: MOD
      :value: 1

      Equal to :data:`PRIV_MOD`, ``"mod"``.

   .. attribute:: ADMIN
      :value: 2

      Equal to :data:`PRIV_ADMIN`, ``"admin"``.

   .. attribute:: ROOT
      :value: 3

      Equal to :data:`PRIV_ROOT`, ``"root"``.

   .. attribute:: BANNED
      :value: -1

      Equal to :data:`PRIV_BANNED`, ``"banned"``.

ConnectionStates
----------------

.. class:: ConnectionStates

   The connection states of a client, as in :attr:`Player.connection_state`. :class:`str` of a member is the
   lower-case name, e.g. ``"active"``.

   .. attribute:: FREE
      :value: 0

      Equal to :data:`CS_FREE`.

   .. attribute:: ZOMBIE
      :value: 1

      Equal to :data:`CS_ZOMBIE`.

   .. attribute:: CONNECTED
      :value: 2

      Equal to :data:`CS_CONNECTED`.

   .. attribute:: PRIMED
      :value: 3

      Equal to :data:`CS_PRIMED`.

   .. attribute:: ACTIVE
      :value: 4

      Equal to :data:`CS_ACTIVE`.

Priorities
----------

.. class:: Priorities

   The priorities of hooks and commands. The handler with the highest priority is called first.

   .. attribute:: HIGHEST
      :value: 0

      Equal to :data:`PRI_HIGHEST`.

   .. attribute:: HIGH
      :value: 1

      Equal to :data:`PRI_HIGH`.

   .. attribute:: NORMAL
      :value: 2

      Equal to :data:`PRI_NORMAL`.

   .. attribute:: LOW
      :value: 3

      Equal to :data:`PRI_LOW`.

   .. attribute:: LOWEST
      :value: 4

      Equal to :data:`PRI_LOWEST`.

ReturnCodes
-----------

.. class:: ReturnCodes

   The values that handlers of hooks and commands can return.

   .. attribute:: NONE
      :value: 0

      Equal to :data:`RET_NONE`. Continue execution normally.

   .. attribute:: STOP
      :value: 1

      Equal to :data:`RET_STOP`. Stop execution of event handlers within Python.

   .. attribute:: STOP_EVENT
      :value: 2

      Equal to :data:`RET_STOP_EVENT`. Only stop the event, but let other handlers process it.

   .. attribute:: STOP_ALL
      :value: 3

      Equal to :data:`RET_STOP_ALL`. Stop execution at an engine level.

   .. attribute:: USAGE
      :value: 4

      Equal to :data:`RET_USAGE`. Used for commands: replies to the channel with the command's usage.

Weapon
------

.. class:: Weapon

   The weapons of Quake Live. :class:`str` of a member is the short name that :meth:`Player.weapon` accepts and that
   the fields of :class:`Weapons` use, so ``getattr(player.weapons(), str(Weapon.ROCKET_LAUNCHER))`` is the amount of
   rockets the player has. The integer value is the key of :data:`WEAPONS`.

   .. list-table::
      :header-rows: 1

      * - Member
        - Value
        - Short name
      * - ``NONE``
        - 0
        - ``"none"``
      * - ``GAUNTLET``
        - 1
        - ``"g"``
      * - ``MACHINEGUN``
        - 2
        - ``"mg"``
      * - ``SHOTGUN``
        - 3
        - ``"sg"``
      * - ``GRENADE_LAUNCHER``
        - 4
        - ``"gl"``
      * - ``ROCKET_LAUNCHER``
        - 5
        - ``"rl"``
      * - ``LIGHTNING``
        - 6
        - ``"lg"``
      * - ``RAILGUN``
        - 7
        - ``"rg"``
      * - ``PLASMAGUN``
        - 8
        - ``"pg"``
      * - ``BFG``
        - 9
        - ``"bfg"``
      * - ``GRAPPLING_HOOK``
        - 10
        - ``"gh"``
      * - ``NAILGUN``
        - 11
        - ``"ng"``
      * - ``PROX_LAUNCHER``
        - 12
        - ``"pl"``
      * - ``CHAINGUN``
        - 13
        - ``"cg"``
      * - ``HMG``
        - 14
        - ``"hmg"``
      * - ``HANDS``
        - 15
        - ``"hands"``

VoteStates
----------

.. class:: VoteStates

   The states of a vote on the server.

   .. attribute:: NONE
      :value: 0

   .. attribute:: PENDING
      :value: 1

   .. attribute:: YES
      :value: 2

   .. attribute:: NO
      :value: 3

   .. attribute:: FORCE_PASS
      :value: 4

   .. attribute:: FORCE_FAIL
      :value: 5

   .. attribute:: EXPIRED
      :value: 6

CvarFlags
---------

.. class:: CvarFlags

   The flags of a cvar, e.g. for :meth:`Plugin.set_cvar`. Members can be combined with ``|`` and checked with ``&``,
   the result is a plain :class:`int`. See :ref:`cvar_flags` for the meaning of the flags.

   .. code-block:: python

      flags = shinqlx.CvarFlags.ARCHIVE | shinqlx.CvarFlags.LATCH
      shinqlx.Plugin.set_cvar("qlx_myCvar", "1", flags)

   Members: ``ARCHIVE`` (1), ``USERINFO`` (2), ``SERVERINFO`` (4), ``SYSTEMINFO`` (8), ``INIT`` (16), ``LATCH`` (32),
   ``ROM`` (64), ``USER_CREATED`` (128), ``TEMP`` (256), ``CHEAT`` (512) and ``NORESTART`` (1024). They are equal
   to the constants ``CVAR_*``.

DamageFlags
-----------

.. class:: DamageFlags

   The flags of the damage that a player takes, as passed to the ``damage`` event. Members can be combined with
   ``|`` and checked with ``&``, the result is a plain :class:`int`. See :ref:`damage_flags` for the meaning of
   the flags.

   Members: ``RADIUS`` (1), ``NO_ARMOR`` (2), ``NO_KNOCKBACK`` (4), ``NO_PROTECTION`` (8) and
   ``NO_TEAM_PROTECTION`` (16). They are equal to the constants ``DAMAGE_*``.

MeansOfDeath
------------

.. class:: MeansOfDeath

   The causes of a death, e.g. the argument of :func:`slay_with_mod`. See :ref:`means_of_death` for the meaning of
   the values. The members are named like the constants ``MOD_*`` without the prefix, e.g.
   ``MeansOfDeath.ROCKET_SPLASH`` is equal to :data:`MOD_ROCKET_SPLASH`.

   Members: ``UNKNOWN``, ``SHOTGUN``, ``GAUNTLET``, ``MACHINEGUN``, ``GRENADE``, ``GRENADE_SPLASH``, ``ROCKET``,
   ``ROCKET_SPLASH``, ``PLASMA``, ``PLASMA_SPLASH``, ``RAILGUN``, ``LIGHTNING``, ``BFG``, ``BFG_SPLASH``, ``WATER``,
   ``SLIME``, ``LAVA``, ``CRUSH``, ``TELEFRAG``, ``FALLING``, ``SUICIDE``, ``TARGET_LASER``, ``TRIGGER_HURT``,
   ``NAIL``, ``CHAINGUN``, ``PROXIMITY_MINE``, ``KAMIKAZE``, ``JUICED``, ``GRAPPLE``, ``SWITCH_TEAMS``, ``THAW``,
   ``LIGHTNING_DISCHARGE``, ``HMG`` and ``RAILGUN_HEADSHOT``.
