#define _GNU_SOURCE
#define __STDC_FORMAT_MACROS

#include <string.h>
#include <stdlib.h>
#include <stdio.h>
#include <dlfcn.h>

#include "patterns.h"
#include "common.h"
#include "quake_common.h"
#include "simple_hook.h"

// qagame module.
void* qagame;
void* qagame_dllentry;

void* HookVariadic(void* target, void* replacement) {
    void* returned = NULL;
    int hook_result = 0;

    DebugPrint("target: %p, replacement: %p\n", target, replacement);
    hook_result = Hook(target, replacement, (void*)&returned);
    if (hook_result) {
        return NULL;
    }

    DebugPrint("returned: %p\n", returned);
    return returned;
}

/*
 * Hooks VM calls. Not all use Hook, since the VM calls are stored in a table of
 * pointers. We simply set our function pointer to the current pointer in the table and
 * then replace the it with our replacement function. Just like hooking a VMT.
 * 
 * This must be called AFTER Sys_SetModuleOffset, since Sys_SetModuleOffset is called after
 * the VM DLL has been loaded, meaning the pointer we use has been set.
 *
 * PROTIP: If you can, ALWAYS use VM_Call table hooks instead of using Hook().
*/
void HookVm(void) {
    DebugPrint("Hooking VM functions...\n");

#if defined(__x86_64__) || defined(_M_X64)
    pint vm_call_table = *(int32_t*)OFFSET_RELP_VM_CALL_TABLE + OFFSET_RELP_VM_CALL_TABLE + 4;
#elif defined(__i386) || defined(_M_IX86)
    pint vm_call_table = *(int32_t*)OFFSET_RELP_VM_CALL_TABLE + 0xCEFF4 + (pint)qagame;
#endif

    G_InitGame = *(G_InitGame_ptr*)(vm_call_table + RELOFFSET_VM_CALL_INITGAME);
    *(void**)(vm_call_table + RELOFFSET_VM_CALL_INITGAME) = ShiNQlx_G_InitGame;

    G_RunFrame = *(G_RunFrame_ptr*)(vm_call_table + RELOFFSET_VM_CALL_RUNFRAME);

#ifndef NOPY
    *(void**)(vm_call_table + RELOFFSET_VM_CALL_RUNFRAME) = ShiNQlx_G_RunFrame;

    int res, failed = 0, count = 0;
    res = Hook((void*)ClientConnect, ShiNQlx_ClientConnect, (void*)&ClientConnect);
    if (res) {
        DebugPrint("ERROR: Failed to hook ClientConnect: %d\n", res);
        failed = 1;
    }
    count++;

    res = Hook((void*)G_StartKamikaze, ShiNQlx_G_StartKamikaze, (void*)&G_StartKamikaze);
    if (res) {
        DebugPrint("ERROR: Failed to hook G_StartKamikaze: %d\n", res);
        failed = 1;
    }
    count++;

    res = Hook((void*)ClientSpawn, ShiNQlx_ClientSpawn, (void*)&ClientSpawn);
    if (res) {
        DebugPrint("ERROR: Failed to hook ClientSpawn: %d\n", res);
        failed = 1;
    }
    count++;

    res = Hook((void*)G_Damage, ShiNQlx_G_Damage, (void*)&G_Damage);
    if (res) {
        DebugPrint("ERROR: Failed to hook G_Damage: %d\n", res);
        failed = 1;
    }
    count++;

    if (failed) {
        DebugPrint("Exiting.\n");
        exit(1);
    }

    if ( !seek_hook_slot(-count) ) {
        DebugPrint("ERROR: Failed to rewind hook slot\nExiting.\n");
        exit(1);
    }
#endif
}
