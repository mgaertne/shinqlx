import queue
import sched
import re

import shinqlx

# ====================================================================
#                        REGULAR EXPRESSIONS
# ====================================================================

_re_say = re.compile(r"^say +\"?(?P<msg>.+)\"?$", flags=re.IGNORECASE)
_re_say_team = re.compile(r"^say_team +\"?(?P<msg>.+)\"?$", flags=re.IGNORECASE)
_re_callvote = re.compile(
    r"^(?:cv|callvote) +(?P<cmd>[^ ]+)(?: \"?(?P<args>.+?)\"?)?$", flags=re.IGNORECASE
)
_re_vote = re.compile(r"^vote +(?P<arg>.)", flags=re.IGNORECASE)
_re_team = re.compile(r"^team +(?P<arg>.)", flags=re.IGNORECASE)
_re_userinfo = re.compile(r"^userinfo \"(?P<vars>.+)\"$")


# ====================================================================
#                         LOW-LEVEL HANDLERS
#        These are all called by the C code, not within Python.
# ====================================================================
def handle_client_command(client_id, cmd):
    """Client commands are commands such as "say", "say_team", "scores",
    "disconnect" and so on. This function parses those and passes it
    on to the event dispatcher.

    :param: client_id: The client identifier.
    :type: client_id: int
    :param: cmd: The command being run by the client.
    :type: cmd: str

    """
    # noinspection PyBroadException
    try:
        # Dispatch the "client_command" event before further processing.
        player = shinqlx.Player(client_id)
        retval = shinqlx.EVENT_DISPATCHERS["client_command"].dispatch(player, cmd)
        if retval is False:
            return False
        if isinstance(retval, str):
            # Allow plugins to modify the command before passing it on.
            cmd = retval

        res = _re_say.match(cmd)
        if res:
            msg = res.group("msg").replace('"', "")
            channel = shinqlx.CHAT_CHANNEL
            if (
                shinqlx.EVENT_DISPATCHERS["chat"].dispatch(player, msg, channel)
                is False
            ):
                return False
            return cmd

        res = _re_say_team.match(cmd)
        if res:
            msg = res.group("msg").replace('"', "")
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
            return cmd

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
    except:  # noqa: E722
        shinqlx.log_exception()
        return True


# Executing tasks right before a frame, by the main thread, will often be desirable to avoid
# weird behavior if you were to use threading. This list will act as a task queue.
# Tasks can be added by simply adding the @shinqlx.next_frame decorator to functions.
frame_tasks = sched.scheduler()
next_frame_tasks = queue.Queue()  # type: ignore


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
        try:
            frame_tasks.run(blocking=False)
            break
        except:  # noqa: E722
            shinqlx.log_exception()
            continue
    # noinspection PyBroadException
    try:
        shinqlx.EVENT_DISPATCHERS["frame"].dispatch()
    except:  # noqa: E722
        shinqlx.log_exception()
        return True

    while not next_frame_tasks.empty():
        func, args, kwargs = next_frame_tasks.get_nowait()
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
        # noinspection PyBroadException
        try:
            shinqlx.EVENT_DISPATCHERS["map"].dispatch(
                shinqlx.get_cvar("mapname"), shinqlx.get_cvar("g_factory")
            )
        except:  # noqa: E722
            shinqlx.log_exception()
            return True

    # noinspection PyBroadException
    try:
        shinqlx.EVENT_DISPATCHERS["new_game"].dispatch()
    except:  # noqa: E722
        shinqlx.log_exception()
        return True


def handle_set_configstring(index, value):
    """Called whenever the server tries to set a configstring. Can return
    False to stop the event.

    """
    global _ad_round_number

    # noinspection PyBroadException
    try:
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
                    try:
                        round_number = int(cvars["round"]) * 2 + 1 + int(cvars["turn"])
                        _ad_round_number = round_number
                    except KeyError:
                        round_number = _ad_round_number
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
    except:  # noqa: E722
        shinqlx.log_exception()
        return True


def handle_console_print(text):
    """Called whenever the server prints something to the console and when rcon is used."""
    if not text:
        return

    # noinspection PyBroadException
    try:
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
    except:  # noqa: E722
        shinqlx.log_exception()
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

    return PrintRedirector(channel)


def register_handlers():
    shinqlx.register_handler("client_command", handle_client_command)
    shinqlx.register_handler("new_game", handle_new_game)
    shinqlx.register_handler("set_configstring", handle_set_configstring)
    shinqlx.register_handler("console_print", handle_console_print)
