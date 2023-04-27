#ifndef MAPS_PARSER_H
#define MAPS_PARSER_H

/*
 * I can't believe we have to do this shit to get basic memory
 * layout information. Linux, please. Is this even POSIX?
 *
 * Anyway, instead of parsing the whole thing we only go for
 * a single module, since that's the info we need.
 */

#include <stdint.h>

#if defined(__x86_64__) || defined(_M_X64)
typedef uint64_t pint;
typedef int64_t sint;
#elif defined(__i386) || defined(_M_IX86)
typedef uint32_t pint;
typedef int32_t sint;
#endif

// Permission flags. The two last are mutually exclusive.
#define PG_READ     1
#define PG_WRITE    2
#define PG_EXECUTE  4
#define PG_PRIVATE  8
#define PG_SHARED   16

typedef struct {
    char name[512];
    char path[4096];
    int entries;
    int permissions[128];
    pint address_start[128];
    pint address_end[128];
} module_info_t;

int GetModuleInfo(module_info_t* module_info);

#endif /* MAPS_PARSER_H */
