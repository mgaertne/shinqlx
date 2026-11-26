use core::{
    ffi::{c_char, c_int},
    hint::cold_path,
    sync::atomic::{AtomicUsize, Ordering},
};

use arc_swap::ArcSwapOption;
use detour::TypedDetour;
use tap::{Conv, TapFallible, TapOptional};

#[cfg(test)]
use crate::engine::{mock_quake_functions, test_helpers};
#[cfg(feature = "clientkick-fix")]
use crate::fixes::clientkick_fix::patch_callvote_f;
use crate::{
    engine::{
        QuakeLiveEngine, QuakeLiveEngineError, callbacks::GameCallbacks,
        patterns::QuakeLiveFunction, prelude::*, symbols::function_at,
    },
    prelude::*,
};

pub(crate) trait InitGame<T: Into<c_int>, U: Into<c_int>, V: Into<c_int>> {
    fn init_game(&self, level_time: T, random_seed: U, restart: V);
}

impl<T: Into<c_int>, U: Into<c_int>, V: Into<c_int>> InitGame<T, U, V> for QuakeLiveEngine {
    fn init_game(&self, level_time: T, random_seed: U, restart: V) {
        let level_time_param = level_time.into();
        let random_seed_param = random_seed.into();
        let restart_param = restart.into();
        let _ = self.g_init_game_orig().tap_ok(|original_func| {
            original_func(level_time_param, random_seed_param, restart_param);
        });
    }
}

#[cfg(test)]
mod init_game_quake_live_engine_tests {
    use core::sync::atomic::Ordering;

    use mockall::predicate;

    use super::{
        InitGame, QuakeLiveEngine,
        mock_quake_functions::{G_InitGame, G_InitGame_context},
        test_helpers::*,
    };
    use crate::prelude::serial;

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
            .game
            .g_init_game_orig
            .store(G_InitGame as *const () as usize, Ordering::Release);

        quake_engine.init_game(42, 21, 1);
    }
}

pub(crate) trait ShutdownGame<T: Into<c_int>> {
    fn shutdown_game(&self, restart: T);
}

impl<T: Into<c_int>> ShutdownGame<T> for QuakeLiveEngine {
    fn shutdown_game(&self, restart: T) {
        let restart_param = restart.into();
        let _ = self.g_shutdown_game_orig().tap_ok(|original_func| {
            original_func(restart_param);
        });
    }
}

#[cfg(test)]
mod shutdown_game_quake_live_engine_tests {
    use core::sync::atomic::Ordering;

    use mockall::predicate;

    use super::{
        QuakeLiveEngine, ShutdownGame,
        mock_quake_functions::{G_ShutdownGame, G_ShutdownGame_context},
        test_helpers::*,
    };
    use crate::prelude::serial;

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
            .game
            .g_shutdown_game_orig
            .store(G_ShutdownGame as *const () as usize, Ordering::Release);

        quake_engine.shutdown_game(1);
    }
}

pub(crate) trait RunFrame<T: Into<c_int>> {
    fn run_frame(&self, time: T);
}

impl<T: Into<c_int>> RunFrame<T> for QuakeLiveEngine {
    fn run_frame(&self, time: T) {
        let time_param = time.conv::<c_int>();
        let _ = self.g_run_frame_orig().tap_ok(|original_func| {
            original_func(time_param);
        });
    }
}

#[cfg(test)]
mod run_frame_quake_live_engine_tests {
    use core::sync::atomic::Ordering;

    use super::{
        QuakeLiveEngine, RunFrame,
        mock_quake_functions::{G_RunFrame, G_RunFrame_context},
        test_helpers::*,
    };
    use crate::prelude::serial;

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
            .game
            .g_run_frame_orig
            .store(G_RunFrame as *const () as usize, Ordering::Release);

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
            .game
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
    use core::ffi::{CStr, c_char, c_int};

    use detour::TypedDetour;
    use pretty_assertions::assert_eq;
    use tap::Conv;

    use super::{
        ClientConnect, QuakeLiveEngine,
        mock_quake_functions::{ClientConnect, ClientConnect_context, detoured_ClientConnect},
        test_helpers::*,
    };
    use crate::{engine::prelude::qboolean, prelude::serial};

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
                client_num == 42 && first_time.into() && !is_bot.conv::<bool>()
            })
            .returning(move |_, _, _| returned.as_ptr().cast_mut())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };
        quake_engine
            .vm_functions
            .game
            .client_connect_detour
            .store(Some(
                // SAFETY: the mock original and the replacement have the same signature.
                unsafe {
                    TypedDetour::new(
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
            // SAFETY: the mocked engine call passes a valid, NUL-terminated C string.
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
            .game
            .client_spawn_detour
            .load()
            .as_ref()
            .tap_some(|detour| {
                detour.call(ent.as_mut());
            });
    }
}

#[cfg(test)]
mod client_spawn_quake_live_engine_tests {
    use detour::TypedDetour;

    use super::{
        ClientSpawn, QuakeLiveEngine,
        mock_quake_functions::{ClientSpawn, ClientSpawn_context, detoured_ClientSpawn},
        test_helpers::*,
    };
    use crate::{
        engine::prelude::{GEntityBuilder, MockGameEntity, gentity_t},
        prelude::serial,
    };

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
        quake_engine
            .vm_functions
            .game
            .client_spawn_detour
            .store(Some(
                // SAFETY: the mock original and the replacement have the same signature.
                unsafe {
                    TypedDetour::new(
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

/// The functions and hooks of the game module that run the game and its clients.
#[derive(Default)]
pub(in crate::engine) struct GameFunctions {
    pub(in crate::engine) check_privileges_orig: AtomicUsize,
    pub(in crate::engine) client_connect_orig: AtomicUsize,
    pub(in crate::engine) client_spawn_orig: AtomicUsize,
    pub(in crate::engine) g_init_game_orig: AtomicUsize,
    pub(in crate::engine) g_shutdown_game_orig: AtomicUsize,
    pub(in crate::engine) g_run_frame_orig: AtomicUsize,
    #[cfg(feature = "clientkick-fix")]
    pub(in crate::engine) cmd_callvote_f_orig: AtomicUsize,

    pub(in crate::engine) client_spawn_detour: ArcSwapOption<ClientSpawnDetourType>,
    pub(in crate::engine) client_connect_detour: ArcSwapOption<ClientConnectDetourType>,
}

impl GameFunctions {
    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn symbols(&self) -> Vec<(QuakeLiveFunction, &AtomicUsize)> {
        vec![
            (
                QuakeLiveFunction::CheckPrivileges,
                &self.check_privileges_orig,
            ),
            (QuakeLiveFunction::ClientConnect, &self.client_connect_orig),
            (QuakeLiveFunction::ClientSpawn, &self.client_spawn_orig),
            #[cfg(feature = "clientkick-fix")]
            (QuakeLiveFunction::Cmd_Callvote_f, &self.cmd_callvote_f_orig),
        ]
    }

    pub(in crate::engine) fn hook(
        &self,
        callbacks: &GameCallbacks,
    ) -> Result<(), QuakeLiveEngineError> {
        let client_connect_orig = self.client_connect_orig.load(Ordering::Acquire);
        // SAFETY: the address was found in the engine and has the signature of the function type.
        let client_connect_func = unsafe {
            function_at::<extern "C" fn(c_int, qboolean, qboolean) -> *const c_char>(
                client_connect_orig,
            )
        };
        let client_connect_detour =
            // SAFETY: the original and the replacement are functions with the same signature.
            unsafe { ClientConnectDetourType::new(client_connect_func, callbacks.client_connect) }
                .map_err(|_| {
                    cold_path();
                    QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::ClientConnect)
                })?;
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { client_connect_detour.enable() }.map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::ClientConnect)
        })?;

        self.client_connect_detour
            .swap(Some(client_connect_detour.into()))
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    cold_path();
                    error!(target: "shinqlx", func = "hook"; "error when disabling client_conect detour: {e}");
                });
            });

        let client_spawn_orig = self.client_spawn_orig.load(Ordering::Acquire);
        let client_spawn_func =
            // SAFETY: the address was found in the engine and has the signature of the function type.
            unsafe { function_at::<extern "C" fn(*mut gentity_s)>(client_spawn_orig) };
        let client_spawn_detour =
            // SAFETY: the original and the replacement are functions with the same signature.
            unsafe { ClientSpawnDetourType::new(client_spawn_func, callbacks.client_spawn) }.map_err(
                |_| {
                    cold_path();
                    QuakeLiveEngineError::DetourCouldNotBeCreated(QuakeLiveFunction::ClientSpawn)
                },
            )?;
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe { client_spawn_detour.enable() }.map_err(|_| {
            cold_path();
            QuakeLiveEngineError::DetourCouldNotBeEnabled(QuakeLiveFunction::ClientSpawn)
        })?;

        self.client_spawn_detour
            .swap(Some(client_spawn_detour.into()))
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    cold_path();
                    error!(target: "shinqlx", func = "hook"; "error when disabling client_spawn detour: {e}");
                });
            });

        Ok(())
    }

    pub(in crate::engine) fn unhook(&self) {
        [
            &self.check_privileges_orig,
            &self.client_connect_orig,
            &self.client_spawn_orig,
            &self.g_init_game_orig,
            &self.g_run_frame_orig,
            #[cfg(feature = "clientkick-fix")]
            &self.cmd_callvote_f_orig,
        ]
        .iter()
        .for_each(|field| {
            field.store(0, Ordering::Release);
        });

        self.client_connect_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    error!(target: "shinqlx", func = "unhook"; "error when disabling client_connect detour: {e}");
                });
            });

        self.client_spawn_detour
            .swap(None)
            .filter(|detour| detour.is_enabled())
            .tap_some(|detour| {
                // SAFETY: the detour was created from a function address of the engine and is only disabled while it is enabled.
                let _ = unsafe { detour.disable() }.tap_err(|e| {
                    error!(target: "shinqlx", func = "unhook"; "error when disabling client_spawn detour: {e}");
                });
            });
    }

    #[cfg(feature = "clientkick-fix")]
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(crate) fn patch(&self) {
        let cmd_callvote_f_orig = self.cmd_callvote_f_orig.load(Ordering::Acquire);
        if cmd_callvote_f_orig == 0 {
            return;
        }

        let _ = patch_callvote_f(cmd_callvote_f_orig)
            .tap_ok(|outcome| {
                debug!(target: "shinqlx", func = "patch"; "callvote command patch at {cmd_callvote_f_orig:#x}: {outcome:?}");
            })
            .tap_err(|e| {
                error!(target: "shinqlx", func = "patch"; "could not patch the callvote command at {cmd_callvote_f_orig:#x}: {e}");
            });
    }
}

impl QuakeLiveEngine {
    pub(crate) fn g_init_game_orig(
        &self,
    ) -> Result<extern "C" fn(c_int, c_int, c_int), QuakeLiveEngineError> {
        match self
            .vm_functions
            .game
            .g_init_game_orig
            .load(Ordering::Acquire)
        {
            0 => {
                cold_path();
                Err(QuakeLiveEngineError::VmFunctionNotFound(
                    QuakeLiveFunction::G_InitGame,
                ))
            }
            g_init_game_orig => {
                let g_init_game_func =
                    // SAFETY: the address was found in the engine and has the signature of the function type.
                    unsafe { function_at::<extern "C" fn(c_int, c_int, c_int)>(g_init_game_orig) };
                Ok(g_init_game_func)
            }
        }
    }

    pub(in crate::engine) fn g_shutdown_game_orig(
        &self,
    ) -> Result<extern "C" fn(c_int), QuakeLiveEngineError> {
        match self
            .vm_functions
            .game
            .g_shutdown_game_orig
            .load(Ordering::Acquire)
        {
            0 => {
                cold_path();
                Err(QuakeLiveEngineError::VmFunctionNotFound(
                    QuakeLiveFunction::G_ShutdownGame,
                ))
            }
            g_shutdown_game_orig => {
                let g_shutdown_game_func =
                    // SAFETY: the address was found in the engine and has the signature of the function type.
                    unsafe { function_at::<extern "C" fn(c_int)>(g_shutdown_game_orig) };
                Ok(g_shutdown_game_func)
            }
        }
    }

    pub(crate) fn g_run_frame_orig(&self) -> Result<extern "C" fn(c_int), QuakeLiveEngineError> {
        match self
            .vm_functions
            .game
            .g_run_frame_orig
            .load(Ordering::Acquire)
        {
            0 => {
                cold_path();
                Err(QuakeLiveEngineError::VmFunctionNotFound(
                    QuakeLiveFunction::G_RunFrame,
                ))
            }
            g_run_frame_orig => {
                let g_run_frame_func =
                    // SAFETY: the address was found in the engine and has the signature of the function type.
                    unsafe { function_at::<extern "C" fn(c_int)>(g_run_frame_orig) };
                Ok(g_run_frame_func)
            }
        }
    }
}

#[cfg(test)]
mod vm_accessor_tests {
    use core::sync::atomic::Ordering;

    use crate::{
        engine::{mock_quake_functions::*, patterns::QuakeLiveFunction, test_helpers::*},
        prelude::QuakeLiveEngineError,
    };

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
    fn g_init_game_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .game
            .g_init_game_orig
            .store(G_InitGame as *const () as usize, Ordering::Release);

        let stored_address = quake_engine
            .vm_functions
            .game
            .g_init_game_orig
            .load(Ordering::Acquire);
        let result = quake_engine.g_init_game_orig();
        assert!(result.is_ok_and(|func| func as *const () as usize == stored_address));
    }

    #[test]
    fn g_shutdown_game_orig_when_no_function_pointer_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.g_shutdown_game_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::VmFunctionNotFound(QuakeLiveFunction::G_ShutdownGame,)));
    }

    #[test]
    fn g_shutdown_game_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .game
            .g_shutdown_game_orig
            .store(G_ShutdownGame as *const () as usize, Ordering::Release);

        let stored_address = quake_engine
            .vm_functions
            .game
            .g_shutdown_game_orig
            .load(Ordering::Acquire);
        let result = quake_engine.g_shutdown_game_orig();
        assert!(result.is_ok_and(|func| func as *const () as usize == stored_address));
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
    fn g_run_frame_orig_when_function_pointer_set() {
        let quake_engine = default_quake_engine();
        quake_engine
            .vm_functions
            .game
            .g_run_frame_orig
            .store(G_RunFrame as *const () as usize, Ordering::Release);

        let stored_address = quake_engine
            .vm_functions
            .game
            .g_run_frame_orig
            .load(Ordering::Acquire);
        let result = quake_engine.g_run_frame_orig();
        assert!(result.is_ok_and(|func| func as *const () as usize == stored_address));
    }
}

pub(in crate::engine) type ClientSpawnDetourType = TypedDetour<extern "C" fn(*mut gentity_t)>;
pub(in crate::engine) type ClientConnectDetourType =
    TypedDetour<extern "C" fn(c_int, qboolean, qboolean) -> *const c_char>;
