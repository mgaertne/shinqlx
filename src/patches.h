void patch_vm(void);

#if defined(__x86_64__) || defined(_M_X64)

#define ADDR_VOTE_CLIENTKICK_FIX ((pint)Cmd_CallVote_f + 0x11C8)
#define PTRN_VOTE_CLIENTKICK_FIX "\x39\xFE\x0F\x8D\x90\x00\x00\x00\x48\x69\xD6\xF8\x0B\x00\x00\x48\x01\xD0\x90\x90\x90\x0\x0\x0\x0\x0\x0\x0\x0f\x85\x76\x00\x00\x00\x90\x90\x90\x90"
#define MASK_VOTE_CLIENTKICK_FIX "XXXXXXXXXXXXXXXXXXXXX-------XXXXXXXXXX"

#define PTRN_CMD_CALLVOTE_F "\x41\x57\x41\x56\x41\x55\x41\x54\x55\x48\x89\xfd\x53\x48\x81\xec\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0\xe8\x00\x00\x00\x00"
#define MASK_CMD_CALLVOTE_F "XXXXXXXXXXXXXXXX----XXXXX----XXXX----XXX----"

#else

#define ADDR_VOTE_CLIENTKICK_FIX ((pint)Cmd_CallVote_f + 0x0F8C)
#define PTRN_VOTE_CLIENTKICK_FIX "\x69\xc8\xd0\x0b\x0\x0\x01\xca\x90\x0\x44\x0\x0\x0\x0\x0\x0\x0\x0\x0\x0\x0\x0\x6c\x90\x90\x90\x90\x90\x90\x90\x90"
#define MASK_VOTE_CLIENTKICK_FIX "XXXXXXXXX-X------------XXXXXXXXX"

#define PTRN_CMD_CALLVOTE_F "\x81\xec\x00\x00\x00\x00\x89\x9c\x24\x00\x00\x00\x00\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x89\xbc\x24\x00\x00\x00\x00\x89\xac\x24\x00\x00\x00\x00\x8b\xac\x24\x00\x00\x00\x00"
#define MASK_CMD_CALLVOTE_F "XX----XXX----X----XX----XXX----XXX----XXX----"

#endif
