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
extern serverStatic_t* svs;
extern gentity_t* g_entities;
extern level_locals_t* level;
extern gitem_t* bg_itemlist;
extern int bg_numItems;
// Cvars.
extern cvar_t* sv_maxclients;

// Internal QL function pointer types.
typedef void (__cdecl *Com_Printf_ptr)(char* fmt, ...);
typedef void (__cdecl *Cmd_AddCommand_ptr)(char* cmd, void* func);
typedef char* (__cdecl *Cmd_Args_ptr)(void);
typedef char* (__cdecl *Cmd_Argv_ptr)(int arg);
typedef int (__cdecl *Cmd_Argc_ptr)(void);
typedef void (__cdecl *Cmd_TokenizeString_ptr)(const char* text_in);
typedef void (__cdecl *Cbuf_ExecuteText_ptr)(int exec_when, const char* text);
typedef cvar_t* (__cdecl *Cvar_FindVar_ptr)(const char* var_name);
typedef cvar_t* (__cdecl *Cvar_Get_ptr)(const char* var_name, const char* var_value, int flags);
typedef cvar_t* (__cdecl *Cvar_GetLimit_ptr)(const char* var_name, const char* var_value, const char* min, const char* max, int flag);
typedef cvar_t* (__cdecl *Cvar_Set2_ptr)(const char* var_name, const char* value, qboolean force);
typedef void (__cdecl *SV_SendServerCommand_ptr)(client_t* cl, const char* fmt, ...);
typedef void (__cdecl *SV_ExecuteClientCommand_ptr)(client_t* cl, const char* s, qboolean clientOK);
typedef void (__cdecl *SV_ClientEnterWorld_ptr)(client_t *client, usercmd_t *cmd);
typedef void (__cdecl *SV_Shutdown_ptr)(char* finalmsg);
typedef void (__cdecl *SV_Map_f_ptr)(void);
typedef void (__cdecl *SV_ClientThink_ptr)(client_t* cl, usercmd_t* cmd);
typedef void (__cdecl *SV_SetConfigstring_ptr)(int index, const char* value);
typedef void (__cdecl *SV_GetConfigstring_ptr)(int index, char* buffer, int bufferSize);
typedef void (__cdecl *SV_DropClient_ptr)(client_t* drop, const char* reason);
typedef void (__cdecl *FS_Startup_ptr)(const char* gameName);
typedef void (__cdecl *Sys_SetModuleOffset_ptr)(char* moduleName, void* offset);
typedef void (__cdecl *SV_LinkEntity_ptr)(sharedEntity_t* gEnt);
typedef void (__cdecl *SV_SpawnServer_ptr)(char* server, qboolean killBots);
typedef void (__cdecl *Cmd_ExecuteString_ptr)(const char* text);
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
extern Com_Printf_ptr Com_Printf;
extern Cmd_AddCommand_ptr Cmd_AddCommand;
extern Cmd_Args_ptr Cmd_Args;
extern Cmd_Argv_ptr Cmd_Argv;
extern Cmd_Argc_ptr Cmd_Argc;
extern Cmd_TokenizeString_ptr Cmd_TokenizeString;
extern Cbuf_ExecuteText_ptr Cbuf_ExecuteText;
extern Cvar_FindVar_ptr Cvar_FindVar;
extern Cvar_Get_ptr Cvar_Get;
extern Cvar_GetLimit_ptr Cvar_GetLimit;
extern Cvar_Set2_ptr Cvar_Set2;
extern SV_SendServerCommand_ptr SV_SendServerCommand;
extern SV_ExecuteClientCommand_ptr SV_ExecuteClientCommand;
extern SV_ClientEnterWorld_ptr SV_ClientEnterWorld;
extern SV_Shutdown_ptr SV_Shutdown; // Used to get svs pointer.
extern SV_Map_f_ptr SV_Map_f; // Used to get Cmd_Argc
extern SV_SetConfigstring_ptr SV_SetConfigstring;
extern SV_GetConfigstring_ptr SV_GetConfigstring;
extern SV_DropClient_ptr SV_DropClient;
extern Sys_SetModuleOffset_ptr Sys_SetModuleOffset;
extern SV_SpawnServer_ptr SV_SpawnServer;
extern Cmd_ExecuteString_ptr Cmd_ExecuteString;
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
extern void __cdecl ShiNQlx_Com_Printf(char* fmt, ...);
// VM replacement functions for hooks.
extern void __cdecl ShiNQlx_G_RunFrame(int time);
extern void __cdecl ShiNQlx_G_InitGame(int levelTime, int randomSeed, int restart);
#endif

// Custom commands added using Cmd_AddCommand during initialization.
#ifndef NOPY
// PyCommand is special. It'll serve as the handler for console commands added
// using Python. This means it can serve as the handler for a bunch of commands,
// and it'll take care of redirecting it to Python.
void __cdecl PyCommand(void);
#endif

#endif /* QUAKE_COMMON_H */
