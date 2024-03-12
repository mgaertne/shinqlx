use crate::ffi::c::prelude::*;
use crate::ffi::python::prelude::*;
use crate::prelude::*;
use crate::quake_live_engine::{
    CmdArgc, CmdArgs, CmdArgv, ComPrintf, GameAddEvent, SendServerCommand,
};
use crate::MAIN_ENGINE;

use rand::Rng;

#[no_mangle]
pub extern "C" fn cmd_send_server_command() {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    let Some(cmd_args) = main_engine.cmd_args() else {
        return;
    };

    main_engine.send_server_command(None::<Client>, &format!("{}\n", cmd_args));
}

#[no_mangle]
pub extern "C" fn cmd_center_print() {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    let Some(cmd_args) = main_engine.cmd_args() else {
        return;
    };

    main_engine.send_server_command(None::<Client>, &format!("cp \"{}\"\n", cmd_args));
}

#[no_mangle]
pub extern "C" fn cmd_regular_print() {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    let Some(cmd_args) = main_engine.cmd_args() else {
        return;
    };

    main_engine.send_server_command(None::<Client>, &format!("print \"{}\n\"\n", cmd_args));
}

#[no_mangle]
pub extern "C" fn cmd_slap() {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    let maxclients = main_engine.get_max_clients();

    let argc = main_engine.cmd_argc();

    if argc < 2 {
        let Some(command_name) = main_engine.cmd_argv(0) else {
            return;
        };

        main_engine.com_printf(&format!("Usage: {} <client_id> [damage]\n", command_name));
        return;
    }

    let Some(passed_client_id_str) = main_engine.cmd_argv(1) else {
        return;
    };

    let Ok(client_id) = passed_client_id_str.parse::<i32>() else {
        main_engine.com_printf(&format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients - 1
        ));
        return;
    };

    if client_id < 0 || client_id >= maxclients {
        main_engine.com_printf(&format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients - 1
        ));
        return;
    }

    let dmg = if argc > 2 {
        let passed_dmg = main_engine.cmd_argv(2).unwrap_or("0".into());
        passed_dmg.parse::<i32>().unwrap_or(0)
    } else {
        0
    };

    #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
    let Ok(mut client_entity) = GameEntity::try_from(client_id) else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        main_engine.com_printf("The player is currently not active.\n");
        return;
    }

    main_engine.com_printf("Slapping...\n");

    #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
    let Ok(client) = Client::try_from(client_id) else {
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

    main_engine.send_server_command(None::<Client>, &message);

    let mut rng = rand::rngs::OsRng;
    let Ok(mut game_client) = client_entity.get_game_client() else {
        return;
    };
    game_client.set_velocity((
        rng.gen_range(-1.0..=1.0) * 200.0,
        rng.gen_range(-1.0..=1.0) * 200.0,
        300.0,
    ));
    if dmg > 0 {
        let old_health = client_entity.get_health();
        client_entity.set_health(old_health - dmg);
        if old_health - dmg <= 0 {
            let client_number = client_entity.get_client_number();
            main_engine.game_add_event(
                &mut client_entity,
                entity_event_t::EV_DEATH1,
                client_number,
            );
            return;
        }
    }
    main_engine.game_add_event(&mut client_entity, entity_event_t::EV_PAIN, 99);
}

#[no_mangle]
pub extern "C" fn cmd_slay() {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    let maxclients = main_engine.get_max_clients();

    let argc = main_engine.cmd_argc();

    if argc < 2 {
        let Some(command_name) = main_engine.cmd_argv(0) else {
            return;
        };

        main_engine.com_printf(&format!("Usage: {} <client_id> [damage]\n", command_name));
        return;
    }

    let Some(passed_client_id_str) = main_engine.cmd_argv(1) else {
        return;
    };

    let Ok(client_id) = passed_client_id_str.parse::<i32>() else {
        main_engine.com_printf(&format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients - 1
        ));
        return;
    };

    if client_id < 0 || client_id >= maxclients {
        main_engine.com_printf(&format!(
            "client_id must be a number between 0 and {}.\n",
            maxclients - 1
        ));
        return;
    }

    #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
    let Ok(mut client_entity) = GameEntity::try_from(client_id) else {
        return;
    };
    if !client_entity.in_use() || client_entity.get_health() <= 0 {
        main_engine.com_printf("The player is currently not active.\n");
        return;
    }

    main_engine.com_printf("Slaying player...\n");

    #[cfg_attr(test, allow(clippy::unnecessary_fallible_conversions))]
    let Ok(client) = Client::try_from(client_id) else {
        return;
    };

    main_engine.send_server_command(
        None::<Client>,
        &format!("print \"{}^7 was slain!\n\"\n", client.get_name()),
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
// Execute a pyshinqlx command as if it were the owner executing it.
// Output will appear in the console.
pub extern "C" fn cmd_py_rcon() {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    let Some(commands) = main_engine.cmd_args() else {
        return;
    };

    rcon_dispatcher(commands);
}

#[no_mangle]
pub extern "C" fn cmd_py_command() {
    let Some(ref custom_command_handler) = *CUSTOM_COMMAND_HANDLER.load() else {
        return;
    };

    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    let cmd_args = main_engine.cmd_args();

    Python::with_gil(|py| {
        let result = match cmd_args {
            None => custom_command_handler.call0(py),
            Some(args) => custom_command_handler.call1(py, (args,)),
        };

        if result.is_err()
            || result.is_ok_and(|value| value.is_truthy(py).is_ok_and(|result| !result))
        {
            main_engine
                .com_printf("The command failed to be executed. pyshinqlx found no handler.\n");
        }
    });
}

#[no_mangle]
pub extern "C" fn cmd_restart_python() {
    let Some(ref main_engine) = *MAIN_ENGINE.load() else {
        return;
    };

    main_engine.com_printf("Restarting Python...\n");

    if pyshinqlx_is_initialized() {
        if pyshinqlx_reload().is_err() {
            return;
        };
        // shinqlx initializes after the first new game starts, but since the game already
        // start, we manually trigger the event to make it initialize properly.
        new_game_dispatcher(false);
        return;
    }

    if pyshinqlx_initialize().is_err() {
        return;
    };

    // shinqlx initializes after the first new game starts, but since the game already
    // start, we manually trigger the event to make it initialize properly.
    new_game_dispatcher(false);
}

#[cfg(test)]
mod commands_tests {
    use super::MAIN_ENGINE;
    use super::{
        cmd_center_print, cmd_py_command, cmd_py_rcon, cmd_regular_print, cmd_restart_python,
        cmd_send_server_command, cmd_slap, cmd_slay,
    };
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use rstest::rstest;
    use serial_test::serial;

    #[test]
    #[serial]
    fn cmd_send_server_command_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        cmd_send_server_command()
    }

    #[test]
    #[serial]
    fn cmd_send_server_command_with_no_args() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_args().times(1);
        mock_engine.expect_send_server_command().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_send_server_command();
    }

    #[test]
    #[serial]
    fn cmd_send_server_command_with_server_command() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_cmd_args()
            .return_const(Some("asdf".into()))
            .times(1);
        mock_engine
            .expect_send_server_command()
            .withf(|client, command| client.is_none() && command == "asdf\n")
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_send_server_command();
    }

    #[test]
    #[serial]
    fn cmd_center_print_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        cmd_center_print();
    }

    #[test]
    #[serial]
    fn cmd_center_print_with_no_args() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_args().times(1);
        mock_engine.expect_send_server_command().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_center_print();
    }

    #[test]
    #[serial]
    fn cmd_center_print_with_server_command() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_cmd_args()
            .return_const(Some("asdf".into()))
            .times(1);
        mock_engine
            .expect_send_server_command()
            .withf(|client, command| client.is_none() && command == "cp \"asdf\"\n")
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_center_print();
    }

    #[test]
    #[serial]
    fn cmd_regular_print_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        cmd_regular_print();
    }

    #[test]
    #[serial]
    fn cmd_regular_print_with_no_args() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_args().times(1);
        mock_engine.expect_send_server_command().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_regular_print();
    }

    #[test]
    #[serial]
    fn cmd_regular_print_with_server_command() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_cmd_args()
            .return_const(Some("asdf".into()))
            .times(1);
        mock_engine
            .expect_send_server_command()
            .withf(|client, command| client.is_none() && command == "print \"asdf\n\"\n")
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_regular_print();
    }

    #[test]
    #[serial]
    fn cmd_slap_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        cmd_slap();
    }

    #[test]
    #[serial]
    fn cmd_slap_with_too_few_args() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(1).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(0))
            .return_const(Some("!slap".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Usage: !slap <client_id> [damage]\n"))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_slap();
    }

    #[test]
    #[serial]
    fn cmd_slap_with_unparseable_client_id() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2147483648".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq(
                "client_id must be a number between 0 and 15.\n",
            ))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_slap();
    }

    #[test]
    #[serial]
    fn cmd_slap_with_too_small_client_id() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("-1".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq(
                "client_id must be a number between 0 and 15.\n",
            ))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_slap();
    }

    #[test]
    #[serial]
    fn cmd_slap_with_too_large_client_id() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("42".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq(
                "client_id must be a number between 0 and 15.\n",
            ))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_slap();
    }

    //noinspection DuplicatedCode
    #[test]
    #[serial]
    fn cmd_slap_with_game_entity_not_in_use() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("The player is currently not active.\n"))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock
                    .expect_in_use()
                    .return_const(false)
                    .times(1);
                game_entity_mock
            })
            .times(1);

        cmd_slap();
    }

    //noinspection DuplicatedCode
    #[test]
    #[serial]
    fn cmd_slap_with_game_entity_no_health() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("The player is currently not active.\n"))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().return_const(true).times(1);
                game_entity_mock
                    .expect_get_health()
                    .return_const(0)
                    .times(1);
                game_entity_mock
            })
            .times(1);

        cmd_slap();
    }

    //noinspection DuplicatedCode
    #[test]
    #[serial]
    fn cmd_slap_with_no_damage_provided_slaps() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Slapping...\n"))
            .times(1);
        mock_engine
            .expect_send_server_command()
            .withf(|client, cmd| {
                client.is_none() && cmd == "print \"Slapped Player^7 was slapped\n\"\n"
            })
            .times(1);
        mock_engine
            .expect_game_add_event()
            .withf(|_entity, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_PAIN && event_param == 99
            })
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().return_const(true).times(1);
                game_entity_mock
                    .expect_get_health()
                    .return_const(200)
                    .times(1);
                game_entity_mock
                    .expect_get_game_client()
                    .return_once(|| {
                        let mut game_client_mock = MockGameClient::default();
                        game_client_mock
                            .expect_set_velocity::<(f32, f32, f32)>()
                            .times(1);
                        Ok(game_client_mock)
                    })
                    .times(1);
                game_entity_mock
            })
            .times(1);
        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .return_const("Slapped Player")
                    .times(1);
                client_mock
            })
            .times(1);

        cmd_slap();
    }

    //noinspection DuplicatedCode
    #[test]
    #[serial]
    fn cmd_slap_with_provided_damage_slaps() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(3).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2".into()))
            .times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(2))
            .return_const(Some("1".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Slapping...\n"))
            .times(1);
        mock_engine
            .expect_send_server_command()
            .withf(|client, cmd| {
                client.is_none()
                    && cmd == "print \"Slapped Player^7 was slapped for 1 damage!\n\"\n"
            })
            .times(1);
        mock_engine
            .expect_game_add_event()
            .withf(|_entity, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_PAIN && event_param == 99
            })
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().return_const(true).times(1);
                game_entity_mock
                    .expect_get_health()
                    .return_const(200)
                    .times(1..);
                game_entity_mock
                    .expect_set_health()
                    .with(predicate::eq(199))
                    .times(1);
                game_entity_mock
                    .expect_get_game_client()
                    .return_once(|| {
                        let mut game_client_mock = MockGameClient::default();
                        game_client_mock
                            .expect_set_velocity::<(f32, f32, f32)>()
                            .times(1);
                        Ok(game_client_mock)
                    })
                    .times(1);
                game_entity_mock
            })
            .times(1);
        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .return_const("Slapped Player")
                    .times(1);
                client_mock
            })
            .times(1);

        cmd_slap();
    }

    //noinspection DuplicatedCode
    #[test]
    #[serial]
    fn cmd_slap_with_provided_damage_provided_slaps_and_kills() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(3).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2".into()))
            .times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(2))
            .return_const(Some("666".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Slapping...\n"))
            .times(1);
        mock_engine
            .expect_send_server_command()
            .withf(|client, cmd| {
                client.is_none()
                    && cmd == "print \"Slapped Player^7 was slapped for 666 damage!\n\"\n"
            })
            .times(1);
        mock_engine
            .expect_game_add_event()
            .withf(|_entity, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_DEATH1 && event_param == 42
            })
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().return_const(true).times(1);
                game_entity_mock
                    .expect_get_health()
                    .return_const(200)
                    .times(1..);
                game_entity_mock
                    .expect_set_health()
                    .with(predicate::eq(-466))
                    .times(1);
                game_entity_mock
                    .expect_get_game_client()
                    .return_once(|| {
                        let mut game_client_mock = MockGameClient::default();
                        game_client_mock
                            .expect_set_velocity::<(f32, f32, f32)>()
                            .times(1);
                        Ok(game_client_mock)
                    })
                    .times(1);
                game_entity_mock
                    .expect_get_client_number()
                    .return_const(42)
                    .times(1);
                game_entity_mock
            })
            .times(1);
        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .return_const("Slapped Player")
                    .times(1);
                client_mock
            })
            .times(1);

        cmd_slap()
    }

    //noinspection DuplicatedCode
    #[test]
    #[serial]
    fn cmd_slap_with_unparseable_provided_damage_slaps() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(3).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2".into()))
            .times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(2))
            .return_const(Some("2147483648".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Slapping...\n"))
            .times(1);
        mock_engine
            .expect_send_server_command()
            .withf(|client, cmd| {
                client.is_none() && cmd == "print \"Slapped Player^7 was slapped\n\"\n"
            })
            .times(1);
        mock_engine
            .expect_game_add_event()
            .withf(|_entity, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_PAIN && event_param == 99
            })
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().return_const(true).times(1);
                game_entity_mock
                    .expect_get_health()
                    .return_const(200)
                    .times(1);
                game_entity_mock
                    .expect_get_game_client()
                    .return_once(|| {
                        let mut game_client_mock = MockGameClient::default();
                        game_client_mock
                            .expect_set_velocity::<(f32, f32, f32)>()
                            .times(1);
                        Ok(game_client_mock)
                    })
                    .times(1);
                game_entity_mock
            })
            .times(1);
        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .return_const("Slapped Player")
                    .times(1);
                client_mock
            })
            .times(1);

        cmd_slap();
    }

    #[test]
    #[serial]
    fn cmd_sly_with_not_main_engine() {
        MAIN_ENGINE.store(None);
        cmd_slay();
    }

    #[test]
    #[serial]
    fn cmd_slay_with_too_few_args() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(1).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(0))
            .return_const(Some("!slap".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Usage: !slap <client_id> [damage]\n"))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_slay();
    }

    #[test]
    #[serial]
    fn cmd_slay_with_unparseable_client_id() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2147483648".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq(
                "client_id must be a number between 0 and 15.\n",
            ))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_slay();
    }

    #[test]
    #[serial]
    fn cmd_slay_with_too_small_client_id() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("-1".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq(
                "client_id must be a number between 0 and 15.\n",
            ))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_slay();
    }

    #[test]
    #[serial]
    fn cmd_slay_with_too_large_client_id() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("42".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq(
                "client_id must be a number between 0 and 15.\n",
            ))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        cmd_slay();
    }

    //noinspection DuplicatedCode
    #[test]
    #[serial]
    fn cmd_slay_with_game_entity_not_in_use() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("The player is currently not active.\n"))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock
                    .expect_in_use()
                    .return_const(false)
                    .times(1);
                game_entity_mock
            })
            .times(1);

        cmd_slay();
    }

    //noinspection DuplicatedCode
    #[test]
    #[serial]
    fn cmd_slay_with_game_entity_no_health() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("The player is currently not active.\n"))
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().return_const(true).times(1);
                game_entity_mock
                    .expect_get_health()
                    .return_const(0)
                    .times(1);
                game_entity_mock
            })
            .times(1);

        cmd_slay();
    }

    #[test]
    #[serial]
    fn cmd_slay_player_is_slain() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_argc().return_const(2).times(1);
        mock_engine
            .expect_cmd_argv()
            .with(predicate::eq(1))
            .return_const(Some("2".into()))
            .times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Slaying player...\n"))
            .times(1);
        mock_engine
            .expect_send_server_command()
            .withf(|client, cmd| {
                client.is_none() && cmd == "print \"Slain Player^7 was slain!\n\"\n"
            })
            .times(1);
        mock_engine
            .expect_game_add_event()
            .withf(|_entity, &entity_event, &event_param| {
                entity_event == entity_event_t::EV_GIB_PLAYER && event_param == 42
            })
            .times(1);
        mock_engine.expect_get_max_clients().return_const(16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut game_entity_mock = MockGameEntity::default();
                game_entity_mock.expect_in_use().return_const(true).times(1);
                game_entity_mock
                    .expect_get_health()
                    .return_const(200)
                    .times(1);
                game_entity_mock
                    .expect_set_health()
                    .with(predicate::lt(0))
                    .times(1);
                game_entity_mock
                    .expect_get_client_number()
                    .return_const(42)
                    .times(1);
                game_entity_mock
            })
            .times(1);
        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(2))
            .return_once(|_| {
                let mut client_mock = MockClient::default();
                client_mock
                    .expect_get_name()
                    .return_const("Slain Player")
                    .times(1);
                client_mock
            })
            .times(1);

        cmd_slay();
    }

    #[test]
    #[serial]
    fn cmdpy_rcon_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        cmd_py_rcon();
    }

    #[test]
    #[serial]
    fn cmd_py_rcon_with_no_args() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_args().return_const(None).times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let rcon_dispatcher_ctx = rcon_dispatcher_context();
        rcon_dispatcher_ctx.expect::<&str>().times(0);

        cmd_py_rcon()
    }

    #[test]
    #[serial]
    fn cmd_py_rcon_forwards_args() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_cmd_args()
            .return_const(Some("!version".into()))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let rcon_dispatcher_ctx = rcon_dispatcher_context();
        rcon_dispatcher_ctx
            .expect::<String>()
            .with(predicate::eq("!version".to_string()))
            .times(1);

        cmd_py_rcon();
    }

    #[test]
    #[serial]
    fn cmd_py_command_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        cmd_py_command();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_py_command_with_arguments(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_cmd_args()
            .return_const(Some("custom parameter".into()))
            .times(1);
        mock_engine.expect_com_printf().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code_bound(
                py,
                r#"
def handler(params):
    return (params == "custom parameter")
"#,
                "",
                "",
            )
            .expect("this should not happen");
            let custom_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CUSTOM_COMMAND_HANDLER.store(Some(custom_command_handler.into_py(py).into()));

            cmd_py_command();
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_py_command_with_no_args(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_args().return_const(None).times(1);
        mock_engine.expect_com_printf().times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code_bound(
                py,
                r#"
def handler():
    return True
"#,
                "",
                "",
            )
            .expect("this should not happen");
            let custom_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CUSTOM_COMMAND_HANDLER.store(Some(custom_command_handler.into_py(py).into()));

            cmd_py_command();
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_py_command_returns_error(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_args().return_const(None).times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq(
                "The command failed to be executed. pyshinqlx found no handler.\n",
            ))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code_bound(
                py,
                r#"
def handler():
    raise Exception 
"#,
                "",
                "",
            )
            .expect("this should not happen");
            let custom_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CUSTOM_COMMAND_HANDLER.store(Some(custom_command_handler.into_py(py).into()));

            cmd_py_command();
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_py_command_returns_false(_pyshinqlx_setup: ()) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_cmd_args().return_const(None).times(1);
        mock_engine
            .expect_com_printf()
            .with(predicate::eq(
                "The command failed to be executed. pyshinqlx found no handler.\n",
            ))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let pymodule = PyModule::from_code_bound(
                py,
                r#"
def handler():
    return False 
"#,
                "",
                "",
            )
            .expect("this should not happen");
            let custom_command_handler =
                pymodule.getattr("handler").expect("this should not happen");
            CUSTOM_COMMAND_HANDLER.store(Some(custom_command_handler.into_py(py).into()));

            cmd_py_command();
        });
    }

    #[test]
    #[serial]
    fn cmd_restart_python_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        cmd_restart_python();
    }

    #[test]
    #[serial]
    fn cmd_restart_python_already_initialized() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Restarting Python...\n"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let pyshinqlx_is_initialized_ctx = pyshinqlx_is_initialized_context();
        pyshinqlx_is_initialized_ctx
            .expect()
            .return_const(true)
            .times(1);
        let pyshinqlx_reload_ctx = pyshinqlx_reload_context();
        pyshinqlx_reload_ctx.expect().return_const(Ok(())).times(1);
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx
            .expect()
            .with(predicate::eq(false))
            .times(1);

        cmd_restart_python();
    }

    #[test]
    #[serial]
    fn cmd_restart_python_already_initialized_reload_fails() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Restarting Python...\n"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let pyshinqlx_is_initialized_ctx = pyshinqlx_is_initialized_context();
        pyshinqlx_is_initialized_ctx
            .expect()
            .return_const(true)
            .times(1);
        let pyshinqlx_reload_ctx = pyshinqlx_reload_context();
        pyshinqlx_reload_ctx
            .expect()
            .return_const(Err(PythonInitializationError::NotInitializedError))
            .times(1);
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx.expect().times(0);

        cmd_restart_python();
    }

    #[test]
    #[serial]
    fn cmd_restart_python_not_previously_initialized() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Restarting Python...\n"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let pyshinqlx_is_initialized_ctx = pyshinqlx_is_initialized_context();
        pyshinqlx_is_initialized_ctx
            .expect()
            .return_const(false)
            .times(1);
        let pyshinqlx_initialize_ctx = pyshinqlx_initialize_context();
        pyshinqlx_initialize_ctx
            .expect()
            .return_const(Ok(()))
            .times(1);
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx
            .expect()
            .with(predicate::eq(false))
            .times(1);

        cmd_restart_python();
    }

    #[test]
    #[serial]
    fn cmd_restart_python_not_previously_initialized_initialize_fails() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_com_printf()
            .with(predicate::eq("Restarting Python...\n"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let pyshinqlx_is_initialized_ctx = pyshinqlx_is_initialized_context();
        pyshinqlx_is_initialized_ctx
            .expect()
            .return_const(false)
            .times(1);
        let pyshinqlx_initialize_ctx = pyshinqlx_initialize_context();
        pyshinqlx_initialize_ctx
            .expect()
            .return_const(Err(PythonInitializationError::MainScriptError))
            .times(1);
        let new_game_dispatcher_ctx = new_game_dispatcher_context();
        new_game_dispatcher_ctx.expect().times(0);

        cmd_restart_python();
    }
}
