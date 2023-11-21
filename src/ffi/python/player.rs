use crate::ffi::python::clean_text;
use crate::ffi::python::{parse_variables, PlayerInfo};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

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

    #[getter(id)]
    fn get_id(&self) -> i32 {
        self.id
    }
    #[getter(steam_id)]
    fn get_steam_id(&self) -> u64 {
        self.steam_id
    }

    #[getter(clean_name)]
    fn get_clean_name(&self) -> String {
        clean_text(&self.name)
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod pyshinqlx_player_tests {
    use super::Player;
    use crate::ffi::c::client::MockClient;
    use crate::ffi::c::game_entity::MockGameEntity;
    use crate::ffi::python::PlayerInfo;
    use crate::prelude::clientState_t::CS_CONNECTED;
    use crate::prelude::privileges_t::PRIV_NONE;
    use crate::prelude::team_t::TEAM_SPECTATOR;
    use crate::prelude::*;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::{PyCell, Python};

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
    #[serial]
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
    #[serial]
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
    #[serial]
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
    #[serial]
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
    #[serial]
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
}
