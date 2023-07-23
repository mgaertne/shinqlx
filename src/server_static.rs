use crate::quake_live_engine::QuakeLiveEngineError;
use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
use crate::quake_types::serverStatic_t;
use crate::MAIN_ENGINE;

#[derive(Debug, PartialEq)]
#[allow(non_snake_case)]
#[repr(transparent)]
pub(crate) struct ServerStatic {
    pub(crate) serverStatic_t: &'static mut serverStatic_t,
}

impl TryFrom<*mut serverStatic_t> for ServerStatic {
    type Error = QuakeLiveEngineError;

    fn try_from(server_static: *mut serverStatic_t) -> Result<Self, Self::Error> {
        unsafe { server_static.as_mut() }
            .map(|svs| Self {
                serverStatic_t: svs,
            })
            .ok_or(NullPointerPassed("null pointer passed".into()))
    }
}

impl Default for ServerStatic {
    fn default() -> Self {
        let Ok(main_engine_guard) = MAIN_ENGINE.try_read() else {
            debug_println!("main quake live engine not accessible.");
            panic!("main quake live engine not accessible.");
        };

        let Some(ref main_engine) = *main_engine_guard else {
            debug_println!("main quake live engine not initialized.");
            panic!("main quake live engine not initialized.");
        };

        let Ok(func_pointer) = main_engine.sv_shutdown_orig() else {
            debug_println!("SV_Shutdown function not initialized.");
            panic!("SV_Shutdown function not initialized.");
        };

        let svs_ptr_ptr = func_pointer as usize + 0xAC;
        let svs_ptr: u32 = unsafe { std::ptr::read(svs_ptr_ptr as *const u32) };
        Self::try_from(svs_ptr as *mut serverStatic_t).unwrap()
    }
}

#[cfg(test)]
pub(crate) mod server_static_tests {
    #[cfg(not(miri))]
    use crate::quake_live_engine::QuakeLiveEngine;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::{serverStatic_t, ServerStaticBuilder};
    use crate::server_static::ServerStatic;
    #[cfg(not(miri))]
    use crate::MAIN_ENGINE;
    use pretty_assertions::assert_eq;
    #[cfg(not(miri))]
    use test_context::{test_context, TestContext};

    #[cfg(not(miri))]
    struct QuakeLiveEngineContext;

    #[cfg(not(miri))]
    impl TestContext for QuakeLiveEngineContext {
        fn setup() -> Self {
            let main_engine = QuakeLiveEngine::new();

            let Ok(mut guard) = MAIN_ENGINE.write() else {
                panic!("could not write MAIN_ENGINE");
            };
            *guard = Some(main_engine);

            Self {}
        }

        fn teardown(self) {
            let Ok(mut guard) = MAIN_ENGINE.write() else {
                panic!("could not write MAIN_ENGINE");
            };
            *guard = None;
        }
    }

    #[cfg(not(miri))]
    struct NoQuakeLiveEngineContext;

    #[cfg(not(miri))]
    impl TestContext for NoQuakeLiveEngineContext {
        fn setup() -> Self {
            let Ok(mut guard) = MAIN_ENGINE.write() else {
                panic!("could not write MAIN_ENGINE");
            };
            *guard = None;

            Self {}
        }
    }

    #[test]
    pub(crate) fn server_static_try_from_null_results_in_error() {
        assert_eq!(
            ServerStatic::try_from(std::ptr::null_mut()),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }

    #[test]
    pub(crate) fn server_static_try_from_valid_server_static() {
        let mut server_static = ServerStaticBuilder::default().build().unwrap();
        assert_eq!(
            ServerStatic::try_from(&mut server_static as *mut serverStatic_t).is_ok(),
            true
        );
    }

    #[cfg(not(miri))]
    #[test_context(NoQuakeLiveEngineContext)]
    #[test]
    #[should_panic(expected = "main quake live engine not initialized.")]
    pub(crate) fn server_static_default_panics_when_no_main_engine_found(
        _ctx: &mut NoQuakeLiveEngineContext,
    ) {
        ServerStatic::default();
    }

    #[cfg(not(miri))]
    #[test_context(QuakeLiveEngineContext)]
    #[test]
    #[should_panic(expected = "SV_Shutdown function not initialized")]
    pub(crate) fn server_static_default_panics_when_offset_function_not_initialized(
        _ctx: &mut QuakeLiveEngineContext,
    ) {
        ServerStatic::default();
    }
}
