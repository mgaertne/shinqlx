use alloc::ffi::CString;
use core::{
    ffi::{c_char, c_int},
    hint::cold_path,
    sync::atomic::{AtomicUsize, Ordering},
};

use detour::TypedDetour;
#[cfg(all(feature = "fps-patch", target_os = "linux"))]
use procfs::process::MMPermissions;
#[cfg(target_os = "linux")]
use procfs::process::MemoryMap;
#[cfg(target_os = "linux")]
use procfs::process::{MMapPath, Process};
use rayon::prelude::*;
use tap::TapFallible;

#[cfg(target_os = "linux")]
use crate::engine::patterns::pattern_search_module;
#[cfg(all(target_os = "linux", not(test)))]
use crate::engine::qagame::stats_feed;
#[cfg(target_os = "linux")]
use crate::engine::symbols::try_find_static_function;
#[cfg(test)]
use crate::engine::{mock_quake_functions, test_helpers};
#[cfg(all(feature = "fps-patch", target_os = "linux"))]
use crate::fixes::sv_fps_patch::{ModuleRegion, patch_sv_fps};
use crate::{
    engine::{
        QuakeLiveEngine, QuakeLiveEngineError,
        callbacks::EngineCallbacks,
        patterns::QuakeLiveFunction,
        qagame::{entity::EntityFunctions, game::GameFunctions, item::ItemFunctions},
        *,
    },
    prelude::*,
};

/// The functions of the game module, which are searched and hooked anew for every map.
#[derive(Default)]
pub(in crate::engine) struct VmFunctions {
    pub(in crate::engine) vm_call_table: AtomicUsize,
    pub(in crate::engine) game: GameFunctions,
    pub(in crate::engine) entity: EntityFunctions,
    pub(in crate::engine) item: ItemFunctions,
}

#[cfg_attr(
    any(not(target_os = "linux"), test),
    allow(
        dead_code,
        reason = "stand-ins for engine symbols that not every test calls"
    )
)]
const OFFSET_VM_CALL_TABLE: usize = 0x3;
#[cfg_attr(
    any(not(target_os = "linux"), test),
    allow(
        dead_code,
        reason = "stand-ins for engine symbols that not every test calls"
    )
)]
const OFFSET_INITGAME: usize = 0x18;
#[cfg_attr(
    any(not(target_os = "linux"), test),
    allow(
        dead_code,
        reason = "stand-ins for engine symbols that not every test calls"
    )
)]
const OFFSET_RUNFRAME: usize = 0x8;

impl VmFunctions {
    pub(crate) fn try_initialize_from(
        &self,
        #[allow(unused_variables, reason = "only used on Linux")] module_offset: usize,
    ) -> Result<(), QuakeLiveEngineError> {
        #[cfg(not(target_os = "linux"))]
        return Err(QuakeLiveEngineError::ProcessNotFound(
            "could not find my own process".to_string(),
        ));
        #[cfg(target_os = "linux")]
        {
            let myself_process = Process::myself().map_err(|_| {
                cold_path();
                QuakeLiveEngineError::ProcessNotFound("could not find my own process".to_string())
            })?;
            let myself_maps = myself_process.maps().map_err(|_| {
                cold_path();
                QuakeLiveEngineError::NoMemoryMappingInformationFound(
                    "no memory mapping information found".to_string(),
                )
            })?;

            let qagame_maps = myself_maps
                .iter()
                .filter(|mmap| {
                    let MMapPath::Path(path) = &mmap.pathname else {
                        return false;
                    };
                    path.file_name()
                        .is_some_and(|file_name| file_name.to_string_lossy() == QAGAME)
                })
                .collect::<Vec<_>>();

            debug!(target: "shinqlx", func = "try_initialize_from"; "Searching for necessary VM functions...");
            let failed_functions = self
                .entity
                .symbols()
                .into_iter()
                .chain(self.game.symbols())
                .chain(self.item.symbols())
                .collect::<Vec<_>>()
            .par_iter()
            .filter_map(
                |(ql_func, field)| match pattern_search_module(&qagame_maps, ql_func) {
                    None => Some(*ql_func),
                    Some(orig_func) => {
                        debug!(target: "shinqlx", func = "try_initialize_from"; "{ql_func}: {orig_func:#X}");
                        field.store(orig_func, Ordering::Release);
                        None
                    }
                },
            )
            .collect::<Vec<_>>();

            if !failed_functions.is_empty() {
                cold_path();
                return Err(QuakeLiveEngineError::VmFunctionNotFound(
                    failed_functions[0],
                ));
            }

            self.read_vm_call_table(module_offset);
            Ok(())
        }
    }

    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn read_vm_call_table(&self, module_offset: usize) {
        // SAFETY: the address derives from a function of the loaded engine, whose code is readable.
        let base_address = unsafe {
            ptr::read_unaligned((module_offset as u64 + OFFSET_VM_CALL_TABLE as u64) as *const i32)
        };
        let vm_call_table = base_address as usize + module_offset + OFFSET_VM_CALL_TABLE + 4;
        self.vm_call_table.store(vm_call_table, Ordering::Release);

        // SAFETY: the call table of the qagame module is readable.
        let g_initgame_orig = unsafe {
            ptr::read(
                (vm_call_table + OFFSET_INITGAME)
                    as *const *const extern "C" fn(c_int, c_int, c_int),
            )
        };
        debug!(target: "shinqlx", func = "read_vm_call_table"; "G_InitGame: {:#X}", g_initgame_orig as usize);
        self.game
            .g_init_game_orig
            .store(g_initgame_orig as usize, Ordering::Release);

        let g_shutdowngame_orig =
            // SAFETY: the call table of the qagame module is readable.
            unsafe { ptr::read_unaligned(vm_call_table as *const *const extern "C" fn(c_int)) };
        debug!(target: "shinqlx", func = "read_vm_call_table"; "G_ShutdownGame: {:#X}", g_shutdowngame_orig as usize);
        self.game
            .g_shutdown_game_orig
            .store(g_shutdowngame_orig as usize, Ordering::Release);

        // SAFETY: the call table of the qagame module is readable.
        let g_runframe_orig = unsafe {
            ptr::read((vm_call_table + OFFSET_RUNFRAME) as *const *const extern "C" fn(c_int))
        };
        debug!(target: "shinqlx", func = "read_vm_call_table"; "G_RunFrame: {:#X}", g_runframe_orig as usize);
        self.game
            .g_run_frame_orig
            .store(g_runframe_orig as usize, Ordering::Release);
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
    pub(crate) fn hook(&self, callbacks: &EngineCallbacks) -> Result<(), QuakeLiveEngineError> {
        let vm_call_table = self.vm_call_table.load(Ordering::Acquire);

        debug!(target: "shinqlx", func = "hook"; "Hooking VM functions...");
        // SAFETY: the call table of the qagame module is writable after the memory protection was lifted.
        unsafe {
            ptr::write(
                (vm_call_table + 0x18) as *mut usize,
                callbacks.vm.g_initgame as *const () as usize,
            );
        }

        // SAFETY: the call table of the qagame module is writable after the memory protection was lifted.
        unsafe {
            ptr::write(
                (vm_call_table + 0x8) as *mut usize,
                callbacks.vm.g_runframe as *const () as usize,
            );
        }

        // SAFETY: the call table of the qagame module is writable after the memory protection was lifted.
        unsafe {
            ptr::write(
                vm_call_table as *mut usize,
                callbacks.vm.g_shutdowngame as *const () as usize,
            );
        }

        self.game.hook(&callbacks.game)?;
        self.entity.hook(&callbacks.entity)?;
        self.item.hook(&callbacks.item)
    }

    pub(crate) fn unhook(&self) {
        self.vm_call_table.store(0, Ordering::Release);
        self.game.unhook();
        self.entity.unhook();
        self.item.unhook();
    }
}

impl QuakeLiveEngine {
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(crate) fn initialize_vm(&self, module_offset: usize) -> Result<(), QuakeLiveEngineError> {
        self.vm_functions.try_initialize_from(module_offset)?;
        self.current_vm.store(module_offset, Ordering::Release);

        self.vm_functions.hook(self.callbacks())?;
        #[cfg(feature = "clientkick-fix")]
        self.vm_functions.game.patch();
        #[cfg(all(feature = "fps-patch", target_os = "linux"))]
        self.patch_sv_fps();

        Ok(())
    }

    #[cfg(all(feature = "fps-patch", target_os = "linux"))]
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(in crate::engine) fn patch_sv_fps(&self) {
        let regions = Process::myself()
            .and_then(|process| process.maps())
            .map(|maps| {
                maps.iter()
                    .filter(|mmap| {
                        matches!(&mmap.pathname, MMapPath::Path(path)
                            if path.file_name().is_some_and(|name| name.to_string_lossy() == QAGAME))
                    })
                    .filter(|mmap| mmap.perms.contains(MMPermissions::READ))
                    .map(|mmap| ModuleRegion {
                        range: mmap.address.0 as usize..mmap.address.1 as usize,
                        writable: mmap.perms.contains(MMPermissions::WRITE),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let _ = patch_sv_fps(&regions)
            .tap_ok(|outcome| {
                debug!(target: "shinqlx", func = "patch_sv_fps"; "sv_fps default value 125: {outcome:?}");
            })
            .tap_err(|e| {
                error!(target: "shinqlx", func = "patch_sv_fps"; "could not set the default value of sv_fps to 125: {e}");
            });
    }

    pub(crate) fn unhook_vm(&self, _restart: bool) {
        #[cfg(all(target_os = "linux", not(test)))]
        stats_feed::forget_writer();
        self.vm_functions.unhook();
    }

    /// Hooks the engine function that publishes the stats events, so that plugins get them without ZMQ.
    #[cfg(all(target_os = "linux", not(test)))]
    pub(crate) fn hook_stats_feed(&self) {
        stats_feed::install(
            self.vm_functions
                .entity
                .stats_medal_orig
                .load(Ordering::Acquire),
        );
    }
}

#[cfg(test)]
mod vm_functions_tests {
    use core::{
        ptr,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use pretty_assertions::assert_eq;

    use super::{QuakeLiveEngineError, VmFunctions};
    use crate::{
        composition::ENGINE_CALLBACKS,
        engine::{
            mock_quake_functions::{
                CheckPrivileges, ClientConnect, ClientSpawn, Drop_Item, FireWeapon, G_AddEvent,
                G_Damage, G_FreeEntity, G_InitGame, G_RunFrame, G_StartKamikaze, G_StatsFlagStatus,
                G_StatsMedal, LaunchItem, Touch_Item, detoured_ClientConnect, detoured_ClientSpawn,
                detoured_G_Damage, detoured_G_StartKamikaze,
            },
            qagame::{
                entity::{EntityFunctions, GDamageDetourType, GStartKamikazeDetourType},
                game::{ClientConnectDetourType, ClientSpawnDetourType, GameFunctions},
                item::ItemFunctions,
            },
        },
        hooks::{shinqlx_g_initgame, shinqlx_g_runframe, shinqlx_g_shutdowngame},
        prelude::serial,
    };

    pub(in crate::engine) fn default_vm_functions() -> VmFunctions {
        VmFunctions::default()
    }

    fn address_of(function: *const ()) -> AtomicUsize {
        AtomicUsize::new(function as usize)
    }

    fn vm_functions_with_stand_ins() -> VmFunctions {
        VmFunctions {
            vm_call_table: AtomicUsize::new(42),
            game: GameFunctions {
                check_privileges_orig: address_of(CheckPrivileges as *const ()),
                client_connect_orig: address_of(ClientConnect as *const ()),
                client_spawn_orig: address_of(ClientSpawn as *const ()),
                g_init_game_orig: address_of(G_InitGame as *const ()),
                g_run_frame_orig: address_of(G_RunFrame as *const ()),
                ..Default::default()
            },
            entity: EntityFunctions {
                g_addevent_orig: address_of(G_AddEvent as *const ()),
                g_damage_orig: address_of(G_Damage as *const ()),
                fire_weapon_orig: address_of(FireWeapon as *const ()),
                stats_medal_orig: address_of(G_StatsMedal as *const ()),
                stats_flag_status_orig: address_of(G_StatsFlagStatus as *const ()),
                g_start_kamikaze_orig: address_of(G_StartKamikaze as *const ()),
                g_free_entity_orig: address_of(G_FreeEntity as *const ()),
                ..Default::default()
            },
            item: ItemFunctions {
                touch_item_orig: address_of(Touch_Item as *const ()),
                launch_item_orig: address_of(LaunchItem as *const ()),
                drop_item_orig: address_of(Drop_Item as *const ()),
                ..Default::default()
            },
        }
    }

    fn assert_unhooked(vm_functions: &VmFunctions) {
        [
            &vm_functions.vm_call_table,
            &vm_functions.entity.g_addevent_orig,
            &vm_functions.game.check_privileges_orig,
            &vm_functions.game.client_connect_orig,
            &vm_functions.game.client_spawn_orig,
            &vm_functions.entity.g_damage_orig,
            &vm_functions.item.touch_item_orig,
            &vm_functions.item.launch_item_orig,
            &vm_functions.item.drop_item_orig,
            &vm_functions.entity.g_start_kamikaze_orig,
            &vm_functions.entity.g_free_entity_orig,
            &vm_functions.game.g_init_game_orig,
            &vm_functions.game.g_run_frame_orig,
        ]
        .iter()
        .for_each(|field| assert_eq!(field.load(Ordering::Acquire), 0));

        assert!(vm_functions.game.client_connect_detour.load().is_none());
        assert!(vm_functions.entity.g_start_kamikaze_detour.load().is_none());
        assert!(vm_functions.game.client_spawn_detour.load().is_none());
        assert!(vm_functions.entity.g_damage_detour.load().is_none());
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn try_initialize_from_without_qagame_module_reports_missing_function() {
        let vm_functions = default_vm_functions();

        let result = vm_functions.try_initialize_from(0);

        assert!(matches!(
            result,
            Err(QuakeLiveEngineError::VmFunctionNotFound(_))
        ));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn read_vm_call_table_reads_the_function_pointers() {
        let memory = [9usize << 24, 0, 0x1111, 0x2222, 0, 0x3333, 0, 0];
        let module_offset = memory.as_ptr() as usize;
        let vm_functions = default_vm_functions();

        vm_functions.read_vm_call_table(module_offset);

        assert_eq!(
            vm_functions.vm_call_table.load(Ordering::Acquire),
            module_offset + 16
        );
        assert_eq!(
            vm_functions
                .game
                .g_shutdown_game_orig
                .load(Ordering::Acquire),
            0x1111
        );
        assert_eq!(
            vm_functions.game.g_run_frame_orig.load(Ordering::Acquire),
            0x2222
        );
        assert_eq!(
            vm_functions.game.g_init_game_orig.load(Ordering::Acquire),
            0x3333
        );
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    pub(in crate::engine) fn hook_replaces_the_vm_calls_and_enables_the_detours() {
        let mut table = [0usize; 4];
        let table_pointer = table.as_mut_ptr();
        let table_address = table_pointer.expose_provenance();
        let vm_functions = VmFunctions {
            vm_call_table: AtomicUsize::new(table_address),
            ..vm_functions_with_stand_ins()
        };

        let result = vm_functions.hook(&ENGINE_CALLBACKS);

        assert!(result.is_ok());
        // SAFETY: the table holds at least as many entries as the index.
        let read_entry = |index: usize| unsafe { ptr::read_volatile(table_pointer.add(index)) };
        assert_eq!(read_entry(0), shinqlx_g_shutdowngame as *const () as usize);
        assert_eq!(read_entry(1), shinqlx_g_runframe as *const () as usize);
        assert_eq!(read_entry(3), shinqlx_g_initgame as *const () as usize);
        assert!(
            vm_functions
                .game
                .client_connect_detour
                .load()
                .as_ref()
                .is_some_and(|detour| detour.is_enabled())
        );
        assert!(
            vm_functions
                .entity
                .g_damage_detour
                .load()
                .as_ref()
                .is_some_and(|detour| detour.is_enabled())
        );
        assert!(
            vm_functions
                .item
                .touch_item_detour
                .load()
                .as_ref()
                .is_some_and(|detour| detour.is_enabled())
        );

        vm_functions.unhook();
    }

    #[test]
    pub(in crate::engine) fn unhook_with_no_functions_set_before() {
        let vm_functions = default_vm_functions();

        vm_functions.unhook();
        assert_unhooked(&vm_functions);
    }

    #[test]
    pub(in crate::engine) fn unhook_with_functions_set_before_but_no_detours() {
        let vm_functions = vm_functions_with_stand_ins();

        vm_functions.unhook();

        assert_unhooked(&vm_functions);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    pub(in crate::engine) fn unhook_with_functions_and_disabled_detours_set_before() {
        let vm_functions = vm_functions_with_stand_ins();

        let client_connect_detour =
            // SAFETY: the original and the replacement are functions with the same signature.
            unsafe { ClientConnectDetourType::new(ClientConnect, detoured_ClientConnect) }
                .expect("this should not happen");
        vm_functions
            .game
            .client_connect_detour
            .store(Some(client_connect_detour.into()));
        let g_start_kamikaze_detour =
            // SAFETY: the original and the replacement are functions with the same signature.
            unsafe { GStartKamikazeDetourType::new(G_StartKamikaze, detoured_G_StartKamikaze) }
                .expect("this should not happen");
        vm_functions
            .entity
            .g_start_kamikaze_detour
            .store(Some(g_start_kamikaze_detour.into()));
        let client_spawn_detour =
            // SAFETY: the original and the replacement are functions with the same signature.
            unsafe { ClientSpawnDetourType::new(ClientSpawn, detoured_ClientSpawn) }
                .expect("this should not happen");
        vm_functions
            .game
            .client_spawn_detour
            .store(Some(client_spawn_detour.into()));
        // SAFETY: the original and the replacement are functions with the same signature.
        let g_damage_detour = unsafe { GDamageDetourType::new(G_Damage, detoured_G_Damage) }
            .expect("this should not happen");
        vm_functions
            .entity
            .g_damage_detour
            .store(Some(g_damage_detour.into()));

        vm_functions.unhook();

        assert_unhooked(&vm_functions);
    }

    #[test]
    #[cfg_attr(any(miri, target_os = "macos"), ignore)]
    #[serial]
    pub(in crate::engine) fn unhook_with_functions_and_enabled_detours_set_before() {
        let vm_functions = vm_functions_with_stand_ins();

        let client_connect_detour =
            // SAFETY: the original and the replacement are functions with the same signature.
            unsafe { ClientConnectDetourType::new(ClientConnect, detoured_ClientConnect) }
                .expect("this should not happen");
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { client_connect_detour.enable() }.expect("this should not happen");
        vm_functions
            .game
            .client_connect_detour
            .store(Some(client_connect_detour.into()));
        let g_start_kamikaze_detour =
            // SAFETY: the original and the replacement are functions with the same signature.
            unsafe { GStartKamikazeDetourType::new(G_StartKamikaze, detoured_G_StartKamikaze) }
                .expect("this should not happen");
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { g_start_kamikaze_detour.enable() }.expect("this should not happen");
        vm_functions
            .entity
            .g_start_kamikaze_detour
            .store(Some(g_start_kamikaze_detour.into()));
        let client_spawn_detour =
            // SAFETY: the original and the replacement are functions with the same signature.
            unsafe { ClientSpawnDetourType::new(ClientSpawn, detoured_ClientSpawn) }
                .expect("this should not happen");
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { client_spawn_detour.enable() }.expect("this should not happen");
        vm_functions
            .game
            .client_spawn_detour
            .store(Some(client_spawn_detour.into()));
        // SAFETY: the original and the replacement are functions with the same signature.
        let g_damage_detour = unsafe { GDamageDetourType::new(G_Damage, detoured_G_Damage) }
            .expect("this should not happen");
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { g_damage_detour.enable() }.expect("this should not happen");
        vm_functions
            .entity
            .g_damage_detour
            .store(Some(g_damage_detour.into()));

        vm_functions.unhook();

        assert_unhooked(&vm_functions);
    }
}

#[cfg(test)]
mod quake_live_engine_tests {
    use core::sync::atomic::Ordering;

    use super::{mock_quake_functions::G_InitGame, test_helpers::default_quake_engine};
    use crate::{
        engine::patterns::QuakeLiveFunction,
        prelude::{QuakeLiveEngineError, serial},
    };

    #[test]
    fn unhook_vm_when_restarted() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .game
            .g_init_game_orig
            .store(G_InitGame as *const () as usize, Ordering::Release);

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
            .game
            .g_init_game_orig
            .store(G_InitGame as *const () as usize, Ordering::Release);

        quake_engine.unhook_vm(false);
        assert!(
            quake_engine.g_init_game_orig().is_err_and(|err| err
                == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_InitGame))
        );
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    #[cfg(target_os = "linux")]
    fn initialize_vm_without_game_module_reports_missing_function() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.initialize_vm(0);

        assert!(matches!(
            result,
            Err(QuakeLiveEngineError::VmFunctionNotFound(_))
        ));
    }
}

pub(crate) trait SetModuleOffset<T: AsRef<str>> {
    fn set_module_offset(&self, module_name: T, offset: unsafe extern "C" fn());
}

impl<T: AsRef<str>> SetModuleOffset<T> for QuakeLiveEngine {
    fn set_module_offset(&self, module_name: T, offset: unsafe extern "C" fn()) {
        let _ = self.sys_setmoduleoffset_detour().tap_ok(|detour| {
            let _ = CString::new(module_name.as_ref()).tap_ok(|c_module_name| {
                detour.call(c_module_name.as_ptr().cast_mut(), offset);
            });
        });
    }
}

#[cfg(test)]
mod set_module_offset_quake_live_engine_tests {
    use core::ffi::CStr;

    use super::{
        QuakeLiveEngine, SetModuleOffset, mock_quake_functions::Sys_SetModuleOffset_context,
        test_helpers::*,
    };
    use crate::prelude::serial;

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
                // SAFETY: the engine passes a valid, NUL-terminated C string.
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

/// The function of the server program with which it announces the game module it has loaded.
#[derive(Debug)]
pub(in crate::engine) struct ModuleLoaderFunctions {
    pub(in crate::engine) sys_setmoduleoffset_orig:
        extern "C" fn(*mut c_char, unsafe extern "C" fn()),
}

impl ModuleLoaderFunctions {
    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn locate(
        qzeroded_maps: &[&MemoryMap],
    ) -> Result<Self, QuakeLiveEngineError> {
        let sys_setmoduleoffset_orig = try_find_static_function::<
            extern "C" fn(*mut c_char, unsafe extern "C" fn()),
        >(
            qzeroded_maps, QuakeLiveFunction::Sys_SetModuleOffset
        )?;

        Ok(Self {
            sys_setmoduleoffset_orig,
        })
    }
}

#[derive(Debug)]
pub(in crate::engine) struct ModuleLoaderDetours {
    pub(in crate::engine) sys_setmoduleoffset_detour:
        TypedDetour<extern "C" fn(*mut c_char, unsafe extern "C" fn())>,
}

impl ModuleLoaderDetours {
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(in crate::engine) fn hook(engine: &QuakeLiveEngine) -> Result<Self, QuakeLiveEngineError> {
        let sys_setmoduleoffset_detour = QuakeLiveFunction::Sys_SetModuleOffset
            .create_and_enable_generic_detour(
                engine.sys_setmoduleoffset_orig()?,
                engine.callbacks().module_loader.sys_setmoduleoffset,
            )?;

        Ok(Self {
            sys_setmoduleoffset_detour,
        })
    }
}

impl QuakeLiveEngine {
    pub(in crate::engine) fn sys_setmoduleoffset_orig(
        &self,
    ) -> Result<extern "C" fn(*mut c_char, unsafe extern "C" fn()), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Sys_SetModuleOffset,
                ))
            },
            |static_functions| Ok(static_functions.module_loader.sys_setmoduleoffset_orig),
        )
    }

    pub(in crate::engine) fn sys_setmoduleoffset_detour(
        &self,
    ) -> Result<
        &TypedDetour<extern "C" fn(*mut c_char, unsafe extern "C" fn())>,
        QuakeLiveEngineError,
    > {
        self.static_detours.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::Sys_SetModuleOffset,
                ))
            },
            |static_detours| Ok(&static_detours.module_loader.sys_setmoduleoffset_detour),
        )
    }
}

#[cfg(test)]
mod table_accessor_tests {
    use core::{ffi::c_char, ptr};

    use crate::{
        engine::{
            QuakeLiveEngine, mock_quake_functions::*, patterns::QuakeLiveFunction, test_helpers::*,
        },
        prelude::QuakeLiveEngineError,
    };

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
}
