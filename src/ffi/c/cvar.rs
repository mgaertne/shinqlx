use super::prelude::*;
use crate::prelude::*;

use core::ffi::CStr;

#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct CVar {
    cvar: &'static mut cvar_t,
}

impl TryFrom<*mut cvar_t> for CVar {
    type Error = QuakeLiveEngineError;

    fn try_from(cvar: *mut cvar_t) -> Result<Self, Self::Error> {
        unsafe { cvar.as_mut() }.map(|cvar| Self { cvar }).ok_or(
            QuakeLiveEngineError::NullPointerPassed("null pointer passed".into()),
        )
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
mod cvar_tests {
    use crate::ffi::c::prelude::*;
    use crate::prelude::*;

    use alloc::ffi::CString;
    use core::ffi::c_char;
    use pretty_assertions::assert_eq;

    #[test]
    fn cvar_try_from_null_results_in_error() {
        assert_eq!(
            CVar::try_from(ptr::null_mut() as *mut cvar_t),
            Err(QuakeLiveEngineError::NullPointerPassed(
                "null pointer passed".into()
            ))
        );
    }

    #[test]
    fn cvar_try_from_valid_cvar() {
        let mut cvar = CVarBuilder::default()
            .build()
            .expect("this should not happen");
        assert_eq!(CVar::try_from(&mut cvar as *mut cvar_t).is_ok(), true);
    }

    #[test]
    fn cvar_try_get_string() {
        let cvar_string = CString::new("some cvar value").expect("this should not happen");
        let mut cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr() as *mut c_char)
            .build()
            .expect("this should not happen");
        let cvar_rust = CVar::try_from(&mut cvar as *mut cvar_t).expect("this should not happen");
        assert_eq!(cvar_rust.get_string(), "some cvar value");
    }

    #[test]
    fn cvar_try_get_integer() {
        let mut cvar = CVarBuilder::default()
            .integer(42)
            .build()
            .expect("this should not happen");
        let cvar_rust = CVar::try_from(&mut cvar as *mut cvar_t).expect("this should not happen");
        assert_eq!(cvar_rust.get_integer(), 42);
    }
}
