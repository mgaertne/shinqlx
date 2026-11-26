use core::{
    hint::cold_path,
    sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
};
use std::sync::OnceLock;

use tap::TapOptional;

#[cfg(test)]
use crate::engine::{mock_quake_functions, test_helpers};
use crate::{
    engine::{QuakeLiveEngineError, callbacks::EngineCallbacks, *},
    prelude::*,
};

pub(crate) struct QuakeLiveEngine {
    pub(super) callbacks: &'static EngineCallbacks,
    pub(super) static_functions: OnceLock<StaticFunctions>,
    pub(super) static_detours: OnceLock<StaticDetours>,

    pub(crate) sv_maxclients: AtomicI32,
    pub(super) common_initialized: AtomicBool,

    pub(super) vm_functions: VmFunctions,
    pub(super) current_vm: AtomicUsize,
}

impl QuakeLiveEngine {
    /// An engine that is neither searched nor hooked yet.
    pub(crate) fn with_callbacks(callbacks: &'static EngineCallbacks) -> Self {
        Self {
            callbacks,
            static_functions: OnceLock::new(),
            static_detours: OnceLock::new(),
            sv_maxclients: AtomicI32::default(),
            common_initialized: AtomicBool::default(),
            vm_functions: VmFunctions::default(),
            current_vm: AtomicUsize::default(),
        }
    }

    /// What the engine calls back into.
    pub(crate) fn callbacks(&self) -> &'static EngineCallbacks {
        self.callbacks
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
            .tap_some(|new_tags| {
                self.set_cvar_forced("sv_tags", new_tags, false);
            });
    }

    // sv_maxclients is only known once the game is initialized, so it is read here and not at startup.
    pub(crate) fn initialize_cvars(&self) {
        self.find_cvar("sv_maxclients").tap_some(|maxclients| {
            self.sv_maxclients
                .store(maxclients.get_integer(), Ordering::Release);
        });
    }

    pub(crate) fn get_max_clients(&self) -> i32 {
        self.sv_maxclients.load(Ordering::Acquire)
    }

    // Currently called by My_Cmd_AddCommand(), since it's called at a point where we
    // can safely do whatever we do below. It'll segfault if we do it at the entry
    // point, since functions like Cmd_AddCommand need initialization first.
    pub(crate) fn initialize_static(&self) -> Result<(), QuakeLiveEngineError> {
        if self.common_initialized.load(Ordering::Acquire) {
            cold_path();
            return Err(QuakeLiveEngineError::MainEngineNotInitialized);
        }
        debug!(target: "shinqlx", func = "initialize_static"; "Initializing...");
        self.callbacks
            .console_commands
            .iter()
            .for_each(|&(name, command)| self.add_command(name, command));

        (self.callbacks.initialize_scripting)().map_err(|err| {
            cold_path();
            error!(target: "shinqlx", func = "initialize_static"; "Python initialization failed.");
            QuakeLiveEngineError::PythonInitializationFailed(err)
        })?;

        self.common_initialized.store(true, Ordering::Release);
        Ok(())
    }

    pub(crate) fn is_common_initialized(&self) -> bool {
        self.common_initialized.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod quake_live_engine_tests {
    use core::{borrow::BorrowMut, ffi::CStr, ptr, sync::atomic::Ordering};

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use tap::Conv;

    use super::{
        QuakeLiveEngine,
        mock_quake_functions::{Cmd_AddCommand_context, Cvar_FindVar_context, Cvar_Set2_context},
        test_helpers::{default_quake_engine, default_static_detours, default_static_functions},
    };
    use crate::{
        commands::{
            cmd_center_print, cmd_plugin_commands, cmd_py_command, cmd_py_rcon, cmd_regular_print,
            cmd_reliable_guard, cmd_restart_python, cmd_scoreboard_trim, cmd_send_server_command,
            cmd_slap, cmd_slay,
        },
        engine::prelude::CVarBuilder,
        prelude::{QuakeLiveEngineError, serial},
        python::{PythonInitializationError, prelude::pyshinqlx_initialize_context},
    };

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
                // SAFETY: the engine passes a valid, NUL-terminated C string.
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
                // SAFETY: the engine passes a valid, NUL-terminated C string.
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"sv_tags"
            })
            .returning_st(move |_| returned1.borrow_mut())
            .times(1);

        let cvar_set2_ctx = Cvar_Set2_context();
        cvar_set2_ctx
            .expect()
            .withf(|&cvar_name, &cvar_value, &forced| {
                !cvar_name.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cvar_name) } == c"sv_tags"
                    && !cvar_value.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cvar_value) } == c"shinqlx,ca,elo"
                    && !forced.conv::<bool>()
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
                // SAFETY: the engine passes a valid, NUL-terminated C string.
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"sv_tags"
            })
            .returning_st(move |_| returned1.borrow_mut())
            .times(1);

        let cvar_set2_ctx = Cvar_Set2_context();
        cvar_set2_ctx
            .expect()
            .withf(|&cvar_name, &cvar_value, &forced| {
                !cvar_name.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cvar_name) } == c"sv_tags"
                    && !cvar_value.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cvar_value) } == c"shinqlx"
                    && !forced.conv::<bool>()
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
                // SAFETY: the engine passes a valid, NUL-terminated C string.
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
                // SAFETY: the engine passes a valid, NUL-terminated C string.
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
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"cmd"
                    && ptr::fn_addr_eq(func, cmd_send_server_command as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"cp"
                    && ptr::fn_addr_eq(func, cmd_center_print as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"print"
                    && ptr::fn_addr_eq(func, cmd_regular_print as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"slap"
                    && ptr::fn_addr_eq(func, cmd_slap as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"slay"
                    && ptr::fn_addr_eq(func, cmd_slay as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"qlx"
                    && ptr::fn_addr_eq(func, cmd_py_rcon as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"pycmd"
                    && ptr::fn_addr_eq(func, cmd_py_command as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"pyrestart"
                    && ptr::fn_addr_eq(func, cmd_restart_python as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"reliable"
                    && ptr::fn_addr_eq(func, cmd_reliable_guard as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"scoreboard"
                    && ptr::fn_addr_eq(func, cmd_scoreboard_trim as extern "C" fn())
            })
            .times(1);
        add_cmd_ctx
            .expect()
            .withf(|&cmd, &func| {
                !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"plugincmds"
                    && ptr::fn_addr_eq(func, cmd_plugin_commands as extern "C" fn())
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
        pyshinqlx_init_ctx.expect().returning(|| Ok(())).times(0);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .common_initialized
            .store(true, Ordering::Release);

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
            .store(false, Ordering::Release);

        assert!(!quake_engine.is_common_initialized());
    }

    #[test]
    fn is_common_initialized_when_initialized_is_set_to_true() {
        let quake_engine = default_quake_engine();
        quake_engine
            .common_initialized
            .store(true, Ordering::Release);

        assert!(quake_engine.is_common_initialized());
    }
}
