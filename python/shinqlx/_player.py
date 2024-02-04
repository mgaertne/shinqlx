import re

import shinqlx

_DUMMY_USERINFO = (
    "ui_singlePlayerActive\\0\\cg_autoAction\\1\\cg_autoHop\\0"
    "\\cg_predictItems\\1\\model\\bitterman/sport_blue\\headmodel\\crash/red"
    "\\handicap\\100\\cl_anonymous\\0\\color1\\4\\color2\\23\\sex\\male"
    "\\teamtask\\0\\rate\\25000\\country\\NO"
)


class NonexistentPlayerError(Exception):
    """An exception that is raised when a player that disconnected is being used
    as if the player were still present.

    """


class Player:
    """A class that represents a player on the server. As opposed to minqlbot,
    attributes are all the values from when the class was instantiated. This
    means for instance if a player is on the blue team when you check, but
    then moves to red, it will still be blue when you check a second time.
    To update it, use :meth:`~.Player.update`. Note that if you update it
    and the player has disconnected, it will raise a
    :exc:`shinqlx.NonexistentPlayerError` exception.

    """

    __slots__ = ("_valid", "_id", "_info", "_userinfo", "_steam_id", "_name")

    def __init__(self, client_id, info=None):
        self._valid = True

        # Can pass own info for efficiency when getting all players and to allow dummy players.
        if info:
            self._id = client_id
            self._info = info
        else:
            self._id = client_id
            self._info = shinqlx.player_info(client_id)
            if not self._info:
                self._invalidate(
                    f"Tried to initialize a Player instance of nonexistant player {client_id}."
                )

        self._userinfo = None
        self._steam_id = self._info.steam_id

        # When a player connects, the name field in the client struct has yet to be initialized,
        # so we fall back to the userinfo and try parse it ourselves to get the name if needed.
        if self._info.name:
            self._name = self._info.name
        else:
            self._userinfo = shinqlx.parse_variables(self._info.userinfo, ordered=True)
            self._name = self._userinfo.get("name", "")

    def __repr__(self):
        if not self._valid:
            return f"{self.__class__.__name__}(INVALID:'{self.clean_name}':{self.steam_id})"

        return (
            f"{self.__class__.__name__}({self._id}:'{self.clean_name}':{self.steam_id})"
        )

    def __str__(self):
        return self.name

    def __contains__(self, key):
        return key in self.cvars

    def __getitem__(self, key):
        return self.cvars[key]

    def __eq__(self, other):
        if isinstance(other, type(self)):
            return self.steam_id == other.steam_id

        return self.steam_id == other

    def __ne__(self, other):
        return not self.__eq__(other)

    def update(self):
        """Update the player information with the latest data. If the player
        disconnected it will raise an exception and invalidates a player.
        The player's name and Steam ID can still be accessed after being
        invalidated, but anything else will make it throw an exception too.

        :raises: shinqlx.NonexistentPlayerError

        """
        self._info = shinqlx.player_info(self._id)

        if not self._info or self._steam_id != self._info.steam_id:
            self._invalidate()
            return

        if self._info.name:
            self._name = self._info.name
        else:
            self._userinfo = shinqlx.parse_variables(self._info.userinfo, ordered=True)
            self._name = self._userinfo.get("name", "")

    def _invalidate(
        self, e="The player does not exist anymore. Did the player disconnect?"
    ):
        self._valid = False
        raise NonexistentPlayerError(e)

    @property
    def cvars(self):
        if not self._valid:
            self._invalidate()

        if not self._userinfo:
            self._userinfo = shinqlx.parse_variables(self._info.userinfo, ordered=True)

        return self._userinfo.copy()

    # noinspection PyUnresolvedReferences
    @cvars.setter
    def cvars(self, new_cvars):
        new = "".join([f"\\{key}\\{new_cvars[key]}" for key in new_cvars])
        shinqlx.client_command(self.id, f'userinfo "{new}"')

    @property
    def steam_id(self):
        return self._steam_id

    @property
    def id(self):
        return self._id

    @property
    def ip(self):
        if "ip" in self:
            return self["ip"].split(":")[0]
        return ""

    @property
    def clan(self):
        """The clan tag. Not actually supported by QL, but it used to be and
        fortunately the scoreboard still properly displays it if we manually
        set the configstring to use clan tags."""
        try:
            return shinqlx.parse_variables(shinqlx.get_configstring(529 + self._id))[
                "cn"
            ]
        except KeyError:
            return ""

    # noinspection PyUnresolvedReferences
    @clan.setter
    def clan(self, tag):
        index = self.id + 529
        cs = shinqlx.parse_variables(shinqlx.get_configstring(index), ordered=True)
        cs["xcn"] = tag
        cs["cn"] = tag
        new_cs = "".join([f"\\{key}\\{cs[key]}" for key in cs])
        shinqlx.set_configstring(index, new_cs)

    @property
    def name(self):
        return self._name + "^7"

    # noinspection PyUnresolvedReferences
    @name.setter
    def name(self, value):
        new = self.cvars
        new["name"] = value
        self.cvars = new

    @property
    def clean_name(self):
        """Removes color tags from the name."""
        return re.sub(r"\^\d", "", self.name)

    @property
    def qport(self):
        if "qport" in self:
            return int(self["qport"])

        return -1

    @property
    def team(self):
        return shinqlx.TEAMS[self._info.team]

    # noinspection PyUnresolvedReferences
    @team.setter
    def team(self, new_team):
        self.put(new_team)

    @property
    def colors(self):
        # Float because they can occasionally be floats for some reason.
        return float(self["color1"]), float(self["color2"])

    # noinspection PyUnresolvedReferences
    @colors.setter
    def colors(self, value):
        new = self.cvars
        c1, c2 = value
        new["color1"] = c1
        new["color2"] = c2
        self.cvars = new

    @property
    def model(self):
        return self["model"]

    # noinspection PyUnresolvedReferences
    @model.setter
    def model(self, value):
        new = self.cvars
        new["model"] = value
        self.cvars = new

    @property
    def headmodel(self):
        return self["headmodel"]

    # noinspection PyUnresolvedReferences
    @headmodel.setter
    def headmodel(self, value):
        new = self.cvars
        new["headmodel"] = value
        self.cvars = new

    @property
    def handicap(self):
        return self["handicap"]

    # noinspection PyUnresolvedReferences
    @handicap.setter
    def handicap(self, value):
        new = self.cvars
        new["handicap"] = value
        self.cvars = new

    @property
    def autohop(self):
        return bool(int(self["cg_autoHop"]))

    # noinspection PyUnresolvedReferences
    @autohop.setter
    def autohop(self, value):
        new = self.cvars
        new["autohop"] = int(value)
        self.cvars = new

    @property
    def autoaction(self):
        return bool(int(self["cg_autoAction"]))

    # noinspection PyUnresolvedReferences
    @autoaction.setter
    def autoaction(self, value):
        new = self.cvars
        new["cg_autoAction"] = int(value)
        self.cvars = new

    @property
    def predictitems(self):
        return bool(int(self["cg_predictItems"]))

    # noinspection PyUnresolvedReferences
    @predictitems.setter
    def predictitems(self, value):
        new = self.cvars
        new["cg_predictItems"] = int(value)
        self.cvars = new

    @property
    def connection_state(self):
        """A string describing the connection state of a player.

        Possible values:
        - *free* -- The player has disconnected and the slot is free to be used by someone else.
        - *zombie* -- The player disconnected and his/her slot will be available to other players shortly.
        - *connected* -- The player connected, but is currently loading the game.
        - *primed* -- The player was sent the necessary information to play, but has yet to send commands.
        - *active* -- The player finished loading and is actively sending commands to the server.

        In other words, if you need to make sure a player is in-game, check if ``player.connection_state == "active"``.

        """
        return shinqlx.CONNECTION_STATES[self._info.connection_state]

    @property
    def state(self):
        return shinqlx.player_state(self.id)

    @property
    def privileges(self):
        if self._info.privileges == shinqlx.PRIV_MOD:
            return "mod"
        if self._info.privileges == shinqlx.PRIV_ADMIN:
            return "admin"
        if self._info.privileges == shinqlx.PRIV_ROOT:
            return "root"
        if self._info.privileges == shinqlx.PRIV_BANNED:
            return "banned"
        return None

    # noinspection PyUnresolvedReferences
    @privileges.setter
    def privileges(self, value):
        if not value or value == "none":
            shinqlx.set_privileges(self.id, shinqlx.PRIV_NONE)
        elif value == "mod":
            shinqlx.set_privileges(self.id, shinqlx.PRIV_MOD)
        elif value == "admin":
            shinqlx.set_privileges(self.id, shinqlx.PRIV_ADMIN)
        else:
            raise ValueError("Invalid privilege level.")

    @property
    def country(self):
        return self["country"]

    # noinspection PyUnresolvedReferences
    @country.setter
    def country(self, value):
        new = self.cvars
        new["country"] = value
        self.cvars = new

    @property
    def valid(self):
        return self._valid

    @property
    def stats(self):
        return shinqlx.player_stats(self.id)

    @property
    def ping(self):
        return self.stats.ping

    def position(self, reset=False, **kwargs):
        pos = shinqlx.Vector3((0, 0, 0)) if reset else self.state.position

        if not kwargs:
            return pos

        x = kwargs.get("x", pos.x)
        y = kwargs.get("y", pos.y)
        z = kwargs.get("z", pos.z)

        return shinqlx.set_position(self.id, shinqlx.Vector3((x, y, z)))

    def velocity(self, reset=False, **kwargs):
        vel = shinqlx.Vector3((0, 0, 0)) if reset else self.state.velocity

        if not kwargs:
            return vel

        x = kwargs.get("x", vel.x)
        y = kwargs.get("y", vel.y)
        z = kwargs.get("z", vel.z)

        return shinqlx.set_velocity(self.id, shinqlx.Vector3((x, y, z)))

    def weapons(self, reset=False, **kwargs):
        weaps = shinqlx.Weapons((False,) * 15) if reset else self.state.weapons

        if not kwargs:
            return weaps

        g = kwargs.get("g", weaps.g)
        mg = kwargs.get("mg", weaps.mg)
        sg = kwargs.get("sg", weaps.sg)
        gl = kwargs.get("gl", weaps.gl)
        rl = kwargs.get("rl", weaps.rl)
        lg = kwargs.get("lg", weaps.lg)
        rg = kwargs.get("rg", weaps.rg)
        pg = kwargs.get("pg", weaps.pg)
        bfg = kwargs.get("bfg", weaps.bfg)
        gh = kwargs.get("gh", weaps.gh)
        ng = kwargs.get("ng", weaps.ng)
        pl = kwargs.get("pl", weaps.pl)
        cg = kwargs.get("cg", weaps.cg)
        hmg = kwargs.get("hmg", weaps.hmg)
        hands = kwargs.get("hands", weaps.hands)

        return shinqlx.set_weapons(
            self.id,
            shinqlx.Weapons(
                (g, mg, sg, gl, rl, lg, rg, pg, bfg, gh, ng, pl, cg, hmg, hands)
            ),
        )

    def weapon(self, new_weapon=None):
        if new_weapon is None:
            return self.state.weapon
        if new_weapon in shinqlx.WEAPONS:
            pass
        elif new_weapon in shinqlx.WEAPONS.values():
            new_weapon = tuple(shinqlx.WEAPONS.values()).index(new_weapon)

        return shinqlx.set_weapon(self.id, new_weapon)

    def ammo(self, reset=False, **kwargs):
        a = shinqlx.Weapons((0,) * 15) if reset else self.state.ammo

        if not kwargs:
            return a

        g = kwargs.get("g", a.g)
        mg = kwargs.get("mg", a.mg)
        sg = kwargs.get("sg", a.sg)
        gl = kwargs.get("gl", a.gl)
        rl = kwargs.get("rl", a.rl)
        lg = kwargs.get("lg", a.lg)
        rg = kwargs.get("rg", a.rg)
        pg = kwargs.get("pg", a.pg)
        bfg = kwargs.get("bfg", a.bfg)
        gh = kwargs.get("gh", a.gh)
        ng = kwargs.get("ng", a.ng)
        pl = kwargs.get("pl", a.pl)
        cg = kwargs.get("cg", a.cg)
        hmg = kwargs.get("hmg", a.hmg)
        hands = kwargs.get("hands", a.hands)

        return shinqlx.set_ammo(
            self.id,
            shinqlx.Weapons(
                (g, mg, sg, gl, rl, lg, rg, pg, bfg, gh, ng, pl, cg, hmg, hands)
            ),
        )

    def powerups(self, reset=False, **kwargs):
        pu = shinqlx.Powerups((0,) * 6) if reset else self.state.powerups

        if not kwargs:
            return pu

        quad = pu.quad if "quad" not in kwargs else round(kwargs["quad"] * 1000)
        bs = (
            pu.battlesuit
            if "battlesuit" not in kwargs
            else round(kwargs["battlesuit"] * 1000)
        )
        haste = pu.haste if "haste" not in kwargs else round(kwargs["haste"] * 1000)
        invis = (
            pu.invisibility
            if "invisibility" not in kwargs
            else round(kwargs["invisibility"] * 1000)
        )
        regen = (
            pu.regeneration
            if "regeneration" not in kwargs
            else round(kwargs["regeneration"] * 1000)
        )
        invul = (
            pu.invulnerability
            if "invulnerability" not in kwargs
            else round(kwargs["invulnerability"] * 1000)
        )

        return shinqlx.set_powerups(
            self.id, shinqlx.Powerups((quad, bs, haste, invis, regen, invul))
        )

    @property
    def holdable(self):
        return self.state.holdable

    # noinspection PyUnresolvedReferences
    @holdable.setter
    def holdable(self, value):
        if not value:
            shinqlx.set_holdable(self.id, 0)
        elif value == "teleporter":
            shinqlx.set_holdable(self.id, 27)
        elif value == "medkit":
            shinqlx.set_holdable(self.id, 28)
        elif value == "flight":
            shinqlx.set_holdable(self.id, 34)
            self.flight(reset=True)
        elif value == "kamikaze":
            shinqlx.set_holdable(self.id, 37)
        elif value == "portal":
            shinqlx.set_holdable(self.id, 38)
        elif value == "invulnerability":
            shinqlx.set_holdable(self.id, 39)
        else:
            raise ValueError("Invalid holdable item.")

    def drop_holdable(self):
        shinqlx.drop_holdable(self.id)

    def flight(self, reset=False, **kwargs):
        state = self.state
        if state.holdable != "flight":
            self.holdable = "flight"
            reset = True

        # Set to defaults on reset.
        fl = shinqlx.Flight((16000, 16000, 1200, 0)) if reset else state.flight

        fuel = kwargs.get("fuel", fl.fuel)
        max_fuel = kwargs.get("max_fuel", fl.max_fuel)
        thrust = kwargs.get("thrust", fl.thrust)
        refuel = kwargs.get("refuel", fl.refuel)

        return shinqlx.set_flight(
            self.id, shinqlx.Flight((fuel, max_fuel, thrust, refuel))
        )

    @property
    def noclip(self):
        return self.state.noclip

    # noinspection PyUnresolvedReferences
    @noclip.setter
    def noclip(self, value):
        shinqlx.noclip(self.id, bool(value))

    @property
    def health(self):
        return self.state.health

    # noinspection PyUnresolvedReferences
    @health.setter
    def health(self, value):
        shinqlx.set_health(self.id, value)

    @property
    def armor(self):
        return self.state.armor

    # noinspection PyUnresolvedReferences
    @armor.setter
    def armor(self, value):
        shinqlx.set_armor(self.id, value)

    @property
    def is_alive(self):
        return self.state.is_alive

    # noinspection PyUnresolvedReferences
    @is_alive.setter
    def is_alive(self, value):
        if not isinstance(value, bool):
            raise ValueError("is_alive needs to be a boolean.")

        cur = self.is_alive
        if cur and value is False:
            # TODO: Proper death and not just setting health to 0.
            self.health = 0
        elif not cur and value is True:
            shinqlx.player_spawn(self.id)

    @property
    def is_frozen(self):
        return self.state.is_frozen

    @property
    def score(self):
        return self.stats.score

    # noinspection PyUnresolvedReferences
    @score.setter
    def score(self, value):
        shinqlx.set_score(self.id, value)

    @property
    def channel(self):
        return shinqlx.TellChannel(self)

    def center_print(self, msg):
        shinqlx.send_server_command(self.id, f'cp "{msg}"')

    def tell(self, msg, **kwargs):
        return shinqlx.Plugin.tell(msg, self, **kwargs)

    def kick(self, reason=""):
        return shinqlx.Plugin.kick(self, reason)

    def ban(self):
        return shinqlx.Plugin.ban(self)

    def tempban(self):
        return shinqlx.Plugin.tempban(self)

    def addadmin(self):
        return shinqlx.Plugin.addadmin(self)

    def addmod(self):
        return shinqlx.Plugin.addmod(self)

    def demote(self):
        return shinqlx.Plugin.demote(self)

    def mute(self):
        return shinqlx.Plugin.mute(self)

    def unmute(self):
        return shinqlx.Plugin.unmute(self)

    def put(self, team):
        return shinqlx.Plugin.put(self, team)

    def addscore(self, score):
        return shinqlx.Plugin.addscore(self, score)

    def switch(self, other_player):
        return shinqlx.Plugin.switch(self, other_player)

    def slap(self, damage=0):
        return shinqlx.Plugin.slap(self, damage)

    def slay(self):
        return shinqlx.Plugin.slay(self)

    def slay_with_mod(self, mod):
        return shinqlx.slay_with_mod(self.id, mod)

    @classmethod
    def all_players(cls):
        return [
            cls(info.client_id, info=info) for info in shinqlx.players_info() if info
        ]


class AbstractDummyPlayer(Player):
    def __init__(self, name="DummyPlayer"):
        info = shinqlx.PlayerInfo(
            (
                -1,
                name,
                shinqlx.CS_CONNECTED,
                _DUMMY_USERINFO,
                -1,
                shinqlx.TEAM_SPECTATOR,
                shinqlx.PRIV_NONE,
            )
        )
        super().__init__(-1, info=info)

    @property
    def id(self):
        raise AttributeError("Dummy players do not have client IDs.")

    @property
    def steam_id(self):
        raise NotImplementedError("steam_id property needs to be implemented.")

    def update(self):
        pass

    @property
    def channel(self):
        raise NotImplementedError("channel property needs to be implemented.")

    def tell(self, msg, **kwargs):
        raise NotImplementedError("tell() needs to be implemented.")


class RconDummyPlayer(AbstractDummyPlayer):
    def __init__(self):
        super().__init__(name=self.__class__.__name__)

    @property
    def steam_id(self):
        return shinqlx.owner()

    @property
    def channel(self):
        return shinqlx.CONSOLE_CHANNEL

    def tell(self, msg, **kwargs):
        self.channel.reply(msg)
