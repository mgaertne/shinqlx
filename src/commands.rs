use crate::client::Client;
use crate::game_entity::GameEntity;
use crate::prelude::*;
use crate::pyminqlx::{
    new_game_dispatcher, pyminqlx_initialize, pyminqlx_is_initialized, pyminqlx_reload,
    rcon_dispatcher, CUSTOM_COMMAND_HANDLER,
};
use crate::quake_live_engine::{
    CmdArgc, CmdArgs, CmdArgv, ComPrintf, GameAddEvent, SendServerCommand,
};
use crate::MAIN_ENGINE;
use pyo3::Python;
use rand::Rng;

#[no_mangle]
pub extern "C" fn cmd_send_server_command() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    cmd_send_server_command_intern(main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_send_server_command_intern<T>(main_engine: &T)
where
    T: CmdArgs + SendServerCommand<Client, String>,
{
    let Some(cmd_args) = main_engine.cmd_args() else {
        return;
    };

    main_engine.send_server_command(None::<Client>, format!("{}\n", cmd_args));
}

#[no_mangle]
pub extern "C" fn cmd_center_print() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    cmd_center_print_intern(main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_center_print_intern<T>(main_engine: &T)
where
    T: CmdArgs + SendServerCommand<Client, String>,
{
    let Some(cmd_args) = main_engine.cmd_args() else {
        return;
    };

    main_engine.send_server_command(None::<Client>, format!("cp \"{}\"\n", cmd_args));
}

#[no_mangle]
pub extern "C" fn cmd_regular_print() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    cmd_regular_print_intern(main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_regular_print_intern<T>(main_engine: &T)
where
    T: CmdArgs + SendServerCommand<Client, String>,
{
    let Some(cmd_args) = main_engine.cmd_args() else {
        return;
    };

    main_engine.send_server_command(None::<Client>, format!("print \"{}\n\"\n", cmd_args));
}

#[no_mangle]
pub extern "C" fn cmd_slap() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    let maxclients = main_engine.get_max_clients();

    cmd_slap_intern(maxclients, main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_slap_intern<T, U>(maxclients: T, main_engine: &U)
where
    T: Into<i32> + Copy,
    U: CmdArgc
        + CmdArgv<i32>
        + ComPrintf<String>
        + for<'b> GameAddEvent<&'b mut GameEntity, i32>
        + SendServerCommand<Client, String>,
{
    let argc = main_engine.cmd_argc();

    if argc < 2 {
        let Some(command_name) = main_engine.cmd_argv(0) else {
            return;
        };

        main_engine.com_printf(format!("Usage: {} <client_id> [damage]\n", command_name));
        return;
    }

    let Some(passed_client_id_str) = main_engine.cmd_argv(1) else {
        return;
    };

    let Some(client_id) = passed_client_id_str.parse::<i32>().ok() else {
        main_engine.com_printf(format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients.into() - 1
        ));
        return;
    };

    if client_id < 0 || client_id >= maxclients.into() {
        main_engine.com_printf(format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients.into() - 1
        ));
        return;
    }

    let dmg = if argc > 2 {
        let passed_dmg = main_engine.cmd_argv(2).unwrap_or("0");
        passed_dmg.parse::<i32>().unwrap_or(0)
    } else {
        0
    };

    let Some(mut client_entity) = GameEntity::try_from(client_id).ok() else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        main_engine.com_printf("The player is currently not active.\n".to_string());
        return;
    }

    main_engine.com_printf("Slapping...\n".to_string());

    let Some(client) = Client::try_from(client_id).ok() else {
        return;
    };
    let message = if dmg != 0 {
        format!(
            "print \"{}^7 was slapped for {} damage!\n\"\n",
            client.get_name(),
            dmg
        )
    } else {
        format!("print \"{}^7 was slapped\n\"\n", client.get_name())
    };

    main_engine.send_server_command(None::<Client>, message);

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
        main_engine.game_add_event(&mut client_entity, entity_event_t::EV_DEATH1, client_number);
        return;
    }
    main_engine.game_add_event(&mut client_entity, entity_event_t::EV_PAIN, 99);
}

#[no_mangle]
pub extern "C" fn cmd_slay() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    let maxclients = main_engine.get_max_clients();

    cmd_slay_intern(maxclients, main_engine);
}

#[cfg_attr(not(test), inline)]
fn cmd_slay_intern<T, U>(maxclients: T, main_engine: &U)
where
    T: Into<i32> + Copy,
    U: CmdArgc
        + CmdArgv<i32>
        + ComPrintf<String>
        + for<'a> ComPrintf<&'a str>
        + for<'b> GameAddEvent<&'b mut GameEntity, i32>
        + SendServerCommand<Client, String>,
{
    let argc = main_engine.cmd_argc();

    if argc < 2 {
        let Some(command_name) = main_engine.cmd_argv(0) else {
            return;
        };

        main_engine.com_printf(format!("Usage: {} <client_id> [damage]\n", command_name));
        return;
    }

    let Some(passed_client_id_str) = main_engine.cmd_argv(1) else {
        return;
    };

    let Some(client_id) = passed_client_id_str.parse::<i32>().ok() else {
        main_engine.com_printf(format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients.into() - 1
        ));
        return;
    };

    if client_id >= maxclients.into() {
        main_engine.com_printf(format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients.into() - 1
        ));
        return;
    }

    let Some(mut client_entity) = GameEntity::try_from(client_id).ok() else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        main_engine.com_printf("The player is currently not active.\n");
        return;
    }

    main_engine.com_printf("Slaying player...\n");

    let Some(client) = Client::try_from(client_id).ok() else {
        return;
    };

    main_engine.send_server_command(
        None::<Client>,
        format!("print \"{}^7 was slain!\n\"\n", client.get_name()),
    );

    client_entity.set_health(-40);
    let client_number = client_entity.get_client_number();
    main_engine.game_add_event(
        &mut client_entity,
        entity_event_t::EV_GIB_PLAYER,
        client_number,
    );
}

#[no_mangle]
// Execute a pyminqlx command as if it were the owner executing it.
// Output will appear in the console.
pub extern "C" fn cmd_py_rcon() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    let Some(commands) = main_engine.cmd_args() else {
        return;
    };

    rcon_dispatcher(commands);
}

#[no_mangle]
pub extern "C" fn cmd_py_command() {
    let Some(custom_command_lock) = CUSTOM_COMMAND_HANDLER.try_read() else {
        return;
    };

    let Some(ref custom_command_handler) = *custom_command_lock else {
        return;
    };

    Python::with_gil(|py| {
        let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
            return;
        };

        let Some(ref main_engine) = *main_engine_guard else {
            return;
        };

        let result = match main_engine.cmd_args() {
            None => custom_command_handler.call0(py),
            Some(args) => custom_command_handler.call1(py, (args,)),
        };

        if result.is_err() || !result.unwrap().is_true(py).unwrap() {
            main_engine
                .com_printf("The command failed to be executed. pyshinqlx found no handler.\n");
        }
    });
}

#[no_mangle]
pub extern "C" fn cmd_restart_python() {
    let Some(main_engine_guard) = MAIN_ENGINE.try_read() else {
        return;
    };

    let Some(ref main_engine) = *main_engine_guard else {
        return;
    };

    main_engine.com_printf("Restarting Python...\n");

    if pyminqlx_is_initialized() {
        if pyminqlx_reload().is_err() {
            return;
        };
        // minqlx initializes after the first new game starts, but since the game already
        // start, we manually trigger the event to make it initialize properly.
        new_game_dispatcher(false);
        return;
    }

    if pyminqlx_initialize().is_err() {
        return;
    };

    // minqlx initializes after the first new game starts, but since the game already
    // start, we manually trigger the event to make it initialize properly.
    new_game_dispatcher(false);
}

#[cfg(test)]
pub(crate) mod commands_tests {
    use crate::client::Client;
    use crate::commands::{
        cmd_center_print_intern, cmd_regular_print_intern, cmd_send_server_command_intern,
        cmd_slap_intern,
    };
    use crate::game_entity::GameEntity;
    use crate::quake_live_engine::{
        CmdArgc, CmdArgs, CmdArgv, ComPrintf, GameAddEvent, SendServerCommand,
    };
    use crate::quake_types::entity_event_t;
    use mockall::predicate::eq;
    use mockall::*;

    #[test]
    fn cmd_send_server_command_with_no_args() {
        mock! {
            QuakeEngine {}
            impl CmdArgs for QuakeEngine {
                fn cmd_args(&self) -> Option<String>;
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None);
        mock.expect_send_server_command().times(0);

        cmd_send_server_command_intern(&mock);
    }

    #[test]
    fn cmd_send_server_command_with_server_command() {
        mock! {
            QuakeEngine {}
            impl CmdArgs for QuakeEngine {
                fn cmd_args(&self) -> Option<String>;
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args()
            .return_once_st(|| Some("asdf".to_string()));
        mock.expect_send_server_command()
            .withf_st(move |client, command| client.is_none() && command == "asdf\n")
            .return_const(());

        cmd_send_server_command_intern(&mock);
    }

    #[test]
    fn cmd_center_print_with_no_args() {
        mock! {
            QuakeEngine {}
            impl CmdArgs for QuakeEngine {
                fn cmd_args(&self) -> Option<String>;
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None);
        mock.expect_send_server_command().times(0);

        cmd_center_print_intern(&mock);
    }

    #[test]
    fn cmd_center_print_with_server_command() {
        mock! {
            QuakeEngine {}
            impl CmdArgs for QuakeEngine {
                fn cmd_args(&self) -> Option<String>;
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args()
            .return_once_st(|| Some("asdf".to_string()));
        mock.expect_send_server_command()
            .withf_st(move |client, command| client.is_none() && command == "cp \"asdf\"\n")
            .return_const(());

        cmd_center_print_intern(&mock);
    }

    #[test]
    fn cmd_regular_print_with_no_args() {
        mock! {
            QuakeEngine {}
            impl CmdArgs for QuakeEngine {
                fn cmd_args(&self) -> Option<String>;
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args().return_once_st(|| None);
        mock.expect_send_server_command().times(0);

        cmd_regular_print_intern(&mock);
    }

    #[test]
    fn cmd_regular_print_with_server_command() {
        mock! {
            QuakeEngine {}
            impl CmdArgs for QuakeEngine {
                fn cmd_args(&self) -> Option<String>;
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_args()
            .return_once_st(|| Some("asdf".to_string()));
        mock.expect_send_server_command()
            .withf_st(move |client, command| client.is_none() && command == "print \"asdf\n\"\n")
            .return_const(());

        cmd_regular_print_intern(&mock);
    }

    #[test]
    fn cmd_slap_with_too_few_args() {
        mock! {
            QuakeEngine {}
            impl CmdArgc for QuakeEngine {
                fn cmd_argc(&self) -> i32;
            }
            impl CmdArgv<i32> for QuakeEngine {
                fn cmd_argv(&self, argno: i32) -> Option<&'static str>;
            }
            impl ComPrintf<String> for QuakeEngine {
                fn com_printf(&self, msg: String);
            }
            impl GameAddEvent<&mut GameEntity, i32> for QuakeEngine {
                fn game_add_event(&self, game_entity: &mut GameEntity, event: entity_event_t, event_param: i32);
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 1);
        mock.expect_cmd_argv()
            .with(eq(0))
            .return_once_st(|_| Some("!slap"));
        mock.expect_com_printf()
            .withf_st(|text| text == "Usage: !slap <client_id> [damage]\n")
            .return_const(());

        cmd_slap_intern(16, &mock);
    }

    #[test]
    fn cmd_slap_with_unparseable_client_id() {
        mock! {
            QuakeEngine {}
            impl CmdArgc for QuakeEngine {
                fn cmd_argc(&self) -> i32;
            }
            impl CmdArgv<i32> for QuakeEngine {
                fn cmd_argv(&self, argno: i32) -> Option<&'static str>;
            }
            impl ComPrintf<String> for QuakeEngine {
                fn com_printf(&self, msg: String);
            }
            impl GameAddEvent<&mut GameEntity, i32> for QuakeEngine {
                fn game_add_event(&self, game_entity: &mut GameEntity, event: entity_event_t, event_param: i32);
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2);
        mock.expect_cmd_argv()
            .with(eq(1))
            .return_once_st(|_| Some("2147483648"));
        mock.expect_com_printf()
            .withf_st(|text| text == "client_id must be a number between 0 and 15.\n")
            .return_const(());

        cmd_slap_intern(16, &mock);
    }

    #[test]
    fn cmd_slap_with_too_small_client_id() {
        mock! {
            QuakeEngine {}
            impl CmdArgc for QuakeEngine {
                fn cmd_argc(&self) -> i32;
            }
            impl CmdArgv<i32> for QuakeEngine {
                fn cmd_argv(&self, argno: i32) -> Option<&'static str>;
            }
            impl ComPrintf<String> for QuakeEngine {
                fn com_printf(&self, msg: String);
            }
            impl GameAddEvent<&mut GameEntity, i32> for QuakeEngine {
                fn game_add_event(&self, game_entity: &mut GameEntity, event: entity_event_t, event_param: i32);
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2);
        mock.expect_cmd_argv()
            .with(eq(1))
            .return_once_st(|_| Some("-1"));
        mock.expect_com_printf()
            .withf_st(|text| text == "client_id must be a number between 0 and 15.\n")
            .return_const(());

        cmd_slap_intern(16, &mock);
    }

    #[test]
    fn cmd_slap_with_too_large_client_id() {
        mock! {
            QuakeEngine {}
            impl CmdArgc for QuakeEngine {
                fn cmd_argc(&self) -> i32;
            }
            impl CmdArgv<i32> for QuakeEngine {
                fn cmd_argv(&self, argno: i32) -> Option<&'static str>;
            }
            impl ComPrintf<String> for QuakeEngine {
                fn com_printf(&self, msg: String);
            }
            impl GameAddEvent<&mut GameEntity, i32> for QuakeEngine {
                fn game_add_event(&self, game_entity: &mut GameEntity, event: entity_event_t, event_param: i32);
            }
            impl SendServerCommand<Client, String> for QuakeEngine {
                fn send_server_command(&self, client: Option<Client>, command: String);
            }
        }

        let mut mock = MockQuakeEngine::new();
        mock.expect_cmd_argc().return_once_st(|| 2);
        mock.expect_cmd_argv()
            .with(eq(1))
            .return_once_st(|_| Some("42"));
        mock.expect_com_printf()
            .withf_st(|text| text == "client_id must be a number between 0 and 15.\n")
            .return_const(());

        cmd_slap_intern(16, &mock);
    }
}
