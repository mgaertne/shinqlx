#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>
#include <stdint.h>
#include <string.h>
#include <signal.h>
#include <time.h>

#include "common.h"
#include "quake_common.h"
#include "patterns.h"
#include "maps_parser.h"

#if defined(__x86_64__) || defined(_M_X64)
const char qzeroded[] = "qzeroded.x64";
const char qagame_name[] = "qagamex64.so";
#elif defined(__i386) || defined(_M_IX86)
const char qzeroded[] = "qzeroded.x86";
const char qagame_name[] = "qagamei386.so";
#endif

// VM functions
G_RunFrame_ptr G_RunFrame;
G_AddEvent_ptr G_AddEvent;
G_InitGame_ptr G_InitGame;
CheckPrivileges_ptr CheckPrivileges;
ClientConnect_ptr ClientConnect;
ClientSpawn_ptr ClientSpawn;
G_Damage_ptr G_Damage;
Touch_Item_ptr Touch_Item;
LaunchItem_ptr LaunchItem;
Drop_Item_ptr Drop_Item;
G_StartKamikaze_ptr G_StartKamikaze;
G_FreeEntity_ptr G_FreeEntity;

// VM global variables.
gentity_t* g_entities;
level_locals_t* level;
gitem_t* bg_itemlist;
int bg_numItems;

// Cvars.
cvar_t* sv_maxclients;

// TODO: Make it output everything to a file too.
void DebugPrint(const char* fmt, ...) {
    va_list args;
    va_start(args, fmt);
    printf("%s", DEBUG_PRINT_PREFIX);
    vprintf(fmt, args);
    va_end(args);
}

// TODO: Make it output everything to a file too.
void DebugError(const char* fmt, const char* file, int line, const char* func, ...) {
    va_list args;
    va_start(args, func);
    fprintf(stderr, DEBUG_ERROR_FORMAT, file, line, func);
    vfprintf(stderr, fmt, args);
    va_end(args);
}

#define VM_SEARCH(x, p, m) x = (x ## _ptr) PatternSearch((void*)((pint)qagame + 0xB000), 0xB0000, p, m); if (x == NULL) { DebugPrint("ERROR: Unable to find " #x ".\n"); failed = 1;} else DebugPrint(#x ": %p\n", x)

// NOTE: Some functions can easily and reliably be found on the VM_Call table instead.
void SearchVmFunctions(void) {
    int failed = 0;

    // For some reason, the module doesn't show up when reading /proc/self/maps.
    // Perhaps this needs to be called later? In any case, we know exactly where
    // the module is mapped, so I think this is fine. If it ever breaks, it'll
    // be trivial to fix.
    VM_SEARCH(G_AddEvent, PTRN_G_ADDEVENT, MASK_G_ADDEVENT);
    VM_SEARCH(CheckPrivileges, PTRN_CHECKPRIVILEGES, MASK_CHECKPRIVILEGES);
    VM_SEARCH(ClientConnect, PTRN_CLIENTCONNECT, MASK_CLIENTCONNECT);
    VM_SEARCH(ClientSpawn, PTRN_CLIENTSPAWN, MASK_CLIENTSPAWN);
    VM_SEARCH(G_Damage, PTRN_G_DAMAGE, MASK_G_DAMAGE);
    VM_SEARCH(Touch_Item, PTRN_TOUCH_ITEM, MASK_TOUCH_ITEM);
    VM_SEARCH(LaunchItem, PTRN_LAUNCHITEM, MASK_LAUNCHITEM);
    VM_SEARCH(Drop_Item, PTRN_DROP_ITEM, MASK_DROP_ITEM);
    VM_SEARCH(G_StartKamikaze, PTRN_G_STARTKAMIKAZE, MASK_G_STARTKAMIKAZE);
    VM_SEARCH(G_FreeEntity, PTRN_G_FREEENTITY, MASK_G_FREEENTITY);

    if (failed) {
            DebugPrint("Exiting.\n");
            exit(1);
    }
}

// Initialize VM stuff. Needs to be called whenever Sys_SetModuleOffset is called,
// after qagame pointer has been initialized.
void InitializeVm(void) {
    DebugPrint("Initializing VM pointers...\n");
#if defined(__x86_64__) || defined(_M_X64)
    g_entities = (gentity_t*)(*(int32_t*)OFFSET_RELP_G_ENTITIES + OFFSET_RELP_G_ENTITIES + 4);
    level = (level_locals_t*)(*(int32_t*)OFFSET_RELP_LEVEL + OFFSET_RELP_LEVEL + 4);
    bg_itemlist = (gitem_t*)*(int64_t*)((*(int32_t*)OFFSET_RELP_BG_ITEMLIST + OFFSET_RELP_BG_ITEMLIST + 4));
#elif defined(__i386) || defined(_M_IX86)
    g_entities = (gentity_t*)(*(int32_t*)OFFSET_RELP_G_ENTITIES + 0xCEFF4 + (pint)qagame);
    level = (level_locals_t*)(*(int32_t*)OFFSET_RELP_LEVEL + 0xCEFF4 + (pint)qagame);
    bg_itemlist = (gitem_t*)*(int32_t*)((*(int32_t*)OFFSET_RELP_BG_ITEMLIST + 0xCEFF4 + (pint)qagame));
#endif
    for (bg_numItems = 1; bg_itemlist[ bg_numItems ].classname; bg_numItems++) {}
}
