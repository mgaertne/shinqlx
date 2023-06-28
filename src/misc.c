#include <stdlib.h>
#include <string.h>

#include "common.h"
#include "quake_common.h"

void* PatternSearch(void* address, size_t length, const char* pattern, const char* mask) {
    for (size_t i = 0; i < length; i++) {
        for (size_t j = 0; mask[j]; j++) {
            if (mask[j] == 'X' && pattern[j] != ((char*)address)[i + j]) {
                break;
            } else if (mask[j + 1]) {
                continue;
            }

            return (void*)(((pint)address) + i);
        }
    }
    return NULL;
}
