from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import sys

    if sys.version_info >= (3, 11):
        from typing import NotRequired
    else:
        from typing_extensions import NotRequired
    from typing import Iterable, TypedDict
    from shinqlx import (
        PlayerInfo,
        PlayerState,
        PlayerStats,
        Vector3,
        Weapons,
        Powerups,
        Flight,
        AbstractChannel,
    )

_DUMMY_USERINFO: Iterable[str]

UserInfo = TypedDict(
    "UserInfo",
    {
        "ip": str,
        "ui_singlePlayerActive": str,
        "cg_autoAction": str,
        "cg_autoHop": str,
        "cg_predictItems": str,
        "model": str,
        "headmodel": str,
        "cl_anonymous": str,
        "country": str,
        "color1": str,
        "rate": str,
        "color2": str,
        "sex": str,
        "teamtask": str,
        "name": str,
        "handicap": str,
        "password": NotRequired[str],
    },
)

class NonexistentPlayerError(Exception): ...

class Player:
    @classmethod
    def all_players(cls) -> list[Player]: ...

    _valid: bool
    _id: int
    _info: PlayerInfo | None
    _userinfo: UserInfo | None
    _steam_id = int
    _name: str

    def __init__(self, client_id: int, info: PlayerInfo | None = ...) -> None: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def __contains__(self, key: str) -> bool: ...
    def __getitem__(self, key: str) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    def __ne__(self, other: object) -> bool: ...
    def update(self) -> None: ...
    def _invalidate(self, e: str = ...) -> None: ...
    @property
    def cvars(self) -> dict[str, str | int]: ...
    @cvars.setter
    def cvars(self, new_cvars: dict[str, str | int]) -> None: ...
    @property
    def steam_id(self) -> int: ...
    @property
    def id(self) -> int: ...
    @property
    def ip(self) -> str: ...
    @property
    def clan(self) -> str: ...
    @clan.setter
    def clan(self, tag: str) -> None: ...
    @property
    def name(self) -> str: ...
    @name.setter
    def name(self, value: str) -> None: ...
    @property
    def clean_name(self) -> str: ...
    @property
    def qport(self) -> int: ...
    @property
    def team(self) -> str: ...
    @team.setter
    def team(self, new_team: str) -> None: ...
    @property
    def colors(self) -> tuple[float, float]: ...
    @colors.setter
    def colors(self, value: tuple[float, float]) -> None: ...
    @property
    def model(self) -> str: ...
    @model.setter
    def model(self, value: str) -> None: ...
    @property
    def headmodel(self) -> str: ...
    @headmodel.setter
    def headmodel(self, value: str) -> None: ...
    @property
    def handicap(self) -> int: ...
    @handicap.setter
    def handicap(self, value: int) -> None: ...
    @property
    def autohop(self) -> bool: ...
    @autohop.setter
    def autohop(self, value: bool) -> None: ...
    @property
    def autoaction(self) -> bool: ...
    @autoaction.setter
    def autoaction(self, value: bool) -> None: ...
    @property
    def predictitems(self) -> bool: ...
    @predictitems.setter
    def predictitems(self, value: bool) -> None: ...
    @property
    def connection_state(self) -> int: ...
    @property
    def state(self) -> PlayerState | None: ...
    @property
    def privileges(self) -> str: ...
    @privileges.setter
    def privileges(self, value: None | str) -> None: ...
    @property
    def country(self) -> str: ...
    @country.setter
    def country(self, value: str) -> None: ...
    @property
    def valid(self) -> bool: ...
    @property
    def stats(self) -> PlayerStats | None: ...
    @property
    def ping(self) -> int: ...
    def position(self, reset: bool = ..., **kwargs: int) -> bool | Vector3: ...
    def velocity(self, reset: bool = ..., **kwargs: int) -> bool | Vector3: ...
    def weapons(self, reset: bool = ..., **kwargs: bool) -> bool | Weapons: ...
    def weapon(self, new_weapon: int | str | None = ...) -> bool | int: ...
    def ammo(self, reset: bool = ..., **kwargs: int) -> bool | Weapons: ...
    def powerups(self, reset: bool = ..., **kwargs: int) -> bool | Powerups: ...
    @property
    def holdable(self) -> int: ...
    @holdable.setter
    def holdable(self, value: str | None) -> None: ...
    def drop_holdable(self) -> None: ...
    def flight(self, reset: bool = ..., **kwargs: int) -> bool | Flight: ...
    @property
    def noclip(self) -> bool: ...
    @noclip.setter
    def noclip(self, value: bool | int | str) -> None: ...
    @property
    def health(self) -> int: ...
    @health.setter
    def health(self, value: int) -> None: ...
    @property
    def armor(self) -> int: ...
    @armor.setter
    def armor(self, value: int) -> None: ...
    @property
    def is_alive(self) -> bool: ...
    @is_alive.setter
    def is_alive(self, value: bool) -> None: ...
    @property
    def is_frozen(self) -> bool: ...
    @property
    def score(self) -> int: ...
    @score.setter
    def score(self, value: int) -> None: ...
    @property
    def channel(self) -> AbstractChannel: ...
    def center_print(self, msg: str) -> None: ...
    def tell(self, msg: str, **kwargs: str) -> None: ...
    def kick(self, reason: str = ...) -> None: ...
    def ban(self) -> None: ...
    def tempban(self) -> None: ...
    def addadmin(self) -> None: ...
    def addmod(self) -> None: ...
    def demote(self) -> None: ...
    def mute(self) -> None: ...
    def unmute(self) -> None: ...
    def put(self, team: str) -> None: ...
    def addscore(self, score: int) -> None: ...
    def switch(self, other_player: Player) -> None: ...
    def slap(self, damage: int = ...) -> None: ...
    def slay(self) -> None: ...
    def slay_with_mod(self, mod: int) -> bool: ...

class AbstractDummyPlayer(Player):
    def __init__(self, name: str = ...) -> None: ...
    @property
    def id(self) -> int: ...
    @property
    def steam_id(self) -> int: ...
    def update(self) -> None: ...
    @property
    def channel(self) -> AbstractChannel: ...
    def tell(self, msg: str, **kwargs: str) -> None: ...

class RconDummyPlayer(AbstractDummyPlayer):
    def __init__(self) -> None: ...
    @property
    def steam_id(self) -> int: ...
    @property
    def channel(self) -> AbstractChannel: ...
    def tell(self, msg: str, **kwargs: str) -> None: ...
