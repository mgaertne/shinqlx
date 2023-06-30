#include <stddef.h>
#include <stdarg.h>
#include <stdio.h>

#include "quake_types.h"

typedef uint64_t pint;
#define __cdecl

#define PTRN_G_ADDEVENT "\x85\xf6\x74\x00\x48\x8b\x8f\x00\x00\x00\x00\x48\x85\xc9\x74\x00\x8b\x81\x00\x00\x00\x00\x25\x00\x00\x00\x00\x05\x00\x00\x00\x00\x25\x00\x00\x00\x00\x09\xf0\x89\x81\x00\x00\x00\x00"
#define MASK_G_ADDEVENT "XXX-XXX----XXXX-XX----X----X----X----XXXX----"
#define PTRN_CHECKPRIVILEGES "\x41\x56\x89\x15\x00\x00\x00\x00\x49\x89\xfe\x48\x8d\x3d\x00\x00\x00\x00\x41\x55\x41\x89\xd5\x41\x54\x49\x89\xf4\x55\x31\xed\x53\x48\x8d\x1d\x00\x00\x00\x00\xeb\x00\x0f\x1f\x80\x00\x00\x00\x00"
#define MASK_CHECKPRIVILEGES "XXXX----XXXXXX----XXXXXXXXXXXXXXXXX----X-XXX----"
#define PTRN_CLIENTCONNECT "\x41\x57\x4c\x63\xff\x41\x56\x41\x89\xf6\x41\x55\x41\x54\x55\x4c\x89\xfd\x48\xc1\xe5\x00\x53\x89\xfb\x48\x81\xec\x00\x00\x00\x00\x4c\x8b\x2d\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00"
#define MASK_CLIENTCONNECT "XXXXXXXXXXXXXXXXXXXXX-XXXXXX----XXX----XXXXX----"
#define PTRN_CLIENTSPAWN "\x41\x57\x41\x56\x49\x89\xfe\x41\x55\x41\x54\x55\x53\x48\x81\xec\x00\x00\x00\x00\x4c\x8b\xbf\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0"
#define MASK_CLIENTSPAWN "XXXXXXXXXXXXXXXX----XXX----XXXXX----XXXX----XX"
#define PTRN_G_DAMAGE "\x41\x57\x41\x56\x41\x55\x41\x54\x55\x53\x48\x89\xfb\x48\x81\xec\x00\x00\x00\x00\x44\x8b\x97\x00\x00\x00\x00\x48\x8b\xaf\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00"
#define MASK_G_DAMAGE "XXXXXXXXXXXXXXXX----XXX----XXX----XXXXX----"
#define PTRN_TOUCH_ITEM "\x41\x57\x41\x56\x41\x55\x41\x54\x55\x53\x48\x89\xf3\x48\x81\xec\x00\x00\x00\x00\x4c\x8b\x86\x00\x00\x00\x00\x4d\x85\xc0\x74\x00\x8b\x96\x00\x00\x00\x00\x85\xd2\x7e\x00\x4c\x8b\x35\x00\x00\x00\x00"
#define MASK_TOUCH_ITEM "XXXXXXXXXXXXXXXX----XXX----XXXX-XX----XXX-XXX----"
#define PTRN_LAUNCHITEM "\x41\x55\x31\xc0\x49\x89\xf5\x41\x54\x49\x89\xd4\x55\x48\x89\xfd\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\xc7\x40\x04\x00\x00\x00\x00\x48\x89\xc3\x48\x89\xe8\x48\x2b\x05\x00\x00\x00\x00"
#define MASK_LAUNCHITEM "XXXXXXXXXXXXXXXXXXXX-X----XXX----XXXXXXXXX----"
#define PTRN_DROP_ITEM "\x41\x54\x31\xc9\x31\xd2\x49\x89\xf4\x55\x53\x48\x89\xfb\x48\x83\xec\x00\xf3\x0f\x10\x4f\x00\x48\x8d\x6c\x24\x00\xc7\x44\x24\x20\x00\x00\x00\x00\xf3\x0f\x58\xc8\xf3\x0f\x10\x57\x00\x48\x8d\x7c\x24\x00"
#define MASK_DROP_ITEM "XXXXXXXXXXXXXXXXX-XXXX-XXXX-XXXX----XXXXXXXX-XXXX-"
#define PTRN_G_STARTKAMIKAZE "\x41\x55\x31\xc0\x41\x54\x55\x48\x89\xfd\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x4c\x8b\x25\x00\x00\x00\x00\xc7\x40\x04\x00\x00\x00\x00\x48\x89\xc3\x41\x8b\x44\x00\x24\x89\x83\x00\x00\x00\x00"
#define MASK_G_STARTKAMIKAZE "XXXXXXXXXXXXXX-X----XXX----XXX----XXXXXX-XXX----"
#define PTRN_G_FREEENTITY "\x48\x8b\x05\x00\x00\x00\x00\x53\x48\x89\xfb\x48\x8b\x00\xff\x90\x00\x00\x00\x00\x8b\x83\x00\x00\x00\x00\x85\xc0\x74\x00\x5b\xc3"
#define MASK_G_FREEENTITY "XXX----XXXXXXXXX----XX----XXX-XX"

// qagame structs and global varaibles.
#define OFFSET_RELP_VM_CALL_TABLE		((pint)qagame_dllentry + 0x3)

// VM_Call table offsets.
#define RELOFFSET_VM_CALL_INITGAME   0x18
#define RELOFFSET_VM_CALL_RUNFRAME   0x8
#define RELOFFSET_VM_CALL_SHUTDOWNGAME   0x0

// VM functions.
typedef void (__cdecl *G_RunFrame_ptr)(int time);
typedef void (__cdecl *G_AddEvent_ptr)(gentity_t* ent, int event, int eventParm);
typedef void (__cdecl *G_ShutdownGame_ptr)(int restart);
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

// VM functions
G_RunFrame_ptr G_RunFrame;
G_AddEvent_ptr G_AddEvent;
G_ShutdownGame_ptr G_ShutdownGame;
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

#define DEBUG_PRINT_PREFIX "[ShiNQlx] "
#define DEBUG_ERROR_FORMAT "[ShiNQlx] ERROR @ %s:%d in %s:\n" DEBUG_PRINT_PREFIX

// TODO: Make it output everything to a file too.
void DebugPrint(const char* fmt, ...) {
    va_list args;
	va_start(args, fmt);
    printf(DEBUG_PRINT_PREFIX);
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

void* PatternSearch(void* address, size_t length, const char* pattern, const char* mask) {
  for (size_t i = 0; i < length; i++) {
    for (size_t j = 0; mask[j]; j++) {
      if (mask[j] == 'X' && pattern[j] != ((char*)address)[i + j]) {
        break;
      }
      else if (mask[j + 1]) {
        continue;
      }

      return (void*)(((pint)address) + i);
    }
  }
  return NULL;
}

#define VM_SEARCH(x, p, m) x = (x ## _ptr) PatternSearch((void*)((pint)qagame + 0xB000), 0xB0000, p, m); if (x == NULL) { DebugPrint("ERROR: Unable to find " #x ".\n"); failed = 1;} else DebugPrint(#x ": %p\n", x)

// NOTE: Some functions can easily and reliably be found on the VM_Call table instead.
int SearchVmFunctions(void* qagame, void* qagame_dllentry) {
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

    pint vm_call_table = *(int32_t*)OFFSET_RELP_VM_CALL_TABLE + OFFSET_RELP_VM_CALL_TABLE + 4;
    G_InitGame = *(G_InitGame_ptr*)(vm_call_table + RELOFFSET_VM_CALL_INITGAME);
    DebugPrint("G_InitGame: %p\n", G_InitGame);
    G_RunFrame = *(G_RunFrame_ptr*)(vm_call_table + RELOFFSET_VM_CALL_RUNFRAME);
    DebugPrint("G_RunFrame: %p\n", G_RunFrame);
    G_ShutdownGame = *(G_ShutdownGame_ptr*)(vm_call_table + RELOFFSET_VM_CALL_SHUTDOWNGAME);
    DebugPrint("G_ShutdownGame: %p\n", G_ShutdownGame);

    return failed;
}
