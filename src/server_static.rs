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
        let Some(main_engine) =
            MAIN_ENGINE.get() else
        {
            debug_println!("Main Engine not found");
            panic!("Main Engine not found");
        };
        let Ok(func_pointer) = main_engine.sv_shutdown_orig() else {
            debug_println!("necessary offset function not found");
            panic!("necessary offset function not found");
        };
        let svs_ptr_ptr = func_pointer as usize + 0xAC;
        let svs_ptr: u32 = unsafe { std::ptr::read(svs_ptr_ptr as *const u32) };
        Self::try_from(svs_ptr as *mut serverStatic_t).unwrap()
    }
}
