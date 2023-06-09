use crate::client::Client;
use crate::game_entity::GameEntity;
use crate::pyminqlx::{
    new_game_dispatcher, pyminqlx_initialize, pyminqlx_is_initialized, pyminqlx_reload,
    rcon_dispatcher, CUSTOM_COMMAND_HANDLER,
};
use crate::quake_live_engine::{
    CmdArgc, CmdArgs, CmdArgv, ComPrintf, GameAddEvent, SendServerCommand,
};
use crate::quake_types::entity_event_t::{EV_DEATH1, EV_GIB_PLAYER, EV_PAIN};
use crate::MAIN_ENGINE;
use pyo3::Python;
use rand::Rng;

#[no_mangle]
pub extern "C" fn cmd_send_server_command() {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    let Some(cmd_args) = quake_live_engine.cmd_args() else { return; };

    let server_command = format!("{}\n", cmd_args);
    quake_live_engine.send_server_command(None, server_command.as_str());
}

#[no_mangle]
pub extern "C" fn cmd_center_print() {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    let Some(cmd_args) = quake_live_engine.cmd_args() else { return; };

    let server_command = format!("cp \"{}\"\n", cmd_args);
    quake_live_engine.send_server_command(None, server_command.as_str());
}

#[no_mangle]
pub extern "C" fn cmd_regular_print() {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    let Some(cmd_args) = quake_live_engine.cmd_args() else { return; };

    let server_command = format!("print \"{}\n\"\n", cmd_args);
    quake_live_engine.send_server_command(None, server_command.as_str());
}

#[no_mangle]
pub extern "C" fn cmd_slap() {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
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
    let maxclients = quake_live_engine.get_max_clients();
    let Some(client_id) = passed_client_id_str.parse::<i32>().ok() else {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients-1
        );
        quake_live_engine.com_printf(usage_note.as_str());
        return;
    };

    if client_id < 0 || client_id >= maxclients {
        let usage_note = format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients - 1
        );
        quake_live_engine.com_printf(usage_note.as_str());
        return;
    }

    let dmg = if argc > 2 {
        let passed_dmg = quake_live_engine.cmd_argv(2).unwrap_or("0");
        passed_dmg.parse::<i32>().unwrap_or(0)
    } else {
        0
    };

    let Some(mut client_entity) = GameEntity::try_from(client_id).ok() else {
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

    quake_live_engine.send_server_command(None, message.as_str());

    let mut rng = rand::thread_rng();
    let Ok(mut game_client) = client_entity.get_game_client() else {
        return;
    };
    game_client.set_velocity((
        (rng.gen_range(-1.0..=1.0) * 200.0),
        (rng.gen_range(-1.0..=1.0) * 200.0),
        300.0,
    ));
    let old_health = client_entity.get_health();
    client_entity.set_health(old_health - dmg);
    if old_health - dmg <= 0 {
        let client_number = client_entity.get_client_number();
        quake_live_engine.game_add_event(&mut client_entity, EV_DEATH1, client_number);
        return;
    }
    quake_live_engine.game_add_event(&mut client_entity, EV_PAIN, 99);
}

#[no_mangle]
pub extern "C" fn cmd_slay() {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
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
    let maxclients = quake_live_engine.get_max_clients();
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

    let Some(mut client_entity) = GameEntity::try_from(client_id).ok() else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        quake_live_engine.com_printf("The player is currently not active.\n");
        return;
    }

    quake_live_engine.com_printf("Slaying player...\n");

    let Some(client) = Client::try_from(client_id).ok() else { return; };

    let message = format!("print \"{}^7 was slain!\n\"\n", client.get_name());

    quake_live_engine.send_server_command(None, message.as_str());

    client_entity.set_health(-40);
    let client_number = client_entity.get_client_number();
    quake_live_engine.game_add_event(&mut client_entity, EV_GIB_PLAYER, client_number);
}

#[no_mangle]
// Execute a pyminqlx command as if it were the owner executing it.
// Output will appear in the console.
pub extern "C" fn cmd_py_rcon() {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    let Some(commands) = quake_live_engine.cmd_args() else { return;
    };
    rcon_dispatcher(commands.as_str());
}

#[no_mangle]
pub extern "C" fn cmd_py_command() {
    let Ok(custom_command_lock) = CUSTOM_COMMAND_HANDLER.try_read() else {
        return;
    };

    let Some(ref custom_command_handler) = *custom_command_lock else { return; };

    Python::with_gil(|py| {
        let Some(quake_live_engine) = MAIN_ENGINE.get() else {
            return;
        };
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

#[no_mangle]
pub extern "C" fn cmd_restart_python() {
    let Some(quake_live_engine) = MAIN_ENGINE.get() else {
        return;
    };
    quake_live_engine.com_printf("Restarting Python...\n");

    if pyminqlx_is_initialized() {
        pyminqlx_reload();
        // minqlx initializes after the first new game starts, but since the game already
        // start, we manually trigger the event to make it initialize properly.
        new_game_dispatcher(false);
        return;
    }
    pyminqlx_initialize();

    // minqlx initializes after the first new game starts, but since the game already
    // start, we manually trigger the event to make it initialize properly.
    new_game_dispatcher(false);
}
