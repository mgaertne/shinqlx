#[cfg(not(test))]
use crate::client::Client;
use crate::game_entity::GameEntity;
#[cfg(test)]
use crate::hooks::mock_python::{
    client_command_dispatcher, client_disconnect_dispatcher, client_loaded_dispatcher,
    console_print_dispatcher, frame_dispatcher, new_game_dispatcher, server_command_dispatcher,
    set_configstring_dispatcher,
};
#[cfg(test)]
use crate::hooks::MockClient as Client;
use crate::prelude::*;
#[cfg(not(test))]
use crate::pyminqlx::{
    client_command_dispatcher, client_disconnect_dispatcher, client_loaded_dispatcher,
    console_print_dispatcher, frame_dispatcher, new_game_dispatcher, server_command_dispatcher,
    set_configstring_dispatcher,
};
use crate::pyminqlx::{
    client_connect_dispatcher, client_spawn_dispatcher, damage_dispatcher,
    kamikaze_explode_dispatcher, kamikaze_use_dispatcher,
};
use crate::quake_live_engine::{
    AddCommand, ClientConnect, ClientEnterWorld, ClientSpawn, ComPrintf, ExecuteClientCommand,
    InitGame, RegisterDamage, RunFrame, SendServerCommand, SetConfigstring, SetModuleOffset,
    ShutdownGame, SpawnServer,
};
use crate::MAIN_ENGINE;
use alloc::string::String;
use core::ffi::{c_char, c_int, CStr, VaList, VaListImpl};
#[cfg(test)]
use mockall::{automock, mock};

pub(crate) fn shinqlx_cmd_addcommand(cmd: *const c_char, func: unsafe extern "C" fn()) {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
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
        main_engine.add_command(command, func);
    }
}

pub(crate) fn shinqlx_sys_setmoduleoffset(
    module_name: *const c_char,
    offset: unsafe extern "C" fn(),
) {
    let converted_module_name = unsafe { CStr::from_ptr(module_name) }.to_string_lossy();

    // We should be getting qagame, but check just in case.
    if converted_module_name.as_ref() != "qagame" {
        error!(target: "shinqlx", "Unknown module: {}", converted_module_name);
    }

    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    main_engine.set_module_offset(converted_module_name, offset);

    if let Err(err) = main_engine.initialize_vm(offset as usize) {
        error!(target: "shinqlx", "{:?}", err);
        error!(target: "shinqlx", "VM could not be initializied. Exiting.");
        panic!("VM could not be initializied. Exiting.");
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_InitGame(level_time: c_int, random_seed: c_int, restart: c_int) {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    main_engine.init_game(level_time, random_seed, restart);

    main_engine.set_tag();
    main_engine.initialize_cvars();

    if restart != 0 {
        new_game_dispatcher(true);
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_ShutdownGame(restart: c_int) {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    main_engine.unhook_vm().unwrap();
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
    T: Into<String>,
    U: Into<qboolean> + Into<bool> + Copy,
{
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    shinqlx_execute_client_command_intern(main_engine, client, cmd.into(), client_ok);
}

#[cfg_attr(not(test), inline)]
fn shinqlx_execute_client_command_intern<T, U>(
    main_engine: &T,
    client: Option<Client>,
    cmd: String,
    client_ok: U,
) where
    T: ExecuteClientCommand<Client, String, qboolean>,
    U: Into<qboolean> + Into<bool> + Copy,
{
    if cmd.is_empty() {
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
        let Some(dispatcher_result) = client_command_dispatcher(client_id, cmd) else {
            return;
        };
        dispatcher_result
    } else {
        cmd
    };

    if !passed_on_cmd_str.is_empty() {
        main_engine.execute_client_command(client, passed_on_cmd_str, client_ok.into());
    }
}

#[no_mangle]
pub unsafe extern "C" fn ShiNQlx_SV_SendServerCommand(
    client: *const client_t,
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
        let safe_client = Client::try_from(client);
        if safe_client.is_ok() {
            shinqlx_send_server_command(safe_client.ok(), cmd);
        }
    }
}

pub(crate) fn shinqlx_send_server_command<T>(client: Option<Client>, cmd: T)
where
    T: Into<String>,
{
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    shinqlx_send_server_command_intern(main_engine, client, cmd.into());
}

#[cfg_attr(not(test), inline)]
fn shinqlx_send_server_command_intern<T>(main_engine: &T, client: Option<Client>, cmd: String)
where
    T: SendServerCommand<Client, String>,
{
    if cmd.is_empty() {
        return;
    }
    let mut passed_on_cmd_str = cmd;

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

    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    shinqlx_sv_cliententerworld_intern(main_engine, safe_client, cmd);
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
    // Indices 16 and 66X are spammed a ton every frame for some reason,
    // so we add some exceptions for those. I don't think we should have any
    // use for those particular ones anyway. If we don't do this, we get
    // like a 25% increase in CPU usage on an empty server.
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    #[allow(clippy::unnecessary_to_owned)]
    shinqlx_set_configstring_intern(main_engine, index.into(), value.as_ref().to_string());
}

#[cfg_attr(not(test), inline)]
fn shinqlx_set_configstring_intern<T>(main_engine: &T, index: u32, value: String)
where
    T: SetConfigstring<c_int, String>,
{
    let Ok(c_index) = index.try_into() else {
        return;
    };

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

#[allow(unused_mut)]
#[no_mangle]
pub unsafe extern "C" fn ShiNQlx_Com_Printf(fmt: *const c_char, mut fmt_args: ...) {
    extern "C" {
        fn vsnprintf(s: *mut c_char, n: usize, format: *const c_char, arg: VaList) -> c_int;
    }

    let mut buffer: [u8; MAX_MSGLEN as usize] = [0; MAX_MSGLEN as usize];
    let result = vsnprintf(
        buffer.as_mut_ptr() as *mut c_char,
        buffer.len(),
        fmt,
        fmt_args.as_va_list(),
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
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    shinqlx_com_printf_intern(main_engine, msg.as_ref().to_string());
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

    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    shinqlx_sv_spawnserver_intern(main_engine, server_str, kill_bots);
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

#[no_mangle]
pub extern "C" fn ShiNQlx_G_RunFrame(time: c_int) {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    shinqlx_g_runframe_intern(main_engine, time);
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

#[no_mangle]
pub extern "C" fn ShiNQlx_ClientConnect(
    client_num: c_int,
    first_time: qboolean,
    is_bot: qboolean,
) -> *const c_char {
    if first_time.into() {
        if let Some(res) = client_connect_dispatcher(client_num, is_bot.into()) {
            if !<qboolean as Into<bool>>::into(is_bot) {
                return unsafe { to_return_string(client_num, res) };
            }
        }
    }

    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return core::ptr::null();
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return core::ptr::null();
    };

    main_engine.client_connect(client_num, first_time, is_bot)
}

#[no_mangle]
pub extern "C" fn ShiNQlx_ClientSpawn(ent: *mut gentity_t) {
    let Some(game_entity): Option<GameEntity> = ent.try_into().ok() else {
        return;
    };

    shinqlx_client_spawn(game_entity)
}

pub(crate) fn shinqlx_client_spawn(mut game_entity: GameEntity) {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    main_engine.client_spawn(&mut game_entity);

    // Since we won't ever stop the real function from being called,
    // we trigger the event after calling the real one. This will allow
    // us to set weapons and such without it getting overriden later.
    client_spawn_dispatcher(game_entity.get_entity_id());
}

#[allow(non_snake_case)]
pub extern "C" fn ShiNQlx_G_StartKamikaze(ent: *mut gentity_t) {
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

#[allow(non_snake_case)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn ShiNQlx_G_Damage(
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
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

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
#[automock]
#[allow(dead_code)]
mod python {
    pub(crate) fn client_command_dispatcher(_client_id: i32, _cmd: String) -> Option<String> {
        None
    }
    pub(crate) fn server_command_dispatcher(
        _client_id: Option<i32>,
        _cmd: String,
    ) -> Option<String> {
        None
    }
    pub(crate) fn client_loaded_dispatcher(_client_id: i32) {}

    pub(crate) fn set_configstring_dispatcher(_index: u32, _value: String) -> Option<String> {
        None
    }

    pub(crate) fn client_disconnect_dispatcher(_client_id: i32, _reason: String) {}

    pub(crate) fn console_print_dispatcher(_msg: String) -> Option<String> {
        None
    }

    pub(crate) fn new_game_dispatcher(_restart: bool) {}

    pub(crate) fn frame_dispatcher() {}
}

#[cfg(test)]
mock! {
    QuakeEngine{}
    impl ExecuteClientCommand<Client, String, qboolean> for QuakeEngine {
        fn execute_client_command(&self, client: Option<Client>, cmd: String, client_ok: qboolean);
    }
    impl SendServerCommand<Client, String > for QuakeEngine {
        fn send_server_command(&self, client: Option <Client>, cmd: String);
    }
    impl ClientEnterWorld<&mut Client> for QuakeEngine {
        fn client_enter_world(&self, client: &mut Client, cmd: * mut usercmd_t);
    }
    impl SetConfigstring<c_int, String> for QuakeEngine {
        fn set_configstring(&self, index: c_int, value: String);
    }
    impl ComPrintf<String> for QuakeEngine {
        fn com_printf(&self, msg: String);
    }
    impl SpawnServer<String, bool> for QuakeEngine {
        fn spawn_server(&self, server_str: String, kill_bots: bool);
    }
    impl RunFrame<c_int> for QuakeEngine {
        fn run_frame(&self, time: c_int);
    }
}

#[cfg(test)]
mock! {
    pub(crate) Client {
        pub(crate) fn has_gentity(&self) -> bool;
        pub(crate) fn get_client_id(&self) -> i32;
        pub(crate) fn get_state(&self) -> clientState_t;
        pub(crate) fn disconnect(&mut self, reason: String);
    }
    impl AsMut<client_t> for Client {
        fn as_mut(&mut self) -> &mut client_t;
    }
    impl AsRef<client_t> for Client {
        fn as_ref(&self) -> &client_t;
    }
    impl TryFrom<*mut client_t> for Client {
        type Error = QuakeLiveEngineError;
        fn try_from(client: *mut client_t) -> Result<Self, QuakeLiveEngineError>;
    }
    impl From<*const client_t> for Client {
        fn from(client: *const client_t) -> Self;
    }
}

#[cfg(test)]
mod hooks_tests {
    use crate::hooks::mock_python::{
        client_command_dispatcher_context, client_disconnect_dispatcher_context,
        client_loaded_dispatcher_context, console_print_dispatcher_context,
        frame_dispatcher_context, new_game_dispatcher_context, server_command_dispatcher_context,
        set_configstring_dispatcher_context,
    };
    use crate::hooks::{
        shinqlx_com_printf_intern, shinqlx_drop_client, shinqlx_execute_client_command_intern,
        shinqlx_g_runframe_intern, shinqlx_send_server_command_intern,
        shinqlx_set_configstring_intern, shinqlx_sv_cliententerworld_intern,
        shinqlx_sv_spawnserver_intern, MockClient, MockQuakeEngine,
    };
    use crate::prelude::*;
    use rstest::*;
    use serial_test::serial;

    #[test]
    fn execute_client_command_for_none_client_non_empty_cmd() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_execute_client_command()
            .withf_st(|client, cmd, &client_ok| {
                client.is_none() && cmd == "cp asdf" && client_ok.into()
            })
            .return_const_st(())
            .times(1);

        shinqlx_execute_client_command_intern(&mock, None, "cp asdf".into(), true);
    }

    #[test]
    fn execute_client_command_for_not_ok_client_non_empty_cmd() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_execute_client_command()
            .withf_st(|client, cmd, &client_ok| {
                client.is_some() && cmd == "cp asdf" && !<qboolean as Into<bool>>::into(client_ok)
            })
            .return_const_st(())
            .times(1);
        let mock_client = MockClient::new();

        shinqlx_execute_client_command_intern(&mock, Some(mock_client), "cp asdf".into(), false);
    }

    #[test]
    fn execute_client_command_for_ok_client_without_gentity_non_empty_cmd() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_execute_client_command()
            .withf_st(|client, cmd, &client_ok| {
                client.is_some() && cmd == "cp asdf" && client_ok.into()
            })
            .return_const_st(())
            .times(1);
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const_st(false)
            .times(1);

        shinqlx_execute_client_command_intern(&mock, Some(mock_client), "cp asdf".into(), true);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_ok_client_with_gentity_non_empty_cmd_dispatcher_returns_none() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_execute_client_command().times(0);
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const_st(true)
            .times(1);
        mock_client
            .expect_get_client_id()
            .return_const_st(42)
            .times(1);
        let client_command_ctx = client_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf_st(|&client_id, cmd| client_id == 42 && cmd == "cp asdf")
            .return_const_st(None)
            .times(1);

        shinqlx_execute_client_command_intern(&mock, Some(mock_client), "cp asdf".into(), true);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_ok_client_with_gentity_non_empty_cmd_dispatcher_returns_modified_string(
    ) {
        let mut mock = MockQuakeEngine::new();
        mock.expect_execute_client_command()
            .withf_st(|client, cmd, &client_ok| {
                client.is_some() && cmd == "cp modified" && client_ok.into()
            })
            .return_const_st(())
            .times(1);

        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const_st(true)
            .times(1);
        mock_client
            .expect_get_client_id()
            .return_const_st(42)
            .times(1);
        let client_command_ctx = client_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf_st(|&client_id, cmd| client_id == 42 && cmd == "cp asdf")
            .return_const_st(Some("cp modified".into()))
            .times(1);

        shinqlx_execute_client_command_intern(&mock, Some(mock_client), "cp asdf".into(), true);
    }

    #[test]
    #[serial]
    fn execute_client_command_for_ok_client_with_gentity_non_empty_cmd_dispatcher_returns_empty_string(
    ) {
        let mut mock = MockQuakeEngine::new();
        mock.expect_execute_client_command().times(0);

        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const_st(true)
            .times(1);
        mock_client
            .expect_get_client_id()
            .return_const_st(42)
            .times(1);
        let client_command_ctx = client_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf_st(|&client_id, cmd| client_id == 42 && cmd == "cp asdf")
            .return_const_st(Some("".into()))
            .times(1);

        shinqlx_execute_client_command_intern(&mock, Some(mock_client), "cp asdf".into(), true);
    }

    #[test]
    #[serial]
    fn send_server_command_for_none_client_non_empty_cmd_dispatcher_returns_none() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_send_server_command().times(0);
        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf_st(|&client_id, cmd| client_id.is_none() && cmd == "cp asdf")
            .return_const_st(None)
            .times(1);

        shinqlx_send_server_command_intern(&mock, None, "cp asdf".into());
    }

    #[test]
    #[serial]
    fn send_server_command_for_none_client_non_empty_cmd_dispatcher_returns_modified_cmd() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_send_server_command()
            .withf_st(|client, cmd| client.is_none() && cmd == "cp modified")
            .return_const_st(())
            .times(1);
        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf_st(|&client_id, cmd| client_id.is_none() && cmd == "cp asdf")
            .return_const_st(Some("cp modified".into()))
            .times(1);

        shinqlx_send_server_command_intern(&mock, None, "cp asdf".into());
    }

    #[test]
    fn send_server_command_for_client_without_gentity_non_empty_cmd() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_send_server_command()
            .withf_st(|client, cmd| client.is_some() && cmd == "cp asdf")
            .return_const_st(())
            .times(1);
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const_st(false)
            .times(1);

        shinqlx_send_server_command_intern(&mock, Some(mock_client), "cp asdf".into());
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_with_gentity_non_empty_cmd_dispatcher_returns_none() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_send_server_command().times(0);
        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const_st(true)
            .times(1);
        mock_client
            .expect_get_client_id()
            .return_const_st(42)
            .times(1);
        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf_st(|&client_id, cmd| client_id == Some(42) && cmd == "cp asdf")
            .return_const_st(None)
            .times(1);

        shinqlx_send_server_command_intern(&mock, Some(mock_client), "cp asdf".into());
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_with_gentity_non_empty_cmd_dispatcher_returns_modified_string(
    ) {
        let mut mock = MockQuakeEngine::new();
        mock.expect_send_server_command()
            .withf_st(|client, cmd| client.is_some() && cmd == "cp modified")
            .return_const_st(())
            .times(1);

        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const_st(true)
            .times(1);
        mock_client
            .expect_get_client_id()
            .return_const_st(42)
            .times(1);
        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf_st(|&client_id, cmd| client_id == Some(42) && cmd == "cp asdf")
            .return_const_st(Some("cp modified".into()))
            .times(1);

        shinqlx_send_server_command_intern(&mock, Some(mock_client), "cp asdf".into());
    }

    #[test]
    #[serial]
    fn send_server_command_for_client_with_gentity_non_empty_cmd_dispatcher_returns_empty_string() {
        let mut mock = MockQuakeEngine::new();
        mock.expect_send_server_command().times(0);

        let mut mock_client = MockClient::new();
        mock_client
            .expect_has_gentity()
            .return_const_st(true)
            .times(1);
        mock_client
            .expect_get_client_id()
            .return_const_st(42)
            .times(1);
        let client_command_ctx = server_command_dispatcher_context();
        client_command_ctx
            .expect()
            .withf_st(|&client_id, cmd| client_id == Some(42) && cmd == "cp asdf")
            .return_const_st(Some("".into()))
            .times(1);

        shinqlx_send_server_command_intern(&mock, Some(mock_client), "cp asdf".into());
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
    fn set_confgistring_for_undispatched_index(#[case] test_index: u32) {
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
    fn set_confgistring_dispatcher_returns_none() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_set_configstring()
            .withf_st(move |&index, value| index == 42 && value == "some value")
            .return_const_st(())
            .times(0);

        let set_configstring_dispatcher_ctx = set_configstring_dispatcher_context();
        set_configstring_dispatcher_ctx
            .expect()
            .withf_st(|&index, value| index == 42 && value == "some value")
            .return_const_st(None)
            .times(1);

        shinqlx_set_configstring_intern(&mock_engine, 42, "some value".into());
    }

    #[test]
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
}
