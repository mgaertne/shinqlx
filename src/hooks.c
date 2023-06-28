#define _GNU_SOURCE
#define __STDC_FORMAT_MACROS

#include <string.h>
#include <stdlib.h>
#include <stdio.h>
#include <dlfcn.h>

#include "simple_hook.h"

// qagame module.
void* qagame;
void* qagame_dllentry;

void* HookRaw(void* target, void* replacement) {
    void* returned = NULL;
    int hook_result = 0;

    hook_result = Hook(target, replacement, (void*)&returned);
    if (hook_result) {
        return NULL;
    }

    return returned;
}
