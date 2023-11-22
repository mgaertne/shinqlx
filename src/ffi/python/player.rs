use super::{clean_text, parse_variables, PlayerInfo};
use pyo3::basic::CompareOp;
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyKeyError};
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDict};

create_exception!(pyshinqlx_module, NonexistentPlayerError, PyException);

/// A class that represents a player on the server. As opposed to minqlbot,
///    attributes are all the values from when the class was instantiated. This
///    means for instance if a player is on the blue team when you check, but
///    then moves to red, it will still be blue when you check a second time.
///    To update it, use :meth:`~.Player.update`. Note that if you update it
///    and the player has disconnected, it will raise a
///    :exc:`shinqlx.NonexistentPlayerError` exception.
#[pyclass]
#[pyo3(module = "shinqlx", name = "Player", get_all)]
#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct Player {
    #[pyo3(name = "_valid")]
    valid: bool,
    #[pyo3(name = "_id")]
    id: i32,
    #[pyo3(name = "_playerinfo")]
    player_info: PlayerInfo,
    #[pyo3(name = "_userinfo")]
    user_info: String,
    #[pyo3(name = "_steam_id")]
    steam_id: u64,
    #[pyo3(name = "_name")]
    name: String,
}

#[pymethods]
impl Player {
    #[new]
    #[pyo3(signature = (client_id, info = None))]
    fn py_new(client_id: i32, info: Option<PlayerInfo>) -> PyResult<Self> {
        let player_info = info.unwrap_or_else(|| PlayerInfo::from(client_id));

        // When a player connects, the name field in the client struct has yet to be initialized,
        // so we fall back to the userinfo and try parse it ourselves to get the name if needed.
        let name = if player_info.name.is_empty() {
            let cvars = parse_variables(player_info.userinfo.clone());
            cvars
                .into_iter()
                .filter(|(key, _value)| *key == "name")
                .map(|(_key, value)| value)
                .nth(0)
                .unwrap_or_default()
        } else {
            player_info.name.clone()
        };

        Ok(Player {
            valid: true,
            id: client_id,
            user_info: player_info.userinfo.clone(),
            steam_id: player_info.steam_id,
            player_info,
            name,
        })
    }

    fn __repr__(slf: &PyCell<Self>) -> String {
        let Ok(classname) = slf.get_type().name() else {
            return "NonexistentPlayer".into();
        };
        let Ok(id) = slf.getattr("id") else {
            return format!("{classname}(N/A:'':-1)");
        };
        let Ok(clean_name) = slf.getattr("clean_name") else {
            return format!("{classname}({id}:'':-1)");
        };
        let Ok(steam_id) = slf.getattr("steam_id") else {
            return format!("{classname}({id}:'{clean_name}':-1)");
        };
        format!("{classname}({id}:'{clean_name}':{steam_id})")
    }

    fn __str__(&self) -> String {
        self.name.clone()
    }

    fn __contains__(&self, item: String) -> PyResult<bool> {
        if !self.valid {
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        Ok(parse_variables(self.user_info.clone())
            .iter()
            .any(|(key, _value)| *key == item))
    }

    fn __getitem__(&self, item: String) -> PyResult<String> {
        if !self.valid {
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        let opt_value = parse_variables(self.user_info.clone())
            .into_iter()
            .filter(|(key, _value)| *key == item)
            .map(|(_key, value)| value)
            .nth(0);
        opt_value.map_or_else(|| Err(PyKeyError::new_err(format!("'{item}'"))), Ok)
    }

    fn __richcmp__(&self, other: &PyAny, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => {
                if let Ok(other_player) = other.extract::<Self>() {
                    (*self == other_player).into_py(py)
                } else if let Ok(steam_id) = other.extract::<u64>() {
                    (self.steam_id == steam_id).into_py(py)
                } else {
                    false.into_py(py)
                }
            }
            CompareOp::Ne => {
                if let Ok(other_player) = other.extract::<Self>() {
                    (*self != other_player).into_py(py)
                } else if let Ok(steam_id) = other.extract::<u64>() {
                    (self.steam_id != steam_id).into_py(py)
                } else {
                    true.into_py(py)
                }
            }
            _ => py.NotImplemented(),
        }
    }

    #[getter(cvars)]
    fn get_cvars<'a>(&self, py: Python<'a>) -> PyResult<&'a PyDict> {
        if !self.valid {
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        Ok(parse_variables(self.user_info.clone()).into_py_dict(py))
    }

    #[getter(steam_id)]
    fn get_steam_id(&self) -> u64 {
        self.steam_id
    }

    #[getter(id)]
    fn get_id(&self) -> i32 {
        self.id
    }

    #[getter(clean_name)]
    fn get_clean_name(&self) -> String {
        clean_text(&self.name)
    }
}

#[cfg(test)]
mod pyshinqlx_player_tests {
    use super::{NonexistentPlayerError, Player};
    use crate::ffi::c::client::MockClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    #[cfg(not(miri))]
    use crate::ffi::python::pyshinqlx_setup_fixture::*;
    use crate::ffi::python::PlayerInfo;
    use crate::prelude::clientState_t::CS_CONNECTED;
    use crate::prelude::privileges_t::PRIV_NONE;
    use crate::prelude::team_t::TEAM_SPECTATOR;
    use crate::prelude::*;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::PyKeyError;
    use pyo3::types::IntoPyDict;
    use pyo3::{IntoPy, PyCell, Python};
    use rstest::rstest;

    #[test]
    #[serial]
    fn pyconstructor_with_empty_playerinfo() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity
                    .expect_get_player_name()
                    .return_const("UnnamedPlayer");
                mock_entity.expect_get_team().return_const(TEAM_SPECTATOR);
                mock_entity.expect_get_privileges().return_const(PRIV_NONE);
                mock_entity
            });
        let client_from_ctx = MockClient::from_context();
        client_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_client = MockClient::new();
                mock_client.expect_get_state().return_const(CS_CONNECTED);
                mock_client.expect_get_user_info().return_const("");
                mock_client
                    .expect_get_steam_id()
                    .return_const(1234567890u64);
                mock_client
            });

        let result = Player::py_new(2, None);
        assert_eq!(
            result.expect("result was not OK"),
            Player {
                valid: true,
                id: 2,
                player_info: PlayerInfo {
                    client_id: 2,
                    name: "UnnamedPlayer".to_string(),
                    connection_state: CS_CONNECTED as i32,
                    userinfo: "".to_string(),
                    steam_id: 1234567890,
                    team: TEAM_SPECTATOR as i32,
                    privileges: PRIV_NONE as i32,
                },
                user_info: "".to_string(),
                steam_id: 1234567890,
                name: "UnnamedPlayer".to_string(),
            }
        );
    }

    #[test]
    fn pyconstructor_with_empty_name() {
        let result = Player::py_new(
            2,
            Some(PlayerInfo {
                client_id: 2,
                name: "".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "\\name\\UnnamedPlayer".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            }),
        );
        assert_eq!(
            result.expect("result was not OK"),
            Player {
                valid: true,
                id: 2,
                player_info: PlayerInfo {
                    client_id: 2,
                    name: "".to_string(),
                    connection_state: CS_CONNECTED as i32,
                    userinfo: "\\name\\UnnamedPlayer".to_string(),
                    steam_id: 1234567890,
                    team: TEAM_SPECTATOR as i32,
                    privileges: PRIV_NONE as i32,
                },
                user_info: "\\name\\UnnamedPlayer".to_string(),
                steam_id: 1234567890,
                name: "UnnamedPlayer".to_string(),
            }
        );
    }

    #[test]
    fn pyconstructor_with_empty_name_and_no_name_in_userinfo() {
        let result = Player::py_new(
            2,
            Some(PlayerInfo {
                client_id: 2,
                name: "".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            }),
        );
        assert_eq!(
            result.expect("result was not OK"),
            Player {
                valid: true,
                id: 2,
                player_info: PlayerInfo {
                    client_id: 2,
                    name: "".to_string(),
                    connection_state: CS_CONNECTED as i32,
                    userinfo: "".to_string(),
                    steam_id: 1234567890,
                    team: TEAM_SPECTATOR as i32,
                    privileges: PRIV_NONE as i32,
                },
                user_info: "".to_string(),
                steam_id: 1234567890,
                name: "".to_string(),
            }
        );
    }

    #[test]
    fn pyconstructor_with_nonempty_playerinfo() {
        let result = Player::py_new(
            2,
            Some(PlayerInfo {
                client_id: 2,
                name: "UnnamedPlayer".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            }),
        );
        assert_eq!(
            result.expect("result was not OK"),
            Player {
                valid: true,
                id: 2,
                player_info: PlayerInfo {
                    client_id: 2,
                    name: "UnnamedPlayer".to_string(),
                    connection_state: CS_CONNECTED as i32,
                    userinfo: "".to_string(),
                    steam_id: 1234567890,
                    team: TEAM_SPECTATOR as i32,
                    privileges: PRIV_NONE as i32,
                },
                user_info: "".to_string(),
                steam_id: 1234567890,
                name: "UnnamedPlayer".to_string(),
            }
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn repr_with_all_values_set() {
        let result = Python::with_gil(|py| {
            let player = PyCell::new(
                py,
                Player {
                    valid: true,
                    id: 2,
                    player_info: PlayerInfo {
                        client_id: 2,
                        name: "UnnamedPlayer".to_string(),
                        connection_state: CS_CONNECTED as i32,
                        userinfo: "".to_string(),
                        steam_id: 1234567890,
                        team: TEAM_SPECTATOR as i32,
                        privileges: PRIV_NONE as i32,
                    },
                    user_info: "".to_string(),
                    steam_id: 1234567890,
                    name: "UnnamedPlayer".to_string(),
                },
            )
            .expect("this should not happen");
            Player::__repr__(player)
        });
        assert_eq!(result, "Player(2:'UnnamedPlayer':1234567890)");
    }

    #[test]
    fn str_returns_player_name() {
        let player = Player {
            valid: true,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "^1Unnamed^2Player".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            },
            user_info: "".to_string(),
            steam_id: 1234567890,
            name: "^1Unnamed^2Player".to_string(),
        };
        assert_eq!(player.__str__(), "^1Unnamed^2Player");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn contains_with_invalid_player() {
        let player = Player {
            valid: false,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "^1Unnamed^2Player".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            },
            user_info: "".to_string(),
            steam_id: 1234567890,
            name: "^1Unnamed^2Player".to_string(),
        };
        let result = player.__contains__("asdf".into());
        Python::with_gil(|py| {
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
    }

    #[test]
    fn contains_where_value_is_part_of_userinfo() {
        let player = Player {
            valid: true,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "^1Unnamed^2Player".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "\\asdf\\some value".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            },
            user_info: "\\asdf\\some value".to_string(),
            steam_id: 1234567890,
            name: "^1Unnamed^2Player".to_string(),
        };

        let result = player.__contains__("asdf".into());
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    fn contains_where_value_is_not_in_userinfo() {
        let player = Player {
            valid: true,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "^1Unnamed^2Player".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "\\name\\^1Unnamed^2Player".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            },
            user_info: "\\name\\^1Unnamed^2Player".to_string(),
            steam_id: 1234567890,
            name: "^1Unnamed^2Player".to_string(),
        };

        let result = player.__contains__("asdf".into());
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn getitem_with_invalid_player() {
        let player = Player {
            valid: false,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "^1Unnamed^2Player".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            },
            user_info: "".to_string(),
            steam_id: 1234567890,
            name: "^1Unnamed^2Player".to_string(),
        };
        let result = player.__getitem__("asdf".into());
        Python::with_gil(|py| {
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
    }

    #[test]
    fn getitem_where_value_is_part_of_userinfo() {
        let player = Player {
            valid: true,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "^1Unnamed^2Player".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "\\asdf\\some value".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            },
            user_info: "\\asdf\\some value".to_string(),
            steam_id: 1234567890,
            name: "^1Unnamed^2Player".to_string(),
        };

        let result = player.__getitem__("asdf".into());
        assert_eq!(result.expect("result was not OK"), "some value");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn getitem_where_value_is_not_in_userinfo() {
        let player = Player {
            valid: true,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "^1Unnamed^2Player".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "\\name\\^1Unnamed^2Player".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            },
            user_info: "\\name\\^1Unnamed^2Player".to_string(),
            steam_id: 1234567890,
            name: "^1Unnamed^2Player".to_string(),
        };

        let result = player.__getitem__("asdf".into());
        Python::with_gil(|py| {
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)))
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cvars_with_invalid_player() {
        let player = Player {
            valid: false,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "^1Unnamed^2Player".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            },
            user_info: "".to_string(),
            steam_id: 1234567890,
            name: "^1Unnamed^2Player".to_string(),
        };
        Python::with_gil(|py| {
            let result = player.get_cvars(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cvars_where_value_is_part_of_userinfo() {
        let player = Player {
            valid: true,
            id: 2,
            player_info: PlayerInfo {
                client_id: 2,
                name: "^1Unnamed^2Player".to_string(),
                connection_state: CS_CONNECTED as i32,
                userinfo: "\\asdf\\some value".to_string(),
                steam_id: 1234567890,
                team: TEAM_SPECTATOR as i32,
                privileges: PRIV_NONE as i32,
            },
            user_info: "\\asdf\\some value".to_string(),
            steam_id: 1234567890,
            name: "^1Unnamed^2Player".to_string(),
        };

        Python::with_gil(|py| {
            let result = player.get_cvars(py);
            assert!(result
                .expect("result was not OK")
                .get_item("asdf")
                .is_ok_and(|opt_value| opt_value.is_some_and(|value| value
                    .extract::<String>()
                    .expect("this should not happen")
                    == "some value")))
        });
    }

    #[cfg(not(miri))]
    #[rstest]
    fn player_can_be_compared_for_equality_with_other_player_in_python(_pyshinqlx_setup: ()) {
        let player_info = PlayerInfo {
            client_id: 42,
            name: "test".to_string(),
            connection_state: CS_CONNECTED as i32,
            userinfo: "".to_string(),
            steam_id: 1234567890,
            team: TEAM_SPECTATOR as i32,
            privileges: PRIV_NONE as i32,
        };
        let result = Python::with_gil(|py| {
            py.run(
                r#"
import _shinqlx
assert(_shinqlx.Player(42, player_info) == _shinqlx.Player(42, player_info))
assert((_shinqlx.Player(42, player_info) == _shinqlx.Player(41, player_info)) == False)
            "#,
                None,
                Some([("player_info", player_info.into_py(py))].into_py_dict(py)),
            )
        });
        assert!(result.as_ref().is_ok(), "{:?}", result.unwrap());
    }

    #[cfg(not(miri))]
    #[rstest]
    fn player_can_be_compared_for_equality_with_steam_id_in_python(_pyshinqlx_setup: ()) {
        let player_info = PlayerInfo {
            client_id: 42,
            name: "test".to_string(),
            connection_state: CS_CONNECTED as i32,
            userinfo: "".to_string(),
            steam_id: 1234567890,
            team: TEAM_SPECTATOR as i32,
            privileges: PRIV_NONE as i32,
        };
        let result = Python::with_gil(|py| {
            py.run(
                r#"
import _shinqlx
assert(_shinqlx.Player(42, player_info) == 1234567890)
assert((_shinqlx.Player(42, player_info) == 1234567891) == False)
            "#,
                None,
                Some([("player_info", player_info.into_py(py))].into_py_dict(py)),
            )
        });
        assert!(result.as_ref().is_ok(), "{:?}", result.unwrap());
    }

    #[cfg(not(miri))]
    #[rstest]
    fn player_can_be_compared_for_inequality_with_other_player_in_python(_pyshinqlx_setup: ()) {
        let player_info = PlayerInfo {
            client_id: 42,
            name: "test".to_string(),
            connection_state: CS_CONNECTED as i32,
            userinfo: "".to_string(),
            steam_id: 1234567890,
            team: TEAM_SPECTATOR as i32,
            privileges: PRIV_NONE as i32,
        };
        let result = Python::with_gil(|py| {
            py.run(
                r#"
import _shinqlx
assert((_shinqlx.Player(42, player_info) != _shinqlx.Player(42, player_info)) == False)
assert(_shinqlx.Player(42, player_info) != _shinqlx.Player(41, player_info))
            "#,
                None,
                Some([("player_info", player_info.into_py(py))].into_py_dict(py)),
            )
        });
        assert!(result.as_ref().is_ok(), "{:?}", result.unwrap());
    }

    #[cfg(not(miri))]
    #[rstest]
    fn player_can_be_compared_for_inequality_with_steam_id_in_python(_pyshinqlx_setup: ()) {
        let player_info = PlayerInfo {
            client_id: 42,
            name: "test".to_string(),
            connection_state: CS_CONNECTED as i32,
            userinfo: "".to_string(),
            steam_id: 1234567890,
            team: TEAM_SPECTATOR as i32,
            privileges: PRIV_NONE as i32,
        };
        let result = Python::with_gil(|py| {
            py.run(
                r#"
import _shinqlx
assert((_shinqlx.Player(42, player_info) != 1234567890) == False)
assert(_shinqlx.Player(42, player_info) != 1234567891)
            "#,
                None,
                Some([("player_info", player_info.into_py(py))].into_py_dict(py)),
            )
        });
        assert!(result.as_ref().is_ok(), "{:?}", result.unwrap());
    }
}
