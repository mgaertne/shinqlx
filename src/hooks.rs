#[cfg(not(feature = "cdispatchers"))]
use crate::pyminqlx::{
    client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
    client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher, frame_dispatcher,
    kamikaze_explode_dispatcher, kamikaze_use_dispatcher, new_game_dispatcher,
    server_command_dispatcher, set_configstring_dispatcher,
};
use crate::quake_common::{
    cbufExec_t, clientState_t, client_t, gentity_t, qboolean, usercmd_t, AddCommand,
    CbufExecuteText, Client, ClientConnect, ClientEnterWorld, ClientSpawn, ComPrintf,
    ExecuteClientCommand, FindCVar, GameEntity, InitGame, QuakeLiveEngine, RunFrame,
    SendServerCommand, SetConfigstring, SetModuleOffset, SpawnServer, SV_TAGS_PREFIX,
};
use crate::{initialize_cvars, initialize_static, COMMON_INITIALIZED, CVARS_INITIALIZED};
use std::ffi::{c_char, c_int, CStr, CString};

fn set_tag() {
    let Some(sv_tags) = QuakeLiveEngine::find_cvar("sv_tags") else {
        return;
    };

    let sv_tags_string = sv_tags.get_string();

    if sv_tags_string.split(',').any(|x| x == SV_TAGS_PREFIX) {
        return;
    }

    let new_tags = if sv_tags_string.len() > 2 {
        format!("sv_tags \"{},{}\"", SV_TAGS_PREFIX, sv_tags_string)
    } else {
        format!("sv_tags \"{}\"", SV_TAGS_PREFIX)
    };
    QuakeLiveEngine::cbuf_execute_text(cbufExec_t::EXEC_INSERT, &new_tags);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_Cmd_AddCommand(cmd: *const c_char, func: unsafe extern "C" fn()) {
    if unsafe { !COMMON_INITIALIZED } {
        initialize_static();
    }

    let command = unsafe { CStr::from_ptr(cmd).to_string_lossy() };
    QuakeLiveEngine::add_command(command.as_ref(), func);
}

extern "C" {
    fn SearchVmFunctions();
    fn HookVm();
    fn InitializeVm();
    fn patch_vm();
}

#[no_mangle]
pub extern "C" fn ShiNQlx_Sys_SetModuleOffset(
    module_name: *const c_char,
    offset: unsafe extern "C" fn(),
) {
    let converted_module_name = unsafe { CStr::from_ptr(module_name).to_string_lossy() };
    QuakeLiveEngine::set_module_offset(converted_module_name.as_ref(), offset);

    if unsafe { !COMMON_INITIALIZED } {
        return;
    }
    unsafe {
        SearchVmFunctions();
        HookVm();
        InitializeVm();
        patch_vm();
    }
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn NewGameDispatcher(restart: c_int);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_InitGame(level_time: c_int, random_seed: c_int, restart: c_int) {
    QuakeLiveEngine::init_game(level_time, random_seed, restart);

    if unsafe { !CVARS_INITIALIZED } {
        set_tag();
    }

    initialize_cvars();

    if restart != 0 {
        #[cfg(not(feature = "cdispatchers"))]
        new_game_dispatcher(true);
        #[cfg(feature = "cdispatchers")]
        unsafe {
            NewGameDispatcher(restart)
        };
    }
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn ClientCommandDispatcher(client_id: c_int, cmd: *const c_char) -> *const c_char;
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_ExecuteClientCommand(
    client: *const client_t,
    cmd: *const c_char,
    client_ok: qboolean,
) {
    let rust_cmd = unsafe { CStr::from_ptr(cmd).to_string_lossy() };

    shinqlx_execute_client_command(client.try_into().ok(), rust_cmd.as_ref(), client_ok.into());
}

pub(crate) fn shinqlx_execute_client_command(client: Option<Client>, cmd: &str, client_ok: bool) {
    #[cfg(feature = "cdispatchers")]
    let res: *const c_char;
    #[cfg(feature = "cdispatchers")]
    let mut passed_on_cmd: *const c_char = CString::new(cmd).unwrap().into_raw();

    if let Some(safe_client) = &client {
        if client_ok && safe_client.has_gentity() {
            #[cfg(feature = "cdispatchers")]
            {
                res =
                    unsafe { ClientCommandDispatcher(safe_client.get_client_id(), passed_on_cmd) };
                if res.is_null() {
                    return;
                }
                passed_on_cmd = res;
            }

            #[cfg(not(feature = "cdispatchers"))]
            if let Some(passed_on_cmd) = client_command_dispatcher(safe_client.get_client_id(), cmd)
            {
                QuakeLiveEngine::execute_client_command(
                    client.as_ref(),
                    passed_on_cmd.as_str(),
                    client_ok,
                );
                return;
            };
        }
    }

    #[cfg(feature = "cdispatchers")]
    {
        let passed_on_cmd_str = unsafe { CStr::from_ptr(passed_on_cmd).to_string_lossy() };
        QuakeLiveEngine::execute_client_command(
            client.as_ref(),
            passed_on_cmd_str.as_ref(),
            client_ok,
        );
    }

    #[cfg(not(feature = "cdispatchers"))]
    QuakeLiveEngine::execute_client_command(client.as_ref(), cmd, client_ok);
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn ServerCommandDispatcher(client_id: c_int, command: *const c_char) -> *const c_char;
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_SendServerCommand(client: *const client_t, command: *const c_char) {
    let cmd = unsafe { CStr::from_ptr(command).to_string_lossy() };
    shinqlx_send_server_command(client.try_into().ok(), cmd.as_ref());
}

pub(crate) fn shinqlx_send_server_command(client: Option<Client>, cmd: &str) {
    let client_id = match &client {
        Some(safe_client) => safe_client.get_client_id(),
        None => -1,
    };

    #[cfg(feature = "cdispatchers")]
    {
        let c_cmd = CString::new(cmd).unwrap().into_raw();
        let res = unsafe { ServerCommandDispatcher(client_id, c_cmd) };

        if res.is_null() {
            return;
        }

        let result = unsafe { CStr::from_ptr(res).to_string_lossy() };
        QuakeLiveEngine::send_server_command(client, result.as_ref());
    }

    #[cfg(not(feature = "cdispatchers"))]
    if let Some(res) = server_command_dispatcher(client_id, cmd) {
        QuakeLiveEngine::send_server_command(client, res.as_str());
    }
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn ClientLoadedDispatcher(client_id: c_int);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_ClientEnterWorld(client: *const client_t, cmd: *const usercmd_t) {
    let Some(safe_client): Option<Client> = client.try_into().ok() else {
        return;
    };

    let state = safe_client.get_state();
    QuakeLiveEngine::client_enter_world(&safe_client, cmd);

    // gentity is NULL if map changed.
    // state is CS_PRIMED only if it's the first time they connect to the server,
    // otherwise the dispatcher would also go off when a game starts and such.
    {
        if safe_client.has_gentity() && state == clientState_t::CS_PRIMED as i32 {
            #[cfg(feature = "cdispatchers")]
            unsafe {
                ClientLoadedDispatcher(safe_client.get_client_id());
            }
            #[cfg(not(feature = "cdispatchers"))]
            client_loaded_dispatcher(safe_client.get_client_id());
        }
    }
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn SetConfigstringDispatcher(index: c_int, value: *const c_char) -> *const c_char;
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_SetConfigstring(index: c_int, value: *const c_char) {
    let safe_value = if value.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(value).to_str().unwrap_or("") }
    };

    shinqlx_set_configstring(index, safe_value);
}

pub(crate) fn shinqlx_set_configstring(index: i32, value: &str) {
    // Indices 16 and 66X are spammed a ton every frame for some reason,
    // so we add some exceptions for those. I don't think we should have any
    // use for those particular ones anyway. If we don't do this, we get
    // like a 25% increase in CPU usage on an empty server.
    if index == 16 || (662..670).contains(&index) {
        QuakeLiveEngine::set_configstring(&index, value);
        return;
    }

    #[cfg(feature = "cdispatchers")]
    {
        let value_cstring = CString::new(value).unwrap().into_raw();
        let res = unsafe { SetConfigstringDispatcher(index, value_cstring) };
        if res.is_null() {
            return;
        }

        let res_string = unsafe { CStr::from_ptr(res).to_string_lossy() };
        QuakeLiveEngine::set_configstring(&index, res_string.as_ref());
    }

    #[cfg(not(feature = "cdispatchers"))]
    if let Some(res) = set_configstring_dispatcher(index, value) {
        QuakeLiveEngine::set_configstring(&index, res.as_str());
    }
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn ClientDisconnectDispatcher(client_id: c_int, reason: *const c_char);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_DropClient(client: *const client_t, reason: *const c_char) {
    if let Ok(safe_client) = Client::try_from(client) {
        shinqlx_drop_client(&safe_client, unsafe {
            CStr::from_ptr(reason).to_str().unwrap_or("")
        });
    }
}

pub(crate) fn shinqlx_drop_client(client: &Client, reason: &str) {
    #[cfg(feature = "cdispatchers")]
    {
        let c_reason = CString::new(reason).unwrap().into_raw();
        unsafe {
            ClientDisconnectDispatcher(client.get_client_id(), c_reason);
        }
    }

    #[cfg(not(feature = "cdispatchers"))]
    client_disconnect_dispatcher(client.get_client_id(), reason);

    client.disconnect(reason);
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn ConsolePrintDispatcher(msg: *const c_char) -> *const c_char;
}

#[no_mangle]
pub extern "C" fn ShiNQlx_Com_Printf(msg: *const c_char) {
    let rust_msg = unsafe { CStr::from_ptr(msg).to_string_lossy() };
    shinqlx_com_printf(&rust_msg);
}

pub(crate) fn shinqlx_com_printf(msg: &str) {
    #[cfg(feature = "cdispatchers")]
    {
        let text = CString::new(msg).unwrap().into_raw();
        let res = unsafe { ConsolePrintDispatcher(text) };
        if res.is_null() {
            return;
        }

        QuakeLiveEngine::com_printf(msg);
    }

    #[cfg(not(feature = "cdispatchers"))]
    if let Some(_res) = console_print_dispatcher(msg) {
        QuakeLiveEngine::com_printf(msg);
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_SpawnServer(server: *const c_char, kill_bots: qboolean) {
    if let Ok(server_str) = unsafe { CStr::from_ptr(server).to_str() } {
        QuakeLiveEngine::spawn_server(server_str, kill_bots.into());

        #[cfg(feature = "cdispatchers")]
        unsafe {
            NewGameDispatcher(qboolean::qfalse.into());
        }

        #[cfg(not(feature = "cdispatchers"))]
        new_game_dispatcher(false);
    }
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn FrameDispatcher();
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_RunFrame(time: c_int) {
    #[cfg(feature = "cdispatchers")]
    unsafe {
        FrameDispatcher();
    }

    #[cfg(not(feature = "cdispatchers"))]
    frame_dispatcher();

    QuakeLiveEngine::run_frame(time);
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn ClientConnectDispatcher(client_num: c_int, is_bot: qboolean) -> *const c_char;
}

#[no_mangle]
pub extern "C" fn ShiNQlx_ClientConnect(
    client_num: c_int,
    first_time: qboolean,
    is_bot: qboolean,
) -> *const c_char {
    #[cfg(feature = "cdispatchers")]
    if first_time.into() {
        let res = unsafe { ClientConnectDispatcher(client_num, is_bot) };
        if !res.is_null() && !<qboolean as Into<bool>>::into(is_bot) {
            return res;
        }
    }

    let client_connect_result =
        QuakeLiveEngine::client_connect(client_num, first_time.into(), is_bot.into());

    #[cfg(not(feature = "cdispatchers"))]
    if first_time.into() {
        if let Some(res) = client_connect_dispatcher(client_num, is_bot.into()) {
            if !<qboolean as Into<bool>>::into(is_bot) {
                return CString::new(res).unwrap().into_raw();
            }
        }
    }

    match client_connect_result {
        None => std::ptr::null(),
        Some(message) => CString::new(message.as_ref()).unwrap().into_raw(),
    }
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn ClientSpawnDispatcher(ent: c_int);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_ClientSpawn(ent: *mut gentity_t) {
    let Some(game_entity): Option<GameEntity> = ent.try_into().ok() else {
        return;
    };

    shinqlx_client_spawn(game_entity)
}

pub(crate) fn shinqlx_client_spawn(game_entity: GameEntity) {
    QuakeLiveEngine::client_spawn(&game_entity);
    // Since we won't ever stop the real function from being called,
    // we trigger the event after calling the real one. This will allow
    // us to set weapons and such without it getting overriden later.
    #[cfg(feature = "cdispatchers")]
    unsafe {
        ClientSpawnDispatcher(game_entity.get_client_id());
    };

    #[cfg(not(feature = "cdispatchers"))]
    client_spawn_dispatcher(game_entity.get_client_id());
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn KamikazeUseDispatcher(client_id: c_int);
    fn KamikazeExplodeDispatcher(client_id: c_int, used_on_demand: c_int);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_StartKamikaze(ent: *mut gentity_t) {
    let Some(game_entity): Option<GameEntity> = ent.try_into().ok() else {
        return;
    };

    let client_id = if let Some(game_client) = game_entity.get_game_client() {
        game_client.get_client_num()
    } else if let Some(activator) = game_entity.get_activator() {
        activator.get_owner_num()
    } else {
        -1
    };

    if let Some(mut game_client) = game_entity.get_game_client() {
        game_client.remove_kamikaze_flag();
        #[cfg(feature = "cdispatchers")]
        unsafe {
            KamikazeUseDispatcher(client_id);
        }

        #[cfg(not(feature = "cdispatchers"))]
        kamikaze_use_dispatcher(client_id);
    }

    game_entity.start_kamikaze();

    if client_id == -1 {
        return;
    }

    #[cfg(feature = "cdispatchers")]
    unsafe {
        KamikazeExplodeDispatcher(client_id, game_entity.get_game_client().is_some() as c_int);
    }

    #[cfg(not(feature = "cdispatchers"))]
    kamikaze_explode_dispatcher(client_id, game_entity.get_game_client().is_some())
}
