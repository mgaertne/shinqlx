from typing import TYPE_CHECKING, overload

if TYPE_CHECKING:
    from typing import ClassVar, Type, Callable, Iterable, Mapping, Literal

    from logging import Logger

    from shinqlx import (
        Command,
        Player,
        Game,
        CancellableEventReturn,
        UncancellableEventReturn,
        AbstractChannel,
        StatsData,
        GameStartData,
        GameEndData,
        RoundEndData,
        KillData,
        DeathData,
        UserinfoEventInput,
        UserInfo,
    )
    from shinqlx.database import Redis

class Plugin:
    _loaded_plugins: ClassVar[dict[str, Plugin]] = ...
    database: ClassVar[Type[Redis] | None] = ...

    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[str] = ...) -> str | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[bool]) -> bool | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[int]) -> int | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[float]) -> float | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[list]) -> list[str] | None: ...
    @classmethod
    @overload
    def get_cvar(cls, name: str, return_type: Type[set]) -> set[str] | None: ...
    @classmethod
    @overload
    def get_cvar(
        cls, name: str, return_type: Type[tuple]
    ) -> tuple[str, ...] | None: ...
    @classmethod
    def set_cvar(
        cls,
        name: str,
        value: str | bool | int | float | list | set | tuple,
        flags: int = ...,
    ) -> bool: ...
    @classmethod
    def set_cvar_limit(
        cls,
        name: str,
        value: int | float,
        minimum: int | float,
        maximum: int | float,
        flags: int = ...,
    ) -> bool: ...
    @classmethod
    def set_cvar_once(
        cls,
        name: str,
        value: str | bool | int | float | list | set | tuple,
        flags: int = ...,
    ) -> bool: ...
    @classmethod
    def set_cvar_limit_once(
        cls,
        name: str,
        value: int | float,
        minimum: int | float,
        maximum: int | float,
        flags: int = ...,
    ) -> bool: ...
    @classmethod
    def players(cls) -> list[Player]: ...
    @classmethod
    def player(
        cls, name: str | int | Player, player_list: Iterable[Player] | None = ...
    ) -> Player | None: ...
    @classmethod
    def msg(cls, msg: str, chat_channel: str = ..., **kwargs: str) -> None: ...
    @classmethod
    def console(cls, text: str) -> None: ...
    @classmethod
    def clean_text(cls, text: str) -> str: ...
    @classmethod
    def colored_name(
        cls, name: str | Player, player_list: Iterable[Player] | None = ...
    ) -> str | None: ...
    @classmethod
    def client_id(
        cls, name: str | int | Player, player_list: Iterable[Player] | None = ...
    ) -> int | None: ...
    @classmethod
    def find_player(
        cls, name: str, player_list: Iterable[Player] | None = ...
    ) -> list[Player]: ...
    @classmethod
    def teams(
        cls, player_list: Iterable[Player] | None = ...
    ) -> Mapping[str, list[Player]]: ...
    @classmethod
    def center_print(
        cls, msg: str, recipient: str | int | Player | None = ...
    ) -> None: ...
    @classmethod
    def tell(cls, msg: str, recipient: str | int | Player, **kwargs: str) -> None: ...
    @classmethod
    def is_vote_active(cls) -> bool: ...
    @classmethod
    def current_vote_count(cls) -> tuple[int, int] | None: ...
    @classmethod
    def callvote(cls, vote: str, display: str, time: int = ...) -> bool: ...
    @classmethod
    def force_vote(cls, pass_it: bool) -> bool: ...
    @classmethod
    def teamsize(cls, size: int) -> None: ...
    @classmethod
    def kick(cls, player: str | int | Player, reason: str = ...) -> None: ...
    @classmethod
    def shuffle(cls) -> None: ...
    @classmethod
    def cointoss(cls) -> None: ...
    @classmethod
    def change_map(cls, new_map: str, factory: str | None = ...) -> None: ...
    @classmethod
    def switch(cls, player: Player, other_player: Player) -> None: ...
    @classmethod
    def play_sound(cls, sound_path: str, player: Player | None = ...) -> bool: ...
    @classmethod
    def play_music(cls, music_path: str, player: Player | None = ...) -> bool: ...
    @classmethod
    def stop_sound(cls, player: Player | None = ...) -> None: ...
    @classmethod
    def stop_music(cls, player: Player | None = ...) -> None: ...
    @classmethod
    def slap(cls, player: str | int | Player, damage: int = ...) -> None: ...
    @classmethod
    def slay(cls, player: str | int | Player) -> None: ...
    @classmethod
    def timeout(cls) -> None: ...
    @classmethod
    def timein(cls) -> None: ...
    @classmethod
    def allready(cls) -> None: ...
    @classmethod
    def pause(cls) -> None: ...
    @classmethod
    def unpause(cls) -> None: ...
    @classmethod
    def lock(cls, team: str | None = ...) -> None: ...
    @classmethod
    def unlock(cls, team: str | None = ...) -> None: ...
    @classmethod
    def put(cls, player: Player, team: str) -> None: ...
    @classmethod
    def mute(cls, player: Player) -> None: ...
    @classmethod
    def unmute(cls, player: Player) -> None: ...
    @classmethod
    def tempban(cls, player: Player) -> None: ...
    @classmethod
    def ban(cls, player: Player) -> None: ...
    @classmethod
    def unban(cls, player: Player) -> None: ...
    @classmethod
    def opsay(cls, msg: str) -> None: ...
    @classmethod
    def addadmin(cls, player: Player) -> None: ...
    @classmethod
    def addmod(cls, player: Player) -> None: ...
    @classmethod
    def demote(cls, player: Player) -> None: ...
    @classmethod
    def abort(cls) -> None: ...
    @classmethod
    def addscore(cls, player: Player, score: int) -> None: ...
    @classmethod
    def addteamscore(cls, team: str, score: int) -> None: ...
    @classmethod
    def setmatchtime(cls, time: int) -> None: ...
    def __init__(self) -> None:
        self._hooks: list[tuple[str, Callable, int]] = ...
        self._commands: list[Command] = ...
        self._db_instance: Redis | None = ...
        ...

    def __str__(self) -> str: ...
    @property
    def db(self) -> Redis | None: ...
    @property
    def name(self) -> str: ...
    @property
    def plugins(self) -> Mapping[str, Plugin]: ...
    @property
    def hooks(self) -> Iterable[tuple[str, Callable, int]]: ...
    @property
    def commands(self) -> Iterable[Command]: ...
    @property
    def game(self) -> Game | None: ...
    @property
    def logger(self) -> Logger: ...
    @overload
    def add_hook(
        self,
        event: Literal["console_print"],
        handler: Callable[
            [str],
            str | CancellableEventReturn,
        ],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["command"],
        handler: Callable[
            [Player, Command, str],
            CancellableEventReturn,
        ],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["client_command"],
        handler: Callable[[Player | None, str], str | bool | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["server_command"],
        handler: Callable[[Player | None, str], str | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["frame"],
        handler: Callable[[], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["set_configstring"],
        handler: Callable[[int, str], str | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["chat"],
        handler: Callable[[Player, str, AbstractChannel], str | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["unload"],
        handler: Callable[[Plugin], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_connect"],
        handler: Callable[[Player], str | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_loaded"],
        handler: Callable[[Player], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_disconnect"],
        handler: Callable[[Player, str | None], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["player_spawn"],
        handler: Callable[[Player], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["stats"],
        handler: Callable[[StatsData], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote_called"],
        handler: Callable[[Player, str, str | None], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote_started"],
        handler: Callable[[Player, str, str | None], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote_ended"],
        handler: Callable[
            [tuple[int, int], str, str | None, bool], CancellableEventReturn
        ],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["vote"],
        handler: Callable[[Player, bool], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["game_countdown"],
        handler: Callable[[], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["game_start"],
        handler: Callable[[GameStartData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["game_end"],
        handler: Callable[[GameEndData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["round_countdown"],
        handler: Callable[[int], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["round_start"],
        handler: Callable[[int], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["round_end"],
        handler: Callable[[RoundEndData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["team_switch"],
        handler: Callable[[Player, str, str], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["team_switch_attempt"],
        handler: Callable[[Player, str, str], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["map"],
        handler: Callable[[str, str], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["new_game"],
        handler: Callable[[], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["kill"],
        handler: Callable[[Player, Player, KillData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["death"],
        handler: Callable[[Player, Player | None, DeathData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["userinfo"],
        handler: Callable[
            [Player, UserinfoEventInput], UserInfo | CancellableEventReturn
        ],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["kamikaze_use"],
        handler: Callable[[Player], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["kamikaze_explde"],
        handler: Callable[[Player, bool], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def add_hook(
        self,
        event: Literal["damage"],
        handler: Callable[
            [Player | int | None, Player | int | None, int, int, int],
            UncancellableEventReturn,
        ],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["console_print"],
        handler: Callable[
            [str],
            str | CancellableEventReturn,
        ],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["command"],
        handler: Callable[
            [Player, Command, str],
            CancellableEventReturn,
        ],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["client_command"],
        handler: Callable[[Player | None, str], str | bool | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["server_command"],
        handler: Callable[[Player | None, str], str | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["frame"],
        handler: Callable[[], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["set_configstring"],
        handler: Callable[[int, str], str | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["chat"],
        handler: Callable[[Player, str, AbstractChannel], str | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["unload"],
        handler: Callable[[Plugin], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_connect"],
        handler: Callable[[Player], str | CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_loaded"],
        handler: Callable[[Player], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_disconnect"],
        handler: Callable[[Player, str | None], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["player_spawn"],
        handler: Callable[[Player], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["stats"],
        handler: Callable[[StatsData], UncancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote_called"],
        handler: Callable[[Player, str, str | None], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote_started"],
        handler: Callable[[Player, str, str | None], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote_ended"],
        handler: Callable[
            [tuple[int, int], str, str | None, bool], CancellableEventReturn
        ],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["vote"],
        handler: Callable[[Player, bool], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["game_countdown"],
        handler: Callable[[], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["game_start"],
        handler: Callable[[GameStartData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["game_end"],
        handler: Callable[[GameEndData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["round_countdown"],
        handler: Callable[[int], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["round_start"],
        handler: Callable[[int], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["round_end"],
        handler: Callable[[RoundEndData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["team_switch"],
        handler: Callable[[Player, str, str], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["team_switch_attempt"],
        handler: Callable[[Player, str, str], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["map"],
        handler: Callable[[str, str], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["new_game"],
        handler: Callable[[], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["kill"],
        handler: Callable[[Player, Player, KillData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["death"],
        handler: Callable[[Player, Player | None, DeathData], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["userinfo"],
        handler: Callable[
            [Player, UserinfoEventInput], UserInfo | CancellableEventReturn
        ],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["kamikaze_use"],
        handler: Callable[[Player], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["kamikaze_explde"],
        handler: Callable[[Player, bool], CancellableEventReturn],
        priority: int = ...,
    ) -> None: ...
    @overload
    def remove_hook(
        self,
        event: Literal["damage"],
        handler: Callable[
            [Player | int | None, Player | int | None, int, int, int],
            UncancellableEventReturn,
        ],
        priority: int = ...,
    ) -> None: ...
    def add_command(
        self,
        name: str | Iterable[str],
        handler: Callable[
            [Player, str | list[str], AbstractChannel], CancellableEventReturn
        ],
        permission: int = ...,
        channels: Iterable[AbstractChannel] | None = ...,
        exclude_channels: Iterable[AbstractChannel] = ...,
        priority: int = ...,
        client_cmd_pass: bool = ...,
        client_cmd_perm: int = ...,
        prefix: bool = ...,
        usage: str = ...,
    ) -> None: ...
    def remove_command(
        self,
        name: Iterable[str],
        handler: Callable[[Player, str, AbstractChannel], CancellableEventReturn],
    ) -> None: ...
