#ifndef COMMON_H
#define COMMON_H

#ifndef MINQLX_VERSION
#define MINQLX_VERSION "NOT_SET"
#endif

#define DEBUG_PRINT_PREFIX "[ShiNQlx] "
#define DEBUG_ERROR_FORMAT "[ShiNQlx] ERROR @ %s:%d in %s:\n" DEBUG_PRINT_PREFIX

#ifndef NOPY
#define SV_TAGS_PREFIX "ShiNQlx"
#else
#define SV_TAGS_PREFIX "ShiNQlx-nopy"
#endif

// TODO: Add minqlx version to serverinfo.

#include <stdint.h>

// We need an unsigned integer that's guaranteed to be the size of a pointer.
// "unsigned int" should do it, but we might want to port this to Windows for
// listen servers, where ints are 32 even on 64-bit Windows, so we're explicit.
#if defined(__x86_64__) || defined(_M_X64)
typedef uint64_t pint;
typedef int64_t sint;
#define __cdecl
#elif defined(__i386) || defined(_M_IX86)
typedef uint32_t pint;
typedef int32_t sint;
#define __cdecl __attribute__((__cdecl__))
#endif

void* HookRaw(void* target, void* replacement);
void DebugPrint(const char* fmt, ...);
void DebugError(const char* fmt, const char* file, int line, const char* func, ...);

// Misc.
void* PatternSearch(void* address, size_t length, const char* pattern, const char* mask);

#endif /* COMMON_H */
