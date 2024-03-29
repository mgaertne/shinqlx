use crate::prelude::*;

use core::{
    borrow::Borrow,
    fmt::{Display, Formatter},
};
use retour::{Function, GenericDetour, HookableWith};

#[cfg(target_os = "linux")]
pub(crate) fn pattern_search_module<T>(
    module_info: &[&procfs::process::MemoryMap],
    ql_func: T,
) -> Option<usize>
where
    T: Borrow<QuakeLiveFunction>,
{
    module_info
        .iter()
        .filter(|memory_map| {
            memory_map
                .perms
                .contains(procfs::process::MMPermissions::READ)
        })
        .filter_map(|memory_map| {
            pattern_search(
                memory_map.address.0 as usize,
                memory_map.address.1 as usize,
                ql_func.borrow(),
            )
        })
        .take(1)
        .next()
}

#[allow(dead_code)]
fn pattern_search<T>(start: usize, end: usize, ql_func: T) -> Option<usize>
where
    T: Borrow<QuakeLiveFunction>,
{
    let pattern = ql_func.borrow().pattern();
    let mask = ql_func.borrow().mask();
    (start..end)
        .filter(|i| {
            (0..pattern.len())
                .filter(|j| mask[*j] == b'X')
                .all(|j| pattern[j] == unsafe { ptr::read((*i + j) as *const u8) })
        })
        .take(1)
        .next()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[allow(dead_code)]
#[allow(non_camel_case_types)]
pub enum QuakeLiveFunction {
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
    G_ShutdownGame,
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
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
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
            QuakeLiveFunction::G_ShutdownGame => f.write_str("G_ShutdownGame"),
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
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn create_and_enable_generic_detour<T, D>(
        &self,
        function: T,
        replacement: D,
    ) -> Result<GenericDetour<T>, QuakeLiveEngineError>
    where
        T: HookableWith<D>,
        D: Function,
    {
        let detour = unsafe {
            GenericDetour::new(function, replacement)
                .map_err(|_| QuakeLiveEngineError::DetourCouldNotBeCreated(*self))?
        };
        unsafe {
            detour
                .enable()
                .map_err(|_| QuakeLiveEngineError::DetourCouldNotBeEnabled(*self))?
        };

        Ok(detour)
    }

    pub(crate) fn pattern(&self) -> &[u8] {
        #[cfg(target_pointer_width = "64")]
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
            QuakeLiveFunction::G_ShutdownGame => b"",
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
        #[cfg(target_pointer_width = "32")]
        match self {
            QuakeLiveFunction::Com_Printf => b"\x57\xba\x00\x00\x00\x00\x56\xb9\x00\x00\x00\x00\x53\x81\xec\x00\x00\x00\x00\x65\xa1\x00\x00\x00\x00\x89\x84\x24\x00\x00\x00\x00\x31\xc0\x8d\x84\x24\x00\x00\x00\x00\x89\x44\x24\x00",
            QuakeLiveFunction::Cmd_AddCommand => b"\x56\x53\x83\xec\x00\x8b\x1d\x00\x00\x00\x00\x8b\x74\x24\x00\x85\xdb\x75\x00\xeb\x00\x8d\x76\x00\x8b\x1b\x85\xdb\x74\x00\x8b\x43\x00\x89\x34\x24\x89\x44\x24\x00\xe8\x00\x00\x00\x00\x85\xc0\x75\x00",
            QuakeLiveFunction::Cmd_Args => b"\x57\x56\x53\x83\xec\x00\x8b\x3d\x00\x00\x00\x00\xc6\x05\x60\xaa\x2a\x08\x00\x83\xff\x00\x7e\x00\x8d\x5f\x00\xbe\x00\x00\x00\x00\xba\x00\x00\x00\x00\x8b\x0a\x83\xc2\x00\x8d\x81\x00\x00\x00\x00\xf7\xd1",
            QuakeLiveFunction::Cmd_Argv => b"\x8b\x54\x24\x00\xb8\x00\x00\x00\x00\x3b\x15\x00\x00\x00\x00\x73\x00\x8b\x04\x95\x00\x00\x00\x00\xc3",
            QuakeLiveFunction::Cmd_Argc => b"\xa1\x00\x00\x00\x00\xc3",
            QuakeLiveFunction::Cmd_Tokenizestring => b"\x57\x31\xc0\x56\x53\x83\xec\x00\x8b\x5c\x24\x00\xa3\x00\x00\x00\x00\x85\xdb\x0f\x84\x00\x00\x00\x00\xb8\x00\x00\x00\x00\xbe\x00\x00\x00\x00\x89\x44\x24\x00\x89\x5c\x24\x00\xc7\x04\x24\x00\x00\x00\x00",
            QuakeLiveFunction::Cbuf_ExecuteText => b"\x8b\x44\x24\x00\x8b\x54\x24\x00\x83\xf8\x00\x74\x00\x83\xf8\x00\x74\x00\x85\xc0\x74\x00\xb8\x00\x00\x00\x00\x89\x44\x24\x00\x31\xc0\x89\x44\x24\x00\xe9\x00\x00\x00\x00\x8d\xb6\x00\x00\x00\x00\x85\xd2",
            QuakeLiveFunction::Cvar_FindVar => b"\x56\x53\x83\xec\x00\x8b\x74\x24\x00\x89\xf0\xe8\x00\x00\x00\x00\x8b\x1c\x85\x00\x00\x00\x00\x85\xdb\x75\x00\xeb\x00\x8d\x76\x00\x8b\x5b\x00\x85\xdb\x74\x00\x8b\x13\x89\x34\x24\x89\x54\x24\x00",
            QuakeLiveFunction::Cvar_Get => b"\x83\xec\x00\x89\x74\x24\x00\x8b\x74\x24\x00\x89\x7c\x24\x00\x8b\x7c\x24\x00\x89\x5c\x24\x00\x89\x6c\x24\x00\x85\xf6\x0f\x84\x00\x00\x00\x00\x85\xff\x0f\x84\x00\x00\x00\x00\x89\xf0\xe8\x00\x00\x00\x00",
            QuakeLiveFunction::Cvar_GetLimit => b"\x55\x31\xc0\x57\x56\x53\x83\xec\x00\x89\x44\x24\x00\x8b\x44\x24\x00\x8b\x5c\x24\x00\x8b\x7c\x24\x00\x8b\x74\x24\x00\x89\x04\x24\xe8\x00\x00\x00\x00\x31\xc0\x89\x44\x24\x00\x89\x3c\x24\xd9\x5c\x24\x00",
            QuakeLiveFunction::Cvar_Set2 => b"\x83\xec\x00\x89\x74\x24\x00\x8b\x74\x24\x00\x89\x7c\x24\x00\x8b\x7c\x24\x00\xc7\x04\x24\x00\x00\x00\x00\x89\x6c\x24\x00\x8b\x6c\x24\x00\x89\x74\x24\x00\x89\x7c\x24\x00\x89\x5c\x24\x00",
            QuakeLiveFunction::SV_SendServerCommand => b"\x81\xec\x00\x00\x00\x00\x65\xa1\x00\x00\x00\x00\x89\x84\x24\x00\x00\x00\x00\x31\xc0\x8d\x84\x24\x00\x00\x00\x00\x89\x44\x24\x00\x8b\x84\x24\x00\x00\x00\x00\x89\xac\x24\x00\x00\x00\x00\x8d\x6c\x24\x00",
            QuakeLiveFunction::SV_ExecuteClientCommand => b"\x55\x57\x56\xbe\x00\x00\x00\x00\x53\x83\xec\x00\x8b\x44\x24\x00\x8b\x7c\x24\x00\x8b\x6c\x24\x00\x89\x04\x24\xe8\x00\x00\x00\x00\x8b\x1d\x00\x00\x00\x00\x85\xdb\x75\x00\xeb\x00\x8d\x74\x26\x00",
            QuakeLiveFunction::SV_Shutdown => b"\x53\x83\xec\x00\xa1\x00\x00\x00\x00\x8b\x5c\x24\x00\x85\xc0\x74\x00\x8b\x40\x00\x85\xc0\x75\x00\x83\xc4\x00\x5b\xc3",
            QuakeLiveFunction::SV_Map_f => b"\x55\x57\x56\x53\x81\xec\x00\x00\x00\x00\xc7\x04\x24\x00\x00\x00\x00\x8d\xac\x24\x00\x00\x00\x00\x65\xa1\x00\x00\x00\x00\x89\x84\x24\x00\x00\x00\x00\x31\xc0\x8d\x7c\x24\x00\xe8\x00\x00\x00\x00",
            QuakeLiveFunction::SV_ClientEnterWorld => b"\x83\xec\x00\x89\x5c\x24\x00\x8b\x5c\x24\x00\xc7\x04\x24\x00\x00\x00\x00\x89\x74\x24\x00\x89\x7c\x24\x00\x8b\x7c\x24\x00\x8d\x83\x00\x00\x00\x00\x89\x44\x24\x00\xe8\x00\x00\x00\x00\x89\xd8",
            QuakeLiveFunction::SV_SetConfigstring => b"\x55\x57\x56\x53\x81\xec\x00\x00\x00\x00\x65\xa1\x00\x00\x00\x00\x89\x84\x24\x00\x00\x00\x00\x31\xc0\x8b\xac\x24\x00\x00\x00\x00\x81\xbc\x24\x50\x04\x00\x00\x00\x00\x00\x00\x0f\x87\x00\x00\x00\x00",
            QuakeLiveFunction::SV_GetConfigstring => b"\x83\xec\x00\x89\x74\x24\x00\x8b\x74\x24\x00\x89\x5c\x24\x00\x8b\x5c\x24\x00\x89\x7c\x24\x00\x8b\x7c\x00\x24\x85\xf6\x7e\x00\x81\xfb\x00\x00\x00\x00\x77\x00\x8b\x04\x9d\x00\x00\x00\x00\x85\xc0\x74\x00",
            QuakeLiveFunction::SV_DropClient => b"\x8d\xb4\x26\x00\x00\x00\x00\x57\x56\x53\x83\xec\x00\x8b\x5c\x24\x00\x8b\x7c\x24\x00\x83\x3b\x00\x0f\x84\x00\x00\x00\x00\x8b\x83\x00\x00\x00\x00\x85\xc0\x74\x00\xf6\x80\xe0\x01\x00\x00\x00\x75\x00",
            QuakeLiveFunction::Sys_SetModuleOffset => b"\x83\xec\x00\x89\x5c\x24\x00\x8b\x5c\x24\x00\x89\x6c\x24\x00\x8b\x6c\x24\x00\xc7\x04\x24\x00\x00\x00\x00\x89\x74\x24\x00\x89\x5c\x24\x00\x89\xde\x89\x6c\x24\x00\x89\x7c\x24\x00\xbf\x00\x00\x00\x00",
            QuakeLiveFunction::SV_SpawnServer => b"\x55\x57\x56\x53\x81\xec\x00\x00\x00\x00\x8b\xbc\x24\x00\x00\x00\x00\x65\xa1\x00\x00\x00\x00\x89\x84\x24\x00\x00\x00\x00\x31\xc0\x8b\xb4\x24\x00\x00\x00\x00\xe8\x00\x00\x00\x00",
            QuakeLiveFunction::Cmd_ExecuteString => b"\x57\x56\x53\x83\xec\x00\x8b\x7c\x24\x00\x89\x3c\x24\xe8\x00\x00\x00\x00\xa1\x00\x00\x00\x00\x85\xc0\x0f\x84\x00\x00\x00\x00\x8b\x1d\x00\x00\x00\x00\xbe\x00\x00\x00\x00\x85\xdb\x75\x00\xeb\x00\x89\xde",
            QuakeLiveFunction::G_InitGame => b"\x81\xec\x00\x00\x00\x00\xb9\x00\x00\x00\x00\x89\x9c\x24\x00\x00\x00\x00\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x89\xb4\x24\x00\x00\x00\x00\x65\xa1\x00\x00\x00\x00\x89\x84\x24\x00\x00\x00\x00",
            QuakeLiveFunction::G_ShutdownGame => b"",
            QuakeLiveFunction::G_RunFrame => b"\x55\x57\x56\x53\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x83\xec\x00\x8b\xbb\x00\x00\x00\x00\x85\xff\x74\x00\x83\xc4\x00\x5b\x5e\x5f\x5d\xc3",
            QuakeLiveFunction::ClientConnect => b"\x55\xb9\x00\x00\x00\x00\x57\x56\x53\x81\xec\x00\x00\x00\x00\x8b\xac\x24\x00\x00\x00\x00\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x65\xa1\x00\x00\x00\x00\x89\x84\x24\x00\x00\x00\x00\x31\xc0",
            QuakeLiveFunction::G_StartKamikaze => b"\x83\xec\x00\x89\x5c\x24\x00\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x89\x74\x24\x00\x8b\x74\x24\x00\x89\x7c\x24\x00\x89\x6c\x24\x00\xe8\x00\x00\x00\x00\x8b\xbb\x00\x00\x00\x00",
            QuakeLiveFunction::ClientSpawn => b"\x55\x57\x56\x53\x81\xec\x00\x00\x00\x00\x8b\xac\x24\x00\x00\x00\x00\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x65\xa1\x00\x00\x00\x00\x89\x84\x24\x00\x00\x00\x00\x31\xc0\x8b\xb5\x00\x00\x00\x00",
            QuakeLiveFunction::G_Damage => b"\x81\xec\x00\x00\x00\x00\x89\xb4\x24\x00\x00\x00\x00\x8b\xb4\x24\x00\x00\x00\x00\x89\x9c\x24\x00\x00\x00\x00\x8b\x84\x24\x00\x00\x00\x00\x89\xac\x24\x00\x00\x00\x00\x8b\x8c\x24\x00\x00\x00\x00",
            QuakeLiveFunction::G_AddEvent => b"\x83\xec\x00\x89\x74\x24\x00\x8b\x74\x24\x00\x89\x5c\x24\x00\x8b\x54\x24\x00\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x85\xf6\x74\x00\x8b\x8a\x00\x00\x00\x00\x85\xc9\x74\x00\x8b\x81\x00\x00\x00\x00",
            QuakeLiveFunction::CheckPrivileges => b"\x55\x31\xed\x57\x56\x53\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x83\xec\x00\x8b\x7c\x24\x00\x8b\x74\x24\x00\x89\xbb\x00\x00\x00\x00\x8d\x83\x00\x00\x00\x00\xeb\x00\x8d\xb6\x00\x00\x00\x00\x45",
            QuakeLiveFunction::Touch_Item => b"\x81\xec\x00\x00\x00\x00\x89\xbc\x24\x00\x00\x00\x00\x8b\xbc\x24\x00\x00\x00\x00\x89\x9c\x24\x00\x00\x00\x00\x89\xb4\x24\x00\x00\x00\x00\x89\xac\x24\x00\x00\x00\x00\x8b\x87\x00\x00\x00\x00",
            QuakeLiveFunction::LaunchItem => b"\x55\x57\x56\x53\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x83\xec\x00\x8b\x7c\x24\x00\x8b\x6c\x24\x00\xe8\x00\x00\x00\x00\x8b\x93\x00\x00\x00\x00\xb9\x00\x00\x00\x00\xc7\x40\x04\x00\x00\x00\x00",
            QuakeLiveFunction::Drop_Item => b"\x83\xec\x00\x31\xc0\x89\x74\x24\x00\x8b\x74\x24\x00\x89\x5c\x24\x00\x89\x7c\x24\x00\x8b\x7c\x24\x00\x89\x6c\x24\x00\x8d\x6c\x24\x00\xd9\x46\x00\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00",
            QuakeLiveFunction::G_FreeEntity => b"\x83\xec\x00\x89\x5c\x24\x00\xe8\x00\x00\x00\x00\x81\xc3\x00\x00\x00\x00\x89\x74\x24\x00\x8b\x74\x24\x00\x89\x7c\x24\x00\x8b\x83\x00\x00\x00\x00\x8b\x00\x89\x34\x24\xff\x90\x00\x00\x00\x00",
            QuakeLiveFunction::Cmd_Callvote_f => b"\x69\xc8\xd0\x0b\x00\x00\x01\xca\x90\x00\x44\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x6c\x90\x90\x90\x90\x90\x90\x90\x90",
        }
    }

    pub(crate) fn mask(&self) -> &[u8] {
        #[cfg(target_pointer_width = "64")]
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
            QuakeLiveFunction::G_ShutdownGame => b"",
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
        #[cfg(target_pointer_width = "32")]
        match self {
            QuakeLiveFunction::Com_Printf => b"XX----XX----XXX----XX----XXX----XXXXX----XXX-",
            QuakeLiveFunction::Cmd_AddCommand => {
                b"XXXX-XX----XXX-XXX-X-XX-XXXXX-XX-XXXXXX-X----XXX-"
            }
            QuakeLiveFunction::Cmd_Args => b"XXXXX-XX----XXXXXX-XX-X-XX-X----X----XXXX-XX----XX",
            QuakeLiveFunction::Cmd_Argv => b"XXX-X----XX----X-XXX----X",
            QuakeLiveFunction::Cmd_Argc => b"X----X",
            QuakeLiveFunction::Cmd_Tokenizestring => {
                b"XXXXXXX-XXX-X----XXXX----X----X----XXX-XXX-XXX----"
            }
            QuakeLiveFunction::Cbuf_ExecuteText => {
                b"XXX-XXX-XX-X-XX-X-XXX-X----XXX-XXXXX-X----XX----XX"
            }
            QuakeLiveFunction::Cvar_FindVar => {
                b"XXX-XXX-XX-X-XX-X-XXX-X----XXX-XXXXX-X----XX----XX"
            }
            QuakeLiveFunction::Cvar_Get => b"XX-XXX-XXX-XXX-XXX-XXX-XXX-XXXX----XXXX----XXX----",
            QuakeLiveFunction::Cvar_GetLimit => {
                b"XXXXXXXX-XXX-XXX-XXX-XXX-XXX-XXXX----XXXXX-XXXXXX-"
            }
            QuakeLiveFunction::Cvar_Set2 => b"XX-XXX-XXX-XXX-XXX-XXX----XXX-XXX-XXX-XXX-XXX-",
            QuakeLiveFunction::SV_SendServerCommand => {
                b"XX----XX----XXX----XXXXX----XXX-XXX----XXX----XXX-"
            }
            QuakeLiveFunction::SV_ExecuteClientCommand => {
                b"XXXX----XXX-XXX-XXX-XXX-XXXX----XX----XXX-X-XXX-"
            }
            QuakeLiveFunction::SV_Shutdown => b"XXX-X----XXX-XXX-XX-XXX-XX-XX",
            QuakeLiveFunction::SV_Map_f => b"XXXXXX----XXX----XXX----XX----XXX----XXXXX-X----",
            QuakeLiveFunction::SV_ClientEnterWorld => {
                b"XX-XXX-XXX-XXX----XXX-XXX-XXX-XX----XXX-X----XX"
            }
            QuakeLiveFunction::SV_SetConfigstring => {
                b"XX-XXX-XXX-XXX----XXX-XXX-XXX-XX----XXX-X----XX"
            }
            QuakeLiveFunction::SV_GetConfigstring => {
                b"XX-XXX-XXX-XXX-XXX-XXX-XX-XXXX-XX----X-XXX----XXX-"
            }
            QuakeLiveFunction::SV_DropClient => {
                b"XXX----XXXXX-XXX-XXX-XX-XX----XX----XXX-XXXXXX-X-"
            }
            QuakeLiveFunction::Sys_SetModuleOffset => {
                b"XX-XXX-XXX-XXX-XXX-XXX----XXX-XXX-XXXXX-XXX-X----"
            }
            QuakeLiveFunction::SV_SpawnServer => b"XXXXXX----XXX----XX----XXX----XXXXX----X----",
            QuakeLiveFunction::Cmd_ExecuteString => {
                b"XXXXX-XXX-XXXX----X----XXXX----XX----X----XXX-X-XX"
            }
            QuakeLiveFunction::G_InitGame => b"XX----X----XXX----X----XX----XXX----XX----XXX----",
            QuakeLiveFunction::G_ShutdownGame => b"",
            QuakeLiveFunction::G_RunFrame => b"XXXXX----XX----XX-XX----XXX-XX-XXXXX",
            QuakeLiveFunction::ClientConnect => b"XX----XXXXX----XXX----X----XX----XX----XXX----XX",
            QuakeLiveFunction::G_StartKamikaze => b"XX-XXX-X----XX----XXX-XXX-XXX-XXX-X----XX----",
            QuakeLiveFunction::ClientSpawn => b"XXXXXX----XXX----X----XX----XX----XXX----XXXX----",
            QuakeLiveFunction::G_Damage => b"XX----XXX----XXX----XXX----XXX----XXX----XXX----",
            QuakeLiveFunction::G_AddEvent => b"XX-XXX-XXX-XXX-XXX-X----XX----XXX-XX----XXX-XX----",
            QuakeLiveFunction::CheckPrivileges => {
                b"XXXXXXX----XX----XX-XXX-XXX-XX----XX----X-XX----X"
            }
            QuakeLiveFunction::Touch_Item => b"XX----XXX----XXX----XXX----XXX----XXX----XX----",
            QuakeLiveFunction::LaunchItem => b"XXXXX----XX----XX-XXX-XXX-X----XX----X----XXX----",
            QuakeLiveFunction::Drop_Item => b"XX-XXXXX-XXX-XXX-XXX-XXX-XXX-XXX-XX-X----XX----",
            QuakeLiveFunction::G_FreeEntity => b"XX-XXX-X----XX----XXX-XXX-XXX-XX----XXXXXXX----",
            QuakeLiveFunction::Cmd_Callvote_f => b"XXXXXXXXX-X------------XXXXXXXXX",
        }
    }
}
