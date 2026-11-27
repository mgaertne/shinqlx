use alloc::ffi::CString;
use core::{
    ffi::{CStr, c_char, c_int},
    hint::cold_path,
    ptr::NonNull,
};

use detour::TypedDetour;
#[cfg(target_os = "linux")]
use procfs::process::MemoryMap;

#[cfg(target_os = "linux")]
use crate::engine::symbols::try_find_static_function;
#[cfg(test)]
use crate::engine::{mock_quake_functions, test_helpers};
use crate::{
    engine::{QuakeLiveEngine, QuakeLiveEngineError, patterns::QuakeLiveFunction, prelude::*},
    prelude::*,
};

/// Handle to a cvar of the engine.
#[derive(Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct CVar {
    cvar: NonNull<cvar_t>,
}

impl TryFrom<*mut cvar_t> for CVar {
    type Error = QuakeLiveEngineError;

    fn try_from(cvar: *mut cvar_t) -> Result<Self, Self::Error> {
        NonNull::new(cvar)
            .map(|cvar| Self { cvar })
            .ok_or(QuakeLiveEngineError::NullPointerPassed)
    }
}

impl CVar {
    fn cvar(&self) -> &cvar_t {
        // SAFETY: the pointer was non-null on creation and points to a cvar of the engine.
        unsafe { self.cvar.as_ref() }
    }

    pub(crate) fn get_string(&self) -> String {
        if self.cvar().string.is_null() {
            return "".into();
        }
        // SAFETY: the field is a NUL-terminated C string owned by the engine structure.
        unsafe { CStr::from_ptr(self.cvar().string) }
            .to_string_lossy()
            .to_string()
    }

    #[cfg(any(all(target_os = "linux", feature = "fps-patch"), test))]
    pub(crate) fn as_ptr(&self) -> *mut cvar_t {
        self.cvar.as_ptr()
    }

    pub(crate) fn get_integer(&self) -> i32 {
        self.cvar().integer
    }
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
    use core::{borrow::BorrowMut, ffi::CStr, ptr};

    use super::{
        FindCVar, QuakeLiveEngine, mock_quake_functions::Cvar_FindVar_context, test_helpers::*,
    };
    use crate::{engine::prelude::CVarBuilder, prelude::serial};

    #[test]
    fn find_cvar_with_no_original_func() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.find_cvar("sv_maxclients");
        assert!(result.is_none());
    }

    #[test]
    #[serial]
    fn find_cvar_when_function_returns_valid_cvar() {
        let mut cvar = CVarBuilder::default()
            .build()
            .expect("this should not happen");

        let find_cvar_ctx = Cvar_FindVar_context();
        find_cvar_ctx
            .expect()
            .withf(|&cvar_name| {
                // SAFETY: the engine passes a valid, NUL-terminated C string.
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
    #[serial]
    fn find_cvar_when_function_returns_null_ptr() {
        let find_cvar_ctx = Cvar_FindVar_context();
        find_cvar_ctx
            .expect()
            .withf(|&cvar_name| {
                // SAFETY: the engine passes a valid, NUL-terminated C string.
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
    use core::{
        borrow::BorrowMut,
        ffi::{CStr, c_int},
    };

    use super::{
        GetCVar, QuakeLiveEngine, mock_quake_functions::Cvar_Get_context, test_helpers::*,
    };
    use crate::{
        engine::prelude::{CVarBuilder, cvar_flags::CVAR_CHEAT},
        prelude::serial,
    };

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
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
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
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
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

#[cfg(any(all(target_os = "linux", feature = "fps-patch"), test))]
pub(crate) trait GetCvarUndetected {
    /// Runs the original `Cvar_Get` of the engine, past the hook that changes defaults.
    fn get_cvar_undetected(
        &self,
        name: *const c_char,
        value: *const c_char,
        flags: c_int,
    ) -> *mut cvar_t;
}

#[cfg(any(all(target_os = "linux", feature = "fps-patch"), test))]
impl GetCvarUndetected for QuakeLiveEngine {
    fn get_cvar_undetected(
        &self,
        name: *const c_char,
        value: *const c_char,
        flags: c_int,
    ) -> *mut cvar_t {
        self.static_detours
            .get()
            .map_or(ptr::null_mut(), |static_detours| {
                static_detours.cvar.cvar_get_detour.call(name, value, flags)
            })
    }
}

pub(crate) trait SetCvarUndetected {
    /// Runs the original `Cvar_Set2` of the engine, past the hook that reports changed cvars.
    fn set_cvar_undetected(
        &self,
        name: *const c_char,
        value: *const c_char,
        force: qboolean,
    ) -> *mut cvar_t;
}

impl SetCvarUndetected for QuakeLiveEngine {
    fn set_cvar_undetected(
        &self,
        name: *const c_char,
        value: *const c_char,
        force: qboolean,
    ) -> *mut cvar_t {
        self.static_detours
            .get()
            .map_or(ptr::null_mut(), |static_detours| {
                static_detours
                    .cvar
                    .cvar_set2_detour
                    .call(name, value, force)
            })
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
    use core::{borrow::BorrowMut, ffi::CStr};

    use super::{
        QuakeLiveEngine, SetCVarForced, mock_quake_functions::Cvar_Set2_context, test_helpers::*,
    };
    use crate::{engine::prelude::CVarBuilder, prelude::serial};

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
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
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
    use core::{
        borrow::BorrowMut,
        ffi::{CStr, c_int},
    };

    use super::{
        QuakeLiveEngine, SetCVarLimit, mock_quake_functions::Cvar_GetLimit_context, test_helpers::*,
    };
    use crate::{
        engine::prelude::{CVarBuilder, cvar_flags::CVAR_CHEAT},
        prelude::serial,
    };

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
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(value) } == c"16"
                    && !min.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(min) } == c"2"
                    && !max.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
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
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cvar) } == c"sv_maxclients"
                    && !value.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(value) } == c"16"
                    && !min.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(min) } == c"2"
                    && !max.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
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

pub(in crate::engine) type CvarSet2DetourType =
    TypedDetour<extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t>;

#[cfg(any(all(target_os = "linux", feature = "fps-patch"), test))]
pub(in crate::engine) type CvarGetDetourType =
    TypedDetour<extern "C" fn(*const c_char, *const c_char, c_int) -> *mut cvar_t>;

#[derive(Debug)]
pub(in crate::engine) struct CvarFunctions {
    pub(in crate::engine) cvar_findvar_orig: extern "C" fn(*const c_char) -> *mut cvar_t,
    pub(in crate::engine) cvar_get_orig:
        extern "C" fn(*const c_char, *const c_char, c_int) -> *mut cvar_t,
    pub(in crate::engine) cvar_getlimit_orig: extern "C" fn(
        *const c_char,
        *const c_char,
        *const c_char,
        *const c_char,
        c_int,
    ) -> *mut cvar_t,
    pub(in crate::engine) cvar_set2_orig:
        extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t,
}

impl CvarFunctions {
    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn locate(
        qzeroded_maps: &[&MemoryMap],
    ) -> Result<Self, QuakeLiveEngineError> {
        let cvar_findvar_orig = try_find_static_function::<
            extern "C" fn(*const c_char) -> *mut cvar_t,
        >(qzeroded_maps, QuakeLiveFunction::Cvar_FindVar)?;

        let cvar_get_orig = try_find_static_function::<
            extern "C" fn(*const c_char, *const c_char, c_int) -> *mut cvar_t,
        >(qzeroded_maps, QuakeLiveFunction::Cvar_Get)?;

        let cvar_getlimit_orig = try_find_static_function::<
            extern "C" fn(
                *const c_char,
                *const c_char,
                *const c_char,
                *const c_char,
                c_int,
            ) -> *mut cvar_t,
        >(qzeroded_maps, QuakeLiveFunction::Cvar_GetLimit)?;

        let cvar_set2_orig = try_find_static_function::<
            extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t,
        >(qzeroded_maps, QuakeLiveFunction::Cvar_Set2)?;

        Ok(Self {
            cvar_findvar_orig,
            cvar_get_orig,
            cvar_getlimit_orig,
            cvar_set2_orig,
        })
    }
}

#[derive(Debug)]
pub(in crate::engine) struct CvarDetours {
    pub(in crate::engine) cvar_set2_detour: CvarSet2DetourType,
    #[cfg(any(all(target_os = "linux", feature = "fps-patch"), test))]
    pub(in crate::engine) cvar_get_detour: CvarGetDetourType,
}

impl CvarDetours {
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(in crate::engine) fn hook(engine: &QuakeLiveEngine) -> Result<Self, QuakeLiveEngineError> {
        let cvar_set2_detour = QuakeLiveFunction::Cvar_Set2.create_and_enable_generic_detour(
            engine.cvar_set2_orig()?,
            engine.callbacks().cvar.cvar_set2,
        )?;

        #[cfg(any(all(target_os = "linux", feature = "fps-patch"), test))]
        let cvar_get_detour = QuakeLiveFunction::Cvar_Get.create_and_enable_generic_detour(
            engine.cvar_get_orig()?,
            engine.callbacks().cvar.cvar_get,
        )?;

        Ok(Self {
            cvar_set2_detour,
            #[cfg(any(all(target_os = "linux", feature = "fps-patch"), test))]
            cvar_get_detour,
        })
    }
}

impl QuakeLiveEngine {
    pub(in crate::engine) fn cvar_findvar_orig(
        &self,
    ) -> Result<extern "C" fn(*const c_char) -> *mut cvar_t, QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_FindVar,
                ))
            },
            |static_functions| Ok(static_functions.cvar.cvar_findvar_orig),
        )
    }

    pub(in crate::engine) fn cvar_get_orig(
        &self,
    ) -> Result<
        extern "C" fn(*const c_char, *const c_char, c_int) -> *mut cvar_t,
        QuakeLiveEngineError,
    > {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_Get,
                ))
            },
            |static_functions| Ok(static_functions.cvar.cvar_get_orig),
        )
    }

    pub(in crate::engine) fn cvar_getlimit_orig(
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
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_GetLimit,
                ))
            },
            |static_functions| Ok(static_functions.cvar.cvar_getlimit_orig),
        )
    }

    pub(in crate::engine) fn cvar_set2_orig(
        &self,
    ) -> Result<
        extern "C" fn(*const c_char, *const c_char, qboolean) -> *mut cvar_t,
        QuakeLiveEngineError,
    > {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cvar_Set2,
                ))
            },
            |static_functions| Ok(static_functions.cvar.cvar_set2_orig),
        )
    }
}

#[cfg(test)]
mod table_accessor_tests {
    use core::{
        ffi::{c_char, c_int},
        ptr,
    };

    use crate::{
        engine::{
            QuakeLiveEngine, mock_quake_functions::*, patterns::QuakeLiveFunction, prelude::*,
            test_helpers::*,
        },
        prelude::QuakeLiveEngineError,
    };

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
}

#[cfg(test)]
mod cvar_tests {
    use core::borrow::BorrowMut;

    use pretty_assertions::assert_eq;

    use crate::{engine::prelude::*, prelude::*};

    #[test]
    fn cvar_try_from_null_results_in_error() {
        assert_eq!(
            CVar::try_from(ptr::null_mut() as *mut cvar_t),
            Err(QuakeLiveEngineError::NullPointerPassed)
        );
    }

    #[test]
    fn cvar_try_from_valid_cvar() {
        let mut cvar = CVarBuilder::default()
            .build()
            .expect("this should not happen");
        assert_eq!(
            CVar::try_from(cvar.borrow_mut() as *mut cvar_t).is_ok(),
            true
        );
    }

    #[test]
    fn cvar_try_get_string() {
        let cvar_string = c"some cvar value";
        let mut cvar = CVarBuilder::default()
            .string(cvar_string.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");
        let cvar_rust =
            CVar::try_from(cvar.borrow_mut() as *mut cvar_t).expect("this should not happen");
        assert_eq!(cvar_rust.get_string(), "some cvar value");
    }

    #[test]
    fn cvar_try_get_integer() {
        let mut cvar = CVarBuilder::default()
            .integer(42)
            .build()
            .expect("this should not happen");
        let cvar_rust =
            CVar::try_from(cvar.borrow_mut() as *mut cvar_t).expect("this should not happen");
        assert_eq!(cvar_rust.get_integer(), 42);
    }
}
