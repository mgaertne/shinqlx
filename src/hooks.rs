#[cfg(not(feature = "cdispatchers"))]
use crate::pyminqlx::{
    client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
    client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher, damage_dispatcher,
    frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher, new_game_dispatcher,
    server_command_dispatcher, set_configstring_dispatcher,
};
use crate::quake_common::clientState_t::CS_PRIMED;
use crate::quake_common::{
    cbufExec_t, client_t, gentity_t, qboolean, usercmd_t, MAX_MSGLEN, SV_TAGS_PREFIX,
};
use crate::quake_live_engine::{
    AddCommand, CbufExecuteText, Client, ClientConnect, ClientEnterWorld, ClientSpawn, ComPrintf,
    ExecuteClientCommand, FindCVar, GameEntity, InitGame, QuakeLiveEngine, RegisterDamage,
    RunFrame, SendServerCommand, SetConfigstring, SetModuleOffset, SpawnServer,
};
use crate::{initialize_cvars, initialize_static, COMMON_INITIALIZED, CVARS_INITIALIZED};
use alloc::borrow::Cow;
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

#[no_mangle]
pub extern "C" fn ShiNQlx_Cmd_AddCommand(cmd: *const c_char, func: unsafe extern "C" fn()) {
    if unsafe { !COMMON_INITIALIZED } {
        initialize_static();
    }

    let command = unsafe { CStr::from_ptr(cmd) }.to_string_lossy();
    if !command.is_empty() {
        QuakeLiveEngine::default().add_command(command, func);
    }
}

#[repr(C)]
pub struct DlInfo {
    pub dli_fname: *const c_char,
    pub dli_fbase: *mut c_void,
    pub dli_sname: *const c_char,
    pub dli_saddr: *mut c_void,
}

#[no_mangle]
pub extern "C" fn ShiNQlx_Sys_SetModuleOffset(
    module_name: *const c_char,
    offset: unsafe extern "C" fn(),
) {
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

    QuakeLiveEngine::default().set_module_offset(converted_module_name, offset);

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
        #[cfg(not(feature = "cdispatchers"))]
        new_game_dispatcher(true);

        #[cfg(feature = "cdispatchers")]
        {
            extern "C" {
                fn NewGameDispatcher(restart: c_int);
            }
            unsafe { NewGameDispatcher(restart) };
        }
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_ExecuteClientCommand(
    client: *const client_t,
    cmd: *const c_char,
    client_ok: qboolean,
) {
    let rust_cmd = unsafe { CStr::from_ptr(cmd) }.to_string_lossy();
    if !rust_cmd.is_empty() {
        shinqlx_execute_client_command(client.try_into().ok(), rust_cmd, client_ok.into());
    }
}

pub(crate) fn shinqlx_execute_client_command(
    client: Option<Client>,
    cmd: Cow<'_, str>,
    client_ok: bool,
) {
    #[cfg(feature = "cdispatchers")]
    let res: *const c_char;
    let mut passed_on_cmd_str = cmd;
    if client
        .as_ref()
        .is_some_and(|safe_client| safe_client.has_gentity())
    {
        let client_id = client
            .as_ref()
            .map(|safe_client| safe_client.get_client_id())
            .unwrap();
        #[cfg(feature = "cdispatchers")]
        {
            extern "C" {
                fn ClientCommandDispatcher(client_id: c_int, cmd: *const c_char) -> *const c_char;
            }

            let passed_on_cmd = CString::new(passed_on_cmd_str.as_ref()).unwrap();
            res = unsafe { ClientCommandDispatcher(client_id, passed_on_cmd.into_raw()) };
            if res.is_null() {
                return;
            }
            passed_on_cmd_str = unsafe { CStr::from_ptr(res) }.to_string_lossy();
        }

        #[cfg(not(feature = "cdispatchers"))]
        if let Some(dispatcher_result) =
            client_command_dispatcher(client_id, passed_on_cmd_str.as_ref().into())
        {
            passed_on_cmd_str = dispatcher_result.into();
        }
    }

    if !passed_on_cmd_str.is_empty() {
        QuakeLiveEngine::default().execute_client_command(
            client.as_ref(),
            passed_on_cmd_str,
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
        shinqlx_send_server_command(Client::try_from(client).ok(), cmd);
    }
}

pub(crate) fn shinqlx_send_server_command(client: Option<Client>, cmd: Cow<'_, str>) {
    let mut passed_on_cmd_str = cmd;

    if client
        .as_ref()
        .is_some_and(|safe_client| safe_client.has_gentity())
    {
        let client_id = client
            .as_ref()
            .map(|safe_client| safe_client.get_client_id())
            .unwrap();
        #[cfg(feature = "cdispatchers")]
        {
            extern "C" {
                fn ServerCommandDispatcher(
                    client_id: c_int,
                    command: *const c_char,
                ) -> *const c_char;
            }

            let c_cmd = CString::new(passed_on_cmd_str.as_ref()).unwrap();
            let res = unsafe { ServerCommandDispatcher(client_id, c_cmd.into_raw()) };
            passed_on_cmd_str = unsafe { CStr::from_ptr(res) }.to_string_lossy();
        }
        #[cfg(not(feature = "cdispatchers"))]
        if let Some(res) =
            server_command_dispatcher(Some(client_id), passed_on_cmd_str.as_ref().into())
        {
            passed_on_cmd_str = res.into();
        }
    } else if client.as_ref().is_none() {
        #[cfg(feature = "cdispatchers")]
        {
            extern "C" {
                fn ServerCommandDispatcher(
                    client_id: c_int,
                    command: *const c_char,
                ) -> *const c_char;
            }
            let c_cmd = CString::new(passed_on_cmd_str.as_ref()).unwrap();
            let res = unsafe { ServerCommandDispatcher(-1, c_cmd.into_raw()) };
            passed_on_cmd_str = unsafe { CStr::from_ptr(res) }.to_string_lossy();
        }
        #[cfg(not(feature = "cdispatchers"))]
        if let Some(res) = server_command_dispatcher(None, passed_on_cmd_str.as_ref().into()) {
            passed_on_cmd_str = res.into();
        }
    }

    if !passed_on_cmd_str.is_empty() {
        QuakeLiveEngine::default().send_server_command(client, passed_on_cmd_str);
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_ClientEnterWorld(client: *const client_t, cmd: *const usercmd_t) {
    let Some(safe_client): Option<Client> = client.try_into().ok() else {
        return;
    };

    let state = safe_client.get_state();
    QuakeLiveEngine::default().client_enter_world(&safe_client, cmd);

    // gentity is NULL if map changed.
    // state is CS_PRIMED only if it's the first time they connect to the server,
    // otherwise the dispatcher would also go off when a game starts and such.
    if safe_client.has_gentity() && state == CS_PRIMED as i32 {
        #[cfg(feature = "cdispatchers")]
        {
            extern "C" {
                fn ClientLoadedDispatcher(client_id: c_int);
            }

            unsafe { ClientLoadedDispatcher(safe_client.get_client_id()) };
        }

        #[cfg(not(feature = "cdispatchers"))]
        client_loaded_dispatcher(safe_client.get_client_id());
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_SetConfigstring(index: c_int, value: *const c_char) {
    let safe_value = if value.is_null() {
        Cow::from("")
    } else {
        unsafe { CStr::from_ptr(value) }.to_string_lossy()
    };

    shinqlx_set_configstring(index, safe_value);
}

pub(crate) fn shinqlx_set_configstring(index: i32, value: Cow<'_, str>) {
    // Indices 16 and 66X are spammed a ton every frame for some reason,
    // so we add some exceptions for those. I don't think we should have any
    // use for those particular ones anyway. If we don't do this, we get
    // like a 25% increase in CPU usage on an empty server.
    if index == 16 || (662..670).contains(&index) {
        QuakeLiveEngine::default().set_configstring(&index, value);
        return;
    }

    #[cfg(feature = "cdispatchers")]
    {
        extern "C" {
            fn SetConfigstringDispatcher(index: c_int, value: *const c_char) -> *const c_char;
        }

        let value_cstring = CString::new(value.as_ref()).unwrap();
        let res = unsafe { SetConfigstringDispatcher(index, value_cstring.into_raw()) };
        if res.is_null() {
            return;
        }

        let res_string = unsafe { CStr::from_ptr(res) }.to_string_lossy();
        QuakeLiveEngine::default().set_configstring(&index, res_string);
    }

    #[cfg(not(feature = "cdispatchers"))]
    if let Some(res) = set_configstring_dispatcher(index, value) {
        QuakeLiveEngine::default().set_configstring(&index, res.into());
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_DropClient(client: *const client_t, reason: *const c_char) {
    if let Ok(safe_client) = Client::try_from(client) {
        shinqlx_drop_client(
            &safe_client,
            unsafe { CStr::from_ptr(reason) }.to_string_lossy(),
        );
    }
}

pub(crate) fn shinqlx_drop_client(client: &Client, reason: Cow<'_, str>) {
    #[cfg(feature = "cdispatchers")]
    {
        extern "C" {
            fn ClientDisconnectDispatcher(client_id: c_int, reason: *const c_char);
        }

        let c_reason = CString::new(reason.as_ref()).unwrap();
        unsafe { ClientDisconnectDispatcher(client.get_client_id(), c_reason.into_raw()) };
    }

    #[cfg(not(feature = "cdispatchers"))]
    client_disconnect_dispatcher(client.get_client_id(), reason.as_ref().into());

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
        shinqlx_com_printf(rust_msg);
    }
}

pub(crate) fn shinqlx_com_printf(msg: Cow<'_, str>) {
    #[cfg(feature = "cdispatchers")]
    {
        extern "C" {
            fn ConsolePrintDispatcher(msg: *const c_char) -> *const c_char;
        }

        let text = CString::new(msg.as_ref()).unwrap();
        let res = unsafe { ConsolePrintDispatcher(text.into_raw()) };
        if res.is_null() {
            return;
        }

        QuakeLiveEngine::default().com_printf(msg);
    }

    #[cfg(not(feature = "cdispatchers"))]
    if let Some(_res) = console_print_dispatcher(msg.as_ref().into()) {
        QuakeLiveEngine::default().com_printf(msg);
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_SV_SpawnServer(server: *const c_char, kill_bots: qboolean) {
    let server_str = unsafe { CStr::from_ptr(server) }.to_string_lossy();
    if server_str.is_empty() {
        return;
    }
    QuakeLiveEngine::default().spawn_server(server_str, kill_bots.into());

    #[cfg(feature = "cdispatchers")]
    {
        extern "C" {
            fn NewGameDispatcher(restart: c_int);
        }

        unsafe { NewGameDispatcher(qboolean::qfalse.into()) };
    }

    #[cfg(not(feature = "cdispatchers"))]
    new_game_dispatcher(false);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_RunFrame(time: c_int) {
    #[cfg(feature = "cdispatchers")]
    {
        extern "C" {
            fn FrameDispatcher();
        }

        unsafe { FrameDispatcher() };
    }

    #[cfg(not(feature = "cdispatchers"))]
    frame_dispatcher();

    QuakeLiveEngine::default().run_frame(time);
}

#[no_mangle]
pub extern "C" fn ShiNQlx_ClientConnect(
    client_num: c_int,
    first_time: qboolean,
    is_bot: qboolean,
) -> *const c_char {
    #[cfg(feature = "cdispatchers")]
    {
        extern "C" {
            fn ClientConnectDispatcher(client_num: c_int, is_bot: qboolean) -> *const c_char;
        }

        if first_time.into() {
            let res = unsafe { ClientConnectDispatcher(client_num, is_bot) };
            if !res.is_null() && !<qboolean as Into<bool>>::into(is_bot) {
                return res;
            }
        }
    }

    #[cfg(not(feature = "cdispatchers"))]
    if first_time.into() {
        if let Some(res) = client_connect_dispatcher(client_num, is_bot.into()) {
            if !<qboolean as Into<bool>>::into(is_bot) {
                let result = CString::new(res).unwrap();
                return result.into_raw();
            }
        }
    }

    match QuakeLiveEngine::default().client_connect(client_num, first_time.into(), is_bot.into()) {
        None => std::ptr::null_mut(),
        Some(message) => {
            let result = CString::new(message).unwrap();
            result.into_raw()
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

pub(crate) fn shinqlx_client_spawn(game_entity: GameEntity) {
    QuakeLiveEngine::default().client_spawn(&game_entity);

    // Since we won't ever stop the real function from being called,
    // we trigger the event after calling the real one. This will allow
    // us to set weapons and such without it getting overriden later.
    #[cfg(feature = "cdispatchers")]
    {
        extern "C" {
            fn ClientSpawnDispatcher(ent: c_int);
        }

        unsafe {
            ClientSpawnDispatcher(game_entity.get_client_id());
        };
    }

    #[cfg(not(feature = "cdispatchers"))]
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
        #[cfg(feature = "cdispatchers")]
        {
            extern "C" {
                fn KamikazeUseDispatcher(client_id: c_int);
            }

            unsafe { KamikazeUseDispatcher(client_id) };
        }

        #[cfg(not(feature = "cdispatchers"))]
        kamikaze_use_dispatcher(client_id);
    }

    game_entity.start_kamikaze();

    if client_id == -1 {
        return;
    }

    #[cfg(feature = "cdispatchers")]
    {
        extern "C" {
            fn KamikazeExplodeDispatcher(client_id: c_int, used_on_demand: c_int);
        }

        unsafe {
            KamikazeExplodeDispatcher(client_id, game_entity.get_game_client().is_some() as c_int)
        };
    }

    #[cfg(not(feature = "cdispatchers"))]
    kamikaze_explode_dispatcher(client_id, game_entity.get_game_client().is_some())
}

#[no_mangle]
extern "C" fn ShiNQlx_G_Damage(
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

    #[cfg(not(feature = "cdispatchers"))]
    {
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
