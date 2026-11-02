use core::sync::atomic::Ordering;

use mockall::{Sequence, predicate};
use pretty_assertions::assert_eq;
use pyo3::{
    IntoPyObjectExt,
    exceptions::{PyEnvironmentError, PyKeyError, PyTypeError, PyValueError},
    intern,
    types::{IntoPyDict, PyBool, PyInt, PyString},
};
use rstest::rstest;

use super::{NonexistentPlayerError, PlayerMethods};
use crate::{
    ffi::{
        c::prelude::*,
        python::{prelude::*, pyshinqlx_test_support::*},
    },
    hooks::mock_hooks::{
        shinqlx_client_spawn_context, shinqlx_drop_client_context,
        shinqlx_execute_client_command_context, shinqlx_send_server_command_context,
    },
    prelude::*,
};

#[test]
#[serial]
fn pyconstructor_with_empty_playerinfo() {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_CONNECTED);
            mock_client.expect_get_user_info().return_const("");
            mock_client
                .expect_get_steam_id()
                .return_const(1234567890u64);
            mock_client
        });

    MockGameEntityBuilder::default()
        .with_player_name(|| "UnnamedPlayer".to_string(), 1..)
        .with_team(|| team_t::TEAM_SPECTATOR, 1..)
        .with_privileges(|| privileges_t::PRIV_NONE, 1..)
        .run(predicate::eq(2), || {
            let result = Player::py_new(2, None);
            assert_eq!(
                result.expect("result was not OK"),
                Player {
                    name: "UnnamedPlayer".to_string().into(),
                    player_info: PlayerInfo {
                        name: "UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                }
            );
        });
}

#[test]
fn player_from_player_info_with_empty_name_uses_the_userinfo_name() {
    let player = Player::from(PlayerInfo {
        userinfo: r"\name\UnnamedPlayer".to_string(),
        ..default_test_player_info()
    });

    assert_eq!(*player.name.read(), "UnnamedPlayer");
    assert_eq!(player.id, default_test_player_info().client_id);
}

#[test]
fn pyconstructor_with_empty_name() {
    let result = Player::py_new(
        2,
        Some(PlayerInfo {
            userinfo: r"\name\UnnamedPlayer".to_string(),
            ..default_test_player_info()
        }),
    );
    assert_eq!(
        result.expect("result was not OK"),
        Player {
            player_info: PlayerInfo {
                userinfo: r"\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            user_info: r"\name\UnnamedPlayer".to_string(),
            name: "UnnamedPlayer".to_string().into(),
            ..default_test_player()
        }
    );
}

#[test]
fn pyconstructor_with_empty_name_and_no_name_in_userinfo() {
    let result = Player::py_new(2, Some(default_test_player_info()));
    assert_eq!(result.expect("result was not OK"), default_test_player());
}

#[test]
fn pyconstructor_with_nonempty_playerinfo() {
    let result = Player::py_new(
        2,
        Some(PlayerInfo {
            name: "UnnamedPlayer".to_string(),
            ..default_test_player_info()
        }),
    );
    assert_eq!(
        result.expect("result was not OK"),
        Player {
            name: "UnnamedPlayer".to_string().into(),
            player_info: PlayerInfo {
                name: "UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        }
    );
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn repr_with_all_values_set(_pyshinqlx_setup: ()) {
    let result = Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    name: "UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                name: "UnnamedPlayer".to_string().into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");
        Player::__repr__(&player)
    });
    assert_eq!(result, "Player(2:'UnnamedPlayer':1234567890)");
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn repr_with_an_invalidated_player(_pyshinqlx_setup: ()) {
    let result = Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    name: "UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                name: "UnnamedPlayer".to_string().into(),
                valid: false.into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");
        Player::__repr__(&player)
    });
    assert_eq!(result, "Player(INVALID:'UnnamedPlayer':1234567890)");
}

#[test]
fn str_returns_player_name() {
    let player = Player {
        player_info: PlayerInfo {
            name: "^1Unnamed^2Player".to_string(),
            ..default_test_player_info()
        }
        .into(),
        name: "^1Unnamed^2Player".to_string().into(),
        ..default_test_player()
    };
    assert_eq!(format!("{}", player), "^1Unnamed^2Player");
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn contains_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                valid: false.into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.contains("asdf");
        assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn contains_where_value_is_part_of_userinfo(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    userinfo: r"\asdf\some value".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                user_info: r"\asdf\some value".to_string(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.contains("asdf");
        assert_eq!(result.expect("result was not OK"), true);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn contains_where_value_is_not_in_userinfo(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    userinfo: r"\name\^1Unnamed^2Player".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                user_info: r"\name\^1Unnamed^2Player".to_string(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.contains("asdf");
        assert_eq!(result.expect("result was not OK"), false);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn getitem_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                valid: false.into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_item(intern!(py, "asdf"));
        assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn getitem_where_value_is_part_of_userinfo(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    userinfo: r"\asdf\some value".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                user_info: r"\asdf\some value".to_string(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_item(intern!(py, "asdf"));
        assert_eq!(result.expect("result was not OK").to_string(), "some value");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn getitem_where_value_is_not_in_userinfo(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    userinfo: r"\name\^1Unnamed^2Player".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                user_info: r"\name\^1Unnamed^2Player".to_string(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_item(intern!(py, "asdf"));
        assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)))
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn cvars_with_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                valid: false.into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_cvars();
        assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn cvars_where_value_is_part_of_userinfo(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    userinfo: r"\asdf\some value".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                user_info: r"\asdf\some value".to_string(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_cvars();
        assert!(
            result
                .expect("result was not OK")
                .get_item(intern!(py, "asdf"))
                .is_ok_and(|opt_value| opt_value.is_some_and(|value| value
                    .extract::<String>()
                    .expect("this should not happen")
                    == "some value"))
        )
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_can_be_compared_for_equality_with_other_player_in_python(_pyshinqlx_setup: ()) {
    let player_info = PlayerInfo {
        client_id: 42,
        steam_id: 1234567890,
        ..default_test_player_info()
    };
    let player_info2 = PlayerInfo {
        client_id: 42,
        steam_id: 41,
        ..default_test_player_info()
    };
    let result = Python::attach(|py| {
        py.run(
            cr#"
import shinqlx
assert(shinqlx.Player(42, player_info) == shinqlx.Player(42, player_info))
assert((shinqlx.Player(42, player_info) == shinqlx.Player(41, player_info2)) == False)
            "#,
            None,
            Some(
                &[
                    ("player_info", player_info.into_bound_py_any(py)?),
                    ("player_info2", player_info2.into_bound_py_any(py)?),
                ]
                .into_py_dict(py)?,
            ),
        )
    });
    assert!(result.is_ok());
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_can_be_compared_for_equality_with_steam_id_in_python(_pyshinqlx_setup: ()) {
    let player_info = PlayerInfo {
        client_id: 42,
        steam_id: 1234567890,
        ..default_test_player_info()
    };
    let result = Python::attach(|py| {
        py.run(
            cr#"
import shinqlx
assert(shinqlx.Player(42, player_info) == 1234567890)
assert((shinqlx.Player(42, player_info) == 1234567891) == False)
assert((shinqlx.Player(42, player_info) == "asdf") == False)
            "#,
            None,
            Some(&[("player_info", player_info.into_bound_py_any(py)?)].into_py_dict(py)?),
        )
    });
    assert!(result.is_ok());
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_can_be_compared_for_inequality_with_other_player_in_python(_pyshinqlx_setup: ()) {
    let player_info = PlayerInfo {
        client_id: 42,
        steam_id: 1234567890,
        ..default_test_player_info()
    };
    let player_info2 = PlayerInfo {
        client_id: 42,
        steam_id: 42,
        ..default_test_player_info()
    };
    let result = Python::attach(|py| {
        py.run(
            cr#"
import shinqlx
assert((shinqlx.Player(42, player_info) != shinqlx.Player(42, player_info)) == False)
assert(shinqlx.Player(42, player_info) != shinqlx.Player(41, player_info2))
            "#,
            None,
            Some(
                &[
                    ("player_info", player_info.into_bound_py_any(py)?),
                    ("player_info2", player_info2.into_bound_py_any(py)?),
                ]
                .into_py_dict(py)?,
            ),
        )
    });
    assert!(result.is_ok());
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_can_be_compared_for_inequality_with_steam_id_in_python(_pyshinqlx_setup: ()) {
    let player_info = PlayerInfo {
        client_id: 42,
        steam_id: 1234567890,
        ..default_test_player_info()
    };
    let result = Python::attach(|py| {
        py.run(
            cr#"
import shinqlx
assert((shinqlx.Player(42, player_info) != 1234567890) == False)
assert(shinqlx.Player(42, player_info) != 1234567891)
assert(shinqlx.Player(42, player_info) != "asdf")
            "#,
            None,
            Some(&[("player_info", player_info.into_bound_py_any(py)?)].into_py_dict(py)?),
        )
    });
    assert!(result.is_ok());
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn player_can_not_be_compared_for_larger_than(_pyshinqlx_setup: ()) {
    let player_info = PlayerInfo {
        client_id: 42,
        steam_id: 1234567890,
        ..default_test_player_info()
    };
    let player_info2 = PlayerInfo {
        client_id: 42,
        steam_id: 41,
        ..default_test_player_info()
    };

    Python::attach(|py| {
        let result = py.run(
            cr#"
import shinqlx
shinqlx.Player(42, player_info) < shinqlx.Player(42, player_info)
            "#,
            None,
            Some(
                &[
                    (
                        "player_info",
                        player_info
                            .into_bound_py_any(py)
                            .expect("this should not happen"),
                    ),
                    (
                        "player_info2",
                        player_info2
                            .into_bound_py_any(py)
                            .expect("this should not happen"),
                    ),
                ]
                .into_py_dict(py)
                .expect("this should not happen"),
            ),
        );
        assert!(result.is_err_and(|err| err.is_instance_of::<PyTypeError>(py)));
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn update_with_different_steam_id(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_CONNECTED);
            mock_client.expect_get_user_info().return_const("");
            mock_client
                .expect_get_steam_id()
                .return_const(1234567891u64);
            mock_client
        });

    MockGameEntityBuilder::default()
        .with_player_name(|| "UnnamedPlayer".to_string(), 1..)
        .with_team(|| team_t::TEAM_SPECTATOR, 1..)
        .with_privileges(|| privileges_t::PRIV_NONE, 1..)
        .run(predicate::eq(2), || {
            Python::attach(|py| {
                let player = Bound::new(
                    py,
                    Player {
                        steam_id: 1234567890,
                        ..default_test_player()
                    },
                )
                .expect("this should not happen");

                let result = player.update();
                assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
                assert_eq!(player.get().valid.load(Ordering::Acquire), false);
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn update_can_be_called_from_python(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_CONNECTED);
            mock_client.expect_get_user_info().return_const("");
            mock_client
                .expect_get_steam_id()
                .return_const(1234567890u64);
            mock_client
        });

    let player = Player {
        steam_id: 1234567890,
        ..default_test_player()
    };

    MockGameEntityBuilder::default()
        .with_player_name(|| "UnnamedPlayer".to_string(), 1..)
        .with_team(|| team_t::TEAM_SPECTATOR, 1..)
        .with_privileges(|| privileges_t::PRIV_NONE, 1..)
        .run(predicate::eq(2), || {
            let result = Python::attach(|py| {
                py.run(
                    cr#"
player.update()
assert(player._valid)
            "#,
                    None,
                    Some(&[("player", Bound::new(py, player)?)].into_py_dict(py)?),
                )
            });
            assert!(result.is_ok());
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn update_updates_new_player_name(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_CONNECTED);
            mock_client.expect_get_user_info().return_const("");
            mock_client
                .expect_get_steam_id()
                .return_const(1234567890u64);
            mock_client
        });

    MockGameEntityBuilder::default()
        .with_player_name(|| "NewUnnamedPlayer".to_string(), 1..)
        .with_team(|| team_t::TEAM_SPECTATOR, 1..)
        .with_privileges(|| privileges_t::PRIV_NONE, 1..)
        .run(predicate::eq(2), || {
            Python::attach(|py| {
                let player = Bound::new(
                    py,
                    Player {
                        steam_id: 1234567890,
                        ..default_test_player()
                    },
                )
                .expect("this should not happen");

                player.update().expect("this should not happen");
                assert_eq!(player.get().valid.load(Ordering::Acquire), true);
                assert_eq!(&*player.get().name.read(), "NewUnnamedPlayer");
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn update_updates_new_player_name_from_userinfo(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(|_| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .return_const(clientState_t::CS_CONNECTED);
            mock_client
                .expect_get_user_info()
                .return_const(r"\name\NewUnnamedPlayer");
            mock_client
                .expect_get_steam_id()
                .return_const(1234567890u64);
            mock_client
        });

    MockGameEntityBuilder::default()
        .with_player_name(|| "".to_string(), 1..)
        .with_team(|| team_t::TEAM_SPECTATOR, 1..)
        .with_privileges(|| privileges_t::PRIV_NONE, 1..)
        .run(predicate::eq(2), || {
            Python::attach(|py| {
                let player = Bound::new(
                    py,
                    Player {
                        steam_id: 1234567890,
                        ..default_test_player()
                    },
                )
                .expect("this should not happen");

                player.update().expect("this should not happen");
                assert_eq!(player.get().valid.load(Ordering::Acquire), true);
                assert_eq!(&*player.get().name.read(), "NewUnnamedPlayer");
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn invalidate_invalidates_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");
        let result = player.invalidate("invalid player");
        assert_eq!(player.get().valid.load(Ordering::Acquire), false);
        assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_cvars_sets_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some() && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer""# && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.set_cvars(
                &[("asdf", "qwertz"), ("name", "UnnamedPlayer")]
                    .into_py_dict(py)
                    .expect("this should not happen"),
            );
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_ip_where_no_ip_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: "".to_string(),
                player_info: PlayerInfo {
                    userinfo: "".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_ip(), "");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_ip_for_ip_with_no_port(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\ip\127.0.0.1".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\ip\127.0.0.1".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_ip(), "127.0.0.1");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_ip_for_ip_with_port(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\ip\127.0.0.1:27666".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\ip\127.0.0.1:27666".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_ip(), "127.0.0.1");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_clan_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");
        let result = player.get_clan();
        assert_eq!(result, "");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_clan_with_no_clan_set(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring((CS_PLAYERS + 2) as u16, "", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");
                let result = player.get_clan();
                assert_eq!(result, "");
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_clan_with_clan_set(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring((CS_PLAYERS + 2) as u16, r"\cn\asdf", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");
                let result = player.get_clan();
                assert_eq!(result, "asdf");
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_clan_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        player.set_clan("asdf")
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_clan_with_no_clan_set(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring((CS_PLAYERS + 2) as u16, "", 1)
        .configure(|mock_engine| {
            mock_engine
                .expect_set_configstring()
                .withf(|index, value| {
                    *index == 531i32 && value.contains(r"\cn\clan") && value.contains(r"\xcn\clan")
                })
                .times(1);
        })
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                player.set_clan("clan")
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_clan_with_clan_set(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_get_configstring((CS_PLAYERS + 2) as u16, r"\xcn\asdf\cn\asdf", 1)
        .configure(|mock_engine| {
            mock_engine
                .expect_set_configstring()
                .withf(|index, value| {
                    *index == 531i32
                        && value.contains(r"\cn\clan")
                        && value.contains(r"\xcn\clan")
                        && !value.contains(r"\cn\asdf")
                        && !value.contains(r"\xcn\asdf")
                })
                .times(1);
        })
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                player.set_clan("clan")
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_name_for_color_terminated_name(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                name: "UnnamedPlayer^7".to_string().into(),
                player_info: PlayerInfo {
                    name: "UnnamedPlayer^7".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_name(), "UnnamedPlayer^7");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_name_for_color_unterminated_name(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                name: "UnnamedPlayer".to_string().into(),
                player_info: PlayerInfo {
                    name: "UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_name(), "UnnamedPlayer^7");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_name_updated_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some()
                && cmd == r#"userinfo "\asdf\qwertz\name\^1Unnamed^2Player""#
                && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    user_info: r"\asdf\qwertz\name\UnnamedPlayer".to_string(),
                    player_info: PlayerInfo {
                        userinfo: r"\asdf\qwertz\name\UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.set_name("^1Unnamed^2Player");
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_clean_name_returns_cleaned_name(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                name: "^7^1S^3hi^4N^10^7".to_string().into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_clean_name();
        assert_eq!(result, "ShiN0");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_qport_where_no_port_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: "".to_string(),
                player_info: PlayerInfo {
                    userinfo: "".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_qport(), -1);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_qport_for_port_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\qport\27666".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\qport\27666".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_qport(), 27666);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_qport_for_invalid_port_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\qport\asdf".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\qport\asdf".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_qport(), -1);
    });
}

#[rstest]
#[case(team_t::TEAM_FREE, "free")]
#[case(team_t::TEAM_RED, "red")]
#[case(team_t::TEAM_BLUE, "blue")]
#[case(team_t::TEAM_SPECTATOR, "spectator")]
#[cfg_attr(miri, ignore)]
fn get_team_for_team_t_values(
    _pyshinqlx_setup: (),
    #[case] team: team_t,
    #[case] return_value: &str,
) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    team: team as i32,
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_team().expect("result was not OK"), return_value)
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_team_for_invalid_team(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    team: 42,
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert!(
            player
                .get_team()
                .is_err_and(|err| err.is_instance_of::<PyValueError>(py))
        );
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_team_with_invalid_team(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let result = Bound::new(py, default_test_player())
            .expect("this should not happen")
            .set_team("invalid team");
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[case("red")]
#[case("RED")]
#[case("free")]
#[case("blue")]
#[case("spectator")]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_team_puts_player_on_a_specific_team(_pyshinqlx_setup: (), #[case] new_team: &str) {
    MockEngineBuilder::default()
        .with_execute_console_command(format!("put 2 {}", new_team.to_lowercase()), 1)
        .run(|| {
            Python::attach(|py| {
                let result = Bound::new(py, default_test_player())
                    .expect("this should not happen")
                    .set_team(new_team);
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_colors_where_no_colors_are_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: "".to_string(),
                player_info: PlayerInfo {
                    userinfo: "".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_colors(), (0.0, 0.0));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_colors_for_colors_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\color1\42\color2\21".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\color1\42\colors2\21".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_colors(), (42.0, 21.0));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_colors_for_invalid_color1_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\color1\asdf\color2\42".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\color1\asdf\color2\42".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_colors(), (0.0, 42.0));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_colors_for_invalid_color2_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\color1\42\color2\asdf".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\color1\42\color2\asdf".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_colors(), (42.0, 0.0));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_colors_updates_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some()
                && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer\color1\0\color2\3""#
                && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    user_info: r"\asdf\qwertz\color1\7.0\color2\5\name\UnnamedPlayer".to_string(),
                    player_info: PlayerInfo {
                        userinfo: r"\asdf\qwertz\color1\7.0\color2\5\name\UnnamedPlayer"
                            .to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.set_colors((0, 3));
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_model_when_no_model_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: "".to_string(),
                player_info: PlayerInfo {
                    userinfo: "".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_model();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_model_when_model_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\model\asdf".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\model\asdf".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_model();
        assert_eq!(result.expect("result was not OK"), "asdf");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_model_updates_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some()
                && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer\model\Uriel""#
                && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    user_info: r"\asdf\qwertz\model\Anarki\name\UnnamedPlayer".to_string(),
                    player_info: PlayerInfo {
                        userinfo: r"\asdf\qwertz\model\Anarki\name\UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.set_model("Uriel");
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_headmodel_when_no_headmodel_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: "".to_string(),
                player_info: PlayerInfo {
                    userinfo: "".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_headmodel();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_headmodel_when_headmodel_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\headmodel\asdf".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\headmodel\asdf".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_headmodel();
        assert_eq!(result.expect("result was not OK"), "asdf");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_headmodel_updates_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some()
                && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer\headmodel\Uriel""#
                && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    user_info: r"\asdf\qwertz\headmodel\Anarki\name\UnnamedPlayer".to_string(),
                    player_info: PlayerInfo {
                        userinfo: r"\asdf\qwertz\headmodel\Anarki\name\UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.set_headmodel("Uriel");
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_handicap_when_no_handicap_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: "".to_string(),
                player_info: PlayerInfo {
                    userinfo: "".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_handicap();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_handicap_when_handicap_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\handicap\42".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\handicap\42".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_handicap();
        assert_eq!(result.expect("result was not OK"), "42");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_handicap_updates_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some()
                && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer\handicap\50""#
                && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    user_info: r"\asdf\qwertz\handicap\100\name\UnnamedPlayer".to_string(),
                    player_info: PlayerInfo {
                        userinfo: r"\asdf\qwertz\handicap\100\name\UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.set_handicap(PyString::intern(py, "50").as_any());
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_handicap_for_unparseable_value(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\asdf\qwertz\handicap\100\name\UnnamedPlayer".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\asdf\qwertz\handicap\100\name\UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.set_handicap(PyString::intern(py, "asdf").as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_autohop_when_no_autohop_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: "".to_string(),
                player_info: PlayerInfo {
                    userinfo: "".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_autohop();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_autohop_when_autohop_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\autohop\1".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\autohop\1".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_autohop();
        assert_eq!(result.expect("result was not OK"), 1);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_autohop_when_autohop_is_disabled(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\autohop\0".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\autohop\0".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_autohop();
        assert_eq!(result.expect("result was not OK"), 0);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_autohop_when_autohop_cannot_be_parsed(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\autohop\asdf".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\autohop\asdf".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_autohop();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_autohop_updates_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some()
                && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer\autohop\0""#
                && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    user_info: r"\asdf\qwertz\autohop\1\name\UnnamedPlayer".to_string(),
                    player_info: PlayerInfo {
                        userinfo: r"\asdf\qwertz\autohop\1\name\UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.set_autohop(PyInt::new(py, 0i32).as_any());
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_autohop_for_unparseable_value(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\asdf\qwertz\autohop\1\name\UnnamedPlayer".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\asdf\qwertz\autohop\1\name\UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.set_autohop(PyString::intern(py, "asdf").as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_autoaction_when_no_autoaction_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: "".to_string(),
                player_info: PlayerInfo {
                    userinfo: "".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_autoaction();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_autoaction_when_autohop_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\autoaction\1".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\autoaction\1".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_autoaction();
        assert_eq!(result.expect("result was not OK"), 1);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_autoaction_when_autoaction_is_disabled(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\autoaction\0".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\autoaction\0".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_autoaction();
        assert_eq!(result.expect("result was not OK"), 0);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_autoaction_when_autoaction_cannot_be_parsed(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\autoaction\asdf".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\autoaction\asdf".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_autoaction();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_autoaction_updates_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some()
                && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer\autoaction\0""#
                && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    user_info: r"\asdf\qwertz\autoaction\1\name\UnnamedPlayer".to_string(),
                    player_info: PlayerInfo {
                        userinfo: r"\asdf\qwertz\autoaction\1\name\UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.set_autoaction(PyInt::new(py, 0i32).as_any());
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_autoaction_with_unparseable_value(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\asdf\qwertz\autoaction\1\name\UnnamedPlayer".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\asdf\qwertz\autoaction\1\name\UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.set_autoaction(PyString::intern(py, "asdf").as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_predictitems_when_no_predictitems_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: "".to_string(),
                player_info: PlayerInfo {
                    userinfo: "".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_predictitems();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
    })
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_predictitems_when_predictitems_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\cg_predictitems\1".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\cg_predictitems\1".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_predictitems();
        assert_eq!(result.expect("result was not OK"), 1);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_predititems_when_predictitems_is_disabled(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\cg_predictitems\0".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\cg_predictitems\0".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_predictitems();
        assert_eq!(result.expect("result was not OK"), 0);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_predititems_when_predictitems_is_unparseable(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\cg_predictitems\asdf".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\cg_predictitems\asdf".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_predictitems();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_predictitems_updates_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some()
                && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer\cg_predictitems\0""#
                && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    user_info: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer".to_string(),
                    player_info: PlayerInfo {
                        userinfo: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.set_predictitems(PyInt::new(py, 0i32).as_any());
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_predictitems_with_unparseable_value(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.set_predictitems(PyString::intern(py, "asdf").as_any());
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[case(clientState_t::CS_FREE, "free")]
#[case(clientState_t::CS_ZOMBIE, "zombie")]
#[case(clientState_t::CS_CONNECTED, "connected")]
#[case(clientState_t::CS_PRIMED, "primed")]
#[case(clientState_t::CS_ACTIVE, "active")]
#[cfg_attr(miri, ignore)]
fn get_connection_state_for_valid_values(
    _pyshinqlx_setup: (),
    #[case] client_state: clientState_t,
    #[case] expected_value: &str,
) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    connection_state: client_state as i32,
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_connection_state();
        assert_eq!(result.expect("result was not Ok"), expected_value);
    })
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_connection_state_for_invalid_value(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    connection_state: 42,
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_connection_state();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_state_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_state();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_state_for_client_without_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_state();
                    assert_eq!(result.expect("result was not OK"), None);
                });
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_state_transforms_from_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(123, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_get_position()
                .returning(|| (1.0, 2.0, 3.0));
            mock_game_client
                .expect_get_velocity()
                .returning(|| (4.0, 5.0, 6.0));
            mock_game_client.expect_is_alive().returning(|| true);
            mock_game_client.expect_get_armor().returning(|| 456);
            mock_game_client.expect_get_noclip().returning(|| true);
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_NAILGUN);
            mock_game_client
                .expect_get_weapons()
                .returning(|| [1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1]);
            mock_game_client
                .expect_get_ammos()
                .returning(|| [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
            mock_game_client
                .expect_get_powerups()
                .returning(|| [12, 34, 56, 78, 90, 24]);
            mock_game_client
                .expect_get_holdable()
                .returning(|| Holdable::Kamikaze.into());
            mock_game_client
                .expect_get_current_flight_fuel()
                .returning(|| 12);
            mock_game_client
                .expect_get_max_flight_fuel()
                .returning(|| 34);
            mock_game_client.expect_get_flight_thrust().returning(|| 56);
            mock_game_client.expect_get_flight_refuel().returning(|| 78);
            mock_game_client.expect_is_chatting().returning(|| true);
            mock_game_client.expect_is_frozen().returning(|| true);
            Ok(mock_game_client)
        })
        .run(predicate::eq(2), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_state();
                    assert_eq!(
                        result.expect("result was not OK"),
                        Some(PlayerState {
                            is_alive: true,
                            position: Vector3(1, 2, 3),
                            velocity: Vector3(4, 5, 6),
                            health: 123,
                            armor: 456,
                            noclip: true,
                            weapon: weapon_t::WP_NAILGUN.into(),
                            weapons: Weapons(1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1),
                            ammo: Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
                            powerups: Powerups(12, 34, 56, 78, 90, 24),
                            holdable: Holdable::Kamikaze,
                            flight: Flight(12, 34, 56, 78),
                            is_chatting: true,
                            is_frozen: true,
                        })
                    );
                });
            });
        });
}

#[rstest]
#[case(privileges_t::PRIV_MOD as i32, Some("mod"))]
#[case(privileges_t::PRIV_ADMIN as i32, Some("admin"))]
#[case(privileges_t::PRIV_ROOT as i32, Some("root"))]
#[case(privileges_t::PRIV_BANNED as i32, Some("banned"))]
#[case(privileges_t::PRIV_NONE as i32, None)]
#[case(42, None)]
#[cfg_attr(miri, ignore)]
fn get_privileges_various_values(
    _pyshinqlx_setup: (),
    #[case] privileges: i32,
    #[case] expected_value: Option<&str>,
) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                player_info: PlayerInfo {
                    privileges,
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_privileges();
        assert_eq!(result.as_deref(), expected_value);
    });
}

#[rstest]
#[case(None, & privileges_t::PRIV_NONE)]
#[case(Some("none"), & privileges_t::PRIV_NONE)]
#[case(Some("mod"), & privileges_t::PRIV_MOD)]
#[case(Some("admin"), & privileges_t::PRIV_ADMIN)]
#[serial]
#[cfg_attr(miri, ignore)]
fn set_privileges_for_valid_values(
    _pyshinqlx_setup: (),
    #[case] opt_priv: Option<&str>,
    #[case] privileges: &'static privileges_t,
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_privileges()
                .with(predicate::eq(*privileges as i32))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_privileges(opt_priv);
                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn set_privileges_for_invalid_string(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.set_privileges(Some("root"));
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_country_when_country_is_set(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                user_info: r"\country\de".to_string(),
                player_info: PlayerInfo {
                    userinfo: r"\country\de".to_string(),
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.get_country();
        assert_eq!(result.expect("result was not OK"), "de");
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_country_updates_client_cvars(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(move |_client_id| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .return_const(clientState_t::CS_CONNECTED);
        mock_client
    });

    let hook_ctx = shinqlx_execute_client_command_context();
    hook_ctx
        .expect()
        .withf(|client, cmd, &client_ok| {
            client.is_some()
                && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer\country\uk""#
                && client_ok
        })
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    user_info: r"\asdf\qwertz\country\de\name\UnnamedPlayer".to_string(),
                    player_info: PlayerInfo {
                        userinfo: r"\asdf\qwertz\country\de\name\UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.set_country("uk");
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_valid_for_valid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                valid: true.into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_valid(), true);
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_valid_for_invalid_player(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                valid: false.into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        assert_eq!(player.get_valid(), false);
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_stats_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_stats();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_stats_for_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_score().returning(|| 42);
            mock_game_client.expect_get_kills().returning(|| 7);
            mock_game_client.expect_get_deaths().returning(|| 9);
            mock_game_client
                .expect_get_damage_dealt()
                .returning(|| 5000);
            mock_game_client
                .expect_get_damage_taken()
                .returning(|| 4200);
            mock_game_client.expect_get_time_on_team().returning(|| 123);
            mock_game_client.expect_get_ping().returning(|| 9);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_stats();

                    assert_eq!(
                        result
                            .expect("result was not OK")
                            .expect("result was not Some"),
                        PlayerStats {
                            score: 42,
                            kills: 7,
                            deaths: 9,
                            damage_dealt: 5000,
                            damage_taken: 4200,
                            time: 123,
                            ping: 9,
                        }
                    );
                });
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_stats_for_game_entiy_with_no_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_stats();

                    assert_eq!(result.expect("result was not OK"), None);
                });
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_ping_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_ping();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_ping_for_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_score().returning(|| 42);
            mock_game_client.expect_get_kills().returning(|| 7);
            mock_game_client.expect_get_deaths().returning(|| 9);
            mock_game_client
                .expect_get_damage_dealt()
                .returning(|| 5000);
            mock_game_client
                .expect_get_damage_taken()
                .returning(|| 4200);
            mock_game_client.expect_get_time_on_team().returning(|| 123);
            mock_game_client.expect_get_ping().returning(|| 42);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_ping();

                    assert_eq!(result.expect("result was not OK"), 42);
                });
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_ping_for_game_entiy_with_no_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_ping();

                    assert_eq!(result.expect("result was not OK"), 999);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn position_gathers_players_position_with_no_kwargs(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_get_position()
                .returning(|| (1.0, 2.0, 3.0));
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.position(false, None);
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<Vector3>()
                            .expect("result was not a Vector3")
                            == Vector3(1, 2, 3)
                    }));
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn position_sets_players_position_when_provided(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_get_position()
                    .returning(|| (1.0, 2.0, 3.0));
                mock_game_client.expect_get_velocity();
                mock_game_client.expect_is_alive();
                mock_game_client.expect_get_armor();
                mock_game_client.expect_get_noclip();
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
                mock_game_client.expect_get_weapons();
                mock_game_client.expect_get_ammos();
                mock_game_client.expect_get_powerups();
                mock_game_client.expect_get_holdable();
                mock_game_client.expect_get_current_flight_fuel();
                mock_game_client.expect_get_max_flight_fuel();
                mock_game_client.expect_get_flight_thrust();
                mock_game_client.expect_get_flight_refuel();
                mock_game_client.expect_is_chatting();
                mock_game_client.expect_is_frozen();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_position()
                    .with(predicate::eq((4.0, 5.0, 6.0)))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.position(
                false,
                Some(
                    &[("x", 4), ("y", 5), ("z", 6)]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                true
            );
        });
    });
}

#[rstest]
#[case([("x", 42)], (42.0, 0.0, 0.0))]
#[case([("y", 42)], (0.0, 42.0, 0.0))]
#[case([("z", 42)], (0.0, 0.0, 42.0))]
#[cfg_attr(miri, ignore)]
#[serial]
fn position_resets_players_position_with_single_value(
    _pyshinqlx_setup: (),
    #[case] position: [(&str, i32); 1],
    #[case] expected_position: (f32, f32, f32),
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_position()
                .with(predicate::eq(expected_position))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.position(
                        true,
                        Some(&position.into_py_dict(py).expect("this should not happen")),
                    );
                    assert_eq!(
                        result
                            .expect("result was not Ok")
                            .extract::<bool>()
                            .expect("result was not a bool value"),
                        true
                    );
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn position_sets_players_position_with_no_game_client(_pyshinqlx_setup: ()) {
    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx.expect().returning(|_| {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity
            .expect_get_game_client()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        mock_game_entity
    });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.position(
                false,
                Some(
                    &[("x", 4), ("y", 5), ("z", 6)]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                false
            );
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn velocity_gathers_players_velocity_with_no_kwargs(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client
                .expect_get_velocity()
                .returning(|| (1.0, 2.0, 3.0));
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.velocity(false, None);
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<Vector3>()
                            .expect("result was not a Vector3")
                            == Vector3(1, 2, 3)
                    }));
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn velocity_sets_players_velocity_when_provided(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_position();
                mock_game_client
                    .expect_get_velocity()
                    .returning(|| (1.0, 2.0, 3.0));
                mock_game_client.expect_is_alive();
                mock_game_client.expect_get_armor();
                mock_game_client.expect_get_noclip();
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
                mock_game_client.expect_get_weapons();
                mock_game_client.expect_get_ammos();
                mock_game_client.expect_get_powerups();
                mock_game_client.expect_get_holdable();
                mock_game_client.expect_get_current_flight_fuel();
                mock_game_client.expect_get_max_flight_fuel();
                mock_game_client.expect_get_flight_thrust();
                mock_game_client.expect_get_flight_refuel();
                mock_game_client.expect_is_chatting();
                mock_game_client.expect_is_frozen();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_velocity()
                    .with(predicate::eq((4.0, 5.0, 6.0)))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.velocity(
                false,
                Some(
                    &[("x", 4), ("y", 5), ("z", 6)]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                true
            );
        });
    });
}

#[rstest]
#[case([("x", 42)], (42.0, 0.0, 0.0))]
#[case([("y", 42)], (0.0, 42.0, 0.0))]
#[case([("z", 42)], (0.0, 0.0, 42.0))]
#[cfg_attr(miri, ignore)]
#[serial]
fn velocity_resets_players_veloity_with_single_value(
    _pyshinqlx_setup: (),
    #[case] velocity: [(&str, i32); 1],
    #[case] expected_velocity: (f32, f32, f32),
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_velocity()
                .with(predicate::eq(expected_velocity))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.velocity(
                        true,
                        Some(&velocity.into_py_dict(py).expect("this should not happen")),
                    );
                    assert_eq!(
                        result
                            .expect("result was not Ok")
                            .extract::<bool>()
                            .expect("result was not a bool value"),
                        true
                    );
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn velocity_sets_players_velocity_with_no_game_client(_pyshinqlx_setup: ()) {
    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx.expect().returning(|_| {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity
            .expect_get_game_client()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        mock_game_entity
    });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.velocity(
                false,
                Some(
                    &[("x", 4), ("y", 5), ("z", 6)]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                false
            );
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn weapons_gathers_players_weapons_with_no_kwargs(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client
                .expect_get_weapons()
                .returning(|| [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1]);
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.weapons(false, None);
                    assert!(result.is_ok_and(|value| {
                        value.extract::<Weapons>().expect("result was not Weapons")
                            == Weapons(1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1)
                    }));
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn weapons_sets_players_weapons_when_provided(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_position();
                mock_game_client.expect_get_velocity();
                mock_game_client.expect_is_alive();
                mock_game_client.expect_get_armor();
                mock_game_client.expect_get_noclip();
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
                mock_game_client
                    .expect_get_weapons()
                    .returning(|| [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1]);
                mock_game_client.expect_get_ammos();
                mock_game_client.expect_get_powerups();
                mock_game_client.expect_get_holdable();
                mock_game_client.expect_get_current_flight_fuel();
                mock_game_client.expect_get_max_flight_fuel();
                mock_game_client.expect_get_flight_thrust();
                mock_game_client.expect_get_flight_refuel();
                mock_game_client.expect_is_chatting();
                mock_game_client.expect_is_frozen();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_weapons()
                    .with(predicate::eq([1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.weapons(
                false,
                Some(
                    &[
                        ("g", true),
                        ("mg", false),
                        ("sg", true),
                        ("gl", false),
                        ("rl", true),
                        ("lg", false),
                        ("rg", true),
                        ("pg", false),
                        ("bfg", true),
                        ("gh", false),
                        ("ng", true),
                        ("pl", false),
                        ("cg", true),
                        ("hmg", false),
                        ("hands", true),
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                true
            );
        });
    });
}

#[rstest]
#[case([("g", 1)], [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("mg", 1)], [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("sg", 1)], [0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("gl", 1)], [0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("rl", 1)], [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("lg", 1)], [0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("rg", 1)], [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("pg", 1)], [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0])]
#[case([("bfg", 1)], [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0])]
#[case([("gh", 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0])]
#[case([("ng", 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0])]
#[case([("pl", 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0])]
#[case([("cg", 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0])]
#[case([("hmg", 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0])]
#[case([("hands", 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])]
#[cfg_attr(miri, ignore)]
#[serial]
fn weapons_resets_players_weapons_with_single_value(
    _pyshinqlx_setup: (),
    #[case] weapons: [(&str, i32); 1],
    #[case] expected_weapons: [i32; 15],
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_weapons()
                .with(predicate::eq(expected_weapons))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.weapons(
                        true,
                        Some(&weapons.into_py_dict(py).expect("this should not happen")),
                    );
                    assert_eq!(
                        result
                            .expect("result was not Ok")
                            .extract::<bool>()
                            .expect("result was not a bool value"),
                        true
                    );
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn weapons_sets_players_weapons_with_no_game_client(_pyshinqlx_setup: ()) {
    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx.expect().returning(|_| {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity
            .expect_get_game_client()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        mock_game_entity
    });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.weapons(
                false,
                Some(
                    &[
                        ("g", true),
                        ("mg", false),
                        ("sg", true),
                        ("gl", false),
                        ("rl", true),
                        ("lg", false),
                        ("rg", true),
                        ("pg", false),
                        ("bfg", true),
                        ("gh", false),
                        ("ng", true),
                        ("pl", false),
                        ("cg", true),
                        ("hmg", false),
                        ("hands", true),
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                false
            );
        });
    });
}

#[rstest]
#[case(weapon_t::WP_GAUNTLET)]
#[case(weapon_t::WP_MACHINEGUN)]
#[case(weapon_t::WP_SHOTGUN)]
#[case(weapon_t::WP_GRENADE_LAUNCHER)]
#[case(weapon_t::WP_ROCKET_LAUNCHER)]
#[case(weapon_t::WP_LIGHTNING)]
#[case(weapon_t::WP_RAILGUN)]
#[case(weapon_t::WP_PLASMAGUN)]
#[case(weapon_t::WP_BFG)]
#[case(weapon_t::WP_GRAPPLING_HOOK)]
#[case(weapon_t::WP_NAILGUN)]
#[case(weapon_t::WP_PROX_LAUNCHER)]
#[case(weapon_t::WP_CHAINGUN)]
#[case(weapon_t::WP_HMG)]
#[case(weapon_t::WP_HANDS)]
#[cfg_attr(miri, ignore)]
#[serial]
fn weapon_gets_currently_held_weapon(_pyshinqlx_setup: (), #[case] weapon: weapon_t) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(move || weapon);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.weapon(None);
                    assert_eq!(
                        result
                            .expect("result was not Ok")
                            .extract::<i32>()
                            .expect("result was not an integer"),
                        weapon as i32
                    )
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn weapon_gets_currently_held_weapon_with_no_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.weapon(None);
                    assert_eq!(
                        result
                            .expect("result was not Ok")
                            .extract::<i32>()
                            .expect("result was not an integer"),
                        weapon_t::WP_HANDS as i32
                    )
                });
            });
        });
}

#[rstest]
#[case("g", weapon_t::WP_GAUNTLET)]
#[case("mg", weapon_t::WP_MACHINEGUN)]
#[case("sg", weapon_t::WP_SHOTGUN)]
#[case("gl", weapon_t::WP_GRENADE_LAUNCHER)]
#[case("rl", weapon_t::WP_ROCKET_LAUNCHER)]
#[case("lg", weapon_t::WP_LIGHTNING)]
#[case("rg", weapon_t::WP_RAILGUN)]
#[case("pg", weapon_t::WP_PLASMAGUN)]
#[case("bfg", weapon_t::WP_BFG)]
#[case("gh", weapon_t::WP_GRAPPLING_HOOK)]
#[case("ng", weapon_t::WP_NAILGUN)]
#[case("pl", weapon_t::WP_PROX_LAUNCHER)]
#[case("cg", weapon_t::WP_CHAINGUN)]
#[case("hmg", weapon_t::WP_HMG)]
#[case("hands", weapon_t::WP_HANDS)]
#[cfg_attr(miri, ignore)]
#[serial]
fn weapon_sets_players_weapon_from_str(
    _pyshinqlx_setup: (),
    #[case] weapon_str: &str,
    #[case] expected_weapon: weapon_t,
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_weapon()
                .with(predicate::eq(expected_weapon as i32))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.weapon(Some(PyString::intern(py, weapon_str).into_any()));
                    assert_eq!(
                        result
                            .expect("result was not Ok")
                            .extract::<bool>()
                            .expect("result was not a bool value"),
                        true
                    );
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn weapon_sets_players_weapon_from_invalid_str(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.weapon(Some(PyString::intern(py, "invalid weapon").into_any()));
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[case(1, weapon_t::WP_GAUNTLET)]
#[case(2, weapon_t::WP_MACHINEGUN)]
#[case(3, weapon_t::WP_SHOTGUN)]
#[case(4, weapon_t::WP_GRENADE_LAUNCHER)]
#[case(5, weapon_t::WP_ROCKET_LAUNCHER)]
#[case(6, weapon_t::WP_LIGHTNING)]
#[case(7, weapon_t::WP_RAILGUN)]
#[case(8, weapon_t::WP_PLASMAGUN)]
#[case(9, weapon_t::WP_BFG)]
#[case(10, weapon_t::WP_GRAPPLING_HOOK)]
#[case(11, weapon_t::WP_NAILGUN)]
#[case(12, weapon_t::WP_PROX_LAUNCHER)]
#[case(13, weapon_t::WP_CHAINGUN)]
#[case(14, weapon_t::WP_HMG)]
#[case(15, weapon_t::WP_HANDS)]
#[cfg_attr(miri, ignore)]
#[serial]
fn weapon_sets_players_weapon_from_int(
    _pyshinqlx_setup: (),
    #[case] weapon_index: i32,
    #[case] expected_weapon: weapon_t,
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_weapon()
                .with(predicate::eq(expected_weapon as i32))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.weapon(Some(PyInt::new(py, weapon_index).into_any()));
                    assert_eq!(
                        result
                            .expect("result was not Ok")
                            .extract::<bool>()
                            .expect("result was not a bool value"),
                        true
                    );
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn weapon_sets_players_weapon_from_invalid_int(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.weapon(Some(PyInt::new(py, 42i32).into_any()));
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn ammo_gathers_players_ammo_with_no_kwargs(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client
                .expect_get_ammos()
                .returning(|| [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.ammo(false, None);
                    assert!(result.is_ok_and(|value| {
                        value.extract::<Weapons>().expect("result was not Weapons")
                            == Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15)
                    }));
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn ammo_sets_players_ammo_when_provided(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_position();
                mock_game_client.expect_get_velocity();
                mock_game_client.expect_is_alive();
                mock_game_client.expect_get_armor();
                mock_game_client.expect_get_noclip();
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
                mock_game_client.expect_get_weapons();
                mock_game_client.expect_get_ammos();
                mock_game_client.expect_get_powerups();
                mock_game_client.expect_get_holdable();
                mock_game_client.expect_get_current_flight_fuel();
                mock_game_client.expect_get_max_flight_fuel();
                mock_game_client.expect_get_flight_thrust();
                mock_game_client.expect_get_flight_refuel();
                mock_game_client.expect_is_chatting();
                mock_game_client.expect_is_frozen();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_ammos()
                    .with(predicate::eq([
                        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
                    ]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.ammo(
                false,
                Some(
                    &[
                        ("g", 1),
                        ("mg", 2),
                        ("sg", 3),
                        ("gl", 4),
                        ("rl", 5),
                        ("lg", 6),
                        ("rg", 7),
                        ("pg", 8),
                        ("bfg", 9),
                        ("gh", 10),
                        ("ng", 11),
                        ("pl", 12),
                        ("cg", 13),
                        ("hmg", 14),
                        ("hands", 15),
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                true
            );
        });
    });
}

#[rstest]
#[case([("g", 42)], [42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("mg", 42)], [0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("sg", 42)], [0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("gl", 42)], [0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("rl", 42)], [0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("lg", 42)], [0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("rg", 42)], [0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0])]
#[case([("pg", 42)], [0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0])]
#[case([("bfg", 42)], [0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0])]
#[case([("gh", 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0])]
#[case([("ng", 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0])]
#[case([("pl", 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0])]
#[case([("cg", 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0])]
#[case([("hmg", 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0])]
#[case([("hands", 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42])]
#[cfg_attr(miri, ignore)]
#[serial]
fn ammo_resets_players_ammos_with_single_value(
    _pyshinqlx_setup: (),
    #[case] ammos: [(&str, i32); 1],
    #[case] expected_ammos: [i32; 15],
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_ammos()
                .with(predicate::eq(expected_ammos))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.ammo(
                        true,
                        Some(&ammos.into_py_dict(py).expect("this should not happen")),
                    );
                    assert_eq!(
                        result
                            .expect("result was not Ok")
                            .extract::<bool>()
                            .expect("result was not a bool value"),
                        true
                    );
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn ammo_sets_players_ammo_with_no_game_client(_pyshinqlx_setup: ()) {
    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx.expect().returning(|_| {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity
            .expect_get_game_client()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        mock_game_entity
    });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.ammo(
                false,
                Some(
                    &[
                        ("g", 1),
                        ("mg", 2),
                        ("sg", 3),
                        ("gl", 4),
                        ("rl", 5),
                        ("lg", 6),
                        ("rg", 7),
                        ("pg", 8),
                        ("bfg", 9),
                        ("gh", 10),
                        ("ng", 11),
                        ("pl", 12),
                        ("cg", 13),
                        ("hmg", 14),
                        ("hands", 15),
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                false
            );
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn powerups_gathers_players_powerups_with_no_kwargs(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client
                .expect_get_powerups()
                .returning(|| [1000, 2000, 3000, 4000, 5000, 6000]);
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.powerups(false, None);
                    assert!(result.is_ok_and(|value| {
                        value
                            .extract::<Powerups>()
                            .expect("result was not a Powerups")
                            == Powerups(1000, 2000, 3000, 4000, 5000, 6000)
                    }));
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn powerups_sets_players_powerups_when_provided(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_position();
                mock_game_client.expect_get_velocity();
                mock_game_client.expect_is_alive();
                mock_game_client.expect_get_armor();
                mock_game_client.expect_get_noclip();
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
                mock_game_client.expect_get_weapons();
                mock_game_client.expect_get_ammos();
                mock_game_client
                    .expect_get_powerups()
                    .returning(|| [1000, 2000, 3000, 4000, 5000, 6000]);
                mock_game_client.expect_get_holdable();
                mock_game_client.expect_get_current_flight_fuel();
                mock_game_client.expect_get_max_flight_fuel();
                mock_game_client.expect_get_flight_thrust();
                mock_game_client.expect_get_flight_refuel();
                mock_game_client.expect_is_chatting();
                mock_game_client.expect_is_frozen();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_powerups()
                    .with(predicate::eq([6500, 5000, 4250, 3000, 2125, 1000]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.powerups(
                false,
                Some(
                    &[
                        ("quad", 6.5),
                        ("battlesuit", 5.0),
                        ("haste", 4.25),
                        ("invisibility", 3.0),
                        ("regeneration", 2.125),
                        ("invulnerability", 1.0),
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                true
            );
        });
    });
}

#[rstest]
#[case([("quad", 42)], [42000, 0, 0, 0, 0, 0])]
#[case([("battlesuit", 42)], [0, 42000, 0, 0, 0, 0])]
#[case([("haste", 42)], [0, 0, 42000, 0, 0, 0])]
#[case([("invisibility", 42)], [0, 0, 0, 42000, 0, 0])]
#[case([("regeneration", 42)], [0, 0, 0, 0, 42000, 0])]
#[case([("invulnerability", 42)], [0, 0, 0, 0, 0, 42000])]
#[cfg_attr(miri, ignore)]
#[serial]
fn powerups_resets_players_powerups_with_single_value(
    _pyshinqlx_setup: (),
    #[case] powerups: [(&str, i32); 1],
    #[case] expected_powerups: [i32; 6],
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_powerups()
                .with(predicate::eq(expected_powerups))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.powerups(
                        true,
                        Some(&powerups.into_py_dict(py).expect("this should not happen")),
                    );
                    assert_eq!(
                        result
                            .expect("result was not Ok")
                            .extract::<bool>()
                            .expect("result was not a bool value"),
                        true
                    );
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn powerups_sets_players_powerups_with_no_game_client(_pyshinqlx_setup: ()) {
    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx.expect().returning(|_| {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity
            .expect_get_game_client()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        mock_game_entity
    });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.powerups(
                false,
                Some(
                    &[
                        ("quad", 6),
                        ("battlesuit", 5),
                        ("haste", 4),
                        ("invisibility", 3),
                        ("regeneration", 2),
                        ("invulnerability", 1),
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                false
            );
        });
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_holdable_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_holdable();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[case(Holdable::None, None)]
#[case(Holdable::Teleporter, Some("teleporter"))]
#[case(Holdable::MedKit, Some("medkit"))]
#[case(Holdable::Flight, Some("flight"))]
#[case(Holdable::Kamikaze, Some("kamikaze"))]
#[case(Holdable::Portal, Some("portal"))]
#[case(Holdable::Invulnerability, Some("invulnerability"))]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_holdable_with_various_values(
    _pyshinqlx_setup: (),
    #[case] holdable: Holdable,
    #[case] expected_result: Option<&str>,
) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client
                .expect_get_holdable()
                .returning(move || holdable.into());
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_holdable();
                    assert_eq!(
                        result.expect("result was not Ok").as_deref(),
                        expected_result
                    );
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_holdable_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.set_holdable(Some("kamikaze"));
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)))
    });
}

#[rstest]
#[case("unknown")]
#[case("asdf")]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_holdable_for_unknown_values(_pyshinqlx_setup: (), #[case] invalid_str: &str) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.set_holdable(Some(invalid_str));
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)))
    });
}

#[rstest]
#[case(None, Holdable::None)]
#[case(Some("none"), Holdable::None)]
#[case(Some("teleporter"), Holdable::Teleporter)]
#[case(Some("medkit"), Holdable::MedKit)]
#[case(Some("kamikaze"), Holdable::Kamikaze)]
#[case(Some("portal"), Holdable::Portal)]
#[case(Some("invulnerability"), Holdable::Invulnerability)]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_holdable_for_various_values(
    _pyshinqlx_setup: (),
    #[case] new_holdable: Option<&str>,
    #[case] expected_holdable: Holdable,
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_holdable()
                .with(predicate::eq(expected_holdable))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_holdable(new_holdable);
                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_holdable_for_flight(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_holdable()
                    .with(predicate::eq(Holdable::Flight))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_flight::<[i32; 4]>()
                    .with(predicate::eq([16_000, 16_000, 1_200, 0]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.set_holdable(Some("flight"));
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn drop_holdable_when_player_holds_one(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_remove_kamikaze_flag().times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_get_holdable()
                    .returning(|| Holdable::Kamikaze as i32);
                Ok(mock_game_client)
            });
            mock_game_entity.expect_drop_holdable().times(1);
            mock_game_entity
        });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.drop_holdable();
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn flight_gathers_players_flight_parameters_with_no_kwargs(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client
                .expect_get_holdable()
                .returning(|| Holdable::Flight as i32);
            mock_game_client
                .expect_get_current_flight_fuel()
                .returning(|| 1);
            mock_game_client
                .expect_get_max_flight_fuel()
                .returning(|| 2);
            mock_game_client.expect_get_flight_thrust().returning(|| 3);
            mock_game_client.expect_get_flight_refuel().returning(|| 4);
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.flight(false, None);
                    assert!(result.is_ok_and(|value| {
                        value.extract::<Flight>().expect("result was not a Flight")
                            == Flight(1, 2, 3, 4)
                    }));
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn flight_sets_players_flight_when_provided(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_position();
                mock_game_client.expect_get_velocity();
                mock_game_client.expect_is_alive();
                mock_game_client.expect_get_armor();
                mock_game_client.expect_get_noclip();
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
                mock_game_client.expect_get_weapons();
                mock_game_client.expect_get_ammos();
                mock_game_client.expect_get_powerups();
                mock_game_client
                    .expect_get_holdable()
                    .returning(|| Holdable::Flight as i32);
                mock_game_client.expect_get_current_flight_fuel();
                mock_game_client.expect_get_max_flight_fuel();
                mock_game_client.expect_get_flight_thrust();
                mock_game_client.expect_get_flight_refuel();
                mock_game_client.expect_is_chatting();
                mock_game_client.expect_is_frozen();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_flight::<[i32; 4]>()
                    .with(predicate::eq([5, 6, 7, 8]))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.flight(
                false,
                Some(
                    &[("fuel", 5), ("max_fuel", 6), ("thrust", 7), ("refuel", 8)]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                true
            );
        });
    });
}

#[rstest]
#[case([("fuel", 42)], Flight(42, 16_000, 1_200, 0))]
#[case([("max_fuel", 42)], Flight(16_000, 42, 1_200, 0))]
#[case([("thrust", 42)], Flight(16_000, 16_000, 42, 0))]
#[case([("refuel", 42)], Flight(16_000, 16_000, 1_200, 42))]
#[cfg_attr(miri, ignore)]
#[serial]
fn flight_resets_players_flight_with_single_value(
    _pyshinqlx_setup: (),
    #[case] flight_opts: [(&str, i32); 1],
    #[case] expected_flight: Flight,
) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_position();
                mock_game_client.expect_get_velocity();
                mock_game_client.expect_is_alive();
                mock_game_client.expect_get_armor();
                mock_game_client.expect_get_noclip();
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
                mock_game_client.expect_get_weapons();
                mock_game_client.expect_get_ammos();
                mock_game_client.expect_get_powerups();
                mock_game_client
                    .expect_get_holdable()
                    .returning(|| Holdable::Flight as i32);
                mock_game_client.expect_get_current_flight_fuel();
                mock_game_client.expect_get_max_flight_fuel();
                mock_game_client.expect_get_flight_thrust();
                mock_game_client.expect_get_flight_refuel();
                mock_game_client.expect_is_chatting();
                mock_game_client.expect_is_frozen();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_flight::<[i32; 4]>()
                        .with(predicate::eq([
                            expected_flight.0,
                            expected_flight.1,
                            expected_flight.2,
                            expected_flight.3,
                        ]))
                        .times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity
        });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.flight(
                true,
                Some(
                    &flight_opts
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                true
            );
        });
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn flight_sets_players_flight_with_no_game_client(_pyshinqlx_setup: ()) {
    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx.expect().returning(|_| {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity
            .expect_get_game_client()
            .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
        mock_game_entity
    });

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.flight(
                false,
                Some(
                    &[("fuel", 5), ("max_fuel", 6), ("refuel", 8), ("thrust", 7)]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            );
            assert_eq!(
                result
                    .expect("result was not Ok")
                    .extract::<bool>()
                    .expect("result was not a bool value"),
                false
            );
        });
    });
}

#[rstest]
#[case(true)]
#[case(false)]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_noclip_returns_players_noclip_state(_pyshinqlx_setup: (), #[case] noclip_state: bool) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client
                .expect_get_noclip()
                .returning(move || noclip_state);
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_noclip();
                    assert_eq!(result.expect("result was not Ok"), noclip_state.to_owned());
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_noclip_for_player_without_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_noclip();
                    assert_eq!(result.expect("result was not Ok"), false);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_noclip_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_noclip();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[case(true)]
#[case(false)]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_noclip_set_players_noclip_value_by_bool(_pyshinqlx_setup: (), #[case] noclip_value: bool) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client
                .expect_get_noclip()
                .returning(move || !noclip_value);
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            mock_game_client
                .expect_set_noclip()
                .with(predicate::eq(noclip_value))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result =
                        player.set_noclip(PyBool::new(py, noclip_value).to_owned().as_any());

                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[case(42, true)]
#[case(0, false)]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_noclip_set_players_noclip_value_by_integer(
    _pyshinqlx_setup: (),
    #[case] noclip_value: i32,
    #[case] expected_noclip: bool,
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client
                .expect_get_noclip()
                .returning(move || !expected_noclip);
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            mock_game_client
                .expect_set_noclip()
                .with(predicate::eq(expected_noclip))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_noclip(PyInt::new(py, noclip_value).as_any());

                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[case("asdf", true)]
#[case("", false)]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_noclip_set_players_noclip_value_by_string(
    _pyshinqlx_setup: (),
    #[case] noclip_value: &'static str,
    #[case] expected_noclip: bool,
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client
                .expect_get_noclip()
                .returning(move || !expected_noclip);
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            mock_game_client
                .expect_set_noclip()
                .with(predicate::eq(expected_noclip))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_noclip(PyString::intern(py, noclip_value).as_any());

                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_noclip_set_players_noclip_value_by_none(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip().returning(|| true);
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            mock_game_client
                .expect_set_noclip()
                .with(predicate::eq(false))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_noclip(py.None().bind(py));

                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_health_returns_players_health_state(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(42, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_health();
                    assert_eq!(result.expect("result was not Ok"), 42);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_health_for_player_without_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_health();
                    assert_eq!(result.expect("result was not Ok"), 0);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_health_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_health();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_health_set_players_health(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_set_health(predicate::eq(666), 1)
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_health(666);

                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_armor_returns_players_armor_state(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor().returning(|| 42);
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_armor();
                    assert_eq!(result.expect("result was not Ok"), 42);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_armor_for_player_without_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_health();
                    assert_eq!(result.expect("result was not Ok"), 0);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_armor_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_armor();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_armor_set_players_armor(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            mock_game_client
                .expect_set_armor()
                .with(predicate::eq(666))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_armor(666);

                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[case(true)]
#[case(false)]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_is_alive_returns_players_is_alive_state(_pyshinqlx_setup: (), #[case] is_alive: bool) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client
                .expect_is_alive()
                .returning(move || is_alive);
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_is_alive();
                    assert_eq!(result.expect("result was not Ok"), is_alive);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_is_alive_for_player_without_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_is_alive();
                    assert_eq!(result.expect("result was not Ok"), false);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_is_alive_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_is_alive();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_is_alive_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_is_alive();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_is_alive_for_alive_player_with_false(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_position();
                mock_game_client.expect_get_velocity();
                mock_game_client.expect_is_alive().returning(|| true);
                mock_game_client.expect_get_armor();
                mock_game_client.expect_get_noclip();
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
                mock_game_client.expect_get_weapons();
                mock_game_client.expect_get_ammos();
                mock_game_client.expect_get_powerups();
                mock_game_client.expect_get_holdable();
                mock_game_client.expect_get_current_flight_fuel();
                mock_game_client.expect_get_max_flight_fuel();
                mock_game_client.expect_get_flight_thrust();
                mock_game_client.expect_get_flight_refuel();
                mock_game_client.expect_is_chatting();
                mock_game_client.expect_is_frozen();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_set_health()
                .with(predicate::eq(0))
                .times(1);
            mock_game_entity
        });
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_client_number()
                .return_const(42)
                .times(1);
            mock_game_entity
        });

    MockEngineBuilder::default()
        .with_max_clients(16)
        .configure(|mock_engine| {
            mock_engine
                .expect_game_add_event()
                .withf(|_entity, &entity_event, &event_param| {
                    entity_event == entity_event_t::EV_DEATH1 && event_param == 42
                })
                .times(1);
        })
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.set_is_alive(false);
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_is_alive_for_dead_player_with_false(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive().returning(|| false);
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_is_alive(false);
                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_is_alive_for_alive_player_with_true(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive().returning(|| true);
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_is_alive(true);
                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn set_is_alive_for_dead_player_with_true(_pyshinqlx_setup: ()) {
    let mut seq = Sequence::new();

    let game_entity_from_ctx = MockGameEntity::from_context();
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client.expect_get_position();
                mock_game_client.expect_get_velocity();
                mock_game_client.expect_is_alive().returning(|| false);
                mock_game_client.expect_get_armor();
                mock_game_client.expect_get_noclip();
                mock_game_client
                    .expect_get_weapon()
                    .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
                mock_game_client.expect_get_weapons();
                mock_game_client.expect_get_ammos();
                mock_game_client.expect_get_powerups();
                mock_game_client.expect_get_holdable();
                mock_game_client.expect_get_current_flight_fuel();
                mock_game_client.expect_get_max_flight_fuel();
                mock_game_client.expect_get_flight_thrust();
                mock_game_client.expect_get_flight_refuel();
                mock_game_client.expect_is_chatting();
                mock_game_client.expect_is_frozen();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });
    game_entity_from_ctx
        .expect()
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            let mut seq2 = Sequence::new();
            mock_game_entity
                .expect_get_game_client()
                .times(1)
                .in_sequence(&mut seq2)
                .returning(|| {
                    let mock_game_client = MockGameClient::new();
                    Ok(mock_game_client)
                });
            mock_game_entity
                .expect_get_game_client()
                .times(1)
                .in_sequence(&mut seq2)
                .returning(|| {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client.expect_spawn().times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

    let shinqlx_client_spawn_ctx = shinqlx_client_spawn_context();
    shinqlx_client_spawn_ctx.expect().times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.set_is_alive(true);
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[case(true)]
#[case(false)]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_is_frozen_returns_players_is_frozen_state(_pyshinqlx_setup: (), #[case] is_frozen: bool) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client.expect_is_chatting();
            mock_game_client
                .expect_is_frozen()
                .returning(move || is_frozen);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_is_frozen();
                    assert_eq!(result.expect("result was not Ok"), is_frozen);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_is_frozen_for_player_without_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_is_frozen();
                    assert_eq!(result.expect("result was not Ok"), false);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_is_frozen_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_is_frozen();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[case(true)]
#[case(false)]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_is_chatting_returns_players_is_chatting_state(
    _pyshinqlx_setup: (),
    #[case] is_chatting: bool,
) {
    MockGameEntityBuilder::default()
        .with_game_client(move || {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_position();
            mock_game_client.expect_get_velocity();
            mock_game_client.expect_is_alive();
            mock_game_client.expect_get_armor();
            mock_game_client.expect_get_noclip();
            mock_game_client
                .expect_get_weapon()
                .returning(|| weapon_t::WP_ROCKET_LAUNCHER);
            mock_game_client.expect_get_weapons();
            mock_game_client.expect_get_ammos();
            mock_game_client.expect_get_powerups();
            mock_game_client.expect_get_holdable();
            mock_game_client.expect_get_current_flight_fuel();
            mock_game_client.expect_get_max_flight_fuel();
            mock_game_client.expect_get_flight_thrust();
            mock_game_client.expect_get_flight_refuel();
            mock_game_client
                .expect_is_chatting()
                .returning(move || is_chatting);
            mock_game_client.expect_is_frozen();
            Ok(mock_game_client)
        })
        .with_health(200, 0..)
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_is_chatting();
                    assert_eq!(result.expect("result was not Ok"), is_chatting);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_is_chatting_for_player_without_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_is_chatting();
                    assert_eq!(result.expect("result was not Ok"), false);
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
#[serial]
fn get_is_chatting_with_no_main_engine(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_is_chatting();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_score_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_score();
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_score_for_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client.expect_get_score().returning(|| 42);
            mock_game_client.expect_get_kills().returning(|| 7);
            mock_game_client.expect_get_deaths().returning(|| 9);
            mock_game_client
                .expect_get_damage_dealt()
                .returning(|| 5000);
            mock_game_client
                .expect_get_damage_taken()
                .returning(|| 4200);
            mock_game_client.expect_get_time_on_team().returning(|| 123);
            mock_game_client.expect_get_ping().returning(|| 9);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_score();
                    assert_eq!(result.expect("result was not OK"), 42);
                });
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn get_score_for_game_entiy_with_no_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.get_score();
                    assert_eq!(result.expect("result was not OK"), 0);
                });
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn set_score_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.set_score(42);
        assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn set_score_for_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| {
            let mut mock_game_client = MockGameClient::new();
            mock_game_client
                .expect_set_score()
                .with(predicate::eq(42))
                .times(1);
            Ok(mock_game_client)
        })
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_score(42);
                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn set_score_for_game_entiy_with_no_game_client(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_game_client(|| Err(QuakeLiveEngineError::MainEngineNotInitialized))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.set_score(42);
                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[cfg_attr(miri, ignore)]
fn get_channel_returns_tell_channel(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.get_channel();
        assert!(result.is_some());
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn center_print_sends_center_print_server_command(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(|_| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .returning(|| clientState_t::CS_ACTIVE);
        mock_client
    });

    let send_server_cmd_ctx = shinqlx_send_server_command_context();
    send_server_cmd_ctx
        .expect()
        .withf(|_, cmd| cmd == "cp \"asdf\"")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.center_print("asdf");
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn tell_with_no_keywords(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(|_| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .returning(|| clientState_t::CS_ACTIVE);
        mock_client
    });

    let send_server_cmd_ctx = shinqlx_send_server_command_context();
    send_server_cmd_ctx
        .expect()
        .withf(|_client, msg| msg == "print \"asdf\n\"\n")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.tell("asdf", None);
            assert!(result.is_ok());

            let _ = run_all_frame_tasks(py);
        });
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn tell_can_be_called_from_python(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(|_| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .returning(|| clientState_t::CS_ACTIVE);
        mock_client
    });

    let send_server_cmd_ctx = shinqlx_send_server_command_context();
    send_server_cmd_ctx
        .expect()
        .withf(|_client, msg| msg == "print \"asdf\n\"\n")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.call_method1("tell", ("asdf",));
            assert!(result.is_ok());

            let _ = run_all_frame_tasks(py);
        });
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn tell_with_limit_keyword(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(|_| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .returning(|| clientState_t::CS_ACTIVE);
        mock_client.expect_get_user_info().return_const("");
        mock_client
            .expect_get_steam_id()
            .return_const(1234567890u64);
        mock_client
    });

    let send_server_cmd_ctx = shinqlx_send_server_command_context();
    send_server_cmd_ctx
        .expect()
        .withf(|_client, msg| msg == "print \"These \nare \nfour \nlines\n\"\n")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    player_info: PlayerInfo {
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.tell(
                "These are four lines",
                Some(
                    &[("limit", 5)]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                ),
            );
            assert!(result.is_ok());

            let _ = run_all_frame_tasks(py);
        });
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn tell_with_delimiter_keyword(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(|_| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .returning(|| clientState_t::CS_ACTIVE);
        mock_client.expect_get_user_info().return_const("");
        mock_client
            .expect_get_steam_id()
            .return_const(1234567890u64);
        mock_client
    });

    let send_server_cmd_ctx = shinqlx_send_server_command_context();
    send_server_cmd_ctx
        .expect()
        .withf(|_client, msg| msg == "print \"These_\nare_\nfour_\nlines\n\"\n")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    player_info: PlayerInfo {
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.tell(
                "These_are_four_lines",
                Some(
                    &[
                        ("limit", PyInt::new(py, 5i32).into_any()),
                        ("delimiter", PyString::intern(py, "_").into_any()),
                    ]
                    .into_py_dict(py)
                    .expect("this should not happen"),
                ),
            );
            assert!(result.is_ok());

            let _ = run_all_frame_tasks(py);
        });
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn kick_kicks_player(_pyshinqlx_setup: ()) {
    let client_from_ctx = MockClient::from_context();
    client_from_ctx.expect().returning(|_| {
        let mut mock_client = MockClient::new();
        mock_client
            .expect_get_state()
            .returning(|| clientState_t::CS_ACTIVE);
        mock_client.expect_get_user_info().return_const("");
        mock_client
            .expect_get_steam_id()
            .return_const(1234567890u64);
        mock_client
    });

    let drop_client_ctx = shinqlx_drop_client_context();
    drop_client_ctx
        .expect()
        .withf(|_client, reason| reason == "you stink, go away!")
        .times(1);

    MockEngineBuilder::default().with_max_clients(16).run(|| {
        Python::attach(|py| {
            let player = Bound::new(
                py,
                Player {
                    player_info: PlayerInfo {
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        ..default_test_player_info()
                    }
                    .into(),
                    ..default_test_player()
                },
            )
            .expect("this should not happen");

            let result = player.kick("you stink, go away!");
            assert!(result.is_ok());
        });
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn ban_bans_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("ban 2", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.ban();
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn tempban_tempbans_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("tempban 2", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.tempban();
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn addadmin_adds_player_to_admins(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("addadmin 2", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.addadmin();
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn addmod_adds_player_to_mods(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("addmod 2", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.addmod();
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn demote_demotes_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("demote 2", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.demote();
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn mute_mutes_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("mute 2", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.mute();
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn unmute_unmutes_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("unmute 2", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.unmute();
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn put_with_invalid_team(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(py, default_test_player()).expect("this should not happen");

        let result = player.put("invalid team");
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[case("red")]
#[case("RED")]
#[case("free")]
#[case("blue")]
#[case("spectator")]
#[serial]
#[cfg_attr(miri, ignore)]
fn put_put_player_on_a_specific_team(_pyshinqlx_setup: (), #[case] new_team: &str) {
    MockEngineBuilder::default()
        .with_execute_console_command(format!("put 2 {}", new_team.to_lowercase()), 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.put(new_team);
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn addscore_adds_score_to_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("addscore 2 42", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.addscore(42);
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn switch_with_player_on_same_team(_pyshinqlx_setup: ()) {
    Python::attach(|py| {
        let player = Bound::new(
            py,
            Player {
                id: 2,
                player_info: PlayerInfo {
                    team: team_t::TEAM_SPECTATOR as i32,
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");
        let other_player = Bound::new(
            py,
            Player {
                id: 1,
                player_info: PlayerInfo {
                    team: team_t::TEAM_SPECTATOR as i32,
                    ..default_test_player_info()
                }
                .into(),
                ..default_test_player()
            },
        )
        .expect("this should not happen");

        let result = player.switch(&other_player);
        assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
    });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn switch_with_player_on_different_team(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("put 2 blue", 1)
        .with_execute_console_command("put 1 red", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(
                    py,
                    Player {
                        id: 2,
                        player_info: PlayerInfo {
                            team: team_t::TEAM_RED as i32,
                            ..default_test_player_info()
                        }
                        .into(),
                        ..default_test_player()
                    },
                )
                .expect("this should not happen");
                let other_player = Bound::new(
                    py,
                    Player {
                        id: 1,
                        player_info: PlayerInfo {
                            team: team_t::TEAM_BLUE as i32,
                            ..default_test_player_info()
                        }
                        .into(),
                        ..default_test_player()
                    },
                )
                .expect("this should not happen");

                let result = player.switch(&other_player);
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn slap_slaps_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("slap 2 42", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.slap(42);
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn slay_slays_player(_pyshinqlx_setup: ()) {
    MockEngineBuilder::default()
        .with_execute_console_command("slay 2", 1)
        .run(|| {
            Python::attach(|py| {
                let player = Bound::new(py, default_test_player()).expect("this should not happen");

                let result = player.slay();
                assert!(result.is_ok());
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn slay_with_mod_slays_with_mod(_pyshinqlx_setup: ()) {
    MockGameEntityBuilder::default()
        .with_health(0, 1..)
        .with_slay_with_mod(predicate::always(), 0)
        .with_game_client(|| Ok(MockGameClient::new()))
        .run(predicate::always(), || {
            MockEngineBuilder::default().with_max_clients(16).run(|| {
                Python::attach(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.slay_with_mod(meansOfDeath_t::MOD_PROXIMITY_MINE as i32);
                    assert!(result.is_ok());
                });
            });
        });
}

#[rstest]
#[serial]
#[cfg_attr(miri, ignore)]
fn all_players_for_existing_clients(_pyshinqlx_setup: ()) {
    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(0))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    client_try_from_ctx
        .expect()
        .with(predicate::eq(1))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_FREE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    let client_try_from_ctx = MockClient::from_context();
    client_try_from_ctx
        .expect()
        .with(predicate::eq(2))
        .returning(|_client_id| {
            let mut mock_client = MockClient::new();
            mock_client
                .expect_get_state()
                .returning(|| clientState_t::CS_ACTIVE);
            mock_client
                .expect_get_user_info()
                .returning(|| "asdf".into());
            mock_client.expect_get_steam_id().returning(|| 1234);
            mock_client
        });

    let game_entity_try_from_ctx = MockGameEntity::from_context();
    game_entity_try_from_ctx.expect().returning(|_client_id| {
        let mut mock_game_entity = MockGameEntity::new();
        mock_game_entity
            .expect_get_player_name()
            .returning(|| "Mocked Player".to_string());
        mock_game_entity
            .expect_get_team()
            .returning(|| team_t::TEAM_RED);
        mock_game_entity
            .expect_get_privileges()
            .returning(|| privileges_t::PRIV_NONE);
        mock_game_entity
    });

    MockEngineBuilder::default().with_max_clients(3).run(|| {
        let all_players = Python::attach(|py| Player::all_players(&py.get_type::<Player>()));
        assert_eq!(
            all_players.expect("result was not ok"),
            vec![
                Player {
                    valid: true.into(),
                    id: 0,
                    player_info: PlayerInfo {
                        client_id: 0,
                        name: "Mocked Player".to_string(),
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        userinfo: "asdf".to_string(),
                        steam_id: 1234,
                        team: team_t::TEAM_RED as i32,
                        privileges: 0,
                    }
                    .into(),
                    name: "Mocked Player".to_string().into(),
                    steam_id: 1234,
                    user_info: "asdf".to_string(),
                },
                Player {
                    valid: true.into(),
                    id: 2,
                    player_info: PlayerInfo {
                        client_id: 2,
                        name: "Mocked Player".to_string(),
                        connection_state: clientState_t::CS_ACTIVE as i32,
                        userinfo: "asdf".to_string(),
                        steam_id: 1234,
                        team: team_t::TEAM_RED as i32,
                        privileges: 0,
                    }
                    .into(),
                    name: "Mocked Player".to_string().into(),
                    steam_id: 1234,
                    user_info: "asdf".to_string(),
                },
            ]
        );
    });
}
