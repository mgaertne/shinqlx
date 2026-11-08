use alloc::borrow::Cow;
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
use core::sync::atomic::{AtomicU64, Ordering};
use core::{
    borrow::BorrowMut,
    ffi::{CStr, VaList, c_char, c_int},
    hint::cold_path,
};
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
use std::{sync::LazyLock, time::Instant};

use tap::{Conv, TapFallible, TapOptional, TryConv};

use crate::{
    MAIN_ENGINE,
    ffi::{c::prelude::*, python::prelude::*},
    prelude::*,
    quake_live_engine::{
        AddCommand, ClientConnect, ClientEnterWorld, ClientSpawn, ComPrintf, ExecuteClientCommand,
        InitGame, RegisterDamage, RunFrame, SendServerCommand, SetConfigstring, SetModuleOffset,
        ShutdownGame, SpawnServer,
    },
};
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
use crate::{
    fixes::query_rate_limiter::{QueryLimits, QueryRateLimiter},
    quake_live_engine::{FindCVar, HandleSteamPacket},
};

fn message_from_buffer(buffer: &[u8]) -> Cow<'_, str> {
    CStr::from_bytes_until_nul(buffer)
        .map_or_else(|_| String::from_utf8_lossy(buffer), CStr::to_string_lossy)
}

pub(crate) extern "C" fn shinqlx_cmd_addcommand(cmd: *const c_char, func: unsafe extern "C" fn()) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        if !main_engine.is_common_initialized() {
            let _ = main_engine.initialize_static().tap_err(|err| {
                cold_path();
                error!(target: "shinqlx", "{err:?}");
                error!(target: "shinqlx", "Static initialization failed. Exiting.");
                panic!("Static initialization failed. Exiting.");
            });
        }

        // SAFETY: the engine passes a valid, NUL-terminated C string.
        let command = unsafe { CStr::from_ptr(cmd) }.to_string_lossy();
        if !command.is_empty() {
            main_engine.add_command(&command, func);
        }
    });
}

pub(crate) extern "C" fn shinqlx_sys_setmoduleoffset(
    module_name: *mut c_char,
    offset: unsafe extern "C" fn(),
) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        // SAFETY: the engine passes a valid, NUL-terminated C string.
        let converted_module_name = unsafe { CStr::from_ptr(module_name) }.to_string_lossy();

        // We should be getting qagame, but check just in case.
        if converted_module_name.as_ref() != "qagame" {
            cold_path();
            error!(target: "shinqlx", "Unknown module: {converted_module_name}");
        }

        main_engine.set_module_offset(&converted_module_name, offset);

        let _ = main_engine.initialize_vm(offset as usize).tap_err(|err| {
            error!(target: "shinqlx", "{err:?}");
            error!(target: "shinqlx", "VM could not be initializied. Exiting.");
            panic!("VM could not be initializied. Exiting.");
        });
    });
}

pub(crate) fn shinqlx_g_initgame(level_time: c_int, random_seed: c_int, restart: c_int) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        main_engine.init_game(level_time, random_seed, restart);

        main_engine.set_tag();
        main_engine.initialize_cvars();

        if restart != 0 {
            new_game_dispatcher(true);
        }
    });
}

pub(crate) fn shinqlx_g_shutdowngame(restart: c_int) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        main_engine.unhook_vm(restart != 0);
        main_engine.shutdown_game(restart);
    });
}

pub(crate) extern "C" fn shinqlx_sv_executeclientcommand(
    client: *mut client_t,
    cmd: *const c_char,
    client_ok: qboolean,
) {
    // SAFETY: the engine passes a valid, NUL-terminated C string.
    let rust_cmd = unsafe { CStr::from_ptr(cmd) }.to_string_lossy();
    shinqlx_execute_client_command(Client::try_from(client).ok(), rust_cmd, client_ok);
}

pub(crate) fn shinqlx_execute_client_command<T, U>(client: Option<Client>, cmd: T, client_ok: U)
where
    T: AsRef<str>,
    U: Into<qboolean> + Into<bool> + Copy,
{
    if MAIN_ENGINE.load().is_none() {
        cold_path();
        return;
    }
    if cmd.as_ref().is_empty() {
        return;
    }
    let client_ok_flag: bool = client_ok.into();
    let dispatching_client_id = client
        .as_ref()
        .filter(|safe_client| client_ok_flag && safe_client.has_gentity())
        .map(|safe_client| safe_client.get_client_id());
    let passed_on_cmd_str: Cow<'_, str> = match dispatching_client_id {
        Some(client_id) => {
            let Some(dispatcher_result) = client_command_dispatcher(client_id, cmd.as_ref()) else {
                return;
            };
            dispatcher_result
        }
        None => Cow::Borrowed(cmd.as_ref()),
    };

    if !passed_on_cmd_str.is_empty() {
        MAIN_ENGINE.load().as_ref().tap_some(|main_engine| {
            main_engine.execute_client_command(
                client,
                passed_on_cmd_str.as_ref(),
                client_ok.conv::<qboolean>(),
            )
        });
    }
}

#[unsafe(no_mangle)]
#[rustversion::before(1.94)]
pub unsafe extern "C" fn ShiNQlx_SV_SendServerCommand(
    client: *mut client_t,
    fmt: *const c_char,
    mut fmt_args: ...
) {
    unsafe extern "C" {
        fn vsnprintf(s: *mut c_char, n: usize, format: *const c_char, arg: VaList) -> c_int;
    }

    let mut buffer: [u8; MAX_MSGLEN as usize] = [0; MAX_MSGLEN as usize];
    let result = unsafe {
        vsnprintf(
            buffer.as_mut_ptr() as *mut c_char,
            buffer.len(),
            fmt,
            fmt_args.as_va_list(),
        )
    };
    if result < 0 {
        cold_path();
        warn!(target: "shinqlx", "some formatting problem occurred");
    }

    let cmd = message_from_buffer(&buffer);
    if client.is_null() {
        shinqlx_send_server_command(None, cmd);
    } else {
        let _ = client.try_conv::<Client>().map(|safe_client| {
            shinqlx_send_server_command(Some(safe_client), cmd);
        });
    }
}

#[unsafe(no_mangle)]
#[rustversion::since(1.94)]
pub unsafe extern "C" fn ShiNQlx_SV_SendServerCommand(
    client: *mut client_t,
    fmt: *const c_char,
    fmt_args: ...
) {
    unsafe extern "C" {
        fn vsnprintf(s: *mut c_char, n: usize, format: *const c_char, arg: VaList) -> c_int;
    }

    let mut buffer: [u8; MAX_MSGLEN as usize] = [0; MAX_MSGLEN as usize];
    // SAFETY: the buffer is writable for its length, the format and arguments come from the engine's variadic call.
    let result = unsafe {
        vsnprintf(
            buffer.as_mut_ptr() as *mut c_char,
            buffer.len(),
            fmt,
            fmt_args,
        )
    };
    if result < 0 {
        cold_path();
        warn!(target: "shinqlx", "some formatting problem occurred");
    }

    let cmd = message_from_buffer(&buffer);
    if client.is_null() {
        shinqlx_send_server_command(None, cmd);
    } else {
        let _ = client.try_conv::<Client>().map(|safe_client| {
            shinqlx_send_server_command(Some(safe_client), cmd);
        });
    }
}

pub(crate) fn shinqlx_send_server_command<T>(client: Option<Client>, cmd: T)
where
    T: AsRef<str>,
{
    if MAIN_ENGINE.load().is_none() {
        cold_path();
        return;
    }

    if cmd.as_ref().is_empty() {
        return;
    }
    let mut passed_on_cmd_str = Cow::Borrowed(cmd.as_ref());

    match client.as_ref() {
        Some(safe_client) if safe_client.has_gentity() => {
            let client_id = safe_client.get_client_id();
            let Some(res) = server_command_dispatcher(Some(client_id), cmd.as_ref()) else {
                return;
            };
            passed_on_cmd_str = res;
        }
        None => {
            let Some(res) = server_command_dispatcher(None, cmd.as_ref()) else {
                return;
            };
            passed_on_cmd_str = res;
        }
        _ => (),
    }

    if !passed_on_cmd_str.is_empty() {
        MAIN_ENGINE
            .load()
            .as_ref()
            .tap_some(|main_engine| main_engine.send_server_command(client, &passed_on_cmd_str));
    }
}

pub(crate) extern "C" fn shinqlx_sv_cliententerworld(client: *mut client_t, cmd: *mut usercmd_t) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        client
            .try_conv::<Client>()
            .ok()
            .tap_some_mut(|safe_client| {
                let state = safe_client.get_state();

                main_engine.client_enter_world(safe_client.borrow_mut(), cmd);

                // gentity is NULL if map changed.
                // state is CS_PRIMED only if it's the first time they connect to the server,
                // otherwise the dispatcher would also go off when a game starts and such.
                if safe_client.has_gentity() && state == clientState_t::CS_PRIMED {
                    client_loaded_dispatcher(safe_client.get_client_id());
                }
            });
    });
}

pub(crate) extern "C" fn shinqlx_sv_setconfigstring(index: c_int, value: *const c_char) {
    let safe_value = if !value.is_null() {
        // SAFETY: the engine passes a valid, NUL-terminated C string.
        unsafe { CStr::from_ptr(value) }.to_string_lossy()
    } else {
        "".into()
    };

    let Ok(ql_index) = u32::try_from(index) else {
        cold_path();
        return;
    };

    shinqlx_set_configstring(ql_index, &safe_value);
}

pub(crate) fn shinqlx_set_configstring<T>(index: T, value: &str)
where
    T: TryInto<c_int> + Into<u32> + Copy,
{
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        match index.try_conv::<c_int>() {
            // Indices 16 and 66X are spammed a ton every frame for some reason,
            // so we add some exceptions for those. I don't think we should have any
            // use for those particular ones anyway. If we don't do this, we get
            // like a 25% increase in CPU usage on an empty server.
            Ok(c_index) if c_index == 16 || (662..670).contains(&c_index) => {
                main_engine.set_configstring(c_index, value.as_ref());
            }
            Ok(c_index) => {
                set_configstring_dispatcher(index.conv::<u32>(), value).tap_some(|res| {
                    main_engine.set_configstring(c_index, res);
                });
            }
            _ => (),
        }
    });
}

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
static QUERY_LIMITER: parking_lot::Mutex<QueryRateLimiter> =
    parking_lot::Mutex::new(QueryRateLimiter::new(QueryLimits::DEFAULT));
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
static QUERY_CLOCK: LazyLock<Instant> = LazyLock::new(Instant::now);
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
static QUERY_LIMITS_REFRESHED_MS: AtomicU64 = AtomicU64::new(0);

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
const QUERY_LIMITS_REFRESH_INTERVAL_MS: u64 = 5_000;

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
const STEAM_QUERY_HEADER: [u8; 4] = [0xFF; 4];
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
const STEAM_QUERY_OPCODES: &[u8; 3] = b"TUV";

/// The message buffer as the engine hands it to Steam; it has no `oob` field between the two
/// flags and `data`, unlike `msg_t`.
#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
#[repr(C)]
struct SteamPacketBuffer {
    allowoverflow: c_int,
    overflowed: c_int,
    data: *const u8,
    maxsize: c_int,
    cursize: c_int,
}

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
fn is_steam_query(msg: *const msg_t) -> bool {
    // SAFETY: a non-null pointer from the engine points to a valid structure, null yields None.
    let Some(msg) = (unsafe { msg.cast::<SteamPacketBuffer>().as_ref() }) else {
        return false;
    };
    let Ok(size) = usize::try_from(msg.cursize) else {
        return false;
    };
    let fits_buffer = msg.cursize <= msg.maxsize;
    if msg.data.is_null() || !fits_buffer || size <= STEAM_QUERY_HEADER.len() {
        return false;
    }

    // SAFETY: data was checked as non-null and cursize as non-negative and within maxsize.
    let packet = unsafe { core::slice::from_raw_parts(msg.data, size) };
    packet.starts_with(&STEAM_QUERY_HEADER)
        && packet
            .get(STEAM_QUERY_HEADER.len())
            .is_some_and(|opcode| STEAM_QUERY_OPCODES.contains(opcode))
}

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
fn query_limits_from_cvars<T>(main_engine: &T) -> QueryLimits
where
    T: FindCVar<&'static str>,
{
    let read = |name: &'static str, default: f64| {
        main_engine
            .find_cvar(name)
            .and_then(|cvar| cvar.get_string().trim().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(default)
    };
    QueryLimits {
        per_source_rate: read("qlx_steamQueryRate", QueryLimits::DEFAULT.per_source_rate),
        burst: read("qlx_steamQueryBurst", QueryLimits::DEFAULT.burst),
        global_rate: read("qlx_steamQueryGlobalRate", QueryLimits::DEFAULT.global_rate),
    }
}

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
fn steam_query_allowed<T>(main_engine: &T, from: &netadr_t, now_ms: u64) -> bool
where
    T: FindCVar<&'static str>,
{
    if from.type_ != netadrtype_t::NA_IP {
        return true;
    }

    let mut limiter = QUERY_LIMITER.lock();
    let refreshed_ms = QUERY_LIMITS_REFRESHED_MS.load(Ordering::Acquire);
    if refreshed_ms == 0 || now_ms.saturating_sub(refreshed_ms) >= QUERY_LIMITS_REFRESH_INTERVAL_MS
    {
        limiter.set_limits(query_limits_from_cvars(main_engine));
        QUERY_LIMITS_REFRESHED_MS.store(now_ms.max(1), Ordering::Release);
    }

    let allowed = limiter.allow(from.ip, now_ms);
    if let Some(dropped) = limiter.take_dropped_summary(now_ms) {
        warn!(
            target: "shinqlx",
            "Dropped {dropped} Steam query packets that exceeded the rate limit."
        );
    }
    allowed
}

#[cfg(any(all(target_os = "linux", feature = "steam-query-limit"), test))]
/// Hook of the engine function that hands connectionless packets to the Steam game server API.
///
/// Steam answers server queries with packets larger than the request, which makes the server a
/// reflector for spoofed sources. Requests above the rate limit are reported as handled without
/// ever reaching Steam.
pub(crate) extern "C" fn shinqlx_sv_handlesteampacket(
    from: *const netadr_t,
    msg: *const msg_t,
) -> c_int {
    MAIN_ENGINE.load().as_ref().map_or(0, |main_engine| {
        let now_ms = u64::try_from(QUERY_CLOCK.elapsed().as_millis()).unwrap_or(u64::MAX);
        // SAFETY: a non-null pointer from the engine points to a valid structure, null yields None.
        match unsafe { from.as_ref() } {
            Some(address)
                if is_steam_query(msg) && !steam_query_allowed(&**main_engine, address, now_ms) =>
            {
                1
            }
            _ => c_int::from(main_engine.handle_steam_packet(from, msg)),
        }
    })
}

pub(crate) extern "C" fn shinqlx_sv_dropclient(client: *mut client_t, reason: *const c_char) {
    let _ = Client::try_from(client).tap_ok_mut(|safe_client| {
        shinqlx_drop_client(
            safe_client,
            // SAFETY: the engine passes a valid, NUL-terminated C string.
            unsafe { CStr::from_ptr(reason) }.to_string_lossy(),
        );
    });
}

pub(crate) fn shinqlx_drop_client<T>(client: &mut Client, reason: T)
where
    T: AsRef<str>,
{
    client_disconnect_dispatcher(client.get_client_id(), reason.as_ref());

    client.disconnect(reason.as_ref());
}

#[unsafe(no_mangle)]
#[rustversion::before(1.94)]
pub unsafe extern "C" fn ShiNQlx_Com_Printf(fmt: *const c_char, mut fmt_args: ...) {
    unsafe extern "C" {
        fn vsnprintf(s: *mut c_char, n: usize, format: *const c_char, arg: VaList) -> c_int;
    }

    let mut buffer: [u8; MAX_MSGLEN as usize] = [0; MAX_MSGLEN as usize];
    let result = unsafe {
        vsnprintf(
            buffer.as_mut_ptr() as *mut c_char,
            buffer.len(),
            fmt,
            fmt_args.as_va_list(),
        )
    };
    if result < 0 {
        cold_path();
        warn!(target: "shinqlx", "some formatting problem occurred");
    }

    let rust_msg = message_from_buffer(&buffer);
    if !rust_msg.is_empty() {
        shinqlx_com_printf(rust_msg);
    }
}

#[unsafe(no_mangle)]
#[rustversion::since(1.94)]
pub unsafe extern "C" fn ShiNQlx_Com_Printf(fmt: *const c_char, fmt_args: ...) {
    unsafe extern "C" {
        fn vsnprintf(s: *mut c_char, n: usize, format: *const c_char, arg: VaList) -> c_int;
    }

    let mut buffer: [u8; MAX_MSGLEN as usize] = [0; MAX_MSGLEN as usize];
    // SAFETY: the buffer is writable for its length, the format and arguments come from the engine's variadic call.
    let result = unsafe {
        vsnprintf(
            buffer.as_mut_ptr() as *mut c_char,
            buffer.len(),
            fmt,
            fmt_args,
        )
    };
    if result < 0 {
        cold_path();
        warn!(target: "shinqlx", "some formatting problem occurred");
    }

    let rust_msg = message_from_buffer(&buffer);
    if !rust_msg.is_empty() {
        shinqlx_com_printf(rust_msg);
    }
}

pub(crate) fn shinqlx_com_printf<T>(msg: T)
where
    T: AsRef<str>,
{
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        let Some(_) = console_print_dispatcher(msg.as_ref()) else {
            return;
        };

        main_engine.com_printf(msg.as_ref());
    });
}

pub(crate) extern "C" fn shinqlx_sv_spawnserver(server: *mut c_char, kill_bots: qboolean) {
    // SAFETY: the engine passes a valid, NUL-terminated C string.
    let server_str = unsafe { CStr::from_ptr(server) }.to_string_lossy();
    if server_str.is_empty() {
        return;
    }

    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        main_engine.spawn_server(server_str.as_ref(), kill_bots.conv::<bool>());

        new_game_dispatcher(false);
    });
}

pub(crate) fn shinqlx_g_runframe(time: c_int) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        frame_dispatcher();

        main_engine.run_frame(time);
    });
}

static CLIENT_CONNECT_BUFFER: [parking_lot::RwLock<
    arrayvec::ArrayVec<c_char, { MAX_STRING_CHARS as usize }>,
>; MAX_CLIENTS as usize] =
    [const { parking_lot::RwLock::new(arrayvec::ArrayVec::new_const()) }; MAX_CLIENTS as usize];

fn to_return_string(client_id: i32, input: String) -> *const c_char {
    CLIENT_CONNECT_BUFFER[client_id as usize]
        .try_write()
        .tap_some_mut(|buffer_write_guard| {
            **buffer_write_guard = input
                .as_bytes()
                .iter()
                .map(|&char| char as c_char)
                .collect();
            buffer_write_guard.push(0);
        });
    CLIENT_CONNECT_BUFFER[client_id as usize].read().as_ptr()
}

pub(crate) extern "C" fn shinqlx_client_connect(
    client_num: c_int,
    first_time: qboolean,
    is_bot: qboolean,
) -> *const c_char {
    if first_time.conv::<bool>()
        && let Some(res) = client_connect_dispatcher(client_num, is_bot.conv::<bool>())
        && !is_bot.conv::<bool>()
    {
        return to_return_string(client_num, res);
    }

    MAIN_ENGINE
        .load()
        .as_ref()
        .map_or(ptr::null_mut(), |main_engine| {
            main_engine.client_connect(client_num, first_time.conv::<bool>(), is_bot.conv::<bool>())
        })
}

pub(crate) extern "C" fn shinqlx_clientspawn(ent: *mut gentity_t) {
    let _ = GameEntity::try_from(ent).tap_ok_mut(|game_entity| {
        shinqlx_client_spawn(game_entity);
    });
}

pub(crate) fn shinqlx_client_spawn(game_entity: &mut GameEntity) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        main_engine.client_spawn(game_entity.borrow_mut());

        // Since we won't ever stop the real function from being called,
        // we trigger the event after calling the real one. This will allow
        // us to set weapons and such without it getting overriden later.
        client_spawn_dispatcher(game_entity.get_entity_id());
    });
}

pub(crate) extern "C" fn shinqlx_g_startkamikaze(ent: *mut gentity_t) {
    let Some(mut game_entity): Option<GameEntity> = GameEntity::try_from(ent).ok() else {
        cold_path();
        return;
    };

    let client_id = match game_entity.get_game_client() {
        Ok(game_client) => game_client.get_client_num(),
        _ => match game_entity.get_activator() {
            Ok(activator) => activator.get_owner_num(),
            _ => -1,
        },
    };

    let _ = game_entity.get_game_client().tap_ok_mut(|game_client| {
        game_client.remove_kamikaze_flag();
        kamikaze_use_dispatcher(client_id);
    });
    game_entity.start_kamikaze();

    if client_id == -1 {
        return;
    }

    kamikaze_explode_dispatcher(client_id, game_entity.get_game_client().is_ok())
}

pub(crate) extern "C" fn shinqlx_g_damage(
    target: *mut gentity_t,
    inflictor: *mut gentity_t,
    attacker: *mut gentity_t, // entity that caused the inflictor to damage target
    dir: *mut vec3_t,         // direction of the attack for knockback
    pos: *mut vec3_t,         // point at which the damage is inflicted, used for headshots
    damage: c_int,
    dflags: c_int, // DAMAGE_* flags controlling how T_Damage works
    means_of_death: c_int,
) {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        main_engine.register_damage(
            target,
            inflictor,
            attacker,
            dir,
            pos,
            damage,
            dflags,
            means_of_death,
        );

        let _ = GameEntity::try_from(target).tap_ok(|target_entity| match attacker.is_null() {
            true => {
                damage_dispatcher(
                    target_entity.get_entity_id(),
                    None,
                    damage,
                    dflags,
                    means_of_death,
                );
            }
            false => match GameEntity::try_from(attacker) {
                Err(_) => {
                    damage_dispatcher(
                        target_entity.get_entity_id(),
                        None,
                        damage,
                        dflags,
                        means_of_death,
                    );
                }
                Ok(attacker_entity) => {
                    damage_dispatcher(
                        target_entity.get_entity_id(),
                        Some(attacker_entity.get_entity_id()),
                        damage,
                        dflags,
                        means_of_death,
                    );
                }
            },
        });
    });
}

#[cfg(test)]
#[mockall::automock]
#[allow(
    dead_code,
    clippy::module_inception,
    reason = "the mock module is named like the module it replaces"
)]
pub(crate) mod hooks {
    use super::{Client, GameEntity};

    #[cfg(not(tarpaulin_include))]
    pub(crate) fn shinqlx_execute_client_command(
        _client: Option<Client>,
        _cmd: &str,
        _client_ok: bool,
    ) {
    }
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn shinqlx_send_server_command(_client: Option<Client>, _cmd: &str) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn shinqlx_drop_client(_client: &mut Client, _reason: &str) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn shinqlx_client_spawn(_game_entity: &mut GameEntity) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn shinqlx_set_configstring(_index: u32, _value: &str) {}
    #[cfg(not(tarpaulin_include))]
    pub(crate) fn shinqlx_com_printf(_msg: &str) {}
}

#[cfg(test)]
mod hooks_tests {
    use core::{
        borrow::BorrowMut,
        ffi::{CStr, c_int},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use rstest::*;
    use tap::Conv;

    use super::{
        QUERY_LIMITER, QUERY_LIMITS_REFRESHED_MS, ShiNQlx_Com_Printf, ShiNQlx_SV_SendServerCommand,
        SteamPacketBuffer, shinqlx_client_connect, shinqlx_client_spawn, shinqlx_clientspawn,
        shinqlx_cmd_addcommand, shinqlx_com_printf, shinqlx_drop_client,
        shinqlx_execute_client_command, shinqlx_g_damage, shinqlx_g_initgame, shinqlx_g_runframe,
        shinqlx_g_shutdowngame, shinqlx_g_startkamikaze, shinqlx_send_server_command,
        shinqlx_set_configstring, shinqlx_sv_cliententerworld, shinqlx_sv_dropclient,
        shinqlx_sv_executeclientcommand, shinqlx_sv_handlesteampacket, shinqlx_sv_setconfigstring,
        shinqlx_sv_spawnserver, shinqlx_sys_setmoduleoffset, steam_query_allowed,
    };
    use crate::{
        MAIN_ENGINE,
        ffi::{
            c::prelude::*,
            python::{
                mock_python_tests::{
                    __client_command_dispatcher, __client_connect_dispatcher,
                    __client_loaded_dispatcher, __damage_dispatcher, __new_game_dispatcher,
                    __server_command_dispatcher, __set_configstring_dispatcher,
                },
                prelude::*,
            },
        },
        fixes::query_rate_limiter::{QueryLimits, QueryRateLimiter},
        prelude::*,
    };

    #[fixture]
    fn new_game_dispatcher_ctx() -> __new_game_dispatcher::Context {
        new_game_dispatcher_context()
    }

    #[fixture]
    fn client_command_dispatcher_ctx() -> __client_command_dispatcher::Context {
        client_command_dispatcher_context()
    }

    #[fixture]
    fn server_command_dispatcher_ctx() -> __server_command_dispatcher::Context {
        server_command_dispatcher_context()
    }

    #[fixture]
    fn client_loaded_dispatcher_ctx() -> __client_loaded_dispatcher::Context {
        client_loaded_dispatcher_context()
    }

    #[fixture]
    fn set_configstring_dispatcher_ctx() -> __set_configstring_dispatcher::Context {
        set_configstring_dispatcher_context()
    }

    #[fixture]
    fn client_connect_dispatcher_ctx() -> __client_connect_dispatcher::Context {
        client_connect_dispatcher_context()
    }

    #[fixture]
    fn damage_dispatcher_ctx() -> __damage_dispatcher::Context {
        damage_dispatcher_context()
    }

    static DUMMY_FUNCTION_CALLS: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn dummy_function() {
        DUMMY_FUNCTION_CALLS.fetch_add(1, Ordering::SeqCst);
    }

    fn is_dummy_address(address: usize) -> bool {
        // SAFETY: the address belongs to a function with this signature.
        let function = unsafe {
            core::mem::transmute::<*const (), unsafe extern "C" fn()>(
                core::ptr::with_exposed_provenance(address),
            )
        };
        is_dummy_function(function)
    }

    fn is_dummy_function(function: unsafe extern "C" fn()) -> bool {
        let calls_before = DUMMY_FUNCTION_CALLS.load(Ordering::SeqCst);
        // SAFETY: the function is a dummy without arguments that only counts its calls.
        unsafe { function() };
        DUMMY_FUNCTION_CALLS.load(Ordering::SeqCst) == calls_before + 1
    }

    #[test]
    #[serial]
    fn add_command_with_no_main_engine() {
        let cmd_string = c"";
        shinqlx_cmd_addcommand(cmd_string.as_ptr(), dummy_function);
    }

    #[test]
    #[serial]
    fn add_command_with_main_engine_already_initiailized_command_empty() {
        let cmd_string = c"";
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_is_common_initialized()
                    .return_const(true);
                mock_engine.expect_add_command().times(0);
            })
            .run(|| {
                shinqlx_cmd_addcommand(cmd_string.as_ptr(), dummy_function);
            });
    }

    #[test]
    #[serial]
    fn add_command_with_main_engine_already_initialized() {
        let cmd_string = c"slap";
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_is_common_initialized()
                    .return_const(true);
                mock_engine
                    .expect_add_command()
                    .withf(|cmd, &func| cmd == "slap" && is_dummy_function(func))
                    .times(1);
            })
            .run(|| {
                shinqlx_cmd_addcommand(cmd_string.as_ptr(), dummy_function);
            });
    }

    #[test]
    #[serial]
    fn add_command_with_main_engine_not_initiailized_command_non_empty() {
        let cmd_string = c"slap";
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_is_common_initialized()
                    .return_const(false);
                mock_engine
                    .expect_initialize_static()
                    .returning(|| Ok(()))
                    .times(1);
                mock_engine
                    .expect_add_command()
                    .withf(|cmd, &func| cmd == "slap" && is_dummy_function(func))
                    .times(1);
            })
            .run(|| {
                shinqlx_cmd_addcommand(cmd_string.as_ptr(), dummy_function);
            });
    }

    #[test]
    #[should_panic]
    #[ignore]
    #[serial]
    fn add_command_with_main_engine_already_initiailized_init_returns_err() {
        let cmd_string = c"slap";
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_is_common_initialized()
                    .return_const(false);
                mock_engine
                    .expect_initialize_static()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
                    .times(1);
            })
            .run(|| {
                shinqlx_cmd_addcommand(cmd_string.as_ptr(), dummy_function);
            });
    }

    #[test]
    #[serial]
    fn sys_setmoduleoffset_no_main_engine() {
        let module_string = c"qagame";
        shinqlx_sys_setmoduleoffset(module_string.as_ptr().cast_mut(), dummy_function);
    }

    #[test]
    #[serial]
    fn sys_setmoduleoffset_vm_init_ok() {
        let module_string = c"qagame";
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_module_offset()
                    .withf(|module_name, &offset| {
                        module_name == "qagame" && is_dummy_function(offset)
                    })
                    .times(1);
                mock_engine
                    .expect_initialize_vm()
                    .withf(|&offset| is_dummy_address(offset))
                    .returning(|_offset| Ok(()))
                    .times(1);
            })
            .run(|| {
                shinqlx_sys_setmoduleoffset(module_string.as_ptr().cast_mut(), dummy_function);
            });
    }

    #[test]
    #[ignore]
    #[should_panic]
    #[serial]
    fn sys_setmoduleoffset_vm_init_returns_err() {
        let module_string = c"qagame";
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_module_offset()
                    .withf_st(|module_name, &offset| {
                        module_name == "qagame" && is_dummy_function(offset)
                    })
                    .times(1);
                mock_engine
                    .expect_initialize_vm()
                    .withf(|&func| is_dummy_address(func))
                    .returning(|_offset| Err(QuakeLiveEngineError::MainEngineNotInitialized))
                    .times(1);
            })
            .run(|| {
                shinqlx_sys_setmoduleoffset(module_string.as_ptr().cast_mut(), dummy_function);
            });
    }

    #[test]
    #[serial]
    fn init_game_with_no_main_engine() {
        shinqlx_g_initgame(42, 21, 0);
    }

    #[rstest]
    #[serial]
    fn init_game_without_restart(new_game_dispatcher_ctx: __new_game_dispatcher::Context) {
        new_game_dispatcher_ctx.expect().times(0);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_init_game()
                    .with(predicate::eq(42), predicate::eq(21), predicate::eq(0))
                    .times(1);
                mock_engine.expect_set_tag().times(1);
                mock_engine.expect_initialize_cvars().times(1);
            })
            .run(|| {
                shinqlx_g_initgame(42, 21, 0);
            });
    }

    #[rstest]
    #[serial]
    fn init_game_with_restart(new_game_dispatcher_ctx: __new_game_dispatcher::Context) {
        new_game_dispatcher_ctx
            .expect()
            .with(predicate::eq(true))
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_init_game()
                    .with(predicate::eq(42), predicate::eq(21), predicate::eq(1))
                    .times(1);
                mock_engine.expect_set_tag().times(1);
                mock_engine.expect_initialize_cvars().times(1);
            })
            .run(|| {
                shinqlx_g_initgame(42, 21, 1);
            });
    }

    #[test]
    #[serial]
    fn shut_down_game_with_no_main_engine() {
        shinqlx_g_shutdowngame(42);
    }

    #[test]
    #[serial]
    fn shut_down_game_unhooks_vm() {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_unhook_vm().times(1);
                mock_engine
                    .expect_shutdown_game()
                    .with(predicate::eq(42))
                    .times(1);
            })
            .run(|| {
                shinqlx_g_shutdowngame(42);
            });
    }

    #[test]
    #[serial]
    fn execute_client_command_with_no_main_engine() {
        shinqlx_execute_client_command(None, "cp asdf", true);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_none_client_non_empty_cmd() {
        MockEngineBuilder::default()
            .with_execute_client_command(
                |client, cmd, &client_ok| client.is_none() && cmd == "cp asdf" && client_ok.into(),
                1,
            )
            .run(|| {
                shinqlx_execute_client_command(None, "cp asdf", true);
            });
    }

    #[test]
    #[serial]
    fn execute_client_command_for_not_ok_client_non_empty_cmd() {
        let mock_client = MockClient::new();

        MockEngineBuilder::default()
            .with_execute_client_command(
                |client, cmd, &client_ok| {
                    client.is_some() && cmd == "cp asdf" && !client_ok.conv::<bool>()
                },
                1,
            )
            .run(|| {
                shinqlx_execute_client_command(Some(mock_client), "cp asdf", false);
            });
    }

    #[test]
    #[serial]
    fn execute_client_command_for_ok_client_without_gentity_non_empty_cmd() {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const(false)
            .times(1);

        MockEngineBuilder::default()
            .with_execute_client_command(
                |client, cmd, &client_ok| client.is_some() && cmd == "cp asdf" && client_ok.into(),
                1,
            )
            .run(|| {
                shinqlx_execute_client_command(Some(mock_client), "cp asdf", true);
            });
    }

    #[rstest]
    #[serial]
    fn execute_client_command_for_ok_client_with_gentity_non_empty_cmd_dispatcher_returns_none(
        client_command_dispatcher_ctx: __client_command_dispatcher::Context,
    ) {
        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);
        client_command_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq("cp asdf".to_string()))
            .times(1);

        MockEngineBuilder::default()
            .with_execute_client_command(|_client, _cmd, _client_ok| true, 0)
            .run(|| {
                shinqlx_execute_client_command(Some(mock_client), "cp asdf", true);
            });
    }

    #[rstest]
    #[serial]
    fn execute_client_command_for_ok_client_with_gentity_non_empty_cmd_dispatcher_returns_modified_string(
        client_command_dispatcher_ctx: __client_command_dispatcher::Context,
    ) {
        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);

        client_command_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq("cp asdf"))
            .return_const(Some("cp modified".into()))
            .times(1);

        MockEngineBuilder::default()
            .with_execute_client_command(
                |client, cmd, &client_ok| {
                    client.is_some() && cmd == "cp modified" && client_ok.into()
                },
                1,
            )
            .run(|| {
                shinqlx_execute_client_command(Some(mock_client), "cp asdf", true);
            });
    }

    #[rstest]
    #[serial]
    fn execute_client_command_for_ok_client_with_gentity_non_empty_cmd_dispatcher_returns_empty_string(
        client_command_dispatcher_ctx: __client_command_dispatcher::Context,
    ) {
        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);

        client_command_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq("cp asdf"))
            .return_const(Some("".into()))
            .times(1);

        MockEngineBuilder::default()
            .with_execute_client_command(|_client, _cmd, _client_ok| true, 0)
            .run(|| {
                shinqlx_execute_client_command(Some(mock_client), "cp asdf", true);
            });
    }

    #[test]
    #[serial]
    fn send_server_command_with_no_main_engine() {
        shinqlx_send_server_command(None, "cp asdf");
    }

    #[rstest]
    #[serial]
    fn send_server_command_for_none_client_non_empty_cmd_dispatcher_returns_none(
        server_command_dispatcher_ctx: __server_command_dispatcher::Context,
    ) {
        server_command_dispatcher_ctx
            .expect()
            .with(predicate::eq(None), predicate::eq("cp asdf".to_string()))
            .return_const(None)
            .times(1);

        MockEngineBuilder::default()
            .with_send_server_command(|_client, _cmd| true, 0)
            .run(|| {
                shinqlx_send_server_command(None, "cp asdf");
            });
    }

    #[rstest]
    #[serial]
    fn send_server_command_for_none_client_non_empty_cmd_dispatcher_returns_modified_cmd(
        server_command_dispatcher_ctx: __server_command_dispatcher::Context,
    ) {
        server_command_dispatcher_ctx
            .expect()
            .with(predicate::eq(None), predicate::eq("cp asdf"))
            .return_const(Some("cp modified".into()))
            .times(1);

        MockEngineBuilder::default()
            .with_send_server_command(|client, cmd| client.is_none() && cmd == "cp modified", 1)
            .run(|| {
                shinqlx_send_server_command(None, "cp asdf");
            });
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_without_gentity_non_empty_cmd() {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const(false)
            .times(1);

        MockEngineBuilder::default()
            .with_send_server_command(|client, cmd| client.is_some() && cmd == "cp asdf", 1)
            .run(|| {
                shinqlx_send_server_command(Some(mock_client), "cp asdf");
            });
    }

    #[rstest]
    #[serial]
    fn send_server_command_for_client_with_gentity_non_empty_cmd_dispatcher_returns_none(
        server_command_dispatcher_ctx: __server_command_dispatcher::Context,
    ) {
        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);

        server_command_dispatcher_ctx
            .expect()
            .with(
                predicate::eq(Some(42)),
                predicate::eq("cp asdf".to_string()),
            )
            .return_const(None)
            .times(1);

        MockEngineBuilder::default()
            .with_send_server_command(|_client, _cmd| true, 0)
            .run(|| {
                shinqlx_send_server_command(Some(mock_client), "cp asdf");
            });
    }

    #[rstest]
    #[serial]
    fn send_server_command_for_client_with_gentity_non_empty_cmd_dispatcher_returns_modified_string(
        server_command_dispatcher_ctx: __server_command_dispatcher::Context,
    ) {
        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);

        server_command_dispatcher_ctx
            .expect()
            .with(predicate::eq(Some(42)), predicate::eq("cp asdf"))
            .return_const(Some("cp modified".into()))
            .times(1);

        MockEngineBuilder::default()
            .with_send_server_command(|client, cmd| client.is_some() && cmd == "cp modified", 1)
            .run(|| {
                shinqlx_send_server_command(Some(mock_client), "cp asdf");
            });
    }

    #[rstest]
    #[serial]
    fn send_server_command_for_client_with_gentity_non_empty_cmd_dispatcher_returns_empty_string(
        server_command_dispatcher_ctx: __server_command_dispatcher::Context,
    ) {
        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);

        server_command_dispatcher_ctx
            .expect()
            .with(predicate::eq(Some(42)), predicate::eq("cp asdf"))
            .return_const(Some("".into()))
            .times(1);

        MockEngineBuilder::default()
            .with_send_server_command(|_client, _cmd| true, 0)
            .run(|| {
                shinqlx_send_server_command(Some(mock_client), "cp asdf");
            });
    }

    #[test]
    #[serial]
    fn client_enter_world_with_no_main_engine() {
        let mock_client = MockClient::new();

        let client_try_from_ctx = MockClient::try_from_context();
        client_try_from_ctx
            .expect()
            .return_once_st(|_| Ok(mock_client));

        let mut usercmd = UserCmdBuilder::default()
            .build()
            .expect("this should not happen");

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");

        shinqlx_sv_cliententerworld(client.borrow_mut(), usercmd.borrow_mut() as *mut usercmd_t);
    }

    #[rstest]
    #[serial]
    fn client_enter_world_for_unprimed_client(
        client_loaded_dispatcher_ctx: __client_loaded_dispatcher::Context,
    ) {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_ZOMBIE)
            .times(1);
        mock_client.expect_has_gentity().return_const(true).times(1);

        let client_try_from_ctx = MockClient::try_from_context();
        client_try_from_ctx
            .expect()
            .return_once_st(|_| Ok(mock_client));

        let mut usercmd = UserCmdBuilder::default()
            .build()
            .expect("this should not happen");

        client_loaded_dispatcher_ctx.expect().times(0);

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_client_enter_world().times(1);
            })
            .run(|| {
                shinqlx_sv_cliententerworld(
                    client.borrow_mut(),
                    usercmd.borrow_mut() as *mut usercmd_t,
                );
            });
    }

    #[rstest]
    #[serial]
    fn client_enter_world_for_primed_client_without_gentity(
        client_loaded_dispatcher_ctx: __client_loaded_dispatcher::Context,
    ) {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_PRIMED)
            .times(1);
        mock_client
            .expect_has_gentity()
            .return_const(false)
            .times(1);

        let client_try_from_ctx = MockClient::try_from_context();
        client_try_from_ctx
            .expect()
            .return_once_st(|_| Ok(mock_client));

        let mut usercmd = UserCmdBuilder::default()
            .build()
            .expect("this should not happen");
        client_loaded_dispatcher_ctx.expect().times(0);

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_client_enter_world().times(1);
            })
            .run(|| {
                shinqlx_sv_cliententerworld(
                    client.borrow_mut(),
                    usercmd.borrow_mut() as *mut usercmd_t,
                );
            });
    }

    #[rstest]
    #[serial]
    fn client_enter_world_for_primed_client_with_gentity_informs_python(
        client_loaded_dispatcher_ctx: __client_loaded_dispatcher::Context,
    ) {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_PRIMED)
            .times(1);
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);
        let mut usercmd = UserCmdBuilder::default()
            .build()
            .expect("this should not happen");
        let client_try_from_ctx = MockClient::try_from_context();
        client_try_from_ctx
            .expect()
            .return_once_st(|_| Ok(mock_client));

        client_loaded_dispatcher_ctx
            .expect()
            .with(predicate::eq(42))
            .times(1);

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_client_enter_world().times(1);
            })
            .run(|| {
                shinqlx_sv_cliententerworld(
                    client.borrow_mut(),
                    usercmd.borrow_mut() as *mut usercmd_t,
                );
            });
    }

    #[rstest]
    #[serial]
    fn sv_set_configstring_with_parseable_variable(
        set_configstring_dispatcher_ctx: __set_configstring_dispatcher::Context,
    ) {
        set_configstring_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq(r"\some\value"))
            .return_const(Some(r"\some\value".into()))
            .times(1);

        let value = cr"\some\value";
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(predicate::eq(42), predicate::eq(r"\some\value"))
                    .times(1);
            })
            .run(|| {
                shinqlx_sv_setconfigstring(42 as c_int, value.as_ptr());
            });
    }

    #[test]
    #[serial]
    fn set_configstring_with_no_main_engine() {
        shinqlx_set_configstring(42u32, "some value");
    }

    #[rstest]
    #[case(16)]
    #[case(662)]
    #[case(663)]
    #[case(664)]
    #[case(665)]
    #[case(666)]
    #[case(667)]
    #[case(668)]
    #[case(669)]
    #[serial]
    fn set_configstring_for_undispatched_index(
        #[case] test_index: u16,
        set_configstring_dispatcher_ctx: __set_configstring_dispatcher::Context,
    ) {
        set_configstring_dispatcher_ctx.expect().times(0);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(
                        predicate::eq::<i32>(test_index.into()),
                        predicate::eq("some value"),
                    )
                    .times(1);
            })
            .run(|| {
                shinqlx_set_configstring(test_index, "some value");
            });
    }

    #[rstest]
    #[serial]
    fn set_confgistring_dispatcher_returns_none(
        set_configstring_dispatcher_ctx: __set_configstring_dispatcher::Context,
    ) {
        set_configstring_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq("some value"))
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_set_configstring().times(0);
            })
            .run(|| {
                shinqlx_set_configstring(42u32, "some value");
            });
    }

    #[rstest]
    #[serial]
    fn set_confgistring_dispatcher_returns_modified_string(
        set_configstring_dispatcher_ctx: __set_configstring_dispatcher::Context,
    ) {
        set_configstring_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq("some value"))
            .return_const(Some("other value".into()))
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(predicate::eq(42), predicate::eq("other value"))
                    .times(1);
            })
            .run(|| {
                shinqlx_set_configstring(42u32, "some value");
            });
    }

    #[rstest]
    #[serial]
    fn set_confgistring_dispatcher_returns_unmodified_string(
        set_configstring_dispatcher_ctx: __set_configstring_dispatcher::Context,
    ) {
        set_configstring_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq("some value"))
            .return_const(Some("some value".into()))
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(predicate::eq(42), predicate::eq("some value"))
                    .times(1);
            })
            .run(|| {
                shinqlx_set_configstring(42u32, "some value");
            });
    }

    #[test]
    #[serial]
    fn drop_client_is_dispatched_and_original_function_called() {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_disconnect()
            .with(predicate::eq("disconnected."))
            .times(1);
        mock_client.expect_get_client_id().return_const(42);

        let client_disconnect_dispatcher_ctx = client_disconnect_dispatcher_context();
        client_disconnect_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq("disconnected."))
            .times(1);

        shinqlx_drop_client(mock_client.borrow_mut(), "disconnected.");
    }

    #[test]
    #[serial]
    fn com_printf_with_no_main_engine() {
        shinqlx_com_printf("Hello world!");
    }

    #[test]
    #[serial]
    fn com_printf_when_dispatcher_returns_none() {
        let console_print_dispatcher_ctx = console_print_dispatcher_context();
        console_print_dispatcher_ctx
            .expect()
            .with(predicate::eq("Hello World!"))
            .times(1);

        MockEngineBuilder::default()
            .with_com_printf(predicate::always(), 0)
            .run(|| {
                shinqlx_com_printf("Hello World!");
            });
    }

    #[test]
    #[serial]
    fn com_printf_when_dispatcher_returns_some_value() {
        let console_print_dispatcher_ctx = console_print_dispatcher_context();
        console_print_dispatcher_ctx
            .expect()
            .with(predicate::eq("Hello World!"))
            .return_const(Some("Hello you!".into()))
            .times(1);

        MockEngineBuilder::default()
            .with_com_printf(predicate::eq("Hello World!"), 1)
            .run(|| {
                shinqlx_com_printf("Hello World!");
            });
    }

    #[test]
    #[serial]
    fn sv_spawnserver_with_no_main_engine() {
        let server_str = c"l33t ql server";
        shinqlx_sv_spawnserver(server_str.as_ptr().cast_mut(), qboolean::qtrue);
    }

    #[test]
    #[serial]
    fn sv_spawnserver_forwards_to_python() {
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx
            .expect()
            .with(predicate::eq(false))
            .times(1);

        let server_str = c"l33t ql server";
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_spawn_server()
                    .with(predicate::eq("l33t ql server"), predicate::eq(true))
                    .times(1);
            })
            .run(|| {
                shinqlx_sv_spawnserver(server_str.as_ptr().cast_mut(), qboolean::qtrue);
            });
    }

    #[test]
    #[serial]
    fn g_runframe_with_no_main_engine() {
        shinqlx_g_runframe(42);
    }

    #[test]
    #[serial]
    fn g_runframe_forwards_to_python() {
        let frame_dispatcher_ctx = frame_dispatcher_context();
        frame_dispatcher_ctx.expect().times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_run_frame()
                    .with(predicate::eq(42))
                    .times(1);
            })
            .run(|| {
                shinqlx_g_runframe(42);
            });
    }

    #[test]
    #[serial]
    fn client_connect_with_no_main_engine() {
        let result = shinqlx_client_connect(42, qboolean::qfalse, qboolean::qfalse);
        assert!(result.is_null());
    }

    #[test]
    #[serial]
    fn client_connect_not_first_time_client() {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_client_connect()
                    .with(
                        predicate::eq(42),
                        predicate::eq(false),
                        predicate::eq(false),
                    )
                    .returning(|_client_num, _first_time, _is_bot| c"".as_ptr().cast_mut())
                    .times(1);
            })
            .run(|| {
                shinqlx_client_connect(42, qboolean::qfalse, qboolean::qfalse);
            });
    }

    #[rstest]
    #[serial]
    fn client_connect_first_time_client_dispatcher_returns_none(
        client_connect_dispatcher_ctx: __client_connect_dispatcher::Context,
    ) {
        client_connect_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq(false))
            .return_const(None)
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_client_connect()
                    .with(predicate::eq(42), predicate::eq(true), predicate::eq(false))
                    .returning(|_client_num, _first_time, _is_bot| c"".as_ptr().cast_mut())
                    .times(1);
            })
            .run(|| {
                shinqlx_client_connect(42, qboolean::qtrue, qboolean::qfalse);
            });
    }

    #[rstest]
    #[serial]
    fn client_connect_first_time_client_dispatcher_returns_string(
        client_connect_dispatcher_ctx: __client_connect_dispatcher::Context,
    ) {
        client_connect_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq(false))
            .return_const(Some("you are banned from this server".to_string()))
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_client_connect().times(0);
            })
            .run(|| {
                let result = shinqlx_client_connect(42, qboolean::qtrue, qboolean::qfalse);
                assert_eq!(
                    // SAFETY: the mocked engine call passes a valid, NUL-terminated C string.
                    unsafe { CStr::from_ptr(result) },
                    c"you are banned from this server"
                );
            });
    }

    #[rstest]
    #[serial]
    fn client_connect_first_time_client_dispatcher_returns_some_for_bot(
        client_connect_dispatcher_ctx: __client_connect_dispatcher::Context,
    ) {
        client_connect_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq(true))
            .return_const(Some("we don't like bots here".to_string()))
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_client_connect()
                    .with(predicate::eq(42), predicate::eq(true), predicate::eq(true))
                    .returning(|_client_num, _first_time, _is_bot| c"".as_ptr().cast_mut())
                    .times(1);
            })
            .run(|| {
                shinqlx_client_connect(42, qboolean::qtrue, qboolean::qtrue);
            });
    }

    #[test]
    #[serial]
    fn client_spawn_with_no_main_engine() {
        let mut mock_entity = MockGameEntity::new();
        shinqlx_client_spawn(mock_entity.borrow_mut());
    }

    #[test]
    #[serial]
    fn client_spawn_forwards_to_ql_and_python() {
        let mut mock_entity = MockGameEntity::new();
        mock_entity.expect_get_entity_id().return_const(42).times(1);

        let client_spawn_dispatcher_ctx = client_spawn_dispatcher_context();
        client_spawn_dispatcher_ctx
            .expect()
            .with(predicate::eq(42))
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_client_spawn().times(1);
            })
            .run(|| {
                shinqlx_client_spawn(mock_entity.borrow_mut());
            });
    }

    #[test]
    #[serial]
    fn kamikaze_start_for_non_game_client() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx.expect().returning(|_| {
            let mut mock_gentity = MockGameEntity::new();
            mock_gentity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_gentity
                .expect_get_activator()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_gentity.expect_start_kamikaze().times(1);
            Ok(mock_gentity)
        });
        let kamikaze_explode_dispatcher_ctx = kamikaze_explode_dispatcher_context();
        kamikaze_explode_dispatcher_ctx.expect().times(0);

        shinqlx_g_startkamikaze(gentity.borrow_mut() as *mut gentity_t);
    }

    #[test]
    #[serial]
    fn kamikaze_start_for_existing_game_client_removes_kamikaze_flag() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx.expect().returning(|_| {
            let mut mock_gentity = MockGameEntity::new();
            mock_gentity
                .expect_get_game_client()
                .times(1)
                .return_once(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_get_client_num().return_const(42);
                    Ok(mock_game_client)
                });
            mock_gentity
                .expect_get_game_client()
                .times(1)
                .return_once(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_remove_kamikaze_flag();
                    Ok(mock_game_client)
                });
            mock_gentity
                .expect_get_game_client()
                .times(1)
                .return_once(|| Ok(MockGameClient::new()));
            mock_gentity.expect_start_kamikaze().times(1);
            Ok(mock_gentity)
        });
        let kamikaze_use_dispatcher_ctx = kamikaze_use_dispatcher_context();
        kamikaze_use_dispatcher_ctx
            .expect()
            .with(predicate::eq(42))
            .times(1);
        let kamikaze_explode_dispatcher_ctx = kamikaze_explode_dispatcher_context();
        kamikaze_explode_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq(true))
            .times(1);

        shinqlx_g_startkamikaze(gentity.borrow_mut() as *mut gentity_t);
    }

    #[test]
    #[serial]
    fn kamikaze_start_for_activator_use() {
        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx.expect().return_once(|_| {
            let mut mock_gentity = MockGameEntity::new();
            mock_gentity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_gentity.expect_get_activator().return_once(|| {
                let mut mock_activator = MockActivator::new();
                mock_activator.expect_get_owner_num().return_const(42);
                Ok(mock_activator)
            });
            mock_gentity.expect_start_kamikaze().times(1);
            Ok(mock_gentity)
        });
        let kamikaze_explode_dispatcher_ctx = kamikaze_explode_dispatcher_context();
        kamikaze_explode_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq(false))
            .times(1);

        shinqlx_g_startkamikaze(gentity.borrow_mut() as *mut gentity_t);
    }

    #[test]
    #[serial]
    fn g_damage_with_no_main_engine() {
        shinqlx_g_damage(
            ptr::null_mut() as *mut gentity_t,
            ptr::null_mut() as *mut gentity_t,
            ptr::null_mut() as *mut gentity_t,
            ptr::null_mut() as *mut vec3_t,
            ptr::null_mut() as *mut vec3_t,
            0,
            0,
            0,
        );
    }

    #[rstest]
    #[serial]
    fn g_damage_for_null_target_is_not_forwarded(
        damage_dispatcher_ctx: __damage_dispatcher::Context,
    ) {
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx
            .expect()
            .return_once(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        damage_dispatcher_ctx.expect().times(0);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_register_damage()
                    .withf(
                        |&target,
                         &inflictor,
                         &attacker,
                         &dir,
                         &pos,
                         &damage,
                         &dflags,
                         &means_of_death| {
                            target.is_null()
                                && inflictor.is_null()
                                && attacker.is_null()
                                && pos.is_null()
                                && dir.is_null()
                                && damage == 0
                                && dflags == 0
                                && means_of_death == 0
                        },
                    )
                    .times(1);
            })
            .run(|| {
                shinqlx_g_damage(
                    ptr::null_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut vec3_t,
                    ptr::null_mut() as *mut vec3_t,
                    0,
                    0,
                    0,
                );
            });
    }

    #[rstest]
    #[serial]
    fn g_damage_for_null_attacker(damage_dispatcher_ctx: __damage_dispatcher::Context) {
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx
            .expect()
            .return_once(|_| {
                let mut mock_gentity = MockGameEntity::new();
                mock_gentity.expect_get_entity_id().return_const(42);
                Ok(mock_gentity)
            })
            .times(1);

        damage_dispatcher_ctx
            .expect()
            .with(
                predicate::eq(42),
                predicate::eq(None),
                predicate::eq(666),
                predicate::eq(0),
                predicate::eq(0),
            )
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_register_damage()
                    .withf(
                        |&target,
                         &inflictor,
                         &attacker,
                         &dir,
                         &pos,
                         &damage,
                         &dflags,
                         &means_of_death| {
                            target.is_null()
                                && inflictor.is_null()
                                && attacker.is_null()
                                && pos.is_null()
                                && dir.is_null()
                                && damage == 666
                                && dflags == 0
                                && means_of_death == 0
                        },
                    )
                    .times(1);
            })
            .run(|| {
                shinqlx_g_damage(
                    ptr::null_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut vec3_t,
                    ptr::null_mut() as *mut vec3_t,
                    666,
                    0,
                    0,
                );
            });
    }

    #[rstest]
    #[serial]
    fn g_damage_for_non_null_attacker_try_from_returns_err(
        damage_dispatcher_ctx: __damage_dispatcher::Context,
    ) {
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx
            .expect()
            .return_once(|_| {
                let mut mock_gentity = MockGameEntity::new();
                mock_gentity.expect_get_entity_id().return_const(42);
                Ok(mock_gentity)
            })
            .times(1);
        try_from_ctx
            .expect()
            .return_once(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .times(1);

        damage_dispatcher_ctx
            .expect()
            .with(
                predicate::eq(42),
                predicate::eq(None),
                predicate::eq(666),
                predicate::eq(16),
                predicate::eq(7),
            )
            .times(1);

        let mut attacker = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_register_damage()
                    .withf(
                        |&target,
                         &inflictor,
                         &attacker,
                         &dir,
                         &pos,
                         &damage,
                         &dflags,
                         &means_of_death| {
                            target.is_null()
                                && inflictor.is_null()
                                && !attacker.is_null()
                                && pos.is_null()
                                && dir.is_null()
                                && damage == 666
                                && dflags == 16
                                && means_of_death == 7
                        },
                    )
                    .times(1);
            })
            .run(|| {
                shinqlx_g_damage(
                    ptr::null_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut gentity_t,
                    attacker.borrow_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut vec3_t,
                    ptr::null_mut() as *mut vec3_t,
                    666,
                    16,
                    7,
                );
            });
    }

    #[rstest]
    #[serial]
    fn g_damage_for_non_null_attacker_try_from_returns_ok(
        damage_dispatcher_ctx: __damage_dispatcher::Context,
    ) {
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx
            .expect()
            .return_once(|_| {
                let mut mock_gentity = MockGameEntity::new();
                mock_gentity.expect_get_entity_id().return_const(42);
                Ok(mock_gentity)
            })
            .times(1);
        try_from_ctx
            .expect()
            .return_once(|_| {
                let mut gentity = MockGameEntity::new();
                gentity.expect_get_entity_id().return_const(21);
                Ok(gentity)
            })
            .times(1);

        damage_dispatcher_ctx
            .expect()
            .with(
                predicate::eq(42),
                predicate::eq(Some(21)),
                predicate::eq(50),
                predicate::eq(4),
                predicate::eq(2),
            )
            .times(1);

        let mut attacker = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_register_damage()
                    .withf(
                        |&target,
                         &inflictor,
                         &attacker,
                         &dir,
                         &pos,
                         &damage,
                         &dflags,
                         &means_of_death| {
                            target.is_null()
                                && inflictor.is_null()
                                && !attacker.is_null()
                                && pos.is_null()
                                && dir.is_null()
                                && damage == 50
                                && dflags == 4
                                && means_of_death == 2
                        },
                    )
                    .times(1);
            })
            .run(|| {
                shinqlx_g_damage(
                    ptr::null_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut gentity_t,
                    attacker.borrow_mut() as *mut gentity_t,
                    ptr::null_mut() as *mut vec3_t,
                    ptr::null_mut() as *mut vec3_t,
                    50,
                    4,
                    2,
                );
            });
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn com_printf_formats_the_variable_arguments() {
        let console_print_dispatcher_ctx = console_print_dispatcher_context();
        console_print_dispatcher_ctx
            .expect()
            .with(predicate::eq("Hello 42 world"))
            .return_const(Some("Hello 42 world".into()))
            .times(1);

        MockEngineBuilder::default()
            .with_com_printf(predicate::eq("Hello 42 world"), 1)
            // SAFETY: the format and its variadic arguments match, the strings are NUL-terminated.
            .run(|| unsafe {
                ShiNQlx_Com_Printf(c"Hello %d %s".as_ptr(), 42 as c_int, c"world".as_ptr());
            });
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn com_printf_with_empty_message_is_not_forwarded() {
        let console_print_dispatcher_ctx = console_print_dispatcher_context();
        console_print_dispatcher_ctx.expect().times(0);

        MockEngineBuilder::default()
            .with_com_printf(predicate::always(), 0)
            // SAFETY: the format is a NUL-terminated string without conversions.
            .run(|| unsafe {
                ShiNQlx_Com_Printf(c"".as_ptr());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn send_server_command_formats_arguments_for_null_client(
        server_command_dispatcher_ctx: __server_command_dispatcher::Context,
    ) {
        server_command_dispatcher_ctx
            .expect()
            .with(predicate::eq(None), predicate::eq("cp 7"))
            .return_const(Some("cp 7".into()))
            .times(1);

        MockEngineBuilder::default()
            .with_send_server_command(|client, cmd| client.is_none() && cmd == "cp 7", 1)
            // SAFETY: the format and its variadic argument match, a null client is allowed.
            .run(|| unsafe {
                ShiNQlx_SV_SendServerCommand(ptr::null_mut(), c"cp %d".as_ptr(), 7 as c_int);
            });
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn send_server_command_formats_arguments_for_valid_client() {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const(false)
            .times(1);
        let client_try_from_ctx = MockClient::try_from_context();
        client_try_from_ctx
            .expect()
            .return_once_st(|_| Ok(mock_client));

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_send_server_command(|client, cmd| client.is_some() && cmd == "print hi", 1)
            // SAFETY: the format and its variadic arguments match, the client is valid.
            .run(|| unsafe {
                ShiNQlx_SV_SendServerCommand(
                    client.borrow_mut(),
                    c"print %s".as_ptr(),
                    c"hi".as_ptr(),
                );
            });
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn send_server_command_for_invalid_client_is_not_forwarded() {
        let client_try_from_ctx = MockClient::try_from_context();
        client_try_from_ctx
            .expect()
            .return_once_st(|_| Err(QuakeLiveEngineError::NullPointerPassed("null".to_string())));

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_send_server_command(|_client, _cmd| true, 0)
            // SAFETY: the format is a NUL-terminated string without conversions, the client is valid.
            .run(|| unsafe {
                ShiNQlx_SV_SendServerCommand(client.borrow_mut(), c"print hi".as_ptr());
            });
    }

    #[test]
    #[serial]
    fn send_server_command_with_empty_cmd_is_not_forwarded() {
        MockEngineBuilder::default()
            .with_send_server_command(|_client, _cmd| true, 0)
            .run(|| {
                shinqlx_send_server_command(None, "");
            });
    }

    #[test]
    #[serial]
    fn execute_client_command_with_empty_cmd_is_not_forwarded() {
        MockEngineBuilder::default()
            .with_execute_client_command(|_client, _cmd, _client_ok| true, 0)
            .run(|| {
                shinqlx_execute_client_command(None, "", true);
            });
    }

    #[test]
    #[serial]
    fn sv_executeclientcommand_forwards_valid_client() {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const(false)
            .times(1);
        let client_try_from_ctx = MockClient::try_from_context();
        client_try_from_ctx
            .expect()
            .return_once_st(|_| Ok(mock_client));

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_execute_client_command(
                |client, cmd, &client_ok| client.is_some() && cmd == "cp asdf" && client_ok.into(),
                1,
            )
            .run(|| {
                shinqlx_sv_executeclientcommand(
                    client.borrow_mut(),
                    c"cp asdf".as_ptr(),
                    qboolean::qtrue,
                );
            });
    }

    #[test]
    #[serial]
    fn sys_setmoduleoffset_for_unknown_module_still_sets_the_offset() {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_module_offset()
                    .withf(|module_name, _offset| module_name == "other")
                    .times(1);
                mock_engine
                    .expect_initialize_vm()
                    .returning(|_offset| Ok(()))
                    .times(1);
            })
            .run(|| {
                shinqlx_sys_setmoduleoffset(c"other".as_ptr().cast_mut(), dummy_function);
            });
    }

    #[rstest]
    #[serial]
    fn sv_set_configstring_with_null_value_uses_empty_string(
        set_configstring_dispatcher_ctx: __set_configstring_dispatcher::Context,
    ) {
        set_configstring_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq(""))
            .return_const(None)
            .times(1);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_set_configstring().times(0);
            })
            .run(|| {
                shinqlx_sv_setconfigstring(42 as c_int, ptr::null());
            });
    }

    #[rstest]
    #[serial]
    fn sv_set_configstring_with_negative_index_is_ignored(
        set_configstring_dispatcher_ctx: __set_configstring_dispatcher::Context,
    ) {
        set_configstring_dispatcher_ctx.expect().times(0);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_set_configstring().times(0);
            })
            .run(|| {
                shinqlx_sv_setconfigstring(-1 as c_int, c"value".as_ptr());
            });
    }

    #[test]
    #[serial]
    fn sv_dropclient_dispatches_and_disconnects_valid_client() {
        let mut mock_client = MockClient::new();
        mock_client.expect_get_client_id().return_const(42);
        mock_client
            .expect_disconnect()
            .with(predicate::eq("bye"))
            .times(1);
        let client_try_from_ctx = MockClient::try_from_context();
        client_try_from_ctx
            .expect()
            .return_once_st(|_| Ok(mock_client));

        let client_disconnect_dispatcher_ctx = client_disconnect_dispatcher_context();
        client_disconnect_dispatcher_ctx
            .expect()
            .with(predicate::eq(42), predicate::eq("bye"))
            .times(1);

        let mut client = ClientBuilder::default()
            .build()
            .expect("this should not happen");

        shinqlx_sv_dropclient(client.borrow_mut(), c"bye".as_ptr());
    }

    fn reset_query_limiter() {
        *QUERY_LIMITER.lock() = QueryRateLimiter::new(QueryLimits::DEFAULT);
        QUERY_LIMITS_REFRESHED_MS.store(0, Ordering::Release);
    }

    fn query_address(type_: netadrtype_t) -> netadr_t {
        NetadrBuilder::default()
            .type_(type_)
            .ip([192, 0, 2, 7])
            .build()
            .expect("this should not happen")
    }

    const A2S_INFO: &[u8] = b"\xff\xff\xff\xffTSource Engine Query\0";

    fn packet_buffer(payload: &[u8], maxsize: usize) -> SteamPacketBuffer {
        SteamPacketBuffer {
            allowoverflow: 0,
            overflowed: 0,
            data: payload.as_ptr(),
            maxsize: maxsize as c_int,
            cursize: payload.len() as c_int,
        }
    }

    fn send_buffers(
        from: *const netadr_t,
        buffer: &SteamPacketBuffer,
        packets: usize,
    ) -> Vec<c_int> {
        let msg = ptr::from_ref(buffer).cast::<msg_t>();
        (0..packets)
            .map(|_| shinqlx_sv_handlesteampacket(from, msg))
            .collect()
    }

    fn send_packets(from: *const netadr_t, payload: &[u8], packets: usize) -> Vec<c_int> {
        send_buffers(from, &packet_buffer(payload, payload.len()), packets)
    }

    #[test]
    fn the_packet_buffer_matches_the_layout_the_engine_reads() {
        assert_eq!(core::mem::offset_of!(SteamPacketBuffer, data), 0x8);
        assert_eq!(core::mem::offset_of!(SteamPacketBuffer, maxsize), 0x10);
        assert_eq!(core::mem::offset_of!(SteamPacketBuffer, cursize), 0x14);
    }

    fn send_query_packets(from: *const netadr_t, packets: usize) -> Vec<c_int> {
        send_packets(from, A2S_INFO, packets)
    }

    #[test]
    #[serial]
    fn sv_handlesteampacket_forwards_packets_within_the_burst() {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(10);
            })
            .run(|| {
                assert_eq!(send_query_packets(&address, 10), vec![0; 10]);
            });
    }

    #[test]
    #[serial]
    fn sv_handlesteampacket_drops_packets_above_the_burst_without_calling_steam() {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(10);
            })
            .run(|| {
                let results = send_query_packets(&address, 12);

                assert_eq!(results, [vec![0; 10], vec![1; 2]].concat());
            });
    }

    #[rstest]
    #[case::info(b"\xff\xff\xff\xffTSource Engine Query\0".as_slice())]
    #[case::player(b"\xff\xff\xff\xffU\xff\xff\xff\xff".as_slice())]
    #[case::rules(b"\xff\xff\xff\xffV\xff\xff\xff\xff".as_slice())]
    #[serial]
    fn sv_handlesteampacket_limits_every_query_opcode(#[case] payload: &[u8]) {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(10);
            })
            .run(|| {
                let results = send_packets(&address, payload, 11);

                assert_eq!(results, [vec![0; 10], vec![1]].concat());
            });
    }

    #[rstest]
    #[case::other_opcode(b"\xff\xff\xff\xffW\xff\xff\xff\xff".as_slice())]
    #[case::quake_connectionless(b"\xff\xff\xff\xffgetchallenge\0".as_slice())]
    #[case::missing_header(b"TSource Engine Query\0".as_slice())]
    #[case::header_only(b"\xff\xff\xff\xff".as_slice())]
    #[case::too_short(b"\xff".as_slice())]
    #[serial]
    fn sv_handlesteampacket_forwards_packets_that_are_no_queries_unlimited(#[case] payload: &[u8]) {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(30);
            })
            .run(|| {
                assert_eq!(send_packets(&address, payload, 30), vec![0; 30]);
            });
    }

    #[test]
    #[serial]
    fn sv_handlesteampacket_forwards_packets_without_message_unlimited() {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(30);
            })
            .run(|| {
                let results: Vec<c_int> = (0..30)
                    .map(|_| shinqlx_sv_handlesteampacket(&address, ptr::null()))
                    .collect();

                assert_eq!(results, vec![0; 30]);
            });
    }

    #[test]
    #[serial]
    fn sv_handlesteampacket_does_not_trust_messages_larger_than_their_buffer() {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);
        let buffer = packet_buffer(A2S_INFO, 4);

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(30);
            })
            .run(|| {
                let results = send_buffers(&address, &buffer, 30);

                assert_eq!(results, vec![0; 30]);
            });
    }

    #[test]
    #[serial]
    fn sv_handlesteampacket_does_not_count_other_packets_against_the_query_limit() {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(10 + 50);
            })
            .run(|| {
                send_packets(&address, b"\xff\xff\xff\xffW\xff\xff\xff\xff", 50);

                assert_eq!(send_query_packets(&address, 10), vec![0; 10]);
            });
    }

    #[test]
    #[serial]
    fn sv_handlesteampacket_does_not_limit_other_address_types() {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_LOOPBACK);

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(20);
            })
            .run(|| {
                assert_eq!(send_query_packets(&address, 20), vec![0; 20]);
            });
    }

    #[test]
    #[serial]
    fn sv_handlesteampacket_forwards_packets_without_source_address() {
        reset_query_limiter();

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(true)
                    .times(1);
            })
            .run(|| {
                assert_eq!(send_query_packets(ptr::null(), 1), vec![1]);
            });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn sv_handlesteampacket_applies_the_limits_from_the_cvars() {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);
        let mut burst_cvar = CVarBuilder::default()
            .string(c"2".as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |_| true,
                move |name| {
                    (name == "qlx_steamQueryBurst")
                        .then(|| CVar::try_from(burst_cvar.borrow_mut() as *mut cvar_t).ok())
                        .flatten()
                },
                1..,
            )
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(2);
            })
            .run(|| {
                let results = send_query_packets(&address, 4);

                assert_eq!(results, vec![0, 0, 1, 1]);
            });
    }

    #[test]
    #[serial]
    fn sv_handlesteampacket_does_not_trust_messages_with_negative_size() {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);
        let buffer = SteamPacketBuffer {
            cursize: -1,
            ..packet_buffer(A2S_INFO, A2S_INFO.len())
        };

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .configure(|mock_engine| {
                mock_engine
                    .expect_handle_steam_packet()
                    .return_const(false)
                    .times(30);
            })
            .run(|| {
                assert_eq!(send_buffers(&address, &buffer, 30), vec![0; 30]);
            });
    }

    #[test]
    #[serial]
    fn steam_query_allowed_reports_the_dropped_packets_once_per_interval() {
        reset_query_limiter();
        let address = query_address(netadrtype_t::NA_IP);
        let after_interval_ms = 1_000 + 61_000;

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .run(|| {
                let main_engine = MAIN_ENGINE.load();
                let main_engine = main_engine
                    .as_ref()
                    .expect("the mock engine is registered by run");

                let allowed = (0..12)
                    .filter(|_| steam_query_allowed(&**main_engine, &address, 1_000))
                    .count();
                assert_eq!(allowed, 10);

                assert!(steam_query_allowed(
                    &**main_engine,
                    &address,
                    after_interval_ms
                ));
                assert_eq!(
                    QUERY_LIMITER.lock().take_dropped_summary(after_interval_ms),
                    None
                );
            });
    }

    #[test]
    #[serial]
    fn sv_spawnserver_with_empty_server_name_is_ignored() {
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx.expect().times(0);

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_spawn_server().times(0);
            })
            .run(|| {
                shinqlx_sv_spawnserver(c"".as_ptr().cast_mut(), qboolean::qfalse);
            });
    }

    #[test]
    #[serial]
    fn clientspawn_forwards_valid_entity() {
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx.expect().return_once_st(|_| {
            let mut mock_entity = MockGameEntity::new();
            mock_entity.expect_get_entity_id().return_const(42);
            Ok(mock_entity)
        });
        let client_spawn_dispatcher_ctx = client_spawn_dispatcher_context();
        client_spawn_dispatcher_ctx
            .expect()
            .with(predicate::eq(42))
            .times(1);

        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine.expect_client_spawn().times(1);
            })
            .run(|| {
                shinqlx_clientspawn(gentity.borrow_mut() as *mut gentity_t);
            });
    }

    #[test]
    #[serial]
    fn kamikaze_start_for_invalid_entity_is_ignored() {
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx
            .expect()
            .return_once_st(|_| Err(QuakeLiveEngineError::NullPointerPassed("null".to_string())));
        let kamikaze_use_dispatcher_ctx = kamikaze_use_dispatcher_context();
        kamikaze_use_dispatcher_ctx.expect().times(0);

        let mut gentity = GEntityBuilder::default()
            .build()
            .expect("this should not happen");

        shinqlx_g_startkamikaze(gentity.borrow_mut() as *mut gentity_t);
    }
}
