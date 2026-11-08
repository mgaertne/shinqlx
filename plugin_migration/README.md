# plugin_migration

Migrates a folder of [minqlx](https://github.com/MinoMino/minqlx) plugins to shinqlx.

```console
plugin_migration [-n] [-j N] <input> <output>
```

- `<input>` is the folder with the minqlx plugins, `<output>` receives the migrated plugins. The output folder must not
  exist yet or must be empty, and it must not be inside the input folder.
- Python files that need no change and all other files are copied as they are. A file that is changed keeps its
  formatting and comments, only the affected places are rewritten.
- `-n`/`--dry-run` only reports what would change. `-j`/`--jobs` sets the number of worker threads (default: number of
  CPUs).
- The output lists the migrated files, the places that need a manual look as warnings, and a summary.

The migrated plugins run on shinqlx only. The tool does not touch strings and comments, so database keys like
`"minqlx:players:{}"` stay as they are and keep working with existing data.

## Rules

The rules run in this order, each on the result of the previous one:

| Rule | Does |
|---|---|
| `redis-keywords` | `db.setex(key, time, value)`, `db.lrem(key, count, value)` and `db.zincrby(key, value, amount)` take their arguments by keyword in shinqlx. For `zincrby` the amount is recognised by being a number or a negated value, because redis-py 2.x and 3 differ in the order. |
| `slay-with-mod` | `mod=` of `slay_with_mod` becomes `means_of_death=` (`mean_of_death=` for the module function). |
| `default-plugin-access` | Warns about `plugins["essentials"].something` that the Rust default plugins do not provide, and about `next_frame_tasks`. |
| `event-names` | The event name in `add_hook`/`remove_hook` becomes `Events.X`. Unknown names stay strings. |
| `enum-constants` | `RET_*`, `PRI_*`, `CVAR_*`, `TEAM_*`, `CS_*`, `MOD_*`, `PRIV_*` and `DAMAGE_*` become members of `ReturnCodes`, `Priorities`, `CvarFlags`, `Teams`, `ConnectionStates`, `MeansOfDeath`, `Privileges` and `DamageFlags`, but only where a member is accepted: in `return`, in `==`/`!=`/`in` comparisons and in the arguments that take the enum. |
| `permission-levels` | A permission level 0 to 5 in `add_command` becomes `Permissions.PLAYER` to `Permissions.GOD`. |
| `decorators` | `self.add_hook(...)` and `self.add_command(...)` directly in `__init__` become `@hook`/`@command` on the handler, if all arguments are constants and the handler is a method of the same class. Other registrations are kept, with a warning that names the reason. |
| `module-rename` | `minqlx` becomes `shinqlx` in imports and attribute accesses. If a file rebinds the name `minqlx`, it is left alone. |

The names of the enums and constants are read from `python/shinqlx/_shinqlx.pyi` when the tool is built, so they follow
the stub. The members that the Rust default plugins provide are checked against their sources by a test.

## Checked against

The result was checked on the plugins of [MinoMino/minqlx-plugins](https://github.com/MinoMino/minqlx-plugins) and the
repositories of the [Useful Plugins](https://github.com/MinoMino/minqlx/wiki/Useful-Plugins) list: all files stay valid
Python, and every class registers the same hooks and commands with the same arguments as before.
