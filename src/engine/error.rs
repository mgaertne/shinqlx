use core::{fmt::Debug, hint::cold_path};

use crate::{engine::patterns::QuakeLiveFunction, prelude::error};

/// Why the scripting layer could not be started or reloaded.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum PythonInitializationError {
    MainScriptError,
    AlreadyInitialized,
    NotInitializedError,
}

#[derive(Debug, PartialEq, Eq)]
pub enum QuakeLiveEngineError {
    NullPointerPassed,
    EntityNotFound,
    EntitiesNotInitialized,
    InvalidId(i32),
    ClientNotFound,
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

impl QuakeLiveEngineError {
    /// Logs why a detour could not be created and turns the cause into the error for `function`.
    pub(crate) fn detour_not_created<E: Debug>(
        function: QuakeLiveFunction,
    ) -> impl FnOnce(E) -> Self {
        move |cause| {
            cold_path();
            error!(target: "shinqlx", func = "hook"; "creating the detour of {function:?} failed: {cause:?}");
            Self::DetourCouldNotBeCreated(function)
        }
    }

    /// Logs why a detour could not be enabled and turns the cause into the error for `function`.
    pub(crate) fn detour_not_enabled<E: Debug>(
        function: QuakeLiveFunction,
    ) -> impl FnOnce(E) -> Self {
        move |cause| {
            cold_path();
            error!(target: "shinqlx", func = "hook"; "enabling the detour of {function:?} failed: {cause:?}");
            Self::DetourCouldNotBeEnabled(function)
        }
    }
}

#[cfg(test)]
mod error_tests {
    use pretty_assertions::assert_eq;

    use super::QuakeLiveEngineError;
    use crate::engine::patterns::QuakeLiveFunction;

    #[test]
    fn a_detour_that_cannot_be_created_names_the_function() {
        let result: Result<(), _> = Err("not executable").map_err(
            QuakeLiveEngineError::detour_not_created(QuakeLiveFunction::Com_Printf),
        );

        assert_eq!(
            result,
            Err(QuakeLiveEngineError::DetourCouldNotBeCreated(
                QuakeLiveFunction::Com_Printf
            ))
        );
    }

    #[test]
    fn a_detour_that_cannot_be_enabled_names_the_function() {
        let result: Result<(), _> = Err("already patched").map_err(
            QuakeLiveEngineError::detour_not_enabled(QuakeLiveFunction::Touch_Item),
        );

        assert_eq!(
            result,
            Err(QuakeLiveEngineError::DetourCouldNotBeEnabled(
                QuakeLiveFunction::Touch_Item
            ))
        );
    }
}
