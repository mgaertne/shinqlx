import shinqlx
from shinqlx import AbstractDummyPlayer


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
