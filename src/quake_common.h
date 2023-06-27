/*
Copyright (C) 1997-2001 Id Software, Inc.
Copyright (C) 2015 Mino <mino@minomino.org>

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <http://www.gnu.org/licenses/>.
*/

/*
 * Mino: A lot of this is from Q3 sources, but obviously the structs aren't
 * exactly the same, so there's a good number of modifications to make it
 * fit QL. The end of the file has a bunch of stuff I added. Might want
 * to refactor it. TODO.
*/

#ifndef QUAKE_COMMON_H
#define QUAKE_COMMON_H

#include "patterns.h"
#include "common.h"
#include "quake_types.h"

// A pointer to the qagame module in memory and its entry point.
extern void* qagame;
extern void* qagame_dllentry;

// Internal QL function pointer types.
// VM functions.
typedef void (__cdecl *Cmd_CallVote_f_ptr)(gentity_t *ent);

// Some of them are initialized by Initialize(), but not all of them necessarily.
// VM functions.
extern Cmd_CallVote_f_ptr Cmd_CallVote_f;

#endif /* QUAKE_COMMON_H */
