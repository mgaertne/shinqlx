#[cfg(target_os = "linux")]
use crate::QZERODED;
use crate::commands::{
    cmd_center_print, cmd_py_command, cmd_py_rcon, cmd_regular_print, cmd_restart_python,
    cmd_send_server_command, cmd_slap, cmd_slay,
};
use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;
use crate::hooks::{
    ShiNQlx_Com_Printf, ShiNQlx_SV_SendServerCommand, shinqlx_client_connect, shinqlx_clientspawn,
    shinqlx_cmd_addcommand, shinqlx_g_damage, shinqlx_g_initgame, shinqlx_g_runframe,
    shinqlx_g_shutdowngame, shinqlx_g_startkamikaze, shinqlx_sv_cliententerworld,
    shinqlx_sv_dropclient, shinqlx_sv_executeclientcommand, shinqlx_sv_setconfigstring,
    shinqlx_sv_spawnserver, shinqlx_sys_setmoduleoffset,
};
#[cfg(feature = "patches")]
use crate::patches::patch_callvote_f;
use crate::prelude::*;
use crate::quake_live_functions::QuakeLiveFunction;
#[cfg(target_os = "linux")]
use crate::quake_live_functions::pattern_search_module;

use alloc::ffi::CString;
use arc_swap::ArcSwapOption;
#[cfg(target_os = "linux")]
use arrayvec::ArrayVec;
use core::{
    ffi::{CStr, c_char, c_int},
    sync::atomic::{AtomicI32, AtomicUsize, Ordering},
};
#[cfg(test)]
use mockall::predicate;
use once_cell::{race::OnceBool, sync::OnceCell};
#[cfg(target_os = "linux")]
use procfs::process::{MMapPath, MemoryMap, Process};
use retour::{GenericDetour, RawDetour};

#[cfg_attr(any(not(target_os = "linux"), test), allow(dead_code))]
const QAGAME: &str = "qagamex64.so";

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
    PythonInitializationFailed(PythonInitializationError),
    DetourCouldNotBeCreated(QuakeLiveFunction),
    DetourCouldNotBeEnabled(QuakeLiveFunction),
    StaticDetourNotFound(QuakeLiveFunction),
    VmFunctionNotFound(QuakeLiveFunction),
    MainEngineNotInitialized,
}

#[derive(Debug)]
struct StaticFunctions {
    com_printf_orig: unsafe extern "C" fn(*const c_char, ...),
    cmd_addcommand_orig: extern "C" fn(*const c_char, unsafe extern "C" fn()),
    cmd_args_orig: extern "C" fn() -> *const c_char,
    cmd_argv_orig: extern "C" fn(c_int) -> *const c_char,
    cmd_tokenizestring_orig: extern "C" fn(*const c_char) -> *const c_char,
    cbuf_executetext_orig: extern "C" fn(cbufExec_t, *const c_char),
    cvar_findvar_orig: extern "C" fn(*const c_char) -> *mut cvar_t,
    cvar_get_orig: extern "C" fn(*const c_char, *const c_char, c_int) -> *mut cvar_t,
    cvar_getlimit_orig: extern "C" fn(
        *const c_char,
        *const c_char,
        *const c_char,
        *const c_char,
        c_int,
    ) -> *mut cvar_t,
    cvar_set2_orig: extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t,
    sv_sendservercommand_orig: unsafe extern "C" fn(*mut client_t, *const c_char, ...),
    sv_executeclientcommand_orig: extern "C" fn(*mut client_t, *const c_char, qboolean),
    sv_shutdown_orig: extern "C" fn(*const c_char),
    sv_map_f_orig: extern "C" fn(),
    sv_cliententerworld_orig: extern "C" fn(*mut client_t, *mut usercmd_t),
    sv_setconfigstring_orig: extern "C" fn(c_int, *const c_char),
    sv_getconfigstring_orig: extern "C" fn(c_int, *mut c_char, c_int),
    sv_dropclient_orig: extern "C" fn(*mut client_t, *const c_char),
    sys_setmoduleoffset_orig: extern "C" fn(*mut c_char, unsafe extern "C" fn()),
    sv_spawnserver_orig: extern "C" fn(*mut c_char, qboolean),
    cmd_executestring_orig: extern "C" fn(*const c_char),
    cmd_argc_orig: extern "C" fn() -> c_int,
}

#[derive(Debug)]
struct StaticDetours {
    cmd_addcommand_detour: GenericDetour<extern "C" fn(*const c_char, unsafe extern "C" fn())>,
    sys_setmoduleoffset_detour: GenericDetour<extern "C" fn(*mut c_char, unsafe extern "C" fn())>,
    sv_executeclientcommand_detour:
        GenericDetour<extern "C" fn(*mut client_t, *const c_char, qboolean)>,
    sv_cliententerworld_detour: GenericDetour<extern "C" fn(*mut client_t, *mut usercmd_t)>,
    sv_setconfgistring_detour: GenericDetour<extern "C" fn(c_int, *const c_char)>,
    sv_dropclient_detour: GenericDetour<extern "C" fn(*mut client_t, *const c_char)>,
    sv_spawnserver_detour: GenericDetour<extern "C" fn(*mut c_char, qboolean)>,
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

struct VmFunctions {
    vm_call_table: AtomicUsize,

    g_addevent_orig: AtomicUsize,
    check_privileges_orig: AtomicUsize,
    client_connect_orig: AtomicUsize,
    client_spawn_orig: AtomicUsize,
    g_damage_orig: AtomicUsize,
    touch_item_orig: AtomicUsize,
    launch_item_orig: AtomicUsize,
    drop_item_orig: AtomicUsize,
    g_start_kamikaze_orig: AtomicUsize,
    g_free_entity_orig: AtomicUsize,
    g_init_game_orig: AtomicUsize,
    g_shutdown_game_orig: AtomicUsize,
    g_run_frame_orig: AtomicUsize,
    #[cfg(feature = "patches")]
    cmd_callvote_f_orig: AtomicUsize,

    client_spawn_detour: ArcSwapOption<ClientSpawnDetourType>,
    client_connect_detour: ArcSwapOption<ClientConnectDetourType>,
    g_start_kamikaze_detour: ArcSwapOption<GStartKamikazeDetourType>,
    g_damage_detour: ArcSwapOption<GDamageDetourType>,
}

#[cfg_attr(any(not(target_os = "linux"), test), allow(dead_code))]
const OFFSET_VM_CALL_TABLE: usize = 0x3;
#[cfg_attr(any(not(target_os = "linux"), test), allow(dead_code))]
const OFFSET_INITGAME: usize = 0x18;
#[cfg_attr(any(not(target_os = "linux"), test), allow(dead_code))]
const OFFSET_RUNFRAME: usize = 0x8;

impl VmFunctions {
    pub(crate) fn try_initialize_from(
        &self,
        #[allow(unused_variables)] module_offset: usize,
    ) -> Result<(), QuakeLiveEngineError> {
        #[cfg(not(target_os = "linux"))]
        return Err(QuakeLiveEngineError::ProcessNotFound(
            "could not find my own process".to_string(),
        ));
        #[cfg(target_os = "linux")]
        {
            let myself_process = Process::myself().map_err(|_| {
                QuakeLiveEngineError::ProcessNotFound("could not find my own process".to_string())
            })?;
            let myself_maps = myself_process.maps().map_err(|_| {
                QuakeLiveEngineError::NoMemoryMappingInformationFound(
                    "no memory mapping information found".to_string(),
                )
            })?;

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
            let failed_functions: ArrayVec<QuakeLiveFunction, 11> = [
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
            .filter_map(
                |(ql_func, field)| match pattern_search_module(&qagame_maps, ql_func) {
                    None => Some(*ql_func),
                    Some(orig_func) => {
                        debug!(target: "shinqlx", "{}: {:#X}", ql_func, orig_func);
                        field.store(orig_func, Ordering::Release);
                        None
                    }
                },
            )
            .collect();

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
            self.vm_call_table.store(vm_call_table, Ordering::Release);

            let g_initgame_orig = unsafe {
                ptr::read(
                    (vm_call_table + OFFSET_INITGAME)
                        as *const *const extern "C" fn(c_int, c_int, c_int),
                )
            };
            debug!(target: "shinqlx", "G_InitGame: {:#X}", g_initgame_orig as usize);
            self.g_init_game_orig
                .store(g_initgame_orig as usize, Ordering::Release);

            let g_shutdowngame_orig =
                unsafe { ptr::read_unaligned(vm_call_table as *const *const extern "C" fn(c_int)) };
            debug!(target: "shinqlx", "G_ShutdownGame: {:#X}", g_shutdowngame_orig as usize);
            self.g_shutdown_game_orig
                .store(g_shutdowngame_orig as usize, Ordering::Release);

            let g_runframe_orig = unsafe {
                ptr::read((vm_call_table + OFFSET_RUNFRAME) as *const *const extern "C" fn(c_int))
            };
            debug!(target: "shinqlx", "G_RunFrame: {:#X}", g_runframe_orig as usize);
            self.g_run_frame_orig
                .store(g_runframe_orig as usize, Ordering::Release);

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
    pub(crate) fn hook(&self) -> Result<(), QuakeLiveEngineError> {
        let vm_call_table = self.vm_call_table.load(Ordering::Acquire);

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

        let client_connect_orig = self.client_connect_orig.load(Ordering::Acquire);
        let client_connect_func = unsafe {
            mem::transmute::<usize, extern "C" fn(c_int, qboolean, qboolean) -> *const c_char>(
                client_connect_orig,
            )
        };
        let client_connect_detour =
            unsafe { ClientConnectDetourType::new(client_connect_func, shinqlx_client_connect) }
                .map_err(|_| {
                    QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::ClientConnect)
                })?;
        unsafe { client_connect_detour.enable() }.map_err(|_| {
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::ClientConnect)
        })?;

        self.client_connect_detour
            .swap(Some(client_connect_detour.into()))
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling client_conect detour: {}", e);
                }
            });

        let g_start_kamikaze_orig = self.g_start_kamikaze_orig.load(Ordering::Acquire);
        let g_start_kamikaze_func = unsafe {
            mem::transmute::<usize, extern "C" fn(*mut gentity_s)>(g_start_kamikaze_orig)
        };
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
            .swap(Some(g_start_kamikaze_detour.into()))
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling start_kamikaze detour: {}", e);
                }
            });

        let client_spawn_orig = self.client_spawn_orig.load(Ordering::Acquire);
        let client_spawn_func =
            unsafe { mem::transmute::<usize, extern "C" fn(*mut gentity_s)>(client_spawn_orig) };
        let client_spawn_detour =
            unsafe { ClientSpawnDetourType::new(client_spawn_func, shinqlx_clientspawn) }.map_err(
                |_| QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::ClientSpawn),
            )?;
        unsafe { client_spawn_detour.enable() }.map_err(|_| {
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::ClientSpawn)
        })?;

        self.client_spawn_detour
            .swap(Some(client_spawn_detour.into()))
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling client_spawn detour: {}", e);
                }
            });

        let g_damage_orig = self.g_damage_orig.load(Ordering::Acquire);
        let g_damage_func = unsafe {
            mem::transmute::<
                usize,
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
            >(g_damage_orig)
        };
        let g_damage_detour = unsafe { GDamageDetourType::new(g_damage_func, shinqlx_g_damage) }
            .map_err(|_| {
                QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::G_Damage)
            })?;
        unsafe { g_damage_detour.enable() }.map_err(|_| {
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::G_Damage)
        })?;

        self.g_damage_detour
            .swap(Some(g_damage_detour.into()))
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling damage detour: {}", e);
                }
            });

        Ok(())
    }

    #[cfg(feature = "patches")]
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn patch(&self) {
        let cmd_callvote_f_orig = self.cmd_callvote_f_orig.load(Ordering::Acquire);
        if cmd_callvote_f_orig == 0 {
            return;
        }

        patch_callvote_f(cmd_callvote_f_orig);
    }

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
            field.store(0, Ordering::Release);
        });

        self.client_connect_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling client_connect detour: {}", e);
                }
            });

        self.g_start_kamikaze_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling start_kamikaze detour: {}", e);
                }
            });

        self.client_spawn_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling client_spawn detour: {}", e);
                }
            });

        self.g_damage_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .iter()
            .for_each(|detour| {
                if let Err(e) = unsafe { detour.disable() } {
                    error!(target: "shinqlx", "error when disabling damage detour: {}", e);
                }
            });
    }
}

#[cfg(test)]
mod vm_functions_tests {
    use super::{
        ClientConnectDetourType, ClientSpawnDetourType, GDamageDetourType,
        GStartKamikazeDetourType, VmFunctions,
    };

    use crate::quake_live_engine::mock_quake_functions::{
        CheckPrivileges, ClientConnect, ClientSpawn, Drop_Item, G_AddEvent, G_Damage, G_FreeEntity,
        G_InitGame, G_RunFrame, G_StartKamikaze, LaunchItem, Touch_Item, detoured_ClientConnect,
        detoured_ClientSpawn, detoured_G_Damage, detoured_G_StartKamikaze,
    };

    use core::sync::atomic::{AtomicUsize, Ordering};

    use pretty_assertions::assert_eq;

    fn default_vm_functions() -> VmFunctions {
        VmFunctions {
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
            client_spawn_detour: Default::default(),
            client_connect_detour: Default::default(),
            g_start_kamikaze_detour: Default::default(),
            g_damage_detour: Default::default(),
        }
    }

    #[test]
    fn unhook_with_no_functions_set_before() {
        let vm_functions = default_vm_functions();

        vm_functions.unhook();

        assert_eq!(vm_functions.vm_call_table.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_addevent_orig.load(Ordering::Acquire), 0);
        assert_eq!(
            vm_functions.check_privileges_orig.load(Ordering::Acquire),
            0
        );
        assert_eq!(vm_functions.client_connect_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.client_spawn_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_damage_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.touch_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.launch_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.drop_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(
            vm_functions.g_start_kamikaze_orig.load(Ordering::Acquire),
            0
        );
        assert_eq!(vm_functions.g_free_entity_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_init_game_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_run_frame_orig.load(Ordering::Acquire), 0);

        assert!(vm_functions.client_connect_detour.load().is_none());
        assert!(vm_functions.g_start_kamikaze_detour.load().is_none());
        assert!(vm_functions.client_spawn_detour.load().is_none());
        assert!(vm_functions.g_damage_detour.load().is_none());
    }

    #[test]
    fn unhook_with_functions_set_before_but_no_detours() {
        let vm_functions = VmFunctions {
            vm_call_table: AtomicUsize::new(42),
            g_addevent_orig: AtomicUsize::new(G_AddEvent as usize),
            check_privileges_orig: AtomicUsize::new(CheckPrivileges as usize),
            client_connect_orig: AtomicUsize::new(ClientConnect as usize),
            client_spawn_orig: AtomicUsize::new(ClientSpawn as usize),
            g_damage_orig: AtomicUsize::new(G_Damage as usize),
            touch_item_orig: AtomicUsize::new(Touch_Item as usize),
            launch_item_orig: AtomicUsize::new(LaunchItem as usize),
            drop_item_orig: AtomicUsize::new(Drop_Item as usize),
            g_start_kamikaze_orig: AtomicUsize::new(G_StartKamikaze as usize),
            g_free_entity_orig: AtomicUsize::new(G_FreeEntity as usize),
            g_init_game_orig: AtomicUsize::new(G_InitGame as usize),
            g_run_frame_orig: AtomicUsize::new(G_RunFrame as usize),
            ..default_vm_functions()
        };

        vm_functions.unhook();

        assert_eq!(vm_functions.vm_call_table.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_addevent_orig.load(Ordering::Acquire), 0);
        assert_eq!(
            vm_functions.check_privileges_orig.load(Ordering::Acquire),
            0
        );
        assert_eq!(vm_functions.client_connect_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.client_spawn_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_damage_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.touch_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.launch_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.drop_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(
            vm_functions.g_start_kamikaze_orig.load(Ordering::Acquire),
            0
        );
        assert_eq!(vm_functions.g_free_entity_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_init_game_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_run_frame_orig.load(Ordering::Acquire), 0);

        assert!(vm_functions.client_connect_detour.load().is_none());
        assert!(vm_functions.g_start_kamikaze_detour.load().is_none());
        assert!(vm_functions.client_spawn_detour.load().is_none());
        assert!(vm_functions.g_damage_detour.load().is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn unhook_with_functions_and_disabled_detours_set_before() {
        let vm_functions = VmFunctions {
            vm_call_table: AtomicUsize::new(42),
            g_addevent_orig: AtomicUsize::new(G_AddEvent as usize),
            check_privileges_orig: AtomicUsize::new(CheckPrivileges as usize),
            client_connect_orig: AtomicUsize::new(ClientConnect as usize),
            client_spawn_orig: AtomicUsize::new(ClientSpawn as usize),
            g_damage_orig: AtomicUsize::new(G_Damage as usize),
            touch_item_orig: AtomicUsize::new(Touch_Item as usize),
            launch_item_orig: AtomicUsize::new(LaunchItem as usize),
            drop_item_orig: AtomicUsize::new(Drop_Item as usize),
            g_start_kamikaze_orig: AtomicUsize::new(G_StartKamikaze as usize),
            g_free_entity_orig: AtomicUsize::new(G_FreeEntity as usize),
            g_init_game_orig: AtomicUsize::new(G_InitGame as usize),
            g_run_frame_orig: AtomicUsize::new(G_RunFrame as usize),
            ..default_vm_functions()
        };

        let client_connect_detour =
            unsafe { ClientConnectDetourType::new(ClientConnect, detoured_ClientConnect) }
                .expect("this should not happen");
        vm_functions
            .client_connect_detour
            .store(Some(client_connect_detour.into()));
        let g_start_kamikaze_detour =
            unsafe { GStartKamikazeDetourType::new(G_StartKamikaze, detoured_G_StartKamikaze) }
                .expect("this should not happen");
        vm_functions
            .g_start_kamikaze_detour
            .store(Some(g_start_kamikaze_detour.into()));
        let client_spawn_detour =
            unsafe { ClientSpawnDetourType::new(ClientSpawn, detoured_ClientSpawn) }
                .expect("this should not happen");
        vm_functions
            .client_spawn_detour
            .store(Some(client_spawn_detour.into()));
        let g_damage_detour = unsafe { GDamageDetourType::new(G_Damage, detoured_G_Damage) }
            .expect("this should not happen");
        vm_functions
            .g_damage_detour
            .store(Some(g_damage_detour.into()));

        vm_functions.unhook();

        assert_eq!(vm_functions.vm_call_table.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_addevent_orig.load(Ordering::Acquire), 0);
        assert_eq!(
            vm_functions.check_privileges_orig.load(Ordering::Acquire),
            0
        );
        assert_eq!(vm_functions.client_connect_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.client_spawn_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_damage_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.touch_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.launch_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.drop_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(
            vm_functions.g_start_kamikaze_orig.load(Ordering::Acquire),
            0
        );
        assert_eq!(vm_functions.g_free_entity_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_init_game_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_run_frame_orig.load(Ordering::Acquire), 0);

        assert!(vm_functions.client_connect_detour.load().is_none());
        assert!(vm_functions.g_start_kamikaze_detour.load().is_none());
        assert!(vm_functions.client_spawn_detour.load().is_none());
        assert!(vm_functions.g_damage_detour.load().is_none());
    }

    #[test]
    #[cfg_attr(any(miri, target_os = "macos"), ignore)]
    fn unhook_with_functions_and_enabled_detours_set_before() {
        let vm_functions = VmFunctions {
            vm_call_table: AtomicUsize::new(42),
            g_addevent_orig: AtomicUsize::new(G_AddEvent as usize),
            check_privileges_orig: AtomicUsize::new(CheckPrivileges as usize),
            client_connect_orig: AtomicUsize::new(ClientConnect as usize),
            client_spawn_orig: AtomicUsize::new(ClientSpawn as usize),
            g_damage_orig: AtomicUsize::new(G_Damage as usize),
            touch_item_orig: AtomicUsize::new(Touch_Item as usize),
            launch_item_orig: AtomicUsize::new(LaunchItem as usize),
            drop_item_orig: AtomicUsize::new(Drop_Item as usize),
            g_start_kamikaze_orig: AtomicUsize::new(G_StartKamikaze as usize),
            g_free_entity_orig: AtomicUsize::new(G_FreeEntity as usize),
            g_init_game_orig: AtomicUsize::new(G_InitGame as usize),
            g_run_frame_orig: AtomicUsize::new(G_RunFrame as usize),
            ..default_vm_functions()
        };

        let client_connect_detour =
            unsafe { ClientConnectDetourType::new(ClientConnect, detoured_ClientConnect) }
                .expect("this should not happen");
        unsafe { client_connect_detour.enable() }.expect("this should not happen");
        vm_functions
            .client_connect_detour
            .store(Some(client_connect_detour.into()));
        let g_start_kamikaze_detour =
            unsafe { GStartKamikazeDetourType::new(G_StartKamikaze, detoured_G_StartKamikaze) }
                .expect("this should not happen");
        unsafe { g_start_kamikaze_detour.enable() }.expect("this should not happen");
        vm_functions
            .g_start_kamikaze_detour
            .store(Some(g_start_kamikaze_detour.into()));
        let client_spawn_detour =
            unsafe { ClientSpawnDetourType::new(ClientSpawn, detoured_ClientSpawn) }
                .expect("this should not happen");
        unsafe { client_spawn_detour.enable() }.expect("this should not happen");
        vm_functions
            .client_spawn_detour
            .store(Some(client_spawn_detour.into()));
        let g_damage_detour = unsafe { GDamageDetourType::new(G_Damage, detoured_G_Damage) }
            .expect("this should not happen");
        unsafe { g_damage_detour.enable() }.expect("this should not happen");
        vm_functions
            .g_damage_detour
            .store(Some(g_damage_detour.into()));

        vm_functions.unhook();

        assert_eq!(vm_functions.vm_call_table.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_addevent_orig.load(Ordering::Acquire), 0);
        assert_eq!(
            vm_functions.check_privileges_orig.load(Ordering::Acquire),
            0
        );
        assert_eq!(vm_functions.client_connect_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.client_spawn_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_damage_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.touch_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.launch_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.drop_item_orig.load(Ordering::Acquire), 0);
        assert_eq!(
            vm_functions.g_start_kamikaze_orig.load(Ordering::Acquire),
            0
        );
        assert_eq!(vm_functions.g_free_entity_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_init_game_orig.load(Ordering::Acquire), 0);
        assert_eq!(vm_functions.g_run_frame_orig.load(Ordering::Acquire), 0);

        assert!(vm_functions.client_connect_detour.load().is_none());
        assert!(vm_functions.g_start_kamikaze_detour.load().is_none());
        assert!(vm_functions.client_spawn_detour.load().is_none());
        assert!(vm_functions.g_damage_detour.load().is_none());
    }
}

#[cfg(target_os = "linux")]
fn try_find_static_function<FuncType>(
    maps: &[&MemoryMap],
    func: QuakeLiveFunction,
) -> Result<FuncType, QuakeLiveEngineError> {
    pattern_search_module(maps, func).map_or_else(
        || {
            error!(target: "shinqlx", "Function {} not found", &func);
            Err(QuakeLiveEngineError::StaticFunctionNotFound(func))
        },
        |result| {
            debug!(target: "shinqlx", "{}: {:#X}", &func, result);
            Ok(unsafe { mem::transmute_copy::<usize, FuncType>(&result) })
        },
    )
}

pub(crate) struct QuakeLiveEngine {
    static_functions: OnceCell<StaticFunctions>,
    static_detours: OnceCell<StaticDetours>,

    pub(crate) sv_maxclients: AtomicI32,
    common_initialized: OnceBool,

    vm_functions: VmFunctions,
    current_vm: AtomicUsize,
}

#[cfg(target_os = "linux")]
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
                #[cfg(feature = "patches")]
                cmd_callvote_f_orig: Default::default(),
                client_spawn_detour: ArcSwapOption::empty(),
                client_connect_detour: ArcSwapOption::empty(),
                g_start_kamikaze_detour: ArcSwapOption::empty(),
                g_damage_detour: ArcSwapOption::empty(),
            },
            current_vm: AtomicUsize::new(0),
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn search_static_functions(&self) -> Result<(), QuakeLiveEngineError> {
        #[cfg(not(target_os = "linux"))]
        return Err(QuakeLiveEngineError::ProcessNotFound(
            "could not find my own process".to_string(),
        ));
        #[cfg(target_os = "linux")]
        {
            let myself_process = Process::myself().map_err(|_| {
                QuakeLiveEngineError::ProcessNotFound("could not find my own process".to_string())
            })?;
            let myself_maps = myself_process.maps().map_err(|_| {
                QuakeLiveEngineError::NoMemoryMappingInformationFound(
                    "no memory mapping information found".to_string(),
                )
            })?;
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
                    "no memory mapping information found".to_string(),
                ));
            }

            debug!(target: "shinqlx", "Searching for necessary functions...");
            let com_printf_orig = try_find_static_function::<extern "C" fn(*const c_char, ...)>(
                &qzeroded_maps,
                QuakeLiveFunction::Com_Printf,
            )?;

            let cmd_addcommand_orig = try_find_static_function::<
                extern "C" fn(*const c_char, unsafe extern "C" fn()),
            >(
                &qzeroded_maps, QuakeLiveFunction::Cmd_AddCommand
            )?;

            let cmd_args_orig = try_find_static_function::<extern "C" fn() -> *const c_char>(
                &qzeroded_maps,
                QuakeLiveFunction::Cmd_Args,
            )?;

            let cmd_argv_orig = try_find_static_function::<extern "C" fn(c_int) -> *const c_char>(
                &qzeroded_maps,
                QuakeLiveFunction::Cmd_Argv,
            )?;

            let cmd_tokenizestring_orig = try_find_static_function::<
                extern "C" fn(*const c_char) -> *const c_char,
            >(
                &qzeroded_maps, QuakeLiveFunction::Cmd_Tokenizestring
            )?;

            let cbuf_executetext_orig = try_find_static_function::<
                extern "C" fn(cbufExec_t, *const c_char),
            >(
                &qzeroded_maps, QuakeLiveFunction::Cbuf_ExecuteText
            )?;

            let cvar_findvar_orig = try_find_static_function::<
                extern "C" fn(*const c_char) -> *mut cvar_t,
            >(&qzeroded_maps, QuakeLiveFunction::Cvar_FindVar)?;

            let cvar_get_orig = try_find_static_function::<
                extern "C" fn(*const c_char, *const c_char, c_int) -> *mut cvar_t,
            >(&qzeroded_maps, QuakeLiveFunction::Cvar_Get)?;

            let cvar_getlimit_orig =
                try_find_static_function::<
                    extern "C" fn(
                        *const c_char,
                        *const c_char,
                        *const c_char,
                        *const c_char,
                        c_int,
                    ) -> *mut cvar_t,
                >(&qzeroded_maps, QuakeLiveFunction::Cvar_GetLimit)?;

            let cvar_set2_orig = try_find_static_function::<
                extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t,
            >(&qzeroded_maps, QuakeLiveFunction::Cvar_Set2)?;

            let sv_sendservercommand_orig =
                try_find_static_function::<extern "C" fn(*mut client_t, *const c_char, ...)>(
                    &qzeroded_maps,
                    QuakeLiveFunction::SV_SendServerCommand,
                )?;

            let sv_executeclientcommand_orig =
                try_find_static_function::<extern "C" fn(*mut client_t, *const c_char, qboolean)>(
                    &qzeroded_maps,
                    QuakeLiveFunction::SV_ExecuteClientCommand,
                )?;

            let sv_shutdown_orig = try_find_static_function::<extern "C" fn(*const c_char)>(
                &qzeroded_maps,
                QuakeLiveFunction::SV_Shutdown,
            )?;

            let sv_map_f_orig = try_find_static_function::<extern "C" fn()>(
                &qzeroded_maps,
                QuakeLiveFunction::SV_Map_f,
            )?;

            let sv_cliententerworld_orig =
                try_find_static_function::<extern "C" fn(*mut client_t, *mut usercmd_t)>(
                    &qzeroded_maps,
                    QuakeLiveFunction::SV_ClientEnterWorld,
                )?;

            let sv_setconfigstring_orig = try_find_static_function::<
                extern "C" fn(c_int, *const c_char),
            >(
                &qzeroded_maps, QuakeLiveFunction::SV_SetConfigstring
            )?;

            let sv_getconfigstring_orig = try_find_static_function::<
                extern "C" fn(c_int, *mut c_char, c_int),
            >(
                &qzeroded_maps, QuakeLiveFunction::SV_GetConfigstring
            )?;

            let sv_dropclient_orig = try_find_static_function::<
                extern "C" fn(*mut client_t, *const c_char),
            >(
                &qzeroded_maps, QuakeLiveFunction::SV_DropClient
            )?;

            let sys_setmoduleoffset_orig =
                try_find_static_function::<extern "C" fn(*mut c_char, unsafe extern "C" fn())>(
                    &qzeroded_maps,
                    QuakeLiveFunction::Sys_SetModuleOffset,
                )?;

            let sv_spawnserver_orig = try_find_static_function::<
                extern "C" fn(*mut c_char, qboolean),
            >(
                &qzeroded_maps, QuakeLiveFunction::SV_SpawnServer
            )?;

            let cmd_executestring_orig = try_find_static_function::<extern "C" fn(*const c_char)>(
                &qzeroded_maps,
                QuakeLiveFunction::Cmd_ExecuteString,
            )?;

            // Cmd_Argc is really small, making it hard to search for, so we use a reference to it instead.
            let base_address = unsafe {
                ptr::read_unaligned(
                    (sv_map_f_orig as usize + OFFSET_CMD_ARGC as usize) as *const i32,
                )
            };
            #[allow(clippy::fn_to_numeric_cast_with_truncation)]
            let cmd_argc_ptr = base_address + sv_map_f_orig as i32 + OFFSET_CMD_ARGC + 4;
            debug!(target: "shinqlx", "{}: {:#X}", QuakeLiveFunction::Cmd_Argc, cmd_argc_ptr);
            let cmd_argc_orig =
                unsafe { mem::transmute::<usize, extern "C" fn() -> c_int>(cmd_argc_ptr as usize) };

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

    pub(crate) fn set_tag(&self) {
        const SV_TAGS_PREFIX: &str = "shinqlx";

        self.find_cvar("sv_tags")
            .map(|cvar| cvar.get_string().to_string())
            .filter(|sv_tags_string| sv_tags_string.split(',').all(|tag| tag != SV_TAGS_PREFIX))
            .map(|mut sv_tags_string| {
                if sv_tags_string.len() > 2 {
                    sv_tags_string.insert(0, ',');
                }
                sv_tags_string.insert_str(0, SV_TAGS_PREFIX);
                sv_tags_string
            })
            .iter()
            .for_each(|new_tags| {
                self.set_cvar_forced("sv_tags", new_tags, false);
            });
    }

    // Called after the game is initialized.
    pub(crate) fn initialize_cvars(&self) {
        self.find_cvar("sv_maxclients")
            .iter()
            .for_each(|maxclients| {
                self.sv_maxclients
                    .store(maxclients.get_integer(), Ordering::Release);
            })
    }

    pub(crate) fn get_max_clients(&self) -> i32 {
        self.sv_maxclients.load(Ordering::Acquire)
    }

    // Currently called by My_Cmd_AddCommand(), since it's called at a point where we
    // can safely do whatever we do below. It'll segfault if we do it at the entry
    // point, since functions like Cmd_AddCommand need initialization first.
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

        pyshinqlx_initialize().map_err(|err| {
            error!(target: "shinqlx", "Python initialization failed.");
            QuakeLiveEngineError::PythonInitializationFailed(err)
        })?;

        self.common_initialized
            .set(true)
            .map_err(|_| QuakeLiveEngineError::MainEngineNotInitialized)
    }

    pub(crate) fn is_common_initialized(&self) -> bool {
        self.common_initialized
            .get()
            .is_some_and(|is_initialized| is_initialized)
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn initialize_vm(&self, module_offset: usize) -> Result<(), QuakeLiveEngineError> {
        self.vm_functions.try_initialize_from(module_offset)?;
        self.current_vm.store(module_offset, Ordering::Release);

        self.vm_functions.hook()?;
        #[cfg(feature = "patches")]
        self.vm_functions.patch();

        Ok(())
    }

    pub(crate) fn unhook_vm(&self, _restart: bool) {
        self.vm_functions.unhook();
    }

    fn com_printf_orig(
        &self,
    ) -> Result<unsafe extern "C" fn(*const c_char, ...), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Com_Printf,
            )),
            |static_functions| Ok(static_functions.com_printf_orig),
        )
    }

    fn cmd_addcommand_orig(
        &self,
    ) -> Result<extern "C" fn(*const c_char, unsafe extern "C" fn()), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_AddCommand,
            )),
            |static_functions| Ok(static_functions.cmd_addcommand_orig),
        )
    }

    fn cmd_args_orig(&self) -> Result<extern "C" fn() -> *const c_char, QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Args,
            )),
            |static_functions| Ok(static_functions.cmd_args_orig),
        )
    }

    fn cmd_argv_orig(&self) -> Result<extern "C" fn(c_int) -> *const c_char, QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Argv,
            )),
            |static_functions| Ok(static_functions.cmd_argv_orig),
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn cmd_tokenizestring_orig(
        &self,
    ) -> Result<extern "C" fn(*const c_char) -> *const c_char, QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Tokenizestring,
            )),
            |static_functions| Ok(static_functions.cmd_tokenizestring_orig),
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn cbuf_executetext_orig(
        &self,
    ) -> Result<extern "C" fn(cbufExec_t, *const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cbuf_ExecuteText,
            )),
            |static_functions| Ok(static_functions.cbuf_executetext_orig),
        )
    }

    fn cvar_findvar_orig(
        &self,
    ) -> Result<extern "C" fn(*const c_char) -> *mut cvar_t, QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cvar_FindVar,
            )),
            |static_functions| Ok(static_functions.cvar_findvar_orig),
        )
    }

    #[allow(clippy::type_complexity)]
    fn cvar_get_orig(
        &self,
    ) -> Result<
        extern "C" fn(*const c_char, *const c_char, c_int) -> *mut cvar_t,
        QuakeLiveEngineError,
    > {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cvar_Get,
            )),
            |static_functions| Ok(static_functions.cvar_get_orig),
        )
    }

    #[allow(clippy::type_complexity)]
    fn cvar_getlimit_orig(
        &self,
    ) -> Result<
        extern "C" fn(
            *const c_char,
            *const c_char,
            *const c_char,
            *const c_char,
            c_int,
        ) -> *mut cvar_t,
        QuakeLiveEngineError,
    > {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cvar_GetLimit,
            )),
            |static_functions| Ok(static_functions.cvar_getlimit_orig),
        )
    }

    #[allow(clippy::type_complexity)]
    fn cvar_set2_orig(
        &self,
    ) -> Result<
        extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t,
        QuakeLiveEngineError,
    > {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cvar_Set2,
            )),
            |static_functions| Ok(static_functions.cvar_set2_orig),
        )
    }

    fn sv_sendservercommand_orig(
        &self,
    ) -> Result<unsafe extern "C" fn(*mut client_t, *const c_char, ...), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SendServerCommand,
            )),
            |static_functions| Ok(static_functions.sv_sendservercommand_orig),
        )
    }

    fn sv_executeclientcommand_orig(
        &self,
    ) -> Result<extern "C" fn(*mut client_t, *const c_char, qboolean), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ExecuteClientCommand,
            )),
            |static_functions| Ok(static_functions.sv_executeclientcommand_orig),
        )
    }

    pub(crate) fn sv_shutdown_orig(
        &self,
    ) -> Result<extern "C" fn(*const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_Shutdown,
            )),
            |static_functions| Ok(static_functions.sv_shutdown_orig),
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn sv_map_f_orig(&self) -> Result<extern "C" fn(), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_Map_f,
            )),
            |static_functions| Ok(static_functions.sv_map_f_orig),
        )
    }

    fn sv_cliententerworld_orig(
        &self,
    ) -> Result<extern "C" fn(*mut client_t, *mut usercmd_t), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ClientEnterWorld,
            )),
            |static_functions| Ok(static_functions.sv_cliententerworld_orig),
        )
    }

    fn sv_setconfigstring_orig(
        &self,
    ) -> Result<extern "C" fn(c_int, *const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SetConfigstring,
            )),
            |static_functions| Ok(static_functions.sv_setconfigstring_orig),
        )
    }

    fn sv_getconfigstring_orig(
        &self,
    ) -> Result<extern "C" fn(c_int, *mut c_char, c_int), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_GetConfigstring,
            )),
            |static_functions| Ok(static_functions.sv_getconfigstring_orig),
        )
    }

    fn sv_dropclient_orig(
        &self,
    ) -> Result<extern "C" fn(*mut client_t, *const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_DropClient,
            )),
            |static_functions| Ok(static_functions.sv_dropclient_orig),
        )
    }

    fn sys_setmoduleoffset_orig(
        &self,
    ) -> Result<extern "C" fn(*mut c_char, unsafe extern "C" fn()), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Sys_SetModuleOffset,
            )),
            |static_functions| Ok(static_functions.sys_setmoduleoffset_orig),
        )
    }

    fn sv_spawnserver_orig(
        &self,
    ) -> Result<extern "C" fn(*mut c_char, qboolean), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SpawnServer,
            )),
            |static_functions| Ok(static_functions.sv_spawnserver_orig),
        )
    }

    fn cmd_executestring_orig(&self) -> Result<extern "C" fn(*const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_ExecuteString,
            )),
            |static_functions| Ok(static_functions.cmd_executestring_orig),
        )
    }

    fn cmd_argc_orig(&self) -> Result<extern "C" fn() -> c_int, QuakeLiveEngineError> {
        self.static_functions.get().map_or(
            Err(QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Argc,
            )),
            |static_functions| Ok(static_functions.cmd_argc_orig),
        )
    }

    fn cmd_addcommand_detour(
        &self,
    ) -> Result<
        &GenericDetour<extern "C" fn(*const c_char, unsafe extern "C" fn())>,
        QuakeLiveEngineError,
    > {
        self.static_detours.get().map_or(
            Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::Cmd_AddCommand,
            )),
            |static_detours| Ok(&static_detours.cmd_addcommand_detour),
        )
    }

    fn sys_setmoduleoffset_detour(
        &self,
    ) -> Result<
        &GenericDetour<extern "C" fn(*mut c_char, unsafe extern "C" fn())>,
        QuakeLiveEngineError,
    > {
        self.static_detours.get().map_or(
            Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::Sys_SetModuleOffset,
            )),
            |static_detours| Ok(&static_detours.sys_setmoduleoffset_detour),
        )
    }

    #[allow(clippy::type_complexity)]
    fn sv_executeclientcommand_detour(
        &self,
    ) -> Result<
        &GenericDetour<extern "C" fn(*mut client_t, *const c_char, qboolean)>,
        QuakeLiveEngineError,
    > {
        self.static_detours.get().map_or(
            Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_ExecuteClientCommand,
            )),
            |static_detours| Ok(&static_detours.sv_executeclientcommand_detour),
        )
    }

    #[allow(clippy::type_complexity)]
    fn sv_cliententerworld_detour(
        &self,
    ) -> Result<&GenericDetour<extern "C" fn(*mut client_t, *mut usercmd_t)>, QuakeLiveEngineError>
    {
        self.static_detours.get().map_or(
            Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_ClientEnterWorld,
            )),
            |static_detours| Ok(&static_detours.sv_cliententerworld_detour),
        )
    }

    #[allow(clippy::type_complexity)]
    fn sv_setconfgistring_detour(
        &self,
    ) -> Result<&GenericDetour<extern "C" fn(c_int, *const c_char)>, QuakeLiveEngineError> {
        self.static_detours.get().map_or(
            Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_SetConfigstring,
            )),
            |static_detours| Ok(&static_detours.sv_setconfgistring_detour),
        )
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn sv_dropclient_detour(
        &self,
    ) -> Result<&GenericDetour<extern "C" fn(*mut client_t, *const c_char)>, QuakeLiveEngineError>
    {
        self.static_detours.get().map_or(
            Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_DropClient,
            )),
            |static_detours| Ok(&static_detours.sv_dropclient_detour),
        )
    }

    #[allow(clippy::type_complexity)]
    fn sv_spawnserver_detour(
        &self,
    ) -> Result<&GenericDetour<extern "C" fn(*mut c_char, qboolean)>, QuakeLiveEngineError> {
        self.static_detours.get().map_or(
            Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_SpawnServer,
            )),
            |static_detours| Ok(&static_detours.sv_spawnserver_detour),
        )
    }

    fn sv_sendservercommand_detour(&self) -> Result<&RawDetour, QuakeLiveEngineError> {
        self.static_detours.get().map_or(
            Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_SendServerCommand,
            )),
            |static_detours| Ok(&static_detours.sv_sendservercommand_detour),
        )
    }

    fn com_printf_detour(&self) -> Result<&RawDetour, QuakeLiveEngineError> {
        self.static_detours.get().map_or(
            Err(QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::Com_Printf,
            )),
            |static_detours| Ok(&static_detours.com_printf_detour),
        )
    }

    pub(crate) fn g_init_game_orig(
        &self,
    ) -> Result<extern "C" fn(c_int, c_int, c_int), QuakeLiveEngineError> {
        match self.vm_functions.g_init_game_orig.load(Ordering::Acquire) {
            0 => Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_InitGame,
            )),
            g_init_game_orig => {
                let g_init_game_func = unsafe {
                    mem::transmute::<usize, extern "C" fn(c_int, c_int, c_int)>(g_init_game_orig)
                };
                Ok(g_init_game_func)
            }
        }
    }

    fn g_shutdown_game_orig(&self) -> Result<extern "C" fn(c_int), QuakeLiveEngineError> {
        match self
            .vm_functions
            .g_shutdown_game_orig
            .load(Ordering::Acquire)
        {
            0 => Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_ShutdownGame,
            )),
            g_shutdown_game_orig => {
                let g_shutdown_game_func =
                    unsafe { mem::transmute::<usize, extern "C" fn(c_int)>(g_shutdown_game_orig) };
                Ok(g_shutdown_game_func)
            }
        }
    }

    pub(crate) fn g_run_frame_orig(&self) -> Result<extern "C" fn(c_int), QuakeLiveEngineError> {
        match self.vm_functions.g_run_frame_orig.load(Ordering::Acquire) {
            0 => Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_RunFrame,
            )),
            g_run_frame_orig => {
                let g_run_frame_func =
                    unsafe { mem::transmute::<usize, extern "C" fn(c_int)>(g_run_frame_orig) };
                Ok(g_run_frame_func)
            }
        }
    }

    fn g_addevent_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t, entity_event_t, c_int), QuakeLiveEngineError> {
        match self.vm_functions.g_addevent_orig.load(Ordering::Acquire) {
            0 => Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_AddEvent,
            )),
            g_addevent_orig => {
                let g_addevent_func = unsafe {
                    mem::transmute::<usize, extern "C" fn(*mut gentity_t, entity_event_t, c_int)>(
                        g_addevent_orig,
                    )
                };
                Ok(g_addevent_func)
            }
        }
    }

    pub(crate) fn g_free_entity_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t), QuakeLiveEngineError> {
        match self.vm_functions.g_free_entity_orig.load(Ordering::Acquire) {
            0 => Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::G_FreeEntity,
            )),
            g_free_entity_orig => {
                let g_free_entity_func = unsafe {
                    mem::transmute::<usize, extern "C" fn(*mut gentity_t)>(g_free_entity_orig)
                };
                Ok(g_free_entity_func)
            }
        }
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn launch_item_orig(
        &self,
    ) -> Result<
        extern "C" fn(*mut gitem_t, *mut vec3_t, *mut vec3_t) -> *mut gentity_t,
        QuakeLiveEngineError,
    > {
        match self.vm_functions.launch_item_orig.load(Ordering::Acquire) {
            0 => Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::LaunchItem,
            )),
            launch_item_orig => {
                let launch_item_func = unsafe {
                    mem::transmute::<
                        usize,
                        extern "C" fn(*mut gitem_t, *mut vec3_t, *mut vec3_t) -> *mut gentity_t,
                    >(launch_item_orig)
                };
                Ok(launch_item_func)
            }
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn touch_item_orig(
        &self,
    ) -> Result<extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t), QuakeLiveEngineError>
    {
        match self.vm_functions.touch_item_orig.load(Ordering::Acquire) {
            0 => Err(QuakeLiveEngineError::VmFunctionNotFound(
                QuakeLiveFunction::Touch_Item,
            )),
            touch_item_orig => {
                let touch_item_func = unsafe {
                    mem::transmute::<
                        usize,
                        extern "C" fn(*mut gentity_t, *mut gentity_t, *mut trace_t),
                    >(touch_item_orig)
                };
                Ok(touch_item_func)
            }
        }
    }
}

#[cfg(test)]
mod quake_live_engine_tests {
    use super::QuakeLiveEngine;

    use super::mock_quake_functions::{
        Cbuf_ExecuteText, Cmd_AddCommand, Cmd_AddCommand_context, Cmd_Argc, Cmd_Args, Cmd_Argv,
        Cmd_ExecuteString, Cmd_Tokenizestring, Com_Printf, Cvar_FindVar, Cvar_FindVar_context,
        Cvar_Get, Cvar_GetLimit, Cvar_Set2, Cvar_Set2_context, G_AddEvent, G_FreeEntity,
        G_InitGame, G_RunFrame, G_ShutdownGame, LaunchItem, SV_ClientEnterWorld, SV_DropClient,
        SV_ExecuteClientCommand, SV_GetConfigstring, SV_Map_f, SV_SendServerCommand,
        SV_SetConfigstring, SV_Shutdown, SV_SpawnServer, Sys_SetModuleOffset,
    };
    use super::quake_live_engine_test_helpers::{
        default_quake_engine, default_static_detours, default_static_functions,
    };

    use crate::commands::{
        cmd_center_print, cmd_py_command, cmd_py_rcon, cmd_regular_print, cmd_restart_python,
        cmd_send_server_command, cmd_slap, cmd_slay,
    };
    use crate::quake_live_functions::QuakeLiveFunction;

    use crate::ffi::c::prelude::{
        CVarBuilder, cbufExec_t, client_t, cvar_t, entity_event_t, gentity_t, gitem_t, qboolean,
        usercmd_t, vec3_t,
    };
    use crate::ffi::python::PythonInitializationError;
    use crate::ffi::python::prelude::pyshinqlx_initialize_context;

    use crate::prelude::{QuakeLiveEngineError, serial};
    use pretty_assertions::assert_eq;

    use core::borrow::BorrowMut;
    use core::ffi::{CStr, c_char, c_int};
    use core::ptr;
    use core::sync::atomic::Ordering;
    use mockall::predicate;

    #[test]
    fn set_tag_with_no_cvar() {
        let quake_engine = default_quake_engine();

        quake_engine.set_tag();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_tag_when_tag_already_inserted() {
        let existing_tags = c"shinqlx,ca,elo";

        let mut returned = CVarBuilder::default()
            .string(existing_tags.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let cvar_find_var_ctx = Cvar_FindVar_context();
        cvar_find_var_ctx
            .expect()
            .withf_st(|&cvar_name| {
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"sv_tags"
            })
            .returning_st(move |_| returned.borrow_mut())
            .times(1);

        let cvar_set2_ctx = Cvar_Set2_context();
        cvar_set2_ctx.expect().times(0);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.set_tag();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_tag_when_tag_not_inserted_yet_with_other_values() {
        let existing_tags = c"ca,elo";

        let mut returned1 = CVarBuilder::default()
            .string(existing_tags.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let mut returned2 = CVarBuilder::default()
            .build()
            .expect("this should not happen");

        let cvar_find_var_ctx = Cvar_FindVar_context();
        cvar_find_var_ctx
            .expect()
            .withf_st(|&cvar_name| {
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"sv_tags"
            })
            .returning_st(move |_| returned1.borrow_mut())
            .times(1);

        let cvar_set2_ctx = Cvar_Set2_context();
        cvar_set2_ctx
            .expect()
            .withf(|&cvar_name, &cvar_value, &forced| {
                !cvar_name.is_null()
                    && unsafe { CStr::from_ptr(cvar_name) } == c"sv_tags"
                    && !cvar_value.is_null()
                    && unsafe { CStr::from_ptr(cvar_value) } == c"shinqlx,ca,elo"
                    && !<qboolean as Into<bool>>::into(forced)
            })
            .returning_st(move |_, _, _| returned2.borrow_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.set_tag();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_tag_when_tag_not_inserted_yet_with_empty_original_tags() {
        let existing_tags = c"";

        let mut returned1 = CVarBuilder::default()
            .string(existing_tags.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let mut returned2 = CVarBuilder::default()
            .build()
            .expect("this should not happen");

        let cvar_find_var_ctx = Cvar_FindVar_context();
        cvar_find_var_ctx
            .expect()
            .withf_st(|&cvar_name| {
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"sv_tags"
            })
            .returning_st(move |_| returned1.borrow_mut())
            .times(1);

        let cvar_set2_ctx = Cvar_Set2_context();
        cvar_set2_ctx
            .expect()
            .withf(|&cvar_name, &cvar_value, &forced| {
                !cvar_name.is_null()
                    && unsafe { CStr::from_ptr(cvar_name) } == c"sv_tags"
                    && !cvar_value.is_null()
                    && unsafe { CStr::from_ptr(cvar_value) } == c"shinqlx"
                    && !<qboolean as Into<bool>>::into(forced)
            })
            .returning_st(move |_, _, _| returned2.borrow_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.set_tag();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn initialize_cvars_with_no_cvar_returned() {
        let cvar_find_var_ctx = Cvar_FindVar_context();
        cvar_find_var_ctx
            .expect()
            .withf_st(|&cvar_name| {
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"sv_maxclients"
            })
            .returning_st(move |_| ptr::null_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.initialize_cvars();

        assert_eq!(quake_engine.sv_maxclients.load(Ordering::Acquire), 0);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn initialize_cvars_caches_cvar_value_for_maxclients() {
        let mut returned = CVarBuilder::default()
            .integer(16)
            .build()
            .expect("this should not happen");

        let cvar_find_var_ctx = Cvar_FindVar_context();
        cvar_find_var_ctx
            .expect()
            .withf_st(|&cvar_name| {
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"sv_maxclients"
            })
            .returning_st(move |_| returned.borrow_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.initialize_cvars();

        assert_eq!(quake_engine.sv_maxclients.load(Ordering::Acquire), 16);
    }

    #[test]
    fn get_maxclients_returns_stored_value() {
        let quake_engine = default_quake_engine();

        quake_engine.sv_maxclients.store(42, Ordering::Release);

        assert_eq!(quake_engine.get_max_clients(), 42);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn initialize_static_initializes_everything_successfully() {
        let add_cmd_ctx = Cmd_AddCommand_context();
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"cmd"
                    && ptr::fn_addr_eq(func, cmd_send_server_command as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"cp"
                    && ptr::fn_addr_eq(func, cmd_center_print as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"print"
                    && ptr::fn_addr_eq(func, cmd_regular_print as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"slap"
                    && ptr::fn_addr_eq(func, cmd_slap as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"slay"
                    && ptr::fn_addr_eq(func, cmd_slay as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"qlx"
                    && ptr::fn_addr_eq(func, cmd_py_rcon as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"pycmd"
                    && ptr::fn_addr_eq(func, cmd_py_command as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"pyrestart"
                    && ptr::fn_addr_eq(func, cmd_restart_python as extern "C" fn())
            })
            .times(1);

        let pyshinqlx_init_ctx = pyshinqlx_initialize_context();
        pyshinqlx_init_ctx.expect().returning(|| Ok(())).times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.initialize_static();
        assert!(result.is_ok());

        assert!(quake_engine.is_common_initialized());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn initialize_static_when_python_init_fails() {
        let add_cmd_ctx = Cmd_AddCommand_context();
        add_cmd_ctx
            .expect()
            .with(predicate::always(), predicate::always());

        let pyshinqlx_init_ctx = pyshinqlx_initialize_context();
        pyshinqlx_init_ctx
            .expect()
            .returning(|| Err(PythonInitializationError::MainScriptError))
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.initialize_static();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::PythonInitializationFailed(
                PythonInitializationError::MainScriptError
            )));

        assert!(!quake_engine.is_common_initialized());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn initialize_static_when_common_already_initiailized() {
        let add_cmd_ctx = Cmd_AddCommand_context();
        add_cmd_ctx
            .expect()
            .with(predicate::always(), predicate::always());

        let pyshinqlx_init_ctx = pyshinqlx_initialize_context();
        pyshinqlx_init_ctx.expect().returning(|| Ok(())).times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .common_initialized
            .set(true)
            .expect("this should not happen");

        let result = quake_engine.initialize_static();
        assert!(result.is_err_and(|err| err == QuakeLiveEngineError::MainEngineNotInitialized));

        assert!(quake_engine.is_common_initialized());
    }

    #[test]
    fn is_common_initialized_when_not_initialized() {
        let quake_engine = default_quake_engine();

        assert!(!quake_engine.is_common_initialized());
    }

    #[test]
    fn is_common_initialized_when_initialized_is_set_to_false() {
        let quake_engine = default_quake_engine();
        quake_engine
            .common_initialized
            .set(false)
            .expect("this should not happen");

        assert!(!quake_engine.is_common_initialized());
    }

    #[test]
    fn is_common_initialized_when_initialized_is_set_to_true() {
        let quake_engine = default_quake_engine();
        quake_engine
            .common_initialized
            .set(true)
            .expect("this should not happen");

        assert!(quake_engine.is_common_initialized());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn unhook_vm_when_restarted() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .g_init_game_orig
            .store(G_InitGame as usize, Ordering::Release);

        quake_engine.unhook_vm(true);
        assert!(
            quake_engine.g_init_game_orig().is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_InitGame))
        );
    }

    #[test]
    fn unhook_vm_when_game_not_restarted() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .g_init_game_orig
            .store(G_InitGame as usize, Ordering::Release);

        quake_engine.unhook_vm(false);
        assert!(
            quake_engine.g_init_game_orig().is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_InitGame))
        );
    }

    #[test]
    fn com_printf_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.com_printf_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Com_Printf,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn com_printf_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.com_printf_orig();
        assert!(result.is_ok_and(|func| {
            ptr::fn_addr_eq(func, Com_Printf as unsafe extern "C" fn(*const c_char, ...))
        }));
    }

    #[test]
    fn cmd_addcommand_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_addcommand_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cmd_AddCommand,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_addcommand_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_addcommand_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cmd_AddCommand as extern "C" fn(*const c_char, unsafe extern "C" fn())
        )));
    }

    #[test]
    fn cmd_args_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_args_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cmd_Args,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_args_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_args_orig();
        assert!(
            result.is_ok_and(|func| ptr::fn_addr_eq(
                func,
                Cmd_Args as extern "C" fn() -> *const c_char
            ))
        );
    }

    #[test]
    fn cmd_argv_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_argv_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cmd_Argv,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_argv_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argv_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cmd_Argv as extern "C" fn(c_int) -> *const c_char
        )));
    }

    #[test]
    fn cmd_tokenizestring_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_tokenizestring_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Tokenizestring,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_tokenizestring_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_tokenizestring_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cmd_Tokenizestring as extern "C" fn(*const c_char) -> *const c_char
        )));
    }

    #[test]
    fn cbuf_exectutetext_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cbuf_executetext_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cbuf_ExecuteText,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cbuf_executetext_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cbuf_executetext_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cbuf_ExecuteText as extern "C" fn(cbufExec_t, *const c_char)
        )));
    }

    #[test]
    fn cvar_findvar_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cvar_findvar_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cvar_FindVar,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cvar_findvar_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cvar_findvar_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cvar_FindVar as extern "C" fn(*const c_char) -> *mut cvar_t
        )));
    }

    #[test]
    fn cvar_get_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cvar_get_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cvar_Get,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cvar_get_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cvar_get_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cvar_Get as extern "C" fn(*const c_char, *const c_char, c_int) -> *mut cvar_t
        )));
    }

    #[test]
    fn cvar_getlimit_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cvar_getlimit_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cvar_GetLimit,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cvar_getlimit_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cvar_getlimit_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cvar_GetLimit
                as extern "C" fn(
                    *const c_char,
                    *const c_char,
                    *const c_char,
                    *const c_char,
                    c_int,
                ) -> *mut cvar_t
        )));
    }

    #[test]
    fn cvar_set2_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cvar_set2_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cvar_Set2,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cvar_set2_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cvar_set2_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cvar_Set2 as extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t
        )));
    }

    #[test]
    fn sv_sendservercommand_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_sendservercommand_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SendServerCommand,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_sendservercommand_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_sendservercommand_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_SendServerCommand as unsafe extern "C" fn(*mut client_t, *const c_char, ...)
        )));
    }

    #[test]
    fn sv_executeclientcommand_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_executeclientcommand_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ExecuteClientCommand,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_executeclientcommand_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_executeclientcommand_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_ExecuteClientCommand as extern "C" fn(*mut client_t, *const c_char, qboolean)
        )));
    }

    #[test]
    fn sv_shutdown_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_shutdown_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::SV_Shutdown,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_shutdown_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_shutdown_orig();
        assert!(
            result.is_ok_and(|func| ptr::fn_addr_eq(
                func,
                SV_Shutdown as extern "C" fn(*const c_char)
            ))
        );
    }

    #[test]
    fn sv_map_f_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_map_f_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::SV_Map_f,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_map_f_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_map_f_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(func, SV_Map_f as extern "C" fn())));
    }

    #[test]
    fn sv_cliententerworld_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_cliententerworld_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ClientEnterWorld,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_cliententerworld_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_cliententerworld_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_ClientEnterWorld as extern "C" fn(*mut client_t, *mut usercmd_t)
        )));
    }

    #[test]
    fn sv_setconfigstring_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_setconfigstring_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SetConfigstring,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_setconfigstring_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_setconfigstring_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_SetConfigstring as extern "C" fn(c_int, *const c_char)
        )));
    }

    #[test]
    fn sv_getconfigstring_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_getconfigstring_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_GetConfigstring,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_getconfigstring_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_getconfigstring_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_GetConfigstring as extern "C" fn(c_int, *mut c_char, c_int)
        )));
    }

    #[test]
    fn sv_dropclient_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_dropclient_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::SV_DropClient,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_dropclient_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_dropclient_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_DropClient as extern "C" fn(*mut client_t, *const c_char)
        )));
    }

    #[test]
    fn sys_setmoduleoffset_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sys_setmoduleoffset_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Sys_SetModuleOffset,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sys_moduleoffset_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sys_setmoduleoffset_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Sys_SetModuleOffset as extern "C" fn(*mut c_char, unsafe extern "C" fn())
        )));
    }

    #[test]
    fn sv_spawnserver_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_spawnserver_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::SV_SpawnServer,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_spawnserver_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_spawnserver_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_SpawnServer as extern "C" fn(*mut c_char, qboolean)
        )));
    }

    #[test]
    fn cmd_executestring_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_executestring_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_ExecuteString,
                ))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_executestring_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_executestring_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cmd_ExecuteString as extern "C" fn(*const c_char)
        )));
    }

    #[test]
    fn cmd_argc_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_argc_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cmd_Argc,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_argc_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argc_orig();
        assert!(
            result.is_ok_and(|func| ptr::fn_addr_eq(func, Cmd_Argc as extern "C" fn() -> c_int))
        );
    }

    #[test]
    fn cmd_addcommand_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_addcommand_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::Cmd_AddCommand,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_addcommand_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_addcommand_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sys_setmoduleoffset_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sys_setmoduleoffset_detour();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::Sys_SetModuleOffset,
                ))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sys_setmoduleoffset_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sys_setmoduleoffset_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_executeclientcommand_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_executeclientcommand_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_ExecuteClientCommand,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_executeclientcommand_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_executeclientcommand_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_cliententerworld_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_cliententerworld_detour();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::SV_ClientEnterWorld,
                ))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_cliententerworld_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_cliententerworld_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_setconfigstring_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_setconfgistring_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::SV_SetConfigstring,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_setconfigstring_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_setconfgistring_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_dropclient_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_dropclient_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::SV_DropClient,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_dropclient_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_dropclient_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_spawnserver_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_spawnserver_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::SV_SpawnServer,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_spawnserver_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_spawnserver_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_sendservercommand_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_sendservercommand_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_SendServerCommand,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_sendservercommand_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_sendservercommand_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn com_printf_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.com_printf_detour();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::Com_Printf,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn com_printf_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.com_printf_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn g_init_game_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.g_init_game_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_InitGame,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn g_init_game_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .g_init_game_orig
            .store(G_InitGame as usize, Ordering::Release);

        let result = quake_engine.g_init_game_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            G_InitGame as extern "C" fn(c_int, c_int, c_int)
        )));
    }

    #[test]
    fn g_shutdown_game_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.g_shutdown_game_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_ShutdownGame,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn g_shutdown_game_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .g_shutdown_game_orig
            .store(G_ShutdownGame as usize, Ordering::Release);

        let result = quake_engine.g_shutdown_game_orig();
        assert!(
            result.is_ok_and(|func| ptr::fn_addr_eq(func, G_ShutdownGame as extern "C" fn(c_int)))
        );
    }

    #[test]
    fn g_run_frame_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.g_run_frame_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_RunFrame,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn g_run_frame_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .g_run_frame_orig
            .store(G_RunFrame as usize, Ordering::Release);

        let result = quake_engine.g_run_frame_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(func, G_RunFrame as extern "C" fn(c_int))));
    }

    #[test]
    fn g_addevent_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.g_addevent_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_AddEvent,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn g_addevent_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .g_addevent_orig
            .store(G_AddEvent as usize, Ordering::Release);

        let result = quake_engine.g_addevent_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            G_AddEvent as extern "C" fn(*mut gentity_t, entity_event_t, c_int)
        )));
    }

    #[test]
    fn g_free_entity_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.g_free_entity_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_FreeEntity,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn g_free_entity_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .g_free_entity_orig
            .store(G_FreeEntity as usize, Ordering::Release);

        let result = quake_engine.g_free_entity_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            G_FreeEntity as extern "C" fn(*mut gentity_t)
        )));
    }

    #[test]
    fn launch_item_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.launch_item_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::LaunchItem,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn launch_item_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .launch_item_orig
            .store(LaunchItem as usize, Ordering::Release);

        let result = quake_engine.launch_item_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            LaunchItem as extern "C" fn(*mut gitem_t, *mut vec3_t, *mut vec3_t) -> *mut gentity_t
        )));
    }
}

#[cfg(test)]
mod quake_live_engine_test_helpers {
    use super::mock_quake_functions::*;
    use super::{QuakeLiveEngine, StaticDetours, StaticFunctions};

    use crate::ffi::c::prelude::{client_t, qboolean, usercmd_t};

    use core::ffi::{c_char, c_int};

    use retour::{GenericDetour, RawDetour};

    pub(crate) fn default_static_functions() -> StaticFunctions {
        StaticFunctions {
            com_printf_orig: Com_Printf,
            cmd_addcommand_orig: Cmd_AddCommand,
            cmd_args_orig: Cmd_Args,
            cmd_argv_orig: Cmd_Argv,
            cmd_tokenizestring_orig: Cmd_Tokenizestring,
            cbuf_executetext_orig: Cbuf_ExecuteText,
            cvar_findvar_orig: Cvar_FindVar,
            cvar_get_orig: Cvar_Get,
            cvar_getlimit_orig: Cvar_GetLimit,
            cvar_set2_orig: Cvar_Set2,
            sv_sendservercommand_orig: SV_SendServerCommand,
            sv_executeclientcommand_orig: SV_ExecuteClientCommand,
            sv_shutdown_orig: SV_Shutdown,
            sv_map_f_orig: SV_Map_f,
            sv_cliententerworld_orig: SV_ClientEnterWorld,
            sv_setconfigstring_orig: SV_SetConfigstring,
            sv_getconfigstring_orig: SV_GetConfigstring,
            sv_dropclient_orig: SV_DropClient,
            sys_setmoduleoffset_orig: Sys_SetModuleOffset,
            sv_spawnserver_orig: SV_SpawnServer,
            cmd_executestring_orig: Cmd_ExecuteString,
            cmd_argc_orig: Cmd_Argc,
        }
    }

    pub(crate) fn default_static_detours() -> StaticDetours {
        StaticDetours {
            cmd_addcommand_detour: unsafe {
                GenericDetour::new(
                    Cmd_AddCommand as extern "C" fn(*const c_char, unsafe extern "C" fn()),
                    detoured_Cmd_AddCommand,
                )
            }
            .expect("this should not happen"),
            sys_setmoduleoffset_detour: unsafe {
                GenericDetour::new(
                    Sys_SetModuleOffset as extern "C" fn(*mut c_char, unsafe extern "C" fn()),
                    detoured_Sys_SetModuleOffset,
                )
            }
            .expect("this should not happen"),
            sv_executeclientcommand_detour: unsafe {
                GenericDetour::new(
                    SV_ExecuteClientCommand
                        as extern "C" fn(*mut client_t, *const c_char, qboolean),
                    detoured_SV_ExecuteClientCommand,
                )
            }
            .expect("this should not happen"),
            sv_cliententerworld_detour: unsafe {
                GenericDetour::new(
                    SV_ClientEnterWorld as extern "C" fn(*mut client_t, *mut usercmd_t),
                    detoured_SV_ClientEnterWorld,
                )
            }
            .expect("this should not happen"),
            sv_setconfgistring_detour: unsafe {
                GenericDetour::new(
                    SV_SetConfigstring as extern "C" fn(c_int, *const c_char),
                    detoured_SV_SetConfigstring,
                )
            }
            .expect("this should not happen"),
            sv_dropclient_detour: unsafe {
                GenericDetour::new(
                    SV_DropClient as extern "C" fn(*mut client_t, *const c_char),
                    detoured_SV_DropClient,
                )
            }
            .expect("this should not happen"),
            sv_spawnserver_detour: unsafe {
                GenericDetour::new(
                    SV_SpawnServer as extern "C" fn(*mut c_char, qboolean),
                    detoured_SV_SpawnServer,
                )
            }
            .expect("this should not happen"),
            sv_sendservercommand_detour: unsafe {
                RawDetour::new(
                    SV_SendServerCommand as *const (),
                    detoured_SV_SendServerCommand as *const (),
                )
            }
            .expect("this should not happen"),
            com_printf_detour: unsafe {
                RawDetour::new(Com_Printf as *const (), detoured_Com_Printf as *const ())
            }
            .expect("this should not happen"),
        }
    }

    pub(crate) fn default_quake_engine() -> QuakeLiveEngine {
        QuakeLiveEngine::new()
    }

    #[cfg(not(tarpaulin_include))]
    pub(crate) unsafe extern "C" fn test_func() {}
}

pub(crate) trait FindCVar<T: AsRef<str>> {
    fn find_cvar(&self, name: T) -> Option<CVar>;
}

impl<T: AsRef<str>> FindCVar<T> for QuakeLiveEngine {
    fn find_cvar(&self, name: T) -> Option<CVar> {
        self.cvar_findvar_orig()
            .ok()
            .and_then(|original_func| {
                CString::new(name.as_ref())
                    .ok()
                    .map(|c_name| original_func(c_name.as_ptr()))
            })
            .and_then(|cvar| CVar::try_from(cvar).ok())
    }
}

#[cfg(test)]
mod find_cvar_quake_live_engine_tests {
    use super::{FindCVar, QuakeLiveEngine};

    use super::mock_quake_functions::Cvar_FindVar_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::CVarBuilder;

    use crate::prelude::serial;

    use core::borrow::BorrowMut;
    use core::ffi::CStr;
    use core::ptr;

    #[test]
    fn find_cvar_with_no_original_func() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.find_cvar("sv_maxclients");
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn find_cvar_when_function_returns_valid_cvar() {
        let mut cvar = CVarBuilder::default()
            .build()
            .expect("this should not happen");

        let find_cvar_ctx = Cvar_FindVar_context();
        find_cvar_ctx
            .expect()
            .withf(|&cvar_name| {
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"sv_maxclients"
            })
            .returning_st(move |_| cvar.borrow_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.find_cvar("sv_maxclients");
        assert!(result.is_some());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn find_cvar_when_function_returns_null_ptr() {
        let find_cvar_ctx = Cvar_FindVar_context();
        find_cvar_ctx
            .expect()
            .withf(|&cvar_name| {
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"sv_maxclients"
            })
            .returning(|_| ptr::null_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.find_cvar("sv_maxclients");
        assert!(result.is_none());
    }
}

pub(crate) trait AddCommand<T: AsRef<str>> {
    fn add_command(&self, cmd: T, func: unsafe extern "C" fn());
}

impl<T: AsRef<str>> AddCommand<T> for QuakeLiveEngine {
    fn add_command(&self, cmd: T, func: unsafe extern "C" fn()) {
        if let Ok(detour) = self.cmd_addcommand_detour() {
            if let Ok(c_cmd) = CString::new(cmd.as_ref()) {
                detour.call(c_cmd.as_ptr(), func)
            }
        }
    }
}

#[cfg(test)]
mod add_command_quake_live_engine_tests {
    use super::{AddCommand, QuakeLiveEngine};

    use super::mock_quake_functions::Cmd_AddCommand_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::ffi::CStr;

    #[test]
    fn add_command_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.add_command("spank", test_func);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_command_with_valid_function() {
        let add_command_ctx = Cmd_AddCommand_context();
        add_command_ctx
            .expect()
            .withf(|&cvar_name, _| {
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"spank"
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.add_command("spank", test_func);
    }
}

pub(crate) trait SetModuleOffset<T: AsRef<str>> {
    fn set_module_offset(&self, module_name: T, offset: unsafe extern "C" fn());
}

impl<T: AsRef<str>> SetModuleOffset<T> for QuakeLiveEngine {
    fn set_module_offset(&self, module_name: T, offset: unsafe extern "C" fn()) {
        if let Ok(detour) = self.sys_setmoduleoffset_detour() {
            if let Ok(c_module_name) = CString::new(module_name.as_ref()) {
                detour.call(c_module_name.as_ptr().cast_mut(), offset);
            }
        }
    }
}

#[cfg(test)]
mod set_module_offset_quake_live_engine_tests {
    use super::{QuakeLiveEngine, SetModuleOffset};

    use super::mock_quake_functions::Sys_SetModuleOffset_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::ffi::CStr;

    #[test]
    fn set_module_offset_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.set_module_offset("qagame", test_func);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_module_offset_with_valid_function() {
        let set_module_offset_ctx = Sys_SetModuleOffset_context();
        set_module_offset_ctx
            .expect()
            .withf(|&cvar_name, _| {
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"qagame"
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.set_module_offset("qagame", test_func);
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
            original_func(level_time_param, random_seed_param, restart_param);
        });
    }
}

#[cfg(test)]
mod init_game_quake_live_engine_tests {
    use super::{InitGame, QuakeLiveEngine};

    use super::mock_quake_functions::{G_InitGame, G_InitGame_context};
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::sync::atomic::Ordering;
    use mockall::predicate;

    #[test]
    fn init_game_with_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        quake_engine.init_game(42, 21, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn init_game_with_valid_function() {
        let g_init_game_ctx = G_InitGame_context();
        g_init_game_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq(21), predicate::eq(1))
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .g_init_game_orig
            .store(G_InitGame as usize, Ordering::Release);

        quake_engine.init_game(42, 21, 1);
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
            .for_each(|original_func| {
                original_func(restart_param);
            });
    }
}

#[cfg(test)]
mod shutdown_game_quake_live_engine_tests {
    use super::{QuakeLiveEngine, ShutdownGame};

    use super::mock_quake_functions::{G_ShutdownGame, G_ShutdownGame_context};
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::sync::atomic::Ordering;
    use mockall::predicate;

    #[test]
    fn shutdown_game_with_function_pointer_set() {
        let quake_engine = default_quake_engine();

        quake_engine.shutdown_game(0);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn shutdown_game_with_valid_function() {
        let g_shutdown_game_ctx = G_ShutdownGame_context();
        g_shutdown_game_ctx.expect().with(predicate::eq(1)).times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .g_shutdown_game_orig
            .store(G_ShutdownGame as usize, Ordering::Release);

        quake_engine.shutdown_game(1);
    }
}

pub(crate) trait ExecuteClientCommand<T: AsMut<client_t>, U: AsRef<str>, V: Into<qboolean>> {
    fn execute_client_command(&self, client: Option<T>, cmd: U, client_ok: V);
}

impl<T: AsMut<client_t>, U: AsRef<str>, V: Into<qboolean>> ExecuteClientCommand<T, U, V>
    for QuakeLiveEngine
{
    fn execute_client_command(&self, client: Option<T>, cmd: U, client_ok: V) {
        if let Ok(detour) = self.sv_executeclientcommand_detour() {
            if let Ok(c_command) = CString::new(cmd.as_ref()) {
                let raw_client = client.map_or(ptr::null_mut(), |mut c_client| c_client.as_mut());
                detour.call(raw_client, c_command.as_ptr(), client_ok.into());
            }
        }
    }
}

#[cfg(test)]
mod execute_client_command_quake_live_engine_tests {
    use super::{ExecuteClientCommand, QuakeLiveEngine};

    use super::mock_quake_functions::SV_ExecuteClientCommand_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::{Client, ClientBuilder, MockClient};
    use crate::prelude::serial;

    use core::ffi::CStr;

    #[test]
    fn execute_client_command_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.execute_client_command(None::<Client>, "asdf", false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn execute_client_command_with_valid_detour_function_and_no_client() {
        let sv_execute_client_command_ctx = SV_ExecuteClientCommand_context();
        sv_execute_client_command_ctx
            .expect()
            .withf(|&client, &cmd, &client_ok| {
                client.is_null()
                    && !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"asdf"
                    && client_ok.into()
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.execute_client_command(None::<Client>, "asdf", true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn execute_client_command_with_valid_detour_function_and_valid_client() {
        let mut mock_client = MockClient::default();
        mock_client.expect_as_mut().returning(|| {
            ClientBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let sv_execute_client_command_ctx = SV_ExecuteClientCommand_context();
        sv_execute_client_command_ctx
            .expect()
            .withf(|&client, &cmd, &client_ok| {
                !client.is_null()
                    && !cmd.is_null()
                    && unsafe { CStr::from_ptr(cmd) } == c"asdf"
                    && client_ok.into()
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.execute_client_command(Some(mock_client), "asdf", true);
    }
}

pub(crate) trait SendServerCommand<T: AsRef<client_t>> {
    fn send_server_command(&self, client: Option<T>, command: &str);
}

impl<T: AsRef<client_t>> SendServerCommand<T> for QuakeLiveEngine {
    fn send_server_command(&self, client: Option<T>, command: &str) {
        if let Ok(original_func) = self.sv_sendservercommand_detour().map(|detour| unsafe {
            mem::transmute::<&(), extern "C" fn(*const client_t, *const c_char, ...)>(
                detour.trampoline(),
            )
        }) {
            if let Ok(c_command) = CString::new(command) {
                let raw_client = client.map_or(ptr::null(), |c_client| c_client.as_ref());
                original_func(raw_client, c_command.as_ptr());
            }
        }
    }
}

#[cfg(test)]
mod send_server_command_quake_live_engine_tests {
    use super::{QuakeLiveEngine, SendServerCommand};

    use super::mock_quake_functions::SV_SendServerCommand_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::{Client, ClientBuilder, MockClient};
    use crate::prelude::serial;

    use core::ffi::CStr;

    #[test]
    fn send_server_command_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.send_server_command(None::<Client>, "asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_with_valid_detour_function_and_no_client() {
        let sv_send_server_command_ctx = SV_SendServerCommand_context();
        sv_send_server_command_ctx
            .expect()
            .withf(|&client, &cmd| {
                client.is_null() && !cmd.is_null() && unsafe { CStr::from_ptr(cmd) } == c"asdf"
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.send_server_command(None::<Client>, "asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_with_valid_detour_function_and_valid_client() {
        let mut mock_client = MockClient::default();
        mock_client.expect_as_ref().return_const(
            ClientBuilder::default()
                .build()
                .expect("this should not happen"),
        );

        let sv_send_server_command_ctx = SV_SendServerCommand_context();
        sv_send_server_command_ctx
            .expect()
            .withf(|&client, &cmd| {
                !client.is_null() && !cmd.is_null() && unsafe { CStr::from_ptr(cmd) } == c"asdf"
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.send_server_command(Some(mock_client), "asdf");
    }
}

pub(crate) trait ClientEnterWorld<T: AsMut<client_t>> {
    fn client_enter_world(&self, client: T, cmd: *mut usercmd_t);
}

impl<T: AsMut<client_t>> ClientEnterWorld<T> for QuakeLiveEngine {
    fn client_enter_world(&self, mut client: T, cmd: *mut usercmd_t) {
        self.sv_cliententerworld_detour().iter().for_each(|detour| {
            detour.call(client.as_mut(), cmd);
        });
    }
}

#[cfg(test)]
mod client_enter_world_quake_live_engine_tests {
    use super::{ClientEnterWorld, QuakeLiveEngine};

    use super::mock_quake_functions::SV_ClientEnterWorld_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::{ClientBuilder, MockClient, UserCmdBuilder, usercmd_t};
    use crate::prelude::serial;

    use core::borrow::BorrowMut;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_enter_world_with_no_detour_set() {
        let mock_client = MockClient::default();

        let mut usercmd = UserCmdBuilder::default()
            .build()
            .expect("this should not happen");
        let quake_engine = default_quake_engine();

        quake_engine.client_enter_world(mock_client, usercmd.borrow_mut() as *mut usercmd_t);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_enter_world_with_valid_detour_function() {
        let mut mock_client = MockClient::default();
        mock_client.expect_as_mut().returning(|| {
            ClientBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let mut usercmd = UserCmdBuilder::default()
            .build()
            .expect("this should not happen");

        let sv_client_enter_world_ctx = SV_ClientEnterWorld_context();
        sv_client_enter_world_ctx
            .expect()
            .withf(|&client, _| !client.is_null())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.client_enter_world(mock_client, usercmd.borrow_mut() as *mut usercmd_t);
    }
}

pub(crate) trait SetConfigstring<T: Into<c_int>> {
    fn set_configstring(&self, index: T, value: &str);
}

impl<T: Into<c_int>> SetConfigstring<T> for QuakeLiveEngine {
    fn set_configstring(&self, index: T, value: &str) {
        if let Ok(detour) = self.sv_setconfgistring_detour() {
            if let Ok(c_value) = CString::new(value) {
                detour.call(index.into(), c_value.as_ptr());
            }
        }
    }
}

#[cfg(test)]
mod set_confgistring_quake_live_engine_tests {
    use super::{QuakeLiveEngine, SetConfigstring};

    use super::mock_quake_functions::SV_SetConfigstring_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::ffi::CStr;

    #[test]
    fn set_configstring_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.set_configstring(42, "asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_with_valid_detour_function() {
        let sv_set_configstring_ctx = SV_SetConfigstring_context();
        sv_set_configstring_ctx
            .expect()
            .withf(|&index, &value| {
                index == 42 && !value.is_null() && unsafe { CStr::from_ptr(value) } == c"asdf"
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.set_configstring(42, "asdf");
    }
}

pub(crate) trait ComPrintf {
    fn com_printf(&self, msg: &str);
}

impl ComPrintf for QuakeLiveEngine {
    fn com_printf(&self, msg: &str) {
        if let Ok(original_func) = self.com_printf_detour().map(|detour| unsafe {
            mem::transmute::<&(), extern "C" fn(*const c_char, ...)>(detour.trampoline())
        }) {
            if let Ok(c_msg) = CString::new(msg) {
                original_func(c_msg.as_ptr());
            }
        }
    }
}

#[cfg(test)]
mod com_printf_quake_live_engine_tests {
    use super::{ComPrintf, QuakeLiveEngine};

    use super::mock_quake_functions::Com_Printf_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::ffi::CStr;

    #[test]
    fn com_printf_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.com_printf("asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn com_printf_with_valid_detour_function() {
        let com_printf_ctx = Com_Printf_context();
        com_printf_ctx
            .expect()
            .withf(|&value| !value.is_null() && unsafe { CStr::from_ptr(value) } == c"asdf")
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.com_printf("asdf");
    }
}

pub(crate) trait SpawnServer<T: AsRef<str>, U: Into<qboolean>> {
    fn spawn_server(&self, server: T, kill_bots: U);
}

impl<T: AsRef<str>, U: Into<qboolean>> SpawnServer<T, U> for QuakeLiveEngine {
    fn spawn_server(&self, server: T, kill_bots: U) {
        if let Ok(detour) = self.sv_spawnserver_detour() {
            if let Ok(c_server) = CString::new(server.as_ref()) {
                detour.call(c_server.as_ptr().cast_mut(), kill_bots.into());
            }
        }
    }
}

#[cfg(test)]
mod spawn_server_quake_live_engine_tests {
    use super::{QuakeLiveEngine, SpawnServer};

    use super::mock_quake_functions::SV_SpawnServer_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::ffi::CStr;

    #[test]
    fn spawn_server_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.spawn_server("asdf", false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn com_printf_with_valid_detour_function() {
        let sv_spawn_server_ctx = SV_SpawnServer_context();
        sv_spawn_server_ctx
            .expect()
            .withf(|&server_name, &kill_bots| {
                !server_name.is_null()
                    && unsafe { CStr::from_ptr(server_name) } == c"asdf"
                    && kill_bots.into()
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.spawn_server("asdf", true);
    }
}

pub(crate) trait RunFrame<T: Into<c_int>> {
    fn run_frame(&self, time: T);
}

impl<T: Into<c_int>> RunFrame<T> for QuakeLiveEngine {
    fn run_frame(&self, time: T) {
        let time_param = time.into();
        self.g_run_frame_orig().iter().for_each(|original_func| {
            original_func(time_param);
        });
    }
}

#[cfg(test)]
mod run_frame_quake_live_engine_tests {
    use super::{QuakeLiveEngine, RunFrame};

    use super::mock_quake_functions::{G_RunFrame, G_RunFrame_context};
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::sync::atomic::Ordering;

    #[test]
    fn run_frame_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.run_frame(21);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn run_frame_with_valid_detour_function() {
        let g_run_frame_ctx = G_RunFrame_context();
        g_run_frame_ctx.expect().withf(|&time| time == 42).times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .g_run_frame_orig
            .store(G_RunFrame as usize, Ordering::Release);

        quake_engine.run_frame(42);
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
            .map_or(ptr::null_mut(), |detour| {
                detour.call(client_num.into(), first_time.into(), is_bot.into())
            })
    }
}

#[cfg(test)]
mod client_connect_quake_live_engine_tests {
    use super::{ClientConnect, QuakeLiveEngine};

    use super::mock_quake_functions::{
        ClientConnect, ClientConnect_context, detoured_ClientConnect,
    };
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::qboolean;

    use crate::prelude::serial;
    use pretty_assertions::assert_eq;

    use retour::GenericDetour;

    use core::ffi::{CStr, c_char, c_int};

    #[test]
    fn client_connect_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.client_connect(21, false, true);
        assert!(result.is_null());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_connect_with_valid_detour_function() {
        let returned = c"expected connect return";

        let client_connect_ctx = ClientConnect_context();
        client_connect_ctx
            .expect()
            .withf(|&client_num, &first_time, &is_bot| {
                client_num == 42 && first_time.into() && !<qboolean as Into<bool>>::into(is_bot)
            })
            .returning(move |_, _, _| returned.as_ptr().cast_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine.vm_functions.client_connect_detour.store(Some(
            unsafe {
                GenericDetour::new(
                    ClientConnect as extern "C" fn(c_int, qboolean, qboolean) -> *const c_char,
                    detoured_ClientConnect,
                )
            }
            .expect("this should not happen")
            .into(),
        ));

        let result = quake_engine.client_connect(42, true, false);
        assert!(!result.is_null());
        assert_eq!(
            unsafe { CStr::from_ptr(result) },
            c"expected connect return"
        );
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

#[cfg(test)]
mod client_spawn_quake_live_engine_tests {
    use super::{ClientSpawn, QuakeLiveEngine};

    use super::mock_quake_functions::{ClientSpawn, ClientSpawn_context, detoured_ClientSpawn};
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::{GEntityBuilder, MockGameEntity, gentity_t};

    use crate::prelude::serial;

    use retour::GenericDetour;

    #[test]
    fn client_spawn_with_no_detour_set() {
        let mock_gentity = MockGameEntity::default();

        let quake_engine = default_quake_engine();

        quake_engine.client_spawn(mock_gentity);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_spawn_with_valid_detour_function() {
        let mut mock_game_entity = MockGameEntity::default();
        mock_game_entity.expect_as_mut().returning(|| {
            GEntityBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let client_spawn_ctx = ClientSpawn_context();
        client_spawn_ctx
            .expect()
            .withf(|&client| !client.is_null())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine.vm_functions.client_spawn_detour.store(Some(
            unsafe {
                GenericDetour::new(
                    ClientSpawn as extern "C" fn(*mut gentity_t),
                    detoured_ClientSpawn,
                )
            }
            .expect("this should not happen")
            .into(),
        ));

        quake_engine.client_spawn(mock_game_entity);
    }
}

pub(crate) trait CmdArgs {
    fn cmd_args(&self) -> Option<String>;
}

impl CmdArgs for QuakeLiveEngine {
    fn cmd_args(&self) -> Option<String> {
        self.cmd_args_orig()
            .map(|original_func| original_func())
            .ok()
            .filter(|cmd_args| !cmd_args.is_null())
            .map(|cmd_args| unsafe { CStr::from_ptr(cmd_args) }.to_string_lossy().into())
    }
}

#[cfg(test)]
mod cmd_args_quake_live_engine_tests {
    use super::{CmdArgs, QuakeLiveEngine};

    use super::mock_quake_functions::Cmd_Args_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::ptr;

    #[test]
    fn cmd_args_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_args();
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_args_with_valid_ogirinal_function() {
        let returned = c"expected cmd_args return";

        let cmd_args_ctx = Cmd_Args_context();
        cmd_args_ctx
            .expect()
            .returning(move || returned.as_ptr())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_args();
        assert!(result.is_some_and(|args| args == "expected cmd_args return"))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_args_when_original_function_returns_null() {
        let cmd_args_ctx = Cmd_Args_context();
        cmd_args_ctx.expect().returning(ptr::null).times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_args();
        assert!(result.is_none());
    }
}

pub(crate) trait CmdArgc {
    fn cmd_argc(&self) -> i32;
}

impl CmdArgc for QuakeLiveEngine {
    fn cmd_argc(&self) -> i32 {
        self.cmd_argc_orig()
            .map_or(0, |original_func| original_func())
    }
}

#[cfg(test)]
mod cmd_argc_quake_live_engine_tests {
    use super::{CmdArgc, QuakeLiveEngine};

    use super::mock_quake_functions::Cmd_Argc_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;
    use pretty_assertions::assert_eq;

    #[test]
    fn cmd_argc_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_argc();
        assert_eq!(result, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_argc_with_valid_original_function() {
        let cmd_argc_ctx = Cmd_Argc_context();
        cmd_argc_ctx.expect().returning(|| 42).times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argc();
        assert_eq!(result, 42);
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

#[cfg(test)]
mod cmd_argv_quake_live_engine_tests {
    use super::{CmdArgv, QuakeLiveEngine};

    use super::mock_quake_functions::Cmd_Argv_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::ptr;
    use mockall::predicate;

    #[test]
    fn cmd_argv_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_argv(1);
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_argv_with_valid_ogirinal_function() {
        let returned = c"expected cmd_argv return";

        let cmd_argv_ctx = Cmd_Argv_context();
        cmd_argv_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| returned.as_ptr())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argv(2);
        assert!(result.is_some_and(|args| args == "expected cmd_argv return"))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_argv_when_original_function_returns_null() {
        let cmd_argv_ctx = Cmd_Argv_context();
        cmd_argv_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| ptr::null())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argv(1);
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_argv_for_negative_argument_number() {
        let cmd_argv_ctx = Cmd_Argv_context();
        cmd_argv_ctx.expect().times(0);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argv(-1);
        assert!(result.is_none());
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

#[cfg(test)]
mod game_add_event_quake_live_engine_tests {
    use super::{GameAddEvent, QuakeLiveEngine};

    use super::mock_quake_functions::{G_AddEvent, G_AddEvent_context};
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::{GEntityBuilder, MockGameEntity, entity_event_t};

    use crate::prelude::serial;

    use core::sync::atomic::Ordering;

    #[test]
    fn game_add_event_with_no_original_function_set() {
        let mock_gentity = MockGameEntity::default();

        let quake_engine = default_quake_engine();

        quake_engine.game_add_event(mock_gentity, entity_event_t::EV_LIGHTNING_DISCHARGE, 1);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_add_event_with_valid_original_function() {
        let mut mock_game_entity = MockGameEntity::default();
        mock_game_entity.expect_as_mut().returning(|| {
            GEntityBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let g_add_event_ctx = G_AddEvent_context();
        g_add_event_ctx
            .expect()
            .withf(|&ent, &event, &parm| {
                !ent.is_null() && event == entity_event_t::EV_KAMIKAZE && parm == 42
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .g_addevent_orig
            .store(G_AddEvent as usize, Ordering::Release);

        quake_engine.game_add_event(mock_game_entity, entity_event_t::EV_KAMIKAZE, 42);
    }
}

pub(crate) trait ConsoleCommand<T: AsRef<str>> {
    fn execute_console_command(&self, cmd: T);
}

impl<T: AsRef<str>> ConsoleCommand<T> for QuakeLiveEngine {
    fn execute_console_command(&self, cmd: T) {
        if let Ok(original_func) = self.cmd_executestring_orig() {
            if let Ok(c_cmd) = CString::new(cmd.as_ref()) {
                original_func(c_cmd.as_ptr());
            }
        }
    }
}

#[cfg(test)]
mod console_command_quake_live_engine_tests {
    use super::{ConsoleCommand, QuakeLiveEngine};

    use super::mock_quake_functions::Cmd_ExecuteString_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;

    use core::ffi::CStr;

    #[test]
    fn execute_console_command_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        quake_engine.execute_console_command("!slap 0 100");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn execute_console_command_with_valid_original_function() {
        let cmd_execute_string_ctx = Cmd_ExecuteString_context();
        cmd_execute_string_ctx
            .expect()
            .withf(|&cmd| !cmd.is_null() && unsafe { CStr::from_ptr(cmd) } == c"!slap 0 100")
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.execute_console_command("!slap 0 100");
    }
}

pub(crate) trait GetCVar<T: AsRef<str>, U: AsRef<str>, V: Into<c_int>> {
    fn get_cvar(&self, name: T, value: U, flags: Option<V>) -> Option<CVar>;
}

impl<T: AsRef<str>, U: AsRef<str>, V: Into<c_int>> GetCVar<T, U, V> for QuakeLiveEngine {
    fn get_cvar(&self, name: T, value: U, flags: Option<V>) -> Option<CVar> {
        self.cvar_get_orig()
            .ok()
            .and_then(|original_func| {
                CString::new(name.as_ref()).ok().and_then(|c_name| {
                    CString::new(value.as_ref()).ok().map(|c_value| {
                        original_func(
                            c_name.as_ptr(),
                            c_value.as_ptr(),
                            flags.map_or(0, |real_flags| real_flags.into()),
                        )
                    })
                })
            })
            .and_then(|cvar| CVar::try_from(cvar).ok())
    }
}

#[cfg(test)]
mod get_cvar_quake_live_engine_tests {
    use super::{GetCVar, QuakeLiveEngine};

    use super::mock_quake_functions::Cvar_Get_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::CVarBuilder;
    use crate::ffi::c::prelude::cvar_flags::CVAR_CHEAT;
    use crate::prelude::serial;

    use core::borrow::BorrowMut;
    use core::ffi::{CStr, c_int};

    #[test]
    fn get_cvar_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.get_cvar("sv_maxclients", "16", None::<c_int>);
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_with_valid_original_function() {
        let cvar_name = c"sv_maxclients";
        let cvar_value = c"16";

        let mut result = CVarBuilder::default()
            .name(cvar_name.as_ptr().cast_mut())
            .string(cvar_value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let cvar_get_ctx = Cvar_Get_context();
        cvar_get_ctx
            .expect()
            .withf(|&cvar, &value, &flags| {
                !cvar.is_null()
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    && unsafe { CStr::from_ptr(value) } == c"16"
                    && flags == CVAR_CHEAT as c_int
            })
            .returning_st(move |_, _, _| result.borrow_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.get_cvar("sv_maxclients", "16", Some(CVAR_CHEAT as c_int));
        assert!(result.is_some_and(|cvar| cvar.get_string() == "16"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_cvar_with_valid_original_function_and_defaulted_flags() {
        let cvar_name = c"sv_maxclients";
        let cvar_value = c"16";

        let mut result = CVarBuilder::default()
            .name(cvar_name.as_ptr().cast_mut())
            .string(cvar_value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let cvar_get_ctx = Cvar_Get_context();
        cvar_get_ctx
            .expect()
            .withf(|&cvar, &value, &flags| {
                !cvar.is_null()
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    && unsafe { CStr::from_ptr(value) } == c"16"
                    && flags == 0
            })
            .returning_st(move |_, _, _| result.borrow_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.get_cvar("sv_maxclients", "16", None::<c_int>);
        assert!(result.is_some_and(|cvar| cvar.get_string() == "16"));
    }
}

pub(crate) trait SetCVarForced<T: AsRef<str>, U: AsRef<str>, V: Into<qboolean>> {
    fn set_cvar_forced(&self, name: T, value: U, forced: V) -> Option<CVar>;
}

impl<T: AsRef<str>, U: AsRef<str>, V: Into<qboolean>> SetCVarForced<T, U, V> for QuakeLiveEngine {
    fn set_cvar_forced(&self, name: T, value: U, forced: V) -> Option<CVar> {
        self.cvar_set2_orig()
            .ok()
            .and_then(|original_func| {
                CString::new(name.as_ref()).ok().and_then(|c_name| {
                    CString::new(value.as_ref()).ok().map(|c_value| {
                        original_func(c_name.as_ptr(), c_value.as_ptr(), forced.into())
                    })
                })
            })
            .and_then(|cvar| CVar::try_from(cvar).ok())
    }
}

#[cfg(test)]
mod set_cvar_forced_quake_live_engine_tests {
    use super::{QuakeLiveEngine, SetCVarForced};

    use super::mock_quake_functions::Cvar_Set2_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::CVarBuilder;
    use crate::prelude::serial;

    use core::borrow::BorrowMut;
    use core::ffi::CStr;

    #[test]
    fn set_cvar_forced_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.set_cvar_forced("sv_maxclients", "16", false);
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_forced_with_valid_original_function() {
        let cvar_name = c"sv_maxclients";
        let cvar_value = c"16";

        let mut result = CVarBuilder::default()
            .name(cvar_name.as_ptr().cast_mut())
            .string(cvar_value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let cvar_set2_ctx = Cvar_Set2_context();
        cvar_set2_ctx
            .expect()
            .withf(|&cvar, &value, &forced| {
                !cvar.is_null()
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    && unsafe { CStr::from_ptr(value) } == c"16"
                    && forced.into()
            })
            .returning_st(move |_, _, _| result.borrow_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.set_cvar_forced("sv_maxclients", "16", true);
        assert!(result.is_some_and(|cvar| cvar.get_string() == "16"));
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

impl<T: AsRef<str>, U: AsRef<str>, V: AsRef<str>, W: AsRef<str>, X: Into<c_int>>
    SetCVarLimit<T, U, V, W, X> for QuakeLiveEngine
{
    fn set_cvar_limit(&self, name: T, value: U, min: V, max: W, flags: Option<X>) -> Option<CVar> {
        self.cvar_getlimit_orig()
            .ok()
            .and_then(|original_func| {
                CString::new(name.as_ref()).ok().and_then(|c_name| {
                    CString::new(value.as_ref()).ok().and_then(|c_value| {
                        CString::new(min.as_ref()).ok().and_then(|c_min| {
                            CString::new(max.as_ref()).ok().map(|c_max| {
                                original_func(
                                    c_name.as_ptr(),
                                    c_value.as_ptr(),
                                    c_min.as_ptr(),
                                    c_max.as_ptr(),
                                    flags.map_or(0, |real_flags| real_flags.into()),
                                )
                            })
                        })
                    })
                })
            })
            .and_then(|cvar| CVar::try_from(cvar).ok())
    }
}

#[cfg(test)]
mod set_cvar_limit_quake_live_engine_tests {
    use super::{QuakeLiveEngine, SetCVarLimit};

    use super::mock_quake_functions::Cvar_GetLimit_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::CVarBuilder;
    use crate::ffi::c::prelude::cvar_flags::CVAR_CHEAT;

    use crate::prelude::serial;

    use core::borrow::BorrowMut;
    use core::ffi::{CStr, c_int};

    #[test]
    fn set_cvar_limit_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.set_cvar_limit("sv_maxclients", "16", "2", "64", None::<c_int>);
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_with_valid_original_function() {
        let cvar_name = c"sv_maxclients";
        let cvar_value = c"16";

        let mut result = CVarBuilder::default()
            .name(cvar_name.as_ptr().cast_mut())
            .string(cvar_value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let cvar_get_limit_ctx = Cvar_GetLimit_context();
        cvar_get_limit_ctx
            .expect()
            .withf(|&cvar, &value, &min, &max, &flags| {
                !cvar.is_null()
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    && unsafe { CStr::from_ptr(value) } == c"16"
                    && !min.is_null()
                    && unsafe { CStr::from_ptr(min) } == c"2"
                    && !max.is_null()
                    && unsafe { CStr::from_ptr(max) } == c"64"
                    && flags == CVAR_CHEAT as c_int
            })
            .returning_st(move |_, _, _, _, _| result.borrow_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.set_cvar_limit(
            "sv_maxclients",
            "16",
            "2",
            "64",
            Some(CVAR_CHEAT as c_int),
        );
        assert!(result.is_some_and(|cvar| cvar.get_string() == "16"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvar_limit_with_valid_original_function_and_defaulting_flags() {
        let cvar_name = c"sv_maxclients";
        let cvar_value = c"16";

        let mut result = CVarBuilder::default()
            .name(cvar_name.as_ptr().cast_mut())
            .string(cvar_value.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        let cvar_get_limit_ctx = Cvar_GetLimit_context();
        cvar_get_limit_ctx
            .expect()
            .withf(|&cvar, &value, &min, &max, &flags| {
                !cvar.is_null()
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    && unsafe { CStr::from_ptr(value) } == c"16"
                    && !min.is_null()
                    && unsafe { CStr::from_ptr(min) } == c"2"
                    && !max.is_null()
                    && unsafe { CStr::from_ptr(max) } == c"64"
                    && flags == 0
            })
            .returning_st(move |_, _, _, _, _| result.borrow_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.set_cvar_limit("sv_maxclients", "16", "2", "64", None::<c_int>);
        assert!(result.is_some_and(|cvar| cvar.get_string() == "16"));
    }
}

pub(crate) trait GetConfigstring<T: Into<c_int>> {
    fn get_configstring(&self, index: T) -> String;
}

impl<T: Into<c_int>> GetConfigstring<T> for QuakeLiveEngine {
    fn get_configstring(&self, index: T) -> String {
        self.sv_getconfigstring_orig()
            .map_or("".into(), |original_func| {
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
    }
}

#[cfg(test)]
mod get_configstring_quake_live_engine_tests {
    use super::{GetConfigstring, QuakeLiveEngine};

    use super::mock_quake_functions::SV_GetConfigstring_context;
    use super::quake_live_engine_test_helpers::*;

    use crate::prelude::serial;
    use pretty_assertions::assert_eq;

    #[test]
    fn get_configstring_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.get_configstring(42);
        assert_eq!(result, "");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_configstring_with_valid_original_function() {
        let sv_get_configstring_ctx = SV_GetConfigstring_context();
        sv_get_configstring_ctx
            .expect()
            .withf(|&index, &_buffer, &_buffer_len| index == 42)
            .returning(|_, buffer, buffer_len| {
                let returned = c"asdf";
                unsafe { returned.as_ptr().copy_to(buffer, buffer_len as usize) };
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.get_configstring(42);
        assert_eq!(result, "asdf");
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

#[cfg(test)]
mod register_damage_quake_live_engine_tests {
    use super::{QuakeLiveEngine, RegisterDamage};

    use super::mock_quake_functions::{G_Damage, G_Damage_context, detoured_G_Damage};
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::meansOfDeath_t::*;
    use crate::ffi::c::prelude::{
        DAMAGE_NO_PROTECTION, DAMAGE_NO_TEAM_PROTECTION, gentity_t, vec3_t,
    };

    use crate::prelude::serial;

    use core::ffi::c_int;
    use core::ptr;

    use retour::GenericDetour;

    #[test]
    fn register_damage_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.register_damage(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            42,
            DAMAGE_NO_PROTECTION as c_int,
            MOD_LIGHTNING_DISCHARGE as c_int,
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn register_damage_with_valid_detour_function() {
        let g_damage_ctx = G_Damage_context();
        g_damage_ctx
            .expect()
            .withf(
                |&target, &inflictor, &attacker, &dir, &pos, &dmg, &dflags, &means_of_death| {
                    target.is_null()
                        && inflictor.is_null()
                        && attacker.is_null()
                        && dir.is_null()
                        && pos.is_null()
                        && dmg == 42
                        && dflags == DAMAGE_NO_TEAM_PROTECTION as c_int
                        && means_of_death == MOD_BFG as c_int
                },
            )
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine.vm_functions.g_damage_detour.store(Some(
            unsafe {
                GenericDetour::new(
                    G_Damage
                        as extern "C" fn(
                            *mut gentity_t,
                            *mut gentity_t,
                            *mut gentity_t,
                            *mut vec3_t,
                            *mut vec3_t,
                            c_int,
                            c_int,
                            c_int,
                        ),
                    detoured_G_Damage,
                )
            }
            .expect("this should not happen")
            .into(),
        ));

        quake_engine.register_damage(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            42,
            DAMAGE_NO_TEAM_PROTECTION as c_int,
            MOD_BFG as c_int,
        );
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

#[cfg(test)]
mod free_entity_quake_live_engine_tests {
    use super::{FreeEntity, QuakeLiveEngine};

    use super::mock_quake_functions::{G_FreeEntity, G_FreeEntity_context};
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::{GEntityBuilder, MockGameEntity};

    use crate::prelude::serial;

    use core::borrow::BorrowMut;
    use core::sync::atomic::Ordering;

    #[test]
    fn free_entity_with_no_original_function_set() {
        let mock_gentity = MockGameEntity::default();

        let quake_engine = default_quake_engine();

        quake_engine.free_entity(mock_gentity);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn free_entity_with_valid_original_function() {
        let mut mock_gentity = MockGameEntity::new();
        mock_gentity.expect_as_mut().returning(|| {
            GEntityBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let g_free_entity_ctx = G_FreeEntity_context();
        g_free_entity_ctx
            .expect()
            .withf(|&ent| !ent.is_null())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .g_free_entity_orig
            .store(G_FreeEntity as usize, Ordering::Release);

        quake_engine.free_entity(mock_gentity.borrow_mut());
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

#[cfg(test)]
mod try_launch_item_quake_live_engine_tests {
    use super::{QuakeLiveEngine, TryLaunchItem};

    use super::mock_quake_functions::{LaunchItem, LaunchItem_context};
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::{
        GEntityBuilder, GItemBuilder, MockGameEntity, MockGameItem, vec3_t,
    };

    use crate::prelude::{QuakeLiveEngineError, serial};

    use crate::quake_live_functions::QuakeLiveFunction;

    use core::borrow::BorrowMut;
    use core::sync::atomic::Ordering;

    #[test]
    fn try_launch_item_with_no_original_function_set() {
        let mock_item = MockGameItem::default();
        let mut origin = vec3_t::default();
        let mut velocity = vec3_t::default();

        let quake_engine = default_quake_engine();

        let result =
            quake_engine.try_launch_item(mock_item, origin.borrow_mut(), velocity.borrow_mut());
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::LaunchItem,))
        )
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_launch_item_with_valid_original_function() {
        let mut mock_item = MockGameItem::default();
        mock_item.expect_as_mut().returning(|| {
            GItemBuilder::default()
                .build()
                .expect("this should not happen")
        });
        let mut origin = vec3_t::default();
        let mut velocity = vec3_t::default();

        let launch_item_ctx = LaunchItem_context();
        launch_item_ctx
            .expect()
            .withf(|&item, &_pos, &_dir| !item.is_null())
            .returning(|_, _, _| {
                let mut returned = GEntityBuilder::default()
                    .build()
                    .expect("this should not happen");
                returned.borrow_mut()
            })
            .times(1);

        let gentity_try_from_ctx = MockGameEntity::try_from_context();
        gentity_try_from_ctx
            .expect()
            .returning(|_| Ok(MockGameEntity::default()))
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .launch_item_orig
            .store(LaunchItem as usize, Ordering::Release);

        let result =
            quake_engine.try_launch_item(mock_item, origin.borrow_mut(), velocity.borrow_mut());
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn try_launch_item_with_valid_original_function_when_invalid_gentity_returned() {
        let mut mock_item = MockGameItem::default();
        mock_item.expect_as_mut().returning(|| {
            GItemBuilder::default()
                .build()
                .expect("this should not happen")
        });
        let mut origin = vec3_t::default();
        let mut velocity = vec3_t::default();

        let launch_item_ctx = LaunchItem_context();
        launch_item_ctx
            .expect()
            .withf(|&item, &_pos, &_dir| !item.is_null())
            .returning(|_, _, _| {
                let mut returned = GEntityBuilder::default()
                    .build()
                    .expect("this should not happen");
                returned.borrow_mut()
            })
            .times(1);

        let gentity_try_from_ctx = MockGameEntity::try_from_context();
        gentity_try_from_ctx
            .expect()
            .returning(|_| {
                Err(QuakeLiveEngineError::NullPointerPassed(
                    "null pointer passed".to_string(),
                ))
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .launch_item_orig
            .store(LaunchItem as usize, Ordering::Release);

        let result =
            quake_engine.try_launch_item(mock_item, origin.borrow_mut(), velocity.borrow_mut());
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::NullPointerPassed("null pointer passed".to_string(),)));
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
mod start_kamikaze_quake_live_engine_tests {
    use super::{QuakeLiveEngine, StartKamikaze};

    use super::mock_quake_functions::{
        G_StartKamikaze, G_StartKamikaze_context, detoured_G_StartKamikaze,
    };
    use super::quake_live_engine_test_helpers::*;

    use crate::ffi::c::prelude::{GEntityBuilder, MockGameEntity, gentity_t};

    use crate::prelude::serial;

    use retour::GenericDetour;

    #[test]
    fn register_damage_with_no_detour_set() {
        let mock_gentity = MockGameEntity::default();
        let quake_engine = default_quake_engine();

        quake_engine.start_kamikaze(mock_gentity);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn start_kamikaze_with_valid_detour_function() {
        let mut mock_gentity = MockGameEntity::default();
        mock_gentity.expect_as_mut().returning(|| {
            GEntityBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let g_start_kamikaze_ctx = G_StartKamikaze_context();
        g_start_kamikaze_ctx
            .expect()
            .withf(|&ent| !ent.is_null())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .g_start_kamikaze_detour
            .store(Some(
                unsafe {
                    GenericDetour::new(
                        G_StartKamikaze as extern "C" fn(*mut gentity_t),
                        detoured_G_StartKamikaze,
                    )
                }
                .expect("this should not happen")
                .into(),
            ));

        quake_engine.start_kamikaze(mock_gentity);
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

#[cfg(test)]
pub(crate) struct MockEngineBuilder {
    mock_engine: Option<MockQuakeEngine>,
}

#[cfg(test)]
impl MockEngineBuilder {
    pub(crate) fn configure<F>(mut self, setup: F) -> MockEngineBuilder
    where
        F: FnOnce(&mut MockQuakeEngine),
    {
        self.mock_engine.as_mut().map(setup);
        self
    }

    pub(crate) fn with_max_clients(self, max_clients: i32) -> MockEngineBuilder {
        self.configure(|mock_engine| {
            mock_engine
                .expect_get_max_clients()
                .return_const(max_clients);
        })
    }

    pub(crate) fn with_com_printf<F, G>(self, predicate: F, times: G) -> MockEngineBuilder
    where
        F: mockall::Predicate<str> + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine.expect_com_printf().with(predicate).times(times);
        })
    }

    pub(crate) fn with_send_server_command<F, G>(self, matcher: F, times: G) -> MockEngineBuilder
    where
        F: Fn(&Option<MockClient>, &str) -> bool + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_send_server_command()
                .withf(matcher)
                .times(times);
        })
    }

    pub(crate) fn with_execute_client_command<F, G>(self, matcher: F, times: G) -> MockEngineBuilder
    where
        F: Fn(&Option<MockClient>, &String, &qboolean) -> bool + Send + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_execute_client_command()
                .withf(matcher)
                .times(times);
        })
    }

    pub(crate) fn with_execute_console_command<F, G>(
        self,
        expected_cmd: F,
        times: G,
    ) -> MockEngineBuilder
    where
        F: ToString + Send + Sync + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_execute_console_command()
                .withf(move |cmd| cmd == expected_cmd.to_string())
                .times(times);
        })
    }

    pub(crate) fn with_argc(self, argc: i32) -> MockEngineBuilder {
        self.configure(|mock_engine| {
            mock_engine.expect_cmd_argc().return_const_st(argc);
        })
    }

    pub(crate) fn with_argv<F, G, H>(
        self,
        argv: F,
        opt_return: Option<G>,
        times: H,
    ) -> MockEngineBuilder
    where
        F: mockall::Predicate<i32> + Send + 'static,
        G: ToString + Sync + Send + 'static,
        H: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_cmd_argv()
                .with(argv)
                .return_const_st(opt_return.map(move |return_str| return_str.to_string()))
                .times(times);
        })
    }

    pub(crate) fn with_args<F>(
        self,
        opt_return: Option<&'static str>,
        times: F,
    ) -> MockEngineBuilder
    where
        F: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_cmd_args()
                .return_const_st(opt_return.map(move |return_str| return_str.to_string()))
                .times(times);
        })
    }

    pub(crate) fn with_find_cvar<F, G, H>(
        self,
        expect: F,
        returned: G,
        times: H,
    ) -> MockEngineBuilder
    where
        F: Fn(&str) -> bool + Send + 'static,
        G: FnMut(&str) -> Option<CVar> + 'static,
        H: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_find_cvar()
                .withf(expect)
                .returning_st(returned)
                .times(times);
        })
    }

    pub(crate) fn with_get_configstring<F, G>(
        self,
        matcher: u16,
        returned: F,
        times: G,
    ) -> MockEngineBuilder
    where
        F: ToString + Send + Sync + 'static,
        G: Into<mockall::TimesRange>,
    {
        self.configure(|mock_engine| {
            mock_engine
                .expect_get_configstring()
                .with(predicate::eq(matcher))
                .returning(move |_| returned.to_string())
                .times(times);
        })
    }

    pub(crate) fn run<F>(&mut self, execute: F)
    where
        F: FnOnce(),
    {
        let engine = self.mock_engine.take();
        crate::MAIN_ENGINE.store(engine.map(|mock_engine| mock_engine.into()));
        execute();
        crate::MAIN_ENGINE.store(None);
    }
}

#[cfg(test)]
impl Default for MockEngineBuilder {
    fn default() -> Self {
        MockEngineBuilder {
            mock_engine: Some(MockQuakeEngine::default()),
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, mockall::automock)]
#[cfg_attr(test, allow(dead_code))]
mod quake_functions {
    use crate::ffi::c::prelude::{
        cbufExec_t, client_t, cvar_t, entity_event_t, gentity_t, gitem_t, qboolean, trace_t,
        usercmd_t, vec3_t,
    };

    use core::ffi::{c_char, c_float, c_int};
    use core::ptr;

    #[allow(unused_attributes, clippy::just_underscores_and_digits, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) unsafe extern "C" fn Com_Printf(_fmt: *const c_char, ...) {}

    #[allow(unused_attributes, clippy::just_underscores_and_digits, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) unsafe extern "C" fn detoured_Com_Printf(_fmt: *const c_char, ...) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_AddCommand(_cmd: *const c_char, _func: unsafe extern "C" fn()) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_Cmd_AddCommand(
        _cmd: *const c_char,
        _func: unsafe extern "C" fn(),
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_Args() -> *const c_char {
        ptr::null()
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_Argv(_arg: c_int) -> *const c_char {
        ptr::null()
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_Tokenizestring(_text_in: *const c_char) -> *const c_char {
        ptr::null()
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cbuf_ExecuteText(_exec_when: cbufExec_t, _text: *const c_char) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cvar_FindVar(_var_name: *const c_char) -> *mut cvar_t {
        ptr::null_mut()
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cvar_Get(
        _var_name: *const c_char,
        _var_value: *const c_char,
        _flags: c_int,
    ) -> *mut cvar_t {
        ptr::null_mut()
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cvar_GetLimit(
        _var_name: *const c_char,
        _var_value: *const c_char,
        _min: *const c_char,
        _max: *const c_char,
        _flags: c_int,
    ) -> *mut cvar_t {
        ptr::null_mut()
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cvar_Set2(
        _var_name: *const c_char,
        _value: *const c_char,
        _force: qboolean,
    ) -> *mut cvar_t {
        ptr::null_mut()
    }

    #[allow(unused_attributes, clippy::just_underscores_and_digits, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) unsafe extern "C" fn SV_SendServerCommand(
        _cl: *mut client_t,
        _fmt: *const c_char,
        ...
    ) {
    }

    #[allow(unused_attributes, clippy::just_underscores_and_digits, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) unsafe extern "C" fn detoured_SV_SendServerCommand(
        _cl: *mut client_t,
        _fmt: *const c_char,
        ...
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_ExecuteClientCommand(
        _cl: *mut client_t,
        _s: *const c_char,
        _clientOK: qboolean,
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_ExecuteClientCommand(
        _cl: *mut client_t,
        _s: *const c_char,
        _clientOK: qboolean,
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_Shutdown(_finalmsg: *const c_char) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_Map_f() {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_ClientEnterWorld(_client: *mut client_t, _cmd: *mut usercmd_t) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_ClientEnterWorld(
        _client: *mut client_t,
        _cmd: *mut usercmd_t,
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_SetConfigstring(_index: c_int, _value: *const c_char) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_SetConfigstring(_index: c_int, _value: *const c_char) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_GetConfigstring(
        _index: c_int,
        _buffer: *mut c_char,
        _bufferSize: c_int,
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_DropClient(_drop: *mut client_t, _reason: *const c_char) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_DropClient(_drop: *mut client_t, _reason: *const c_char) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Sys_SetModuleOffset(
        _moduleName: *mut c_char,
        _offset: unsafe extern "C" fn(),
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_Sys_SetModuleOffset(
        _moduleName: *mut c_char,
        _offset: unsafe extern "C" fn(),
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn SV_SpawnServer(_server: *mut c_char, _killBots: qboolean) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_SV_SpawnServer(_server: *mut c_char, _killBots: qboolean) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_ExecuteString(_text: *const c_char) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Cmd_Argc() -> c_int {
        0
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_InitGame(_level_time: c_int, _random_see: c_int, _restart: c_int) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_ShutdownGame(_restart: c_int) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_RunFrame(_time: c_int) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn ClientConnect(
        _client_num: c_int,
        _first_time: qboolean,
        _is_bot: qboolean,
    ) -> *const c_char {
        ptr::null_mut()
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_ClientConnect(
        _client_num: c_int,
        _first_time: qboolean,
        _is_bot: qboolean,
    ) -> *const c_char {
        ptr::null_mut()
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn ClientSpawn(_client: *mut gentity_t) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_ClientSpawn(_client: *mut gentity_t) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_AddEvent(
        _ent: *mut gentity_t,
        _event: entity_event_t,
        _eventParm: c_int,
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_Damage(
        _target: *mut gentity_t,
        _inflictor: *mut gentity_t,
        _attacker: *mut gentity_t,
        _dir: *mut vec3_t,
        _pos: *mut vec3_t,
        _damage: c_int,
        _dflags: c_int,
        _means_of_death: c_int,
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_G_Damage(
        _target: *mut gentity_t,
        _inflictor: *mut gentity_t,
        _attacker: *mut gentity_t,
        _dir: *mut vec3_t,
        _pos: *mut vec3_t,
        _damage: c_int,
        _dflags: c_int,
        _means_of_death: c_int,
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_FreeEntity(_ent: *mut gentity_t) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn LaunchItem(
        _item: *mut gitem_t,
        _origin: *mut vec3_t,
        _velocity: *mut vec3_t,
    ) -> *mut gentity_t {
        ptr::null_mut()
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn G_StartKamikaze(_ent: *mut gentity_t) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn detoured_G_StartKamikaze(_ent: *mut gentity_t) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn CheckPrivileges(_ent: *mut gentity_t, _cmd: *mut c_char) {}

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Touch_Item(
        _ent: *mut gentity_t,
        _other: *mut gentity_t,
        _trace: *mut trace_t,
    ) {
    }

    #[allow(unused_attributes, non_snake_case)]
    #[cfg(not(tarpaulin_include))]
    pub(crate) extern "C" fn Drop_Item(
        _ent: *mut gentity_t,
        _item: *mut gitem_t,
        _angle: c_float,
    ) -> *mut gentity_t {
        ptr::null_mut()
    }
}
