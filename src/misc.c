#include <stdlib.h>
#include <string.h>

#include "common.h"
#include "quake_common.h"
#include "maps_parser.h"

/* Takes a 64-bit integer used as a bit field as flags for which player
 * has an action pending, removes the flag and returns the client ID.
 * The server only allows up to 64 players, so a 64-bit int covers it all.
 * 
 * Returns -1 if no flag is set, so use it in a loop until it does so. */
int GetPendingPlayer(uint64_t* players) {
    int flag = -1;
    // We first check if any bitfield is set.
    if (!*players) {
        return flag;
    } else {
        for (int id = 0; id < 64; id++) {
            // Check bit i's flag.
            flag = *players & (1LL << id);
            // Remove the flag we checked, if present.
            *players &= ~flag;
            // If the flag was set, return client id.
            if (flag) return id;
        }
    }

    return -1; // All flags have been cleared.
}

// Set a flag on client ID to indicate a pending action on the player.
void SetPendingPlayer(uint64_t* players, int client_id) {
    *players |= 1LL << client_id;
}

// (0.0f, 1.0f)
float RandomFloat(void) {
    return (float)rand()/(float)RAND_MAX;
}

// (-1.0f, 1.0f)
float RandomFloatWithNegative(void) {
    return (float)rand()/(float)(RAND_MAX/2) - 1;
}

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

void* PatternSearchModule(module_info_t* module, const char* pattern, const char* mask) {
    void* res = NULL;
    for (int i = 0; i < module->entries; i++) {
        if (!(module->permissions[i] & PG_READ)) continue;
        size_t size = module->address_end[i] - module->address_start[i];
        res = PatternSearch((void*)module->address_start[i], size, pattern, mask);
        if (res) break;
    }

    return res;
}

