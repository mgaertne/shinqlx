use crate::pyminqlx::HANDLERS;
use crate::quake_common::entity_event_t::{EV_DEATH1, EV_GIB_PLAYER, EV_PAIN};
use crate::quake_common::{
    Client, CmdArgc, CmdArgs, CmdArgv, ComPrintf, GameAddEvent, GameEntity, QuakeLiveEngine,
    SendServerCommand,
};
use crate::{PyMinqlx_InitStatus_t, SV_MAXCLIENTS};
use pyo3::Python;
use rand::Rng;
use std::ffi::{c_char, c_int, CString};

#[no_mangle]
pub extern "C" fn cmd_send_server_command() {
    let Some(cmd_args) = QuakeLiveEngine::cmd_args() else { return; };

    let server_command = format!("{}\n", cmd_args);
    QuakeLiveEngine::send_server_command(None, &server_command);
}

#[no_mangle]
pub extern "C" fn cmd_center_print() {
    let Some(cmd_args) = QuakeLiveEngine::cmd_args() else { return; };

    let server_command = format!("cp \"{}\"\n", cmd_args);
    QuakeLiveEngine::send_server_command(None, &server_command);
}

#[no_mangle]
pub extern "C" fn cmd_regular_print() {
    let Some(cmd_args) = QuakeLiveEngine::cmd_args() else { return; };

    let server_command = format!("print \"{}\n\"\n", cmd_args);
    QuakeLiveEngine::send_server_command(None, &server_command);
}

#[no_mangle]
pub extern "C" fn cmd_slap() {
    let argc = QuakeLiveEngine::cmd_argc();

    if argc < 2 {
        let Some(command_name) = QuakeLiveEngine::cmd_argv(0) else {return; };
        let usage_note = format!("Usage: {} <client_id> [damage]\n", command_name);

        QuakeLiveEngine::com_printf(usage_note.as_str());
        return;
    }

    let Some(passed_client_id_str) = QuakeLiveEngine::cmd_argv(1) else {
        return;
    };
    let Some(client_id) = passed_client_id_str.parse::<i32>().ok() else {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            *SV_MAXCLIENTS.lock().unwrap()
        );
        QuakeLiveEngine::com_printf(usage_note.as_str());
        return;
    };

    if client_id > *SV_MAXCLIENTS.lock().unwrap() {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            *SV_MAXCLIENTS.lock().unwrap()
        );
        QuakeLiveEngine::com_printf(usage_note.as_str());
        return;
    }

    let dmg = if argc > 2 {
        let passed_dmg = QuakeLiveEngine::cmd_argv(2).unwrap();
        passed_dmg.parse::<i32>().unwrap_or(0)
    } else {
        0
    };

    let Some(client_entity) = GameEntity::try_from(client_id).ok() else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        QuakeLiveEngine::com_printf("The player is currently not active.\n");
        return;
    }

    QuakeLiveEngine::com_printf("Slapping...\n");

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

    QuakeLiveEngine::send_server_command(None, &message);

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
        QuakeLiveEngine::game_add_event(
            &mutable_client_entity,
            EV_DEATH1,
            mutable_client_entity.get_client_number(),
        );
        return;
    }
    QuakeLiveEngine::game_add_event(&mutable_client_entity, EV_PAIN, 99);
}

#[no_mangle]
pub extern "C" fn cmd_slay() {
    let argc = QuakeLiveEngine::cmd_argc();

    if argc < 2 {
        let Some(command_name) = QuakeLiveEngine::cmd_argv(0) else { return; };
        let usage_note = format!("Usage: {} <client_id> [damage]\n", command_name);

        QuakeLiveEngine::com_printf(usage_note.as_str());
        return;
    }

    let Some(passed_client_id_str) = QuakeLiveEngine::cmd_argv(1) else {
        return;
    };
    let Some(client_id) = passed_client_id_str.parse::<i32>().ok() else {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            *SV_MAXCLIENTS.lock().unwrap()
        );
        QuakeLiveEngine::com_printf(usage_note.as_str());
        return;
    };

    if client_id > *SV_MAXCLIENTS.lock().unwrap() {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            *SV_MAXCLIENTS.lock().unwrap()
        );
        QuakeLiveEngine::com_printf(usage_note.as_str());
        return;
    }

    let Some(client_entity) = GameEntity::try_from(client_id).ok() else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        QuakeLiveEngine::com_printf("The player is currently not active.\n");
        return;
    }

    QuakeLiveEngine::com_printf("Slaying player...\n");

    let Some(client) = Client::try_from(client_id).ok() else { return; };

    let message = format!("print \"{}^7 was slain!\n\"\n", client.get_name());

    QuakeLiveEngine::send_server_command(None, &message);

    let mut mutable_client_entity = client_entity;
    mutable_client_entity.set_health(-40);
    QuakeLiveEngine::game_add_event(
        &mutable_client_entity,
        EV_GIB_PLAYER,
        mutable_client_entity.get_client_number(),
    );
}

extern "C" {
    fn RconDispatcher(cmd: *const c_char);
}

#[no_mangle]
// Execute a pyminqlx command as if it were the owner executing it.
// Output will appear in the console.
pub extern "C" fn cmd_py_rcon() {
    let Some(commands) = QuakeLiveEngine::cmd_args() else { return;
    };
    unsafe { RconDispatcher(CString::new(commands).unwrap().into_raw()) }
}

#[no_mangle]
pub extern "C" fn cmd_py_command() {
    if let Some(python_function) = HANDLERS.lock().unwrap().get("custom_command") {
        Python::with_gil(|py| {
            let result = match QuakeLiveEngine::cmd_args() {
                None => python_function.call0(py),
                Some(args) => python_function.call1(py, (args,)),
            };
            if result.is_err() || !result.unwrap().is_true(py).unwrap() {
                QuakeLiveEngine::com_printf(
                    "The command failed to be executed. pyshinqlx found no handler.\n",
                );
            }
        });
    };
}

extern "C" {
    fn PyMinqlx_IsInitiailized() -> c_int;
    fn PyMinqlx_Finalze() -> PyMinqlx_InitStatus_t;
    fn PyMinqlx_Initialize() -> PyMinqlx_InitStatus_t;
    fn NewGameDispatcher(restart: c_int);
}

#[no_mangle]
pub extern "C" fn cmd_restart_python() {
    QuakeLiveEngine::com_printf("Restarting Python...\n");
    if unsafe { PyMinqlx_IsInitiailized() != 0 } {
        unsafe { PyMinqlx_Finalze() };
    }
    unsafe { PyMinqlx_Initialize() };

    // minqlx initializes after the first new game starts, but since the game already
    // start, we manually trigger the event to make it initialize properly.
    unsafe { NewGameDispatcher(0) };
}
