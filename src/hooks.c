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

// Hook static functions. Can be done before program even runs.
void HookStatic(void) {
    int res, failed = 0;

    // ==============================
    //    ONLY NEEDED FOR PYTHON
    // ==============================
#ifndef NOPY
    res = Hook((void*)SV_SendServerCommand,ShiNQlx_SV_SendServerCommand, (void*)&SV_SendServerCommand);
    if (res) {
        DebugPrint("ERROR: Failed to hook SV_SendServerCommand: %d\n", res);
        failed = 1;
    }

    res = Hook((void*)Com_Printf, ShiNQlx_Com_Printf, (void*)&Com_Printf);
    if (res) {
        DebugPrint("ERROR: Failed to hook Com_Printf: %d\n", res);
        failed = 1;
    }
#endif

    if (failed) {
        DebugPrint("Exiting.\n");
        exit(1);
    }
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
#endif
}
