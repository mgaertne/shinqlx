# Release notes

## 0.8.0 (unreleased)

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
  * Plugins that only worked around the crash (such as `rcon_cleanup_guard`) are no longer needed.
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
* **Optimization profile:** the release profile optimizes for speed (`opt-level = 3`) instead of size. A different
  level can be set with `CARGO_PROFILE_RELEASE_OPT_LEVEL`.
* **Failing `plugin_manager` commands** log their traceback instead of `NoneType`. `load_plugin`,
  `unload_plugin` and `reload_plugin` raise their errors without logging them, as in minqlx; whoever catches the error
  logs it (the start through `qlx_plugins` and the plugin manager). The error messages for a missing plugin, a missing class, a class that is no `Plugin` and an unknown plugin source
  in `qlx_plugins` name the plugin.

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
