import shinqlx

# ====================================================================
#                         LOW-LEVEL HANDLERS
#        These are all called by the C code, not within Python.
# ====================================================================
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
    shinqlx.register_handler("new_game", handle_new_game)
    shinqlx.register_handler("set_configstring", handle_set_configstring)
    shinqlx.register_handler("console_print", handle_console_print)
