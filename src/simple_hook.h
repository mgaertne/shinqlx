#ifndef SIMPLE_HOOK_H
#define SIMPLE_HOOK_H

void* HookRaw(void* target, void* replacement),
int Hook(void* target, void* replacement, void** func_ptr);
int seek_hook_slot(int offset);

#endif /* SIMPLE_HOOK_H */
