"""Subscribes to the ZMQ stats protocol and calls the stats event dispatcher when
we get stats from it."""

import zmq
import json

import shinqlx
from threading import Thread


@shinqlx.next_frame
def dispatch_stats_event(stats):
    shinqlx.EVENT_DISPATCHERS["stats"].dispatch(stats)


@shinqlx.next_frame
def dispatch_game_start_event(data):
    shinqlx.EVENT_DISPATCHERS["game_start"].dispatch(data)


@shinqlx.next_frame
def dispatch_round_end_event(data):
    shinqlx.EVENT_DISPATCHERS["round_end"].dispatch(data)


@shinqlx.next_frame
def dispatch_game_end_event(data):
    # MATCH_REPORT event goes off with a map change and map_restart,
    # but we really only want it for when the game actually ends.
    # We use a variable instead of Game().state because by the
    # time we get the event, the game is probably gone.
    shinqlx.EVENT_DISPATCHERS["game_end"].dispatch(data)


@shinqlx.next_frame
def dispatch_player_death_event(data):
    # Dead player.
    sid_victim = int(data["VICTIM"]["STEAM_ID"])
    victim = (
        shinqlx.Plugin.player(sid_victim)
        if sid_victim
        else shinqlx.Plugin.player(data["VICTIM"]["NAME"])
    )

    # Killer player.
    if not data["KILLER"]:
        killer = None
    else:
        sid_killer = int(data["KILLER"]["STEAM_ID"])
        killer = (
            shinqlx.Plugin.player(sid_killer)
            if sid_killer
            else shinqlx.Plugin.player(  # It's a bot. Forced to use name as an identifier.
                data["KILLER"]["NAME"]
            )
        )

    shinqlx.EVENT_DISPATCHERS["death"].dispatch(victim, killer, data)
    if killer:
        shinqlx.EVENT_DISPATCHERS["kill"].dispatch(victim, killer, data)


@shinqlx.next_frame
def dispatch_team_switch_event(data):
    # No idea why they named it "KILLER" here, but whatever.
    steam_id = int(data["KILLER"]["STEAM_ID"])
    player = (
        shinqlx.Plugin.player(steam_id)
        if steam_id
        else shinqlx.Plugin.player(data["KILLER"]["NAME"])
    )
    if player is None:
        return
    old_team = data["KILLER"]["OLD_TEAM"].lower()
    new_team = data["KILLER"]["TEAM"].lower()
    if old_team != new_team:
        res = shinqlx.EVENT_DISPATCHERS["team_switch"].dispatch(
            player, old_team, new_team
        )
        if res is False:
            player.put(old_team)


class StatsListener(Thread):
    def __init__(self):
        super().__init__()

        self.done = False
        self._in_progress = False

        if not bool(int(shinqlx.get_cvar("zmq_stats_enable"))):
            self.done = True
            return

        host = shinqlx.get_cvar("zmq_stats_ip") or "127.0.0.1"
        port = shinqlx.get_cvar("zmq_stats_port") or shinqlx.get_cvar("net_port")
        self.address = f"tcp://{host}:{port}"
        self.password = shinqlx.get_cvar("zmq_stats_password")

    def keep_receiving(self):
        """Receives until 'self.done' is set to True."""
        self.start()

    def run(self):
        if self.done:
            return

        with zmq.Context().instance().socket(zmq.SUB) as socket:
            socket.setsockopt_string(zmq.PLAIN_USERNAME, "stats")
            socket.setsockopt_string(zmq.PLAIN_PASSWORD, self.password)
            socket.setsockopt_string(zmq.ZAP_DOMAIN, "stats")
            socket.connect(self.address)
            socket.subscribe("")

            poller = zmq.Poller()
            poller.register(socket, zmq.POLLIN)

            while True:  # Will throw an expcetion if no more data to get.
                pending_events = dict(poller.poll(timeout=250))
                for receiver in pending_events:
                    stats = json.loads(receiver.recv().decode(errors="replace"))
                    dispatch_stats_event(stats)

                    if stats["TYPE"] == "MATCH_STARTED":
                        self._in_progress = True
                        dispatch_game_start_event(stats["DATA"])
                    elif stats["TYPE"] == "ROUND_OVER":
                        dispatch_round_end_event(stats["DATA"])
                    elif stats["TYPE"] == "MATCH_REPORT":
                        if self._in_progress:
                            dispatch_game_end_event(stats["DATA"])
                        self._in_progress = False
                    elif stats["TYPE"] == "PLAYER_DEATH":
                        dispatch_player_death_event(stats["DATA"])
                    elif stats["TYPE"] == "PLAYER_SWITCHTEAM":
                        dispatch_team_switch_event(stats["DATA"])

    def stop(self):
        self.done = True
