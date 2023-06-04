use crate::pyminqlx::{
    client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
    client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher, damage_dispatcher,
    frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher, new_game_dispatcher,
    server_command_dispatcher, set_configstring_dispatcher,
};
use crate::quake_live_engine::{
    AddCommand, CbufExecuteText, Client, ClientConnect, ClientEnterWorld, ClientSpawn, ComPrintf,
    ExecuteClientCommand, FindCVar, GameEntity, InitGame, QuakeLiveEngine, RegisterDamage,
    RunFrame, SendServerCommand, SetConfigstring, SetModuleOffset, SpawnServer,
};
use crate::quake_types::clientState_t::CS_PRIMED;
use crate::quake_types::{
    cbufExec_t, client_t, gentity_t, qboolean, usercmd_t, MAX_CLIENTS, MAX_MSGLEN,
};
use crate::{
    initialize_cvars, initialize_static, COMMON_INITIALIZED, CVARS_INITIALIZED, SV_TAGS_PREFIX,
};
use retour::static_detour;
use std::error::Error;
use std::ffi::{c_char, c_float, c_int, c_void, CStr, CString, VaList, VaListImpl};

fn set_tag() {
    let quake_live_engine = QuakeLiveEngine::default();
    let Some(sv_tags) = quake_live_engine.find_cvar("sv_tags") else {
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
    quake_live_engine.cbuf_execute_text(cbufExec_t::EXEC_INSERT, &new_tags);
}

fn shinqlx_cmd_addcommand(cmd: *const c_char, func: unsafe extern "C" fn()) {
    if unsafe { !COMMON_INITIALIZED } {
        initialize_static();
    }

    let command = unsafe { CStr::from_ptr(cmd) }.to_string_lossy();
    if !command.is_empty() {
        QuakeLiveEngine::default().add_command(command.as_ref(), func);
    }
}

#[repr(C)]
pub struct DlInfo {
    pub dli_fname: *const c_char,
    pub dli_fbase: *mut c_void,
    pub dli_sname: *const c_char,
    pub dli_saddr: *mut c_void,
}

fn shinqlx_sys_setmoduleoffset(module_name: *const c_char, offset: unsafe extern "C" fn()) {
    extern "C" {
        static mut qagame_dllentry: *mut c_void;
        static mut qagame: *mut c_void;
        fn SearchVmFunctions();
        fn HookVm();
        fn InitializeVm();
        fn patch_vm();
        fn dladdr(addr: *const c_void, into: *mut DlInfo) -> c_int;
    }

    let converted_module_name = unsafe { CStr::from_ptr(module_name) }.to_string_lossy();

    // We should be getting qagame, but check just in case.
    match converted_module_name.as_ref() {
        "qagame" => {
            // Despite the name, it's not the actual module, but vmMain.
            // We use dlinfo to get the base of the module so we can properly
            // initialize all the pointers relative to the base.
            unsafe { qagame_dllentry = offset as *mut c_void };
            let mut dlinfo: DlInfo = DlInfo {
                dli_fname: std::ptr::null_mut(),
                dli_fbase: std::ptr::null_mut(),
                dli_sname: std::ptr::null_mut(),
                dli_saddr: std::ptr::null_mut(),
            };
            let res = unsafe { dladdr(offset as *const c_void, &mut dlinfo as *mut DlInfo) };
            if res != 0 {
                unsafe { qagame = dlinfo.dli_fbase };
            } else {
                debug_println!("dladdr() failed.");
                unsafe { qagame = std::ptr::null_mut() };
            }
        }
        _ => debug_println!(format!("Unknown module: {}", converted_module_name)),
    }

    QuakeLiveEngine::default().set_module_offset(converted_module_name.as_ref(), offset);

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

#[no_mangle]
pub extern "C" fn ShiNQlx_G_InitGame(level_time: c_int, random_seed: c_int, restart: c_int) {
    QuakeLiveEngine::default().init_game(level_time, random_seed, restart);

    if unsafe { !CVARS_INITIALIZED } {
        set_tag();
    }

    initialize_cvars();

    if restart != 0 {
        new_game_dispatcher(true);
    }
}

fn shinqlx_sv_executeclientcommand(
    client: *const client_t,
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

pub(crate) fn shinqlx_execute_client_command(client: Option<Client>, cmd: &str, client_ok: bool) {
    let passed_on_cmd_str = if client_ok
        && client
            .as_ref()
            .is_some_and(|safe_client| safe_client.has_gentity())
    {
        let client_id = client
            .as_ref()
            .map(|safe_client| safe_client.get_client_id())
            .unwrap();
        if let Some(dispatcher_result) = client_command_dispatcher(client_id, cmd) {
            dispatcher_result
        } else {
            return;
        }
    } else {
        cmd.into()
    };

    if !passed_on_cmd_str.is_empty() {
        QuakeLiveEngine::default().execute_client_command(
            client.as_ref(),
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
        QuakeLiveEngine::default().send_server_command(client, passed_on_cmd_str.as_str());
    }
}

fn shinqlx_sv_cliententerworld(client: *const client_t, cmd: *const usercmd_t) {
    let Some(safe_client): Option<Client> = client.try_into().ok() else {
        return;
    };

    let state = safe_client.get_state();
    QuakeLiveEngine::default().client_enter_world(&safe_client, cmd);

    // gentity is NULL if map changed.
    // state is CS_PRIMED only if it's the first time they connect to the server,
    // otherwise the dispatcher would also go off when a game starts and such.
    if safe_client.has_gentity() && state == CS_PRIMED as i32 {
        client_loaded_dispatcher(safe_client.get_client_id());
    }
}

pub(crate) fn shinqlx_sv_setconfigstring(index: c_int, value: *const c_char) {
    let safe_value = if !value.is_null() {
        unsafe { CStr::from_ptr(value) }.to_string_lossy()
    } else {
        "".into()
    };

    shinqlx_set_configstring(index, safe_value.as_ref());
}

pub(crate) fn shinqlx_set_configstring(index: i32, value: &str) {
    // Indices 16 and 66X are spammed a ton every frame for some reason,
    // so we add some exceptions for those. I don't think we should have any
    // use for those particular ones anyway. If we don't do this, we get
    // like a 25% increase in CPU usage on an empty server.
    if index == 16 || (662..670).contains(&index) {
        QuakeLiveEngine::default().set_configstring(&index, value);
        return;
    }

    if let Some(res) = set_configstring_dispatcher(index, value) {
        QuakeLiveEngine::default().set_configstring(&index, res.as_str());
    }
}

fn shinqlx_sv_dropclient(client: *const client_t, reason: *const c_char) {
    if let Ok(safe_client) = Client::try_from(client) {
        shinqlx_drop_client(
            &safe_client,
            unsafe { CStr::from_ptr(reason) }.to_string_lossy().as_ref(),
        );
    }
}

pub(crate) fn shinqlx_drop_client(client: &Client, reason: &str) {
    client_disconnect_dispatcher(client.get_client_id(), reason);

    client.disconnect(reason);
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
    if let Some(_res) = console_print_dispatcher(msg) {
        QuakeLiveEngine::default().com_printf(msg);
    }
}

fn shinqlx_sv_spawnserver(server: *const c_char, kill_bots: qboolean) {
    let server_str = unsafe { CStr::from_ptr(server) }.to_string_lossy();
    if server_str.is_empty() {
        return;
    }
    QuakeLiveEngine::default().spawn_server(server_str.as_ref(), kill_bots.into());

    new_game_dispatcher(false);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_RunFrame(time: c_int) {
    frame_dispatcher();

    QuakeLiveEngine::default().run_frame(time);
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
                if let Ok(result) = CString::new(res) {
                    return result.into_raw();
                }
            }
        }
    }

    match QuakeLiveEngine::default().client_connect(client_num, first_time.into(), is_bot.into()) {
        None => std::ptr::null_mut(),
        Some(message) => {
            if let Ok(result) = CString::new(message) {
                result.into_raw()
            } else {
                CString::new("You are banned from this server.")
                    .unwrap()
                    .into_raw()
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_ClientSpawn(ent: *mut gentity_t) {
    let Some(game_entity): Option<GameEntity> = ent.try_into().ok() else {
        return;
    };

    shinqlx_client_spawn(game_entity)
}

pub(crate) fn shinqlx_client_spawn(mut game_entity: GameEntity) {
    QuakeLiveEngine::default().client_spawn(&mut game_entity);

    // Since we won't ever stop the real function from being called,
    // we trigger the event after calling the real one. This will allow
    // us to set weapons and such without it getting overriden later.
    client_spawn_dispatcher(game_entity.get_client_id());
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
        kamikaze_use_dispatcher(client_id);
    }
    let mut mut_game_entity = game_entity;
    mut_game_entity.start_kamikaze();

    if client_id == -1 {
        return;
    }

    kamikaze_explode_dispatcher(client_id, mut_game_entity.get_game_client().is_some())
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_Damage(
    target: *mut gentity_t,    // entity that is being damaged
    inflictor: *mut gentity_t, // entity that is causing the damage
    attacker: *mut gentity_t,  // entity that caused the inflictor to damage targ
    dir: *const c_float,       // direction of the attack for knockback
    pos: *const c_float,       // point at which the damage is being inflicted, used for headshots
    damage: c_int,             // amount of damage being inflicted
    dflags: c_int,             // these flags are used to control how T_Damage works
    // DAMAGE_RADIUS			damage was indirect (from a nearby explosion)
    // DAMAGE_NO_ARMOR			armor does not protect from this damage
    // DAMAGE_NO_KNOCKBACK		do not affect velocity, just view angles
    // DAMAGE_NO_PROTECTION	kills godmode, armor, everything
    // DAMAGE_NO_TEAM_PROTECTION	kills team mates
    means_of_death: c_int, // means_of_death indicator
) {
    QuakeLiveEngine::default().register_damage(
        target,
        inflictor,
        attacker,
        dir,
        pos,
        damage,
        dflags,
        means_of_death,
    );

    if let Ok(target_entity) = GameEntity::try_from(target) {
        if attacker.is_null() || unsafe { (*attacker).client.is_null() } {
            damage_dispatcher(
                target_entity.get_client_id(),
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
                    target_entity.get_client_id(),
                    None,
                    damage,
                    dflags,
                    means_of_death,
                );
            }
            Ok(attacker_entity) => {
                if (0..MAX_CLIENTS).contains(&(attacker_entity.get_client_id() as u32)) {
                    damage_dispatcher(
                        target_entity.get_client_id(),
                        Some(attacker_entity.get_client_id()),
                        damage,
                        dflags,
                        means_of_death,
                    );
                }
            }
        }
    }
}

static_detour! {
    pub(crate) static CMD_ADDCOMMAND_DETOUR: unsafe extern "C" fn(*const c_char, unsafe extern "C" fn());
    pub(crate) static SYS_SETMODULEOFFSET_DETOUR: unsafe extern "C" fn(*const c_char, unsafe extern "C" fn());
    pub(crate) static SV_EXECUTECLIENTCOMMAND_DETOUR: unsafe extern "C" fn(*const client_t, *const c_char, qboolean);
    pub(crate) static SV_CLIENTENTERWORLD_DETOUR: unsafe extern "C" fn(*const client_t, *const usercmd_t);
    pub(crate) static SV_SETCONFGISTRING_DETOUR: unsafe extern "C" fn(c_int, *const c_char);
    pub(crate) static SV_DROPCLIENT_DETOUR: unsafe extern "C" fn(*const client_t, *const c_char);
    pub(crate) static SV_SPAWNSERVER_DETOUR: unsafe extern "C" fn(*const c_char, qboolean);
}

pub(crate) fn hook_static() -> Result<(), Box<dyn Error>> {
    debug_println!("Hooking...");

    extern "C" {
        static Cmd_AddCommand: unsafe extern "C" fn(*const c_char, unsafe extern "C" fn());
    }
    unsafe {
        CMD_ADDCOMMAND_DETOUR.initialize(Cmd_AddCommand, shinqlx_cmd_addcommand)?;
        CMD_ADDCOMMAND_DETOUR.enable()?;
    }

    extern "C" {
        static Sys_SetModuleOffset: unsafe extern "C" fn(*const c_char, unsafe extern "C" fn());
    }
    unsafe {
        SYS_SETMODULEOFFSET_DETOUR.initialize(Sys_SetModuleOffset, shinqlx_sys_setmoduleoffset)?;
        SYS_SETMODULEOFFSET_DETOUR.enable()?;
    }

    extern "C" {
        static SV_ExecuteClientCommand:
            unsafe extern "C" fn(*const client_t, *const c_char, qboolean);
    }
    unsafe {
        SV_EXECUTECLIENTCOMMAND_DETOUR
            .initialize(SV_ExecuteClientCommand, shinqlx_sv_executeclientcommand)?;
        SV_EXECUTECLIENTCOMMAND_DETOUR.enable()?;
    }
    extern "C" {
        static SV_ClientEnterWorld: unsafe extern "C" fn(*const client_t, *const usercmd_t);
    }
    unsafe {
        SV_CLIENTENTERWORLD_DETOUR.initialize(SV_ClientEnterWorld, shinqlx_sv_cliententerworld)?;
        SV_CLIENTENTERWORLD_DETOUR.enable()?;
    }

    extern "C" {
        static SV_SetConfigstring: unsafe extern "C" fn(c_int, *const c_char);
    }
    unsafe {
        SV_SETCONFGISTRING_DETOUR.initialize(SV_SetConfigstring, shinqlx_sv_setconfigstring)?;
        SV_SETCONFGISTRING_DETOUR.enable()?;
    }

    extern "C" {
        static SV_DropClient: unsafe extern "C" fn(*const client_t, *const c_char);
    }
    unsafe {
        SV_DROPCLIENT_DETOUR.initialize(SV_DropClient, shinqlx_sv_dropclient)?;
        SV_DROPCLIENT_DETOUR.enable()?;
    }

    extern "C" {
        static SV_SpawnServer: unsafe extern "C" fn(*const c_char, qboolean);
    }
    unsafe {
        SV_SPAWNSERVER_DETOUR.initialize(SV_SpawnServer, shinqlx_sv_spawnserver)?;
        SV_SPAWNSERVER_DETOUR.enable()?;
    }

    extern "C" {
        fn HookStatic();
    }
    unsafe { HookStatic() };

    Ok(())
}
