use crate::commands::{
    cmd_center_print, cmd_py_command, cmd_py_rcon, cmd_regular_print, cmd_restart_python,
    cmd_send_server_command, cmd_slap, cmd_slay,
};
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;
use crate::hooks::{
    shinqlx_client_connect, shinqlx_clientspawn, shinqlx_cmd_addcommand, shinqlx_g_damage,
    shinqlx_g_initgame, shinqlx_g_runframe, shinqlx_g_shutdowngame, shinqlx_g_startkamikaze,
    shinqlx_sv_cliententerworld, shinqlx_sv_dropclient, shinqlx_sv_executeclientcommand,
    shinqlx_sv_setconfigstring, shinqlx_sv_spawnserver, shinqlx_sys_setmoduleoffset,
    ShiNQlx_Com_Printf, ShiNQlx_SV_SendServerCommand,
};
#[cfg(feature = "patches")]
use crate::patches::patch_callvote_f;
use crate::prelude::*;
#[cfg(target_os = "linux")]
use crate::quake_live_functions::pattern_search_module;
use crate::quake_live_functions::QuakeLiveFunction;
#[cfg(target_os = "linux")]
use crate::QZERODED;

use alloc::{ffi::CString, sync::Arc};
use arc_swap::ArcSwapOption;
#[cfg(target_os = "linux")]
use arrayvec::ArrayVec;
use core::{
    ffi::{c_char, c_int, CStr},
    sync::atomic::{AtomicI32, AtomicUsize, Ordering},
};
use once_cell::{race::OnceBool, sync::OnceCell};
#[cfg(target_os = "linux")]
use procfs::process::{MMapPath, MemoryMap, Process};
use retour::{GenericDetour, RawDetour};

#[allow(dead_code)]
#[cfg(target_pointer_width = "64")]
const QAGAME: &str = "qagamex64.so";
#[allow(dead_code)]
#[cfg(target_pointer_width = "32")]
const QAGAME: &str = "qagamei386.so";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum QuakeLiveEngineError {
    NullPointerPassed(String),
    EntityNotFound(String),
    InvalidId(i32),
    ClientNotFound(String),
    ProcessNotFound(String),
    #[cfg(target_os = "linux")]
    NoMemoryMappingInformationFound(String),
    StaticFunctionNotFound(QuakeLiveFunction),
    #[cfg_attr(test, allow(dead_code))]
    PythonInitializationFailed(PythonInitializationError),
    DetourCouldNotBeCreated(QuakeLiveFunction),
    DetourCouldNotBeEnabled(QuakeLiveFunction),
    StaticDetourNotFound(QuakeLiveFunction),
    VmFunctionNotFound(QuakeLiveFunction),
    MainEngineNotInitialized,
}

#[derive(Debug)]
struct StaticFunctions {
    #[cfg_attr(test, allow(dead_code))]
    com_printf_orig: extern "C" fn(*const c_char, ...),
    #[cfg_attr(test, allow(dead_code))]
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
    #[cfg_attr(test, allow(dead_code))]
    sv_sendservercommand_orig: extern "C" fn(*mut client_t, *const c_char, ...),
    #[cfg_attr(test, allow(dead_code))]
    sv_executeclientcommand_orig: fn(*mut client_t, *const c_char, qboolean),
    #[cfg_attr(test, allow(dead_code))]
    sv_shutdown_orig: fn(*const c_char),
    sv_map_f_orig: fn(),
    #[cfg_attr(test, allow(dead_code))]
    sv_cliententerworld_orig: fn(*mut client_t, *mut usercmd_t),
    #[cfg_attr(test, allow(dead_code))]
    sv_setconfigstring_orig: fn(c_int, *const c_char),
    sv_getconfigstring_orig: fn(c_int, *const c_char, c_int),
    #[cfg_attr(test, allow(dead_code))]
    sv_dropclient_orig: fn(*mut client_t, *const c_char),
    #[cfg_attr(test, allow(dead_code))]
    sys_setmoduleoffset_orig: fn(*const c_char, unsafe extern "C" fn()),
    #[cfg_attr(test, allow(dead_code))]
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
    #[cfg_attr(test, allow(dead_code))]
    sv_dropclient_detour: GenericDetour<fn(*mut client_t, *const c_char)>,
    sv_spawnserver_detour: GenericDetour<fn(*const c_char, qboolean)>,
    sv_sendservercommand_detour: RawDetour,
    com_printf_detour: RawDetour,
}

type ClientSpawnDetourType = GenericDetour<fn(*mut gentity_t)>;
type ClientConnectDetourType = GenericDetour<fn(c_int, qboolean, qboolean) -> *const c_char>;
type GStartKamikazeDetourType = GenericDetour<fn(*mut gentity_t)>;
type GDamageDetourType = GenericDetour<
    fn(
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

#[cfg_attr(test, allow(dead_code))]
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
    #[cfg(feature = "patches")]
    cmd_callvote_f_orig: AtomicUsize,

    client_spawn_detour: Arc<ArcSwapOption<ClientSpawnDetourType>>,
    client_connect_detour: Arc<ArcSwapOption<ClientConnectDetourType>>,
    g_start_kamikaze_detour: Arc<ArcSwapOption<GStartKamikazeDetourType>>,
    g_damage_detour: Arc<ArcSwapOption<GDamageDetourType>>,
}

#[allow(dead_code)]
const OFFSET_VM_CALL_TABLE: usize = 0x3;
#[allow(dead_code)]
const OFFSET_INITGAME: usize = 0x18;
#[allow(dead_code)]
const OFFSET_RUNFRAME: usize = 0x8;

impl VmFunctions {
    #[cfg_attr(test, allow(dead_code))]
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
                .iter()
                .filter(|mmap| {
                    let MMapPath::Path(path) = &mmap.pathname else {
                        return false;
                    };
                    path.file_name()
                        .is_some_and(|file_name| file_name.to_string_lossy() == QAGAME)
                })
                .collect();

            debug!(target: "shinqlx", "Searching for necessary VM functions...");
            let mut failed_functions: ArrayVec<QuakeLiveFunction, 11> = ArrayVec::new();
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
                #[cfg(feature = "patches")]
                (QuakeLiveFunction::Cmd_Callvote_f, &self.cmd_callvote_f_orig),
            ]
            .iter()
            .for_each(|(ql_func, field)| {
                match pattern_search_module(&qagame_maps, ql_func) {
                    None => failed_functions.push(*ql_func),
                    Some(orig_func) => {
                        debug!(target: "shinqlx", "{}: {:#X}", ql_func, orig_func);
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
                ptr::read_unaligned(
                    (module_offset as u64 + OFFSET_VM_CALL_TABLE as u64) as *const i32,
                )
            };
            let vm_call_table = base_address as usize + module_offset + OFFSET_VM_CALL_TABLE + 4;
            self.vm_call_table.store(vm_call_table, Ordering::SeqCst);

            let g_initgame_orig = unsafe {
                ptr::read(
                    (vm_call_table + OFFSET_INITGAME)
                        as *const *const extern "C" fn(c_int, c_int, c_int),
                )
            };
            debug!(target: "shinqlx", "G_InitGame: {:#X}", g_initgame_orig as usize);
            self.g_init_game_orig
                .store(g_initgame_orig as usize, Ordering::SeqCst);

            let g_shutdowngame_orig =
                unsafe { ptr::read_unaligned(vm_call_table as *const *const extern "C" fn(c_int)) };
            debug!(target: "shinqlx", "G_ShutdownGame: {:#X}", g_shutdowngame_orig as usize);
            self.g_shutdown_game_orig
                .store(g_shutdowngame_orig as usize, Ordering::SeqCst);

            let g_runframe_orig = unsafe {
                ptr::read((vm_call_table + OFFSET_RUNFRAME) as *const *const extern "C" fn(c_int))
            };
            debug!(target: "shinqlx", "G_RunFrame: {:#X}", g_runframe_orig as usize);
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
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn hook(&self) -> Result<(), QuakeLiveEngineError> {
        let vm_call_table = self.vm_call_table.load(Ordering::SeqCst);

        debug!(target: "shinqlx", "Hooking VM functions...");
        unsafe {
            ptr::write(
                (vm_call_table + 0x18) as *mut usize,
                shinqlx_g_initgame as usize,
            );
        }

        unsafe {
            ptr::write(
                (vm_call_table + 0x8) as *mut usize,
                shinqlx_g_runframe as usize,
            );
        }

        unsafe {
            ptr::write(vm_call_table as *mut usize, shinqlx_g_shutdowngame as usize);
        }

        let client_connect_orig = self.client_connect_orig.load(Ordering::SeqCst);
        let client_connect_func = unsafe { mem::transmute(client_connect_orig) };
        let client_connect_detour =
            unsafe { ClientConnectDetourType::new(client_connect_func, shinqlx_client_connect) }
                .map_err(|_| {
                    QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::ClientConnect)
                })?;
        unsafe { client_connect_detour.enable() }.map_err(|_| {
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::ClientConnect)
        })?;

        self.client_connect_detour
            .store(Some(client_connect_detour.into()));

        let g_start_kamikaze_orig = self.g_start_kamikaze_orig.load(Ordering::SeqCst);
        let g_start_kamikaze_func = unsafe { mem::transmute(g_start_kamikaze_orig) };
        let g_start_kamikaze_detour = unsafe {
            GStartKamikazeDetourType::new(g_start_kamikaze_func, shinqlx_g_startkamikaze)
        }
        .map_err(|_| {
            QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::G_StartKamikaze)
        })?;
        unsafe { g_start_kamikaze_detour.enable() }.map_err(|_| {
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::G_StartKamikaze)
        })?;

        self.g_start_kamikaze_detour
            .store(Some(g_start_kamikaze_detour.into()));

        let client_spawn_orig = self.client_spawn_orig.load(Ordering::SeqCst);
        let client_spawn_func = unsafe { mem::transmute(client_spawn_orig) };
        let client_spawn_detour =
            unsafe { ClientSpawnDetourType::new(client_spawn_func, shinqlx_clientspawn) }.map_err(
                |_| QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::ClientSpawn),
            )?;
        unsafe { client_spawn_detour.enable() }.map_err(|_| {
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::ClientSpawn)
        })?;

        self.client_spawn_detour
            .store(Some(client_spawn_detour.into()));

        let g_damage_orig = self.g_damage_orig.load(Ordering::SeqCst);
        let g_damage_func = unsafe { mem::transmute(g_damage_orig) };
        let g_damage_detour = unsafe { GDamageDetourType::new(g_damage_func, shinqlx_g_damage) }
            .map_err(|_| {
                QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::G_Damage)
            })?;
        unsafe { g_damage_detour.enable() }.map_err(|_| {
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::G_Damage)
        })?;

        self.g_damage_detour.store(Some(g_damage_detour.into()));

        Ok(())
    }

    #[cfg(feature = "patches")]
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn patch(&self) {
        let cmd_callvote_f_orig = self.cmd_callvote_f_orig.load(Ordering::SeqCst);
        if cmd_callvote_f_orig == 0 {
            return;
        }

        patch_callvote_f(cmd_callvote_f_orig);
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn unhook(&self) {
        [
            &self.vm_call_table,
            &self.g_addevent_orig,
            &self.check_privileges_orig,
            &self.client_connect_orig,
            &self.client_spawn_orig,
            &self.g_damage_orig,
            &self.touch_item_orig,
            &self.launch_item_orig,
            &self.drop_item_orig,
            &self.g_start_kamikaze_orig,
            &self.g_free_entity_orig,
            &self.g_init_game_orig,
            &self.g_run_frame_orig,
            #[cfg(feature = "patches")]
            &self.cmd_callvote_f_orig,
        ]
        .iter()
        .for_each(|field| {
            field.store(0, Ordering::SeqCst);
        });

        self.client_connect_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling detour: {}", e);
                }
            });

        self.g_start_kamikaze_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling detour: {}", e);
                }
            });

        self.client_spawn_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling detour: {}", e);
                }
            });

        self.g_damage_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling detour: {}", e);
                }
            });
    }
}

pub(crate) struct QuakeLiveEngine {
    static_functions: OnceCell<StaticFunctions>,
    static_detours: OnceCell<StaticDetours>,

    pub(crate) sv_maxclients: AtomicI32,
    #[cfg_attr(test, allow(dead_code))]
    common_initialized: OnceBool,

    vm_functions: VmFunctions,
    #[cfg_attr(test, allow(dead_code))]
    current_vm: AtomicUsize,
}

#[cfg(target_os = "linux")]
const OFFSET_CMD_ARGC: i32 = 0x81;

impl QuakeLiveEngine {
    #[cfg_attr(test, allow(dead_code))]
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
                #[cfg(feature = "patches")]
                cmd_callvote_f_orig: Default::default(),
                client_spawn_detour: ArcSwapOption::empty().into(),
                client_connect_detour: ArcSwapOption::empty().into(),
                g_start_kamikaze_detour: ArcSwapOption::empty().into(),
                g_damage_detour: ArcSwapOption::empty().into(),
            },
            current_vm: AtomicUsize::new(0),
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn search_static_functions(&self) -> Result<(), QuakeLiveEngineError> {
        #[cfg(not(target_os = "linux"))]
        return Err(QuakeLiveEngineError::ProcessNotFound(
            "could not find my own process\n".into(),
        ));
        #[cfg(target_os = "linux")]
        {
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
                .iter()
                .filter(|mmap| {
                    let MMapPath::Path(path) = &mmap.pathname else {
                        return false;
                    };
                    path.file_name()
                        .is_some_and(|file_name| file_name.to_string_lossy() == QZERODED)
                })
                .collect();

            if qzeroded_maps.is_empty() {
                error!(target: "shinqlx", "no memory mapping information for {} found", QZERODED);
                return Err(QuakeLiveEngineError::NoMemoryMappingInformationFound(
                    "no memory mapping information found\n".into(),
                ));
            }

            debug!(target: "shinqlx", "Searching for necessary functions...");
            let Some(result) = pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Com_Printf)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::Com_Printf);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Com_Printf,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Com_Printf, result);
            let com_printf_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cmd_AddCommand)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::Cmd_AddCommand);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_AddCommand,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cmd_AddCommand, result);
            let cmd_addcommand_orig = unsafe { mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cmd_Args)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::Cmd_Args);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Args,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cmd_Args, result);
            let cmd_args_orig = unsafe { mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cmd_Argv)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::Cmd_Argv);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Argv,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cmd_Argv, result);
            let cmd_argv_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cmd_Tokenizestring)
            else {
                error!(target: "shinqlx",
                    "Function {} not found",
                    &QuakeLiveFunction::Cmd_Tokenizestring
                );
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Tokenizestring,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cmd_Tokenizestring, result);
            let cmd_tokenizestring_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cbuf_ExecuteText)
            else {
                error!(target: "shinqlx",
                    "Function {} not found",
                    &QuakeLiveFunction::Cbuf_ExecuteText
                );
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cbuf_ExecuteText,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cbuf_ExecuteText, result);
            let cbuf_executetext_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cvar_FindVar)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::Cvar_FindVar);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_FindVar,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cvar_FindVar, result);
            let cvar_findvar_orig = unsafe { mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cvar_Get)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::Cvar_Get);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_Get,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cvar_Get, result);
            let cvar_get_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cvar_GetLimit)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::Cvar_GetLimit);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_GetLimit,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cvar_GetLimit, result);
            let cvar_getlimit_orig = unsafe { mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cvar_Set2)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::Cvar_Set2);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_Set2,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cvar_Set2, result);
            let cvar_set2_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::SV_SendServerCommand)
            else {
                error!(target: "shinqlx",
                    "Function {} not found",
                    &QuakeLiveFunction::SV_SendServerCommand
                );
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_SendServerCommand,
                ));
            };
            debug!(target: "shinqlx",
                "{}: {:#X}",
                &QuakeLiveFunction::SV_SendServerCommand,
                result
            );
            let sv_sendservercommand_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::SV_ExecuteClientCommand)
            else {
                error!(target: "shinqlx",
                    "Function {} not found",
                    &QuakeLiveFunction::SV_ExecuteClientCommand
                );
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_ExecuteClientCommand,
                ));
            };
            debug!(target: "shinqlx",
                "{}: {:#X}",
                &QuakeLiveFunction::SV_ExecuteClientCommand,
                result
            );
            let sv_executeclientcommand_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::SV_Shutdown)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::SV_Shutdown);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_Shutdown,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::SV_Shutdown, result);
            let sv_shutdown_orig = unsafe { mem::transmute(result) };

            let Some(result) = pattern_search_module(&qzeroded_maps, QuakeLiveFunction::SV_Map_f)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::SV_Map_f);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_Map_f,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", QuakeLiveFunction::SV_Map_f, result);
            let sv_map_f_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::SV_ClientEnterWorld)
            else {
                error!(target: "shinqlx",
                    "Function {} not found",
                    &QuakeLiveFunction::SV_ClientEnterWorld
                );
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_ClientEnterWorld,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::SV_ClientEnterWorld, result);
            let sv_cliententerworld_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::SV_SetConfigstring)
            else {
                error!(target: "shinqlx",
                    "Function {} not found",
                    &QuakeLiveFunction::SV_SetConfigstring
                );
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_SetConfigstring,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::SV_SetConfigstring, result);
            let sv_setconfigstring_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::SV_GetConfigstring)
            else {
                error!(target: "shinqlx",
                    "Function {} not found",
                    &QuakeLiveFunction::SV_GetConfigstring
                );
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_GetConfigstring,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::SV_GetConfigstring, result);
            let sv_getconfigstring_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::SV_DropClient)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::SV_DropClient);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_DropClient,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::SV_DropClient, result);
            let sv_dropclient_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Sys_SetModuleOffset)
            else {
                error!(target: "shinqlx",
                    "Function {} not found",
                    &QuakeLiveFunction::Sys_SetModuleOffset
                );
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Sys_SetModuleOffset,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Sys_SetModuleOffset, result);
            let sys_setmoduleoffset_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::SV_SpawnServer)
            else {
                error!(target: "shinqlx", "Function {} not found", &QuakeLiveFunction::SV_SpawnServer);
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_SpawnServer,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::SV_SpawnServer, result);
            let sv_spawnserver_orig = unsafe { mem::transmute(result) };

            let Some(result) =
                pattern_search_module(&qzeroded_maps, QuakeLiveFunction::Cmd_ExecuteString)
            else {
                error!(target: "shinqlx",
                    "Function {} not found",
                    &QuakeLiveFunction::Cmd_ExecuteString
                );
                return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_ExecuteString,
                ));
            };
            debug!(target: "shinqlx", "{}: {:#X}", &QuakeLiveFunction::Cmd_ExecuteString, result);
            let cmd_executestring_orig = unsafe { mem::transmute(result) };

            // Cmd_Argc is really small, making it hard to search for, so we use a reference to it instead.
            let base_address = unsafe {
                ptr::read_unaligned(
                    (sv_map_f_orig as usize + OFFSET_CMD_ARGC as usize) as *const i32,
                )
            };
            #[allow(clippy::fn_to_numeric_cast_with_truncation)]
            let cmd_argc_ptr = base_address + sv_map_f_orig as i32 + OFFSET_CMD_ARGC + 4;
            debug!(target: "shinqlx", "{}: {:#X}", QuakeLiveFunction::Cmd_Argc, cmd_argc_ptr);
            let cmd_argc_orig = unsafe { mem::transmute(cmd_argc_ptr as u64) };

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

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn hook_static(&self) -> Result<(), QuakeLiveEngineError> {
        debug!(target: "shinqlx", "Hooking...");
        let cmd_addcommand_detour = QuakeLiveFunction::Cmd_AddCommand
            .create_and_enable_generic_detour(
                self.cmd_addcommand_orig()?,
                shinqlx_cmd_addcommand,
            )?;

        let sys_setmoduleoffset_detour = QuakeLiveFunction::Sys_SetModuleOffset
            .create_and_enable_generic_detour(
                self.sys_setmoduleoffset_orig()?,
                shinqlx_sys_setmoduleoffset,
            )?;

        let sv_executeclientcommand_detour = QuakeLiveFunction::SV_ExecuteClientCommand
            .create_and_enable_generic_detour(
                self.sv_executeclientcommand_orig()?,
                shinqlx_sv_executeclientcommand,
            )?;

        let sv_cliententerworld_detour = QuakeLiveFunction::SV_ClientEnterWorld
            .create_and_enable_generic_detour(
                self.sv_cliententerworld_orig()?,
                shinqlx_sv_cliententerworld,
            )?;

        let sv_sendservercommand_detour = unsafe {
            RawDetour::new(
                self.sv_sendservercommand_orig()? as *const (),
                ShiNQlx_SV_SendServerCommand as *const (),
            )
            .map_err(|_| {
                QuakeLiveEngineError::DetourCouldNotBeCreated(
                    QuakeLiveFunction::SV_SendServerCommand,
                )
            })?
        };
        unsafe {
            sv_sendservercommand_detour.enable().map_err(|_| {
                QuakeLiveEngineError::DetourCouldNotBeEnabled(
                    QuakeLiveFunction::SV_SendServerCommand,
                )
            })?
        };

        let sv_setconfgistring_detour = QuakeLiveFunction::SV_SetConfigstring
            .create_and_enable_generic_detour(
                self.sv_setconfigstring_orig()?,
                shinqlx_sv_setconfigstring,
            )?;

        let sv_dropclient_detour = QuakeLiveFunction::SV_DropClient
            .create_and_enable_generic_detour(self.sv_dropclient_orig()?, shinqlx_sv_dropclient)?;

        let com_printf_detour = unsafe {
            RawDetour::new(
                self.com_printf_orig()? as *const (),
                ShiNQlx_Com_Printf as *const (),
            )
            .map_err(|_| {
                QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::Com_Printf)
            })?
        };
        unsafe {
            com_printf_detour.enable().map_err(|_| {
                QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::Com_Printf)
            })?
        };

        let sv_spawnserver_detour = QuakeLiveFunction::SV_SpawnServer
            .create_and_enable_generic_detour(
                self.sv_spawnserver_orig()?,
                shinqlx_sv_spawnserver,
            )?;

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

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn set_tag(&self) {
        const SV_TAGS_PREFIX: &str = "shinqlx";

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
        self.set_cvar_forced("sv_tags", new_tags, false);
    }

    // Called after the game is initialized.
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn initialize_cvars(&self) {
        let Some(maxclients) = self.find_cvar("sv_maxclients") else {
            return;
        };

        self.sv_maxclients
            .store(maxclients.get_integer(), Ordering::SeqCst);
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn get_max_clients(&self) -> i32 {
        self.sv_maxclients.load(Ordering::SeqCst)
    }

    // Currently called by My_Cmd_AddCommand(), since it's called at a point where we
    // can safely do whatever we do below. It'll segfault if we do it at the entry
    // point, since functions like Cmd_AddCommand need initialization first.
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn initialize_static(&self) -> Result<(), QuakeLiveEngineError> {
        debug!(target: "shinqlx", "Initializing...");
        self.add_command("cmd", cmd_send_server_command);
        self.add_command("cp", cmd_center_print);
        self.add_command("print", cmd_regular_print);
        self.add_command("slap", cmd_slap);
        self.add_command("slay", cmd_slay);
        self.add_command("qlx", cmd_py_rcon);
        self.add_command("pycmd", cmd_py_command);
        self.add_command("pyrestart", cmd_restart_python);

        if let Err(err) = pyshinqlx_initialize() {
            error!(target: "shinqlx", "Python initialization failed.");
            return Err(QuakeLiveEngineError::PythonInitializationFailed(err));
        };

        self.common_initialized.set(true).unwrap();
        Ok(())
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn is_common_initialized(&self) -> bool {
        self.common_initialized
            .get()
            .is_some_and(|is_initialized| is_initialized)
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn initialize_vm(&self, module_offset: usize) -> Result<(), QuakeLiveEngineError> {
        self.vm_functions.try_initialize_from(module_offset)?;
        self.current_vm.store(module_offset, Ordering::SeqCst);

        self.vm_functions.hook()?;
        #[cfg(feature = "patches")]
        self.vm_functions.patch();

        Ok(())
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn unhook_vm(&self, restart: bool) {
        if !restart {
            self.vm_functions.unhook();
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    fn com_printf_orig(&self) -> Result<extern "C" fn(*const c_char, ...), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Com_Printf,
            ));
        };
        Ok(static_functions.com_printf_orig)
    }

    #[cfg_attr(test, allow(dead_code))]
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

    #[cfg_attr(test, allow(dead_code))]
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

    #[cfg_attr(test, allow(dead_code))]
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

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn sv_shutdown_orig(&self) -> Result<fn(*const c_char), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_Shutdown,
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

    #[cfg_attr(test, allow(dead_code))]
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

    #[cfg_attr(test, allow(dead_code))]
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

    #[cfg_attr(test, allow(dead_code))]
    fn sv_dropclient_orig(&self) -> Result<fn(*mut client_t, *const c_char), QuakeLiveEngineError> {
        let Some(static_functions) = self.static_functions.get() else {
            return Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_DropClient,
            ));
        };
        Ok(static_functions.sv_dropclient_orig)
    }

    #[cfg_attr(test, allow(dead_code))]
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

    #[cfg_attr(test, allow(dead_code))]
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

    #[cfg_attr(test, allow(dead_code))]
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

        let g_init_game_func = unsafe { mem::transmute(g_init_game_orig) };
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

        let g_shutdown_game_func = unsafe { mem::transmute(g_shutdown_game_orig) };
        Ok(g_shutdown_game_func)
    }

    pub(crate) fn g_run_frame_orig(&self) -> Result<extern "C" fn(c_int), QuakeLiveEngineError> {
        let g_run_frame_orig = self.vm_functions.g_run_frame_orig.load(Ordering::SeqCst);
        if g_run_frame_orig == 0 {
            return Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_RunFrame,
            ));
        }

        let g_run_frame_func = unsafe { mem::transmute(g_run_frame_orig) };
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

        let g_addevent_func = unsafe { mem::transmute(g_addevent_orig) };
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

        let g_free_entity_func = unsafe { mem::transmute(g_free_entity_orig) };
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

        let launch_item_func = unsafe { mem::transmute(launch_item_orig) };
        Ok(launch_item_func)
    }

    #[cfg_attr(test, allow(dead_code))]
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

        let touch_item_func = unsafe { mem::transmute(touch_item_orig) };
        Ok(touch_item_func)
    }
}

pub(crate) trait FindCVar<T: AsRef<str>> {
    fn find_cvar(&self, name: T) -> Option<CVar>;
}

impl<T: AsRef<str>> FindCVar<T> for QuakeLiveEngine {
    fn find_cvar(&self, name: T) -> Option<CVar> {
        let c_name = CString::new(name.as_ref()).ok()?;
        self.cvar_findvar_orig()
            .map(|original_func| original_func(c_name.as_ptr()))
            .and_then(CVar::try_from)
            .ok()
    }
}

pub(crate) trait AddCommand<T: AsRef<str>> {
    fn add_command(&self, cmd: T, func: unsafe extern "C" fn());
}

impl<T: AsRef<str>> AddCommand<T> for QuakeLiveEngine {
    fn add_command(&self, cmd: T, func: unsafe extern "C" fn()) {
        let Ok(c_cmd) = CString::new(cmd.as_ref()) else {
            return;
        };
        self.cmd_addcommand_detour()
            .iter()
            .for_each(|detour| detour.call(c_cmd.as_ptr(), func));
    }
}

pub(crate) trait SetModuleOffset<T: AsRef<str>> {
    fn set_module_offset(&self, module_name: T, offset: unsafe extern "C" fn());
}

impl<T: AsRef<str>> SetModuleOffset<T> for QuakeLiveEngine {
    fn set_module_offset(&self, module_name: T, offset: unsafe extern "C" fn()) {
        let Ok(c_module_name) = CString::new(module_name.as_ref()) else {
            return;
        };
        self.sys_setmoduleoffset_detour()
            .iter()
            .for_each(|detour| detour.call(c_module_name.as_ptr(), offset));
    }
}

pub(crate) trait InitGame<T: Into<c_int>, U: Into<c_int>, V: Into<c_int>> {
    fn init_game(&self, level_time: T, random_seed: U, restart: V);
}

impl<T: Into<c_int>, U: Into<c_int>, V: Into<c_int>> InitGame<T, U, V> for QuakeLiveEngine {
    fn init_game(&self, level_time: T, random_seed: U, restart: V) {
        let level_time_param = level_time.into();
        let random_seed_param = random_seed.into();
        let restart_param = restart.into();
        self.g_init_game_orig().iter().for_each(|original_func| {
            original_func(level_time_param, random_seed_param, restart_param)
        });
    }
}

pub(crate) trait ShutdownGame<T: Into<c_int>> {
    fn shutdown_game(&self, restart: T);
}

impl<T: Into<c_int>> ShutdownGame<T> for QuakeLiveEngine {
    fn shutdown_game(&self, restart: T) {
        let restart_param = restart.into();
        self.g_shutdown_game_orig()
            .iter()
            .for_each(|original_func| original_func(restart_param));
    }
}

pub(crate) trait ExecuteClientCommand<T: AsMut<client_t>, U: AsRef<str>, V: Into<qboolean>> {
    #[allow(clippy::needless_lifetimes)]
    fn execute_client_command(&self, client: Option<T>, cmd: U, client_ok: V);
}

impl<T: AsMut<client_t>, U: AsRef<str>, V: Into<qboolean>> ExecuteClientCommand<T, U, V>
    for QuakeLiveEngine
{
    fn execute_client_command(&self, mut client: Option<T>, cmd: U, client_ok: V) {
        let Ok(c_command) = CString::new(cmd.as_ref()) else {
            return;
        };

        let client_ok_param = client_ok.into();
        self.sv_executeclientcommand_detour()
            .iter()
            .for_each(|detour| match &mut client {
                Some(ref mut safe_client) => {
                    detour.call(safe_client.as_mut(), c_command.as_ptr(), client_ok_param)
                }
                None => detour.call(ptr::null_mut(), c_command.as_ptr(), client_ok_param),
            });
    }
}

pub(crate) trait SendServerCommand<T: AsRef<client_t>> {
    fn send_server_command(&self, client: Option<T>, command: &str);
}

impl<T: AsRef<client_t>> SendServerCommand<T> for QuakeLiveEngine {
    fn send_server_command(&self, client: Option<T>, command: &str) {
        let Ok(c_command) = CString::new(command) else {
            return;
        };

        self.sv_sendservercommand_detour()
            .map(|detour| unsafe { mem::transmute(detour.trampoline()) })
            .iter()
            .for_each(
                |original_func: &extern "C" fn(*const client_t, *const c_char, ...)| match &client {
                    Some(ref safe_client) => {
                        original_func(safe_client.as_ref(), c_command.as_ptr())
                    }
                    None => original_func(ptr::null(), c_command.as_ptr()),
                },
            );
    }
}

pub(crate) trait ClientEnterWorld<T: AsMut<client_t>> {
    fn client_enter_world(&self, client: T, cmd: *mut usercmd_t);
}

impl<T: AsMut<client_t>> ClientEnterWorld<T> for QuakeLiveEngine {
    fn client_enter_world(&self, mut client: T, cmd: *mut usercmd_t) {
        self.sv_cliententerworld_detour()
            .iter()
            .for_each(|detour| detour.call(client.as_mut(), cmd));
    }
}

pub(crate) trait SetConfigstring<T: Into<c_int>> {
    fn set_configstring(&self, index: T, value: &str);
}

impl<T: Into<c_int>> SetConfigstring<T> for QuakeLiveEngine {
    fn set_configstring(&self, index: T, value: &str) {
        let Ok(c_value) = CString::new(value) else {
            return;
        };
        let index_param = index.into();
        self.sv_setconfgistring_detour()
            .iter()
            .for_each(|detour| detour.call(index_param, c_value.as_ptr()));
    }
}

pub(crate) trait ComPrintf {
    fn com_printf(&self, msg: &str);
}

impl ComPrintf for QuakeLiveEngine {
    fn com_printf(&self, msg: &str) {
        let Ok(c_msg) = CString::new(msg) else {
            return;
        };
        self.com_printf_detour().iter().for_each(|detour| {
            let original_func: extern "C" fn(*const c_char, ...) =
                unsafe { mem::transmute(detour.trampoline()) };
            original_func(c_msg.as_ptr())
        });
    }
}

pub(crate) trait SpawnServer<T: AsRef<str>, U: Into<qboolean>> {
    fn spawn_server(&self, server: T, kill_bots: U);
}

impl<T: AsRef<str>, U: Into<qboolean>> SpawnServer<T, U> for QuakeLiveEngine {
    fn spawn_server(&self, server: T, kill_bots: U) {
        let Ok(c_server) = CString::new(server.as_ref()) else {
            return;
        };
        let kill_bots_param = kill_bots.into();
        self.sv_spawnserver_detour()
            .iter()
            .for_each(|detour| detour.call(c_server.as_ptr(), kill_bots_param));
    }
}

pub(crate) trait RunFrame<T: Into<c_int>> {
    fn run_frame(&self, time: T);
}

impl<T: Into<c_int>> RunFrame<T> for QuakeLiveEngine {
    fn run_frame(&self, time: T) {
        let time_param = time.into();
        self.g_run_frame_orig()
            .iter()
            .for_each(|original_func| original_func(time_param));
    }
}

pub(crate) trait ClientConnect<T: Into<c_int>, U: Into<qboolean>, V: Into<qboolean>> {
    fn client_connect(&self, client_num: T, first_time: U, is_bot: V) -> *const c_char;
}

impl<T: Into<c_int>, U: Into<qboolean>, V: Into<qboolean>> ClientConnect<T, U, V>
    for QuakeLiveEngine
{
    fn client_connect(&self, client_num: T, first_time: U, is_bot: V) -> *const c_char {
        self.vm_functions
            .client_connect_detour
            .load()
            .as_ref()
            .map(|detour| detour.call(client_num.into(), first_time.into(), is_bot.into()))
            .unwrap_or_else(ptr::null)
    }
}

pub(crate) trait ClientSpawn<T: AsMut<gentity_t>> {
    fn client_spawn(&self, ent: T);
}

impl<T: AsMut<gentity_t>> ClientSpawn<T> for QuakeLiveEngine {
    fn client_spawn(&self, mut ent: T) {
        self.vm_functions
            .client_spawn_detour
            .load()
            .iter()
            .for_each(|detour| detour.call(ent.as_mut()));
    }
}

pub(crate) trait CmdArgs {
    fn cmd_args(&self) -> Option<String>;
}

impl CmdArgs for QuakeLiveEngine {
    fn cmd_args(&self) -> Option<String> {
        self.cmd_args_orig()
            .ok()
            .map(|original_func| original_func())
            .filter(|cmd_args| !cmd_args.is_null())
            .map(|cmd_args| unsafe { CStr::from_ptr(cmd_args) }.to_string_lossy().into())
    }
}

pub(crate) trait CmdArgc {
    fn cmd_argc(&self) -> i32;
}

impl CmdArgc for QuakeLiveEngine {
    fn cmd_argc(&self) -> i32 {
        self.cmd_argc_orig()
            .map(|original_func| original_func())
            .unwrap_or(0)
    }
}

pub(crate) trait CmdArgv<T: Into<c_int>> {
    fn cmd_argv(&self, argno: T) -> Option<String>;
}

impl<T: Into<c_int> + PartialOrd<c_int>> CmdArgv<T> for QuakeLiveEngine {
    fn cmd_argv(&self, argno: T) -> Option<String> {
        if argno < 0 {
            return None;
        }
        self.cmd_argv_orig()
            .map(|original_func| original_func(argno.into()))
            .ok()
            .filter(|cmd_argv| !cmd_argv.is_null())
            .map(|cmd_argv| unsafe { CStr::from_ptr(cmd_argv) }.to_string_lossy().into())
    }
}

pub(crate) trait GameAddEvent<T: AsMut<gentity_t>, U: Into<c_int>> {
    fn game_add_event(&self, game_entity: T, event: entity_event_t, event_param: U);
}

impl<T: AsMut<gentity_t>, U: Into<c_int>> GameAddEvent<T, U> for QuakeLiveEngine {
    fn game_add_event(&self, mut game_entity: T, event: entity_event_t, event_param: U) {
        let event_param_param = event_param.into();
        self.g_addevent_orig().iter().for_each(|original_func| {
            original_func(game_entity.as_mut(), event, event_param_param)
        });
    }
}

pub(crate) trait ConsoleCommand<T: AsRef<str>> {
    fn execute_console_command(&self, cmd: T);
}

impl<T: AsRef<str>> ConsoleCommand<T> for QuakeLiveEngine {
    fn execute_console_command(&self, cmd: T) {
        let Ok(c_cmd) = CString::new(cmd.as_ref()) else {
            return;
        };
        self.cmd_executestring_orig()
            .iter()
            .for_each(|original_func| original_func(c_cmd.as_ptr()));
    }
}

pub(crate) trait GetCVar<T: AsRef<str>, U: AsRef<str>, V: Into<c_int>> {
    fn get_cvar(&self, name: T, value: U, flags: Option<V>) -> Option<CVar>;
}

impl<T: AsRef<str>, U: AsRef<str>, V: Into<c_int> + Default> GetCVar<T, U, V> for QuakeLiveEngine {
    fn get_cvar(&self, name: T, value: U, flags: Option<V>) -> Option<CVar> {
        let Ok(c_name) = CString::new(name.as_ref()) else {
            return None;
        };
        let Ok(c_value) = CString::new(value.as_ref()) else {
            return None;
        };
        self.cvar_get_orig()
            .map(|original_func| {
                original_func(
                    c_name.as_ptr(),
                    c_value.as_ptr(),
                    flags.unwrap_or_default().into(),
                )
            })
            .and_then(CVar::try_from)
            .ok()
    }
}

pub(crate) trait SetCVarForced<T: AsRef<str>, U: AsRef<str>, V: Into<qboolean>> {
    fn set_cvar_forced(&self, name: T, value: U, forced: V) -> Option<CVar>;
}

impl<T: AsRef<str>, U: AsRef<str>, V: Into<qboolean>> SetCVarForced<T, U, V> for QuakeLiveEngine {
    fn set_cvar_forced(&self, name: T, value: U, forced: V) -> Option<CVar> {
        let Ok(c_name) = CString::new(name.as_ref()) else {
            return None;
        };
        let Ok(c_value) = CString::new(value.as_ref()) else {
            return None;
        };
        self.cvar_set2_orig()
            .map(|original_func| original_func(c_name.as_ptr(), c_value.as_ptr(), forced.into()))
            .and_then(CVar::try_from)
            .ok()
    }
}

pub(crate) trait SetCVarLimit<
    T: AsRef<str>,
    U: AsRef<str>,
    V: AsRef<str>,
    W: AsRef<str>,
    X: Into<c_int>,
>
{
    fn set_cvar_limit(&self, name: T, value: U, min: V, max: W, flags: Option<X>) -> Option<CVar>;
}

impl<T: AsRef<str>, U: AsRef<str>, V: AsRef<str>, W: AsRef<str>, X: Into<c_int> + Default>
    SetCVarLimit<T, U, V, W, X> for QuakeLiveEngine
{
    fn set_cvar_limit(&self, name: T, value: U, min: V, max: W, flags: Option<X>) -> Option<CVar> {
        let Ok(c_name) = CString::new(name.as_ref()) else {
            return None;
        };
        let Ok(c_value) = CString::new(value.as_ref()) else {
            return None;
        };
        let Ok(c_min) = CString::new(min.as_ref()) else {
            return None;
        };
        let Ok(c_max) = CString::new(max.as_ref()) else {
            return None;
        };
        self.cvar_getlimit_orig()
            .map(|original_func| {
                original_func(
                    c_name.as_ptr(),
                    c_value.as_ptr(),
                    c_min.as_ptr(),
                    c_max.as_ptr(),
                    flags.unwrap_or_default().into(),
                )
            })
            .and_then(CVar::try_from)
            .ok()
    }
}

pub(crate) trait GetConfigstring<T: Into<c_int>> {
    fn get_configstring(&self, index: T) -> String;
}

impl<T: Into<c_int>> GetConfigstring<T> for QuakeLiveEngine {
    fn get_configstring(&self, index: T) -> String {
        self.sv_getconfigstring_orig()
            .map(|original_func| {
                let mut buffer: [u8; MAX_STRING_CHARS as usize] = [0; MAX_STRING_CHARS as usize];
                original_func(
                    index.into(),
                    buffer.as_mut_ptr() as *mut c_char,
                    buffer.len() as c_int,
                );
                CStr::from_bytes_until_nul(&buffer)
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into()
            })
            .unwrap_or("".into())
    }
}

pub(crate) trait RegisterDamage<T: Into<c_int>, U: Into<c_int>, V: Into<c_int>> {
    #[allow(clippy::too_many_arguments)]
    fn register_damage(
        &self,
        target: *mut gentity_t,
        inflictor: *mut gentity_t,
        attacker: *mut gentity_t,
        dir: *mut vec3_t,
        pos: *mut vec3_t,
        damage: T,
        dflags: U,
        means_of_death: V,
    );
}

impl<T: Into<c_int>, U: Into<c_int>, V: Into<c_int>> RegisterDamage<T, U, V> for QuakeLiveEngine {
    fn register_damage(
        &self,
        target: *mut gentity_t,
        inflictor: *mut gentity_t,
        attacker: *mut gentity_t,
        dir: *mut vec3_t,
        pos: *mut vec3_t,
        damage: T,
        dflags: U,
        means_of_death: V,
    ) {
        let damage_param = damage.into();
        let dflags_param = dflags.into();
        let means_of_death_param = means_of_death.into();
        self.vm_functions
            .g_damage_detour
            .load()
            .iter()
            .for_each(|detour| {
                detour.call(
                    target,
                    inflictor,
                    attacker,
                    dir,
                    pos,
                    damage_param,
                    dflags_param,
                    means_of_death_param,
                )
            });
    }
}

pub(crate) trait FreeEntity<T: AsMut<gentity_t>> {
    fn free_entity(&self, gentity: T);
}

impl<T: AsMut<gentity_t>> FreeEntity<T> for QuakeLiveEngine {
    fn free_entity(&self, mut gentity: T) {
        self.g_free_entity_orig()
            .iter()
            .for_each(|original_func| original_func(gentity.as_mut()))
    }
}

pub(crate) trait TryLaunchItem<T: AsMut<gitem_t>> {
    fn try_launch_item(
        &self,
        gitem: T,
        origin: &mut vec3_t,
        velocity: &mut vec3_t,
    ) -> Result<GameEntity, QuakeLiveEngineError>;
}

impl<T: AsMut<gitem_t>> TryLaunchItem<T> for QuakeLiveEngine {
    fn try_launch_item(
        &self,
        mut gitem: T,
        origin: &mut vec3_t,
        velocity: &mut vec3_t,
    ) -> Result<GameEntity, QuakeLiveEngineError> {
        self.launch_item_orig()
            .map(|original_func| original_func(gitem.as_mut(), origin, velocity))
            .and_then(GameEntity::try_from)
    }
}

pub(crate) trait StartKamikaze<T: AsMut<gentity_t> + ?Sized> {
    fn start_kamikaze(&self, gentity: T);
}

impl<T: AsMut<gentity_t>> StartKamikaze<T> for QuakeLiveEngine {
    fn start_kamikaze(&self, mut gentity: T) {
        self.vm_functions
            .g_start_kamikaze_detour
            .load()
            .iter()
            .for_each(|detour| detour.call(gentity.as_mut()));
    }
}

#[cfg(test)]
mockall::mock! {
    pub(crate) QuakeEngine{
        pub(crate) fn search_static_functions(&self) -> Result<(), QuakeLiveEngineError>;
        pub(crate) fn hook_static(&self) -> Result<(), QuakeLiveEngineError>;
        pub(crate) fn is_common_initialized(&self) -> bool;
        pub(crate) fn get_max_clients(&self) -> i32;
        pub(crate) fn initialize_static(&self) -> Result<(), QuakeLiveEngineError>;
        pub(crate) fn initialize_vm(&self, module_offset: usize) -> Result<(), QuakeLiveEngineError>;
        pub(crate) fn set_tag(&self);
        pub(crate) fn initialize_cvars(&self);
        pub(crate) fn unhook_vm(&self, restart: bool);
        pub(crate) fn g_init_game_orig(
            &self,
        ) -> Result<extern "C" fn(c_int, c_int, c_int), QuakeLiveEngineError>;
        pub(crate) fn touch_item_orig(
            &self,
        ) -> Result<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t), QuakeLiveEngineError>;
        pub(crate) fn g_free_entity_orig(
            &self,
        ) -> Result<extern "C" fn(*mut gentity_t), QuakeLiveEngineError>;
        pub(crate) fn g_run_frame_orig(&self) -> Result<extern "C" fn(c_int), QuakeLiveEngineError>;
        pub(crate) fn launch_item_orig(
            &self,
        ) -> Result<
            extern "C" fn(*mut gitem_t, &mut vec3_t, &mut vec3_t) -> *mut gentity_t,
            QuakeLiveEngineError,
        >;
        #[allow(clippy::type_complexity)]
        pub(crate) fn sv_dropclient_detour(&self) -> Result<&'static GenericDetour<fn(*mut client_t, *const c_char)>, QuakeLiveEngineError>;
        pub(crate) fn sv_shutdown_orig(&self) -> Result<fn(*const c_char), QuakeLiveEngineError>;
    }
    impl AddCommand<&str> for QuakeEngine {
        fn add_command(&self, cmd: &str, func: unsafe extern "C" fn());
    }
    impl SetModuleOffset<&str> for QuakeEngine {
        fn set_module_offset(&self, module_name: &str, offset: unsafe extern "C" fn());
    }
    impl InitGame<c_int, c_int, c_int> for QuakeEngine {
        fn init_game(&self, level_time: c_int, random_seed: c_int, restart: c_int);
    }
    impl ShutdownGame<c_int> for QuakeEngine {
        fn shutdown_game(&self, restart: c_int);
    }
    impl ExecuteClientCommand<Client, String, qboolean> for QuakeEngine {
        fn execute_client_command(&self, client: Option<Client>, cmd: String, client_ok: qboolean);
    }
    impl SendServerCommand<Client> for QuakeEngine {
        fn send_server_command(&self, client: Option <Client>, cmd: &str);
    }
    impl ClientEnterWorld<&mut Client> for QuakeEngine {
        fn client_enter_world(&self, client: &mut Client, cmd: * mut usercmd_t);
    }
    impl SetConfigstring<c_int> for QuakeEngine {
        fn set_configstring(&self, index: c_int, value: &str);
    }
    impl ComPrintf for QuakeEngine {
        fn com_printf(&self, msg: &str);
    }
    impl SpawnServer<&str, bool> for QuakeEngine {
        fn spawn_server(&self, server_str: &str, kill_bots: bool);
    }
    impl RunFrame<c_int> for QuakeEngine {
        fn run_frame(&self, time: c_int);
    }
    impl ClientConnect<c_int, bool, bool> for QuakeEngine {
        fn client_connect(&self, client_num: c_int, first_time: bool, is_bot: bool) -> *const c_char;
    }
    impl ClientSpawn<&mut GameEntity> for QuakeEngine {
        fn client_spawn(&self, ent: &mut GameEntity);
    }
    impl RegisterDamage<c_int, c_int, c_int> for QuakeEngine {
        #[allow(clippy::too_many_arguments)]
        fn register_damage(&self, target: *mut gentity_t, inflictor: *mut gentity_t, attacker: *mut gentity_t, dir: *mut vec3_t, pos: *mut vec3_t, damage: c_int, dflags: c_int, means_of_death: c_int);
    }
    impl TryLaunchItem<&mut crate::ffi::c::game_item::GameItem> for QuakeEngine {
        fn try_launch_item<'a>(&self, gitem: &'a mut crate::ffi::c::game_item::GameItem, origin: &mut vec3_t, velocity: &mut vec3_t) -> Result<GameEntity, QuakeLiveEngineError>;
    }
    impl GameAddEvent<&mut GameEntity, i32> for QuakeEngine {
        fn game_add_event(&self, game_entity: &mut GameEntity, event: entity_event_t, event_param: i32);
    }
    impl CmdArgs for QuakeEngine {
        fn cmd_args(&self) -> Option<String>;
    }
    impl CmdArgc for QuakeEngine {
        fn cmd_argc(&self) -> i32;
    }
    impl CmdArgv<i32> for QuakeEngine {
        fn cmd_argv(&self, argno: i32) -> Option<String>;
    }
    impl StartKamikaze<&mut crate::ffi::c::game_entity::GameEntity> for QuakeEngine {
        fn start_kamikaze(&self, mut gentity: &mut crate::ffi::c::game_entity::GameEntity);
    }
    impl FreeEntity<&mut crate::ffi::c::game_entity::GameEntity> for QuakeEngine {
        fn free_entity(&self, mut gentity: &mut crate::ffi::c::game_entity::GameEntity);
    }
    impl GetConfigstring<u16> for QuakeEngine {
        fn get_configstring(&self, index: u16) -> String;
    }
    impl ConsoleCommand<&str> for QuakeEngine {
        fn execute_console_command(&self, cmd: &str);
    }
    impl FindCVar<&str> for QuakeEngine {
        fn find_cvar(&self, name: &str) -> Option<CVar>;
    }
    impl GetCVar<&str, &str, i32> for QuakeEngine {
        fn get_cvar(&self, name: &str, value: &str, flags: Option<i32>) -> Option<CVar>;
    }
    impl SetCVarForced<&str, &str, bool> for QuakeEngine {
        fn set_cvar_forced(&self, name: &str, value: &str, forced: bool) -> Option<CVar>;
    }
    impl SetCVarLimit<&str, &str, &str, &str, i32> for QuakeEngine {
        fn set_cvar_limit(&self, name: &str, value: &str, min: &str, max: &str, flags: Option<i32>) -> Option<CVar>;
    }
}
