use crate::engine::patterns::QuakeLiveFunction;

/// Why the scripting layer could not be started or reloaded.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum PythonInitializationError {
    MainScriptError,
    AlreadyInitialized,
    NotInitializedError,
}

#[derive(Debug, PartialEq, Eq)]
pub enum QuakeLiveEngineError {
    NullPointerPassed(String),
    EntityNotFound(String),
    InvalidId(i32),
    ClientNotFound(String),
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
