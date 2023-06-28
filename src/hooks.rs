use crate::client::Client;
use crate::game_entity::GameEntity;
use crate::pyminqlx::{
    client_command_dispatcher, client_connect_dispatcher, client_disconnect_dispatcher,
    client_loaded_dispatcher, client_spawn_dispatcher, console_print_dispatcher, damage_dispatcher,
    frame_dispatcher, kamikaze_explode_dispatcher, kamikaze_use_dispatcher, new_game_dispatcher,
    server_command_dispatcher, set_configstring_dispatcher,
};
use crate::quake_live_engine::{
    AddCommand, CbufExecuteText, ClientConnect, ClientEnterWorld, ClientSpawn, ComPrintf,
    ExecuteClientCommand, FindCVar, InitGame, QuakeLiveEngine, RegisterDamage, RunFrame,
    SendServerCommand, SetConfigstring, SetModuleOffset, SpawnServer,
};
use crate::quake_types::clientState_t::CS_PRIMED;
use crate::quake_types::{
    cbufExec_t, client_t, gentity_t, qboolean, usercmd_t, MAX_MSGLEN, MAX_STRING_CHARS,
};
use crate::{
    initialize_cvars, initialize_static, search_vm_functions, CLIENT_CONNECT_ORIG_PTR,
    CLIENT_SPAWN_ORIG_PTR, CMD_ADDCOMMAND_ORIG_PTR, COMMON_INITIALIZED, COM_PRINTF_ORIG_PTR,
    CVARS_INITIALIZED, G_DAMAGE_ORIG_PTR, G_INIT_GAME_ORIG_PTR, G_RUN_FRAME_ORIG_PTR,
    G_START_KAMIKAZE_ORIG_PTR, SV_CLIENTENTERWORLD_ORIG_PTR, SV_DROPCLIENT_ORIG_PTR,
    SV_EXECUTECLIENTCOMMAND_ORIG_PTR, SV_SENDSERVERCOMMAND_ORIG_PTR, SV_SETCONFIGSTRING_ORIG_PTR,
    SV_SPAWNSERVER_ORIG_PTR, SV_TAGS_PREFIX, SYS_SETMODULEOFFSET_ORIG_PTR,
};
use once_cell::sync::OnceCell;
use retour::static_detour;
use std::error::Error;
use std::ffi::{c_char, c_float, c_int, c_void, CStr, VaList, VaListImpl};
use std::sync::atomic::{AtomicU64, Ordering};

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
    if COMMON_INITIALIZED.get().is_none() {
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

    if COMMON_INITIALIZED.get().is_none() {
        return;
    }

    search_vm_functions();

    #[allow(clippy::fn_to_numeric_cast)]
    hook_vm(offset as u64).unwrap();

    unsafe {
        patch_vm();
    }
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_InitGame(level_time: c_int, random_seed: c_int, restart: c_int) {
    QuakeLiveEngine::default().init_game(level_time, random_seed, restart);

    if !CVARS_INITIALIZED.load(Ordering::Relaxed) {
        set_tag();
    }

    initialize_cvars();

    if restart != 0 {
        new_game_dispatcher(true);
    }
}

fn shinqlx_sv_executeclientcommand(client: *mut client_t, cmd: *const c_char, client_ok: qboolean) {
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
        QuakeLiveEngine::default().execute_client_command(
            client.as_mut(),
            passed_on_cmd_str.as_str(),
            client_ok,
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

fn shinqlx_sv_cliententerworld(client: *mut client_t, cmd: *const usercmd_t) {
    let Some(mut safe_client): Option<Client> = client.try_into().ok() else {
        return;
    };

    let state = safe_client.get_state();
    QuakeLiveEngine::default().client_enter_world(&mut safe_client, cmd);

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
    if index == 16 || (662..670).contains(&index) {
        QuakeLiveEngine::default().set_configstring(&index, value);
        return;
    }

    let Some(res) = set_configstring_dispatcher(index, value) else {
        return;
    };
    QuakeLiveEngine::default().set_configstring(&index, res.as_str());
}

fn shinqlx_sv_dropclient(client: *mut client_t, reason: *const c_char) {
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
    QuakeLiveEngine::default().com_printf(msg);
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

static mut CLIENT_CONNECT_BUFFER: [c_char; MAX_STRING_CHARS as usize] =
    [0; MAX_STRING_CHARS as usize];

pub(crate) unsafe fn to_return_string(input: String) -> *const c_char {
    let bytes = input.as_bytes();
    let len = bytes.len();
    std::ptr::copy(
        [0; MAX_STRING_CHARS as usize].as_ptr(),
        CLIENT_CONNECT_BUFFER.as_mut_ptr(),
        len,
    );
    std::ptr::copy(
        input.as_bytes().as_ptr().cast(),
        CLIENT_CONNECT_BUFFER.as_mut_ptr(),
        len,
    );
    &CLIENT_CONNECT_BUFFER as *const c_char
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
                return unsafe { to_return_string(res) };
            }
        }
    }

    QuakeLiveEngine::default().client_connect(client_num, first_time.into(), is_bot.into())
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
    let mut mut_game_entity = game_entity;
    mut_game_entity.start_kamikaze();

    if client_id == -1 {
        return;
    }

    kamikaze_explode_dispatcher(client_id, mut_game_entity.get_game_client().is_ok())
}

#[no_mangle]
pub extern "C" fn ShiNQlx_G_Damage(
    target: *mut gentity_t,    // entity that is being damaged
    inflictor: *mut gentity_t, // entity that is causing the damage
    attacker: *mut gentity_t,  // entity that caused the inflictor to damage target
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

    let Ok(target_entity) = GameEntity::try_from(target) else {
        return;
    };
    if attacker.is_null() {
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

static_detour! {
    pub(crate) static CMD_ADDCOMMAND_DETOUR: unsafe extern "C" fn(*const c_char, unsafe extern "C" fn());
    pub(crate) static SYS_SETMODULEOFFSET_DETOUR: unsafe extern "C" fn(*const c_char, unsafe extern "C" fn());
    pub(crate) static SV_EXECUTECLIENTCOMMAND_DETOUR: unsafe extern "C" fn(*mut client_t, *const c_char, qboolean);
    pub(crate) static SV_CLIENTENTERWORLD_DETOUR: unsafe extern "C" fn(*mut client_t, *const usercmd_t);
    pub(crate) static SV_SETCONFGISTRING_DETOUR: unsafe extern "C" fn(c_int, *const c_char);
    pub(crate) static SV_DROPCLIENT_DETOUR: unsafe extern "C" fn(*mut client_t, *const c_char);
    pub(crate) static SV_SPAWNSERVER_DETOUR: unsafe extern "C" fn(*const c_char, qboolean);
}

pub(crate) static SV_SENDSERVERCOMMAND_TRAMPOLINE: OnceCell<
    extern "C" fn(*const client_t, *const c_char, ...),
> = OnceCell::new();
pub(crate) static COM_PRINTF_TRAMPOLINE: OnceCell<extern "C" fn(*const c_char, ...)> =
    OnceCell::new();

pub(crate) fn hook_static() -> Result<(), Box<dyn Error>> {
    debug_println!("Hooking...");
    if let Some(original_func) = CMD_ADDCOMMAND_ORIG_PTR.get() {
        unsafe {
            CMD_ADDCOMMAND_DETOUR.initialize(*original_func, shinqlx_cmd_addcommand)?;
            CMD_ADDCOMMAND_DETOUR.enable()?;
        }
    }

    if let Some(original_func) = SYS_SETMODULEOFFSET_ORIG_PTR.get() {
        unsafe {
            SYS_SETMODULEOFFSET_DETOUR.initialize(*original_func, shinqlx_sys_setmoduleoffset)?;
            SYS_SETMODULEOFFSET_DETOUR.enable()?;
        }
    }

    if let Some(original_func) = SV_EXECUTECLIENTCOMMAND_ORIG_PTR.get() {
        unsafe {
            SV_EXECUTECLIENTCOMMAND_DETOUR
                .initialize(*original_func, shinqlx_sv_executeclientcommand)?;
            SV_EXECUTECLIENTCOMMAND_DETOUR.enable()?;
        }
    }

    if let Some(original_func) = SV_CLIENTENTERWORLD_ORIG_PTR.get() {
        unsafe {
            SV_CLIENTENTERWORLD_DETOUR.initialize(*original_func, shinqlx_sv_cliententerworld)?;
            SV_CLIENTENTERWORLD_DETOUR.enable()?;
        }
    }

    if let Some(original_func) = SV_SETCONFIGSTRING_ORIG_PTR.get() {
        unsafe {
            SV_SETCONFGISTRING_DETOUR.initialize(*original_func, shinqlx_sv_setconfigstring)?;
            SV_SETCONFGISTRING_DETOUR.enable()?;
        }
    }

    if let Some(original_func) = SV_DROPCLIENT_ORIG_PTR.get() {
        unsafe {
            SV_DROPCLIENT_DETOUR.initialize(*original_func, shinqlx_sv_dropclient)?;
            SV_DROPCLIENT_DETOUR.enable()?;
        }
    }

    if let Some(original_func) = SV_SPAWNSERVER_ORIG_PTR.get() {
        unsafe {
            SV_SPAWNSERVER_DETOUR.initialize(*original_func, shinqlx_sv_spawnserver)?;
            SV_SPAWNSERVER_DETOUR.enable()?;
        }
    }

    extern "C" {
        fn HookRaw(target: *const c_void, replacement: *const c_void) -> *const c_void;
    }

    if let Some(func_pointer) = SV_SENDSERVERCOMMAND_ORIG_PTR.get() {
        let trampoline_func_ptr = unsafe {
            HookRaw(
                *func_pointer as *const c_void,
                ShiNQlx_SV_SendServerCommand as *const c_void,
            )
        };
        if !trampoline_func_ptr.is_null() {
            let trampoline_func = unsafe { std::mem::transmute(trampoline_func_ptr as u64) };
            SV_SENDSERVERCOMMAND_TRAMPOLINE
                .set(trampoline_func)
                .unwrap();
        }
    }

    if let Some(func_pointer) = COM_PRINTF_ORIG_PTR.get() {
        let trampoline_func_ptr = unsafe {
            HookRaw(
                *func_pointer as *const c_void,
                ShiNQlx_Com_Printf as *const c_void,
            )
        };
        if !trampoline_func_ptr.is_null() {
            let trampoline_func = unsafe { std::mem::transmute(trampoline_func_ptr as u64) };
            COM_PRINTF_TRAMPOLINE.set(trampoline_func).unwrap();
        }
    }

    Ok(())
}

pub(crate) static CLIENT_CONNECT_TRAMPOLINE: AtomicU64 = AtomicU64::new(0);
pub(crate) static CLIENT_SPAWN_TRAMPOLINE: AtomicU64 = AtomicU64::new(0);
pub(crate) static G_START_KAMIKAZE_TRAMPOLINE: AtomicU64 = AtomicU64::new(0);
pub(crate) static G_DAMAGE_TRAMPOLINE: AtomicU64 = AtomicU64::new(0);

/*
 * Hooks VM calls. Not all use Hook, since the VM calls are stored in a table of
 * pointers. We simply set our function pointer to the current pointer in the table and
 * then replace the it with our replacement function. Just like hooking a VMT.
 *
 * This must be called AFTER Sys_SetModuleOffset, since Sys_SetModuleOffset is called after
 * the VM DLL has been loaded, meaning the pointer we use has been set.
 *
 * PROTIP: If you can, ALWAYS use VM_Call table hooks instead of using Hook().
*/
pub(crate) fn hook_vm(qagame_dllentry: u64) -> Result<(), Box<dyn Error>> {
    extern "C" {
        fn HookRaw(target: *const c_void, replacement: *const c_void) -> *const c_void;
    }

    let base_address = unsafe { std::ptr::read_unaligned((qagame_dllentry + 0x3) as *const i32) };
    let vm_call_table = base_address as u64 + qagame_dllentry + 0x3 + 4;
    debug_println!(format!("vm_call_table: {:#X}", vm_call_table));

    let g_initgame_ptr = unsafe {
        std::ptr::read((vm_call_table + 0x18) as *const *const extern "C" fn(c_int, c_int, c_int))
    };
    G_INIT_GAME_ORIG_PTR.store(g_initgame_ptr as u64, Ordering::Relaxed);
    debug_println!(format!("G_InitGame: {:#X}", g_initgame_ptr as u64));

    let g_runframe_ptr =
        unsafe { std::ptr::read((vm_call_table + 0x8) as *const *const extern "C" fn(c_int)) };
    G_RUN_FRAME_ORIG_PTR.store(g_runframe_ptr as u64, Ordering::Relaxed);
    debug_println!(format!("G_RunFrame: {:#X}", g_runframe_ptr as u64));

    debug_println!("Hooking VM functions...");
    #[allow(clippy::fn_to_numeric_cast)]
    unsafe {
        std::ptr::write(
            (vm_call_table + 0x18) as *mut u64,
            ShiNQlx_G_InitGame as u64,
        );
    }

    #[allow(clippy::fn_to_numeric_cast)]
    unsafe {
        std::ptr::write((vm_call_table + 0x8) as *mut u64, ShiNQlx_G_RunFrame as u64);
    }

    let func_pointer = CLIENT_CONNECT_ORIG_PTR.load(Ordering::Relaxed);
    if func_pointer == 0 {
        CLIENT_CONNECT_TRAMPOLINE.store(0, Ordering::Relaxed);
    } else {
        let trampoline_func_ptr = unsafe {
            HookRaw(
                func_pointer as *const c_void,
                ShiNQlx_ClientConnect as *const c_void,
            )
        };
        if trampoline_func_ptr.is_null() {
            CLIENT_CONNECT_TRAMPOLINE.store(0, Ordering::Relaxed);
        } else {
            CLIENT_CONNECT_TRAMPOLINE.store(trampoline_func_ptr as u64, Ordering::Relaxed);
        }
    }

    let func_pointer = G_START_KAMIKAZE_ORIG_PTR.load(Ordering::Relaxed);
    if func_pointer == 0 {
        G_START_KAMIKAZE_TRAMPOLINE.store(0, Ordering::Relaxed);
    } else {
        let trampoline_func_ptr = unsafe {
            HookRaw(
                func_pointer as *const c_void,
                ShiNQlx_G_StartKamikaze as *const c_void,
            )
        };
        if trampoline_func_ptr.is_null() {
            G_START_KAMIKAZE_TRAMPOLINE.store(0, Ordering::Relaxed);
        } else {
            G_START_KAMIKAZE_TRAMPOLINE.store(trampoline_func_ptr as u64, Ordering::Relaxed);
        }
    }

    let func_pointer = CLIENT_SPAWN_ORIG_PTR.load(Ordering::Relaxed);
    if func_pointer == 0 {
        CLIENT_SPAWN_TRAMPOLINE.store(0, Ordering::Relaxed);
    } else {
        let trampoline_func_ptr = unsafe {
            HookRaw(
                func_pointer as *const c_void,
                ShiNQlx_ClientSpawn as *const c_void,
            )
        };
        if trampoline_func_ptr.is_null() {
            CLIENT_SPAWN_TRAMPOLINE.store(0, Ordering::Relaxed);
        } else {
            CLIENT_SPAWN_TRAMPOLINE.store(trampoline_func_ptr as u64, Ordering::Relaxed);
        }
    }

    let func_pointer = G_DAMAGE_ORIG_PTR.load(Ordering::Relaxed);
    if func_pointer == 0 {
        G_DAMAGE_TRAMPOLINE.store(0, Ordering::Relaxed);
    } else {
        let trampoline_func_ptr = unsafe {
            HookRaw(
                func_pointer as *const c_void,
                ShiNQlx_G_Damage as *const c_void,
            )
        };
        if trampoline_func_ptr.is_null() {
            G_DAMAGE_TRAMPOLINE.store(0, Ordering::Relaxed);
        } else {
            G_DAMAGE_TRAMPOLINE.store(trampoline_func_ptr as u64, Ordering::Relaxed);
        }
    }

    Ok(())
}
