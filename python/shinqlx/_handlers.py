import shinqlx


# ====================================================================
#                         LOW-LEVEL HANDLERS
#        These are all called by the C code, not within Python.
# ====================================================================
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
    shinqlx.register_handler("console_print", handle_console_print)
