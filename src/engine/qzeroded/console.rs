use alloc::ffi::CString;
use core::{
    ffi::{CStr, c_char, c_int},
    hint::cold_path,
};

use detour::{RawDetour, TypedDetour};
#[cfg(target_os = "linux")]
use procfs::process::MemoryMap;
use tap::TapFallible;

#[cfg(target_os = "linux")]
use crate::engine::symbols::function_at;
#[cfg(target_os = "linux")]
use crate::engine::symbols::try_find_static_function;
#[cfg(test)]
use crate::engine::{mock_quake_functions, test_helpers};
use crate::{
    engine::{QuakeLiveEngine, QuakeLiveEngineError, patterns::QuakeLiveFunction, prelude::*},
    prelude::*,
};

pub(crate) trait AddCommand<T: AsRef<str>> {
    fn add_command(&self, cmd: T, func: unsafe extern "C" fn());
}

impl<T: AsRef<str>> AddCommand<T> for QuakeLiveEngine {
    fn add_command(&self, cmd: T, func: unsafe extern "C" fn()) {
        let _ = self.cmd_addcommand_detour().tap_ok(|detour| {
            let _ = CString::new(cmd.as_ref()).tap_ok(|c_cmd| detour.call(c_cmd.as_ptr(), func));
        });
    }
}

#[cfg(test)]
mod add_command_quake_live_engine_tests {
    use core::ffi::CStr;

    use super::{
        AddCommand, QuakeLiveEngine, mock_quake_functions::Cmd_AddCommand_context, test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    fn add_command_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.add_command("spank", test_func);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn add_command_with_valid_function() {
        let add_command_ctx = Cmd_AddCommand_context();
        add_command_ctx
            .expect()
            .withf(|&cvar_name, _| {
                // SAFETY: the engine passes a valid, NUL-terminated C string.
                !cvar_name.is_null() && unsafe { CStr::from_ptr(cvar_name) } == c"spank"
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.add_command("spank", test_func);
    }
}

pub(crate) trait ComPrintf {
    fn com_printf(&self, msg: &str);
}

impl ComPrintf for QuakeLiveEngine {
    fn com_printf(&self, msg: &str) {
        let _ = self
            .com_printf_detour()
            // SAFETY: the trampoline has the signature of the detoured function.
            .map(|detour| unsafe {
                mem::transmute::<*const (), extern "C" fn(*const c_char, ...)>(detour.trampoline())
            })
            .tap_ok(|original_func| {
                let _ = CString::new(msg).tap_ok(|c_msg| {
                    original_func(c_msg.as_ptr());
                });
            });
    }
}

#[cfg(test)]
mod com_printf_quake_live_engine_tests {
    use core::ffi::CStr;

    use super::{
        ComPrintf, QuakeLiveEngine, mock_quake_functions::Com_Printf_context, test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    fn com_printf_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.com_printf("asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn com_printf_with_valid_detour_function() {
        let com_printf_ctx = Com_Printf_context();
        com_printf_ctx
            .expect()
            // SAFETY: the mocked engine call passes a valid, NUL-terminated C string.
            .withf(|&value| !value.is_null() && unsafe { CStr::from_ptr(value) } == c"asdf")
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.com_printf("asdf");
    }
}

pub(crate) trait CmdArgs {
    fn cmd_args(&self) -> Option<String>;
}

impl CmdArgs for QuakeLiveEngine {
    fn cmd_args(&self) -> Option<String> {
        self.cmd_args_orig()
            .map(|original_func| original_func())
            .ok()
            .filter(|cmd_args| !cmd_args.is_null())
            // SAFETY: the mocked engine call passes a valid, NUL-terminated C string.
            .map(|cmd_args| unsafe { CStr::from_ptr(cmd_args) }.to_string_lossy().into())
    }
}

#[cfg(test)]
mod cmd_args_quake_live_engine_tests {
    use core::ptr;

    use super::{
        CmdArgs, QuakeLiveEngine, mock_quake_functions::Cmd_Args_context, test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    fn cmd_args_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_args();
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_args_with_valid_ogirinal_function() {
        let returned = c"expected cmd_args return";

        let cmd_args_ctx = Cmd_Args_context();
        cmd_args_ctx
            .expect()
            .returning(move || returned.as_ptr())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_args();
        assert!(result.is_some_and(|args| args == "expected cmd_args return"))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_args_when_original_function_returns_null() {
        let cmd_args_ctx = Cmd_Args_context();
        cmd_args_ctx.expect().returning(ptr::null).times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_args();
        assert!(result.is_none());
    }
}

pub(crate) trait CmdArgc {
    fn cmd_argc(&self) -> i32;
}

impl CmdArgc for QuakeLiveEngine {
    fn cmd_argc(&self) -> i32 {
        self.cmd_argc_orig()
            .map_or(0, |original_func| original_func())
    }
}

#[cfg(test)]
mod cmd_argc_quake_live_engine_tests {
    use pretty_assertions::assert_eq;

    use super::{
        CmdArgc, QuakeLiveEngine, mock_quake_functions::Cmd_Argc_context, test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    fn cmd_argc_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_argc();
        assert_eq!(result, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_argc_with_valid_original_function() {
        let cmd_argc_ctx = Cmd_Argc_context();
        cmd_argc_ctx.expect().returning(|| 42).times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argc();
        assert_eq!(result, 42);
    }
}

pub(crate) trait CmdArgv<T: Into<c_int>> {
    fn cmd_argv(&self, argno: T) -> Option<String>;
}

impl<T: Into<c_int> + PartialOrd<c_int>> CmdArgv<T> for QuakeLiveEngine {
    fn cmd_argv(&self, argno: T) -> Option<String> {
        if argno < 0 {
            cold_path();
            return None;
        }
        self.cmd_argv_orig()
            .map(|original_func| original_func(argno.into()))
            .ok()
            .filter(|cmd_argv| !cmd_argv.is_null())
            // SAFETY: the mocked engine call passes a valid, NUL-terminated C string.
            .map(|cmd_argv| unsafe { CStr::from_ptr(cmd_argv) }.to_string_lossy().into())
    }
}

#[cfg(test)]
mod cmd_argv_quake_live_engine_tests {
    use core::ptr;

    use mockall::predicate;

    use super::{
        CmdArgv, QuakeLiveEngine, mock_quake_functions::Cmd_Argv_context, test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    fn cmd_argv_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_argv(1);
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_argv_with_valid_ogirinal_function() {
        let returned = c"expected cmd_argv return";

        let cmd_argv_ctx = Cmd_Argv_context();
        cmd_argv_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(move |_| returned.as_ptr())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argv(2);
        assert!(result.is_some_and(|args| args == "expected cmd_argv return"))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_argv_when_original_function_returns_null() {
        let cmd_argv_ctx = Cmd_Argv_context();
        cmd_argv_ctx
            .expect()
            .with(predicate::eq(1))
            .returning(|_| ptr::null())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argv(1);
        assert!(result.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_argv_for_negative_argument_number() {
        let cmd_argv_ctx = Cmd_Argv_context();
        cmd_argv_ctx.expect().times(0);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argv(-1);
        assert!(result.is_none());
    }
}

pub(crate) trait ConsoleCommand<T: AsRef<str>> {
    fn execute_console_command(&self, cmd: T);
}

impl<T: AsRef<str>> ConsoleCommand<T> for QuakeLiveEngine {
    fn execute_console_command(&self, cmd: T) {
        let _ = self.cmd_executestring_orig().tap_ok(|original_func| {
            let _ = CString::new(cmd.as_ref()).tap_ok(|c_cmd| {
                original_func(c_cmd.as_ptr());
            });
        });
    }
}

#[cfg(test)]
mod console_command_quake_live_engine_tests {
    use core::ffi::CStr;

    use super::{
        ConsoleCommand, QuakeLiveEngine, mock_quake_functions::Cmd_ExecuteString_context,
        test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    fn execute_console_command_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        quake_engine.execute_console_command("!slap 0 100");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn execute_console_command_with_valid_original_function() {
        let cmd_execute_string_ctx = Cmd_ExecuteString_context();
        cmd_execute_string_ctx
            .expect()
            // SAFETY: the mocked engine call passes a valid, NUL-terminated C string.
            .withf(|&cmd| !cmd.is_null() && unsafe { CStr::from_ptr(cmd) } == c"!slap 0 100")
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.execute_console_command("!slap 0 100");
    }
}

#[derive(Debug)]
pub(in crate::engine) struct ConsoleFunctions {
    pub(in crate::engine) com_printf_orig: unsafe extern "C" fn(*const c_char, ...),
    pub(in crate::engine) cmd_addcommand_orig: extern "C" fn(*const c_char, unsafe extern "C" fn()),
    pub(in crate::engine) cmd_args_orig: extern "C" fn() -> *const c_char,
    pub(in crate::engine) cmd_argv_orig: extern "C" fn(c_int) -> *const c_char,
    pub(in crate::engine) cmd_tokenizestring_orig: extern "C" fn(*const c_char) -> *const c_char,
    pub(in crate::engine) cbuf_executetext_orig: extern "C" fn(cbufExec_t, *const c_char),
    pub(in crate::engine) cmd_executestring_orig: extern "C" fn(*const c_char),
    pub(in crate::engine) cmd_argc_orig: extern "C" fn() -> c_int,
}

#[cfg(target_os = "linux")]
const OFFSET_CMD_ARGC: i32 = 0x81;

impl ConsoleFunctions {
    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn locate(
        qzeroded_maps: &[&MemoryMap],
        sv_map_f_orig: extern "C" fn(),
    ) -> Result<Self, QuakeLiveEngineError> {
        let com_printf_orig = try_find_static_function::<extern "C" fn(*const c_char, ...)>(
            qzeroded_maps,
            QuakeLiveFunction::Com_Printf,
        )?;

        let cmd_addcommand_orig = try_find_static_function::<
            extern "C" fn(*const c_char, unsafe extern "C" fn()),
        >(qzeroded_maps, QuakeLiveFunction::Cmd_AddCommand)?;

        let cmd_args_orig = try_find_static_function::<extern "C" fn() -> *const c_char>(
            qzeroded_maps,
            QuakeLiveFunction::Cmd_Args,
        )?;

        let cmd_argv_orig = try_find_static_function::<extern "C" fn(c_int) -> *const c_char>(
            qzeroded_maps,
            QuakeLiveFunction::Cmd_Argv,
        )?;

        let cmd_tokenizestring_orig = try_find_static_function::<
            extern "C" fn(*const c_char) -> *const c_char,
        >(
            qzeroded_maps, QuakeLiveFunction::Cmd_Tokenizestring
        )?;

        let cbuf_executetext_orig = try_find_static_function::<
            extern "C" fn(cbufExec_t, *const c_char),
        >(qzeroded_maps, QuakeLiveFunction::Cbuf_ExecuteText)?;
        let cmd_executestring_orig = try_find_static_function::<extern "C" fn(*const c_char)>(
            qzeroded_maps,
            QuakeLiveFunction::Cmd_ExecuteString,
        )?;

        // Cmd_Argc is really small, making it hard to search for, so we use a reference to it instead.
        // SAFETY: the address derives from a function of the loaded engine, whose code is readable.
        let base_address = unsafe {
            ptr::read_unaligned((sv_map_f_orig as usize + OFFSET_CMD_ARGC as usize) as *const i32)
        };
        #[allow(
            clippy::fn_to_numeric_cast_with_truncation,
            reason = "the symbol search computes with 32-bit engine offsets"
        )]
        let cmd_argc_ptr = base_address + sv_map_f_orig as i32 + OFFSET_CMD_ARGC + 4;
        debug!(target: "shinqlx", func = "search_static_functions"; "{}: {:#X}", QuakeLiveFunction::Cmd_Argc, cmd_argc_ptr);
        let cmd_argc_orig =
                // SAFETY: the address was found in the engine and has the signature of the function type.
                unsafe { function_at::<extern "C" fn() -> c_int>(cmd_argc_ptr as usize) };

        Ok(Self {
            com_printf_orig,
            cmd_addcommand_orig,
            cmd_args_orig,
            cmd_argv_orig,
            cmd_tokenizestring_orig,
            cbuf_executetext_orig,
            cmd_executestring_orig,
            cmd_argc_orig,
        })
    }
}

#[derive(Debug)]
pub(in crate::engine) struct ConsoleDetours {
    pub(in crate::engine) cmd_addcommand_detour:
        TypedDetour<extern "C" fn(*const c_char, unsafe extern "C" fn())>,
    pub(in crate::engine) com_printf_detour: RawDetour,
}

impl ConsoleDetours {
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(in crate::engine) fn hook(engine: &QuakeLiveEngine) -> Result<Self, QuakeLiveEngineError> {
        let cmd_addcommand_detour = QuakeLiveFunction::Cmd_AddCommand
            .create_and_enable_generic_detour(
                engine.cmd_addcommand_orig()?,
                engine.callbacks().console.cmd_addcommand,
            )?;

        // SAFETY: the original is a function of the engine and the replacement has the same signature.
        let com_printf_detour = unsafe {
            RawDetour::new(
                engine.com_printf_orig()? as *const (),
                engine.callbacks().console.com_printf as *const (),
            )
            .map_err(QuakeLiveEngineError::detour_not_created(
                QuakeLiveFunction::Com_Printf,
            ))?
        };
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe {
            com_printf_detour
                .enable()
                .map_err(QuakeLiveEngineError::detour_not_enabled(
                    QuakeLiveFunction::Com_Printf,
                ))?
        };

        Ok(Self {
            cmd_addcommand_detour,
            com_printf_detour,
        })
    }
}

impl QuakeLiveEngine {
    pub(in crate::engine) fn com_printf_orig(
        &self,
    ) -> Result<unsafe extern "C" fn(*const c_char, ...), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Com_Printf,
                ))
            },
            |static_functions| Ok(static_functions.console.com_printf_orig),
        )
    }

    pub(in crate::engine) fn cmd_addcommand_orig(
        &self,
    ) -> Result<extern "C" fn(*const c_char, unsafe extern "C" fn()), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_AddCommand,
                ))
            },
            |static_functions| Ok(static_functions.console.cmd_addcommand_orig),
        )
    }

    pub(in crate::engine) fn cmd_args_orig(
        &self,
    ) -> Result<extern "C" fn() -> *const c_char, QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Args,
                ))
            },
            |static_functions| Ok(static_functions.console.cmd_args_orig),
        )
    }

    pub(in crate::engine) fn cmd_argv_orig(
        &self,
    ) -> Result<extern "C" fn(c_int) -> *const c_char, QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Argv,
                ))
            },
            |static_functions| Ok(static_functions.console.cmd_argv_orig),
        )
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "stand-ins for engine symbols that not every test calls"
        )
    )]
    pub(in crate::engine) fn cmd_tokenizestring_orig(
        &self,
    ) -> Result<extern "C" fn(*const c_char) -> *const c_char, QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Tokenizestring,
                ))
            },
            |static_functions| Ok(static_functions.console.cmd_tokenizestring_orig),
        )
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "stand-ins for engine symbols that not every test calls"
        )
    )]
    pub(in crate::engine) fn cbuf_executetext_orig(
        &self,
    ) -> Result<extern "C" fn(cbufExec_t, *const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cbuf_ExecuteText,
                ))
            },
            |static_functions| Ok(static_functions.console.cbuf_executetext_orig),
        )
    }

    pub(in crate::engine) fn cmd_executestring_orig(
        &self,
    ) -> Result<extern "C" fn(*const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_ExecuteString,
                ))
            },
            |static_functions| Ok(static_functions.console.cmd_executestring_orig),
        )
    }

    pub(in crate::engine) fn cmd_argc_orig(
        &self,
    ) -> Result<extern "C" fn() -> c_int, QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_Argc,
                ))
            },
            |static_functions| Ok(static_functions.console.cmd_argc_orig),
        )
    }

    pub(in crate::engine) fn cmd_addcommand_detour(
        &self,
    ) -> Result<
        &TypedDetour<extern "C" fn(*const c_char, unsafe extern "C" fn())>,
        QuakeLiveEngineError,
    > {
        self.static_detours.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::Cmd_AddCommand,
                ))
            },
            |static_detours| Ok(&static_detours.console.cmd_addcommand_detour),
        )
    }

    pub(in crate::engine) fn com_printf_detour(&self) -> Result<&RawDetour, QuakeLiveEngineError> {
        self.static_detours.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::Com_Printf,
                ))
            },
            |static_detours| Ok(&static_detours.console.com_printf_detour),
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
    fn com_printf_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.com_printf_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Com_Printf,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn com_printf_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.com_printf_orig();
        assert!(result.is_ok_and(|func| {
            ptr::fn_addr_eq(func, Com_Printf as unsafe extern "C" fn(*const c_char, ...))
        }));
    }

    #[test]
    fn cmd_addcommand_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_addcommand_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cmd_AddCommand,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_addcommand_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_addcommand_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cmd_AddCommand as extern "C" fn(*const c_char, unsafe extern "C" fn())
        )));
    }

    #[test]
    fn cmd_args_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_args_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cmd_Args,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_args_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_args_orig();
        assert!(
            result.is_ok_and(|func| ptr::fn_addr_eq(
                func,
                Cmd_Args as extern "C" fn() -> *const c_char
            ))
        );
    }

    #[test]
    fn cmd_argv_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_argv_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cmd_Argv,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_argv_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argv_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cmd_Argv as extern "C" fn(c_int) -> *const c_char
        )));
    }

    #[test]
    fn cmd_tokenizestring_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_tokenizestring_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::Cmd_Tokenizestring,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_tokenizestring_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_tokenizestring_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cmd_Tokenizestring as extern "C" fn(*const c_char) -> *const c_char
        )));
    }

    #[test]
    fn cbuf_exectutetext_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cbuf_executetext_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cbuf_ExecuteText,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cbuf_executetext_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cbuf_executetext_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cbuf_ExecuteText as extern "C" fn(cbufExec_t, *const c_char)
        )));
    }

    #[test]
    fn cmd_executestring_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_executestring_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::Cmd_ExecuteString,
                ))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_executestring_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_executestring_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            Cmd_ExecuteString as extern "C" fn(*const c_char)
        )));
    }

    #[test]
    fn cmd_argc_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_argc_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::Cmd_Argc,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_argc_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_argc_orig();
        assert!(
            result.is_ok_and(|func| ptr::fn_addr_eq(func, Cmd_Argc as extern "C" fn() -> c_int))
        );
    }

    #[test]
    fn cmd_addcommand_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.cmd_addcommand_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::Cmd_AddCommand,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cmd_addcommand_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.cmd_addcommand_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn com_printf_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.com_printf_detour();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::Com_Printf,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn com_printf_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.com_printf_detour();
        assert!(result.is_ok());
    }
}
