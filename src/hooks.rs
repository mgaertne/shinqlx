use crate::client::Client;
use crate::game_entity::GameEntity;
use crate::pyminqlx::{
    client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
    client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher, damage_dispatcher,
    frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher, new_game_dispatcher,
    server_command_dispatcher, set_configstring_dispatcher,
};
use crate::quake_live_engine::{
    AddCommand, ClientConnect, ClientEnterWorld, ClientSpawn, ComPrintf, ExecuteClientCommand,
    InitGame, RegisterDamage, RunFrame, SendServerCommand, SetConfigstring, SetModuleOffset,
    ShutdownGame, SpawnServer,
};
use crate::quake_types::clientState_t::CS_PRIMED;
use crate::quake_types::{
    client_t, gentity_t, qboolean, usercmd_t, vec3_t, MAX_CLIENTS, MAX_MSGLEN, MAX_STRING_CHARS,
};
use crate::MAIN_ENGINE;
use std::ffi::{c_char, c_int, CStr, VaList, VaListImpl};

pub(crate) fn shinqlx_cmd_addcommand(cmd: *const c_char, func: unsafe extern "C" fn()) {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    if !quake_live_engine.is_common_initialized() {
        quake_live_engine.initialize_static();
    }

    let command = unsafe { CStr::from_ptr(cmd) }.to_string_lossy();
    if !command.is_empty() {
        quake_live_engine.add_command(command.as_ref(), func);
    }
}

pub(crate) fn shinqlx_sys_setmoduleoffset(
    module_name: *const c_char,
    offset: unsafe extern "C" fn(),
) {
    let converted_module_name = unsafe { CStr::from_ptr(module_name) }.to_string_lossy();

    // We should be getting qagame, but check just in case.
    if converted_module_name.as_ref() != "qagame" {
        debug_println!(format!("Unknown module: {}", converted_module_name));
    }

    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    quake_live_engine.set_module_offset(converted_module_name.as_ref(), offset);

    if let Err(err) = quake_live_engine.initialize_vm(offset as usize) {
        debug_println!(format!("{:?}", err));
        debug_println!("VM could not be initializied. Exiting.");
        panic!("VM could not be initializied. Exiting.");
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_InitGame(level_time: c_int, random_seed: c_int, restart: c_int) {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    quake_live_engine.init_game(level_time, random_seed, restart);

    quake_live_engine.set_tag();
    quake_live_engine.initialize_cvars();

    if restart != 0 {
        new_game_dispatcher(true);
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_ShutdownGame(restart: c_int) {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };

    quake_live_engine.unhook_vm().unwrap();
    quake_live_engine.shutdown_game(restart);
}

pub(crate) fn shinqlx_sv_executeclientcommand(
    client: *mut client_t,
    cmd: *const c_char,
    client_ok: qboolean,
) {
    let rust_cmd = unsafe { CStr::from_ptr(cmd) }.to_string_lossy();
    if !rust_cmd.is_empty() {
        shinqlx_execute_client_command(
            Client::try_from(client).ok(),
            rust_cmd.as_ref(),
            client_ok.into(),
        );
    }
}

pub(crate) fn shinqlx_execute_client_command(
    mut client: Option<Client>,
    cmd: &str,
    client_ok: bool,
) {
    let passed_on_cmd_str = if client_ok
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
        cmd.into()
    };

    if !passed_on_cmd_str.is_empty() {
        let Some(quake_live_engine) = MAIN_ENGINE.get() else {
            return;
        };
        quake_live_engine.execute_client_command(
            client.as_mut(),
            passed_on_cmd_str.as_str(),
            client_ok,
        );
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
        dbg!("some formatting problem occurred");
    }

    let cmd = CStr::from_bytes_until_nul(&buffer)
        .unwrap()
        .to_string_lossy();
    if !cmd.is_empty() {
        if client.is_null() {
            shinqlx_send_server_command(None, cmd.as_ref());
        } else {
            let safe_client = Client::try_from(client);
            if safe_client.is_ok() {
                shinqlx_send_server_command(safe_client.ok(), cmd.as_ref());
            }
        }
    }
}

pub(crate) fn shinqlx_send_server_command(client: Option<Client>, cmd: &str) {
    let mut passed_on_cmd_str = cmd.to_string();

    match client.as_ref() {
        Some(safe_client) => {
            if safe_client.has_gentity() {
                let client_id = safe_client.get_client_id();
                if let Some(res) =
                    server_command_dispatcher(Some(client_id), passed_on_cmd_str.as_str())
                {
                    passed_on_cmd_str = res;
                }
            }
        }
        None => {
            if let Some(res) = server_command_dispatcher(None, passed_on_cmd_str.as_str()) {
                passed_on_cmd_str = res;
            }
        }
    }

    if !passed_on_cmd_str.is_empty() {
        let Some(quake_live_engine) = MAIN_ENGINE.get() else {
            return;
        };
        quake_live_engine.send_server_command(client, passed_on_cmd_str.as_str());
    }
}

pub(crate) fn shinqlx_sv_cliententerworld(client: *mut client_t, cmd: *mut usercmd_t) {
    let Some(mut safe_client): Option<Client> = client.try_into().ok() else {
        return;
    };

    let state = safe_client.get_state();
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    quake_live_engine.client_enter_world(&mut safe_client, cmd);

    // gentity is NULL if map changed.
    // state is CS_PRIMED only if it's the first time they connect to the server,
    // otherwise the dispatcher would also go off when a game starts and such.
    if safe_client.has_gentity() && state == CS_PRIMED {
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
    shinqlx_set_configstring(ql_index, safe_value.as_ref());
}

pub(crate) fn shinqlx_set_configstring(index: u32, value: &str) {
    // Indices 16 and 66X are spammed a ton every frame for some reason,
    // so we add some exceptions for those. I don't think we should have any
    // use for those particular ones anyway. If we don't do this, we get
    // like a 25% increase in CPU usage on an empty server.
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    if index == 16 || (662..670).contains(&index) {
        quake_live_engine.set_configstring(&index, value);
        return;
    }

    let Some(res) = set_configstring_dispatcher(index, value) else {
        return;
    };
    quake_live_engine.set_configstring(&index, res.as_str());
}

pub(crate) fn shinqlx_sv_dropclient(client: *mut client_t, reason: *const c_char) {
    let Ok(mut safe_client) = Client::try_from(client) else {
        return;
    };
    shinqlx_drop_client(
        &mut safe_client,
        unsafe { CStr::from_ptr(reason) }.to_string_lossy().as_ref(),
    );
}

pub(crate) fn shinqlx_drop_client(client: &mut Client, reason: &str) {
    client_disconnect_dispatcher(client.get_client_id(), reason);

    client.disconnect(reason);
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
        dbg!("some formatting problem occurred");
    }

    let rust_msg = CStr::from_bytes_until_nul(&buffer)
        .unwrap()
        .to_string_lossy();
    if !rust_msg.is_empty() {
        shinqlx_com_printf(rust_msg.as_ref());
    }
}

pub(crate) fn shinqlx_com_printf(msg: &str) {
    let Some(_res) = console_print_dispatcher(msg) else {
        return;
    };
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    quake_live_engine.com_printf(msg);
}

pub(crate) fn shinqlx_sv_spawnserver(server: *const c_char, kill_bots: qboolean) {
    let server_str = unsafe { CStr::from_ptr(server) }.to_string_lossy();
    if server_str.is_empty() {
        return;
    }
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    quake_live_engine.spawn_server(server_str.as_ref(), kill_bots.into());

    new_game_dispatcher(false);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_RunFrame(time: c_int) {
    frame_dispatcher();

    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    quake_live_engine.run_frame(time);
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

#[allow(non_snake_case)]
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

    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return std::ptr::null();
    };
    quake_live_engine.client_connect(client_num, first_time.into(), is_bot.into())
}

#[allow(non_snake_case)]
pub extern "C" fn ShiNQlx_ClientSpawn(ent: *mut gentity_t) {
    let Some(game_entity): Option<GameEntity> = ent.try_into().ok() else {
        return;
    };

    shinqlx_client_spawn(game_entity)
}

pub(crate) fn shinqlx_client_spawn(mut game_entity: GameEntity) {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    quake_live_engine.client_spawn(&mut game_entity);

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
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    quake_live_engine.register_damage(
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
