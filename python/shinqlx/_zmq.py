"""Subscribes to the ZMQ stats protocol and calls the stats event dispatcher when
we get stats from it. It polls the ZMQ socket approx. every 0.25 seconds."""


import zmq
import shinqlx


class StatsListener:
    def __init__(self):
        self.done = False
        self._in_progress = False

        if not bool(int(shinqlx.get_cvar("zmq_stats_enable"))):
            self.done = True
            return

        host = shinqlx.get_cvar("zmq_stats_ip") or "127.0.0.1"
        port = shinqlx.get_cvar("zmq_stats_port") or shinqlx.get_cvar("net_port")
        self.address = f"tcp://{host}:{port}"
        self.password = shinqlx.get_cvar("zmq_stats_password")

    @shinqlx.delay(0.25)
    def keep_receiving(self):
        """Receives until 'self.done' is set to True."""
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
                    stats = receiver.recv.json()
                    shinqlx.EVENT_DISPATCHERS["stats"].dispatch(stats)

                    if stats["TYPE"] == "MATCH_STARTED":
                        self._in_progress = True
                        shinqlx.EVENT_DISPATCHERS["game_start"].dispatch(stats["DATA"])
                    elif stats["TYPE"] == "ROUND_OVER":
                        shinqlx.EVENT_DISPATCHERS["round_end"].dispatch(stats["DATA"])
                    elif stats["TYPE"] == "MATCH_REPORT":
                        # MATCH_REPORT event goes off with a map change and map_restart,
                        # but we really only want it for when the game actually ends.
                        # We use a variable instead of Game().state because by the
                        # time we get the event, the game is probably gone.
                        if self._in_progress:
                            shinqlx.EVENT_DISPATCHERS["game_end"].dispatch(stats["DATA"])
                        self._in_progress = False
                    elif stats["TYPE"] == "PLAYER_DEATH":
                        # Dead player.
                        sid = int(stats["DATA"]["VICTIM"]["STEAM_ID"])
                        player = (
                            shinqlx.Plugin.player(sid)
                            if sid
                            else shinqlx.Plugin.player(stats["DATA"]["VICTIM"]["NAME"])
                        )

                        # Killer player.
                        if not stats["DATA"]["KILLER"]:
                            player_killer = None
                        else:
                            sid_killer = int(stats["DATA"]["KILLER"]["STEAM_ID"])
                            if sid_killer:
                                player_killer = shinqlx.Plugin.player(sid_killer)
                            else:  # It's a bot. Forced to use name as an identifier.
                                player_killer = shinqlx.Plugin.player(
                                    stats["DATA"]["KILLER"]["NAME"]
                                )

                        shinqlx.EVENT_DISPATCHERS["death"].dispatch(
                            player, player_killer, stats["DATA"]
                        )
                        if player_killer:
                            shinqlx.EVENT_DISPATCHERS["kill"].dispatch(
                                player, player_killer, stats["DATA"]
                            )
                    elif stats["TYPE"] == "PLAYER_SWITCHTEAM":
                        # No idea why they named it "KILLER" here, but whatever.
                        player = shinqlx.Plugin.player(
                            int(stats["DATA"]["KILLER"]["STEAM_ID"])
                        )
                        if player is None:
                            continue
                        old_team = stats["DATA"]["KILLER"]["OLD_TEAM"].lower()
                        new_team = stats["DATA"]["KILLER"]["TEAM"].lower()
                        if old_team != new_team:
                            res = shinqlx.EVENT_DISPATCHERS["team_switch"].dispatch(
                                player, old_team, new_team
                            )
                            if res is False:
                                player.put(old_team)

    def stop(self):
        self.done = True
