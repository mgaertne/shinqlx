use std::fmt::{Display, Formatter};

pub(crate) fn pattern_search_module(
    module_info: &Vec<&procfs::process::MemoryMap>,
    ql_func: &QuakeLiveFunction,
) -> Option<u64> {
    for memory_map in module_info {
        if !memory_map
            .perms
            .contains(procfs::process::MMPermissions::READ)
        {
            continue;
        }
        let result = pattern_search(memory_map.address.0, memory_map.address.1, ql_func);
        if result.is_some() {
            return result;
        }
    }
    None
}

pub(crate) fn pattern_search(start: u64, end: u64, ql_func: &QuakeLiveFunction) -> Option<u64> {
    let pattern = ql_func.pattern();
    let mask = ql_func.mask();
    for i in start..end {
        for j in 0..pattern.len() {
            let char: u8 = unsafe { std::ptr::read((i as usize + j) as *const u8) };
            let pattern_char: u8 = pattern[j];
            let mask_char: u8 = mask[j];
            if mask_char == b'X' && pattern_char != char {
                break;
            } else if j + 1 < mask.len() {
                continue;
            }
            return Some(i);
        }
    }
    None
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[allow(dead_code)]
#[allow(non_camel_case_types)]
pub(crate) enum QuakeLiveFunction {
    Com_Printf,
    Cmd_AddCommand,
    Cmd_Args,
    Cmd_Argv,
    Cmd_Argc,
    Cmd_Tokenizestring,
    Cbuf_ExecuteText,
    Cvar_FindVar,
    Cvar_Get,
    Cvar_GetLimit,
    Cvar_Set2,
    SV_SendServerCommand,
    SV_ExecuteClientCommand,
    SV_Shutdown,
    SV_Map_f,
    SV_ClientEnterWorld,
    SV_SetConfigstring,
    SV_GetConfigstring,
    SV_DropClient,
    Sys_SetModuleOffset,
    SV_SpawnServer,
    Cmd_ExecuteString,
    G_InitGame,
    G_RunFrame,
    ClientConnect,
    G_StartKamikaze,
    ClientSpawn,
    G_Damage,
    G_AddEvent,
    CheckPrivileges,
    Touch_Item,
    LaunchItem,
    Drop_Item,
    G_FreeEntity,
    Cmd_Callvote_f,
}

impl Display for QuakeLiveFunction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            QuakeLiveFunction::Com_Printf => f.write_str("Com_Printf"),
            QuakeLiveFunction::Cmd_AddCommand => f.write_str("Cmd_AddCommand"),
            QuakeLiveFunction::Cmd_Args => f.write_str("Cmd_Args"),
            QuakeLiveFunction::Cmd_Argv => f.write_str("Cmd_Argv"),
            QuakeLiveFunction::Cmd_Argc => f.write_str("Cmd_Argc"),
            QuakeLiveFunction::Cmd_Tokenizestring => f.write_str("Cmd_Tokenizestring"),
            QuakeLiveFunction::Cbuf_ExecuteText => f.write_str("Cbuf_ExecuteText"),
            QuakeLiveFunction::Cvar_FindVar => f.write_str("Cvar_FindVar"),
            QuakeLiveFunction::Cvar_Get => f.write_str("Cvar_Get"),
            QuakeLiveFunction::Cvar_GetLimit => f.write_str("Cvar_GetLimit"),
            QuakeLiveFunction::Cvar_Set2 => f.write_str("Cvar_Set2"),
            QuakeLiveFunction::SV_SendServerCommand => f.write_str("SV_SendServerCommand"),
            QuakeLiveFunction::SV_ExecuteClientCommand => f.write_str("SV_ExecuteClientCommand"),
            QuakeLiveFunction::SV_Shutdown => f.write_str("SV_Shutdown"),
            QuakeLiveFunction::SV_Map_f => f.write_str("SV_Map_f"),
            QuakeLiveFunction::SV_ClientEnterWorld => f.write_str("SV_ClientEnterWorld"),
            QuakeLiveFunction::SV_SetConfigstring => f.write_str("SV_SetConfigstring"),
            QuakeLiveFunction::SV_GetConfigstring => f.write_str("SV_GetConfigstring"),
            QuakeLiveFunction::SV_DropClient => f.write_str("SV_DropClient"),
            QuakeLiveFunction::Sys_SetModuleOffset => f.write_str("Sys_SetModuleOffset"),
            QuakeLiveFunction::SV_SpawnServer => f.write_str("SV_SpawnServer"),
            QuakeLiveFunction::Cmd_ExecuteString => f.write_str("Cmd_ExecuteString"),
            QuakeLiveFunction::G_InitGame => f.write_str("G_InitGame"),
            QuakeLiveFunction::G_RunFrame => f.write_str("G_RunFrame"),
            QuakeLiveFunction::ClientConnect => f.write_str("ClientConnect"),
            QuakeLiveFunction::G_StartKamikaze => f.write_str("G_StartKamikaze"),
            QuakeLiveFunction::ClientSpawn => f.write_str("ClientSpawn"),
            QuakeLiveFunction::G_Damage => f.write_str("G_Damage"),
            QuakeLiveFunction::G_AddEvent => f.write_str("G_AddEvent"),
            QuakeLiveFunction::CheckPrivileges => f.write_str("CheckPrivileges"),
            QuakeLiveFunction::Touch_Item => f.write_str("Touch_Item"),
            QuakeLiveFunction::LaunchItem => f.write_str("LaunchItem"),
            QuakeLiveFunction::Drop_Item => f.write_str("Drop_Item"),
            QuakeLiveFunction::G_FreeEntity => f.write_str("G_FreeEntity"),
            QuakeLiveFunction::Cmd_Callvote_f => f.write_str("Cmd_Callvote_f"),
        }
    }
}

impl QuakeLiveFunction {
    pub(crate) fn pattern(&self) -> &[u8] {
        match self {
            QuakeLiveFunction::Com_Printf => b"\x41\x54\x55\x53\x48\x81\xec\x00\x00\x00\x00\x84\xc0\x48\x89\xb4\x24\x00\x00\x00\x00\x48\x89\x94\x24\x00\x00\x00\x00\x48\x89\x8c\x24\x00\x00\x00\x00\x4c\x89\x84\x24\x00\x00\x00\x00",
            QuakeLiveFunction::Cmd_AddCommand => b"\x41\x55\x49\x89\xf5\x41\x54\x49\x89\xfc\x55\x53\x48\x83\xec\x00\x48\x8b\x1d\x00\x00\x00\x00\x48\x85\xdb\x75\x00\xeb\x00\x66\x90\x48\x8b\x1b\x48\x85\xdb\x74\x00\x48\x8b\x73\x00\x4c\x89\xe7",
            QuakeLiveFunction::Cmd_Args => b"\x8b\x05\x00\x00\x00\x00\xc6\x05\x00\x00\x00\x00\x00\x83\xf8\x00\x0f\x8e\x00\x00\x00\x00\x41\x54\x44\x8d\x60\x00\x83\xe8\x00\x55\x48\x8d\x68\x00\x53\x31\xdb\x66\x0f\x1f\x84\x00\x00\x00\x00\x00",
            QuakeLiveFunction::Cmd_Argv => b"\x3b\x3d\x00\x00\x00\x00\xb8\x00\x00\x00\x00\x73\x00\x48\x63\xff\x48\x8b\x04\xfd\x00\x00\x00\x00\xf3\xc3",
            QuakeLiveFunction::Cmd_Argc => b"\x8b\x05\x00\x00\x00\x00\xc3",
            QuakeLiveFunction::Cmd_Tokenizestring => b"\x48\x85\xff\x53\xc7\x05\x00\x00\x44\x00\x00\x00\x00\x00\x48\x89\xfb\x0f\x84\x00\x00\x00\x00\x48\x89\xfe\xba\x00\x00\x00\x00\xbf\x00\x00\x00\x00\xe8\x00\x00\x00\x00\x8b\x0d\x00\x00\x00\x00",
            QuakeLiveFunction::Cbuf_ExecuteText => b"\x83\xff\x00\x74\x00\x83\xff\x00\x74\x00\x85\xff\x74\x00\xbe\x00\x00\x00\x00\x31\xff\x31\xc0\xe9\x00\x00\x00\x00\x0f\x1f\x40\x00\x48\x85\xf6\x74\x00\x80\x3e\x00\x75\x00\xe9\x00\x00\x00\x00\x90",
            QuakeLiveFunction::Cvar_FindVar => b"\x55\x48\x89\xfd\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x48\x8b\x1c\xc5\x00\x00\x00\x00\x48\x85\xdb\x75\x00\xeb\x00\x0f\x1f\x00\x48\x8b\x5b\x00\x48\x85\xdb\x74\x00\x48\x8b\x33\x48\x89\xef",
            QuakeLiveFunction::Cvar_Get => b"\x41\x56\x48\x85\xff\x41\x55\x41\x89\xd5\x41\x54\x49\x89\xf4\x55\x48\x89\xfd\x53\x0f\x84\x00\x00\x00\x00\x48\x85\xf6\x0f\x84\x00\x00\x00\x00\x48\x89\xef\xe8\x00\x00\x00\x00\x85\xc0",
            QuakeLiveFunction::Cvar_GetLimit => b"\x41\x57\x45\x89\xc7\x41\x56\x49\x89\xd6\x41\x55\x49\x89\xcd\x41\x54\x49\x89\xf4\x31\xf6\x55\x48\x89\xfd\x48\x89\xd7\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x66\x0f\x14\xc0\x31\xf6\x4c\x89\xef",
            QuakeLiveFunction::Cvar_Set2 => b"\x41\x57\x31\xc0\x41\x56\x41\x89\xd6\x48\x89\xf2\x41\x55\x41\x54\x49\x89\xf4\x48\x89\xfe\x55\x48\x89\xfd\xbf\x00\x00\x00\x00\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x48\x89\xef\xe8\x00\x00\x00\x00",
            QuakeLiveFunction::SV_SendServerCommand => b"\x41\x55\x41\x54\x55\x48\x89\xfd\x53\x48\x81\xec\x00\x00\x00\x00\x84\xc0\x48\x89\x94\x24\x00\x00\x00\x00\x48\x89\x8c\x24\x00\x00\x00\x00\x4c\x89\x84\x24\x00\x00\x00\x00\x4c\x89\x8c\x24\x00\x00\x00\x00",
            QuakeLiveFunction::SV_ExecuteClientCommand => b"\x41\x55\x41\x89\xd5\x41\x54\x49\x89\xfc\x48\x89\xf7\x55\xbd\x00\x00\x00\x00\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x48\x8b\x1d\x00\x00\x00\x00\x48\x85\xdb\x75\x00\xe9\x00\x00\x00\x00\x66\x90",
            QuakeLiveFunction::SV_Shutdown => b"\x48\x8b\x05\x00\x00\x00\x00\x48\x85\xc0\x74\x00\x44\x8b\x50\x00\x45\x85\xd2\x75\x00\xf3\xc3",
            QuakeLiveFunction::SV_Map_f => b"\x41\x55\xbf\x00\x00\x00\x00\x41\x54\x55\x53\x48\x81\xec\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0\xe8\x00\x00\x00\x00\xbf\x00\x00\x00\x00\x48\x89\xc5",
            QuakeLiveFunction::SV_ClientEnterWorld => b"\x41\x55\x31\xc0\x49\xbd\x00\x00\x00\x00\x00\x00\x00\x00\x41\x54\x49\x89\xf4\x48\x8d\xb7\x00\x00\x00\x00\x55\x53\x48\x89\xfb\xbf\x00\x00\x00\x00\x48\x89\xdd\x48\x83\xec\x00\xe8\x00\x00\x00\x00",
            QuakeLiveFunction::SV_SetConfigstring => b"\x41\x57\x41\x56\x41\x55\x41\x54\x41\x89\xfc\x55\x53\x48\x81\xec\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0\x81\xff\x00\x00\x00\x00\x48\x89\x74\x24\x00",
            QuakeLiveFunction::SV_GetConfigstring => b"\x41\x54\x85\xd2\x49\x89\xf4\x55\x89\xd5\x53\x48\x63\xdf\x7e\x00\x81\xfb\x00\x00\x00\x00\x77\x00\x48\x8b\x34\xdd\x00\x00\x00\x00\x48\x85\xf6\x74\x00\x5b\x89\xea\x4c\x89\xe7\x5d\x41\x5c",
            QuakeLiveFunction::SV_DropClient => b"\x41\x54\x55\x48\x89\xfd\x53\x48\x83\xec\x00\x83\x3f\x00\x0f\x84\x00\x00\x00\x00\x48\x8b\x87\x00\x00\x00\x00\x49\x89\xf4\x48\x85\xc0\x74\x00\xf6\x80\xe0\x01\x00\x00\x00\x75\x00\xbb\x00\x00\x00\x00",
            QuakeLiveFunction::Sys_SetModuleOffset => b"\x55\x48\x89\xf2\x31\xc0\x48\x89\xf5\x48\x89\xfe\x53\x48\x89\xfb\xbf\x00\x00\x00\x00\x48\x83\xec\x00\xe8\x00\x00\x00\x00\xbf\x00\x00\x00\x00\xb9\x00\x00\x00\x00\x48\x89\xde\xf3\xa6\x74\x00",
            QuakeLiveFunction::SV_SpawnServer => b"\x41\x55\x41\x54\x41\x89\xf4\x55\x48\x89\xfd\x53\x48\x81\xec\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0\xe8\x00\x00\x00\x00\x31\xc0\xbf\x00\x00\x00\x00",
            QuakeLiveFunction::Cmd_ExecuteString => b"\x41\x54\x49\x89\xfc\x55\x53\xe8\x00\x00\x00\x00\x44\x8b\x0d\x00\x00\x00\x00\x45\x85\xc9\x0f\x84\x00\x00\x00\x00\x48\x8b\x1d\x00\x00\x00\x00\xbd\x00\x00\x00\x00\x48\x85\xdb\x75\x00\xeb\x00\x90",
            QuakeLiveFunction::G_InitGame => b"\x41\x54\x55\x53\x48\x81\xec\x00\x00\x00\x00\x84\xc0\x48\x89\xb4\x24\x00\x00\x00\x00\x48\x89\x94\x24\x00\x00\x00\x00\x48\x89\x8c\x24\x00\x00\x00\x00\x4c\x89\x84\x24\x00\x00\x00\x00",
            QuakeLiveFunction::G_RunFrame => b"\x8b\x05\x00\x00\x00\x00\x85\xc0\x74\x00\xf3\xc3",
            QuakeLiveFunction::ClientConnect => b"\x41\x57\x4c\x63\xff\x41\x56\x41\x89\xf6\x41\x55\x41\x54\x55\x4c\x89\xfd\x48\xc1\xe5\x00\x53\x89\xfb\x48\x81\xec\x00\x00\x00\x00\x4c\x8b\x2d\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00",
            QuakeLiveFunction::G_StartKamikaze => b"\x41\x55\x31\xc0\x41\x54\x55\x48\x89\xfd\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\x4c\x8b\x25\x00\x00\x00\x00\xc7\x40\x04\x00\x00\x00\x00\x48\x89\xc3\x41\x8b\x44\x00\x24\x89\x83\x00\x00\x00\x00",
            QuakeLiveFunction::ClientSpawn => b"\x41\x57\x41\x56\x49\x89\xfe\x41\x55\x41\x54\x55\x53\x48\x81\xec\x00\x00\x00\x00\x4c\x8b\xbf\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0",
            QuakeLiveFunction::G_Damage => b"\x41\x57\x41\x56\x41\x55\x41\x54\x55\x53\x48\x89\xfb\x48\x81\xec\x00\x00\x00\x00\x44\x8b\x97\x00\x00\x00\x00\x48\x8b\xaf\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00",
            QuakeLiveFunction::G_AddEvent => b"\x85\xf6\x74\x00\x48\x8b\x8f\x00\x00\x00\x00\x48\x85\xc9\x74\x00\x8b\x81\x00\x00\x00\x00\x25\x00\x00\x00\x00\x05\x00\x00\x00\x00\x25\x00\x00\x00\x00\x09\xf0\x89\x81\x00\x00\x00\x00",
            QuakeLiveFunction::CheckPrivileges => b"\x41\x56\x89\x15\x00\x00\x00\x00\x49\x89\xfe\x48\x8d\x3d\x00\x00\x00\x00\x41\x55\x41\x89\xd5\x41\x54\x49\x89\xf4\x55\x31\xed\x53\x48\x8d\x1d\x00\x00\x00\x00\xeb\x00\x0f\x1f\x80\x00\x00\x00\x00",
            QuakeLiveFunction::Touch_Item => b"\x41\x57\x41\x56\x41\x55\x41\x54\x55\x53\x48\x89\xf3\x48\x81\xec\x00\x00\x00\x00\x4c\x8b\x86\x00\x00\x00\x00\x4d\x85\xc0\x74\x00\x8b\x96\x00\x00\x00\x00\x85\xd2\x7e\x00\x4c\x8b\x35\x00\x00\x00\x00",
            QuakeLiveFunction::LaunchItem => b"\x41\x55\x31\xc0\x49\x89\xf5\x41\x54\x49\x89\xd4\x55\x48\x89\xfd\x53\x48\x83\xec\x00\xe8\x00\x00\x00\x00\xc7\x40\x04\x00\x00\x00\x00\x48\x89\xc3\x48\x89\xe8\x48\x2b\x05\x00\x00\x00\x00",
            QuakeLiveFunction::Drop_Item => b"\x41\x54\x31\xc9\x31\xd2\x49\x89\xf4\x55\x53\x48\x89\xfb\x48\x83\xec\x00\xf3\x0f\x10\x4f\x00\x48\x8d\x6c\x24\x00\xc7\x44\x24\x20\x00\x00\x00\x00\xf3\x0f\x58\xc8\xf3\x0f\x10\x57\x00\x48\x8d\x7c\x24\x00",
            QuakeLiveFunction::G_FreeEntity => b"\x48\x8b\x05\x00\x00\x00\x00\x53\x48\x89\xfb\x48\x8b\x00\xff\x90\x00\x00\x00\x00\x8b\x83\x00\x00\x00\x00\x85\xc0\x74\x00\x5b\xc3",
            QuakeLiveFunction::Cmd_Callvote_f => b"\x41\x57\x41\x56\x41\x55\x41\x54\x55\x48\x89\xfd\x53\x48\x81\xec\x00\x00\x00\x00\x64\x48\x8b\x04\x25\x00\x00\x00\x00\x48\x89\x84\x24\x00\x00\x00\x00\x31\xc0\xe8\x00\x00\x00\x00",
        }
    }

    pub(crate) fn mask(&self) -> &[u8] {
        match self {
            QuakeLiveFunction::Com_Printf => b"XXXXXXX----XXXXXX----XXXX----XXXX----XXXX----",
            QuakeLiveFunction::Cmd_AddCommand => b"XXXXXXXXXXXXXXX-XXX----XXXX-X-XXXXXXXXX-XXX-XXX",
            QuakeLiveFunction::Cmd_Args => b"XX----XX----XXX-XX----XXXXX-XX-XXXX-XXXXXXX----X",
            QuakeLiveFunction::Cmd_Argv => b"XX----X----X-XXXXXXX----XX",
            QuakeLiveFunction::Cmd_Argc => b"XX----X",
            QuakeLiveFunction::Cmd_Tokenizestring => {
                b"XXXXXX--X----XXXXXX----XXXX----X----X----XX----"
            }
            QuakeLiveFunction::Cbuf_ExecuteText => {
                b"XX-X-XX-X-XXX-X----XXXXX----XXX-XXXX-XX-X-X----X"
            }
            QuakeLiveFunction::Cvar_FindVar => b"XXXXXXXX-X----XXXX----XXXX-X-XXXXXX-XXXX-XXXXXX",
            QuakeLiveFunction::Cvar_Get => b"XXXXXXXXXXXXXXXXXXXXXX----XXXXX----XXXX----XX",
            QuakeLiveFunction::Cvar_GetLimit => b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX-X----XXXXXXXXX",
            QuakeLiveFunction::Cvar_Set2 => b"XXXXXXXXXXXXXXXXXXXXXXXXXXX----XXXX-X----XXXX----",
            QuakeLiveFunction::SV_SendServerCommand => {
                b"XXXXXXXXXXXX----XXXXXX----XXXX----XXXX----XXXX----"
            }
            QuakeLiveFunction::SV_ExecuteClientCommand => {
                b"XXXXXXXXXXXXXXX----XXXX-X----XXX----XXXX-X----XX"
            }
            QuakeLiveFunction::SV_Shutdown => b"XXX----XXXX-XXX-XXXX-XX",
            QuakeLiveFunction::SV_Map_f => b"XXX----XXXXXXX----XXXXX----XXXX----XXX----X----XXX",
            QuakeLiveFunction::SV_ClientEnterWorld => {
                b"XXXXXX--------XXXXXXXX----XXXXXX----XXXXXX-X----"
            }
            QuakeLiveFunction::SV_SetConfigstring => {
                b"XXXXXXXXXXXXXXXX----XXXXX----XXXX----XXXX----XXXX-"
            }
            QuakeLiveFunction::SV_GetConfigstring => {
                b"XXXXXXXXXXXXXXX-XX----X-XXXX----XXXX-XXXXXXXXX"
            }
            QuakeLiveFunction::SV_DropClient => {
                b"XXXXXXXXXX-XX-XX----XXX----XXXXXXX-XXXXXX-X-X----"
            }
            QuakeLiveFunction::Sys_SetModuleOffset => {
                b"XXXXXXXXXXXXXXXXX----XXX-X----X----X----XXXXXX-"
            }
            QuakeLiveFunction::SV_SpawnServer => {
                b"XXXXXXXXXXXXXXX----XXXXX----XXXX----XXX----XXX----"
            }
            QuakeLiveFunction::Cmd_ExecuteString => {
                b"XXXXXXXX----XXX----XXXXX----XXX----X----XXXX-X-X"
            }
            QuakeLiveFunction::G_InitGame => b"XXXXXXX----XXXXXX----XXXX----XXXX----XXXX----",
            QuakeLiveFunction::G_RunFrame => b"XX----XXX-XX",
            QuakeLiveFunction::ClientConnect => b"XXXXXXXXXXXXXXXXXXXXX-XXXXXX----XXX----XXXXX----",
            QuakeLiveFunction::G_StartKamikaze => {
                b"XXXXXXXXXXXXXX-X----XXX----XXX----XXXXXX-XXX----"
            }
            QuakeLiveFunction::ClientSpawn => b"XXXXXXXXXXXXXXXX----XXX----XXXXX----XXXX----XX",
            QuakeLiveFunction::G_Damage => b"XXXXXXXXXXXXXXXX----XXX----XXX----XXXXX----",
            QuakeLiveFunction::G_AddEvent => b"XXX-XXX----XXXX-XX----X----X----X----XXXX----",
            QuakeLiveFunction::CheckPrivileges => {
                b"XXXX----XXXXXX----XXXXXXXXXXXXXXXXX----X-XXX----"
            }
            QuakeLiveFunction::Touch_Item => b"XXXXXXXXXXXXXXXX----XXX----XXXX-XX----XXX-XXX----",
            QuakeLiveFunction::LaunchItem => b"XXXXXXXXXXXXXXXXXXXX-X----XXX----XXXXXXXXX----",
            QuakeLiveFunction::Drop_Item => b"XXXXXXXXXXXXXXXXX-XXXX-XXXX-XXXX----XXXXXXXX-XXXX-",
            QuakeLiveFunction::G_FreeEntity => b"XXX----XXXXXXXXX----XX----XXX-XX",
            QuakeLiveFunction::Cmd_Callvote_f => b"XXXXXXXXXXXXXXXX----XXXXX----XXXX----XXX----",
        }
    }
}
