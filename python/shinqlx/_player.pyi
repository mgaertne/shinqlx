from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Literal, TypedDict, Iterable

    import sys

    if sys.version_info >= (3, 11):
        from typing import Unpack, NotRequired
    else:
        from typing_extensions import Unpack, NotRequired

    from shinqlx import (
        PlayerInfo,
        PlayerState,
        PlayerStats,
        AbstractChannel,
        Flight,
        Weapons,
        Powerups,
        Vector3,
        UserInfo,
    )

Vector3Kwargs = TypedDict(
    "Vector3Kwargs",
    {
        "x": NotRequired[int | float],
        "y": NotRequired[int | float],
        "z": NotRequired[int | float],
    },
)

WeaponsKwargs = TypedDict(
    "WeaponsKwargs",
    {
        "g": NotRequired[bool | int],
        "mg": NotRequired[bool | int],
        "sg": NotRequired[bool | int],
        "gl": NotRequired[bool | int],
        "rl": NotRequired[bool | int],
        "lg": NotRequired[bool | int],
        "rg": NotRequired[bool | int],
        "pg": NotRequired[bool | int],
        "bfg": NotRequired[bool | int],
        "gh": NotRequired[bool | int],
        "ng": NotRequired[bool | int],
        "pl": NotRequired[bool | int],
        "cg": NotRequired[bool | int],
        "hmg": NotRequired[bool | int],
        "hands": NotRequired[bool | int],
    },
)

AmmoKwargs = TypedDict(
    "AmmoKwargs",
    {
        "g": NotRequired[int],
        "mg": NotRequired[int],
        "sg": NotRequired[int],
        "gl": NotRequired[int],
        "rl": NotRequired[int],
        "lg": NotRequired[int],
        "rg": NotRequired[int],
        "pg": NotRequired[int],
        "bfg": NotRequired[int],
        "gh": NotRequired[int],
        "ng": NotRequired[int],
        "pl": NotRequired[int],
        "cg": NotRequired[int],
        "hmg": NotRequired[int],
        "hands": NotRequired[int],
    },
)

PowerupsKwargs = TypedDict(
    "PowerupsKwargs",
    {
        "quad": NotRequired[str | float],
        "battlesuit": NotRequired[str | float],
        "haste": NotRequired[str | float],
        "invisibility": NotRequired[str | float],
        "regeneration": NotRequired[str | float],
        "invulnerability": NotRequired[str | float],
    },
)

FlightKwargs = TypedDict(
    "FlightKwargs",
    {
        "fuel": NotRequired[int],
        "max_fuel": NotRequired[int],
        "thrust": NotRequired[int],
        "refuel": NotRequired[int],
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
    def team(self) -> Literal["free", "red", "blue", "spectator"]: ...
    @team.setter
    def team(self, new_team: Literal["free", "red", "blue", "spectator"]) -> None: ...
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
    def handicap(self) -> str: ...
    @handicap.setter
    def handicap(self, value: str | int) -> None: ...
    @property
    def autohop(self) -> int: ...
    @autohop.setter
    def autohop(self, value: bool | int | str) -> None: ...
    @property
    def autoaction(self) -> int: ...
    @autoaction.setter
    def autoaction(self, value: int | str) -> None: ...
    @property
    def predictitems(self) -> int: ...
    @predictitems.setter
    def predictitems(self, value: bool | int | str) -> None: ...
    @property
    def connection_state(
        self,
    ) -> Literal["free", "zombie", "connected", "primed", "active"]: ...
    @property
    def state(self) -> PlayerState | None: ...
    @property
    def privileges(self) -> Literal["mod", "admin", "root", "banned"]: ...
    @privileges.setter
    def privileges(self, value: None | Literal["none", "mod", "admin"]) -> None: ...
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
    def position(
        self, reset: bool = ..., **kwargs: Unpack[Vector3Kwargs]
    ) -> Vector3 | bool: ...
    def velocity(
        self, reset: bool = ..., **kwargs: Unpack[Vector3Kwargs]
    ) -> bool | Vector3: ...
    def weapons(
        self, reset: bool = ..., **kwargs: Unpack[WeaponsKwargs]
    ) -> bool | Weapons: ...
    def weapon(
        self,
        new_weapon: (
            Literal[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
            | Literal[
                "g",
                "mg",
                "sg",
                "gl",
                "rl",
                "lg",
                "rg",
                "pg",
                "bfg",
                "gh",
                "ng",
                "pl",
                "cg",
                "hmg",
                "hands",
            ]
            | None
        ) = ...,
    ) -> bool | Literal[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]: ...
    def ammo(
        self, reset: bool = ..., **kwargs: Unpack[AmmoKwargs]
    ) -> bool | Weapons: ...
    def powerups(
        self, reset: bool = ..., **kwargs: Unpack[PowerupsKwargs]
    ) -> bool | Powerups: ...
    @property
    def holdable(
        self,
    ) -> (
        None
        | Literal[
            "teleporter", "medkit", "flight", "kamikaze", "portal", "invulnerability"
        ]
    ): ...
    @holdable.setter
    def holdable(
        self,
        value: (
            None
            | Literal[
                "teleporter",
                "medkit",
                "flight",
                "kamikaze",
                "portal",
                "invulnerability",
                "none",
            ]
        ),
    ) -> None: ...
    def drop_holdable(self) -> None: ...
    def flight(
        self, reset: bool = ..., **kwargs: Unpack[FlightKwargs]
    ) -> bool | Flight: ...
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
    def is_chatting(self) -> bool: ...
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
    def put(self, team: Literal["free", "red", "blue", "spectator"]) -> None: ...
    def addscore(self, score: int) -> None: ...
    def switch(self, other_player: Player) -> None: ...
    def slap(self, damage: int = ...) -> None: ...
    def slay(self) -> None: ...
    def slay_with_mod(self, mod: int) -> bool: ...

_DUMMY_USERINFO: Iterable[str]

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
