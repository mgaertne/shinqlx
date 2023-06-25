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

// Additional key struct pointers.
extern gentity_t* g_entities;
extern level_locals_t* level;
extern gitem_t* bg_itemlist;
extern int bg_numItems;

// Internal QL function pointer types.
typedef void (__cdecl *SV_SendServerCommand_ptr)(client_t* cl, const char* fmt, ...);
// VM functions.
typedef void (__cdecl *G_RunFrame_ptr)(int time);
typedef void (__cdecl *G_AddEvent_ptr)(gentity_t* ent, int event, int eventParm);
typedef void (__cdecl *G_InitGame_ptr)(int levelTime, int randomSeed, int restart);
typedef int (__cdecl *CheckPrivileges_ptr)(gentity_t* ent, char* cmd);
typedef char* (__cdecl *ClientConnect_ptr)(int clientNum, qboolean firstTime, qboolean isBot);
typedef void (__cdecl *ClientSpawn_ptr)(gentity_t* ent);
typedef void (__cdecl *Cmd_CallVote_f_ptr)(gentity_t *ent);
typedef void (__cdecl *G_Damage_ptr)(gentity_t *targ, gentity_t *inflictor, gentity_t *attacker, vec3_t dir, vec3_t point, int damage, int dflags, int mod);
typedef void (__cdecl *Touch_Item_ptr)(gentity_t *ent, gentity_t *other, trace_t *trace);
typedef gentity_t* (__cdecl *LaunchItem_ptr)(gitem_t *item, vec3_t origin, vec3_t velocity);
typedef gentity_t* (__cdecl *Drop_Item_ptr)(gentity_t *ent, gitem_t *item, float angle);
typedef void (__cdecl *G_StartKamikaze_ptr)(gentity_t *ent);
typedef void (__cdecl *G_FreeEntity_ptr)(gentity_t *ed);

// Some of them are initialized by Initialize(), but not all of them necessarily.
extern SV_SendServerCommand_ptr SV_SendServerCommand;
// VM functions.
extern G_RunFrame_ptr G_RunFrame;
extern G_AddEvent_ptr G_AddEvent;
extern G_InitGame_ptr G_InitGame;
extern CheckPrivileges_ptr CheckPrivileges;
extern ClientConnect_ptr ClientConnect;
extern ClientSpawn_ptr ClientSpawn;
extern Cmd_CallVote_f_ptr Cmd_CallVote_f;
extern G_Damage_ptr G_Damage;
extern Touch_Item_ptr Touch_Item;
extern LaunchItem_ptr LaunchItem;
extern Drop_Item_ptr Drop_Item;
extern G_StartKamikaze_ptr G_StartKamikaze;
extern G_FreeEntity_ptr G_FreeEntity;

// Server replacement functions for hooks.
#ifndef NOPY
extern void __cdecl ShiNQlx_SV_SendServerCommand(client_t* cl, char* fmt, ...);
// VM replacement functions for hooks.
extern void __cdecl ShiNQlx_G_RunFrame(int time);
extern void __cdecl ShiNQlx_G_InitGame(int levelTime, int randomSeed, int restart);
extern char* __cdecl ShiNQlx_ClientConnect(int clientNum, qboolean firstTime, qboolean isBot);
extern void __cdecl ShiNQlx_ClientSpawn(gentity_t* ent);

extern void __cdecl ShiNQlx_G_StartKamikaze(gentity_t* ent);
extern void __cdecl ShiNQlx_G_Damage(gentity_t* target, gentity_t* inflictor, gentity_t* attacker, vec3_t dir, vec3_t point, int damage, int dflags, int mod);
#endif

#endif /* QUAKE_COMMON_H */
