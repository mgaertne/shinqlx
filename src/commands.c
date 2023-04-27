#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif

#include <stdlib.h>
#include <stdio.h>
#include <inttypes.h>

#include "quake_common.h"
#include "common.h"

#ifndef NOPY
#include "pyminqlx.h"
#endif

#ifndef NOPY
void __cdecl PyCommand(void) {
    if (!custom_command_handler) {
            return; // No registered handler.
    }
    PyGILState_STATE gstate = PyGILState_Ensure();

    PyObject* result = PyObject_CallFunction(custom_command_handler, "s", Cmd_Args());
    if (result == Py_False) {
        Com_Printf("The command failed to be executed. pyminqlx found no handler.\n");
    }

    Py_XDECREF(result);
    PyGILState_Release(gstate);
}

void __cdecl RestartPython(void) {
    Com_Printf("Restarting Python...\n");
    if (PyMinqlx_IsInitialized())
        PyMinqlx_Finalize();
    PyMinqlx_Initialize();
    // minqlx initializes after the first new game starts, but since the game already
    // start, we manually trigger the event to make it initialize properly.
    NewGameDispatcher(0);
}
#endif
