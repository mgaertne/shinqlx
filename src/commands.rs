use core::{borrow::BorrowMut, hint::cold_path};

use pyo3::types::PyBool;
use rand::Rng;
use tap::{TapOptional, TryConv};

use crate::{
    MAIN_ENGINE,
    ffi::{c::prelude::*, python::prelude::*},
    prelude::*,
    quake_live_engine::{CmdArgc, CmdArgs, CmdArgv, ComPrintf, GameAddEvent, SendServerCommand},
};

#[unsafe(no_mangle)]
pub extern "C" fn cmd_send_server_command() {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        main_engine.cmd_args().tap_some(|cmd_args| {
            main_engine.send_server_command(None::<Client>, &format!("{cmd_args}\n"));
        });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn cmd_center_print() {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        main_engine.cmd_args().tap_some(|cmd_args| {
            main_engine.send_server_command(None::<Client>, &format!("cp \"{cmd_args}\"\n"));
        });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn cmd_regular_print() {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        main_engine.cmd_args().tap_some(|cmd_args| {
            main_engine.send_server_command(None::<Client>, &format!("print \"{cmd_args}\n\"\n"));
        });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn cmd_slap() {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        let maxclients = main_engine.get_max_clients();

        let argc = main_engine.cmd_argc();

        if argc < 2 {
            let Some(command_name) = main_engine.cmd_argv(0) else {
                cold_path();
                return;
            };

            main_engine.com_printf(&format!("Usage: {command_name} <client_id> [damage]\n"));
            return;
        }

        let Some(passed_client_id_str) = main_engine.cmd_argv(1) else {
            cold_path();
            return;
        };

        let Ok(client_id) = passed_client_id_str.parse::<i32>() else {
            main_engine.com_printf(&format!(
                "client_id must be a number between 0 and {}.\n",
                maxclients - 1
            ));
            return;
        };

        if !(0..maxclients).contains(&client_id) {
            main_engine.com_printf(&format!(
                "client_id must be a number between 0 and {}.\n",
                maxclients - 1
            ));
            return;
        }

        let dmg = if argc > 2 {
            main_engine
                .cmd_argv(2)
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(0)
        } else {
            0
        };

        #[cfg_attr(test, allow(irrefutable_let_patterns))]
        let Ok(mut client_entity) = client_id.try_conv::<GameEntity>() else {
            cold_path();
            return;
        };
        if !client_entity.in_use() || client_entity.get_health() <= 0 {
            main_engine.com_printf("The player is currently not active.\n");
            return;
        }

        main_engine.com_printf("Slapping...\n");

        #[cfg_attr(test, allow(irrefutable_let_patterns))]
        let Ok(client) = client_id.try_conv::<Client>() else {
            cold_path();
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

        let mut rng = rand::rng();
        let Ok(mut game_client) = client_entity.get_game_client() else {
            cold_path();
            return;
        };
        game_client.set_velocity((
            rng.random_range(-1.0..=1.0) * 200.0,
            rng.random_range(-1.0..=1.0) * 200.0,
            300.0,
        ));
        if dmg > 0 {
            let old_health = client_entity.get_health();
            client_entity.set_health(old_health - dmg);
            if old_health <= dmg {
                let client_number = client_entity.get_client_number();
                main_engine.game_add_event(
                    client_entity.borrow_mut(),
                    entity_event_t::EV_DEATH1,
                    client_number,
                );
                return;
            }
        }
        main_engine.game_add_event(&mut client_entity, entity_event_t::EV_PAIN, 99);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn cmd_slay() {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        let maxclients = main_engine.get_max_clients();

        let argc = main_engine.cmd_argc();

        if argc < 2 {
            let Some(command_name) = main_engine.cmd_argv(0) else {
                cold_path();
                return;
            };

            main_engine.com_printf(&format!("Usage: {command_name} <client_id> [damage]\n"));
            return;
        }

        let Some(passed_client_id_str) = main_engine.cmd_argv(1) else {
            cold_path();
            return;
        };

        let Ok(client_id) = passed_client_id_str.parse::<i32>() else {
            main_engine.com_printf(&format!(
                "client_id must be a number between 0 and {}.\n",
                maxclients - 1
            ));
            return;
        };

        if !(0..maxclients).contains(&client_id) {
            main_engine.com_printf(&format!(
                "client_id must be a number between 0 and {}.\n",
                maxclients - 1
            ));
            return;
        }

        #[cfg_attr(test, allow(irrefutable_let_patterns))]
        let Ok(mut client_entity) = client_id.try_conv::<GameEntity>() else {
            cold_path();
            return;
        };
        if !client_entity.in_use() || client_entity.get_health() <= 0 {
            main_engine.com_printf("The player is currently not active.\n");
            return;
        }

        main_engine.com_printf("Slaying player...\n");

        #[cfg_attr(test, allow(irrefutable_let_patterns))]
        let Ok(client) = client_id.try_conv::<Client>() else {
            cold_path();
            return;
        };

        main_engine.send_server_command(
            None::<Client>,
            &format!("print \"{}^7 was slain!\n\"\n", client.get_name()),
        );

        client_entity.set_health(-40);
        let client_number = client_entity.get_client_number();
        main_engine.game_add_event(
            client_entity.borrow_mut(),
            entity_event_t::EV_GIB_PLAYER,
            client_number,
        );
    });
}

#[unsafe(no_mangle)]
// Execute a pyshinqlx command as if it were the owner executing it.
// Output will appear in the console.
pub extern "C" fn cmd_py_rcon() {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        let Some(commands) = main_engine.cmd_args() else {
            cold_path();
            return;
        };

        rcon_dispatcher(commands);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn cmd_py_command() {
    CUSTOM_COMMAND_HANDLER
        .load()
        .as_ref()
        .tap_some(|&custom_command_handler| {
            MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
                let cmd_args = main_engine.cmd_args();

                Python::with_gil(|py| {
                    let result = match cmd_args {
                        None => custom_command_handler.call0(py),
                        Some(args) => custom_command_handler.call1(py, (args,)),
                    };

                    if result.is_err()
                        || result.is_ok_and(|value| {
                            value
                                .bind(py)
                                .downcast::<PyBool>()
                                .is_ok_and(|bool_value| !bool_value.is_true())
                        })
                    {
                        main_engine.com_printf(
                            "The command failed to be executed. pyshinqlx found no handler.\n",
                        );
                    }
                });
            });
        });
}

#[unsafe(no_mangle)]
pub extern "C" fn cmd_restart_python() {
    MAIN_ENGINE.load().as_ref().tap_some(|&main_engine| {
        main_engine.com_printf("Restarting Python...\n");

        match pyshinqlx_is_initialized() {
            true if pyshinqlx_reload().is_ok() => {
                // shinqlx initializes after the first new game starts, but since the game already
                // start, we manually trigger the event to make it initialize properly.
                new_game_dispatcher(false);
            }
            false if pyshinqlx_initialize().is_ok() => {
                // shinqlx initializes after the first new game starts, but since the game already
                // start, we manually trigger the event to make it initialize properly.
                new_game_dispatcher(false);
            }
            _ => (),
        }
    });
}

#[cfg(test)]
mod commands_tests {
    use mockall::predicate;
    use pyo3::{intern, types::PyBool};
    use rstest::rstest;

    use super::{
        cmd_center_print, cmd_py_command, cmd_py_rcon, cmd_regular_print, cmd_restart_python,
        cmd_send_server_command, cmd_slap, cmd_slay,
    };
    use crate::{
        ffi::{
            c::prelude::*,
            python::{
                prelude::*,
                pyshinqlx_test_support::{
                    python_function_raising_exception, python_function_returning,
                },
            },
        },
        prelude::*,
    };

    #[test]
    #[serial]
    fn cmd_send_server_command_with_no_main_engine() {
        cmd_send_server_command()
    }

    #[test]
    #[serial]
    fn cmd_send_server_command_with_no_args() {
        MockEngineBuilder::default()
            .with_send_server_command(|_client, _cmd| true, 0)
            .with_args(None, 1)
            .run(|| {
                cmd_send_server_command();
            });
    }

    #[test]
    #[serial]
    fn cmd_send_server_command_with_server_command() {
        MockEngineBuilder::default()
            .with_send_server_command(|client, command| client.is_none() && command == "asdf\n", 1)
            .with_args(Some("asdf"), 1)
            .run(|| {
                cmd_send_server_command();
            });
    }

    #[test]
    #[serial]
    fn cmd_center_print_with_no_main_engine() {
        cmd_center_print();
    }

    #[test]
    #[serial]
    fn cmd_center_print_with_no_args() {
        MockEngineBuilder::default()
            .with_send_server_command(|_client, _cmd| true, 0)
            .with_args(None, 1)
            .run(|| {
                cmd_center_print();
            });
    }

    #[test]
    #[serial]
    fn cmd_center_print_with_server_command() {
        MockEngineBuilder::default()
            .with_send_server_command(
                |client, command| client.is_none() && command == "cp \"asdf\"\n",
                1,
            )
            .with_args(Some("asdf"), 1)
            .run(|| {
                cmd_center_print();
            });
    }

    #[test]
    #[serial]
    fn cmd_regular_print_with_no_main_engine() {
        cmd_regular_print();
    }

    #[test]
    #[serial]
    fn cmd_regular_print_with_no_args() {
        MockEngineBuilder::default()
            .with_send_server_command(|_client, _cmd| true, 0)
            .with_args(None, 1)
            .run(|| {
                cmd_regular_print();
            });
    }

    #[test]
    #[serial]
    fn cmd_regular_print_with_server_command() {
        MockEngineBuilder::default()
            .with_send_server_command(
                |client, command| client.is_none() && command == "print \"asdf\n\"\n",
                1,
            )
            .with_args(Some("asdf"), 1)
            .run(|| {
                cmd_regular_print();
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_no_main_engine() {
        cmd_slap();
    }

    #[test]
    #[serial]
    fn cmd_slap_with_too_few_args() {
        MockEngineBuilder::default()
            .with_max_clients(16)
            .with_com_printf(predicate::eq("Usage: !slap <client_id> [damage]\n"), 1)
            .with_argc(1)
            .with_argv(predicate::eq(0), Some("!slap"), 1)
            .run(|| {
                cmd_slap();
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_unparseable_client_id() {
        MockEngineBuilder::default()
            .with_max_clients(16)
            .with_com_printf(
                predicate::eq("client_id must be a number between 0 and 15.\n"),
                1,
            )
            .with_argc(2)
            .with_argv(predicate::eq(1), Some("2147483648"), 1)
            .run(|| {
                cmd_slap();
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_too_small_client_id() {
        MockEngineBuilder::default()
            .with_max_clients(16)
            .with_com_printf(
                predicate::eq("client_id must be a number between 0 and 15.\n"),
                1,
            )
            .with_argc(2)
            .with_argv(predicate::eq(1), Some("-1"), 1)
            .run(|| {
                cmd_slap();
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_too_large_client_id() {
        MockEngineBuilder::default()
            .with_max_clients(16)
            .with_com_printf(
                predicate::eq("client_id must be a number between 0 and 15.\n"),
                1,
            )
            .with_argc(2)
            .with_argv(predicate::eq(1), Some("42"), 1)
            .run(|| {
                cmd_slap();
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_game_entity_not_in_use() {
        MockGameEntityBuilder::default()
            .with_in_use(false, 1)
            .run(predicate::eq(2), || {
                MockEngineBuilder::default()
                    .with_max_clients(16)
                    .with_com_printf(predicate::eq("The player is currently not active.\n"), 1)
                    .with_argc(2)
                    .with_argv(predicate::eq(1), Some("2"), 1)
                    .run(|| {
                        cmd_slap();
                    });
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_game_entity_no_health() {
        MockGameEntityBuilder::default()
            .with_in_use(true, 1)
            .with_health(0, 1)
            .run(predicate::eq(2), || {
                MockEngineBuilder::default()
                    .with_max_clients(16)
                    .with_com_printf(predicate::eq("The player is currently not active.\n"), 1)
                    .with_argc(2)
                    .with_argv(predicate::eq(1), Some("2"), 1)
                    .run(|| {
                        cmd_slap();
                    });
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_no_damage_provided_slaps() {
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

        MockGameEntityBuilder::default()
            .with_in_use(true, 1)
            .with_health(200, 1)
            .with_game_client(|| {
                let mut game_client_mock = MockGameClient::default();
                game_client_mock
                    .expect_set_velocity::<(f32, f32, f32)>()
                    .times(1);
                Ok(game_client_mock)
            })
            .run(predicate::eq(2), || {
                MockEngineBuilder::default()
                    .with_max_clients(16)
                    .with_com_printf(predicate::eq("Slapping...\n"), 1)
                    .with_send_server_command(
                        |client, cmd| {
                            client.is_none() && cmd == "print \"Slapped Player^7 was slapped\n\"\n"
                        },
                        1,
                    )
                    .with_argc(2)
                    .with_argv(predicate::eq(1), Some("2"), 1)
                    .configure(|mock_engine| {
                        mock_engine
                            .expect_game_add_event()
                            .withf(|_entity, &entity_event, &event_param| {
                                entity_event == entity_event_t::EV_PAIN && event_param == 99
                            })
                            .times(1);
                    })
                    .run(|| {
                        cmd_slap();
                    });
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_provided_damage_slaps() {
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

        MockGameEntityBuilder::default()
            .with_in_use(true, 1)
            .with_health(200, 1..)
            .with_set_health(predicate::eq(199), 1)
            .with_game_client(|| {
                let mut game_client_mock = MockGameClient::default();
                game_client_mock
                    .expect_set_velocity::<(f32, f32, f32)>()
                    .times(1);
                Ok(game_client_mock)
            })
            .run(predicate::eq(2), || {
                MockEngineBuilder::default()
                    .with_max_clients(16)
                    .with_com_printf(predicate::eq("Slapping...\n"), 1)
                    .with_send_server_command(
                        |client, cmd| {
                            client.is_none()
                                && cmd == "print \"Slapped Player^7 was slapped for 1 damage!\n\"\n"
                        },
                        1,
                    )
                    .with_argc(3)
                    .with_argv(predicate::eq(1), Some("2"), 1)
                    .with_argv(predicate::eq(2), Some("1"), 1)
                    .configure(|mock_engine| {
                        mock_engine
                            .expect_game_add_event()
                            .withf(|_entity, &entity_event, &event_param| {
                                entity_event == entity_event_t::EV_PAIN && event_param == 99
                            })
                            .times(1);
                    })
                    .run(|| {
                        cmd_slap();
                    });
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_provided_damage_provided_slaps_and_kills() {
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

        MockGameEntityBuilder::default()
            .with_in_use(true, 1)
            .with_health(200, 1..)
            .with_set_health(predicate::eq(-466), 1)
            .with_client_number(42, 1)
            .with_game_client(|| {
                let mut game_client_mock = MockGameClient::default();
                game_client_mock
                    .expect_set_velocity::<(f32, f32, f32)>()
                    .times(1);
                Ok(game_client_mock)
            })
            .run(predicate::eq(2), || {
                MockEngineBuilder::default()
                    .with_max_clients(16)
                    .with_com_printf(predicate::eq("Slapping...\n"), 1)
                    .with_send_server_command(
                        |client, cmd| {
                            client.is_none()
                                && cmd
                                    == "print \"Slapped Player^7 was slapped for 666 damage!\n\"\n"
                        },
                        1,
                    )
                    .with_argc(3)
                    .with_argv(predicate::eq(1), Some("2"), 1)
                    .with_argv(predicate::eq(2), Some("666"), 1)
                    .configure(|mock_engine| {
                        mock_engine
                            .expect_game_add_event()
                            .withf(|_entity, &entity_event, &event_param| {
                                entity_event == entity_event_t::EV_DEATH1 && event_param == 42
                            })
                            .times(1);
                    })
                    .run(|| {
                        cmd_slap();
                    });
            });
    }

    #[test]
    #[serial]
    fn cmd_slap_with_unparseable_provided_damage_slaps() {
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

        MockGameEntityBuilder::default()
            .with_in_use(true, 1)
            .with_health(200, 1)
            .with_game_client(|| {
                let mut game_client_mock = MockGameClient::default();
                game_client_mock
                    .expect_set_velocity::<(f32, f32, f32)>()
                    .times(1);
                Ok(game_client_mock)
            })
            .run(predicate::eq(2), || {
                MockEngineBuilder::default()
                    .with_max_clients(16)
                    .with_com_printf(predicate::eq("Slapping...\n"), 1)
                    .with_send_server_command(
                        |client, cmd| {
                            client.is_none() && cmd == "print \"Slapped Player^7 was slapped\n\"\n"
                        },
                        1,
                    )
                    .with_argc(3)
                    .with_argv(predicate::eq(1), Some("2"), 1)
                    .with_argv(predicate::eq(2), Some("2147483648"), 1)
                    .configure(|mock_engine| {
                        mock_engine
                            .expect_game_add_event()
                            .withf(|_entity, &entity_event, &event_param| {
                                entity_event == entity_event_t::EV_PAIN && event_param == 99
                            })
                            .times(1);
                    })
                    .run(|| {
                        cmd_slap();
                    });
            });
    }

    #[test]
    #[serial]
    fn cmd_sly_with_not_main_engine() {
        cmd_slay();
    }

    #[test]
    #[serial]
    fn cmd_slay_with_too_few_args() {
        MockEngineBuilder::default()
            .with_max_clients(16)
            .with_com_printf(predicate::eq("Usage: !slap <client_id> [damage]\n"), 1)
            .with_argc(1)
            .with_argv(predicate::eq(0), Some("!slap"), 1)
            .run(|| {
                cmd_slay();
            });
    }

    #[test]
    #[serial]
    fn cmd_slay_with_unparseable_client_id() {
        MockEngineBuilder::default()
            .with_max_clients(16)
            .with_com_printf(
                predicate::eq("client_id must be a number between 0 and 15.\n"),
                1,
            )
            .with_argc(2)
            .with_argv(predicate::eq(1), Some("2147483648"), 1)
            .run(|| {
                cmd_slay();
            });
    }

    #[test]
    #[serial]
    fn cmd_slay_with_too_small_client_id() {
        MockEngineBuilder::default()
            .with_max_clients(16)
            .with_com_printf(
                predicate::eq("client_id must be a number between 0 and 15.\n"),
                1,
            )
            .with_argc(2)
            .with_argv(predicate::eq(1), Some("-1"), 1)
            .run(|| {
                cmd_slay();
            });
    }

    #[test]
    #[serial]
    fn cmd_slay_with_too_large_client_id() {
        MockEngineBuilder::default()
            .with_max_clients(16)
            .with_com_printf(
                predicate::eq("client_id must be a number between 0 and 15.\n"),
                1,
            )
            .with_argc(2)
            .with_argv(predicate::eq(1), Some("42"), 1)
            .run(|| {
                cmd_slay();
            });
    }

    #[test]
    #[serial]
    fn cmd_slay_with_game_entity_not_in_use() {
        MockGameEntityBuilder::default()
            .with_in_use(false, 1)
            .run(predicate::eq(2), || {
                MockEngineBuilder::default()
                    .with_max_clients(16)
                    .with_com_printf(predicate::eq("The player is currently not active.\n"), 1)
                    .with_argc(2)
                    .with_argv(predicate::eq(1), Some("2"), 1)
                    .run(|| {
                        cmd_slay();
                    });
            });
    }

    #[test]
    #[serial]
    fn cmd_slay_with_game_entity_no_health() {
        MockGameEntityBuilder::default()
            .with_in_use(true, 1)
            .with_health(0, 1)
            .run(predicate::eq(2), || {
                MockEngineBuilder::default()
                    .with_max_clients(16)
                    .with_com_printf(predicate::eq("The player is currently not active.\n"), 1)
                    .with_argc(2)
                    .with_argv(predicate::eq(1), Some("2"), 1)
                    .run(|| {
                        cmd_slay();
                    });
            });
    }

    #[test]
    #[serial]
    fn cmd_slay_player_is_slain() {
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

        MockGameEntityBuilder::default()
            .with_in_use(true, 1)
            .with_health(200, 1)
            .with_set_health(predicate::lt(0), 1)
            .with_client_number(42, 1)
            .run(predicate::eq(2), || {
                MockEngineBuilder::default()
                    .with_max_clients(16)
                    .with_com_printf(predicate::eq("Slaying player...\n"), 1)
                    .with_send_server_command(
                        |client, cmd| {
                            client.is_none() && cmd == "print \"Slain Player^7 was slain!\n\"\n"
                        },
                        1,
                    )
                    .with_argc(2)
                    .with_argv(predicate::eq(1), Some("2"), 1)
                    .configure(|mock_engine| {
                        mock_engine
                            .expect_game_add_event()
                            .withf(|_entity, &entity_event, &event_param| {
                                entity_event == entity_event_t::EV_GIB_PLAYER && event_param == 42
                            })
                            .times(1);
                    })
                    .run(|| {
                        cmd_slay();
                    });
            });
    }

    #[test]
    #[serial]
    fn cmdpy_rcon_with_no_main_engine() {
        cmd_py_rcon();
    }

    #[test]
    #[serial]
    fn cmd_py_rcon_with_no_args() {
        let rcon_dispatcher_ctx = rcon_dispatcher_context();
        rcon_dispatcher_ctx.expect::<&str>().times(0);

        MockEngineBuilder::default().with_args(None, 1).run(|| {
            cmd_py_rcon();
        });
    }

    #[test]
    #[serial]
    fn cmd_py_rcon_forwards_args() {
        let rcon_dispatcher_ctx = rcon_dispatcher_context();
        rcon_dispatcher_ctx
            .expect::<String>()
            .with(predicate::eq("!version".to_string()))
            .times(1);

        MockEngineBuilder::default()
            .with_args(Some("!version"), 1)
            .run(|| {
                cmd_py_rcon();
            });
    }

    #[test]
    #[serial]
    fn cmd_py_command_with_no_main_engine() {
        cmd_py_command();
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_py_command_with_arguments(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_com_printf(predicate::always(), 0)
            .with_args(Some("custom parameter"), 1)
            .run(|| {
                Python::with_gil(|py| {
                    let pymodule = PyModule::from_code(
                        py,
                        cr#"
def handler(params):
    return (params == "custom parameter")
"#,
                        c"",
                        c"",
                    )
                    .expect("this should not happen");
                    let custom_command_handler = pymodule
                        .getattr(intern!(py, "handler"))
                        .expect("this should not happen");
                    CUSTOM_COMMAND_HANDLER.store(Some(custom_command_handler.unbind().into()));

                    cmd_py_command();
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_py_command_with_no_args(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_com_printf(predicate::always(), 0)
            .with_args(None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let custom_command_handler =
                        python_function_returning(py, &PyBool::new(py, true));
                    CUSTOM_COMMAND_HANDLER.store(Some(custom_command_handler.unbind().into()));

                    cmd_py_command();
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_py_command_returns_error(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_com_printf(
                predicate::eq("The command failed to be executed. pyshinqlx found no handler.\n"),
                1,
            )
            .with_args(None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let custom_command_handler = python_function_raising_exception(py);
                    CUSTOM_COMMAND_HANDLER.store(Some(custom_command_handler.unbind().into()));

                    cmd_py_command();
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cmd_py_command_returns_false(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_com_printf(
                predicate::eq("The command failed to be executed. pyshinqlx found no handler.\n"),
                1,
            )
            .with_args(None, 1)
            .run(|| {
                Python::with_gil(|py| {
                    let custom_command_handler =
                        python_function_returning(py, &PyBool::new(py, false));
                    CUSTOM_COMMAND_HANDLER.store(Some(custom_command_handler.unbind().into()));

                    cmd_py_command();
                });
            });
    }

    #[test]
    #[serial]
    fn cmd_restart_python_with_no_main_engine() {
        cmd_restart_python();
    }

    #[test]
    #[serial]
    fn cmd_restart_python_already_initialized() {
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

        MockEngineBuilder::default()
            .with_com_printf(predicate::eq("Restarting Python...\n"), 1)
            .run(|| {
                cmd_restart_python();
            });
    }

    #[test]
    #[serial]
    fn cmd_restart_python_already_initialized_reload_fails() {
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

        MockEngineBuilder::default()
            .with_com_printf(predicate::eq("Restarting Python...\n"), 1)
            .run(|| {
                cmd_restart_python();
            });
    }

    #[test]
    #[serial]
    fn cmd_restart_python_not_previously_initialized() {
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

        MockEngineBuilder::default()
            .with_com_printf(predicate::eq("Restarting Python...\n"), 1)
            .run(|| {
                cmd_restart_python();
            });
    }

    #[test]
    #[serial]
    fn cmd_restart_python_not_previously_initialized_initialize_fails() {
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

        MockEngineBuilder::default()
            .with_com_printf(predicate::eq("Restarting Python...\n"), 1)
            .run(|| {
                cmd_restart_python();
            });
    }
}
