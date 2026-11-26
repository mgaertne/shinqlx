//! The server program `qzeroded`, which is loaded once.

use core::hint::cold_path;

#[cfg(target_os = "linux")]
use procfs::process::{MMapPath, Process};

#[cfg(target_os = "linux")]
use crate::QZERODED;
use crate::{
    engine::{
        QuakeLiveEngine, QuakeLiveEngineError,
        qagame::module::{ModuleLoaderDetours, ModuleLoaderFunctions},
    },
    prelude::*,
};

pub(crate) mod client;
pub(crate) mod console;
pub(crate) mod cvar;
#[allow(
    dead_code,
    reason = "the scan that uses the file system follows in the next steps"
)]
pub(crate) mod fs;
pub(crate) mod server;
pub(crate) mod server_static;

use console::{ConsoleDetours, ConsoleFunctions};
use cvar::{CvarDetours, CvarFunctions};
use server::{ServerDetours, ServerFunctions};

/// The functions of the server program that are searched at the start.
#[derive(Debug)]
pub(in crate::engine) struct StaticFunctions {
    pub(in crate::engine) console: ConsoleFunctions,
    pub(in crate::engine) cvar: CvarFunctions,
    pub(in crate::engine) server: ServerFunctions,
    pub(in crate::engine) module_loader: ModuleLoaderFunctions,
}

/// The hooks into the server program.
#[derive(Debug)]
pub(in crate::engine) struct StaticDetours {
    pub(in crate::engine) console: ConsoleDetours,
    pub(in crate::engine) cvar: CvarDetours,
    pub(in crate::engine) server: ServerDetours,
    pub(in crate::engine) module_loader: ModuleLoaderDetours,
}

impl QuakeLiveEngine {
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(crate) fn search_static_functions(&self) -> Result<(), QuakeLiveEngineError> {
        #[cfg(not(target_os = "linux"))]
        return Err(QuakeLiveEngineError::ProcessNotFound(
            "could not find my own process".to_string(),
        ));
        #[cfg(target_os = "linux")]
        {
            let myself_process = Process::myself().map_err(|err| {
                cold_path();
                QuakeLiveEngineError::ProcessNotFound(err.to_string())
            })?;
            let myself_maps = myself_process.maps().map_err(|err| {
                cold_path();
                QuakeLiveEngineError::NoMemoryMappingInformationFound(err.to_string())
            })?;
            let qzeroded_maps = myself_maps
                .iter()
                .filter(|mmap| {
                    let MMapPath::Path(path) = &mmap.pathname else {
                        return false;
                    };
                    path.file_name()
                        .is_some_and(|file_name| file_name.to_string_lossy() == QZERODED)
                })
                .collect::<Vec<_>>();

            if qzeroded_maps.is_empty() {
                cold_path();
                error!(target: "shinqlx", func = "search_static_functions"; "no memory mapping information for {QZERODED} found");
                return Err(QuakeLiveEngineError::NoMemoryMappingInformationFound(
                    "no memory mapping information found".to_string(),
                ));
            }

            debug!(target: "shinqlx", func = "search_static_functions"; "Searching for necessary functions...");
            let server = ServerFunctions::locate(&qzeroded_maps)?;
            let console = ConsoleFunctions::locate(&qzeroded_maps, server.sv_map_f_orig)?;
            let cvar = CvarFunctions::locate(&qzeroded_maps)?;
            let module_loader = ModuleLoaderFunctions::locate(&qzeroded_maps)?;

            (self.callbacks().lookup.engine_functions_found)(&qzeroded_maps);

            self.static_functions
                .set(StaticFunctions {
                    console,
                    cvar,
                    server,
                    module_loader,
                })
                .expect("the static functions are searched only once per engine");

            Ok(())
        }
    }

    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(crate) fn hook_static(&self) -> Result<(), QuakeLiveEngineError> {
        debug!(target: "shinqlx", func = "hook_static"; "Hooking...");
        let console = ConsoleDetours::hook(self)?;
        let module_loader = ModuleLoaderDetours::hook(self)?;
        let server = ServerDetours::hook(self)?;
        let cvar = CvarDetours::hook(self)?;

        self.static_detours
            .set(StaticDetours {
                console,
                cvar,
                server,
                module_loader,
            })
            .expect("the static detours are hooked only once per engine");

        Ok(())
    }
}

#[cfg(test)]
mod static_table_tests {
    use crate::{
        engine::{QuakeLiveEngineError, patterns::QuakeLiveFunction, test_helpers::*},
        prelude::serial,
    };

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    #[cfg(target_os = "linux")]
    fn search_static_functions_without_engine_module_reports_missing_memory_mapping() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.search_static_functions();

        assert!(matches!(
            result,
            Err(QuakeLiveEngineError::NoMemoryMappingInformationFound(_))
        ));
    }

    #[test]
    fn hook_static_without_static_functions_reports_missing_function() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.hook_static();

        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cmd_AddCommand)));
    }
}
