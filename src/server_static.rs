use crate::prelude::*;
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
            .ok_or(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into(),
            ))
    }
}

impl ServerStatic {
    pub(crate) fn try_get() -> Result<Self, QuakeLiveEngineError> {
        let Some(ref main_engine) = *MAIN_ENGINE.load() else {
            return Err(QuakeLiveEngineError::MainEngineNotInitialized);
        };

        let func_pointer = main_engine.sv_shutdown_orig()?;

        let svs_ptr_ptr = func_pointer as usize + 0xAC;
        let svs_ptr: u32 = unsafe { ptr::read(svs_ptr_ptr as *const u32) };
        Self::try_from(svs_ptr as *mut serverStatic_t)
    }
}

#[cfg(test)]
mod server_static_tests {
    use crate::prelude::*;
    use crate::quake_live_functions::QuakeLiveFunction::SV_Shutdown;
    use crate::server_static::ServerStatic;
    use crate::MAIN_ENGINE;
    use pretty_assertions::assert_eq;

    #[test]
    fn server_static_try_from_null_results_in_error() {
        assert_eq!(
            ServerStatic::try_from(ptr::null_mut()),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into()
            ))
        );
    }

    #[test]
    fn server_static_try_from_valid_server_static() {
        let mut server_static = ServerStaticBuilder::default().build().unwrap();
        assert_eq!(
            ServerStatic::try_from(&mut server_static as *mut serverStatic_t).is_ok(),
            true
        );
    }

    #[test]
    #[serial]
    fn server_static_default_panics_when_no_main_engine_found() {
        {
            MAIN_ENGINE.store(None);
        }

        let result = ServerStatic::try_get();

        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap(),
            QuakeLiveEngineError::MainEngineNotInitialized
        );
    }

    #[test]
    #[serial]
    fn server_static_default_panics_when_offset_function_not_initialized() {
        {
            MAIN_ENGINE.store(Some(QuakeLiveEngine::new().into()));
        }

        let result = ServerStatic::try_get();

        {
            MAIN_ENGINE.store(None);
        }

        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap(),
            QuakeLiveEngineError::StaticFunctionNotFound(SV_Shutdown)
        );
    }
}
