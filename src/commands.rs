#[cfg(not(feature = "cembed"))]
use crate::pyminqlx::pyminqlx_initialize;
use crate::pyminqlx::pyminqlx_is_initialized;
#[cfg(not(feature = "cembed"))]
use crate::pyminqlx::pyminqlx_reload;
use crate::pyminqlx::CUSTOM_COMMAND_HANDLER;
#[cfg(not(feature = "cdispatchers"))]
use crate::pyminqlx::{new_game_dispatcher, rcon_dispatcher};
use crate::quake_common::entity_event_t::{EV_DEATH1, EV_GIB_PLAYER, EV_PAIN};
use crate::quake_live_engine::{
    Client, CmdArgc, CmdArgs, CmdArgv, ComPrintf, GameAddEvent, GameEntity, QuakeLiveEngine,
    SendServerCommand,
};
#[cfg(feature = "cembed")]
use crate::PyMinqlx_InitStatus_t;
use crate::SV_MAXCLIENTS;
use pyo3::Python;
use rand::Rng;
#[cfg(feature = "cdispatchers")]
use std::ffi::{c_char, c_int, CString};

#[no_mangle]
pub extern "C" fn cmd_send_server_command() {
    let quake_live_engine = QuakeLiveEngine::default();
    let Some(cmd_args) = quake_live_engine.cmd_args() else { return; };

    let server_command = format!("{}\n", cmd_args);
    quake_live_engine.send_server_command(None, &server_command);
}

#[no_mangle]
pub extern "C" fn cmd_center_print() {
    let quake_live_engine = QuakeLiveEngine::default();
    let Some(cmd_args) = quake_live_engine.cmd_args() else { return; };

    let server_command = format!("cp \"{}\"\n", cmd_args);
    quake_live_engine.send_server_command(None, &server_command);
}

#[no_mangle]
pub extern "C" fn cmd_regular_print() {
    let quake_live_engine = QuakeLiveEngine::default();
    let Some(cmd_args) = quake_live_engine.cmd_args() else { return; };

    let server_command = format!("print \"{}\n\"\n", cmd_args);
    quake_live_engine.send_server_command(None, &server_command);
}

#[no_mangle]
pub extern "C" fn cmd_slap() {
    let quake_live_engine = QuakeLiveEngine::default();
    let argc = quake_live_engine.cmd_argc();

    if argc < 2 {
        let Some(command_name) = quake_live_engine.cmd_argv(0) else {return; };
        let usage_note = format!("Usage: {} <client_id> [damage]\n", command_name);

        quake_live_engine.com_printf(usage_note.as_str());
        return;
    }

    let Some(passed_client_id_str) = quake_live_engine.cmd_argv(1) else {
        return;
    };
    let maxclients = unsafe { SV_MAXCLIENTS };
    let Some(client_id) = passed_client_id_str.parse::<i32>().ok() else {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients-1
        );
        quake_live_engine.com_printf(usage_note.as_str());
        return;
    };

    if client_id >= maxclients {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients - 1
        );
        quake_live_engine.com_printf(usage_note.as_str());
        return;
    }

    let dmg = if argc > 2 {
        let passed_dmg = quake_live_engine.cmd_argv(2).unwrap();
        passed_dmg.parse::<i32>().unwrap_or(0)
    } else {
        0
    };

    let Some(client_entity) = GameEntity::try_from(client_id).ok() else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        quake_live_engine.com_printf("The player is currently not active.\n");
        return;
    }

    quake_live_engine.com_printf("Slapping...\n");

    let Some(client) = Client::try_from(client_id).ok() else { return; };
    let message = if dmg != 0 {
        format!(
            "print \"{}^7 was slapped for {} damage!\n\"\n",
            client.get_name(),
            dmg
        )
    } else {
        format!("print \"{}^7 was slapped\n\"\n", client.get_name())
    };

    quake_live_engine.send_server_command(None, &message);

    let mut rng = rand::thread_rng();
    let Some(client) = client_entity.get_game_client() else {
        return;
    };
    let mut mutable_client = client;
    mutable_client.set_velocity((
        (rng.gen_range(-1.0..=1.0) * 200.0),
        (rng.gen_range(-1.0..=1.0) * 200.0),
        300.0,
    ));
    let old_health = client_entity.get_health();
    let mut mutable_client_entity = client_entity;
    mutable_client_entity.set_health(old_health - dmg);
    if old_health - dmg <= 0 {
        quake_live_engine.game_add_event(
            &mutable_client_entity,
            EV_DEATH1,
            mutable_client_entity.get_client_number(),
        );
        return;
    }
    quake_live_engine.game_add_event(&mutable_client_entity, EV_PAIN, 99);
}

#[no_mangle]
pub extern "C" fn cmd_slay() {
    let quake_live_engine = QuakeLiveEngine::default();
    let argc = quake_live_engine.cmd_argc();

    if argc < 2 {
        let Some(command_name) = quake_live_engine.cmd_argv(0) else { return; };
        let usage_note = format!("Usage: {} <client_id> [damage]\n", command_name);

        quake_live_engine.com_printf(usage_note.as_str());
        return;
    }

    let Some(passed_client_id_str) = quake_live_engine.cmd_argv(1) else {
        return;
    };
    let maxclients = unsafe { SV_MAXCLIENTS };
    let Some(client_id) = passed_client_id_str.parse::<i32>().ok() else {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients-1
        );
        quake_live_engine.com_printf(usage_note.as_str());
        return;
    };

    if client_id >= maxclients {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients - 1
        );
        quake_live_engine.com_printf(usage_note.as_str());
        return;
    }

    let Some(client_entity) = GameEntity::try_from(client_id).ok() else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        quake_live_engine.com_printf("The player is currently not active.\n");
        return;
    }

    quake_live_engine.com_printf("Slaying player...\n");

    let Some(client) = Client::try_from(client_id).ok() else { return; };

    let message = format!("print \"{}^7 was slain!\n\"\n", client.get_name());

    quake_live_engine.send_server_command(None, &message);

    let mut mutable_client_entity = client_entity;
    mutable_client_entity.set_health(-40);
    quake_live_engine.game_add_event(
        &mutable_client_entity,
        EV_GIB_PLAYER,
        mutable_client_entity.get_client_number(),
    );
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn RconDispatcher(cmd: *const c_char);
}

#[no_mangle]
// Execute a pyminqlx command as if it were the owner executing it.
// Output will appear in the console.
pub extern "C" fn cmd_py_rcon() {
    let Some(commands) = QuakeLiveEngine::default().cmd_args() else { return;
    };
    #[cfg(not(feature = "cdispatchers"))]
    rcon_dispatcher(commands);
    #[cfg(feature = "cdispatchers")]
    {
        let c_cmd = CString::new(commands).unwrap();
        unsafe { RconDispatcher(c_cmd.into_raw()) }
    }
}

#[no_mangle]
pub extern "C" fn cmd_py_command() {
    let Some(custom_command_handler) = (unsafe { CUSTOM_COMMAND_HANDLER.as_ref() }) else { return; };
    Python::with_gil(|py| {
        let quake_live_engine = QuakeLiveEngine::default();
        let result = match quake_live_engine.cmd_args() {
            None => custom_command_handler.call0(py),
            Some(args) => custom_command_handler.call1(py, (args,)),
        };
        if result.is_err() || !result.unwrap().is_true(py).unwrap() {
            quake_live_engine
                .com_printf("The command failed to be executed. pyshinqlx found no handler.\n");
        }
    });
}

#[cfg(feature = "cembed")]
extern "C" {
    fn PyMinqlx_Finalize() -> PyMinqlx_InitStatus_t;
    fn PyMinqlx_Initialize() -> PyMinqlx_InitStatus_t;
}

#[cfg(feature = "cdispatchers")]
extern "C" {
    fn NewGameDispatcher(restart: c_int);
}

#[no_mangle]
pub extern "C" fn cmd_restart_python() {
    QuakeLiveEngine::default().com_printf("Restarting Python...\n");
    if pyminqlx_is_initialized() {
        #[cfg(feature = "cembed")]
        unsafe {
            PyMinqlx_Finalize()
        };
        #[cfg(not(feature = "cembed"))]
        {
            pyminqlx_reload();
            // minqlx initializes after the first new game starts, but since the game already
            // start, we manually trigger the event to make it initialize properly.
            #[cfg(feature = "cdispatchers")]
            unsafe {
                NewGameDispatcher(0)
            };
            #[cfg(not(feature = "cdispatchers"))]
            new_game_dispatcher(false);
            return;
        }
    }
    #[cfg(feature = "cembed")]
    unsafe {
        PyMinqlx_Initialize()
    };
    #[cfg(not(feature = "cembed"))]
    pyminqlx_initialize();

    // minqlx initializes after the first new game starts, but since the game already
    // start, we manually trigger the event to make it initialize properly.
    #[cfg(feature = "cdispatchers")]
    unsafe {
        NewGameDispatcher(0)
    };
    #[cfg(not(feature = "cdispatchers"))]
    new_game_dispatcher(false);
}
