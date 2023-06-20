# minqlx - Extends Quake Live's dedicated server with extra functionality and scripting.
# Copyright (C) 2015 Mino <mino@minomino.org>

# This file is part of minqlx.

# minqlx is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.

# minqlx is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.

# You should have received a copy of the GNU General Public License
# along with minqlx. If not, see <http://www.gnu.org/licenses/>.

"""Subscribes to the ZMQ stats protocol and calls the stats event dispatcher when
we get stats from it. It polls the ZMQ socket approx. every 0.25 seconds."""

import json

# noinspection PyPackageRequirements
import zmq
import minqlx


class StatsListener:
    def __init__(self):
        self.done = False
        if not bool(int(minqlx.get_cvar("zmq_stats_enable"))):
            self.done = True
            return

        stats = minqlx.get_cvar("zmq_stats_ip")
        port = minqlx.get_cvar("zmq_stats_port")
        if not port:
            port = minqlx.get_cvar("net_port")
        host = stats if stats else "127.0.0.1"
        self.address = f"tcp://{host}:{port}"
        self.password = minqlx.get_cvar("zmq_stats_password")

        # Initialize socket, connect, and subscribe.
        self.context = zmq.Context()
        self.socket = self.context.socket(zmq.SUB)
        if self.password:
            self.socket.plain_username = b"stats"
            self.socket.plain_password = self.password.encode()
        self.socket.zap_domain = b"stats"
        self.socket.connect(self.address)
        self.socket.setsockopt_string(zmq.SUBSCRIBE, "")

        self.done = False
        self._in_progress = False

    @minqlx.delay(0.25)
    def keep_receiving(self):
        """Receives until 'self.done' is set to True. Works by scheduling this
        to be called every 0.25 seconds. If we get an exception, we try
        to reconnect and continue.

        """
        # noinspection PyBroadException
        try:
            if self.done:
                return
            while True:  # Will throw an expcetion if no more data to get.
                stats = json.loads(
                    self.socket.recv(zmq.NOBLOCK).decode(errors="ignore")
                )
                minqlx.EVENT_DISPATCHERS["stats"].dispatch(stats)

                if stats["TYPE"] == "MATCH_STARTED":
                    self._in_progress = True
                    minqlx.EVENT_DISPATCHERS["game_start"].dispatch(stats["DATA"])
                elif stats["TYPE"] == "ROUND_OVER":
                    minqlx.EVENT_DISPATCHERS["round_end"].dispatch(stats["DATA"])
                elif stats["TYPE"] == "MATCH_REPORT":
                    # MATCH_REPORT event goes off with a map change and map_restart,
                    # but we really only want it for when the game actually ends.
                    # We use a variable instead of Game().state because by the
                    # time we get the event, the game is probably gone.
                    if self._in_progress:
                        minqlx.EVENT_DISPATCHERS["game_end"].dispatch(stats["DATA"])
                    self._in_progress = False
                elif stats["TYPE"] == "PLAYER_DEATH":
                    # Dead player.
                    sid = int(stats["DATA"]["VICTIM"]["STEAM_ID"])
                    player = (
                        minqlx.Plugin.player(sid)
                        if sid
                        else minqlx.Plugin.player(stats["DATA"]["VICTIM"]["NAME"])
                    )

                    # Killer player.
                    if not stats["DATA"]["KILLER"]:
                        player_killer = None
                    else:
                        sid_killer = int(stats["DATA"]["KILLER"]["STEAM_ID"])
                        if sid_killer:
                            player_killer = minqlx.Plugin.player(sid_killer)
                        else:  # It's a bot. Forced to use name as an identifier.
                            player_killer = minqlx.Plugin.player(
                                stats["DATA"]["KILLER"]["NAME"]
                            )

                    minqlx.EVENT_DISPATCHERS["death"].dispatch(
                        player, player_killer, stats["DATA"]
                    )
                    if player_killer:
                        minqlx.EVENT_DISPATCHERS["kill"].dispatch(
                            player, player_killer, stats["DATA"]
                        )
                elif stats["TYPE"] == "PLAYER_SWITCHTEAM":
                    # No idea why they named it "KILLER" here, but whatever.
                    player = minqlx.Plugin.player(
                        int(stats["DATA"]["KILLER"]["STEAM_ID"])
                    )
                    if player is None:
                        continue
                    old_team = stats["DATA"]["KILLER"]["OLD_TEAM"].lower()
                    new_team = stats["DATA"]["KILLER"]["TEAM"].lower()
                    if old_team != new_team:
                        res = minqlx.EVENT_DISPATCHERS["team_switch"].dispatch(
                            player, old_team, new_team
                        )
                        if res is False:
                            player.put(old_team)

        except zmq.error.Again:
            pass
        except Exception:
            minqlx.log_exception()
            # Reconnect, just in case. GC will clean up for us.
            self.__init__()  # type: ignore

        self.keep_receiving()

    def stop(self):
        self.done = True
