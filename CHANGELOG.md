# Changelog

## Unreleased

Changes since 0.8.0.

### shinqlx

* **Scoreboard trim for `scores_ca`, `scores_tdm`, `scores_ctf`, `scores_ft`, `scores_ffa` and `scores_rr`:** the engine throws away a server command of 1023 bytes or more, and the game then
  sends a stub without players. With many players the trim cuts the scoreboard back in steps (values of spectators and
  of the ranks beyond `qlx_scoreboardFullPerTeam` set to zero, ranks beyond `qlx_scoreboardLightPerTeam` and spectators
  dropped, finally rows from the larger team down to `qlx_scoreboardMinPerTeam`), only as far as the whole command
  needs to fit `qlx_scoreboardBudget` (default 1000 bytes). The row of the player who gets the scoreboard always
  stays. `qlx_scoreboardTrim 0` switches it off, the console command `scoreboard` shows the counters. In `ffa` and `rr`, which have no teams, all rows count as one group.
* **Reliable guard against "a reliable command was cycled out":** the engine keeps 64 reliable commands per client,
  and at the end of a match with 20 or more players the burst of `castats`, scoreboard, print and chat commands can
  run the ring over. The guard holds back `castats`, `print`, `chat`, `tchat`, `cp` and `scores*` for a client whose
  ring is filling up (`qlx_reliableWatermark`, default 32), hands them out again over the following frames
  (`qlx_reliableBurst` per client, default 8, and `qlx_reliableGlobalBurst` for all clients together, default 64) and
  never drops one: a command that waited 40 frames goes out regardless, and with 128 commands waiting for a client
  further ones are sent straight through. Everything else, configstrings included, is sent immediately.
  `qlx_reliableGuard 0` switches it off, `qlx_reliableRing` lowers the assumed ring size for test rigs. The console
  command `reliable` shows the counters, `reliable reset` clears them. See the new page "Reliable commands" in the
  documentation. In the test rig with 48 bots and a ring of 16
  the guard cut the commands that arrive at a full ring from about 4000 to about 900 without losing any.
* **New cvar `qlx_noDebugEvents`** lists the events that do not write a `debug` line to `shinqlx.log`. It replaces the
  built-in list (`frame`, `set_configstring`, `stats`, `server_command`, `death`, `kill`, `command`, `console_print`,
  `damage`, plus the new `cvar_changed`, `item_touch` and `weapon_fired`); an empty value logs all events.
* **Frequent events only run when somebody listens:** `damage`, `weapon_fired`, `item_touch`, `cvar_changed`,
  `objective`, `kamikaze_use` and `kamikaze_explode` are no longer handed to Python while no plugin hooks them and
  the debug log has no line for them (see `qlx_noDebugEvents`). Hooking them later takes effect immediately.
* **New event `objective`, after minqlxtended:** `handle(player, kind, count)` goes off when a player scores in a
  team mode. `kind` is a member of the new string enumeration `Objective` (`capture`, `return`, `assist`,
  `base_defense`, `carrier_defense`, `frag_carrier`), `count` the new value of the counter. The server compares the
  counters once per frame, only while somebody listens. The event cannot be cancelled.
* **New event `weapon_fired`, after minqlxtended:** `handle(player, weapon)` goes off for every shot of a player,
  before the game fires the weapon. `weapon` is the number that `Player.weapon()` returns. The event cannot be
  cancelled and is not logged by default (see `qlx_noDebugEvents`). The game function `FireWeapon` is found by byte
  pattern.
* **New event `cvar_changed`, after minqlxtended:** `handle(cvar_name, old_value, new_value)` goes off after the value
  of a cvar changed, whoever changed it. It is found by a detour of `Cvar_Set2`. The event cannot be cancelled, and a
  handler that changes cvars does not trigger the event again.
* **New event `item_touch`, after minqlxtended:** `handle(player, entity_id, classname)` goes off before the game
  processes the touch of an item by a player (map items and dropped items). Stopping the event keeps the player from
  picking the item up. The game touches an item in every frame in which the player overlaps it, also when the player
  cannot pick it up, so the hooks should be cheap.
* **New in `Redis`, after minqlxtended:** `get_flags(players, flag, default=False)` reads a flag for many players with one
  `MGET`, `close_shared()` closes the connection that the plugins with the default configuration share, and
  `with self.db as db:` connects on entering and closes on leaving.
* **New opt-in feature `race-hooks` for race maps**, after the idea of the QLRace fork of
  minqlx, but found by byte pattern instead of a fixed offset. After `G_InitGame` the touch function of the entities
  `trigger_multiple` with the message `race_point` (with a `target` or `targetname`), `race_point` and
  `trigger_multiple` with the message `target_init` is replaced: touching a start point a second time no longer
  resets the run time (not for points without spawnflags and for `notr` points), the run time is kept in
  `flagruntime`, and `target_init` resets armor, health, weapons, powerups and holdable item unless its spawnflag
  keeps them (1 armor, 2 health, 4 weapons, 8 powerups, 16 holdable item, 32 removes the machine gun). Checked in the
  test rig on `campgrounds`: the function is found at the same place as in the fork and 14 entities are replaced; the
  behaviour with players on a race server is not tested yet.
* **`hiredis` is optional.** `pip install shinqlx` no longer installs it; `pip install "shinqlx[hiredis]"` or
  `pip install hiredis` does, and `redis` then uses its C parser as before. Without it `redis` falls back to its own
  Python parser, which only matters for replies of about ten thousand entries and more (a third faster with
  `hiredis` in a local measurement; a reply of ten fields takes the same time). The whole range of the commands that the wrapper and the
  plugin collections use (strings, hashes, sets, lists, sorted sets, `scan_iter`, pipelines, `WATCH`/`MULTI`) behaves
  the same with `redis` 5.0.8, 8.1.0 and 8.2.0, with and without `hiredis`.
* **`CvarFlags` and `DamageFlags` are real bit flags** (built on the `bitflags` crate). A value is a set of flags like
  `enum.IntFlag`: `CvarFlags.ARCHIVE | CvarFlags.LATCH` is a `CvarFlags`, `CvarFlags(33)` creates the same value from an
  integer, `flag in flags` tests a flag, iterating gives the members that are set, `^` and `~` work too, and `print`
  shows `ARCHIVE | LATCH`. They still convert to `int` (`int(flags)`, `__index__`) and compare equal to their integer, so
  `set_cvar(..., flags)` and the constants `CVAR_*` and `DAMAGE_*` keep working. **Changed:** the result of `|` and `&`
  is no longer an `int`, so `isinstance(flags, int)` is `False`.
* **The stats listener sets the ZAP domain only together with a stats password.** Without a password the subscriber
  no longer announces the domain `stats`; with the draft build of libzmq (`draft-api`, as used by `ql_rcon` and
  `ql_stats`) no stats arrived otherwise. The same applies to `ql_stats`.
* **`load_plugin` and `unload_plugin` log no error for a wrong request**, as in minqlx and in 0.7.0: they only raise when
  the plugin is not loaded (`PluginUnloadError`) or does not exist, so a caller that catches the exception no longer
  produces an `ERROR` line for it. Errors of the plugin code (import, constructor, missing class, no `Plugin`
  subclass) are still logged once with their traceback and raised. `reload_plugin` logs every failure once. The start
  and `!loadall` keep logging every plugin from `qlx_plugins` that cannot be loaded, also a missing one, followed by
  the line `Plugin 'x' from qlx_plugins could not be loaded, see the error above.`
* **Records of shinqlx itself no longer pass through the Python logger.** Messages such as `Loading plugin '…'...`,
  `Unloading plugin '…'...`, the messages of `late_init`, the warning for an unknown return value of a handler or
  command, the unknown game state and the missing ZMQ stats warning go straight to the log configuration. A handler that
  a plugin attaches to the logger `shinqlx` no longer receives them; records of plugins and libraries still reach the
  log through the handler of shinqlx, with their traceback. `shinqlx.log_exception(plugin)` writes under the logger of
  the plugin as before. Records carry the Python name of the function in `func`.
* **The name of a command in the warning for an unknown return value** is written as `["name"]` instead of `['name']`.
* **`Game.round_state` and the enumeration `RoundStates`** give the round state of the engine: `pregame`,
  `round_warmup`, `round_shuffle`, `round_begun`, `round_over` and `postgame`. The property is read-only and, like
  `Game.state`, equal to its member. It is read from the game module on every access, so it needs neither
  `zmq_stats_enable` nor an event; game types without rounds stay in `pregame`. Only in the `shinqlx` namespace.
* **The level structure of the game module** lacked the field `timeInitialKeySpawn`, so every field behind it was read
  eight bytes too early. This affected `mapIsTrainingMap`, which `allow_single_player` writes.
* **`Plugin.tell` calls `TellChannel.reply` directly**, so a Python override of `TellChannel.reply` no longer sees
  these tells. `Plugin.add_hook` and `Plugin.remove_hook` both
  call the method of the event dispatcher, so a subclass of `EventDispatcher` can override either of them.
* **A dispatched event is only described in the debug log if debug logging is enabled.** Before, `str()` of every
  argument ran for each event that is not on the list of silent events, also with debug logging off.
* **A crashed server that hangs in the cleanup of the engine now ends** after 10 seconds with the signal and a core
  dump, so that `supervisor` or `systemd` can restart it. The old handler for `SIGSEGV` never ran, because the engine
  replaces it at start; the new one is put in front of the handler of the engine before the first map is spawned and
  calls that handler after it has armed the timer.
* **Stubs:** `ReplyKwargs` is exported from the stubs of `shinqlx` and `minqlx`. `CvarFlags` and `DamageFlags` return
  flags from the operators and have `__init__`, `__xor__`, `__invert__`, `__contains__` and `__iter__`.

* **`SIGTERM` and `SIGINT` end the server.** The engine only sets two flags on these signals, and with
  `zmq_rcon_enable 1` the server kept running, so a supervisor had to wait for its kill timeout. Now the next frame
  executes `quit`, and if the server does not end within 30 seconds (for example while it loads a map), the process is
  ended with the signal. Without the ZMQ rcon nothing changes, the server ended on them before.

### plugin_migration

* The `decorators` rule warns where converting a registration can change the order of handlers with the same event or
  command and priority: shinqlx registers decorated methods after `__init__` in the alphabetical order of their names.
  The old warning for every class that kept a registration is gone, because it was too general.
* The `default-plugin-access` rule also warns about `Plugin.cointoss`, which shinqlx removed.

### ql_rcon

* When the server answers a cvar query (`"g_startingWeapons" is:"4100^7" default:...`), a line below names what the
  value stands for: the set bits of a bit field (`= Shotgun + Chaingun`) or the label of an enumeration (`= Clan Arena`),
  as far as the console guide lists them. Bits the guide does not know show up as `unknown bits 0x...`.
* The output of `cvarlist` is easier to read: the flag letters of the server are put at the end of their cvar line
  (`sv_hostname "..."  [SA]`), and a line below the list explains the letters; `help cvarflags` lists them in full. The
  flags are kept in the `[flags]` table of the server file and show up as `[SA]` in front of the description of a cvar
  in the completion menu, next to the notes (`[latched]`, `[read-only]`, `[cheat]`).
* Commands and cvars (also after `set`) match anywhere in the name: `gametype` offers `g_gametype`. The names that start
  with the typed text come first.
* The Quake color codes `^1` to `^7` use the RGB values of the 8 color palette in the console guide (`^1` is
  `0xFF0000`, `^6` is `0xFF00FF` and so on) instead of the 16 terminal colors, so the terminal theme no longer changes
  them; `^0` stays the black of the terminal theme to remain readable. The brighter shade for `print` lines is gone, because it only existed for the terminal colors. The terminal
  needs true color support, otherwise it picks the nearest color.
* **`ql_rcon` uses `reedline` instead of `linefeed`.** Tab opens a menu with one entry per line and its description or
  meaning next to it (the old list padded the text into the entry), the history shows the rest of the matching earlier
  command as a grey hint, and server output still appears above the input line. The history files keep their names.
  Ctrl-C with text in the line clears it, with an empty line it ends the session. `ql_stats` has no line editor, so
  there is nothing to switch there.
* **`ql_rcon` connects to a server without rcon password.** It always asked for the `PLAIN` security with the ZAP
  domain `rcon`, and libzmq rejects an empty password (`Invalid argument`). Without a password neither is set now, as in
  `ql_stats`; with a password nothing changes.
* **A command shows up once in the history.** The history file is cleaned of repeated commands when `ql_rcon` starts
  and after each sent line; the newest entry stays. Before, the older ones were meant to be deleted, but the file
  history of `reedline` cannot delete single entries, so repeated commands stayed.
* **`ql_rcon` shows a `^` that is no color code (such as `^8`) as text** and no longer drops the text up to the next
  color code. `ql_stats` uses the same decoding.
* **`ql_rcon` no longer keeps a CPU core busy while idle** (about 100 % before, about 0.1 % now). The ZMQ socket is
  polled every 25 ms, one server frame, and received lines are shown at once instead of after up to 250 ms. The input
  is read on its own blocking thread, and the tool runs on a single-threaded Tokio runtime without the `RwLock`s and
  `unsafe impl Send/Sync` around the sockets.
* **`ql_rcon` keeps a file per server**: `ql_rcon crimson-arena` reads `servers/crimson-arena.toml` from the config directory and
  connects with the `host`, `password`, `identity` and `color` of its table `[rcon]`; options on the command line win. `--save-server <name>`
  creates the file (readable for the owner only, `ql_rcon` warns about a readable file with a password). Without a
  password in the options and the file, `ql_rcon` asks for it on the terminal. What the server reports (names, maps,
  factories, latched and read-only cvars, usages, defaults and ranges) is kept in tables of the same file instead of
  the `completions-<host>.txt` and `learned-<host>.txt` files; comments in the file stay. See `ql_rcon/README.md`.
* **`ql_rcon` reads along when you type `cmdlist`, `cvarlist` or `dir maps .bsp`** and adds the names to the same
  `completions-<host>.txt` files as `--fetch-completions`, without hiding the output and without the flag.
* **`ql_rcon` notes in the completion list how a cvar can be set**: `[cheat]`, `[init only]`, `[latched]`,
  `[read-only]` (and `[write-protected]` where the guide has it), taken from the flags that the server lists with `cvarlist`. When the server answers a change
  with `<cvar> will be changed to <value> upon restarting.`, `ql_rcon` remembers the cvar as latched in
  `learned-<host>.txt`, which replaces a read-only note (for example for `bot_enable`); the same goes for
  `<cvar> is read only.` (for example for `sv_cheats`). The name of a cvar
  that was queried alone is completed too.
* **`ql_rcon` completes the first word with the commands and cvars of Quake Live** (164 commands from the console
  guide at https://www.euere.eu/ql/ and the 530 cvars that a vanilla dedicated server lists with `cvarlist`, with their
  flags), after the matching history entries; the case is ignored.
  Own names go into `commands.txt` (all servers) and `commands-<host>.txt` (for example `commands-10_0_0_1_27961.txt`)
  in the config directory (`~/.config/ql_zmq/` on Linux), one name per line; empty lines and lines starting with `#` are
  skipped. `ql_rcon --fetch-completions` asks the server once for `cmdlist` and `cvarlist` (without showing the output),
  and keeps the names in `completions-<host>.txt` of the same directory. The completion reads the files on every Tab, so
  new names and edits are used at once. After `set`, `seta`, `sets` and `setu` it completes cvar names, and after
  `set <cvar>` or after a cvar itself the default and the values from the usage of the console guide (923 cvars).
  `--fetch-completions` also asks for `dir maps .bsp`; the maps are kept in `completions-maps-<host>.txt` and completed
  after `map`, `devmap` and `callvote map`. Own maps go into `maps.txt` and `maps-<host>.txt`.
  `ql_rcon` also remembers what the server reports on its own, in `learned-<host>.txt`: the `usage:` line of a command
  (shown on Tab after the command when nothing else fits) and the default and the `minimum`/`maximum` of a cvar that
  was queried (offered as values). The list of factories that `map` without arguments prints is remembered too and
  completed after the map name of `map`, `devmap` and `callvote map`.
  The list of the completion tells what a value means (`g_gametype`: `4  Clan Arena`, 107 cvars with values or bits
  from the console guide, such as `g_voteFlags`) and, for up to 20 names, the first line of their description. Own
  meanings go into `values.txt` and `values-<host>.txt` as lines `cvar<TAB>value<TAB>meaning`; they win over the guide.
* The descriptions of the completion gain the server cvars `sv_tags`, `sv_mapPoolFile`, `sv_idleExit`, `sv_serverType`,
  `sv_master`, `g_accessFile`, `g_allTalk`, the flood protection, timeout and warmup cvars, `serverstartup`, the eight
  `zmq_rcon_*`/`zmq_stats_*` cvars and the commands `setenv`, `setenemycolor` and `setteamcolor`, worded after community server configurations. The
  per-weapon cvars `g_startingAmmo_*`, `weapon_gravity_*` and a few more are described from their names alone; a
  comment line in `help.txt` marks them.

### ql_rcon and ql_stats

* **One config directory and one format for both tools.** The server files and the completion files of
  `ql_rcon` and the server files of `ql_stats` live in `~/.config/ql_zmq/` on Linux; move the directory `~/.config/ql_rcon/`
  of earlier versions there by hand. A server file has the tables `[rcon]`
  (`host`, `port`, `password`, `identity`, `color`) and `[stats]` (`host`, `port`, `password`); without a `host` each takes
  the host of the other table with its own `port`. Move the values `host`, `password`, `identity` and `color` at the top
  of an older file into `[rcon]` by hand. `ql_zmq/servers.example.toml` is a sample with every key commented.
* **More settings in the server file.** `[stats]` takes `format`, `color`, `timestamp`, `types` and `exclude`, which
  are the options `--format`, `--color`, `--timestamp`, `--type` and `--exclude` of `ql_stats`; `[rcon]` takes
  `timestamp` and `fetch_completions`. Options on the command line win. A value of the wrong type or an unknown
  `format` or `color` stops the start with a message that names the key.
* **`--no-timestamp` and `--no-fetch-completions`** turn the switches off for one run when the server file turns them
  on; the last of `--timestamp` and `--no-timestamp` wins.
* **`ql_rcon --timestamp`** puts the time of receipt before every line of the output, as `ql_stats --timestamp` does
  before every message.
* **`--color` and `color` take the same values in `ql_rcon` and `ql_stats`:** `auto`, `always` and `never`, without
  regard to case. `ql_rcon` used the values of the `termcolor` crate (`always-ansi` as a fourth one, which was the same
  as `always`; the help and README wrongly named it `ansi`), `ql_stats` had its own three. `always-ansi` is not accepted
  any more, use `always`. `auto` now means the same in both: a terminal and no `NO_COLOR` in the environment, where
  `ql_rcon` looked only at the terminal before.

### ql_stats

* **`ql_stats` no longer keeps a CPU core busy while idle** (about 100 % before, about 0.1 % now). The ZMQ socket is
  polled every 25 ms, one server frame, the output waits for the next line instead of looking every 100 ms, and the
  tool runs on a single-threaded Tokio runtime without the `RwLock`s and `unsafe impl Send/Sync` around the sockets.
* **Output options for `ql_stats`:** `--format json|pretty|summary` (`summary` prints one readable line for the common
  event types such as `PLAYER_DEATH`, `PLAYER_CONNECT`, `PLAYER_DISCONNECT`, `ROUND_OVER` and `MATCH_REPORT`, other types stay json; `--pretty-print` still
  works), `--type A,B` and `--exclude A,B` filter by message type, `--color auto|always|never` colors the output
  (`auto` only on a terminal and without `NO_COLOR`), and `--timestamp` puts the time of receipt before each message.
  The keys of the json output keep the order of the server instead of being sorted. With color, the player names of
  `summary` appear in their colors of the game (`^1` to `^7`, decoded as in `ql_rcon`); without color the codes are removed.
* **`ql_stats <name>` reads the server file of `ql_rcon`** (`servers/<name>.toml`). The table `[stats]` holds `port` (the
  host of the `[rcon]` table with this port), or `host` as a whole URI, and `password`; options on the command line
  win. `--save-server <name>` writes the table, readable for the owner only, and a warning appears when a server file with
  a password can be read by other users. Without a password `ql_stats` does not ask, as the stats interface often has none.

### Internal

* The functions that walk over all entities (`dev_print_items`, `remove_dropped_items`, `force_weapon_respawn_time`,
  `destroy_kamikaze_timers`, `replace_items` with a classname, and the targetting lookup of an entity) stop at the
  number of entities that the level uses (`level.num_entities`, 204 on `campgrounds`) instead of all 1024 slots.
* The criterion benchmarks of the event handlers run the dispatchers that the hooks call (`client_command_dispatcher`,
  `server_command_dispatcher`, `set_configstring_dispatcher`, `console_print_dispatcher`, `damage_dispatcher`,
  `frame_dispatcher`) instead of the `handle_*` Python functions, so they measure the path of the server. The benchmark
  names changed accordingly (`event_dispatcher/`, `frame_dispatcher/`).
* The banner of the chat log, `essentials` (`!seen`, `!time`, `!random`) and the time stamps of bans and silences are computed in
  Rust (`chrono`, `rand`) instead of the Python modules `time`, `datetime` and `random`. The Cargo feature `clock` of
  `chrono` is enabled for the local time.
* The stats listener receives in a Rust thread named `shinqlx-stats` that sleeps until a message arrives, instead of a
  Python thread that polled every 125 ms; `StatsListener.stop()` wakes it. The private method
  `StatsListener._poll_zmq()` is gone. `arzmq` is built with its `futures` feature, which needs Unix, and `async-io`
  and `futures` are new dependencies.
* The deferred calls of the native plugins and the stats events are queued directly, without the Python decorators
  `delay` and `next_frame` and without helper modules written in Python.
* The chat log is written with a rolling `log4rs` file instead of Python's `logging`; the plugin manager calls the plugin
  loader directly instead of through the `shinqlx` module; a rolled chat log keeps `qlx_chatlogs` files, and
  `qlx_chatlogs 0` truncates the file at `qlx_chatlogsSize` instead of letting it grow.
* The built-in event names live in one Rust enum that the dispatchers and `Events` share.
* The console loop of `ql_rcon` works on a `Transport` and a `CompletionStore` trait instead of the ZMQ socket and the
  command line options. The watching of the ZMQ connection (link events, reconnect, messages) is shared with `ql_stats`
  in the new workspace crate `ql_zmq` and tested without sockets.
* The engine dispatchers for `client_command`, `server_command`, `set_configstring` and `console_print` get an `Outcome`
  from the handlers instead of a Python `bool` or `str` that they decoded again; the functions `handle_*` that Python
  sees return what they returned before. A dispatch with one handler takes about a third less time (213 → 141 ns in
  the `dispatch/` benchmark).
* Cvars of the native plugins are read as Rust values and created with `set_cvar_once` on a `&str`; a missing cvar
  raises the `TypeError` of `int(None)` or `float(None)` as before.
* The next-frame fallback of the event dispatchers queues the bound method instead of compiling Python code on every
  call. `EventDispatcher` reads its name and the list of silent events once.
* `Plugin.tell` and `Plugin.msg` share the parsing of `limit` and `delimiter`; `engine_api.client_id` calls `str()` at
  most once; the stats events pass JSON data to Python objects directly (`json_to_py`, tested against `json.loads`)
  instead of through a string; `set_tags` and `set_workshop_items` join the items without copying them.
* The accessors `Player.position`, `velocity`, `weapons`, `weapon`, `ammo`, `powerups` and `flight` return a
  `StateReading` (the current value or whether the update took effect) that converts to the same Python objects.
  `set_handicap`, `set_autohop`, `set_autoaction` and `set_predictitems` share one function, and the keyword arguments
  of the accessors are read by helpers instead of repeated blocks.
* The `create()` functions of the native plugins return `Bound<Plugin>`, and their handlers take `Command` and `dict`
  arguments instead of `PyAny`.

## 0.8.0 (2026-11-19)

Changes since 0.7.0.

### Changed behaviour

Check these when you upgrade.

* **Dead ZMQ RCON clients no longer crash or stop the server, and `console_print` no longer sees
  `zmq RCON client disconnected: ...`.** The new Cargo feature `zmq-rcon-fix` is part of the default features and does
  two things for bugs of the dedicated server that also occur with an unmodified `qzeroded`:
  * The engine erases dead ZMQ RCON clients in a loop and reports each deletion through `Com_Printf`, which sends the
    line to all RCON clients and enters the same loop again. With two or more dead clients at once, the outer loop
    continues with an entry that the inner loop has already freed, and the server crashes with a segmentation fault.
    The report is dropped before it reaches `Com_Printf` and the plugins.
  * The engine sends the console output to each RCON client without a time limit. A client that stops reading while
    its connection stays open (for example a tool that gives up on a long reply) makes the server wait for it without
    end: the server stops answering, even after the client has gone. The same happens with three or more clients that
    vanish at once or with a client that reconnects and registers about twenty times a second (observed in a local test;
    without this feature the server also crashed in some runs). The feature sets a send timeout on the RCON socket, so
    the server keeps running. The cvar `qlx_zmqRconSendTimeout` sets the timeout in milliseconds (default `25`, about
    one frame, per client that does not take its output; `0` makes the sends non-blocking, a negative value keeps the
    unlimited wait). It is read at every map load. In the local test, clients that read their replies received the same
    number of lines with `25`, `0` and the unlimited wait. A client that stops reading holds about 2.5 MB of output
    with default buffers; later lines are discarded for this client, which stays registered (see `DDOS-PROTECTION.md`).
  * Plugins that only worked around the crash are no longer needed.
  * To keep the report and the unlimited wait, build without the feature:
    `--no-default-features --features alloc,steam-query-limit`.
* **Python 3.9 or newer is required.** Python 3.8 is dropped, because pyo3 requires at least 3.9.
* **`pyzmq` is no longer a dependency.** The stats listener is implemented in Rust, nothing in shinqlx imports `zmq`.
  A plugin that does `import zmq` itself has to bring `pyzmq` along (`pip install pyzmq`).
* **The feature `patches` is split.** It now enables `clientkick-fix`, `fps-patch`, `zmq-rcon-fix` and
  `steam-query-limit`. Select single patches with their own features.
* **`DEFAULT` in `qlx_plugins` is expanded at its position** instead of appending the default plugins at the end.
* **Command arguments are split at any whitespace**, as minqlx does. The unused `cointoss` is removed from the plugin
  base class and from the essentials plugin.
* **Team switch arguments are matched case-insensitively**, and `g_gameState` is required in the serverinfo configstring.
* **Empty cvars return empty collections**, and the items of list, set and tuple cvars are stripped.
* **Percent signs in displayed server commands** are replaced by the full-width percent sign.
* **`reload_plugin` accepts the plugin source prefix** and refuses to load a plugin from another source than the loaded
  one. `load_plugin` understands the prefixes `p!` and `r!` for Python and Rust plugins.
* **Redis database:** `setx` is renamed to `setex`, and `lrem` is keyword-only again with a default `count`, so
  positional calls of it are rejected. `lrem` and `zincrby` are fixed.
* **Rules queries** are limited on their own, and `qlx_steamQueryDropRules` drops them.
* **The level `WARN` is printed as `WARNING`**, as the Python logging does.
* **Logging** goes through one log4rs configuration for Rust and Python (`shinqlx_log.yml`, now in YAML). The file
  appender is asynchronous and holds records back until the log file is known, so logging does not block the frame
  (100 console lines in one frame cost 7 ms before and 0.4 ms after in a local test). Every record of Rust carries the
  logging function as `func` (`[LEVEL @ target.func]`). `shinqlx_log.yml` is not part of the wheel, only of the source
  distribution and the documentation; it is looked up in the working directory first and then in `fs_homepath`. The chat
  log stays a separate plugin (`log`). See the logging page of the documentation. Handlers on the logger of `shinqlx`
  and uses of `main_logger` are reported by the migration tool.
* **No `target-cpu=native` in the repository.** `.cargo/config.toml` no longer sets it, so builds from the source
  distribution and the CI binaries run on any CPU of their architecture. Whoever wants it sets `RUSTFLAGS` or the flag
  in the own `~/.cargo/config.toml` (see the README).
* **Optimization profile:** the release profile optimizes for speed (`opt-level = 3`) instead of size. A different
  level can be set with `CARGO_PROFILE_RELEASE_OPT_LEVEL`.
* **Failing plugin loads** are logged once, with their traceback instead of `NoneType`: `load_plugin`, `unload_plugin`
  and `reload_plugin` log the error and raise it, as in minqlx, whoever calls them. The `plugin_manager` commands only
  report the failure to the channel. A plugin from `qlx_plugins` that could not be loaded at the start (or by
  `!loadall`) is named in one more line after its error: `Plugin 'x' from qlx_plugins could not be loaded, see the
  error above.` The error messages for a missing plugin, a missing class, a class that is no
  `Plugin` and an unknown plugin source in `qlx_plugins` name the plugin.

### New features

* **Built-in Rust plugins:** `workshop`, `permission`, `ban`, `plugin_manager`, `names`, `clan`, `silence`, `motd`, `log`
  and `essentials` (hooks, information, moderation, team and game control commands). They are loaded with the prefix
  `r!`. `is_banned`, `leave_status`, `warn_player` and `is_silenced` of the ban and silence plugins can be used by
  other plugins.
* **Decorators:** `hook` and `command` register hooks and commands when the plugin is loaded, their signatures are
  checked against the arguments of the event. `game` and typed arguments for `command` are new. `next_frame`, `delay`
  and `thread` are implemented in Rust. The new `later(seconds=…)` and `later(frames=…)` defer a call by seconds or by
  game frames; `later(frames=1)` equals `next_frame` and `later(seconds=x)` equals `delay(x)`. It is available in the
  `shinqlx` namespace only.
* **Convenience enums** in the `shinqlx` namespace: `Weapon`, `Permissions`, `ConfigString`, `Holdable`, `GameStates`
  and the string enumeration `Events`. The enums compare only with themselves and with integers. The `Cvar`
  descriptor declares typed cvars of a plugin.
* **A2S protection:** the Steam query limiter (`steam-query-limit`, default) and documentation for it; further
  documentation for driver-level filtering, RCON and stats ports, and a security policy (`SECURITY.md`).
* **New patches:** `fps-patch` lets `qagame` register `sv_fps` with 125 instead of the read-only 40. The clientkick,
  query limiter and sv_fps patches live in a `fixes` module.
* **`plugin_migration`:** a tool that migrates a folder of minqlx plugins to shinqlx with tree-sitter based rules (cvar
  keywords, `slay_with_mod`, default plugin access, event names, enum constants, permission levels, decorators, enum
  imports, removed team chat channel classes). Binaries are built and uploaded by the build job. The documentation has a
  page *Migration from minqlx* with the names and signatures that differ.
* **Stub packages** `shinqlx-stubs` and `minqlx-stubs` are built and published with the release. The stubs are checked
  against the runtime with `stubtest`.
* **API additions:** `Plugin.find_by_name_or_id`, `Game.type`, `Player.valid`, typed `reply` keywords of `tell` and `msg`.
  `set_cvar_once` and `set_cvar_limit_once` take the parameter names of minqlx.
* **Databases:** Valkey, KeyDB and Dragonfly are documented as servers that work in place of Redis.

### Fixes

* The built-in `workshop` plugin adds its items to the map that is running when it is loaded (not only from the next
  map change on) and does not add an item twice.
* A&D round status handling without a round cvar in `handle_set_configstring`.
* `replace_items` checks the entity id against `MAX_GENTITIES` like minqlx instead of the item count.
* The pattern search stops where the pattern no longer fits and looks only into readable and executable memory.
* Typos in the configstring constants; `CS_PLAYERINFO` is added and the item configstring is set at `CS_ITEMS`.
* Players are built from `PlayerInfo` in one place, so `all_players` and the stats listener fall back to the userinfo
  name.
* The signature check of hooks on `vote_started` and `vote_ended` and the restored `PRI_NORMAL` and `return_type=str`
  in the displayed signatures.
* Exposed classes that hold Python references support the garbage collector.
* The undefined behavior found by miri in the client id and function pointer handling.
* Detours use `detour` 0.10 instead of `retour`; failures of the callvote patch are reported and only the patched bytes
  are protected.

### Performance

* Hot paths no longer compile regular expressions or Python code, interned names are used for the Python calls, the
  GIL is kept for short read-only lookups, and `clean_text` works in a single byte pass.
* Dispatchers hand the text of client, server, configstring and console print events on without copying, and event
  arguments are not built when nobody listens.
* Rayon is no longer used for tiny collections; `ql_rcon` has a small parser instead of the regex dependency.

### Documentation and build

* The documentation has a migration page, pages for the built-in plugins, the convenience enums, the logging and plugin
  examples for beginners, and its interface descriptions are corrected against the stubs.
* The CI runs clippy and `cargo check` on the whole workspace, the miri tests with stacked borrows and strict
  provenance, and the tests of the matrix keep running when one version fails.
* `arzmq` is updated to 0.7, and the lints are shared across all crates.
* The release page of GitHub carries `ql_rcon`, `ql_stats` and `plugin_migration` for Linux, macOS and Windows, the
  HTML documentation as a zip file and a `SHA256SUMS` file. The assets are created by the CI when a tag is pushed.
