from typing import ClassVar, Dict

import shinqlx


class Plugin:
    """The base plugin class.

    Every plugin must inherit this or a subclass of this. It does not support any database
    by itself, but it has a *database* static variable that must be a subclass of the
    abstract class :class:`shinqlx.database.AbstractDatabase`. This abstract class requires
    a few methods that deal with permissions. This will make sure that simple plugins that
    only care about permissions can work on any database. Abstraction beyond that is hard,
    so any use of the database past that point will be uncharted territory, meaning the
    plugin will likely be database-specific unless you abstract it yourself.

    Permissions for commands can be overriden in the config. If you have a plugin called
    ``my_plugin`` with a command ``my_command``, you could override its permission
    requirement by adding ``perm_my_command: 3`` under a ``[my_plugin]`` header.
    This allows users to set custom permissions without having to edit the scripts.

    .. warning::
        I/O is the bane of single-threaded applications. You do **not** want blocking operations
        in code called by commands or events. That could make players lag. Helper decorators
        like :func:`shinqlx.thread` can be useful.

    """

    # Static dictionary of plugins currently loaded for the purpose of inter-plugin communication.
    _loaded_plugins: ClassVar[Dict[str, "shinqlx.Plugin"]] = {}
    # The database driver class the plugin should use.
    database = None

    def __init__(self):
        self._hooks = []
        self._commands = []
        self._db_instance = None

    def __str__(self):
        return self.name

    @property
    def db(self):
        """The database instance."""
        if not self.database:
            raise RuntimeError(f"Plugin '{self.name}' does not have a database driver.")
        if not hasattr(self, "_db_instance") or self._db_instance is None:
            self._db_instance = self.database(self)

        return self._db_instance

    @property
    def name(self):
        """The name of the plugin."""
        return self.__class__.__name__

    @property
    def plugins(self):
        """A dictionary containing plugin names as keys and plugin instances
        as values of all currently loaded plugins.

        """
        return self._loaded_plugins.copy()

    @property
    def hooks(self):
        """A list of all the hooks this plugin has."""
        if not hasattr(self, "_hooks"):
            self._hooks = []
        return self._hooks.copy()

    @property
    def commands(self):
        """A list of all the commands this plugin has registered."""
        if not hasattr(self, "_commands"):
            self._commands = []
        return self._commands.copy()

    @property
    def game(self):
        """A Game instance."""
        try:
            return shinqlx.Game()
        except shinqlx.NonexistentGameError:
            return None

    @property
    def logger(self):
        """An instance of :class:`logging.Logger`, but initialized for this plugin."""
        return shinqlx.get_logger(self)

    # TODO: Document plugin methods.
    def add_hook(self, event, handler, priority=shinqlx.PRI_NORMAL):
        if not hasattr(self, "_hooks"):
            self._hooks = []

        self._hooks.append((event, handler, priority))
        shinqlx.EVENT_DISPATCHERS[event].add_hook(self.name, handler, priority)

    def remove_hook(self, event, handler, priority=shinqlx.PRI_NORMAL):
        if not hasattr(self, "_hooks"):
            self._hooks = []
            return

        shinqlx.EVENT_DISPATCHERS[event].remove_hook(self.name, handler, priority)
        self._hooks.remove((event, handler, priority))

    def add_command(
        self,
        name,
        handler,
        permission=0,
        channels=None,
        exclude_channels=(),
        priority=shinqlx.PRI_NORMAL,
        client_cmd_pass=False,
        client_cmd_perm=5,
        prefix=True,
        usage="",
    ):
        if not hasattr(self, "_commands"):
            self._commands = []

        cmd = shinqlx.Command(
            self,
            name,
            handler,
            permission,
            channels,
            exclude_channels,
            client_cmd_pass,
            client_cmd_perm,
            prefix,
            usage,
        )
        self._commands.append(cmd)
        shinqlx.COMMANDS.add_command(cmd, priority)

    def remove_command(self, name, handler):
        if not hasattr(self, "_commands"):
            self._commands = []
            return

        for cmd in self._commands:
            if cmd.name == name and cmd.handler == handler:
                shinqlx.COMMANDS.remove_command(cmd)

    @classmethod
    def get_cvar(cls, name, return_type=str):
        """Gets the value of a cvar as a string.

        :param: name: The name of the cvar.
        :type: name: str
        :param: return_type: The type the cvar should be returned in.
            Supported types: str, int, float, bool, list, tuple

        """
        res = shinqlx.get_cvar(name)
        if return_type == str:
            return res
        if return_type == int:
            return int(res) if res else None
        if return_type == float:
            return float(res) if res else None
        if return_type == bool:
            return bool(int(res)) if res else False
        if return_type == list:
            return [s.strip() for s in res.split(",")] if res else []
        if return_type == set:
            return {s.strip() for s in res.split(",")} if res else set()
        if return_type == tuple:
            return ([s.strip() for s in res.split(",")]) if res else ()

        raise ValueError(f"Invalid return type: {return_type}")

    @classmethod
    def set_cvar(cls, name, value, flags=0):
        """Sets a cvar. If the cvar exists, it will be set as if set from the console,
        otherwise create it.

        :param: name: The name of the cvar.
        :type: name: str
        :param: value: The value of the cvar.
        :type: value: Anything with an __str__ method.
        :param: flags: The flags to set if, and only if, the cvar does not exist and has to be created.
        :type: flags: int
        :returns: True if a new cvar was created, False if an existing cvar was set.
        :rtype: bool

        """
        if cls.get_cvar(name) is None:
            shinqlx.set_cvar(name, value, flags)
            return True

        shinqlx.console_command(f'{name} "{value}"')
        return False

    @classmethod
    def set_cvar_limit(cls, name, value, minimum, maximum, flags=0):
        """Sets a cvar with upper and lower limits. If the cvar exists, it will be set
        as if set from the console, otherwise create it.

        :param: name: The name of the cvar.
        :type: name: str
        :param: value: The value of the cvar.
        :type: value: int, float
        :param: minimum: The minimum value of the cvar.
        :type: value: int, float
        :param: maximum: The maximum value of the cvar.
        :type: value: int, float
        :param: flags: The flags to set if, and only if, the cvar does not exist and has to be created.
        :type: flags: int
        :returns: True if a new cvar was created, False if an existing cvar was set.
        :rtype: bool

        """
        if cls.get_cvar(name) is None:
            shinqlx.set_cvar_limit(name, value, minimum, maximum, flags)
            return True

        shinqlx.set_cvar_limit(name, value, minimum, maximum, flags)
        return False

    @classmethod
    def set_cvar_once(cls, name, value, flags=0):
        """Sets a cvar. If the cvar exists, do nothing.

        :param: name: The name of the cvar.
        :type: name: str
        :param: value: The value of the cvar.
        :type: value: Anything with an __str__ method.
        :param: flags: The flags to set if, and only if, the cvar does not exist and has to be created.
        :type: flags: int
        :returns: True if a new cvar was created, False if an existing cvar was set.
        :rtype: bool

        """
        return shinqlx.set_cvar_once(name, value, flags)

    @classmethod
    def set_cvar_limit_once(cls, name, value, minimum, maximum, flags=0):
        """Sets a cvar with upper and lower limits. If the cvar exists, not do anything.

        :param: name: The name of the cvar.
        :type: name: str
        :param: value: The value of the cvar.
        :type: value: int, float
        :param: minimum: The minimum value of the cvar.
        :type: value: int, float
        :param: maximum: The maximum value of the cvar.
        :type: value: int, float
        :param: flags: The flags to set if, and only if, the cvar does not exist and has to be created.
        :type: flags: int
        :returns: True if a new cvar was created, False if an existing cvar was set.
        :rtype: bool

        """
        return shinqlx.set_cvar_limit_once(name, value, minimum, maximum, flags)

    @classmethod
    def players(cls):
        """Get a list of all the players on the server."""
        return shinqlx.Player.all_players()

    @classmethod
    def player(cls, name, player_list=None):
        """Get a Player instance from the name, client ID,
        or Steam ID. Assumes [0, 64) to be a client ID
        and [64, inf) to be a Steam ID.

        """
        # In case 'name' isn't a string.
        if isinstance(name, shinqlx.Player):
            return name
        if isinstance(name, int) and 0 <= name < 64:
            return shinqlx.Player(name)

        players = player_list if player_list else cls.players()

        if isinstance(name, int) and name >= 64:
            for p in players:
                if p.steam_id == name:
                    return p
        else:
            cid = cls.client_id(name, players)
            if cid is not None:
                for p in players:
                    if p.id == cid:
                        return p

        return None

    @classmethod
    def msg(cls, msg, chat_channel="chat", **kwargs):
        """Send a message to the chat, or any other channel."""
        if isinstance(chat_channel, shinqlx.AbstractChannel):
            chat_channel.reply(msg, **kwargs)
        elif chat_channel == shinqlx.CHAT_CHANNEL:
            shinqlx.CHAT_CHANNEL.reply(msg, **kwargs)
        elif chat_channel == shinqlx.RED_TEAM_CHAT_CHANNEL:
            shinqlx.RED_TEAM_CHAT_CHANNEL.reply(msg, **kwargs)
        elif chat_channel == shinqlx.BLUE_TEAM_CHAT_CHANNEL:
            shinqlx.BLUE_TEAM_CHAT_CHANNEL.reply(msg, **kwargs)
        elif chat_channel == shinqlx.CONSOLE_CHANNEL:
            shinqlx.CONSOLE_CHANNEL.reply(msg, **kwargs)
        else:
            raise ValueError("Invalid channel.")

    @classmethod
    def console(cls, text):
        """Prints text in the console."""
        shinqlx.console_print(str(text))

    @classmethod
    def clean_text(cls, text):
        """Removes color tags from text."""
        return shinqlx.re_color_tag.sub("", text)

    @classmethod
    def colored_name(cls, name, player_list=None):
        """Get the colored name of a decolored name."""
        if isinstance(name, shinqlx.Player):
            return name.name

        players = player_list if player_list else cls.players()

        clean = cls.clean_text(name).lower()
        for p in players:
            if p.clean_name.lower() == clean:
                return p.name

        return None

    @classmethod
    def client_id(cls, name, player_list=None):
        """Get a player's client id from the name, client ID,
        Player instance, or Steam ID. Assumes [0, 64) to be
        a client ID and [64, inf) to be a Steam ID.

        """
        if isinstance(name, int) and 0 <= name < 64:
            return name
        if isinstance(name, shinqlx.Player):
            return name.id

        players = player_list if player_list else cls.players()

        # Check Steam ID first, then name.
        if isinstance(name, int) and name >= 64:
            for p in players:
                if p.steam_id == name:
                    return p.id

        if isinstance(name, str):
            clean = cls.clean_text(name).lower()
            for p in players:
                if p.clean_name.lower() == clean:
                    return p.id

        return None

    @classmethod
    def find_player(cls, name, player_list=None):
        """Find a player based on part of a players name.

        :param: name: A part of someone's name.
        :type: name: str
        :returns: A list of players that had that in their names.

        """
        players = player_list if player_list else cls.players()

        if not name:
            return players

        res = []
        for p in players:
            if cls.clean_text(name.lower()) in p.clean_name.lower():
                res.append(p)

        return res

    @classmethod
    def teams(cls, player_list=None):
        """Get a dictionary with the teams as keys and players as values."""
        players = player_list if player_list else cls.players()

        res = {team_value: [] for team_value in shinqlx.TEAMS.values()}  # type: ignore

        for p in players:
            res[p.team].append(p)

        return res

    @classmethod
    def center_print(cls, msg, recipient=None):
        client_id = None
        if recipient:
            client_id = cls.client_id(recipient)

        shinqlx.send_server_command(client_id, f'cp "{msg}"')

    @classmethod
    def tell(cls, msg, recipient, **kwargs):
        """Send a tell (private message) to someone.

        :param: msg: The message to be sent.
        :type: msg: str
        :param: recipient: The player that should receive the message.
        :type: recipient: str/int/shinqlx.Player
        :returns: bool -- True if succeeded, False otherwise.
        :raises: ValueError
        """
        shinqlx.TellChannel(recipient).reply(msg, **kwargs)

    @classmethod
    def is_vote_active(cls):
        return bool(shinqlx.get_configstring(9))

    @classmethod
    def current_vote_count(cls):
        yes = shinqlx.get_configstring(10)
        no = shinqlx.get_configstring(11)
        if yes and no:
            return int(yes), int(no)

        return None

    @classmethod
    def callvote(cls, vote, display, time=30):
        if cls.is_vote_active():
            return False

        # Tell vote_started's dispatcher that it's a vote called by the server.
        # noinspection PyUnresolvedReferences
        shinqlx.EVENT_DISPATCHERS["vote_started"].caller(None)
        shinqlx.callvote(vote, display, time)
        return True

    @classmethod
    def force_vote(cls, pass_it):
        if pass_it is True or pass_it is False:
            return shinqlx.force_vote(pass_it)

        raise ValueError("pass_it must be either True or False.")

    @classmethod
    def teamsize(cls, size):
        shinqlx.Game().teamsize = size

    @classmethod
    def kick(cls, player, reason=""):
        cid = cls.client_id(player)
        if cid is None:
            raise ValueError("Invalid player.")

        if not reason:
            shinqlx.kick(cid, None)
        else:
            shinqlx.kick(cid, reason)

    @classmethod
    def shuffle(cls):
        shinqlx.Game().shuffle()

    @classmethod
    def cointoss(cls):
        # TODO: Call cointoss directly and implement cointoss().
        pass

    @classmethod
    def change_map(cls, new_map, factory=None):
        if not factory:
            shinqlx.Game().map = new_map
        else:
            shinqlx.console_command(f"map {new_map} {factory}")

    @classmethod
    def switch(cls, player, other_player):
        p1 = cls.player(player)
        p2 = cls.player(other_player)

        if not p1:
            raise ValueError("The first player is invalid.")
        if not p2:
            raise ValueError("The second player is invalid.")

        t1 = p1.team
        t2 = p2.team

        if t1 == t2:
            raise ValueError("Both players are on the same team.")

        cls.put(p1, t2)
        cls.put(p2, t1)

    @classmethod
    def play_sound(cls, sound_path, player=None):
        if not sound_path or "music/" in sound_path.lower():
            return False

        if player:
            shinqlx.send_server_command(player.id, f"playSound {sound_path}")
        else:
            shinqlx.send_server_command(None, f"playSound {sound_path}")
        return True

    @classmethod
    def play_music(cls, music_path, player=None):
        if not music_path or "sound/" in music_path.lower():
            return False

        if player:
            shinqlx.send_server_command(player.id, f"playMusic {music_path}")
        else:
            shinqlx.send_server_command(None, f"playMusic {music_path}")
        return True

    @classmethod
    def stop_sound(cls, player=None):
        shinqlx.send_server_command(player.id if player else None, "clearSounds")

    @classmethod
    def stop_music(cls, player=None):
        shinqlx.send_server_command(player.id if player else None, "stopMusic")

    @classmethod
    def slap(cls, player, damage=0):
        cid = cls.client_id(player)
        if cid is None:
            raise ValueError("Invalid player.")

        shinqlx.console_command(f"slap {cid} {damage}")

    @classmethod
    def slay(cls, player):
        cid = cls.client_id(player)
        if cid is None:
            raise ValueError("Invalid player.")

        shinqlx.console_command(f"slay {cid}")

    # ====================================================================
    #                         ADMIN COMMANDS
    # ====================================================================

    @classmethod
    def timeout(cls):
        shinqlx.Game.timeout()

    @classmethod
    def timein(cls):
        shinqlx.Game.timein()

    @classmethod
    def allready(cls):
        shinqlx.Game.allready()

    @classmethod
    def pause(cls):
        shinqlx.Game.pause()

    @classmethod
    def unpause(cls):
        shinqlx.Game.unpause()

    @classmethod
    def lock(cls, team=None):
        shinqlx.Game.lock(team)

    @classmethod
    def unlock(cls, team=None):
        shinqlx.Game.unlock(team)

    @classmethod
    def put(cls, player, team):
        shinqlx.Game.put(player, team)

    @classmethod
    def mute(cls, player):
        shinqlx.Game.mute(player)

    @classmethod
    def unmute(cls, player):
        shinqlx.Game.unmute(player)

    @classmethod
    def tempban(cls, player):
        # TODO: Add an optional reason to tempban.
        shinqlx.Game.tempban(player)

    @classmethod
    def ban(cls, player):
        shinqlx.Game.ban(player)

    @classmethod
    def unban(cls, player):
        shinqlx.Game.unban(player)

    @classmethod
    def opsay(cls, msg):
        shinqlx.Game.opsay(msg)

    @classmethod
    def addadmin(cls, player):
        shinqlx.Game.addadmin(player)

    @classmethod
    def addmod(cls, player):
        shinqlx.Game.addmod(player)

    @classmethod
    def demote(cls, player):
        shinqlx.Game.demote(player)

    @classmethod
    def abort(cls):
        shinqlx.Game.abort()

    @classmethod
    def addscore(cls, player, score):
        shinqlx.Game.addscore(player, score)

    @classmethod
    def addteamscore(cls, team, score):
        shinqlx.Game.addteamscore(team, score)

    @classmethod
    def setmatchtime(cls, time):
        shinqlx.Game.setmatchtime(time)
