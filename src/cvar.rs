use crate::quake_live_engine::QuakeLiveEngineError;
use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
use crate::quake_types::cvar_t;
use std::ffi::CStr;

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct CVar {
    cvar: &'static cvar_t,
}

impl TryFrom<*const cvar_t> for CVar {
    type Error = QuakeLiveEngineError;

    fn try_from(cvar: *const cvar_t) -> Result<Self, Self::Error> {
        unsafe {
            cvar.as_ref()
                .map(|cvar| Self { cvar })
                .ok_or(NullPointerPassed("null pointer passed".into()))
        }
    }
}

impl CVar {
    pub(crate) fn get_string(&self) -> String {
        unsafe { CStr::from_ptr(self.cvar.string).to_string_lossy().into() }
    }

    pub(crate) fn get_integer(&self) -> i32 {
        self.cvar.integer
    }
}

#[cfg(test)]
pub(crate) mod cvar_tests {
    use crate::cvar::CVar;
    use crate::quake_live_engine::QuakeLiveEngineError::NullPointerPassed;
    use crate::quake_types::{cvar_t, CVarBuilder};
    use pretty_assertions::assert_eq;
    use std::ffi::{c_char, CString};

    #[test]
    pub(crate) fn cvar_try_from_null_results_in_error() {
        assert_eq!(
            CVar::try_from(std::ptr::null_mut() as *const cvar_t),
            Err(NullPointerPassed("null pointer passed".into()))
        );
    }

    #[test]
    pub(crate) fn cvar_try_from_valid_cvar() {
        let cvar = CVarBuilder::default().build().unwrap();
        assert_eq!(CVar::try_from(&cvar as *const cvar_t).is_ok(), true);
    }

    #[test]
    pub(crate) fn cvar_try_get_string() {
        let cvar_string = CString::new("some cvar value").unwrap();
        let cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .unwrap();
        let cvar_rust = CVar::try_from(&cvar as *const cvar_t).unwrap();
        assert_eq!(cvar_rust.get_string(), "some cvar value");
    }

    #[test]
    pub(crate) fn cvar_try_get_integer() {
        let cvar = CVarBuilder::default().integer(42).build().unwrap();
        let cvar_rust = CVar::try_from(&cvar as *const cvar_t).unwrap();
        assert_eq!(cvar_rust.get_integer(), 42);
    }
}
