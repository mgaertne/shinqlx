# ql_rcon

Remote console for the ZMQ rcon interface of Quake Live servers. It sends the lines you type, prints the replies with
the Quake color codes rendered and completes commands, cvars, map names and values with Tab.

```
ql_rcon --host tcp://192.0.2.1:27961 --password secret
ql_rcon crimson-arena
```

## Options

| Option | Meaning |
|---|---|
| `<server>` | Name of a server file, see below. |
| `--host <uri>` | ZMQ URI, `tcp://127.0.0.1:27961` by default; `host:port` is taken as `tcp://host:port`. |
| `--password <text>` | The ZMQ rcon password. Without one in the options and the server file, `ql_rcon` asks for it on the terminal; the input is not echoed and an empty answer means no password. |
| `--identity <text>` | Socket identity, a random UUID by default. |
| `--color <auto\|always\|never>` | Color output, `auto` by default: only on a terminal and without `NO_COLOR`. |
| `--fetch-completions`, `--no-fetch-completions` | Asks the server for its commands, cvars and maps at the start and keeps them for the completion; the second form overrides `fetch_completions = true` of the server file. |
| `--timestamp`, `--no-timestamp` | Puts the time of receipt (`HH:MM:SS`) before every line of the output, or not; the second form overrides `timestamp = true` of the server file. |
| `--save-server <name>` | Saves host, password and identity as the server file `<name>` and connects. |

## Server files

`ql_rcon crimson-arena` reads `servers/crimson-arena.toml` from the config directory (`~/.config/ql_zmq/` on Linux, found by the
`directories` crate, shared with `ql_stats`) and connects with what is in it. Options on the command line win over the
file, the file wins over the defaults. Create it with `--save-server crimson-arena --host … --password …` or by hand:

```toml
[rcon]
host = "tcp://192.0.2.1:28960"
password = "secret"
identity = "my-laptop"
color = "auto"

[stats]
host = "tcp://192.0.2.1:27961"
password = "stats-secret"
```

[`ql_zmq/servers.example.toml`](../ql_zmq/servers.example.toml) is a sample with every key commented.

Every key is optional. A `password = ""` means that the server has no password and `ql_rcon` does not ask for one.

`ql_stats crimson-arena` reads the table `[stats]`; the two tables take the same keys, because the ports and passwords of
the interfaces usually differ while the machine is the same. `host` names the whole URI. Instead you may give only
`port`, which takes the host of the other table with that port (`[stats] port = 27961` next to `[rcon] host`).
`identity`, `color`, `timestamp` and `fetch_completions` belong to `[rcon]`; `format`, `color`, `timestamp`, `types`
and `exclude` belong to `[stats]` and set the options of the same name of `ql_stats`. A value of the wrong type or an
unknown `format` or `color` stops the start with a message that names the key. The switches `timestamp` and
`fetch_completions` can be turned off for one run with `--no-timestamp` and `--no-fetch-completions`; the last of
`--timestamp` and `--no-timestamp` wins. `ql_stats crimson-arena --save-server crimson-arena --host … --password …`
writes `[stats]`.

Earlier versions kept `host`, `password`, `identity` and `color` at the top of the file and the files in
`~/.config/ql_rcon/`; move the directory to `~/.config/ql_zmq/` and the values into the table `[rcon]` by hand.

**The password is stored as plain text, so keep the file private.** `--save-server` creates it readable for the owner
only (`chmod 600`), and `ql_rcon` warns at the start if a server file with a password can be read by other users. When
you create or copy the file yourself, restrict it with `chmod 600 ~/.config/ql_zmq/servers/crimson-arena.toml`. Leave the
password out of the file if the machine is shared; `ql_rcon` then asks for it at every start.

### What the server reports

When a session uses a server file, what the server reports is kept in the same file, in tables that `ql_rcon` updates
and you may edit; comments and the order of your own keys stay as they are:

```toml
[completions]
names = ["addbot", "bot_enable", "sv_maxclients"]   # commands and cvars, from cmdlist and cvarlist
maps = ["aerowalk", "campgrounds"]                    # from "dir maps .bsp"
factories = ["ca", "ctf", "ffa"]                      # from the map command
latched = ["bot_enable"]                              # "<cvar> will be changed to <value> upon restarting."
read_only = ["sv_cheats"]                             # "<cvar> is read only."

[flags]
sv_hostname = "SA"                                    # flag letters of cvarlist, noted next to the name in the menu

[usages]
kick = "kick <player name>"

[cvars.sv_maxclients]                                 # from the answer to a cvar query
default = "8"
minimum = ""
maximum = ""
```

The lists grow when you type `cmdlist`, `cvarlist` or `dir maps .bsp` yourself, or when you start with
`--fetch-completions`. Without a server file the same data goes into `completions-<host>.txt` and
`learned-<host>.txt` next to it (the flags only go into a server file).

When you type `cvarlist` yourself, the flag letters of the server are put at the end of their cvar line,
`sv_hostname "A New Server"  [SA]`, and a line below the list explains them. `help cvarflags` shows the meaning of each
letter; `ql_rcon` answers it itself and does not send it to the server.

## Own completions

`commands.txt` (all servers) and `commands-<host>.txt` in the config directory hold your own command and cvar names,
one per line; empty lines and lines starting with `#` are skipped. The same works for `maps.txt` and `values.txt`
(`cvar<TAB>value<TAB>meaning`).

## History

The input history is kept per server in `~/.ql_zmq_rcon-<host>.history`.
