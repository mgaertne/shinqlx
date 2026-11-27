# ql_demo

Converts the match recordings of shinqlx (`.dm_sqx`) to Quake Live demos (`.dm_91`).

A recording holds the view of every player of a match. `ql_demo` runs offline on a copy of the file; the server is not
involved.

## Usage

```console
ql_demo info <recording>
ql_demo export [-c CLIENT]... [-p NAME]... [-d FOLDER] [--no-private] <recording>...
```

`info` shows the map, the length, why the recording ended and who played when:

```console
$ ql_demo info 20260101_200000_campgrounds.dm_sqx
map: campgrounds
factory: ca
sv_fps: 40
shinqlx: 0.8.1
started: 1767297600
length: 31:12 (74880 frames)
end: the match ended
client    from   until  name
     0    0:00   31:12  Alice
     1    0:08   12:40  Bob
     1   13:05   31:12  Carol
```

`export` writes one demo for every line of that list in a single pass over the recording.

- `-c`/`--client` picks client numbers, `-p`/`--player` picks players by a part of their name, ignoring case and color
  codes. Both may be repeated; a player matching any of them is exported. Without either, everyone is exported.
- `-d`/`--directory` is the folder for the demos, created if needed. Without it they are written next to the recording.
- A demo is named `<recording>_client<number>_<name>.dm_91`; a further stay of the same client number gets `_2`, `_3`
  and so on. Existing files are never overwritten.
- Several recordings can be given at once, for example with `*.dm_sqx` in a shell that expands it. If one
  fails, the others are still converted and the exit code is not zero.
- A recording that was cut off, for example by a crash, is converted up to its last complete record.

A demo runs from the moment its player joins until the player leaves. It contains what that player got from the server,
including team chat and tells, which `--no-private` leaves out. Other messages sent to this player alone, such as the
answers of server plugins, stay in the demo, so look at a demo before passing it on. A recording contains all messages
of *all* players.

Play a demo by copying it into the `demos` folder of the client and entering `demo <name>` in the console.

## Recording

Recording is part of shinqlx behind the Cargo feature `demo-recording`, which is off by default and experimental. The
documentation page [Recording matches](https://shinqlx.readthedocs.io/en/latest/demos.html) explains how to set it up,
its cvars, which recordings are kept and for how long, and what to consider for the privacy of the players.

## Format

The library [`ql_sqx`](../ql_sqx) reads and writes the recordings, for the server as well as for `ql_demo`. The format
is not stable yet: a recording may only be readable by the `ql_demo` of the same build as the server.
