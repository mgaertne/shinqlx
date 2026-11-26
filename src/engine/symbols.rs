use core::hint::cold_path;

#[cfg(target_os = "linux")]
use procfs::process::MemoryMap;

#[cfg(target_os = "linux")]
use crate::engine::patterns::pattern_search_module;
use crate::{
    engine::{QuakeLiveEngineError, patterns::QuakeLiveFunction},
    prelude::*,
};

#[cfg(target_os = "linux")]
/// # Safety
/// `address` must be the address of a function with the signature `FuncType`.
pub(super) unsafe fn function_at<FuncType: Copy>(address: usize) -> FuncType {
    const { assert!(size_of::<FuncType>() == size_of::<*const ()>()) };
    let function_pointer = ptr::with_exposed_provenance::<()>(address);
    // SAFETY: the pointer is a function address with the signature of FuncType.
    unsafe { mem::transmute_copy::<*const (), FuncType>(&function_pointer) }
}

pub(super) fn try_find_static_function<FuncType: Copy>(
    maps: &[&MemoryMap],
    func: QuakeLiveFunction,
) -> Result<FuncType, QuakeLiveEngineError> {
    pattern_search_module(maps, func).map_or_else(
        || {
            cold_path();
            error!(target: "shinqlx", func = "try_find_static_function"; "Function {} not found", func);
            Err(QuakeLiveEngineError::StaticFunctionNotFound(func))
        },
        |result| {
            debug!(target: "shinqlx", func = "try_find_static_function"; "{}: {:#X}", func, result);
            // SAFETY: the address was found in the engine and has the signature of the function type.
            Ok(unsafe { function_at::<FuncType>(result) })
        },
    )
}
