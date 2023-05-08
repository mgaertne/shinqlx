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
// Execute a pyminqlx command as if it were the owner executing it.
// Output will appear in the console.
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
#endif
