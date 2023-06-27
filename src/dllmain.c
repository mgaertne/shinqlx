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
