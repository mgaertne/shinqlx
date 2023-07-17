use crate::client::Client;
use crate::commands::{
    cmd_center_print, cmd_py_command, cmd_py_rcon, cmd_regular_print, cmd_restart_python,
    cmd_send_server_command, cmd_slap, cmd_slay,
};
use crate::cvar::CVar;
use crate::game_entity::GameEntity;
use crate::game_item::GameItem;
use crate::hooks::{
    shinqlx_cmd_addcommand, shinqlx_sv_cliententerworld, shinqlx_sv_dropclient,
    shinqlx_sv_executeclientcommand, shinqlx_sv_setconfigstring, shinqlx_sv_spawnserver,
    shinqlx_sys_setmoduleoffset, ShiNQlx_ClientConnect, ShiNQlx_ClientSpawn, ShiNQlx_Com_Printf,
    ShiNQlx_G_Damage, ShiNQlx_G_InitGame, ShiNQlx_G_RunFrame, ShiNQlx_G_ShutdownGame,
    ShiNQlx_G_StartKamikaze, ShiNQlx_SV_SendServerCommand,
};
use crate::patches::patch_callvote_f;
use crate::pyminqlx::pyminqlx_initialize;
#[cfg(target_os = "linux")]
use crate::quake_live_functions::pattern_search_module;
use crate::quake_live_functions::QuakeLiveFunction;
use crate::quake_types::{
    cbufExec_t, client_t, cvar_t, entity_event_t, gentity_t, gitem_t, qboolean, trace_t, usercmd_t,
    vec3_t, MAX_STRING_CHARS,
};
use crate::PyMinqlx_InitStatus_t::PYM_SUCCESS;
use crate::SV_TAGS_PREFIX;
#[cfg(target_os = "linux")]
use crate::{QAGAME, QZERODED};
#[cfg(test)]
use mockall::*;
use once_cell::race::OnceBool;
use once_cell::sync::OnceCell;
#[cfg(target_os = "linux")]
use procfs::process::{MMapPath, MemoryMap, Process};
use retour::{GenericDetour, RawDetour};
use std::collections::VecDeque;
#[cfg(target_os = "linux")]
use std::ffi::OsStr;
use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::RwLock;

fn try_disable<T: retour::Function>(detour: &GenericDetour<T>) {
    if detour.is_enabled() {
        if let Err(e) = unsafe { detour.disable() } {
            debug_println!(format!("error when disabling detour: {}", e));
        }
    }
}

fn extract_detour<T: retour::Function>(
    lock: &RwLock<Option<GenericDetour<T>>>,
) -> Option<GenericDetour<T>> {
    if lock.is_poisoned() {
        return None;
    }

    let Ok(mut lock_guard) = lock.write() else {
        return None;
    };

    let Some(detour) = (*lock_guard).take() else {
        return None;
    };
    Some(detour)
}

#[derive(Debug, PartialEq, Eq)]
pub enum QuakeLiveEngineError {
    NullPointerPassed(String),
    EntityNotFound(String),
    InvalidId(i32),
    ClientNotFound(String),
    ProcessNotFound(String),
    #[allow(dead_code)]
    NoMemoryMappingInformationFound(String),
    StaticFunctionNotFound(QuakeLiveFunction),
    StaticDetourCouldNotBeCreated(QuakeLiveFunction),
    StaticDetourCouldNotBeEnabled(QuakeLiveFunction),
    StaticDetourNotFound(QuakeLiveFunction),
    VmFunctionNotFound(QuakeLiveFunction),
    VmDetourCouldNotBeCreated(QuakeLiveFunction),
    VmDetourCouldNotBeEnabled(QuakeLiveFunction),
    VmDetourPoisoned(QuakeLiveFunction),
}

#[derive(Debug)]
struct StaticFunctions {
    com_printf_orig: extern "C" fn(*const c_char, ...),
    cmd_addcommand_orig: fn(*const c_char, unsafe extern "C" fn()),
    cmd_args_orig: fn() -> *const c_char,
    cmd_argv_orig: fn(c_int) -> *const c_char,
    #[allow(dead_code)]
    cmd_tokenizestring_orig: fn(*const c_char),
    #[allow(dead_code)]
    cbuf_executetext_orig: fn(cbufExec_t, *const c_char),
    cvar_findvar_orig: fn(*const c_char) -> *mut cvar_t,
    cvar_get_orig: fn(*const c_char, *const c_char, c_int) -> *mut cvar_t,
    cvar_getlimit_orig:
        fn(*const c_char, *const c_char, *const c_char, *const c_char, c_int) -> *mut cvar_t,
    cvar_set2_orig: fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t,
    sv_sendservercommand_orig: extern "C" fn(*mut client_t, *const c_char, ...),
    sv_executeclientcommand_orig: fn(*mut client_t, *const c_char, qboolean),
    sv_shutdown_orig: fn(*const c_char),
    sv_map_f_orig: fn(),
    sv_cliententerworld_orig: fn(*mut client_t, *mut usercmd_t),
    sv_setconfigstring_orig: fn(c_int, *const c_char),
    sv_getconfigstring_orig: fn(c_int, *const c_char, c_int),
    sv_dropclient_orig: fn(*mut client_t, *const c_char),
    sys_setmoduleoffset_orig: fn(*const c_char, unsafe extern "C" fn()),
    sv_spawnserver_orig: fn(*const c_char, qboolean),
    cmd_executestring_orig: fn(*const c_char),
    cmd_argc_orig: fn() -> c_int,
}

#[derive(Debug)]
struct StaticDetours {
    cmd_addcommand_detour: GenericDetour<fn(*const c_char, unsafe extern "C" fn())>,
    sys_setmoduleoffset_detour: GenericDetour<fn(*const c_char, unsafe extern "C" fn())>,
    sv_executeclientcommand_detour: GenericDetour<fn(*mut client_t, *const c_char, qboolean)>,
    sv_cliententerworld_detour: GenericDetour<fn(*mut client_t, *mut usercmd_t)>,
    sv_setconfgistring_detour: GenericDetour<fn(c_int, *const c_char)>,
    sv_dropclient_detour: GenericDetour<fn(*mut client_t, *const c_char)>,
    sv_spawnserver_detour: GenericDetour<fn(*const c_char, qboolean)>,
    sv_sendservercommand_detour: RawDetour,
    com_printf_detour: RawDetour,
}

type ClientSpawnDetourType = GenericDetour<extern "C" fn(*mut gentity_t)>;
type ClientConnectDetourType =
    GenericDetour<extern "C" fn(c_int, qboolean, qboolean) -> *const c_char>;
type GStartKamikazeDetourType = GenericDetour<extern "C" fn(*mut gentity_t)>;
type GDamageDetourType = GenericDetour<
    extern "C" fn(
        *mut gentity_t,
        *mut gentity_t,
        *mut gentity_t,
        *mut vec3_t,
        *mut vec3_t,
        c_int,
        c_int,
        c_int,
    ),
>;
type VmHooksResultType = (
    Option<ClientConnectDetourType>,
    Option<GStartKamikazeDetourType>,
    Option<ClientSpawnDetourType>,
    Option<GDamageDetourType>,
);

#[derive(Debug)]
struct VmFunctions {
    vm_call_table: AtomicUsize,

    g_addevent_orig: AtomicUsize,
    #[allow(dead_code)]
    check_privileges_orig: AtomicUsize,
    client_connect_orig: AtomicUsize,
    client_spawn_orig: AtomicUsize,
    g_damage_orig: AtomicUsize,
    touch_item_orig: AtomicUsize,
    launch_item_orig: AtomicUsize,
    #[allow(dead_code)]
    drop_item_orig: AtomicUsize,
    g_start_kamikaze_orig: AtomicUsize,
    g_free_entity_orig: AtomicUsize,
    g_init_game_orig: AtomicUsize,
    g_shutdown_game_orig: AtomicUsize,
    g_run_frame_orig: AtomicUsize,
    cmd_callvote_f_orig: AtomicUsize,

    client_spawn_detour: RwLock<Option<ClientSpawnDetourType>>,
    client_connect_detour: RwLock<Option<ClientConnectDetourType>>,
    g_start_kamikaze_detour: RwLock<Option<GStartKamikazeDetourType>>,
    g_damage_detour: RwLock<Option<GDamageDetourType>>,
}

#[allow(dead_code)]
const OFFSET_VM_CALL_TABLE: usize = 0x3;
#[allow(dead_code)]
const OFFSET_INITGAME: usize = 0x18;
#[allow(dead_code)]
const OFFSET_RUNFRAME: usize = 0x8;

impl VmFunctions {
    pub(crate) fn try_initialize_from(
        &self,
        #[allow(unused_variables)] module_offset: usize,
    ) -> Result<(), QuakeLiveEngineError> {
        #[cfg(not(target_os = "linux"))]
        return Err(QuakeLiveEngineError::ProcessNotFound(
            "could not find my own process\n".into(),
        ));
        #[cfg(target_os = "linux")]
        {
            let qagame_os_str = OsStr::new(QAGAME);
            let Ok(myself_process) = Process::myself() else {
                return Err(QuakeLiveEngineError::ProcessNotFound(
                    "could not find my own process\n".into(),
                ));
            };
            let Ok(myself_maps) = myself_process.maps() else {
                return Err(QuakeLiveEngineError::NoMemoryMappingInformationFound(
                    "no memory mapping information found\n".into(),
                ));
            };
            let qagame_maps: Vec<&MemoryMap> = myself_maps
                .memory_maps
                .iter()
                .filter(|mmap| {
                    if let MMapPath::Path(path) = &mmap.pathname {
                        path.file_name() == Some(qagame_os_str)
                    } else {
                        false
                    }
                })
                .collect();

            debug_println!("Searching for necessary VM functions...");
            let mut failed_functions: Vec<QuakeLiveFunction> = Vec::new();
            [
                (QuakeLiveFunction::G_AddEvent, &self.g_addevent_orig),
                (
                    QuakeLiveFunction::CheckPrivileges,
                    &self.check_privileges_orig,
                ),
                (QuakeLiveFunction::ClientConnect, &self.client_connect_orig),
                (QuakeLiveFunction::ClientSpawn, &self.client_spawn_orig),
                (QuakeLiveFunction::G_Damage, &self.g_damage_orig),
                (QuakeLiveFunction::Touch_Item, &self.touch_item_orig),
                (QuakeLiveFunction::LaunchItem, &self.launch_item_orig),
                (QuakeLiveFunction::Drop_Item, &self.drop_item_orig),
                (
                    QuakeLiveFunction::G_StartKamikaze,
                    &self.g_start_kamikaze_orig,
                ),
                (QuakeLiveFunction::G_FreeEntity, &self.g_free_entity_orig),
                (QuakeLiveFunction::Cmd_Callvote_f, &self.cmd_callvote_f_orig),
            ]
            .into_iter()
            .for_each(|(ql_func, field)| {
                match pattern_search_module(&qagame_maps, &ql_func) {
                    None => failed_functions.push(ql_func),
                    Some(orig_func) => {
                        debug_println!(format!("{}: {:#X}", &ql_func, orig_func));
                        field.store(orig_func, Ordering::SeqCst);
                    }
                }
            });

            if !failed_functions.is_empty() {
                return Err(QuakeLiveEngineError::VmFunctionNotFound(
                    failed_functions[0],
                ));
            }

            let base_address = unsafe {
                std::ptr::read_unaligned(
                    (module_offset as u64 + OFFSET_VM_CALL_TABLE as u64) as *const i32,
                )
            };
            let vm_call_table = base_address as usize + module_offset + OFFSET_VM_CALL_TABLE + 4;
            self.vm_call_table.store(vm_call_table, Ordering::SeqCst);

            let g_initgame_orig = unsafe {
                std::ptr::read(
                    (vm_call_table + OFFSET_INITGAME)
                        as *const *const extern "C" fn(c_int, c_int, c_int),
                )
            };
            debug_println!(format!("G_InitGame: {:#X}", g_initgame_orig as usize));
            self.g_init_game_orig
                .store(g_initgame_orig as usize, Ordering::SeqCst);

            let g_shutdowngame_orig = unsafe {
                std::ptr::read_unaligned(vm_call_table as *const *const extern "C" fn(c_int))
            };
            debug_println!(format!(
                "G_ShutdownGame: {:#X}",
                g_shutdowngame_orig as usize
            ));
            self.g_shutdown_game_orig
                .store(g_shutdowngame_orig as usize, Ordering::SeqCst);

            let g_runframe_orig = unsafe {
                std::ptr::read(
                    (vm_call_table + OFFSET_RUNFRAME) as *const *const extern "C" fn(c_int),
                )
            };
            debug_println!(format!("G_RunFrame: {:#X}", g_runframe_orig as usize));
            self.g_run_frame_orig
                .store(g_runframe_orig as usize, Ordering::SeqCst);

            Ok(())
        }
    }

    /*
     * Hooks VM calls. Not all use Hook, since the VM calls are stored in a table of
     * pointers. We simply set our function pointer to the current pointer in the table and
     * then replace the it with our replacement function. Just like hooking a VMT.
     *
     * This must be called AFTER Sys_SetModuleOffset, since Sys_SetModuleOffset is called after
     * the VM DLL has been loaded, meaning the pointer we use has been set.
     *
     * PROTIP: If you can, ALWAYS use VM_Call table hooks instead of using Hook().
     */
    pub(crate) fn hook(&self) -> Result<VmHooksResultType, QuakeLiveEngineError> {
        let mut result = (None, None, None, None);
        let vm_call_table = self.vm_call_table.load(Ordering::SeqCst);

        debug_println!("Hooking VM functions...");
        unsafe {
            std::ptr::write(
                (vm_call_table + 0x18) as *mut usize,
                ShiNQlx_G_InitGame as usize,
            );
        }

        unsafe {
            std::ptr::write(
                (vm_call_table + 0x8) as *mut usize,
                ShiNQlx_G_RunFrame as usize,
            );
        }

        unsafe {
            std::ptr::write(vm_call_table as *mut usize, ShiNQlx_G_ShutdownGame as usize);
        }

        let client_connect_orig = self.client_connect_orig.load(Ordering::SeqCst);
        let client_connect_func = unsafe { std::mem::transmute(client_connect_orig) };
        {
            result.0 = extract_detour(&self.client_connect_detour).take();
            if let Some(existing_client_connect_detour) = &result.0 {
                try_disable(existing_client_connect_detour);
            }
            let Ok(client_connect_detour) = (unsafe {
                ClientConnectDetourType::new(client_connect_func, ShiNQlx_ClientConnect)
            }) else {
                debug_println!("Error hooking into ClientConnect");
                return Err(QuakeLiveEngineError::VmDetourCouldNotBeCreated(
                    QuakeLiveFunction::ClientConnect,
                ));
            };
            if unsafe { client_connect_detour.enable() }.is_err() {
                debug_println!("Error enabling ClientConnect hook");
                return Err(QuakeLiveEngineError::VmDetourCouldNotBeEnabled(
                    QuakeLiveFunction::ClientConnect,
                ));
            }
            let Ok(mut guard) = self.client_connect_detour.write() else {
                debug_println!("ClientConnect detour was poisoned. Exiting.");
                return Err(QuakeLiveEngineError::VmDetourPoisoned(
                    QuakeLiveFunction::ClientConnect,
                ));
            };
            *guard = Some(client_connect_detour);
        }
        let g_start_kamikaze_orig = self.g_start_kamikaze_orig.load(Ordering::SeqCst);
        {
            result.1 = extract_detour(&self.g_start_kamikaze_detour).take();
            if let Some(existing_g_start_kamikaze_detour) = &result.1 {
                try_disable(existing_g_start_kamikaze_detour);
            }
            let g_start_kamikaze_func = unsafe { std::mem::transmute(g_start_kamikaze_orig) };
            let Ok(g_start_kamikaze_detour) = (unsafe {
                GStartKamikazeDetourType::new(g_start_kamikaze_func, ShiNQlx_G_StartKamikaze)
            }) else {
                debug_println!("Error hooking into G_StartKamize");
                return Err(QuakeLiveEngineError::VmDetourCouldNotBeCreated(
                    QuakeLiveFunction::G_StartKamikaze,
                ));
            };
            if unsafe { g_start_kamikaze_detour.enable() }.is_err() {
                debug_println!("Error enabling G_StartKamikaze hook");
                return Err(QuakeLiveEngineError::VmDetourCouldNotBeEnabled(
                    QuakeLiveFunction::G_StartKamikaze,
                ));
            };
            let Ok(mut guard) = self.g_start_kamikaze_detour.write() else {
                debug_println!("G_StartKamikaze detour was poisoned. Exiting.");
                return Err(QuakeLiveEngineError::VmDetourPoisoned(
                    QuakeLiveFunction::G_StartKamikaze,
                ));
            };
            *guard = Some(g_start_kamikaze_detour);
        }

        let client_spawn_orig = self.client_spawn_orig.load(Ordering::SeqCst);
        let client_spawn_func = unsafe { std::mem::transmute(client_spawn_orig) };
        {
            result.2 = extract_detour(&self.client_spawn_detour).take();
            if let Some(existing_client_spawn_detour) = &result.2 {
                try_disable(existing_client_spawn_detour);
            }
            let Ok(client_spawn_detour) =
                (unsafe { ClientSpawnDetourType::new(client_spawn_func, ShiNQlx_ClientSpawn) })
            else {
                debug_println!("Error hooking into ClientSpawn");
                return Err(QuakeLiveEngineError::VmDetourCouldNotBeCreated(
                    QuakeLiveFunction::ClientSpawn,
                ));
            };
            if unsafe { client_spawn_detour.enable() }.is_err() {
                debug_println!("Error enabling ClientSpawn hook");
                return Err(QuakeLiveEngineError::VmDetourCouldNotBeEnabled(
                    QuakeLiveFunction::ClientSpawn,
                ));
            }
            let Ok(mut guard) = self.client_spawn_detour.write() else {
                debug_println!("ClientSpawn detour was poisoned. Exiting.");
                return Err(QuakeLiveEngineError::VmDetourPoisoned(
                    QuakeLiveFunction::ClientSpawn,
                ));
            };
            *guard = Some(client_spawn_detour);
        }

        let g_damage_orig = self.g_damage_orig.load(Ordering::SeqCst);
        let g_damage_func = unsafe { std::mem::transmute(g_damage_orig) };
        {
            result.3 = extract_detour(&self.g_damage_detour).take();
            if let Some(existing_g_damage_detour) = &result.3 {
                try_disable(existing_g_damage_detour);
            }
            let Ok(g_damage_detour) =
                (unsafe { GDamageDetourType::new(g_damage_func, ShiNQlx_G_Damage) })
            else {
                debug_println!("Error hooking into G_Damage");
                return Err(QuakeLiveEngineError::VmDetourCouldNotBeCreated(
                    QuakeLiveFunction::G_Damage,
                ));
            };
            if unsafe { g_damage_detour.enable() }.is_err() {
                debug_println!("Error enabling G_Damage hook");
                return Err(QuakeLiveEngineError::VmDetourCouldNotBeEnabled(
                    QuakeLiveFunction::G_Damage,
                ));
            }
            let Ok(mut guard) = self.g_damage_detour.write() else {
                debug_println!("G_Damage detour was poisoned. Exiting.");
                return Err(QuakeLiveEngineError::VmDetourPoisoned(
                    QuakeLiveFunction::G_Damage,
                ));
            };
            *guard = Some(g_damage_detour);
        }

        Ok(result)
    }

    pub(crate) fn patch(&self) {
        let cmd_callvote_f_orig = self.cmd_callvote_f_orig.load(Ordering::SeqCst);
        if cmd_callvote_f_orig == 0 {
            return;
        }

        patch_callvote_f(cmd_callvote_f_orig);
    }

    pub(crate) fn unhook(&self) -> Result<VmHooksResultType, QuakeLiveEngineError> {
        let mut result = (None, None, None, None);

        {
            result.0 = extract_detour(&self.client_connect_detour).take();
            if let Some(client_connect_detour) = &result.0 {
                try_disable(client_connect_detour);
            }
        }

        {
            result.1 = extract_detour(&self.g_start_kamikaze_detour).take();
            if let Some(start_kamikaze_detour) = &result.1 {
                try_disable(start_kamikaze_detour);
            }
        }

        {
            result.2 = extract_detour(&self.client_spawn_detour).take();
            if let Some(client_spawn_detour) = &result.2 {
                try_disable(client_spawn_detour);
            }
        }

        {
            result.3 = extract_detour(&self.g_damage_detour).take();
            if let Some(g_damage_detour) = &result.3 {
                try_disable(g_damage_detour);
            }
        }

        Ok(result)
    }
}

#[derive(Debug)]
pub(crate) struct QuakeLiveEngine {
    static_functions: OnceCell<StaticFunctions>,
    static_detours: OnceCell<StaticDetours>,

    pub(crate) sv_maxclients: AtomicI32,
    common_initialized: OnceBool,

    vm_functions: VmFunctions,
    current_vm: AtomicUsize,

    pending_client_spawn_detours: RwLock<VecDeque<ClientSpawnDetourType>>,
    pending_client_connect_detours: RwLock<VecDeque<ClientConnectDetourType>>,
    pending_g_start_kamikaze_detours: RwLock<VecDeque<GStartKamikazeDetourType>>,
    pending_g_damage_detours: RwLock<VecDeque<GDamageDetourType>>,
}

#[allow(dead_code)]
const OFFSET_CMD_ARGC: i32 = 0x81;

impl QuakeLiveEngine {
    pub(crate) fn new() -> Self {
        Self {
            static_functions: OnceCell::new(),
            static_detours: OnceCell::new(),

            sv_maxclients: AtomicI32::new(0),
            common_initialized: OnceBool::new(),

            vm_functions: VmFunctions {
                vm_call_table: Default::default(),
                g_addevent_orig: Default::default(),
                check_privileges_orig: Default::default(),
                client_connect_orig: Default::default(),
                client_spawn_orig: Default::default(),
                g_damage_orig: Default::default(),
                touch_item_orig: Default::default(),
                launch_item_orig: Default::default(),
                drop_item_orig: Default::default(),
                g_start_kamikaze_orig: Default::default(),
                g_free_entity_orig: Default::default(),
                g_init_game_orig: Default::default(),
                g_shutdown_game_orig: Default::default(),
                g_run_frame_orig: Default::default(),
                cmd_callvote_f_orig: Default::default(),
                client_spawn_detour: Default::default(),
                client_connect_detour: Default::default(),
                g_start_kamikaze_detour: Default::default(),
                g_damage_detour: Default::default(),
            },
            current_vm: AtomicUsize::new(0),

            pending_client_connect_detours: RwLock::new(VecDeque::with_capacity(3)),
            pending_client_spawn_detours: RwLock::new(VecDeque::with_capacity(3)),
            pending_g_start_kamikaze_detours: RwLock::new(VecDeque::with_capacity(3)),
            pending_g_damage_detours: RwLock::new(VecDeque::with_capacity(3)),
        }
    }

    pub(crate) fn search_static_functions(&self) -> Result<(), QuakeLiveEngineError> {
        #[cfg(not(target_os = "linux"))]
        return Err(QuakeLiveEngineError::ProcessNotFound(
            "could not find my own process\n".into(),
        ));
        #[cfg(target_os = "linux")]
        {
            let qzeroded_os_str = OsStr::new(QZERODED);
            let Ok(myself_process) = Process::myself() else {
                return Err(QuakeLiveEngineError::ProcessNotFound(
                    "could not find my own process\n".into(),
                ));
            };
            let Ok(myself_maps) = myself_process.maps() else {
                return Err(QuakeLiveEngineError::NoMemoryMappingInformationFound(
                    "no memory mapping information found\n".into(),
                ));
            };
            let qzeroded_maps: Vec<&MemoryMap> = myself_maps
                .memory_maps
                .iter()
                .filter(|mmap| {
                    if let MMapPath::Path(path) = &mmap.pathname {
                        path.file_name() == Some(qzeroded_os_str)
                    } else {
                        false
                    }
                })
                .collect();

            if qzeroded_maps.is_empty() {
                debug_println!(format!(
                    "no memory mapping information for {} found",
                    QZERODED
                ));
                return Err(QuakeLiveEngineError::NoMemoryMappingInformationFound(
                    "no memory mapping information found\n".into(),
                ));
            }

            debug_println!("Searching for necessary functions...");
            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Com_Printf)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Com_Printf
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Com_Printf,
                ));
            };
            debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Com_Printf, result));
            let com_printf_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_AddCommand)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cmd_AddCommand
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_AddCommand,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::Cmd_AddCommand,
                result
            ));
            let cmd_addcommand_orig = unsafe { std::mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_Args)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cmd_Args
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Args,
                ));
            };
            debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Cmd_Args, result));
            let cmd_args_orig = unsafe { std::mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_Argv)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cmd_Argv
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Argv,
                ));
            };
            debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Cmd_Argv, result));
            let cmd_argv_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_Tokenizestring)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cmd_Tokenizestring
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Tokenizestring,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::Cmd_Tokenizestring,
                result
            ));
            let cmd_tokenizestring_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cbuf_ExecuteText)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cbuf_ExecuteText
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cbuf_ExecuteText,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::Cbuf_ExecuteText,
                result
            ));
            let cbuf_executetext_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cvar_FindVar)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cvar_FindVar
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_FindVar,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::Cvar_FindVar,
                result
            ));
            let cvar_findvar_orig = unsafe { std::mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cvar_Get)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cvar_Get
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_Get,
                ));
            };
            debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Cvar_Get, result));
            let cvar_get_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cvar_GetLimit)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cvar_GetLimit
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_GetLimit,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::Cvar_GetLimit,
                result
            ));
            let cvar_getlimit_orig = unsafe { std::mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cvar_Set2)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cvar_Set2
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_Set2,
                ));
            };
            debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::Cvar_Set2, result));
            let cvar_set2_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_SendServerCommand)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::SV_SendServerCommand
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_SendServerCommand,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::SV_SendServerCommand,
                result
            ));
            let sv_sendservercommand_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_ExecuteClientCommand)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::SV_ExecuteClientCommand
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_ExecuteClientCommand,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::SV_ExecuteClientCommand,
                result
            ));
            let sv_executeclientcommand_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_Shutdown)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::SV_Shutdown
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_Shutdown,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::SV_Shutdown,
                result
            ));
            let sv_shutdown_orig = unsafe { std::mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_Map_f)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::SV_Map_f
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_Map_f,
                ));
            };
            debug_println!(format!("{}: {:#X}", &QuakeLiveFunction::SV_Map_f, result));
            let sv_map_f_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_ClientEnterWorld)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::SV_ClientEnterWorld
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_ClientEnterWorld,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::SV_ClientEnterWorld,
                result
            ));
            let sv_cliententerworld_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_SetConfigstring)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::SV_SetConfigstring
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_SetConfigstring,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::SV_SetConfigstring,
                result
            ));
            let sv_setconfigstring_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_GetConfigstring)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::SV_GetConfigstring
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_GetConfigstring,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::SV_GetConfigstring,
                result
            ));
            let sv_getconfigstring_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_DropClient)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::SV_DropClient
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_DropClient,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::SV_DropClient,
                result
            ));
            let sv_dropclient_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Sys_SetModuleOffset)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Sys_SetModuleOffset
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Sys_SetModuleOffset,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::Sys_SetModuleOffset,
                result
            ));
            let sys_setmoduleoffset_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::SV_SpawnServer)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::SV_SpawnServer
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_SpawnServer,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::SV_SpawnServer,
                result
            ));
            let sv_spawnserver_orig = unsafe { std::mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, &QuakeLiveFunction::Cmd_ExecuteString)
            else {
                debug_println!(format!(
                    "Function {} not found",
                    &QuakeLiveFunction::Cmd_ExecuteString
                ));
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_ExecuteString,
                ));
            };
            debug_println!(format!(
                "{}: {:#X}",
                &QuakeLiveFunction::Cmd_ExecuteString,
                result
            ));
            let cmd_executestring_orig = unsafe { std::mem::transmute(result) };

            // Cmd_Argc is really small, making it hard to search for, so we use a reference to it instead.
            let base_address = unsafe {
                std::ptr::read_unaligned(
                    (sv_map_f_orig as usize + OFFSET_CMD_ARGC as usize) as *const i32,
                )
            };
            #[allow(clippy::fn_to_numeric_cast_with_truncation)]
            let cmd_argc_ptr = base_address + sv_map_f_orig as i32 + OFFSET_CMD_ARGC + 4;
            debug_println!(format!(
                "{}: {:#X}",
                QuakeLiveFunction::Cmd_Argc,
                cmd_argc_ptr
            ));
            let cmd_argc_orig = unsafe { std::mem::transmute(cmd_argc_ptr as u64) };

            self.static_functions
                .set(StaticFunctions {
                    com_printf_orig,
                    cmd_addcommand_orig,
                    cmd_args_orig,
                    cmd_argv_orig,
                    cmd_tokenizestring_orig,
                    cbuf_executetext_orig,
                    cvar_findvar_orig,
                    cvar_get_orig,
                    cvar_getlimit_orig,
                    cvar_set2_orig,
                    sv_sendservercommand_orig,
                    sv_executeclientcommand_orig,
                    sv_shutdown_orig,
                    sv_map_f_orig,
                    sv_cliententerworld_orig,
                    sv_setconfigstring_orig,
                    sv_getconfigstring_orig,
                    sv_dropclient_orig,
                    sys_setmoduleoffset_orig,
                    sv_spawnserver_orig,
                    cmd_executestring_orig,
                    cmd_argc_orig,
                })
                .unwrap();

            Ok(())
        }
    }

    pub(crate) fn hook_static(&self) -> Result<(), QuakeLiveEngineError> {
        debug_println!("Hooking...");
        let Ok(cmd_addcommand_orig) = self.cmd_addcommand_orig() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_AddCommand,
            ));
        };
        let cmd_addcommand_detour = unsafe {
            GenericDetour::new(cmd_addcommand_orig, shinqlx_cmd_addcommand).map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeCreated(
                    QuakeLiveFunction::Cmd_AddCommand,
                )
            })?
        };
        unsafe {
            cmd_addcommand_detour.enable().map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeEnabled(
                    QuakeLiveFunction::Cmd_AddCommand,
                )
            })?
        };

        let Ok(sys_setmoduleoffset_orig) = self.sys_setmoduleoffset_orig() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Sys_SetModuleOffset,
            ));
        };
        let sys_setmoduleoffset_detour = unsafe {
            GenericDetour::new(sys_setmoduleoffset_orig, shinqlx_sys_setmoduleoffset).map_err(
                |_| {
                    QuakeLiveEngineError::StaticDetourCouldNotBeCreated(
                        QuakeLiveFunction::Sys_SetModuleOffset,
                    )
                },
            )?
        };
        unsafe {
            sys_setmoduleoffset_detour.enable().map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeEnabled(
                    QuakeLiveFunction::Cmd_AddCommand,
                )
            })?
        };

        let Ok(sv_executeclientcommand_orig) = self.sv_executeclientcommand_orig() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ExecuteClientCommand,
            ));
        };
        let sv_executeclientcommand_detour = unsafe {
            GenericDetour::new(
                sv_executeclientcommand_orig,
                shinqlx_sv_executeclientcommand,
            )
            .map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeCreated(
                    QuakeLiveFunction::SV_ExecuteClientCommand,
                )
            })?
        };
        unsafe {
            sv_executeclientcommand_detour.enable().map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeEnabled(
                    QuakeLiveFunction::SV_ExecuteClientCommand,
                )
            })?
        };

        let Ok(sv_cliententerworld_orig) = self.sv_cliententerworld_orig() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ClientEnterWorld,
            ));
        };
        let sv_cliententerworld_detour = unsafe {
            GenericDetour::new(sv_cliententerworld_orig, shinqlx_sv_cliententerworld).map_err(
                |_| {
                    QuakeLiveEngineError::StaticDetourCouldNotBeCreated(
                        QuakeLiveFunction::SV_ClientEnterWorld,
                    )
                },
            )?
        };
        unsafe {
            sv_cliententerworld_detour.enable().map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeEnabled(
                    QuakeLiveFunction::SV_ClientEnterWorld,
                )
            })?
        };

        let Ok(sv_sendservercommand_orig) = self.sv_sendservercommand_orig() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SendServerCommand,
            ));
        };
        let sv_sendservercommand_detour = unsafe {
            RawDetour::new(
                sv_sendservercommand_orig as *const (),
                ShiNQlx_SV_SendServerCommand as *const (),
            )
            .map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeCreated(
                    QuakeLiveFunction::SV_SendServerCommand,
                )
            })?
        };
        unsafe {
            sv_sendservercommand_detour.enable().map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeEnabled(
                    QuakeLiveFunction::SV_SendServerCommand,
                )
            })?
        };

        let Ok(sv_setconfigstring_orig) = self.sv_setconfigstring_orig() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SetConfigstring,
            ));
        };
        let sv_setconfgistring_detour = unsafe {
            GenericDetour::new(sv_setconfigstring_orig, shinqlx_sv_setconfigstring).map_err(
                |_| {
                    QuakeLiveEngineError::StaticDetourCouldNotBeCreated(
                        QuakeLiveFunction::SV_SetConfigstring,
                    )
                },
            )?
        };
        unsafe {
            sv_setconfgistring_detour.enable().map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeEnabled(
                    QuakeLiveFunction::SV_SetConfigstring,
                )
            })?
        };

        let Ok(sv_dropclient_orig) = self.sv_dropclient_orig() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SetConfigstring,
            ));
        };
        let sv_dropclient_detour = unsafe {
            GenericDetour::new(sv_dropclient_orig, shinqlx_sv_dropclient).map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeCreated(
                    QuakeLiveFunction::SV_DropClient,
                )
            })?
        };
        unsafe {
            sv_dropclient_detour.enable().map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeEnabled(
                    QuakeLiveFunction::SV_DropClient,
                )
            })?
        };

        let Ok(com_printf_orig) = self.com_printf_orig() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Com_Printf,
            ));
        };
        let com_printf_detour = unsafe {
            RawDetour::new(
                com_printf_orig as *const (),
                ShiNQlx_Com_Printf as *const (),
            )
            .map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeCreated(QuakeLiveFunction::Com_Printf)
            })?
        };
        unsafe {
            com_printf_detour.enable().map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeEnabled(QuakeLiveFunction::Com_Printf)
            })?
        };

        let Ok(original_func) = self.sv_spawnserver_orig() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SpawnServer,
            ));
        };
        let sv_spawnserver_detour = unsafe {
            GenericDetour::new(original_func, shinqlx_sv_spawnserver).map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeCreated(
                    QuakeLiveFunction::SV_SpawnServer,
                )
            })?
        };
        unsafe {
            sv_spawnserver_detour.enable().map_err(|_| {
                QuakeLiveEngineError::StaticDetourCouldNotBeEnabled(
                    QuakeLiveFunction::SV_SpawnServer,
                )
            })?
        };

        self.static_detours
            .set(StaticDetours {
                cmd_addcommand_detour,
                sys_setmoduleoffset_detour,
                sv_executeclientcommand_detour,
                sv_cliententerworld_detour,
                sv_setconfgistring_detour,
                sv_dropclient_detour,
                sv_spawnserver_detour,
                sv_sendservercommand_detour,
                com_printf_detour,
            })
            .unwrap();

        Ok(())
    }

    pub(crate) fn set_tag(&self) {
        let Some(sv_tags) = self.find_cvar("sv_tags") else {
            return;
        };

        let sv_tags_string = sv_tags.get_string();

        if sv_tags_string.split(',').any(|x| x == SV_TAGS_PREFIX) {
            return;
        }

        let new_tags = if sv_tags_string.len() > 2 {
            format!("{},{}", SV_TAGS_PREFIX, sv_tags_string)
        } else {
            SV_TAGS_PREFIX.into()
        };
        self.set_cvar_forced("sv_tags", new_tags.as_str(), false);
    }

    // Called after the game is initialized.
    pub(crate) fn initialize_cvars(&self) {
        let Some(maxclients) = self.find_cvar("sv_maxclients") else {
            return;
        };

        self.sv_maxclients
            .store(maxclients.get_integer(), Ordering::Relaxed);
    }

    pub(crate) fn get_max_clients(&self) -> i32 {
        self.sv_maxclients.load(Ordering::Relaxed)
    }

    // Currently called by My_Cmd_AddCommand(), since it's called at a point where we
    // can safely do whatever we do below. It'll segfault if we do it at the entry
    // point, since functions like Cmd_AddCommand need initialization first.
    pub(crate) fn initialize_static(&self) {
        debug_println!("Initializing...");
        self.add_command("cmd", cmd_send_server_command);
        self.add_command("cp", cmd_center_print);
        self.add_command("print", cmd_regular_print);
        self.add_command("slap", cmd_slap);
        self.add_command("slay", cmd_slay);
        self.add_command("qlx", cmd_py_rcon);
        self.add_command("pycmd", cmd_py_command);
        self.add_command("pyrestart", cmd_restart_python);

        let res = pyminqlx_initialize();

        if res != PYM_SUCCESS {
            debug_println!("Python initialization failed.");
            panic!("Python initialization failed.");
        }

        self.common_initialized.set(true).unwrap();
    }

    pub(crate) fn is_common_initialized(&self) -> bool {
        self.common_initialized
            .get()
            .is_some_and(|is_initialized| is_initialized)
    }

    pub(crate) fn initialize_vm(&self, module_offset: usize) -> Result<(), QuakeLiveEngineError> {
        self.vm_functions.try_initialize_from(module_offset)?;
        self.current_vm.store(module_offset, Ordering::SeqCst);

        let hook_result = self.vm_functions.hook()?;
        self.store_pending_detours(hook_result);
        self.vm_functions.patch();

        #[cfg(debug_assertions)]
        self.print_pending_detour_sizes();

        self.clean_up_pending_detours();
        Ok(())
    }

    fn store_pending_detours(&self, mut vm_hook_result: VmHooksResultType) {
        {
            if let Some(client_connect_detour) = vm_hook_result.0.take() {
                try_disable(&client_connect_detour);
                if let Ok(mut pending_client_connect_lock) =
                    self.pending_client_connect_detours.write()
                {
                    (*pending_client_connect_lock).push_back(client_connect_detour);
                }
            }
        }

        {
            if let Some(start_kamikaze_detour) = vm_hook_result.1.take() {
                try_disable(&start_kamikaze_detour);
                if let Ok(mut pending_start_kamikaze_lock) =
                    self.pending_g_start_kamikaze_detours.write()
                {
                    (*pending_start_kamikaze_lock).push_back(start_kamikaze_detour);
                }
            }
        }

        {
            if let Some(client_spawn_detour) = vm_hook_result.2.take() {
                try_disable(&client_spawn_detour);
                if let Ok(mut pending_client_spawn_lock) = self.pending_client_spawn_detours.write()
                {
                    (*pending_client_spawn_lock).push_back(client_spawn_detour);
                }
            }
        }

        {
            if let Some(g_damage_detour) = vm_hook_result.3.take() {
                if g_damage_detour.is_enabled() {
                    if let Err(e) = unsafe { g_damage_detour.disable() } {
                        debug_println!(format!("error when disabling G_Damage detour: {}", e));
                    }
                }
                if let Ok(mut pending_g_danage_lock) = self.pending_g_damage_detours.write() {
                    (*pending_g_danage_lock).push_back(g_damage_detour);
                }
            }
        }
    }

    fn clean_up_pending_detours(&self) {
        {
            let Ok(mut pending_client_connect_lock) = self.pending_client_connect_detours.write()
            else {
                debug_println!("pending ClientConnect detour poisoned. Exiting.");
                return;
            };

            while (*pending_client_connect_lock).len()
                >= (*pending_client_connect_lock).capacity() - 2
            {
                let Some(detour) = (*pending_client_connect_lock).pop_front() else {
                    continue;
                };
                #[cfg(debug_assertions)]
                debug_println!("Trying to drop pending ClientConnect detour");
                std::mem::drop(detour);
                #[cfg(debug_assertions)]
                debug_println!("Detour dropped sucessfully");
            }
        }

        {
            let Ok(mut pending_g_start_kamikaze_lock) =
                self.pending_g_start_kamikaze_detours.write()
            else {
                debug_println!("pending G_STartKamikaze detour poisoned. Exiting.");
                return;
            };

            while (*pending_g_start_kamikaze_lock).len()
                >= (*pending_g_start_kamikaze_lock).capacity() - 2
            {
                let Some(detour) = (*pending_g_start_kamikaze_lock).pop_front() else {
                    continue;
                };
                #[cfg(debug_assertions)]
                debug_println!("Trying to drop pending G_StartKamikaze detour");
                std::mem::drop(detour);
                #[cfg(debug_assertions)]
                debug_println!("Detour dropped sucessfully");
            }
        }

        {
            let Ok(mut pending_client_spawn_lock) = self.pending_client_spawn_detours.write()
            else {
                debug_println!("pending ClientSpawn detour poisoned. Exiting.");
                return;
            };

            while (*pending_client_spawn_lock).len() >= (*pending_client_spawn_lock).capacity() - 2
            {
                let Some(detour) = (*pending_client_spawn_lock).pop_front() else {
                    continue;
                };
                #[cfg(debug_assertions)]
                debug_println!("Trying to drop pending ClientSpawn detour");
                std::mem::drop(detour);
                #[cfg(debug_assertions)]
                debug_println!("Detour dropped sucessfully");
            }
        }

        {
            let Ok(mut pending_g_damage_lock) = self.pending_g_damage_detours.write() else {
                debug_println!("pending G_Damage detour poisoned. Exiting.");
                return;
            };

            while (*pending_g_damage_lock).len() >= (*pending_g_damage_lock).capacity() - 2 {
                let Some(detour) = (*pending_g_damage_lock).pop_front() else {
                    continue;
                };
                #[cfg(debug_assertions)]
                debug_println!("Trying to drop pending G_Damage detour");
                std::mem::drop(detour);
                #[cfg(debug_assertions)]
                debug_println!("Detour dropped sucessfully");
            }
        }
    }

    pub(crate) fn unhook_vm(&self) -> Result<(), QuakeLiveEngineError> {
        if self.vm_functions.client_connect_detour.is_poisoned() {
            debug_println!("ClientConnect detour poisoned. Exiting.");
            return Err(QuakeLiveEngineError::VmDetourPoisoned(
                QuakeLiveFunction::ClientConnect,
            ));
        }

        if self.vm_functions.g_start_kamikaze_detour.is_poisoned() {
            debug_println!("G_StartKamikaze detour poisoned. Exiting.");
            return Err(QuakeLiveEngineError::VmDetourPoisoned(
                QuakeLiveFunction::G_StartKamikaze,
            ));
        }

        if self.vm_functions.client_spawn_detour.is_poisoned() {
            debug_println!("ClientSpawn detour poisoned. Exiting.");
            return Err(QuakeLiveEngineError::VmDetourPoisoned(
                QuakeLiveFunction::ClientSpawn,
            ));
        }

        if self.vm_functions.g_damage_detour.is_poisoned() {
            debug_println!("G_Damage detour poisoned. Exiting.");
            return Err(QuakeLiveEngineError::VmDetourPoisoned(
                QuakeLiveFunction::G_Damage,
            ));
        }

        let vm_unhook_result = self.vm_functions.unhook()?;
        self.store_pending_detours(vm_unhook_result);
        Ok(())
    }

    #[cfg(debug_assertions)]
    fn print_pending_detour_sizes(&self) {
        if let Ok(pending_client_connect) = self.pending_client_connect_detours.read() {
            debug_println!(format!(
                "pending ClientConnect detours: {}",
                (*pending_client_connect).len()
            ));
        }

        if let Ok(pending_client_spawn) = self.pending_client_spawn_detours.read() {
            debug_println!(format!(
                "pending ClientSpawn detours: {}",
                (*pending_client_spawn).len()
            ));
        }

        if let Ok(pending_g_start_kamikaze) = self.pending_g_start_kamikaze_detours.read() {
            debug_println!(format!(
                "pending G_StartKamikaze detours: {}",
                (*pending_g_start_kamikaze).len()
            ));
        }

        if let Ok(pending_g_damage) = self.pending_g_damage_detours.read() {
            debug_println!(format!(
                "pending G_Damage detours: {}",
                (*pending_g_damage).len()
            ));
        }
    }

    fn com_printf_orig(&self) -> Result<extern "C" fn(*const c_char, ...), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Com_Printf,
            ));
        };
        Ok(static_functions.com_printf_orig)
    }

    fn cmd_addcommand_orig(
        &self,
    ) -> Result<fn(*const c_char, unsafe extern "C" fn()), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_AddCommand,
            ));
        };
        Ok(static_functions.cmd_addcommand_orig)
    }

    fn cmd_args_orig(&self) -> Result<fn() -> *const c_char, QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Args,
            ));
        };
        Ok(static_functions.cmd_args_orig)
    }

    fn cmd_argv_orig(&self) -> Result<fn(c_int) -> *const c_char, QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Argv,
            ));
        };
        Ok(static_functions.cmd_argv_orig)
    }

    #[allow(dead_code)]
    fn cmd_tokenizestring_orig(&self) -> Result<fn(*const c_char), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Tokenizestring,
            ));
        };
        Ok(static_functions.cmd_tokenizestring_orig)
    }

    #[allow(dead_code)]
    fn cbuf_executetext_orig(&self) -> Result<fn(cbufExec_t, *const c_char), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cbuf_ExecuteText,
            ));
        };
        Ok(static_functions.cbuf_executetext_orig)
    }

    fn cvar_findvar_orig(&self) -> Result<fn(*const c_char) -> *mut cvar_t, QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cvar_FindVar,
            ));
        };
        Ok(static_functions.cvar_findvar_orig)
    }

    #[allow(clippy::type_complexity)]
    fn cvar_get_orig(
        &self,
    ) -> Result<fn(*const c_char, *const c_char, c_int) -> *mut cvar_t, QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cvar_Get,
            ));
        };
        Ok(static_functions.cvar_get_orig)
    }

    #[allow(clippy::type_complexity)]
    fn cvar_getlimit_orig(
        &self,
    ) -> Result<
        fn(*const c_char, *const c_char, *const c_char, *const c_char, c_int) -> *mut cvar_t,
        QuakeLiveEngineError,
    > {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cvar_GetLimit,
            ));
        };
        Ok(static_functions.cvar_getlimit_orig)
    }

    #[allow(clippy::type_complexity)]
    fn cvar_set2_orig(
        &self,
    ) -> Result<fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t, QuakeLiveEngineError>
    {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cvar_Set2,
            ));
        };
        Ok(static_functions.cvar_set2_orig)
    }

    fn sv_sendservercommand_orig(
        &self,
    ) -> Result<extern "C" fn(*mut client_t, *const c_char, ...), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SendServerCommand,
            ));
        };
        Ok(static_functions.sv_sendservercommand_orig)
    }

    fn sv_executeclientcommand_orig(
        &self,
    ) -> Result<fn(*mut client_t, *const c_char, qboolean), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ExecuteClientCommand,
            ));
        };
        Ok(static_functions.sv_executeclientcommand_orig)
    }

    pub(crate) fn sv_shutdown_orig(&self) -> Result<fn(*const c_char), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Tokenizestring,
            ));
        };
        Ok(static_functions.sv_shutdown_orig)
    }

    #[allow(dead_code)]
    fn sv_map_f_orig(&self) -> Result<fn(), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_Map_f,
            ));
        };
        Ok(static_functions.sv_map_f_orig)
    }

    fn sv_cliententerworld_orig(
        &self,
    ) -> Result<fn(*mut client_t, *mut usercmd_t), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ClientEnterWorld,
            ));
        };
        Ok(static_functions.sv_cliententerworld_orig)
    }

    fn sv_setconfigstring_orig(&self) -> Result<fn(c_int, *const c_char), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SetConfigstring,
            ));
        };
        Ok(static_functions.sv_setconfigstring_orig)
    }

    fn sv_getconfigstring_orig(
        &self,
    ) -> Result<fn(c_int, *const c_char, c_int), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_GetConfigstring,
            ));
        };
        Ok(static_functions.sv_getconfigstring_orig)
    }

    fn sv_dropclient_orig(&self) -> Result<fn(*mut client_t, *const c_char), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_DropClient,
            ));
        };
        Ok(static_functions.sv_dropclient_orig)
    }

    fn sys_setmoduleoffset_orig(
        &self,
    ) -> Result<fn(*const c_char, unsafe extern "C" fn()), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Sys_SetModuleOffset,
            ));
        };
        Ok(static_functions.sys_setmoduleoffset_orig)
    }

    fn sv_spawnserver_orig(&self) -> Result<fn(*const c_char, qboolean), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SpawnServer,
            ));
        };
        Ok(static_functions.sv_spawnserver_orig)
    }

    fn cmd_executestring_orig(&self) -> Result<fn(*const c_char), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_ExecuteString,
            ));
        };
        Ok(static_functions.cmd_executestring_orig)
    }

    fn cmd_argc_orig(&self) -> Result<fn() -> c_int, QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Argc,
            ));
        };
        Ok(static_functions.cmd_argc_orig)
    }

    fn cmd_addcommand_detour(
        &self,
    ) -> Result<&GenericDetour<fn(*const c_char, unsafe extern "C" fn())>, QuakeLiveEngineError>
    {
        let Some(static_detours) = self.static_detours.get() else {
            return Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::Cmd_AddCommand,
            ));
        };
        Ok(&static_detours.cmd_addcommand_detour)
    }

    fn sys_setmoduleoffset_detour(
        &self,
    ) -> Result<&GenericDetour<fn(*const c_char, unsafe extern "C" fn())>, QuakeLiveEngineError>
    {
        let Some(static_detours) = self.static_detours.get() else {
            return Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::Sys_SetModuleOffset,
            ));
        };
        Ok(&static_detours.sys_setmoduleoffset_detour)
    }

    #[allow(clippy::type_complexity)]
    fn sv_executeclientcommand_detour(
        &self,
    ) -> Result<&GenericDetour<fn(*mut client_t, *const c_char, qboolean)>, QuakeLiveEngineError>
    {
        let Some(static_detours) = self.static_detours.get() else {
            return Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_ExecuteClientCommand,
            ));
        };
        Ok(&static_detours.sv_executeclientcommand_detour)
    }

    #[allow(clippy::type_complexity)]
    fn sv_cliententerworld_detour(
        &self,
    ) -> Result<&GenericDetour<fn(*mut client_t, *mut usercmd_t)>, QuakeLiveEngineError> {
        let Some(static_detours) = self.static_detours.get() else {
            return Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_ClientEnterWorld,
            ));
        };
        Ok(&static_detours.sv_cliententerworld_detour)
    }

    #[allow(clippy::type_complexity)]
    fn sv_setconfgistring_detour(
        &self,
    ) -> Result<&GenericDetour<fn(c_int, *const c_char)>, QuakeLiveEngineError> {
        let Some(static_detours) = self.static_detours.get() else {
            return Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_SetConfigstring,
            ));
        };
        Ok(&static_detours.sv_setconfgistring_detour)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn sv_dropclient_detour(
        &self,
    ) -> Result<&GenericDetour<fn(*mut client_t, *const c_char)>, QuakeLiveEngineError> {
        let Some(static_detours) = self.static_detours.get() else {
            return Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_DropClient,
            ));
        };
        Ok(&static_detours.sv_dropclient_detour)
    }

    #[allow(clippy::type_complexity)]
    fn sv_spawnserver_detour(
        &self,
    ) -> Result<&GenericDetour<fn(*const c_char, qboolean)>, QuakeLiveEngineError> {
        let Some(static_detours) = self.static_detours.get() else {
            return Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_SpawnServer,
            ));
        };
        Ok(&static_detours.sv_spawnserver_detour)
    }

    fn sv_sendservercommand_detour(&self) -> Result<&RawDetour, QuakeLiveEngineError> {
        let Some(static_detours) = self.static_detours.get() else {
            return Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_SendServerCommand,
            ));
        };
        Ok(&static_detours.sv_sendservercommand_detour)
    }

    fn com_printf_detour(&self) -> Result<&RawDetour, QuakeLiveEngineError> {
        let Some(static_detours) = self.static_detours.get() else {
            return Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::Com_Printf,
            ));
        };
        Ok(&static_detours.com_printf_detour)
    }

    pub(crate) fn g_init_game_orig(
        &self,
    ) -> Result<extern "C" fn(c_int, c_int, c_int), QuakeLiveEngineError> {
        let g_init_game_orig = self.vm_functions.g_init_game_orig.load(Ordering::SeqCst);
        if g_init_game_orig == 0 {
            return Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_InitGame,
            ));
        }

        let g_init_game_func = unsafe { std::mem::transmute(g_init_game_orig) };
        Ok(g_init_game_func)
    }

    fn g_shutdown_game_orig(&self) -> Result<extern "C" fn(c_int), QuakeLiveEngineError> {
        let g_shutdown_game_orig = self
            .vm_functions
            .g_shutdown_game_orig
            .load(Ordering::SeqCst);
        if g_shutdown_game_orig == 0 {
            return Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_ShutdownGame,
            ));
        }

        let g_shutdown_game_func = unsafe { std::mem::transmute(g_shutdown_game_orig) };
        Ok(g_shutdown_game_func)
    }

    pub(crate) fn g_run_frame_orig(&self) -> Result<extern "C" fn(c_int), QuakeLiveEngineError> {
        let g_run_frame_orig = self.vm_functions.g_run_frame_orig.load(Ordering::SeqCst);
        if g_run_frame_orig == 0 {
            return Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_RunFrame,
            ));
        }

        let g_run_frame_func = unsafe { std::mem::transmute(g_run_frame_orig) };
        Ok(g_run_frame_func)
    }

    fn g_addevent_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t, entity_event_t, c_int), QuakeLiveEngineError> {
        let g_addevent_orig = self.vm_functions.g_addevent_orig.load(Ordering::SeqCst);
        if g_addevent_orig == 0 {
            return Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_AddEvent,
            ));
        }

        let g_addevent_func = unsafe { std::mem::transmute(g_addevent_orig) };
        Ok(g_addevent_func)
    }

    pub(crate) fn g_free_entity_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t), QuakeLiveEngineError> {
        let g_free_entity_orig = self.vm_functions.g_free_entity_orig.load(Ordering::SeqCst);
        if g_free_entity_orig == 0 {
            return Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_FreeEntity,
            ));
        }

        let g_free_entity_func = unsafe { std::mem::transmute(g_free_entity_orig) };
        Ok(g_free_entity_func)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn launch_item_orig(
        &self,
    ) -> Result<
        extern "C" fn(*mut gitem_t, &mut vec3_t, &mut vec3_t) -> *mut gentity_t,
        QuakeLiveEngineError,
    > {
        let launch_item_orig = self.vm_functions.launch_item_orig.load(Ordering::SeqCst);
        if launch_item_orig == 0 {
            return Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::LaunchItem,
            ));
        }

        let launch_item_func = unsafe { std::mem::transmute(launch_item_orig) };
        Ok(launch_item_func)
    }

    pub(crate) fn touch_item_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t), QuakeLiveEngineError>
    {
        let touch_item_orig = self.vm_functions.touch_item_orig.load(Ordering::SeqCst);
        if touch_item_orig == 0 {
            return Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::Touch_Item,
            ));
        }

        let touch_item_func = unsafe { std::mem::transmute(touch_item_orig) };
        Ok(touch_item_func)
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait FindCVar {
    fn find_cvar(&self, name: &str) -> Option<CVar>;
}

impl FindCVar for QuakeLiveEngine {
    fn find_cvar(&self, name: &str) -> Option<CVar> {
        let Ok(original_func) = self.cvar_findvar_orig() else {
            return None;
        };
        let Ok(c_name) = CString::new(name) else {
            return None;
        };
        let cvar = original_func(c_name.as_ptr());
        CVar::try_from(cvar).ok()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait AddCommand {
    fn add_command(&self, cmd: &str, func: unsafe extern "C" fn());
}

impl AddCommand for QuakeLiveEngine {
    fn add_command(&self, cmd: &str, func: unsafe extern "C" fn()) {
        let Ok(c_cmd) = CString::new(cmd) else {
            return;
        };
        let Ok(detour) = self.cmd_addcommand_detour() else {
            return;
        };

        detour.call(c_cmd.as_ptr(), func);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SetModuleOffset {
    fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn());
}

impl SetModuleOffset for QuakeLiveEngine {
    fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn()) {
        let Ok(c_module_name) = CString::new(module_name) else {
            return;
        };
        let Ok(detour) = self.sys_setmoduleoffset_detour() else {
            return;
        };

        detour.call(c_module_name.as_ptr(), offset);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait InitGame {
    fn init_game(&self, level_time: i32, random_seed: i32, restart: i32);
}

impl InitGame for QuakeLiveEngine {
    fn init_game(&self, level_time: i32, random_seed: i32, restart: i32) {
        let Ok(original_func) = self.g_init_game_orig() else {
            return;
        };
        original_func(level_time, random_seed, restart);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ShutdownGame {
    fn shutdown_game(&self, restart: i32);
}

impl ShutdownGame for QuakeLiveEngine {
    fn shutdown_game(&self, restart: i32) {
        let Ok(original_func) = self.g_shutdown_game_orig() else {
            return;
        };
        original_func(restart);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ExecuteClientCommand {
    #[allow(clippy::needless_lifetimes)]
    fn execute_client_command<'a>(
        &self,
        client: Option<&'a mut Client>,
        cmd: &str,
        client_ok: bool,
    );
}

impl ExecuteClientCommand for QuakeLiveEngine {
    fn execute_client_command(&self, client: Option<&mut Client>, cmd: &str, client_ok: bool) {
        let Ok(detour) = self.sv_executeclientcommand_detour() else {
            return;
        };

        let Ok(c_command) = CString::new(cmd) else {
            return;
        };
        match client {
            Some(safe_client) => {
                detour.call(safe_client.client_t, c_command.as_ptr(), client_ok.into())
            }
            None => detour.call(std::ptr::null_mut(), c_command.as_ptr(), client_ok.into()),
        }
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SendServerCommand {
    fn send_server_command(&self, client: Option<Client>, command: &str);
}

impl SendServerCommand for QuakeLiveEngine {
    fn send_server_command(&self, client: Option<Client>, command: &str) {
        let Ok(detour) = self.sv_sendservercommand_detour() else {
            return;
        };
        let original_func: extern "C" fn(*const client_t, *const c_char, ...) =
            unsafe { std::mem::transmute(detour.trampoline()) };

        let Ok(c_command) = CString::new(command) else {
            return;
        };
        match client {
            Some(safe_client) => original_func(safe_client.client_t, c_command.as_ptr()),
            None => original_func(std::ptr::null(), c_command.as_ptr()),
        }
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ClientEnterWorld {
    fn client_enter_world(&self, client: &mut Client, cmd: *mut usercmd_t);
}

impl ClientEnterWorld for QuakeLiveEngine {
    fn client_enter_world(&self, client: &mut Client, cmd: *mut usercmd_t) {
        let Ok(detour) = self.sv_cliententerworld_detour() else {
            return;
        };

        detour.call(client.client_t, cmd);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SetConfigstring {
    fn set_configstring(&self, index: &u32, value: &str);
}

impl SetConfigstring for QuakeLiveEngine {
    fn set_configstring(&self, index: &u32, value: &str) {
        let Ok(c_value) = CString::new(value) else {
            return;
        };
        let Ok(c_index) = c_int::try_from(index.to_owned()) else {
            return;
        };
        let Ok(detour) = self.sv_setconfgistring_detour() else {
            return;
        };

        detour.call(c_index, c_value.as_ptr());
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ComPrintf {
    fn com_printf(&self, msg: &str);
}

impl ComPrintf for QuakeLiveEngine {
    fn com_printf(&self, msg: &str) {
        let Ok(detour) = self.com_printf_detour() else {
            return;
        };
        let original_func: extern "C" fn(*const c_char, ...) =
            unsafe { std::mem::transmute(detour.trampoline()) };

        let Ok(c_msg) = CString::new(msg) else {
            return;
        };
        original_func(c_msg.as_ptr());
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SpawnServer {
    fn spawn_server(&self, server: &str, kill_bots: bool);
}

impl SpawnServer for QuakeLiveEngine {
    fn spawn_server(&self, server: &str, kill_bots: bool) {
        let Ok(c_server) = CString::new(server) else {
            return;
        };
        let Ok(detour) = self.sv_spawnserver_detour() else {
            return;
        };

        detour.call(c_server.as_ptr(), kill_bots.into());
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait RunFrame {
    fn run_frame(&self, time: i32);
}

impl RunFrame for QuakeLiveEngine {
    fn run_frame(&self, time: i32) {
        let Ok(original_func) = self.g_run_frame_orig() else {
            return;
        };
        original_func(time);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ClientConnect {
    fn client_connect(&self, client_num: i32, first_time: bool, is_bot: bool) -> *const c_char;
}

impl ClientConnect for QuakeLiveEngine {
    fn client_connect(&self, client_num: i32, first_time: bool, is_bot: bool) -> *const c_char {
        let Ok(detour_guard) = self.vm_functions.client_connect_detour.read() else {
            return std::ptr::null();
        };

        let Some(ref detour) = *detour_guard else {
            return std::ptr::null();
        };

        detour.call(client_num, first_time.into(), is_bot.into())
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ClientSpawn {
    fn client_spawn(&self, ent: &mut GameEntity);
}

impl ClientSpawn for QuakeLiveEngine {
    fn client_spawn(&self, ent: &mut GameEntity) {
        let Ok(detour_guard) = self.vm_functions.client_spawn_detour.read() else {
            return;
        };

        let Some(ref detour) = *detour_guard else {
            return;
        };

        detour.call(ent.gentity_t);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait CmdArgs {
    fn cmd_args(&self) -> Option<String>;
}

impl CmdArgs for QuakeLiveEngine {
    fn cmd_args(&self) -> Option<String> {
        let Ok(original_func) = self.cmd_args_orig() else {
            return None;
        };
        let cmd_args = original_func();
        if cmd_args.is_null() {
            return None;
        }
        let cmd_args = unsafe { CStr::from_ptr(cmd_args) }.to_string_lossy();
        Some(cmd_args.to_string())
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait CmdArgc {
    fn cmd_argc(&self) -> i32;
}

impl CmdArgc for QuakeLiveEngine {
    fn cmd_argc(&self) -> i32 {
        let Ok(original_func) = self.cmd_argc_orig() else {
            return 0;
        };
        original_func()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait CmdArgv {
    fn cmd_argv(&self, argno: i32) -> Option<&'static str>;
}

impl CmdArgv for QuakeLiveEngine {
    fn cmd_argv(&self, argno: i32) -> Option<&'static str> {
        if argno < 0 {
            return None;
        }
        let Ok(original_func) = self.cmd_argv_orig() else {
            return None;
        };
        let cmd_argv = original_func(argno);
        if cmd_argv.is_null() {
            return None;
        }
        unsafe { CStr::from_ptr(cmd_argv).to_str().ok() }
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait GameAddEvent {
    fn game_add_event(&self, game_entity: &mut GameEntity, event: entity_event_t, event_param: i32);
}

impl GameAddEvent for QuakeLiveEngine {
    fn game_add_event(
        &self,
        game_entity: &mut GameEntity,
        event: entity_event_t,
        event_param: i32,
    ) {
        let Ok(original_func) = self.g_addevent_orig() else {
            return;
        };
        original_func(game_entity.gentity_t as *mut gentity_t, event, event_param);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait ConsoleCommand {
    fn execute_console_command(&self, cmd: &str);
}

impl ConsoleCommand for QuakeLiveEngine {
    fn execute_console_command(&self, cmd: &str) {
        let Ok(original_func) = self.cmd_executestring_orig() else {
            return;
        };
        let Ok(c_cmd) = CString::new(cmd) else {
            return;
        };
        original_func(c_cmd.as_ptr());
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait GetCVar {
    fn get_cvar(&self, name: &str, value: &str, flags: Option<i32>) -> Option<CVar>;
}

impl GetCVar for QuakeLiveEngine {
    fn get_cvar(&self, name: &str, value: &str, flags: Option<i32>) -> Option<CVar> {
        let Ok(original_func) = self.cvar_get_orig() else {
            return None;
        };
        let Ok(c_name) = CString::new(name) else {
            return None;
        };
        let Ok(c_value) = CString::new(value) else {
            return None;
        };
        let flags_value = flags.unwrap_or_default();
        let cvar = original_func(c_name.as_ptr(), c_value.as_ptr(), flags_value);
        CVar::try_from(cvar).ok()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SetCVarForced {
    fn set_cvar_forced(&self, name: &str, value: &str, forced: bool) -> Option<CVar>;
}

impl SetCVarForced for QuakeLiveEngine {
    fn set_cvar_forced(&self, name: &str, value: &str, forced: bool) -> Option<CVar> {
        let Ok(original_func) = self.cvar_set2_orig() else {
            return None;
        };
        let Ok(c_name) = CString::new(name) else {
            return None;
        };
        let Ok(c_value) = CString::new(value) else {
            return None;
        };
        let cvar = original_func(c_name.as_ptr(), c_value.as_ptr(), forced.into());
        CVar::try_from(cvar).ok()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait SetCVarLimit {
    fn set_cvar_limit(
        &self,
        name: &str,
        value: &str,
        min: &str,
        max: &str,
        flags: Option<i32>,
    ) -> Option<CVar>;
}

impl SetCVarLimit for QuakeLiveEngine {
    fn set_cvar_limit(
        &self,
        name: &str,
        value: &str,
        min: &str,
        max: &str,
        flags: Option<i32>,
    ) -> Option<CVar> {
        let Ok(original_func) = self.cvar_getlimit_orig() else {
            return None;
        };
        let Ok(c_name) = CString::new(name) else {
            return None;
        };
        let Ok(c_value) = CString::new(value) else {
            return None;
        };
        let Ok(c_min) = CString::new(min) else {
            return None;
        };
        let Ok(c_max) = CString::new(max) else {
            return None;
        };
        let flags_value = flags.unwrap_or_default();
        let cvar = original_func(
            c_name.as_ptr(),
            c_value.as_ptr(),
            c_min.as_ptr(),
            c_max.as_ptr(),
            flags_value,
        );
        CVar::try_from(cvar).ok()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait GetConfigstring {
    fn get_configstring(&self, index: u32) -> String;
}

impl GetConfigstring for QuakeLiveEngine {
    fn get_configstring(&self, index: u32) -> String {
        let Ok(original_func) = self.sv_getconfigstring_orig() else {
            return "".into();
        };

        let mut buffer: [u8; MAX_STRING_CHARS as usize] = [0; MAX_STRING_CHARS as usize];
        original_func(
            index as c_int,
            buffer.as_mut_ptr() as *mut c_char,
            buffer.len() as c_int,
        );
        let Ok(result) = CStr::from_bytes_until_nul(&buffer) else {
            return "".into();
        };
        result.to_string_lossy().into()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait RegisterDamage {
    #[allow(clippy::too_many_arguments)]
    fn register_damage(
        &self,
        target: *mut gentity_t,
        inflictor: *mut gentity_t,
        attacker: *mut gentity_t,
        dir: *mut vec3_t,
        pos: *mut vec3_t,
        damage: c_int,
        dflags: c_int,
        means_of_death: c_int,
    );
}

impl RegisterDamage for QuakeLiveEngine {
    fn register_damage(
        &self,
        target: *mut gentity_t,
        inflictor: *mut gentity_t,
        attacker: *mut gentity_t,
        dir: *mut vec3_t,
        pos: *mut vec3_t,
        damage: c_int,
        dflags: c_int,
        means_of_death: c_int,
    ) {
        let Ok(detour_guard) = self.vm_functions.g_damage_detour.read() else {
            return;
        };

        let Some(ref detour) = *detour_guard else {
            return;
        };

        detour.call(
            target,
            inflictor,
            attacker,
            dir,
            pos,
            damage,
            dflags,
            means_of_death,
        );
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait FreeEntity {
    fn free_entity(&self, gentity: *mut gentity_t);
}

impl FreeEntity for QuakeLiveEngine {
    fn free_entity(&self, gentity: *mut gentity_t) {
        let Ok(original_func) = self.g_free_entity_orig() else {
            return;
        };
        original_func(gentity);
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait LaunchItem {
    fn launch_item(
        &self,
        gitem: &mut GameItem,
        origin: &mut vec3_t,
        velocity: &mut vec3_t,
    ) -> GameEntity;
}

impl LaunchItem for QuakeLiveEngine {
    fn launch_item(
        &self,
        gitem: &mut GameItem,
        origin: &mut vec3_t,
        velocity: &mut vec3_t,
    ) -> GameEntity {
        let Ok(original_func) = self.launch_item_orig() else {
            debug_println!("LaunchItem not found!");
            panic!("LaunchItem not found!");
        };
        GameEntity::try_from(original_func(gitem.gitem_t, origin, velocity)).unwrap()
    }
}

#[cfg_attr(test, automock)]
pub(crate) trait StartKamikaze {
    fn start_kamikaze(&self, gentity: &mut GameEntity);
}

impl StartKamikaze for QuakeLiveEngine {
    fn start_kamikaze(&self, gentity: &mut GameEntity) {
        let Ok(detour_guard) = self.vm_functions.g_start_kamikaze_detour.read() else {
            return;
        };

        let Some(ref detour) = *detour_guard else {
            return;
        };

        detour.call(gentity.gentity_t as *mut gentity_t);
    }
}
