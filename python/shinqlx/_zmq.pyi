from typing import TYPE_CHECKING
from threading import Thread

if TYPE_CHECKING:
    from typing import TypedDict
    from shinqlx import StatsData, GameStartData, GameEndData, RoundEndData, DeathData

TeamSwitchPlayerData = TypedDict(
    "TeamSwitchPlayerData", {"NAME": str, "OLD_TEAM": str, "STEAM_ID": str, "TEAM": str}
)

TeamSwitchData = TypedDict(
    "TeamSwitchData",
    {
        "KILLER": TeamSwitchPlayerData,
        "MATCH_GUID": str,
        "TIME": int,
        "WARMUP": bool,
    },
)

def dispatch_stats_event(stats: StatsData) -> None: ...
def dispatch_game_start_event(data: GameStartData) -> None: ...
def dispatch_round_end_event(data: RoundEndData) -> None: ...
def dispatch_game_end_event(data: GameEndData) -> None: ...
def dispatch_player_death_event(data: DeathData) -> None: ...
def dispatch_team_switch_event(data: TeamSwitchData) -> None: ...

class StatsListener(Thread):
    done: bool
    address: str
    password: str | None
    _in_progress: bool

    def __init__(self) -> None: ...
    def keep_receiving(self) -> None: ...
    def run(self) -> None: ...
    def stop(self) -> None: ...
