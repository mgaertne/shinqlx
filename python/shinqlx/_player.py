import shinqlx
from shinqlx import Player

_DUMMY_USERINFO = (
    "ui_singlePlayerActive\\0\\cg_autoAction\\1\\cg_autoHop\\0"
    "\\cg_predictItems\\1\\model\\bitterman/sport_blue\\headmodel\\crash/red"
    "\\handicap\\100\\cl_anonymous\\0\\color1\\4\\color2\\23\\sex\\male"
    "\\teamtask\\0\\rate\\25000\\country\\NO"
)


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
