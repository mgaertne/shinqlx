# The format of `.dm_sqx` recordings

This describes version 2. Until the first release with recordings the format may change without a new version number.
The code of `ql_sqx` is the reference; this is the map to it.

## Container

All integers outside of frame payloads are little endian.

| Part    | Content                                                      |
|---------|--------------------------------------------------------------|
| header  | the 8 bytes `dm_sqx\r\n`, then the version as `u16`          |
| records | `u8` kind, `u32` length of the payload, payload; until the end |

A file is written front to back. A reader

* skips records of kinds it does not know, so new kinds do not need a new version,
* treats an incomplete last record as the end of the file,
* rejects payloads longer than 16 MiB.

## Records

Below, *bytes* are a `u32` length followed by that many bytes.

| Kind | Name          | Payload                                                                                  |
|------|---------------|------------------------------------------------------------------------------------------|
| 1    | start         | `u32` count of descriptions, each as *bytes* key and *bytes* value; `u32` count of configstrings, each as `u16` number and *bytes* value |
| 2    | configstring  | `u16` number, then the new value up to the end; an empty value removes it                 |
| 3    | key frame     | a frame that depends on no earlier one                                                    |
| 4    | delta frame   | a frame as changes to the frame before                                                    |
| 5    | command       | `u8` count of recipients, one `u8` per recipient, then the command; recipient 255 means every client |
| 6    | player left   | `u8` client number                                                                        |
| 7    | end           | why the recording ended, as text                                                          |
| 8    | stats         | a stats message of the game as the JSON object `{"DATA":{…},"TYPE":"…"}`                  |
| 9    | inputs        | the commands of the clients since the frame before, a bit stream, see below              |
| 10   | shot          | `u8` client number, `u32` weapon, `i32` command time, position and view angles as `f32`  |
| 11   | damage        | `u8` client number of the target, `u8` `1` if it killed, `u16` attacking entity (65535 none), `i32` damage asked for, `u32` damage flags, `u32` means of death, `i32` health lost, `i32` armor lost |

The start record is the first one. Its descriptions are `map`, `factory`, `gametype`, `sv_fps`, `shinqlx` (the
version of the server) and `started` (seconds since the Unix epoch).

A command record stands for one command per recipient, in the order listed. Events are in the order they happened, and
a frame holds the state after them.

Key frames are written for the first frame, whenever the server time has advanced by 10 seconds or has gone backwards.
To start reading in the middle of a file, skip records by their length up to a key frame, apply every configstring
record before it, and continue from there.

Damage records tell about hits on living players only.

## Inputs

A bit stream coded like frame payloads: `16` bits number of commands, then the commands in the order the server ran
them. Each is coded as its changes to the command of the same client before, or to an all-zero command:

* `8` bits client number,
* bit `1` and `8` bits by which the time of the command advanced, or bit `0` and the time in `32` bits,
* for each of the three view angles and then the buttons one bit whether it changed, followed by `32` bits,
* one bit whether anything else changed; if so `8` bits each of weapon, primary weapon, field of view, forward, right
  and up.

Key frames start the coding anew: the commands in the first inputs record after a key frame are coded against all-zero
commands.

## Frames

A frame payload is a bit stream as in the network messages of the engine: the lowest bit of a byte comes first, the
bits of a value beyond its whole bytes are written as they are and every whole byte is coded with the static Huffman
code of the engine (`src/huffman_codes.in`). The stream is padded to one byte more than its full bytes. *Raw* bits
below are never Huffman coded.

1. `32` bits server time in milliseconds.
2. `8` bits number of players, then for each `8` bits client number and the changes of its player state.
3. The entity list.
4. The visibility changes.

### Player states

A player state has 58 fields of 32 bits (`PLAYER_FIELDS` in `src/fields.rs`, the order of the engine) and the arrays
`stats`, `persistant`, `ammo` (16 × `i16`) and `powerups` (16 × `i32`). Changes are relative to the state of the same
client in the frame before, or to an all-zero state:

* `8` bits number of fields up to the last changed one,
* for each of them one bit whether it changed, followed by its value,
* one bit whether any array changed; if so, for each of the four arrays one bit whether it changed, followed by a `16`
  bit mask of the changed elements and their values.

Most values are coded like netfields of the engine: integers with the width of the field; floats as a bit that is `0`
for a whole number within ±4096, followed by it plus 4096 in `13` bits, and `1` followed by all `32` bits otherwise.
Recordings code the fields that change all the time differently (`src/player_coding.rs`):

* `commandTime`: bit `1` and the server time minus the value as `i8`, or bit `0` and `32` bits,
* `viewangles`: bit `1` and a `16` raw bit angle in steps of 360/65536 degrees if that gives the value exactly,
  otherwise bit `0` and the netfield,
* `origin` and `velocity`: the exclusive or with the value before, as `5` raw bits for the position of its highest set
  bit and then the raw bits below it.

### Entities

An entity has a number of `10` bits and 58 fields (`ENTITY_FIELDS`). The list is coded like the entities of a network
snapshot: entries in increasing order of their numbers, ended by the number 1023. Entities without an entry did not
change. An entry is the number, then a bit that is `1` if the entity was removed. Otherwise a bit tells whether any
field changed; if so follow `8` bits number of fields up to the last changed one and for each of them one bit whether
it changed, then one bit that is `0` for the value zero, then the value as a netfield.

The entity of a player (its number is the client number) is not compared with the frame before directly. The reader and
the writer first replace the fields the game derives from the player state of this frame (`src/prediction.rs`), so that
only differences from this expectation are stored. New entities start from all zeros, or from the expectation for a
player.

### Visibility

The game shows some entities to one client only, or to all but one. For each entity whose visibility differs from the
frame before (new entities are visible to everyone): `10` bits number, `2` bits kind (`0` everyone, `1` only one,
`2` all but one) and for the latter two `32` bits client number. The number 1023 ends the list.

Entities that are sent to nobody, like an item waiting to respawn, are not part of a frame.
