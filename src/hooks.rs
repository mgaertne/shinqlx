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
    if !*COMMON_INITIALIZED.lock().unwrap() {
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

    if !*COMMON_INITIALIZED.lock().unwrap() {
        return;
    }
    unsafe {
        SearchVmFunctions();
        HookVm();
        InitializeVm();
        patch_vm();
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_InitGame(level_time: c_int, random_seed: c_int, restart: c_int) {
    QuakeLiveEngine::init_game(level_time, random_seed, restart);

    if *CVARS_INITIALIZED.lock().unwrap() {
        set_tag();
    }

    initialize_cvars();

    if restart == 0 {
        return;
    }

    unsafe { NewGameDispatcher(restart) };
}

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
    let res: *const c_char;
    let mut passed_on_cmd: *const c_char = CString::new(cmd).unwrap().into_raw();
    if let Some(safe_client) = &client {
        if client_ok && safe_client.has_gentity() {
            let client_id = safe_client.get_client_id();
            res = unsafe { ClientCommandDispatcher(client_id, passed_on_cmd) };
            if res.is_null() {
                return;
            }
            passed_on_cmd = res;
        }
    }

    let passed_on_cmd_str = unsafe { CStr::from_ptr(passed_on_cmd).to_string_lossy() };
    QuakeLiveEngine::execute_client_command(client.as_ref(), passed_on_cmd_str.as_ref(), client_ok);
}

extern "C" {
    fn ServerCommandDispatcher(client_id: c_int, command: *const c_char) -> *const c_char;
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_SendServerCommand(client: *const client_t, command: *const c_char) {
    let safe_client = client.try_into().ok();
    let cmd = unsafe { CStr::from_ptr(command).to_string_lossy() };
    shinqlx_send_server_command(safe_client, cmd.as_ref());
}

pub(crate) fn shinqlx_send_server_command(client: Option<Client>, cmd: &str) {
    let client_id = match &client {
        Some(safe_client) => safe_client.get_client_id(),
        None => -1,
    };

    let c_cmd = CString::new(cmd).unwrap().into_raw();
    let res = unsafe { ServerCommandDispatcher(client_id, c_cmd) };

    if res.is_null() {
        return;
    }

    let result = unsafe { CStr::from_ptr(res).to_string_lossy() };
    QuakeLiveEngine::send_server_command(client, result.as_ref());
}

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

    if !safe_client.has_gentity() || state != clientState_t::CS_PRIMED as i32 {
        return;
    }
    let client_id = safe_client.get_client_id();

    unsafe {
        ClientLoadedDispatcher(client_id);
    }
}

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
    let value_cstring = CString::new(value).unwrap().into_raw();
    let res = unsafe { SetConfigstringDispatcher(index, value_cstring) };
    if res.is_null() {
        return;
    }

    let res_string = unsafe { CStr::from_ptr(res).to_string_lossy() };
    QuakeLiveEngine::set_configstring(&index, res_string.as_ref());
}

extern "C" {
    fn ClientDisconnectDispatcher(client_id: c_int, reason: *const c_char);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_DropClient(client: *const client_t, reason: *const c_char) {
    let Some(safe_client): Option<Client> = client.try_into().ok() else {
        return;
    };

    shinqlx_drop_client(&safe_client, unsafe {
        CStr::from_ptr(reason).to_str().unwrap_or("")
    });
}

pub(crate) fn shinqlx_drop_client(client: &Client, reason: &str) {
    let c_reason = CString::new(reason).unwrap().into_raw();
    unsafe {
        ClientDisconnectDispatcher(client.get_client_id(), c_reason);
    }
    client.disconnect(reason);
}

extern "C" {
    fn ConsolePrintDispatcher(msg: *const c_char) -> *const c_char;
}

#[no_mangle]
pub extern "C" fn ShiNQlx_Com_Printf(msg: *const c_char) {
    let rust_msg = unsafe { CStr::from_ptr(msg).to_string_lossy() };
    shinqlx_com_printf(rust_msg.as_ref());
}

pub(crate) fn shinqlx_com_printf(msg: &str) {
    let text = CString::new(msg).unwrap().into_raw();
    let res = unsafe { ConsolePrintDispatcher(text) };
    if res.is_null() {
        return;
    }

    QuakeLiveEngine::com_printf(msg);
}

extern "C" {
    fn NewGameDispatcher(restart: c_int);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_SpawnServer(server: *const c_char, kill_bots: qboolean) {
    let Some(server_str) = (unsafe { CStr::from_ptr(server).to_str().ok() }) else { return; };

    QuakeLiveEngine::spawn_server(server_str, kill_bots.into());
    unsafe {
        NewGameDispatcher(qboolean::qfalse.into());
    }
}

extern "C" {
    fn FrameDispatcher();
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_RunFrame(time: c_int) {
    unsafe {
        FrameDispatcher();
    }
    QuakeLiveEngine::run_frame(time);
}

extern "C" {
    fn ClientConnectDispatcher(client_num: c_int, is_bot: qboolean) -> *const c_char;
}

#[no_mangle]
pub extern "C" fn ShiNQlx_ClientConnect(
    client_num: c_int,
    first_time: qboolean,
    is_bot: qboolean,
) -> *const c_char {
    if first_time.into() {
        let res = unsafe { ClientConnectDispatcher(client_num, is_bot) };
        if !res.is_null() && !<qboolean as Into<bool>>::into(is_bot) {
            return res;
        }
    }

    let client_connect_return =
        QuakeLiveEngine::client_connect(client_num, first_time.into(), is_bot.into());

    match client_connect_return {
        None => std::ptr::null(),
        Some(message) => CString::new(message.as_ref()).unwrap().into_raw(),
    }
}

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
    let client_id = game_entity.get_client_id();
    // Since we won't ever stop the real function from being called,
    // we trigger the event after calling the real one. This will allow
    // us to set weapons and such without it getting overriden later.
    unsafe {
        ClientSpawnDispatcher(client_id);
    };
}

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
        unsafe {
            KamikazeUseDispatcher(client_id);
        }
    }

    game_entity.start_kamikaze();

    if client_id == -1 {
        return;
    }

    unsafe {
        KamikazeExplodeDispatcher(client_id, game_entity.get_game_client().is_some() as c_int);
    }
}
