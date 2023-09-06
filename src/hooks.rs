#[cfg(test)]
use crate::hooks::DUMMY_MAIN_ENGINE as MAIN_ENGINE;
use crate::prelude::*;
#[cfg(test)]
use crate::pyminqlx::mock_python::{
    client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
    client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher, damage_dispatcher,
    frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher, new_game_dispatcher,
    server_command_dispatcher, set_configstring_dispatcher,
};
#[cfg(not(test))]
use crate::pyminqlx::{
    client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
    client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher, damage_dispatcher,
    frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher, new_game_dispatcher,
    server_command_dispatcher, set_configstring_dispatcher,
};
#[cfg(test)]
use crate::quake_live_engine::MockQuakeEngine as QuakeLiveEngine;
use crate::quake_live_engine::{
    AddCommand, ClientConnect, ClientEnterWorld, ClientSpawn, ComPrintf, ExecuteClientCommand,
    InitGame, RegisterDamage, RunFrame, SendServerCommand, SetConfigstring, SetModuleOffset,
    ShutdownGame, SpawnServer,
};
#[cfg(not(test))]
use crate::MAIN_ENGINE;
use alloc::string::String;
use core::ffi::{c_char, c_int, CStr, VaList, VaListImpl};
use core::ops::Deref;
#[cfg(test)]
use once_cell::sync::Lazy;
#[cfg(test)]
use swap_arc::SwapArcOption;

#[cfg(test)]
static DUMMY_MAIN_ENGINE: Lazy<SwapArcOption<QuakeLiveEngine>> =
    Lazy::new(|| SwapArcOption::new(None));

pub(crate) fn shinqlx_cmd_addcommand(cmd: *const c_char, func: unsafe extern "C" fn()) {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    if !main_engine.is_common_initialized() {
        if let Err(err) = main_engine.initialize_static() {
            error!(target: "shinqlx", "{:?}", err);
            error!(target: "shinqlx", "Static initialization failed. Exiting.");
            panic!("Static initialization failed. Exiting.");
        }
    }

    let command = unsafe { CStr::from_ptr(cmd) }.to_string_lossy();
    if !command.is_empty() {
        #[allow(clippy::unnecessary_to_owned)]
        main_engine.add_command(command.to_string(), func);
    }
}

pub(crate) fn shinqlx_sys_setmoduleoffset(
    module_name: *const c_char,
    offset: unsafe extern "C" fn(),
) {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    let converted_module_name = unsafe { CStr::from_ptr(module_name) }.to_string_lossy();

    // We should be getting qagame, but check just in case.
    if converted_module_name.as_ref() != "qagame" {
        error!(target: "shinqlx", "Unknown module: {}", converted_module_name);
    }

    #[allow(clippy::unnecessary_to_owned)]
    main_engine.set_module_offset(converted_module_name.to_string(), offset);

    if let Err(err) = main_engine.initialize_vm(offset as usize) {
        error!(target: "shinqlx", "{:?}", err);
        error!(target: "shinqlx", "VM could not be initializied. Exiting.");
        panic!("VM could not be initializied. Exiting.");
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn shinqlx_g_initgame(level_time: c_int, random_seed: c_int, restart: c_int) {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    main_engine.init_game(level_time, random_seed, restart);

    main_engine.set_tag();
    main_engine.initialize_cvars();

    if restart != 0 {
        new_game_dispatcher(true);
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn shinqlx_g_shutdowngame(restart: c_int) {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    main_engine.unhook_vm();
    main_engine.shutdown_game(restart);
}

pub(crate) fn shinqlx_sv_executeclientcommand(
    client: *mut client_t,
    cmd: *const c_char,
    client_ok: qboolean,
) {
    let rust_cmd = unsafe { CStr::from_ptr(cmd) }.to_string_lossy();
    shinqlx_execute_client_command(Client::try_from(client).ok(), rust_cmd, client_ok);
}

pub(crate) fn shinqlx_execute_client_command<T, U>(client: Option<Client>, cmd: T, client_ok: U)
where
    T: AsRef<str> + Into<String>,
    U: Into<qboolean> + Into<bool> + Copy,
{
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    if cmd.as_ref().is_empty() {
        return;
    }
    let passed_on_cmd_str: String = if client_ok.into()
        && client
            .as_ref()
            .is_some_and(|safe_client| safe_client.has_gentity())
    {
        let client_id = client
            .as_ref()
            .map(|safe_client| safe_client.get_client_id())
            .unwrap();
        let Some(dispatcher_result) = client_command_dispatcher(client_id, cmd.into()) else {
            return;
        };
        dispatcher_result
    } else {
        cmd.into()
    };

    if !passed_on_cmd_str.is_empty() {
        main_engine.execute_client_command(
            client,
            passed_on_cmd_str,
            <bool as Into<qboolean>>::into(client_ok.into()),
        );
    }
}

#[no_mangle]
pub unsafe extern "C" fn ShiNQlx_SV_SendServerCommand(
    client: *mut client_t,
    fmt: *const c_char,
    fmt_args: ...
) {
    extern "C" {
        fn vsnprintf(s: *mut c_char, n: usize, format: *const c_char, arg: VaList) -> c_int;
    }

    let mut va_args: VaListImpl = fmt_args.clone();
    let mut buffer: [u8; MAX_MSGLEN as usize] = [0; MAX_MSGLEN as usize];
    let result = vsnprintf(
        buffer.as_mut_ptr() as *mut c_char,
        buffer.len(),
        fmt,
        va_args.as_va_list(),
    );
    if result < 0 {
        warn!(target: "shinqlx", "some formatting problem occurred");
    }

    let cmd = CStr::from_bytes_until_nul(&buffer)
        .unwrap()
        .to_string_lossy();
    if client.is_null() {
        shinqlx_send_server_command(None, cmd);
    } else {
        let safe_client: Result<Client, _> = client.try_into();
        if safe_client.is_ok() {
            shinqlx_send_server_command(safe_client.ok(), cmd);
        }
    }
}

pub(crate) fn shinqlx_send_server_command<T>(client: Option<Client>, cmd: T)
where
    T: AsRef<str> + Into<String>,
{
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    if cmd.as_ref().is_empty() {
        return;
    }
    let mut passed_on_cmd_str = cmd.into();

    match client.as_ref() {
        Some(safe_client) => {
            if safe_client.has_gentity() {
                let client_id = safe_client.get_client_id();
                let Some(res) =
                    server_command_dispatcher(Some(client_id), passed_on_cmd_str.clone())
                else {
                    return;
                };
                passed_on_cmd_str = res;
            }
        }
        None => {
            let Some(res) = server_command_dispatcher(None, passed_on_cmd_str.clone()) else {
                return;
            };
            passed_on_cmd_str = res;
        }
    }

    if !passed_on_cmd_str.is_empty() {
        main_engine.send_server_command(client, passed_on_cmd_str);
    }
}

pub(crate) fn shinqlx_sv_cliententerworld(client: *mut client_t, cmd: *mut usercmd_t) {
    let Some(safe_client): Option<Client> = client.try_into().ok() else {
        return;
    };

    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    shinqlx_sv_cliententerworld_intern(main_engine.deref(), safe_client, cmd);
}

#[cfg_attr(not(test), inline)]
fn shinqlx_sv_cliententerworld_intern<T>(
    main_engine: &T,
    mut safe_client: Client,
    cmd: *mut usercmd_t,
) where
    T: for<'a> ClientEnterWorld<&'a mut Client>,
{
    let state = safe_client.get_state();

    main_engine.client_enter_world(&mut safe_client, cmd);

    // gentity is NULL if map changed.
    // state is CS_PRIMED only if it's the first time they connect to the server,
    // otherwise the dispatcher would also go off when a game starts and such.
    if safe_client.has_gentity() && state == clientState_t::CS_PRIMED {
        client_loaded_dispatcher(safe_client.get_client_id());
    }
}

pub(crate) fn shinqlx_sv_setconfigstring(index: c_int, value: *const c_char) {
    let safe_value = if !value.is_null() {
        unsafe { CStr::from_ptr(value) }.to_string_lossy()
    } else {
        "".into()
    };

    let Ok(ql_index) = u32::try_from(index) else {
        return;
    };

    shinqlx_set_configstring(ql_index, safe_value);
}

pub(crate) fn shinqlx_set_configstring<T, U>(index: T, value: U)
where
    T: TryInto<c_int> + Into<u32> + Copy,
    U: AsRef<str>,
{
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    #[allow(clippy::unnecessary_to_owned)]
    shinqlx_set_configstring_intern(
        main_engine.deref(),
        index.into(),
        value.as_ref().to_string(),
    );
}

#[cfg_attr(not(test), inline)]
fn shinqlx_set_configstring_intern<T>(main_engine: &T, index: u32, value: String)
where
    T: SetConfigstring<c_int, String>,
{
    let Ok(c_index) = index.try_into() else {
        return;
    };

    // Indices 16 and 66X are spammed a ton every frame for some reason,
    // so we add some exceptions for those. I don't think we should have any
    // use for those particular ones anyway. If we don't do this, we get
    // like a 25% increase in CPU usage on an empty server.
    if c_index == 16 || (662..670).contains(&c_index) {
        main_engine.set_configstring(c_index, value);
        return;
    }

    let Some(res) = set_configstring_dispatcher(index, value) else {
        return;
    };
    main_engine.set_configstring(c_index, res);
}

pub(crate) fn shinqlx_sv_dropclient(client: *mut client_t, reason: *const c_char) {
    let Ok(mut safe_client) = Client::try_from(client) else {
        return;
    };
    shinqlx_drop_client(
        &mut safe_client,
        unsafe { CStr::from_ptr(reason) }.to_string_lossy(),
    );
}

pub(crate) fn shinqlx_drop_client<T>(client: &mut Client, reason: T)
where
    T: AsRef<str>,
{
    #[allow(clippy::unnecessary_to_owned)]
    client_disconnect_dispatcher(client.get_client_id(), reason.as_ref().to_string());

    #[allow(clippy::unnecessary_to_owned)]
    client.disconnect(reason.as_ref().to_string());
}

#[no_mangle]
pub unsafe extern "C" fn ShiNQlx_Com_Printf(fmt: *const c_char, fmt_args: ...) {
    extern "C" {
        fn vsnprintf(s: *mut c_char, n: usize, format: *const c_char, arg: VaList) -> c_int;
    }

    let mut va_args: VaListImpl = fmt_args.clone();
    let mut buffer: [u8; MAX_MSGLEN as usize] = [0; MAX_MSGLEN as usize];
    let result = vsnprintf(
        buffer.as_mut_ptr() as *mut c_char,
        buffer.len(),
        fmt,
        va_args.as_va_list(),
    );
    if result < 0 {
        warn!(target: "shinqlx", "some formatting problem occurred");
    }

    let rust_msg = CStr::from_bytes_until_nul(&buffer)
        .unwrap()
        .to_string_lossy();
    if !rust_msg.is_empty() {
        shinqlx_com_printf(rust_msg);
    }
}

pub(crate) fn shinqlx_com_printf<T>(msg: T)
where
    T: AsRef<str>,
{
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    shinqlx_com_printf_intern(main_engine.deref(), msg.as_ref().to_string());
}

#[cfg_attr(not(test), inline)]
fn shinqlx_com_printf_intern<T>(main_engine: &T, msg: String)
where
    T: ComPrintf<String>,
{
    let Some(_res) = console_print_dispatcher(msg.clone()) else {
        return;
    };

    main_engine.com_printf(msg);
}

pub(crate) fn shinqlx_sv_spawnserver(server: *const c_char, kill_bots: qboolean) {
    let server_str = unsafe { CStr::from_ptr(server) }.to_string_lossy();
    if server_str.is_empty() {
        return;
    }

    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    #[allow(clippy::unnecessary_to_owned)]
    shinqlx_sv_spawnserver_intern(
        main_engine.deref(),
        server_str.as_ref().to_string(),
        Into::<bool>::into(kill_bots),
    );
}

#[cfg_attr(not(test), inline)]
fn shinqlx_sv_spawnserver_intern<T, U: AsRef<str>, V: Into<qboolean>>(
    main_engine: &T,
    server_str: U,
    kill_bots: V,
) where
    T: SpawnServer<U, V>,
{
    main_engine.spawn_server(server_str, kill_bots);

    new_game_dispatcher(false);
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn shinqlx_g_runframe(time: c_int) {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    shinqlx_g_runframe_intern(main_engine.deref(), time);
}

#[cfg_attr(not(test), inline)]
fn shinqlx_g_runframe_intern<T>(main_engine: &T, time: c_int)
where
    T: RunFrame<c_int>,
{
    frame_dispatcher();

    main_engine.run_frame(time);
}

static mut CLIENT_CONNECT_BUFFER: [[c_char; MAX_STRING_CHARS as usize]; MAX_CLIENTS as usize] =
    [[0; MAX_STRING_CHARS as usize]; MAX_CLIENTS as usize];

unsafe fn to_return_string(client_id: i32, input: String) -> *const c_char {
    let bytes = input.as_bytes();
    let mut bytes_iter = bytes.iter();
    let len = bytes.len();
    CLIENT_CONNECT_BUFFER[client_id as usize][0..len]
        .fill_with(|| *bytes_iter.next().unwrap() as c_char);
    CLIENT_CONNECT_BUFFER[client_id as usize][len..].fill(0);
    &CLIENT_CONNECT_BUFFER[client_id as usize] as *const c_char
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn shinqlx_client_connect(
    client_num: c_int,
    first_time: qboolean,
    is_bot: qboolean,
) -> *const c_char {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return ptr::null();
    };

    shinqlx_client_connect_intern(
        main_engine.deref(),
        client_num,
        first_time.into(),
        is_bot.into(),
    )
}

#[cfg_attr(not(test), inline)]
fn shinqlx_client_connect_intern<T>(
    main_engine: &T,
    client_num: c_int,
    first_time: bool,
    is_bot: bool,
) -> *const c_char
where
    T: ClientConnect<c_int, bool, bool>,
{
    if first_time {
        if let Some(res) = client_connect_dispatcher(client_num, is_bot) {
            if !is_bot {
                return unsafe { to_return_string(client_num, res) };
            }
        }
    }

    main_engine.client_connect(client_num, first_time, is_bot)
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn shinqlx_clientspawn(ent: *mut gentity_t) {
    let Some(game_entity): Option<GameEntity> = ent.try_into().ok() else {
        return;
    };

    shinqlx_client_spawn(game_entity)
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn shinqlx_client_spawn(game_entity: GameEntity) {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    shinqlx_client_spawn_intern(main_engine.deref(), game_entity);
}

#[cfg_attr(not(test), inline)]
fn shinqlx_client_spawn_intern<T>(main_engine: &T, mut game_entity: GameEntity)
where
    T: for<'a> ClientSpawn<&'a mut GameEntity>,
{
    main_engine.client_spawn(&mut game_entity);

    // Since we won't ever stop the real function from being called,
    // we trigger the event after calling the real one. This will allow
    // us to set weapons and such without it getting overriden later.
    client_spawn_dispatcher(game_entity.get_entity_id());
}

pub(crate) fn shinqlx_g_startkamikaze(ent: *mut gentity_t) {
    let Some(mut game_entity): Option<GameEntity> = ent.try_into().ok() else {
        return;
    };

    let client_id = if let Ok(game_client) = game_entity.get_game_client() {
        game_client.get_client_num()
    } else if let Ok(activator) = game_entity.get_activator() {
        activator.get_owner_num()
    } else {
        -1
    };

    if let Ok(mut game_client) = game_entity.get_game_client() {
        game_client.remove_kamikaze_flag();
        kamikaze_use_dispatcher(client_id);
    }
    game_entity.start_kamikaze();

    if client_id == -1 {
        return;
    }

    kamikaze_explode_dispatcher(client_id, game_entity.get_game_client().is_ok())
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn shinqlx_g_damage(
    target: *mut gentity_t,    // entity that is being damaged
    inflictor: *mut gentity_t, // entity that is causing the damage
    attacker: *mut gentity_t,  // entity that caused the inflictor to damage target
    dir: *mut vec3_t,          // direction of the attack for knockback
    pos: *mut vec3_t,          // point at which the damage is being inflicted, used for headshots
    damage: c_int,             // amount of damage being inflicted
    dflags: c_int,             // these flags are used to control how T_Damage works
    // DAMAGE_RADIUS			damage was indirect (from a nearby explosion)
    // DAMAGE_NO_ARMOR			armor does not protect from this damage
    // DAMAGE_NO_KNOCKBACK		do not affect velocity, just view angles
    // DAMAGE_NO_PROTECTION	kills godmode, armor, everything
    // DAMAGE_NO_TEAM_PROTECTION	kills team mates
    means_of_death: c_int, // means_of_death indicator
) {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    shinqlx_g_damage_intern(
        main_engine.deref(),
        target,
        inflictor,
        attacker,
        dir,
        pos,
        damage,
        dflags,
        means_of_death,
    );
}

#[cfg_attr(not(test), inline)]
#[allow(clippy::too_many_arguments)]
fn shinqlx_g_damage_intern<T>(
    main_engine: &T,
    target: *mut gentity_t,
    inflictor: *mut gentity_t,
    attacker: *mut gentity_t,
    dir: *mut vec3_t,
    pos: *mut vec3_t,
    damage: c_int,
    dflags: c_int,
    means_of_death: c_int,
) where
    T: RegisterDamage<c_int, c_int, c_int>,
{
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

    let Ok(target_entity) = GameEntity::try_from(target) else {
        return;
    };

    if attacker.is_null() {
        damage_dispatcher(
            target_entity.get_entity_id(),
            None,
            damage,
            dflags,
            means_of_death,
        );
        return;
    }

    match GameEntity::try_from(attacker) {
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
    }
}

#[cfg(test)]
#[cfg_attr(test, mockall::automock)]
#[allow(dead_code)]
pub(crate) mod hooks {
    use super::Client;
    use super::GameEntity;

    pub(crate) fn shinqlx_execute_client_command(
        _client: Option<Client>,
        _cmd: String,
        _client_ok: bool,
    ) {
    }
    pub(crate) fn shinqlx_send_server_command(_client: Option<Client>, _cmd: String) {}
    pub(crate) fn shinqlx_drop_client(_client: &mut Client, _reason: String) {}
    pub(crate) fn shinqlx_client_spawn(_game_entity: GameEntity) {}
    pub(crate) fn shinqlx_set_configstring(_index: u32, _value: String) {}
}

#[cfg(test)]
mod hooks_tests {
    use super::MAIN_ENGINE;
    use crate::activator::MockActivator;
    use crate::client::MockClient;
    use crate::game_client::MockGameClient;
    use crate::game_entity::MockGameEntity;
    use crate::hooks::{
        shinqlx_client_connect_intern, shinqlx_client_spawn_intern, shinqlx_com_printf_intern,
        shinqlx_drop_client, shinqlx_execute_client_command, shinqlx_g_damage_intern,
        shinqlx_g_initgame, shinqlx_g_runframe_intern, shinqlx_g_shutdowngame,
        shinqlx_send_server_command, shinqlx_set_configstring_intern,
        shinqlx_sv_cliententerworld_intern, shinqlx_sv_spawnserver_intern,
        shinqlx_sys_setmoduleoffset,
    };
    use crate::hooks::{shinqlx_cmd_addcommand, shinqlx_g_startkamikaze};
    use crate::prelude::*;
    use crate::pyminqlx::mock_python::{
        client_command_dispatcher_context, client_connect_dispatcher_context,
        client_disconnect_dispatcher_context, client_loaded_dispatcher_context,
        client_spawn_dispatcher_context, console_print_dispatcher_context,
        damage_dispatcher_context, frame_dispatcher_context, kamikaze_explode_dispatcher_context,
        kamikaze_use_dispatcher_context, new_game_dispatcher_context,
        server_command_dispatcher_context, set_configstring_dispatcher_context,
    };
    use crate::quake_live_engine::MockQuakeEngine;
    use core::ffi::{c_char, CStr};
    use rstest::*;

    unsafe extern "C" fn dummy_function() {}

    #[test]
    #[serial]
    fn add_command_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        shinqlx_cmd_addcommand("\0".as_ptr() as *const c_char, dummy_function);
    }

    #[test]
    #[serial]
    fn add_command_with_main_engine_already_initiailized_command_empty() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_is_common_initialized()
            .return_const(true);
        mock_engine.expect_add_command().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_cmd_addcommand("\0".as_ptr() as *const c_char, dummy_function);
    }

    #[test]
    #[serial]
    fn add_command_with_main_engine_already_initiailized() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_is_common_initialized()
            .return_const(true);
        #[allow(clippy::fn_address_comparisons)]
        mock_engine
            .expect_add_command()
            .withf(|cmd, &func| cmd == "slap" && func == dummy_function)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_cmd_addcommand("slap\0".as_ptr() as *const c_char, dummy_function);
    }

    #[test]
    #[serial]
    fn add_command_with_main_engine_not_initiailized_command_non_empty() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_is_common_initialized()
            .return_const(false);
        mock_engine
            .expect_initialize_static()
            .returning(|| Ok(()))
            .times(1);
        #[allow(clippy::fn_address_comparisons)]
        mock_engine
            .expect_add_command()
            .withf(|cmd, &func| cmd == "slap" && func == dummy_function)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_cmd_addcommand("slap\0".as_ptr() as *const c_char, dummy_function);
    }

    #[test]
    #[serial]
    #[should_panic]
    fn add_command_with_main_engine_already_initiailized_init_returns_err() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_is_common_initialized()
            .return_const(false);
        mock_engine
            .expect_initialize_static()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_cmd_addcommand("slap\0".as_ptr() as *const c_char, dummy_function);
    }

    #[test]
    #[serial]
    fn sys_setmoduleoffset_no_main_engine() {
        MAIN_ENGINE.store(None);
        shinqlx_sys_setmoduleoffset("qagame\0".as_ptr() as *const c_char, dummy_function);
    }

    #[test]
    #[serial]
    fn sys_setmoduleoffset_vm_init_ok() {
        let mut mock_engine = MockQuakeEngine::new();
        #[allow(clippy::fn_address_comparisons)]
        mock_engine
            .expect_set_module_offset()
            .withf(|module_name, &offset| module_name == "qagame" && offset == dummy_function)
            .times(1);
        mock_engine
            .expect_initialize_vm()
            .withf(|&offset| offset == dummy_function as usize)
            .returning(|_offset| Ok(()))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_sys_setmoduleoffset("qagame\0".as_ptr() as *const c_char, dummy_function);
    }

    #[test]
    #[serial]
    #[should_panic]
    fn sys_setmoduleoffset_vm_init_returns_err() {
        let mut mock_engine = MockQuakeEngine::new();
        #[allow(clippy::fn_address_comparisons)]
        mock_engine
            .expect_set_module_offset()
            .withf(|module_name, &offset| module_name == "qagame" && offset == dummy_function)
            .times(1);
        mock_engine
            .expect_initialize_vm()
            .withf(|&offset| offset == dummy_function as usize)
            .returning(|_offset| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_sys_setmoduleoffset("qagame\0".as_ptr() as *const c_char, dummy_function);
    }

    #[test]
    #[serial]
    fn init_game_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        shinqlx_g_initgame(42, 21, 0);
    }

    #[test]
    #[serial]
    fn init_game_without_restart() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_init_game()
            .withf(|&level_time, &random_seed, &restart| {
                level_time == 42 && random_seed == 21 && restart == 0
            })
            .times(1);
        mock_engine.expect_set_tag().times(1);
        mock_engine.expect_initialize_cvars().times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx.expect().times(0);

        shinqlx_g_initgame(42, 21, 0);
    }

    #[test]
    #[serial]
    fn init_game_with_restart() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_init_game()
            .withf(|&level_time, &random_seed, &restart| {
                level_time == 42 && random_seed == 21 && restart == 1
            })
            .times(1);
        mock_engine.expect_set_tag().times(1);
        mock_engine.expect_initialize_cvars().times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx
            .expect()
            .withf(|&restart| restart)
            .times(1);

        shinqlx_g_initgame(42, 21, 1);
    }

    #[test]
    #[serial]
    fn shut_down_game_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        shinqlx_g_shutdowngame(42);
    }

    #[test]
    #[serial]
    fn shut_down_game_unhooks_vm() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_unhook_vm().times(1);
        mock_engine
            .expect_shutdown_game()
            .withf(|&restart| restart == 42)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_g_shutdowngame(42);
    }

    #[test]
    #[serial]
    fn execute_client_command_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        shinqlx_execute_client_command(None, "cp asdf", true);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_none_client_non_empty_cmd() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_client_command()
            .withf(|client, cmd, &client_ok| {
                client.is_none() && cmd == "cp asdf" && client_ok.into()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_execute_client_command(None, "cp asdf", true);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_not_ok_client_non_empty_cmd() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_client_command()
            .withf(|client, cmd, &client_ok| {
                client.is_some() && cmd == "cp asdf" && !<qboolean as Into<bool>>::into(client_ok)
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        let mock_client = MockClient::new();

        shinqlx_execute_client_command(Some(mock_client), "cp asdf", false);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_ok_client_without_gentity_non_empty_cmd() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_client_command()
            .withf(|client, cmd, &client_ok| {
                client.is_some() && cmd == "cp asdf" && client_ok.into()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const(false)
            .times(1);

        shinqlx_execute_client_command(Some(mock_client), "cp asdf", true);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_ok_client_with_gentity_non_empty_cmd_dispatcher_returns_none() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_execute_client_command().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);
        let client_command_ctx = client_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf(|&client_id, cmd| client_id == 42 && cmd == "cp asdf")
            .times(1);

        shinqlx_execute_client_command(Some(mock_client), "cp asdf", true);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_ok_client_with_gentity_non_empty_cmd_dispatcher_returns_modified_string(
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_client_command()
            .withf(|client, cmd, &client_ok| {
                client.is_some() && cmd == "cp modified" && client_ok.into()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);
        let client_command_ctx = client_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf(|&client_id, cmd| client_id == 42 && cmd == "cp asdf")
            .return_const(Some("cp modified".into()))
            .times(1);

        shinqlx_execute_client_command(Some(mock_client), "cp asdf", true);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_ok_client_with_gentity_non_empty_cmd_dispatcher_returns_empty_string(
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_execute_client_command().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);
        let client_command_ctx = client_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf(|&client_id, cmd| client_id == 42 && cmd == "cp asdf")
            .return_const(Some("".into()))
            .times(1);

        shinqlx_execute_client_command(Some(mock_client), "cp asdf", true);
    }

    #[test]
    #[serial]
    fn send_server_command_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        shinqlx_send_server_command(None, "cp asdf");
    }

    #[test]
    #[serial]
    fn send_server_command_for_none_client_non_empty_cmd_dispatcher_returns_none() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_send_server_command().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf(|&client_id, cmd| client_id.is_none() && cmd == "cp asdf")
            .return_const(None)
            .times(1);

        shinqlx_send_server_command(None, "cp asdf");
    }

    #[test]
    #[serial]
    fn send_server_command_for_none_client_non_empty_cmd_dispatcher_returns_modified_cmd() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_send_server_command()
            .withf(|client, cmd| client.is_none() && cmd == "cp modified")
            .times(1);
        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf(|&client_id, cmd| client_id.is_none() && cmd == "cp asdf")
            .return_const(Some("cp modified".into()))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_send_server_command(None, "cp asdf");
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_without_gentity_non_empty_cmd() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_send_server_command()
            .withf(|client, cmd| client.is_some() && cmd == "cp asdf")
            .times(1);
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const(false)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        shinqlx_send_server_command(Some(mock_client), "cp asdf");
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_with_gentity_non_empty_cmd_dispatcher_returns_none() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_send_server_command().times(0);
        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf(|&client_id, cmd| client_id == Some(42) && cmd == "cp asdf")
            .return_const(None)
            .times(1);

        shinqlx_send_server_command(Some(mock_client), "cp asdf");
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_with_gentity_non_empty_cmd_dispatcher_returns_modified_string(
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_send_server_command()
            .withf(|client, cmd| client.is_some() && cmd == "cp modified")
            .times(1);

        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf(|&client_id, cmd| client_id == Some(42) && cmd == "cp asdf")
            .return_const(Some("cp modified".into()))
            .times(1);

        shinqlx_send_server_command(Some(mock_client), "cp asdf");
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_with_gentity_non_empty_cmd_dispatcher_returns_empty_string() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_send_server_command().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let mut mock_client = MockClient::new();
        mock_client.expect_has_gentity().return_const(true).times(1);
        mock_client.expect_get_client_id().return_const(42).times(1);
        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf(|&client_id, cmd| client_id == Some(42) && cmd == "cp asdf")
            .return_const(Some("".into()))
            .times(1);

        shinqlx_send_server_command(Some(mock_client), "cp asdf");
    }

    #[test]
    #[serial]
    fn client_enter_world_for_unprimed_client() {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const_st(clientState_t::CS_ZOMBIE)
            .times(1);
        mock_client
            .expect_has_gentity()
            .return_const_st(true)
            .times(1);
        let mut usercmd = UserCmdBuilder::default().build().unwrap();
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_client_enter_world()
            .return_const_st(())
            .times(1);
        let client_loaded_ctx = client_loaded_dispatcher_context();
        client_loaded_ctx.expect().times(0);

        shinqlx_sv_cliententerworld_intern(
            &mock_engine,
            mock_client,
            &mut usercmd as *mut usercmd_t,
        );
    }

    #[test]
    #[serial]
    fn client_enter_world_for_primed_client_without_gentity() {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const_st(clientState_t::CS_PRIMED)
            .times(1);
        mock_client
            .expect_has_gentity()
            .return_const_st(false)
            .times(1);
        let mut usercmd = UserCmdBuilder::default().build().unwrap();
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_client_enter_world()
            .return_const_st(())
            .times(1);
        let client_loaded_ctx = client_loaded_dispatcher_context();
        client_loaded_ctx.expect().times(0);

        shinqlx_sv_cliententerworld_intern(
            &mock_engine,
            mock_client,
            &mut usercmd as *mut usercmd_t,
        );
    }

    #[test]
    #[serial]
    fn client_enter_world_for_primed_client_with_gentity_informs_python() {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const_st(clientState_t::CS_PRIMED)
            .times(1);
        mock_client
            .expect_has_gentity()
            .return_const_st(true)
            .times(1);
        mock_client
            .expect_get_client_id()
            .return_const_st(42)
            .times(1);
        let mut usercmd = UserCmdBuilder::default().build().unwrap();
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_client_enter_world()
            .return_const_st(())
            .times(1);
        let client_loaded_ctx = client_loaded_dispatcher_context();
        client_loaded_ctx
            .expect()
            .withf_st(|&client_id| client_id == 42)
            .return_const_st(())
            .times(1);

        shinqlx_sv_cliententerworld_intern(
            &mock_engine,
            mock_client,
            &mut usercmd as *mut usercmd_t,
        );
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
    fn set_configstring_for_undispatched_index(#[case] test_index: u32) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_set_configstring()
            .withf_st(move |&index, value| {
                index == test_index.try_into().unwrap() && value == "some value"
            })
            .return_const_st(())
            .times(1);

        let set_configstring_dispatcher_ctx = set_configstring_dispatcher_context();
        set_configstring_dispatcher_ctx.expect().times(0);

        shinqlx_set_configstring_intern(&mock_engine, test_index, "some value".into());
    }

    #[test]
    #[serial]
    fn set_confgistring_dispatcher_returns_none() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_set_configstring().times(0);

        let set_configstring_dispatcher_ctx = set_configstring_dispatcher_context();
        set_configstring_dispatcher_ctx
            .expect()
            .withf_st(|&index, value| index == 42 && value == "some value")
            .return_const_st(None)
            .times(1);

        shinqlx_set_configstring_intern(&mock_engine, 42, "some value".into());
    }

    #[test]
    #[serial]
    fn set_confgistring_dispatcher_returns_modified_string() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_set_configstring()
            .withf_st(move |&index, value| index == 42 && value == "other value")
            .return_const_st(())
            .times(1);

        let set_configstring_dispatcher_ctx = set_configstring_dispatcher_context();
        set_configstring_dispatcher_ctx
            .expect()
            .withf_st(|&index, value| index == 42 && value == "some value")
            .return_const_st(Some("other value".into()))
            .times(1);

        shinqlx_set_configstring_intern(&mock_engine, 42, "some value".into());
    }

    #[test]
    #[serial]
    fn set_confgistring_dispatcher_returns_unmodified_string() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_set_configstring()
            .withf_st(move |&index, value| index == 42 && value == "some value")
            .return_const_st(())
            .times(1);

        let set_configstring_dispatcher_ctx = set_configstring_dispatcher_context();
        set_configstring_dispatcher_ctx
            .expect()
            .withf_st(|&index, value| index == 42 && value == "some value")
            .return_const_st(Some("some value".into()))
            .times(1);

        shinqlx_set_configstring_intern(&mock_engine, 42, "some value".into());
    }

    #[test]
    #[serial]
    fn drop_client_is_dispatched_and_original_function_called() {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_disconnect()
            .withf_st(|reason| reason == "disconnected.")
            .return_const_st(())
            .times(1);
        mock_client.expect_get_client_id().return_const_st(42);

        let client_disconnect_dispatcher_ctx = client_disconnect_dispatcher_context();
        client_disconnect_dispatcher_ctx
            .expect()
            .withf_st(|&client_id, reason| client_id == 42 && reason == "disconnected.")
            .return_const_st(())
            .times(1);

        shinqlx_drop_client(&mut mock_client, "disconnected.");
    }

    #[test]
    #[serial]
    fn com_printf_when_dispatcher_returns_none() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_com_printf().times(0);
        let mock_console_print_dispatcher_ctx = console_print_dispatcher_context();
        mock_console_print_dispatcher_ctx
            .expect()
            .withf_st(|msg| msg == "Hello World!")
            .return_const_st(None)
            .times(1);

        shinqlx_com_printf_intern(&mock_engine, "Hello World!".into());
    }

    #[test]
    #[serial]
    fn com_printf_when_dispatcher_returns_some_value() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_com_printf()
            .withf_st(|msg| msg == "Hello World!")
            .return_const_st(())
            .times(1);
        let mock_console_print_dispatcher_ctx = console_print_dispatcher_context();
        mock_console_print_dispatcher_ctx
            .expect()
            .withf_st(|msg| msg == "Hello World!")
            .return_const_st(Some("Hello you!".into()))
            .times(1);

        shinqlx_com_printf_intern(&mock_engine, "Hello World!".into());
    }

    #[test]
    #[serial]
    fn sv_spawnserver_forwards_to_python() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_spawn_server()
            .withf_st(|server_str, &kill_bots| server_str == "l33t ql server" && kill_bots)
            .return_const_st(())
            .times(1);

        let mock_new_game_dispatcher_ctx = new_game_dispatcher_context();
        mock_new_game_dispatcher_ctx
            .expect()
            .withf_st(|&restart| !restart)
            .return_const_st(())
            .times(1);

        shinqlx_sv_spawnserver_intern(&mock_engine, "l33t ql server".into(), true);
    }

    #[test]
    #[serial]
    fn g_runframe_forwards_to_python() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_run_frame()
            .withf_st(|&time| time == 42)
            .return_const_st(())
            .times(1);

        let mock_frame_dispatcher_ctx = frame_dispatcher_context();
        mock_frame_dispatcher_ctx
            .expect()
            .return_const_st(())
            .times(1);

        shinqlx_g_runframe_intern(&mock_engine, 42);
    }

    #[test]
    fn client_connect_not_first_time_client() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_client_connect()
            .withf_st(|&client_num, &first_time, &is_bot| {
                client_num == 42 && !first_time && !is_bot
            })
            .return_const_st("\0".as_ptr() as *const c_char)
            .times(1);

        shinqlx_client_connect_intern(&mock_engine, 42, false, false);
    }

    #[test]
    #[serial]
    fn client_connect_first_time_client_dispatcher_returns_none() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_client_connect()
            .withf_st(|&client_num, &first_time, &is_bot| client_num == 42 && first_time && !is_bot)
            .return_const_st("\0".as_ptr() as *const c_char)
            .times(1);
        let client_connect_dispatcher_ctx = client_connect_dispatcher_context();
        client_connect_dispatcher_ctx
            .expect()
            .withf_st(|&client_num, &is_bot| client_num == 42 && !is_bot)
            .return_const_st(None)
            .times(1);

        shinqlx_client_connect_intern(&mock_engine, 42, true, false);
    }

    #[test]
    #[serial]
    fn client_connect_first_time_client_dispatcher_returns_string() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_client_connect().times(0);
        let client_connect_dispatcher_ctx = client_connect_dispatcher_context();
        client_connect_dispatcher_ctx
            .expect()
            .withf_st(|&client_num, &is_bot| client_num == 42 && !is_bot)
            .return_const_st(Some("you are banned from this server".into()))
            .times(1);

        let result = shinqlx_client_connect_intern(&mock_engine, 42, true, false);
        assert_eq!(
            unsafe { CStr::from_ptr(result) }.to_string_lossy(),
            "you are banned from this server"
        );
    }

    #[test]
    #[serial]
    fn client_connect_first_time_client_dispatcher_returns_some_for_bot() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_client_connect()
            .withf_st(|&client_num, &first_time, &is_bot| client_num == 42 && first_time && is_bot)
            .return_const_st("\0".as_ptr() as *const c_char)
            .times(1);
        let client_connect_dispatcher_ctx = client_connect_dispatcher_context();
        client_connect_dispatcher_ctx
            .expect()
            .withf_st(|&client_num, &is_bot| client_num == 42 && is_bot)
            .return_const_st(Some("we don't like bots here".into()))
            .times(1);

        shinqlx_client_connect_intern(&mock_engine, 42, true, true);
    }

    #[test]
    #[serial]
    fn client_spawn_forwards_to_ql_and_python() {
        let mut mock_entity = MockGameEntity::new();
        mock_entity
            .expect_get_entity_id()
            .return_const_st(42)
            .times(1);
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_client_spawn()
            .return_const_st(())
            .times(1);
        let client_spawn_dispatcher_ctx = client_spawn_dispatcher_context();
        client_spawn_dispatcher_ctx
            .expect()
            .withf_st(|&client_id| client_id == 42)
            .return_const_st(())
            .times(1);

        shinqlx_client_spawn_intern(&mock_engine, mock_entity);
    }

    #[test]
    #[serial]
    fn kamikaze_start_for_non_game_client() {
        let mut gentity = GEntityBuilder::default().build().unwrap();

        let mut mock_gentity = MockGameEntity::new();
        mock_gentity
            .expect_get_game_client()
            .returning_st(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        mock_gentity
            .expect_get_activator()
            .returning_st(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        mock_gentity
            .expect_start_kamikaze()
            .return_const_st(())
            .times(1);
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx.expect().return_once_st(|_| Ok(mock_gentity));
        let kamikaze_explode_dispatcher_ctx = kamikaze_explode_dispatcher_context();
        kamikaze_explode_dispatcher_ctx.expect().times(0);

        shinqlx_g_startkamikaze(&mut gentity as *mut gentity_t);
    }

    #[test]
    #[serial]
    fn kamikaze_start_for_existing_game_client_removes_kamikaze_flag() {
        let mut gentity = GEntityBuilder::default().build().unwrap();
        let mut mock_gentity = MockGameEntity::new();
        mock_gentity
            .expect_get_game_client()
            .times(1)
            .return_once_st(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_client_num().return_const_st(42);
                Ok(mock_game_client)
            });
        mock_gentity
            .expect_get_game_client()
            .times(1)
            .return_once_st(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_remove_kamikaze_flag()
                    .return_const_st(());
                Ok(mock_game_client)
            });
        mock_gentity
            .expect_get_game_client()
            .times(1)
            .return_once_st(|| Ok(MockGameClient::new()));
        mock_gentity
            .expect_start_kamikaze()
            .return_const_st(())
            .times(1);
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx.expect().return_once_st(|_| Ok(mock_gentity));
        let kamikaze_use_dispatcher_ctx = kamikaze_use_dispatcher_context();
        kamikaze_use_dispatcher_ctx
            .expect()
            .withf_st(|&client_id| client_id == 42)
            .return_const_st(())
            .times(1);
        let kamikaze_explode_dispatcher_ctx = kamikaze_explode_dispatcher_context();
        kamikaze_explode_dispatcher_ctx
            .expect()
            .withf_st(|&client_id, &is_used_on_demand| client_id == 42 && is_used_on_demand)
            .return_const_st(())
            .times(1);

        shinqlx_g_startkamikaze(&mut gentity as *mut gentity_t);
    }

    #[test]
    #[serial]
    fn kamikaze_start_for_activator_use() {
        let mut gentity = GEntityBuilder::default().build().unwrap();

        let mut mock_gentity = MockGameEntity::new();
        mock_gentity
            .expect_get_game_client()
            .returning_st(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        mock_gentity.expect_get_activator().return_once_st(|| {
            let mut mock_activator = MockActivator::new();
            mock_activator.expect_get_owner_num().return_const_st(42);
            Ok(mock_activator)
        });
        mock_gentity
            .expect_start_kamikaze()
            .return_const_st(())
            .times(1);
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx.expect().return_once_st(|_| Ok(mock_gentity));
        let kamikaze_explode_dispatcher_ctx = kamikaze_explode_dispatcher_context();
        kamikaze_explode_dispatcher_ctx
            .expect()
            .withf_st(|&client_id, &is_used_on_demand| client_id == 42 && !is_used_on_demand)
            .return_const_st(())
            .times(1);

        shinqlx_g_startkamikaze(&mut gentity as *mut gentity_t);
    }

    #[test]
    #[serial]
    fn g_damage_for_null_target_is_not_forwarded() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_register_damage()
            .withf_st(
                |&target, &inflictor, &attacker, &dir, &pos, &damage, &dflags, &means_of_death| {
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
            .return_const_st(())
            .times(1);
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx
            .expect()
            .return_once_st(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized));

        let damage_dispatcher_ctx = damage_dispatcher_context();
        damage_dispatcher_ctx.expect().times(0);

        shinqlx_g_damage_intern(
            &mock_engine,
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

    #[test]
    #[serial]
    fn g_damage_for_null_attacker() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_register_damage()
            .withf_st(
                |&target, &inflictor, &attacker, &dir, &pos, &damage, &dflags, &means_of_death| {
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
            .return_const_st(())
            .times(1);
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx
            .expect()
            .return_once_st(|_| {
                let mut mock_gentity = MockGameEntity::new();
                mock_gentity.expect_get_entity_id().return_const_st(42);
                Ok(mock_gentity)
            })
            .times(1);

        let damage_dispatcher_ctx = damage_dispatcher_context();
        damage_dispatcher_ctx
            .expect()
            .withf_st(|&target_id, &attacker, &damage, &dflags, &means_of_death| {
                target_id == 42
                    && attacker.is_none()
                    && damage == 666
                    && dflags == 0
                    && means_of_death == 0
            })
            .return_const_st(())
            .times(1);

        shinqlx_g_damage_intern(
            &mock_engine,
            ptr::null_mut() as *mut gentity_t,
            ptr::null_mut() as *mut gentity_t,
            ptr::null_mut() as *mut gentity_t,
            ptr::null_mut() as *mut vec3_t,
            ptr::null_mut() as *mut vec3_t,
            666,
            0,
            0,
        );
    }

    #[test]
    #[serial]
    fn g_damage_for_non_null_attacker_try_from_returns_err() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_register_damage()
            .withf_st(
                |&target, &inflictor, &attacker, &dir, &pos, &damage, &dflags, &means_of_death| {
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
            .return_const_st(())
            .times(1);
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx
            .expect()
            .return_once_st(|_| {
                let mut mock_gentity = MockGameEntity::new();
                mock_gentity.expect_get_entity_id().return_const_st(42);
                Ok(mock_gentity)
            })
            .times(1);
        try_from_ctx
            .expect()
            .return_once_st(|_| Err(QuakeLiveEngineError::MainEngineNotInitialized))
            .times(1);

        let damage_dispatcher_ctx = damage_dispatcher_context();
        damage_dispatcher_ctx
            .expect()
            .withf_st(|&target_id, &attacker, &damage, &dflags, &means_of_death| {
                target_id == 42
                    && attacker.is_none()
                    && damage == 666
                    && dflags == 16
                    && means_of_death == 7
            })
            .return_const_st(())
            .times(1);
        let mut attacker = GEntityBuilder::default().build().unwrap();

        shinqlx_g_damage_intern(
            &mock_engine,
            ptr::null_mut() as *mut gentity_t,
            ptr::null_mut() as *mut gentity_t,
            &mut attacker as *mut gentity_t,
            ptr::null_mut() as *mut vec3_t,
            ptr::null_mut() as *mut vec3_t,
            666,
            16,
            7,
        );
    }

    #[test]
    #[serial]
    fn g_damage_for_non_null_attacker_try_from_returns_ok() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_register_damage()
            .withf_st(
                |&target, &inflictor, &attacker, &dir, &pos, &damage, &dflags, &means_of_death| {
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
            .return_const_st(())
            .times(1);
        let try_from_ctx = MockGameEntity::try_from_context();
        try_from_ctx
            .expect()
            .return_once_st(|_| {
                let mut mock_gentity = MockGameEntity::new();
                mock_gentity.expect_get_entity_id().return_const_st(42);
                Ok(mock_gentity)
            })
            .times(1);
        try_from_ctx
            .expect()
            .return_once_st(|_| {
                let mut gentity = MockGameEntity::new();
                gentity.expect_get_entity_id().return_const_st(21);
                Ok(gentity)
            })
            .times(1);

        let damage_dispatcher_ctx = damage_dispatcher_context();
        damage_dispatcher_ctx
            .expect()
            .withf_st(|&target_id, &attacker, &damage, &dflags, &means_of_death| {
                target_id == 42
                    && attacker == Some(21)
                    && damage == 50
                    && dflags == 4
                    && means_of_death == 2
            })
            .return_const_st(())
            .times(1);
        let mut attacker = GEntityBuilder::default().build().unwrap();

        shinqlx_g_damage_intern(
            &mock_engine,
            ptr::null_mut() as *mut gentity_t,
            ptr::null_mut() as *mut gentity_t,
            &mut attacker as *mut gentity_t,
            ptr::null_mut() as *mut vec3_t,
            ptr::null_mut() as *mut vec3_t,
            50,
            4,
            2,
        );
    }
}
