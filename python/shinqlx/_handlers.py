from contextlib import ExitStack, suppress
import queue
import sched
import re

import shinqlx

# ====================================================================
#                        REGULAR EXPRESSIONS
# ====================================================================

_re_say = re.compile(r"^say +(?P<quote>\"?)(?P<msg>.+)(?P=quote)$", flags=re.IGNORECASE)
_re_say_team = re.compile(
    r"^say_team +(?P<quote>\"?)(?P<msg>.+)(?P=quote)$", flags=re.IGNORECASE
)
_re_callvote = re.compile(
    r"^(?:cv|callvote) +(?P<cmd>[^ ]+)(?: \"?(?P<args>.+?)\"?)?$", flags=re.IGNORECASE
)
_re_vote = re.compile(r"^vote +(?P<arg>.)", flags=re.IGNORECASE)
_re_team = re.compile(r"^team +(?P<arg>.)", flags=re.IGNORECASE)
_re_vote_ended = re.compile(r"^print \"Vote (?P<result>passed|failed).\n\"$")
_re_userinfo = re.compile(r"^userinfo \"(?P<vars>.+)\"$")


# ====================================================================
#                         LOW-LEVEL HANDLERS
#        These are all called by the C code, not within Python.
# ====================================================================
def handle_rcon(cmd):
    """Console commands that are to be processed as regular pyshinqlx
    commands as if the owner executes it. This allows the owner to
    interact with the Python part of shinqlx without having to connect.

    """
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        shinqlx.COMMANDS.handle_input(
            shinqlx.RconDummyPlayer(), cmd, shinqlx.CONSOLE_CHANNEL
        )

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_client_command(client_id, cmd):
    """Client commands are commands such as "say", "say_team", "scores",
    "disconnect" and so on. This function parses those and passes it
    on to the event dispatcher.

    :param: client_id: The client identifier.
    :type: client_id: int
    :param: cmd: The command being run by the client.
    :type: cmd: str

    """
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        player_info = (
            shinqlx.player_info(client_id)
            if client_id is not None and client_id >= 0
            else None
        )

        player = (
            shinqlx.Player(client_id, player_info) if player_info is not None else None
        )

        if player is None:
            return True

        # Dispatch the "client_command" event before further processing.
        retval = shinqlx.EVENT_DISPATCHERS["client_command"].dispatch(player, cmd)
        if retval is False:
            return False
        if isinstance(retval, str):
            # Allow plugins to modify the command before passing it on.
            cmd = retval

        res = _re_say.match(cmd)
        if res:
            msg = res.group("msg").replace('"', "'")
            channel = shinqlx.CHAT_CHANNEL
            if (
                shinqlx.EVENT_DISPATCHERS["chat"].dispatch(player, msg, channel)
                is False
            ):
                return False
            return f'say "{msg}"'

        res = _re_say_team.match(cmd)
        if res:
            msg = res.group("msg").replace('"', "'")
            if (
                player.team == "free"
            ):  # I haven't tried this, but I don't think it's even possible.
                channel = shinqlx.FREE_CHAT_CHANNEL
            elif player.team == "red":
                channel = shinqlx.RED_TEAM_CHAT_CHANNEL
            elif player.team == "blue":
                channel = shinqlx.BLUE_TEAM_CHAT_CHANNEL
            else:
                channel = shinqlx.SPECTATOR_CHAT_CHANNEL
            if (
                shinqlx.EVENT_DISPATCHERS["chat"].dispatch(player, msg, channel)
                is False
            ):
                return False
            return f'say_team "{msg}"'

        res = _re_callvote.match(cmd)
        if res and not shinqlx.Plugin.is_vote_active():
            vote = res.group("cmd")
            args = res.group("args") if res.group("args") else ""
            # Set the caller for vote_started in case the vote goes through.
            # noinspection PyUnresolvedReferences
            shinqlx.EVENT_DISPATCHERS["vote_started"].caller(player)
            if (
                shinqlx.EVENT_DISPATCHERS["vote_called"].dispatch(player, vote, args)
                is False
            ):
                return False
            return cmd

        res = _re_vote.match(cmd)
        if res and shinqlx.Plugin.is_vote_active():
            arg = res.group("arg").lower()
            if (
                arg in ["y", "1"]
                and shinqlx.EVENT_DISPATCHERS["vote"].dispatch(player, True) is False
            ):
                return False
            if (
                arg in ["n", "2"]
                and shinqlx.EVENT_DISPATCHERS["vote"].dispatch(player, False) is False
            ):
                return False
            return cmd

        res = _re_team.match(cmd)
        if res:
            arg = res.group("arg").lower()
            target_team = ""
            if arg == player.team[0]:
                # Don't trigger if player is joining the same team.
                return cmd
            if arg == "f":
                target_team = "free"
            elif arg == "r":
                target_team = "red"
            elif arg == "b":
                target_team = "blue"
            elif arg == "s":
                target_team = "spectator"
            elif arg == "a":
                target_team = "any"

            if (
                target_team
                and shinqlx.EVENT_DISPATCHERS["team_switch_attempt"].dispatch(
                    player, player.team, target_team
                )
                is False
            ):
                return False
            return cmd

        res = _re_userinfo.match(cmd)
        if res:
            new_info = shinqlx.parse_variables(res.group("vars"), ordered=True)
            old_info = player.cvars
            changed = {
                key: value
                for key, value in new_info.items()
                if key not in old_info or old_info[key] != value
            }

            if changed:
                ret = shinqlx.EVENT_DISPATCHERS["userinfo"].dispatch(player, changed)
                if ret is False:
                    return False
                if isinstance(ret, dict):
                    for key in ret:
                        new_info[key] = ret[key]
                    formatted_key_values = "".join(
                        [f"\\{key}\\{value}" for key, value in new_info.items()]
                    )
                    cmd = f'userinfo "{formatted_key_values}"'

        return cmd

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_server_command(client_id, cmd):
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        # Dispatch the "server_command" event before further processing.
        player_info = (
            shinqlx.player_info(client_id)
            if client_id is not None and client_id >= 0
            else None
        )

        player = (
            shinqlx.Player(client_id, player_info) if player_info is not None else None
        )

        if player is None:
            return True

        retval = shinqlx.EVENT_DISPATCHERS["server_command"].dispatch(player, cmd)
        if retval is False:
            return False
        if isinstance(retval, str):
            cmd = retval

        res = _re_vote_ended.match(cmd)
        if res:
            shinqlx.EVENT_DISPATCHERS["vote_ended"].dispatch(
                res.group("result") == "passed"
            )

        return cmd

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


# Executing tasks right before a frame, by the main thread, will often be desirable to avoid
# weird behavior if you were to use threading. This list will act as a task queue.
# Tasks can be added by simply adding the @shinqlx.next_frame decorator to functions.
frame_tasks = sched.scheduler()
next_frame_tasks = queue.SimpleQueue()  # type: ignore


def handle_frame():
    """This will be called every frame. To allow threads to call stuff from the
    main thread, tasks can be scheduled using the :func:`shinqlx.next_frame` decorator
    and have it be executed here.

    """

    while True:
        # This will run all tasks that are currently scheduled.
        # If one of the tasks throw an exception, it'll log it
        # and continue execution of the next tasks if any.
        # noinspection PyBroadException
        catcher = shinqlx.ExceptionCatcher()
        with ExitStack() as stack:
            stack.enter_context(suppress(Exception))
            stack.enter_context(shinqlx.ExceptionLogging())
            stack.enter_context(catcher)

            frame_tasks.run(blocking=False)
            break

        # noinspection PyUnreachableCode
        if catcher.is_exception_caught():
            continue

    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        shinqlx.EVENT_DISPATCHERS["frame"].dispatch()

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True

    while not next_frame_tasks.empty():
        func, args, kwargs = next_frame_tasks.get(block=False)
        frame_tasks.enter(0, 1, func, args, kwargs)


_zmq_warning_issued = False
_first_game = True
_ad_round_number = 0


def handle_new_game(is_restart):
    # This is called early in the launch process, so it's a good place to initialize
    # shinqlx stuff that needs QLDS to be initialized.
    global _first_game
    if _first_game:
        shinqlx.late_init()
        _first_game = False

        # A good place to warn the owner if ZMQ stats are disabled.
        global _zmq_warning_issued
        stats_enabled_cvar = shinqlx.get_cvar("zmq_stats_enable")
        if (
            stats_enabled_cvar is None or not bool(int(stats_enabled_cvar))
        ) and not _zmq_warning_issued:
            logger = shinqlx.get_logger()
            logger.warning(
                "Some events will not work because ZMQ stats is not enabled. "
                'Launch the server with "zmq_stats_enable 1"'
            )
            _zmq_warning_issued = True

    shinqlx.set_map_subtitles()

    if not is_restart:
        catcher = shinqlx.ExceptionCatcher()
        with ExitStack() as stack:
            stack.enter_context(suppress(Exception))
            stack.enter_context(shinqlx.ExceptionLogging())
            stack.enter_context(catcher)

            shinqlx.EVENT_DISPATCHERS["map"].dispatch(
                shinqlx.get_cvar("mapname"), shinqlx.get_cvar("g_factory")
            )

        # noinspection PyUnreachableCode
        if catcher.is_exception_caught():
            return True

    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        shinqlx.EVENT_DISPATCHERS["new_game"].dispatch()

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_set_configstring(index, value):
    """Called whenever the server tries to set a configstring. Can return
    False to stop the event.

    """
    global _ad_round_number

    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        res = shinqlx.EVENT_DISPATCHERS["set_configstring"].dispatch(index, value)
        if res is False:
            return False
        if isinstance(res, str):
            value = res

        # VOTES
        if index == 9 and value:
            cmd = value.split()
            vote = cmd[0] if cmd else ""
            args = " ".join(cmd[1:]) if len(cmd) > 1 else ""
            shinqlx.EVENT_DISPATCHERS["vote_started"].dispatch(vote, args)
            return
        # GAME STATE CHANGES
        if index == 0:
            old_cs = shinqlx.parse_variables(shinqlx.get_configstring(index))
            if not old_cs:
                return

            new_cs = shinqlx.parse_variables(value)
            old_state = old_cs["g_gameState"]
            new_state = new_cs["g_gameState"]
            if old_state != new_state:
                if old_state == "PRE_GAME" and new_state == "IN_PROGRESS":
                    pass
                elif old_state == "PRE_GAME" and new_state == "COUNT_DOWN":
                    _ad_round_number = 1
                    shinqlx.EVENT_DISPATCHERS["game_countdown"].dispatch()
                elif (old_state == "COUNT_DOWN" and new_state == "IN_PROGRESS") or (
                    new_state == "PRE_GAME"
                    and old_state
                    in [
                        "IN_PROGRESS",
                        "COUNT_DOWN",
                    ]
                ):
                    pass
                else:
                    logger = shinqlx.get_logger()
                    logger.warning(f"UNKNOWN GAME STATES: {old_state} - {new_state}")
        # ROUND COUNTDOWN AND START
        elif index == 661:
            cvars = shinqlx.parse_variables(value)
            if cvars:
                if "turn" in cvars:
                    # it is A&D
                    if int(cvars["state"]) == 0:
                        return
                    # round cvar appears only on round countdown
                    # and first round is 0, not 1
                    round_number = (
                        int(cvars["round"]) * 2 + 1 + int(cvars["turn"])
                        if "round" in cvars
                        else _ad_round_number
                    )
                    _ad_round_number = round_number
                else:
                    # it is CA
                    round_number = int(cvars["round"])

                if round_number and "time" in cvars:
                    shinqlx.EVENT_DISPATCHERS["round_countdown"].dispatch(round_number)
                    return
                if round_number:
                    shinqlx.EVENT_DISPATCHERS["round_start"].dispatch(round_number)
                    return

        return res

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_player_connect(client_id, _is_bot):
    """This will be called whenever a player tries to connect. If the dispatcher
    returns False, it will not allow the player to connect and instead show them
    a message explaining why. The default message is "You are banned from this
    server.", but it can be set with :func:`shinqlx.set_ban_message`.

    :param: client_id: The client identifier.
    :type: client_id: int
    :param: _is_bot: Whether or not the player is a bot.
    :type: _is_bot: bool

    """
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        player_info = (
            shinqlx.player_info(client_id)
            if client_id is not None and client_id >= 0
            else None
        )

        player = (
            shinqlx.Player(client_id, player_info) if player_info is not None else None
        )

        if player is None:
            return True

        return shinqlx.EVENT_DISPATCHERS["player_connect"].dispatch(player)

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_player_loaded(client_id):
    """This will be called whenever a player has connected and finished loading,
    meaning it'll go off a bit later than the usual "X connected" messages.
    This will not trigger on bots.

    :param: client_id: The client identifier.
    :type: client_id: int

    """
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        player_info = (
            shinqlx.player_info(client_id)
            if client_id is not None and client_id >= 0
            else None
        )

        player = (
            shinqlx.Player(client_id, player_info) if player_info is not None else None
        )

        if player is None:
            return True

        return shinqlx.EVENT_DISPATCHERS["player_loaded"].dispatch(player)

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_player_disconnect(client_id, reason):
    """This will be called whenever a player disconnects.

    :param: client_id: The client identifier.
    :type: client_id: int
    :param: reason: The reason for the disconnect
    :type: reason: str

    """
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        player_info = (
            shinqlx.player_info(client_id)
            if client_id is not None and client_id >= 0
            else None
        )

        player = (
            shinqlx.Player(client_id, player_info) if player_info is not None else None
        )

        if player is None:
            return True

        return shinqlx.EVENT_DISPATCHERS["player_disconnect"].dispatch(player, reason)

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_player_spawn(client_id):
    """Called when a player spawns. Note that a spectator going in free spectate mode
    makes the client spawn, so you'll want to check for that if you only want "actual"
    spawns.

    """
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        player_info = (
            shinqlx.player_info(client_id)
            if client_id is not None and client_id >= 0
            else None
        )

        player = (
            shinqlx.Player(client_id, player_info) if player_info is not None else None
        )

        if player is None:
            return True

        return shinqlx.EVENT_DISPATCHERS["player_spawn"].dispatch(player)

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_kamikaze_use(client_id):
    """This will be called whenever player uses kamikaze item.

    :param: client_id: The client identifier.
    :type: client_id: int

    """
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        player_info = (
            shinqlx.player_info(client_id)
            if client_id is not None and client_id >= 0
            else None
        )

        player = (
            shinqlx.Player(client_id, player_info) if player_info is not None else None
        )

        if player is None:
            return True

        return shinqlx.EVENT_DISPATCHERS["kamikaze_use"].dispatch(player)

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_kamikaze_explode(client_id, is_used_on_demand):
    """This will be called whenever kamikaze explodes.

    :param: client_id: The client identifier.
    :type: client_id: int
    :param: is_used_on_demand: Non-zero if kamikaze is used on demand.
    :type: is_used_on_demand: int


    """
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        player_info = (
            shinqlx.player_info(client_id)
            if client_id is not None and client_id >= 0
            else None
        )

        player = (
            shinqlx.Player(client_id, player_info) if player_info is not None else None
        )

        if player is None:
            return True

        return shinqlx.EVENT_DISPATCHERS["kamikaze_explode"].dispatch(
            player, bool(is_used_on_demand)
        )

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_damage(target_id, attacker_id, damage, dflags, mod):
    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        target_player_info = (
            shinqlx.player_info(target_id) if target_id in range(0, 64) else None
        )
        target_player = (
            shinqlx.Player(target_id, target_player_info)
            if target_player_info is not None
            else target_id
        )
        attacker_player_info = (
            shinqlx.player_info(attacker_id) if attacker_id in range(0, 64) else None
        )
        attacker_player = (
            shinqlx.Player(attacker_id, attacker_player_info)
            if attacker_player_info is not None
            else attacker_id
        )

        shinqlx.EVENT_DISPATCHERS["damage"].dispatch(
            target_player, attacker_player, damage, dflags, mod
        )

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


def handle_console_print(text):
    """Called whenever the server prints something to the console and when rcon is used."""
    if not text:
        return

    catcher = shinqlx.ExceptionCatcher()
    with ExitStack() as stack:
        stack.enter_context(suppress(Exception))
        stack.enter_context(shinqlx.ExceptionLogging())
        stack.enter_context(catcher)

        # Log console output. Removes the need to have stdout logs in addition to shinqlx.log.
        shinqlx.get_logger().debug(text.rstrip("\n"))

        res = shinqlx.EVENT_DISPATCHERS["console_print"].dispatch(text)
        if res is False:
            return False

        if _print_redirection:
            global _print_buffer
            _print_buffer += text

        if isinstance(res, str):
            return res

        return text

    # noinspection PyUnreachableCode
    if catcher.is_exception_caught():
        return True


_print_redirection = None
_print_buffer = ""


def redirect_print(channel):
    """Redirects print output to a channel. Useful for commands that execute console commands
    and want to redirect the output to the channel instead of letting it go to the console.

    To use it, use the return value with the "with" statement.

    .. code-block:: python
        def cmd_echo(self, player, msg, channel):
            with shinqlx.redirect_print(channel):
                shinqlx.console_command("echo {}".format(" ".join(msg)))

    """

    return PrintRedirector(channel)


class PrintRedirector:
    def __init__(self, _channel):
        if not isinstance(_channel, shinqlx.AbstractChannel):
            raise ValueError(
                "The redirection channel must be an instance of shinqlx.AbstractChannel."
            )

        self.channel = _channel

    def __enter__(self):
        global _print_redirection
        _print_redirection = self.channel

    def __exit__(self, exc_type, exc_val, exc_tb):
        global _print_redirection
        self.flush()
        _print_redirection = None

    def flush(self):
        global _print_buffer
        self.channel.reply(_print_buffer)
        _print_buffer = ""


def register_handlers():
    shinqlx.register_handler("rcon", handle_rcon)
    shinqlx.register_handler("client_command", handle_client_command)
    shinqlx.register_handler("server_command", handle_server_command)
    shinqlx.register_handler("new_game", handle_new_game)
    shinqlx.register_handler("set_configstring", handle_set_configstring)
    shinqlx.register_handler("console_print", handle_console_print)
