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
use crate::engine::symbols::try_find_static_function;
#[cfg(test)]
use crate::engine::{mock_quake_functions, test_helpers};
use crate::{
    engine::{QuakeLiveEngine, QuakeLiveEngineError, patterns::QuakeLiveFunction, prelude::*},
    prelude::*,
};

pub(crate) trait ExecuteClientCommand<T: AsMut<client_t>, U: AsRef<str>, V: Into<qboolean>> {
    fn execute_client_command(&self, client: Option<T>, cmd: U, client_ok: V);
}

impl<T: AsMut<client_t>, U: AsRef<str>, V: Into<qboolean>> ExecuteClientCommand<T, U, V>
    for QuakeLiveEngine
{
    fn execute_client_command(&self, client: Option<T>, cmd: U, client_ok: V) {
        let _ = self.sv_executeclientcommand_detour().tap_ok(|detour| {
            let _ = CString::new(cmd.as_ref()).tap_ok(|c_command| {
                let raw_client = client.map_or(ptr::null_mut(), |mut c_client| c_client.as_mut());
                detour.call(raw_client, c_command.as_ptr(), client_ok.into());
            });
        });
    }
}

#[cfg(test)]
mod execute_client_command_quake_live_engine_tests {
    use core::ffi::CStr;

    use super::{
        ExecuteClientCommand, QuakeLiveEngine,
        mock_quake_functions::SV_ExecuteClientCommand_context, test_helpers::*,
    };
    use crate::{
        engine::prelude::{Client, ClientBuilder, MockClient},
        prelude::serial,
    };

    #[test]
    fn execute_client_command_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.execute_client_command(None::<Client>, "asdf", false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn execute_client_command_with_valid_detour_function_and_no_client() {
        let sv_execute_client_command_ctx = SV_ExecuteClientCommand_context();
        sv_execute_client_command_ctx
            .expect()
            .withf(|&client, &cmd, &client_ok| {
                client.is_null()
                    && !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"asdf"
                    && client_ok.into()
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.execute_client_command(None::<Client>, "asdf", true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn execute_client_command_with_valid_detour_function_and_valid_client() {
        let mut mock_client = MockClient::default();
        mock_client.expect_as_mut().returning(|| {
            ClientBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let sv_execute_client_command_ctx = SV_ExecuteClientCommand_context();
        sv_execute_client_command_ctx
            .expect()
            .withf(|&client, &cmd, &client_ok| {
                !client.is_null()
                    && !cmd.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(cmd) } == c"asdf"
                    && client_ok.into()
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.execute_client_command(Some(mock_client), "asdf", true);
    }
}

pub(crate) trait SendServerCommand<T: AsRef<client_t>> {
    fn send_server_command(&self, client: Option<T>, command: &str);
}

impl<T: AsRef<client_t>> SendServerCommand<T> for QuakeLiveEngine {
    fn send_server_command(&self, client: Option<T>, command: &str) {
        let _ = self
            .sv_sendservercommand_detour()
            // SAFETY: the trampoline has the signature of the detoured function.
            .map(|detour| unsafe {
                mem::transmute::<*const (), extern "C" fn(*const client_t, *const c_char, ...)>(
                    detour.trampoline(),
                )
            })
            .tap_ok(|original_func| {
                let _ = CString::new(command).tap_ok(|c_command| {
                    let raw_client = client.map_or(ptr::null(), |c_client| c_client.as_ref());
                    original_func(raw_client, c_command.as_ptr());
                });
            });
    }
}

#[cfg(test)]
mod send_server_command_quake_live_engine_tests {
    use core::ffi::CStr;

    use super::{
        QuakeLiveEngine, SendServerCommand, mock_quake_functions::SV_SendServerCommand_context,
        test_helpers::*,
    };
    use crate::{
        engine::prelude::{Client, ClientBuilder, MockClient},
        prelude::serial,
    };

    #[test]
    fn send_server_command_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.send_server_command(None::<Client>, "asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_with_valid_detour_function_and_no_client() {
        let sv_send_server_command_ctx = SV_SendServerCommand_context();
        sv_send_server_command_ctx
            .expect()
            .withf(|&client, &cmd| {
                // SAFETY: the engine passes a valid, NUL-terminated C string.
                client.is_null() && !cmd.is_null() && unsafe { CStr::from_ptr(cmd) } == c"asdf"
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.send_server_command(None::<Client>, "asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn send_server_command_with_valid_detour_function_and_valid_client() {
        let mut mock_client = MockClient::default();
        mock_client.expect_as_ref().return_const(
            ClientBuilder::default()
                .build()
                .expect("this should not happen"),
        );

        let sv_send_server_command_ctx = SV_SendServerCommand_context();
        sv_send_server_command_ctx
            .expect()
            .withf(|&client, &cmd| {
                // SAFETY: the engine passes a valid, NUL-terminated C string.
                !client.is_null() && !cmd.is_null() && unsafe { CStr::from_ptr(cmd) } == c"asdf"
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.send_server_command(Some(mock_client), "asdf");
    }
}

pub(crate) trait ClientEnterWorld<T: AsMut<client_t>> {
    fn client_enter_world(&self, client: T, cmd: *mut usercmd_t);
}

impl<T: AsMut<client_t>> ClientEnterWorld<T> for QuakeLiveEngine {
    fn client_enter_world(&self, mut client: T, cmd: *mut usercmd_t) {
        let _ = self.sv_cliententerworld_detour().tap_ok(|detour| {
            detour.call(client.as_mut(), cmd);
        });
    }
}

#[cfg(test)]
mod client_enter_world_quake_live_engine_tests {
    use core::borrow::BorrowMut;

    use super::{
        ClientEnterWorld, QuakeLiveEngine, mock_quake_functions::SV_ClientEnterWorld_context,
        test_helpers::*,
    };
    use crate::{
        engine::prelude::{ClientBuilder, MockClient, UserCmdBuilder, usercmd_t},
        prelude::serial,
    };

    #[test]
    #[serial]
    fn client_enter_world_with_no_detour_set() {
        let mock_client = MockClient::default();

        let mut usercmd = UserCmdBuilder::default()
            .build()
            .expect("this should not happen");
        let quake_engine = default_quake_engine();

        quake_engine.client_enter_world(mock_client, usercmd.borrow_mut() as *mut usercmd_t);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn client_enter_world_with_valid_detour_function() {
        let mut mock_client = MockClient::default();
        mock_client.expect_as_mut().returning(|| {
            ClientBuilder::default()
                .build()
                .expect("this should not happen")
        });

        let mut usercmd = UserCmdBuilder::default()
            .build()
            .expect("this should not happen");

        let sv_client_enter_world_ctx = SV_ClientEnterWorld_context();
        sv_client_enter_world_ctx
            .expect()
            .withf(|&client, _| !client.is_null())
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.client_enter_world(mock_client, usercmd.borrow_mut() as *mut usercmd_t);
    }
}

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
pub(crate) trait HandleSteamPacket {
    fn handle_steam_packet(&self, from: *const netadr_t, msg: *const msg_t) -> bool;
}

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
impl HandleSteamPacket for QuakeLiveEngine {
    fn handle_steam_packet(&self, from: *const netadr_t, msg: *const msg_t) -> bool {
        self.sv_handlesteampacket_detour()
            .is_ok_and(|detour| detour.call(from, msg) != 0)
    }
}

#[cfg(test)]
mod handle_steam_packet_quake_live_engine_tests {
    use core::ptr;

    use super::{
        HandleSteamPacket, QuakeLiveEngine, mock_quake_functions::SV_HandleSteamPacket_context,
        test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    #[serial]
    fn handle_steam_packet_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        assert!(!quake_engine.handle_steam_packet(ptr::null(), ptr::null()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_steam_packet_with_valid_detour_function() {
        let sv_handle_steam_packet_ctx = SV_HandleSteamPacket_context();
        sv_handle_steam_packet_ctx.expect().times(1).return_const(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        assert!(quake_engine.handle_steam_packet(ptr::null(), ptr::null()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn handle_steam_packet_reports_unhandled_packets() {
        let sv_handle_steam_packet_ctx = SV_HandleSteamPacket_context();
        sv_handle_steam_packet_ctx.expect().times(1).return_const(0);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        assert!(!quake_engine.handle_steam_packet(ptr::null(), ptr::null()));
    }
}

pub(crate) trait SetConfigstring<T: Into<c_int>> {
    fn set_configstring(&self, index: T, value: &str);
}

impl<T: Into<c_int>> SetConfigstring<T> for QuakeLiveEngine {
    fn set_configstring(&self, index: T, value: &str) {
        let _ = self.sv_setconfgistring_detour().tap_ok(|detour| {
            let _ = CString::new(value).tap_ok(|c_value| {
                detour.call(index.into(), c_value.as_ptr());
            });
        });
    }
}

#[cfg(test)]
mod set_confgistring_quake_live_engine_tests {
    use core::ffi::CStr;

    use super::{
        QuakeLiveEngine, SetConfigstring, mock_quake_functions::SV_SetConfigstring_context,
        test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    fn set_configstring_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.set_configstring(42, "asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_configstring_with_valid_detour_function() {
        let sv_set_configstring_ctx = SV_SetConfigstring_context();
        sv_set_configstring_ctx
            .expect()
            .withf(|&index, &value| {
                // SAFETY: the engine passes a valid, NUL-terminated C string.
                index == 42 && !value.is_null() && unsafe { CStr::from_ptr(value) } == c"asdf"
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.set_configstring(42, "asdf");
    }
}

pub(crate) trait SpawnServer<T: AsRef<str>, U: Into<qboolean>> {
    fn spawn_server(&self, server: T, kill_bots: U);
}

impl<T: AsRef<str>, U: Into<qboolean>> SpawnServer<T, U> for QuakeLiveEngine {
    fn spawn_server(&self, server: T, kill_bots: U) {
        let _ = self.sv_spawnserver_detour().tap_ok(|detour| {
            let _ = CString::new(server.as_ref()).tap_ok(|c_server| {
                detour.call(c_server.as_ptr().cast_mut(), kill_bots.into());
            });
        });
    }
}

#[cfg(test)]
mod spawn_server_quake_live_engine_tests {
    use core::ffi::CStr;

    use super::{
        QuakeLiveEngine, SpawnServer, mock_quake_functions::SV_SpawnServer_context, test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    fn spawn_server_with_no_detour_set() {
        let quake_engine = default_quake_engine();

        quake_engine.spawn_server("asdf", false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn com_printf_with_valid_detour_function() {
        let sv_spawn_server_ctx = SV_SpawnServer_context();
        sv_spawn_server_ctx
            .expect()
            .withf(|&server_name, &kill_bots| {
                !server_name.is_null()
                    // SAFETY: the engine passes a valid, NUL-terminated C string.
                    && unsafe { CStr::from_ptr(server_name) } == c"asdf"
                    && kill_bots.into()
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        quake_engine.spawn_server("asdf", true);
    }
}

pub(crate) trait GetConfigstring<T: Into<c_int>> {
    fn get_configstring(&self, index: T) -> String;
}

impl<T: Into<c_int>> GetConfigstring<T> for QuakeLiveEngine {
    fn get_configstring(&self, index: T) -> String {
        self.sv_getconfigstring_orig().map_or_else(
            |_| "".into(),
            |original_func| {
                let mut buffer: [u8; MAX_STRING_CHARS as usize] = [0; MAX_STRING_CHARS as usize];
                original_func(
                    index.into(),
                    buffer.as_mut_ptr() as *mut c_char,
                    buffer.len() as c_int,
                );
                CStr::from_bytes_until_nul(&buffer)
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into()
            },
        )
    }
}

#[cfg(test)]
mod get_configstring_quake_live_engine_tests {
    use pretty_assertions::assert_eq;

    use super::{
        GetConfigstring, QuakeLiveEngine, mock_quake_functions::SV_GetConfigstring_context,
        test_helpers::*,
    };
    use crate::prelude::serial;

    #[test]
    fn get_configstring_with_no_original_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.get_configstring(42);
        assert_eq!(result, "");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_configstring_with_valid_original_function() {
        let sv_get_configstring_ctx = SV_GetConfigstring_context();
        sv_get_configstring_ctx
            .expect()
            .withf(|&index, &_buffer, &_buffer_len| index == 42)
            .returning(|_, buffer, buffer_len| {
                let returned = c"asdf";
                // SAFETY: the buffer has room for buffer_len bytes and does not overlap the source.
                unsafe { returned.as_ptr().copy_to(buffer, buffer_len as usize) };
            })
            .times(1);

        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.get_configstring(42);
        assert_eq!(result, "asdf");
    }
}

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
pub(in crate::engine) type SteamPacketHandlerType =
    extern "C" fn(*const netadr_t, *const msg_t) -> c_int;

#[derive(Debug)]
pub(in crate::engine) struct ServerFunctions {
    pub(in crate::engine) sv_sendservercommand_orig:
        unsafe extern "C" fn(*mut client_t, *const c_char, ...),
    pub(in crate::engine) sv_executeclientcommand_orig:
        extern "C" fn(*mut client_t, *const c_char, qboolean),
    pub(in crate::engine) sv_shutdown_orig: extern "C" fn(*const c_char),
    pub(in crate::engine) sv_map_f_orig: extern "C" fn(),
    pub(in crate::engine) sv_cliententerworld_orig: extern "C" fn(*mut client_t, *mut usercmd_t),
    pub(in crate::engine) sv_setconfigstring_orig: extern "C" fn(c_int, *const c_char),
    pub(in crate::engine) sv_getconfigstring_orig: extern "C" fn(c_int, *mut c_char, c_int),
    pub(in crate::engine) sv_dropclient_orig: extern "C" fn(*mut client_t, *const c_char),
    pub(in crate::engine) sv_spawnserver_orig: extern "C" fn(*mut c_char, qboolean),
    #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
    pub(in crate::engine) sv_handlesteampacket_orig: Option<SteamPacketHandlerType>,
}

impl ServerFunctions {
    #[cfg(target_os = "linux")]
    pub(in crate::engine) fn locate(
        qzeroded_maps: &[&MemoryMap],
    ) -> Result<Self, QuakeLiveEngineError> {
        let sv_sendservercommand_orig = try_find_static_function::<
            extern "C" fn(*mut client_t, *const c_char, ...),
        >(
            qzeroded_maps, QuakeLiveFunction::SV_SendServerCommand
        )?;

        let sv_executeclientcommand_orig =
            try_find_static_function::<extern "C" fn(*mut client_t, *const c_char, qboolean)>(
                qzeroded_maps,
                QuakeLiveFunction::SV_ExecuteClientCommand,
            )?;

        let sv_shutdown_orig = try_find_static_function::<extern "C" fn(*const c_char)>(
            qzeroded_maps,
            QuakeLiveFunction::SV_Shutdown,
        )?;

        let sv_map_f_orig = try_find_static_function::<extern "C" fn()>(
            qzeroded_maps,
            QuakeLiveFunction::SV_Map_f,
        )?;

        let sv_cliententerworld_orig = try_find_static_function::<
            extern "C" fn(*mut client_t, *mut usercmd_t),
        >(
            qzeroded_maps, QuakeLiveFunction::SV_ClientEnterWorld
        )?;

        let sv_setconfigstring_orig = try_find_static_function::<
            extern "C" fn(c_int, *const c_char),
        >(
            qzeroded_maps, QuakeLiveFunction::SV_SetConfigstring
        )?;

        let sv_getconfigstring_orig = try_find_static_function::<
            extern "C" fn(c_int, *mut c_char, c_int),
        >(
            qzeroded_maps, QuakeLiveFunction::SV_GetConfigstring
        )?;

        let sv_dropclient_orig = try_find_static_function::<
            extern "C" fn(*mut client_t, *const c_char),
        >(qzeroded_maps, QuakeLiveFunction::SV_DropClient)?;
        let sv_spawnserver_orig = try_find_static_function::<extern "C" fn(*mut c_char, qboolean)>(
            qzeroded_maps,
            QuakeLiveFunction::SV_SpawnServer,
        )?;

        // Not essential for the game, so a changed engine only costs the query protection.
        #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
            let sv_handlesteampacket_orig = try_find_static_function::<SteamPacketHandlerType>(
                qzeroded_maps,
                QuakeLiveFunction::SV_HandleSteamPacket,
            )
            .tap_err(|err| {
                cold_path();
                error!(target: "shinqlx", func = "search_static_functions"; "{err:?}");
                error!(
                    target: "shinqlx",
                    func = "search_static_functions"; "Steam query handler not found. The server is NOT protected against query floods."
                );
            })
            .ok();

        Ok(Self {
            sv_sendservercommand_orig,
            sv_executeclientcommand_orig,
            sv_shutdown_orig,
            sv_map_f_orig,
            sv_cliententerworld_orig,
            sv_setconfigstring_orig,
            sv_getconfigstring_orig,
            sv_dropclient_orig,
            sv_spawnserver_orig,
            #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
            sv_handlesteampacket_orig,
        })
    }
}

#[derive(Debug)]
pub(in crate::engine) struct ServerDetours {
    pub(in crate::engine) sv_executeclientcommand_detour:
        TypedDetour<extern "C" fn(*mut client_t, *const c_char, qboolean)>,
    pub(in crate::engine) sv_cliententerworld_detour:
        TypedDetour<extern "C" fn(*mut client_t, *mut usercmd_t)>,
    pub(in crate::engine) sv_setconfgistring_detour:
        TypedDetour<extern "C" fn(c_int, *const c_char)>,
    pub(in crate::engine) sv_dropclient_detour:
        TypedDetour<extern "C" fn(*mut client_t, *const c_char)>,
    pub(in crate::engine) sv_spawnserver_detour: TypedDetour<extern "C" fn(*mut c_char, qboolean)>,
    #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
    pub(in crate::engine) sv_handlesteampacket_detour: Option<TypedDetour<SteamPacketHandlerType>>,
    pub(in crate::engine) sv_sendservercommand_detour: RawDetour,
}

impl ServerDetours {
    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "only called from the running game process, not from tests"
        )
    )]
    pub(in crate::engine) fn hook(engine: &QuakeLiveEngine) -> Result<Self, QuakeLiveEngineError> {
        let sv_executeclientcommand_detour = QuakeLiveFunction::SV_ExecuteClientCommand
            .create_and_enable_generic_detour(
                engine.sv_executeclientcommand_orig()?,
                engine.callbacks().server.sv_executeclientcommand,
            )?;

        let sv_cliententerworld_detour = QuakeLiveFunction::SV_ClientEnterWorld
            .create_and_enable_generic_detour(
                engine.sv_cliententerworld_orig()?,
                engine.callbacks().server.sv_cliententerworld,
            )?;

        // SAFETY: the original is a function of the engine and the replacement has the same signature.
        let sv_sendservercommand_detour = unsafe {
            RawDetour::new(
                engine.sv_sendservercommand_orig()? as *const (),
                engine.callbacks().server.sv_sendservercommand as *const (),
            )
            .map_err(QuakeLiveEngineError::detour_not_created(
                QuakeLiveFunction::SV_SendServerCommand,
            ))?
        };
        // SAFETY: the detour was created from valid function addresses of the engine.
        unsafe {
            sv_sendservercommand_detour.enable().map_err(
                QuakeLiveEngineError::detour_not_enabled(QuakeLiveFunction::SV_SendServerCommand),
            )?
        };

        let sv_setconfgistring_detour = QuakeLiveFunction::SV_SetConfigstring
            .create_and_enable_generic_detour(
                engine.sv_setconfigstring_orig()?,
                engine.callbacks().server.sv_setconfigstring,
            )?;

        let sv_dropclient_detour = QuakeLiveFunction::SV_DropClient
            .create_and_enable_generic_detour(
                engine.sv_dropclient_orig()?,
                engine.callbacks().server.sv_dropclient,
            )?;

        let sv_spawnserver_detour = QuakeLiveFunction::SV_SpawnServer
            .create_and_enable_generic_detour(
                engine.sv_spawnserver_orig()?,
                engine.callbacks().server.sv_spawnserver,
            )?;

        #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
        let sv_handlesteampacket_detour = engine
            .sv_handlesteampacket_orig()
            .ok()
            .map(|orig| {
                QuakeLiveFunction::SV_HandleSteamPacket.create_and_enable_generic_detour(
                    orig,
                    engine.callbacks().server.sv_handlesteampacket,
                )
            })
            .transpose()?;

        Ok(Self {
            sv_executeclientcommand_detour,
            sv_cliententerworld_detour,
            sv_setconfgistring_detour,
            sv_dropclient_detour,
            sv_spawnserver_detour,
            #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
            sv_handlesteampacket_detour,
            sv_sendservercommand_detour,
        })
    }
}

impl QuakeLiveEngine {
    pub(in crate::engine) fn sv_sendservercommand_orig(
        &self,
    ) -> Result<unsafe extern "C" fn(*mut client_t, *const c_char, ...), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_SendServerCommand,
                ))
            },
            |static_functions| Ok(static_functions.server.sv_sendservercommand_orig),
        )
    }

    pub(in crate::engine) fn sv_executeclientcommand_orig(
        &self,
    ) -> Result<extern "C" fn(*mut client_t, *const c_char, qboolean), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_ExecuteClientCommand,
                ))
            },
            |static_functions| Ok(static_functions.server.sv_executeclientcommand_orig),
        )
    }

    pub(crate) fn sv_shutdown_orig(
        &self,
    ) -> Result<extern "C" fn(*const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_Shutdown,
                ))
            },
            |static_functions| Ok(static_functions.server.sv_shutdown_orig),
        )
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "stand-ins for engine symbols that not every test calls"
        )
    )]
    pub(in crate::engine) fn sv_map_f_orig(&self) -> Result<extern "C" fn(), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_Map_f,
                ))
            },
            |static_functions| Ok(static_functions.server.sv_map_f_orig),
        )
    }

    pub(in crate::engine) fn sv_cliententerworld_orig(
        &self,
    ) -> Result<extern "C" fn(*mut client_t, *mut usercmd_t), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_ClientEnterWorld,
                ))
            },
            |static_functions| Ok(static_functions.server.sv_cliententerworld_orig),
        )
    }

    pub(in crate::engine) fn sv_setconfigstring_orig(
        &self,
    ) -> Result<extern "C" fn(c_int, *const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_SetConfigstring,
                ))
            },
            |static_functions| Ok(static_functions.server.sv_setconfigstring_orig),
        )
    }

    pub(in crate::engine) fn sv_getconfigstring_orig(
        &self,
    ) -> Result<extern "C" fn(c_int, *mut c_char, c_int), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_GetConfigstring,
                ))
            },
            |static_functions| Ok(static_functions.server.sv_getconfigstring_orig),
        )
    }

    pub(in crate::engine) fn sv_dropclient_orig(
        &self,
    ) -> Result<extern "C" fn(*mut client_t, *const c_char), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_DropClient,
                ))
            },
            |static_functions| Ok(static_functions.server.sv_dropclient_orig),
        )
    }

    #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
    pub(in crate::engine) fn sv_handlesteampacket_orig(
        &self,
    ) -> Result<SteamPacketHandlerType, QuakeLiveEngineError> {
        self.static_functions
            .get()
            .and_then(|static_functions| static_functions.server.sv_handlesteampacket_orig)
            .ok_or_else(|| {
                cold_path();
                QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_HandleSteamPacket,
                )
            })
    }

    pub(in crate::engine) fn sv_spawnserver_orig(
        &self,
    ) -> Result<extern "C" fn(*mut c_char, qboolean), QuakeLiveEngineError> {
        self.static_functions.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticFunctionNotFound(
                    QuakeLiveFunction::SV_SpawnServer,
                ))
            },
            |static_functions| Ok(static_functions.server.sv_spawnserver_orig),
        )
    }

    pub(in crate::engine) fn sv_executeclientcommand_detour(
        &self,
    ) -> Result<
        &TypedDetour<extern "C" fn(*mut client_t, *const c_char, qboolean)>,
        QuakeLiveEngineError,
    > {
        self.static_detours.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::SV_ExecuteClientCommand,
                ))
            },
            |static_detours| Ok(&static_detours.server.sv_executeclientcommand_detour),
        )
    }

    pub(in crate::engine) fn sv_cliententerworld_detour(
        &self,
    ) -> Result<&TypedDetour<extern "C" fn(*mut client_t, *mut usercmd_t)>, QuakeLiveEngineError>
    {
        self.static_detours.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::SV_ClientEnterWorld,
                ))
            },
            |static_detours| Ok(&static_detours.server.sv_cliententerworld_detour),
        )
    }

    pub(in crate::engine) fn sv_setconfgistring_detour(
        &self,
    ) -> Result<&TypedDetour<extern "C" fn(c_int, *const c_char)>, QuakeLiveEngineError> {
        self.static_detours.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::SV_SetConfigstring,
                ))
            },
            |static_detours| Ok(&static_detours.server.sv_setconfgistring_detour),
        )
    }

    pub(crate) fn sv_dropclient_detour(
        &self,
    ) -> Result<&TypedDetour<extern "C" fn(*mut client_t, *const c_char)>, QuakeLiveEngineError>
    {
        self.static_detours.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::SV_DropClient,
                ))
            },
            |static_detours| Ok(&static_detours.server.sv_dropclient_detour),
        )
    }

    #[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
    pub(in crate::engine) fn sv_handlesteampacket_detour(
        &self,
    ) -> Result<&TypedDetour<SteamPacketHandlerType>, QuakeLiveEngineError> {
        self.static_detours
            .get()
            .and_then(|static_detours| static_detours.server.sv_handlesteampacket_detour.as_ref())
            .ok_or_else(|| {
                cold_path();
                QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::SV_HandleSteamPacket)
            })
    }

    pub(in crate::engine) fn sv_spawnserver_detour(
        &self,
    ) -> Result<&TypedDetour<extern "C" fn(*mut c_char, qboolean)>, QuakeLiveEngineError> {
        self.static_detours.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::SV_SpawnServer,
                ))
            },
            |static_detours| Ok(&static_detours.server.sv_spawnserver_detour),
        )
    }

    pub(in crate::engine) fn sv_sendservercommand_detour(
        &self,
    ) -> Result<&RawDetour, QuakeLiveEngineError> {
        self.static_detours.get().map_or_else(
            || {
                cold_path();
                Err(QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::SV_SendServerCommand,
                ))
            },
            |static_detours| Ok(&static_detours.server.sv_sendservercommand_detour),
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
    fn sv_sendservercommand_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_sendservercommand_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SendServerCommand,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_sendservercommand_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_sendservercommand_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_SendServerCommand as unsafe extern "C" fn(*mut client_t, *const c_char, ...)
        )));
    }

    #[test]
    fn sv_executeclientcommand_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_executeclientcommand_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ExecuteClientCommand,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_executeclientcommand_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_executeclientcommand_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_ExecuteClientCommand as extern "C" fn(*mut client_t, *const c_char, qboolean)
        )));
    }

    #[test]
    fn sv_shutdown_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_shutdown_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::SV_Shutdown,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_shutdown_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_shutdown_orig();
        assert!(
            result.is_ok_and(|func| ptr::fn_addr_eq(
                func,
                SV_Shutdown as extern "C" fn(*const c_char)
            ))
        );
    }

    #[test]
    fn sv_map_f_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_map_f_orig();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::SV_Map_f,))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_map_f_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_map_f_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(func, SV_Map_f as extern "C" fn())));
    }

    #[test]
    fn sv_cliententerworld_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_cliententerworld_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_ClientEnterWorld,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_cliententerworld_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_cliententerworld_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_ClientEnterWorld as extern "C" fn(*mut client_t, *mut usercmd_t)
        )));
    }

    #[test]
    fn sv_setconfigstring_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_setconfigstring_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_SetConfigstring,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_setconfigstring_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_setconfigstring_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_SetConfigstring as extern "C" fn(c_int, *const c_char)
        )));
    }

    #[test]
    fn sv_getconfigstring_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_getconfigstring_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(
                QuakeLiveFunction::SV_GetConfigstring,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_getconfigstring_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_getconfigstring_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_GetConfigstring as extern "C" fn(c_int, *mut c_char, c_int)
        )));
    }

    #[test]
    fn sv_dropclient_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_dropclient_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::SV_DropClient,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_dropclient_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_dropclient_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_DropClient as extern "C" fn(*mut client_t, *const c_char)
        )));
    }

    #[test]
    fn sv_spawnserver_orig_when_no_function_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_spawnserver_orig();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticFunctionNotFound(QuakeLiveFunction::SV_SpawnServer,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_spawnserver_orig_when_orig_function_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_spawnserver_orig();
        assert!(result.is_ok_and(|func| ptr::fn_addr_eq(
            func,
            SV_SpawnServer as extern "C" fn(*mut c_char, qboolean)
        )));
    }

    #[test]
    fn sv_executeclientcommand_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_executeclientcommand_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_ExecuteClientCommand,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_executeclientcommand_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_executeclientcommand_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_cliententerworld_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_cliententerworld_detour();
        assert!(
            result.is_err_and(|err| err
                == QuakeLiveEngineError::StaticDetourNotFound(
                    QuakeLiveFunction::SV_ClientEnterWorld,
                ))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_cliententerworld_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_cliententerworld_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_setconfigstring_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_setconfgistring_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::SV_SetConfigstring,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_setconfigstring_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_setconfgistring_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_dropclient_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_dropclient_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::SV_DropClient,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_dropclient_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_dropclient_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_spawnserver_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_spawnserver_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(QuakeLiveFunction::SV_SpawnServer,)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_spawnserver_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_spawnserver_detour();
        assert!(result.is_ok());
    }

    #[test]
    fn sv_sendservercommand_detour_when_no_detour_set() {
        let quake_engine = default_quake_engine();

        let result = quake_engine.sv_sendservercommand_detour();
        assert!(result.is_err_and(|err| err
            == QuakeLiveEngineError::StaticDetourNotFound(
                QuakeLiveFunction::SV_SendServerCommand,
            )));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sv_sendservercommand_detour_when_detour_set() {
        let quake_engine = QuakeLiveEngine {
            static_functions: default_static_functions().into(),
            static_detours: default_static_detours().into(),
            ..default_quake_engine()
        };

        let result = quake_engine.sv_sendservercommand_detour();
        assert!(result.is_ok());
    }
}
