use super::{clean_text, parse_variables, PlayerInfo};
use crate::ffi::python::embed::{pyshinqlx_client_command, pyshinqlx_console_command};
use crate::prelude::*;
use crate::quake_live_engine::{GetConfigstring, SetConfigstring};
use crate::MAIN_ENGINE;
use itertools::Itertools;
use pyo3::basic::CompareOp;
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyKeyError, PyValueError};
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
    #[pyo3(name = "_info")]
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
            cvars.get("name").unwrap_or_default()
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

    fn __contains__(&self, py: Python<'_>, item: String) -> PyResult<bool> {
        if !self.valid {
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        py.allow_threads(|| {
            let cvars = parse_variables(self.user_info.clone());
            Ok(cvars.get(item).is_some())
        })
    }

    fn __getitem__(&self, py: Python<'_>, item: String) -> PyResult<String> {
        if !self.valid {
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        py.allow_threads(|| {
            let cvars = parse_variables(self.user_info.clone());
            cvars
                .get(&item)
                .map_or_else(|| Err(PyKeyError::new_err(format!("'{item}'"))), Ok)
        })
    }

    fn __richcmp__(&self, other: &PyAny, op: CompareOp, py: Python<'_>) -> PyObject {
        match op {
            CompareOp::Eq => {
                if let Ok(other_player) = other.extract::<Self>() {
                    (self.steam_id == other_player.steam_id).into_py(py)
                } else if let Ok(steam_id) = other.extract::<u64>() {
                    (self.steam_id == steam_id).into_py(py)
                } else {
                    false.into_py(py)
                }
            }
            CompareOp::Ne => {
                if let Ok(other_player) = other.extract::<Self>() {
                    (self.steam_id != other_player.steam_id).into_py(py)
                } else if let Ok(steam_id) = other.extract::<u64>() {
                    (self.steam_id != steam_id).into_py(py)
                } else {
                    true.into_py(py)
                }
            }
            _ => py.NotImplemented(),
        }
    }

    ///Update the player information with the latest data. If the player
    ///         disconnected it will raise an exception and invalidates a player.
    ///         The player's name and Steam ID can still be accessed after being
    ///         invalidated, but anything else will make it throw an exception too.
    ///
    ///         :raises: shinqlx.NonexistentPlayerError
    fn update(&mut self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| self.player_info = PlayerInfo::from(self.id));

        if self.player_info.steam_id != self.steam_id {
            self.valid = false;
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        py.allow_threads(|| {
            let name = if self.player_info.name.is_empty() {
                let cvars = parse_variables(self.player_info.userinfo.clone());
                cvars.get("name").unwrap_or_default()
            } else {
                self.player_info.name.clone()
            };
            self.name = name;
        });

        Ok(())
    }

    #[pyo3(
        name = "_invalidate",
        signature = (e="The player does not exist anymore. Did the player disconnect?".into())
    )]
    fn invalidate(&mut self, e: String) -> PyResult<()> {
        self.valid = false;
        Err(NonexistentPlayerError::new_err(e))
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

    #[setter(cvars)]
    fn set_cvars(&self, py: Python<'_>, new_cvars: &PyDict) -> PyResult<()> {
        let new = new_cvars
            .iter()
            .map(|(key, value)| format!("\\{key}\\{value}"))
            .join("");
        let client_command = format!("userinfo {new}");
        pyshinqlx_client_command(py, self.id, client_command.as_str())?;
        Ok(())
    }

    #[getter(steam_id)]
    fn get_steam_id(&self) -> u64 {
        self.steam_id
    }

    #[getter(id)]
    fn get_id(&self) -> i32 {
        self.id
    }

    #[getter(ip)]
    fn get_ip(&self, py: Python<'_>) -> String {
        py.allow_threads(|| {
            let cvars = parse_variables(self.user_info.clone());
            cvars
                .get("ip")
                .map(|value| value.split(':').next().unwrap_or("").into())
                .unwrap_or("".into())
        })
    }

    /// The clan tag. Not actually supported by QL, but it used to be and
    /// fortunately the scoreboard still properly displays it if we manually
    /// set the configstring to use clan tags.
    #[getter(clan)]
    fn get_clan(&self, py: Python<'_>) -> String {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return "".into();
            };

            let configstring = main_engine.get_configstring(529 + self.id as u16);
            let parsed_cs = parse_variables(configstring);
            parsed_cs.get("cn").unwrap_or("".into())
        })
    }

    #[setter(clan)]
    fn set_clan(&self, py: Python<'_>, tag: String) {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return;
            };

            let config_index = 529 + self.id as u16;

            let configstring = main_engine.get_configstring(config_index);
            let mut parsed_variables = parse_variables(configstring);
            parsed_variables.set("xcn".into(), tag.clone());
            parsed_variables.set("cn".into(), tag.clone());

            let new_configstring: String = parsed_variables.into();
            main_engine.set_configstring(config_index as i32, new_configstring.as_str());
        })
    }

    #[getter(name)]
    fn get_name(&self, py: Python<'_>) -> String {
        py.allow_threads(|| {
            if self.name.ends_with("^7") {
                self.name.clone()
            } else {
                format!("{}^7", self.name)
            }
        })
    }

    #[setter(name)]
    fn set_name(&self, py: Python<'_>, value: String) -> PyResult<()> {
        let new: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(self.user_info.clone());
            new_cvars.set("name".into(), value);
            new_cvars.into()
        });

        let client_command = format!("userinfo {new}");
        pyshinqlx_client_command(py, self.id, client_command.as_str())?;
        Ok(())
    }

    /// Removes color tags from the name.
    #[getter(clean_name)]
    fn get_clean_name(&self, py: Python<'_>) -> String {
        py.allow_threads(|| clean_text(&self.name))
    }

    #[getter(qport)]
    fn get_qport(&self, py: Python<'_>) -> i32 {
        py.allow_threads(|| {
            let cvars = parse_variables(self.user_info.clone());
            cvars
                .get("qport")
                .map(|value| value.parse::<i32>().unwrap_or(-1))
                .unwrap_or(-1)
        })
    }

    #[getter(team)]
    fn get_team(&self, py: Python<'_>) -> PyResult<String> {
        py.allow_threads(|| match team_t::try_from(self.player_info.team) {
            Ok(team_t::TEAM_FREE) => Ok("free".into()),
            Ok(team_t::TEAM_RED) => Ok("red".into()),
            Ok(team_t::TEAM_BLUE) => Ok("blue".into()),
            Ok(team_t::TEAM_SPECTATOR) => Ok("spectator".into()),
            _ => Err(PyValueError::new_err("invalid team")),
        })
    }

    #[setter(team)]
    fn set_team(&self, py: Python<'_>, new_team: String) -> PyResult<()> {
        if !["free", "red", "blue", "spectator"].contains(&new_team.to_lowercase().as_str()) {
            return Err(PyValueError::new_err("Invalid team."));
        }

        let team_change_cmd = format!("put {} {}", self.id, new_team.to_lowercase());
        pyshinqlx_console_command(py, team_change_cmd.as_str())
    }

    #[getter(colors)]
    fn get_colors(&self, py: Python<'_>) -> (f32, f32) {
        py.allow_threads(|| {
            let cvars = parse_variables(self.user_info.clone());
            let color1 = cvars
                .get("color1")
                .map(|value| value.parse::<f32>().unwrap_or(0.0))
                .unwrap_or(0.0);
            let color2 = cvars
                .get("color2")
                .map(|value| value.parse::<f32>().unwrap_or(0.0))
                .unwrap_or(0.0);
            (color1, color2)
        })
    }

    #[setter(colors)]
    fn set_colors(&self, py: Python<'_>, new: (i32, i32)) -> PyResult<()> {
        let new_cvars_string: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(self.player_info.userinfo.clone());
            new_cvars.set("color1".into(), format!("{}", new.0));
            new_cvars.set("color2".into(), format!("{}", new.1));
            new_cvars.into()
        });

        let client_command = format!("userinfo {new_cvars_string}");
        pyshinqlx_client_command(py, self.id, client_command.as_str())?;
        Ok(())
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
    use crate::hooks::mock_hooks::shinqlx_execute_client_command_context;
    use crate::prelude::*;
    use crate::quake_live_engine::MockQuakeEngine;
    use crate::MAIN_ENGINE;
    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyKeyError, PyValueError};
    use pyo3::types::IntoPyDict;
    use pyo3::{IntoPy, PyCell, Python};
    use rstest::rstest;

    fn default_test_player_info() -> PlayerInfo {
        PlayerInfo {
            client_id: 2,
            name: "".to_string(),
            connection_state: clientState_t::CS_CONNECTED as i32,
            userinfo: "".to_string(),
            steam_id: 1234567890,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: privileges_t::PRIV_NONE as i32,
        }
    }

    fn default_test_player() -> Player {
        Player {
            valid: true,
            id: 2,
            player_info: default_test_player_info(),
            user_info: "".to_string(),
            steam_id: 1234567890,
            name: "".to_string(),
        }
    }

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
                mock_entity
                    .expect_get_team()
                    .return_const(team_t::TEAM_SPECTATOR);
                mock_entity
                    .expect_get_privileges()
                    .return_const(privileges_t::PRIV_NONE);
                mock_entity
            });
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

        let result = Player::py_new(2, None);
        assert_eq!(
            result.expect("result was not OK"),
            Player {
                name: "UnnamedPlayer".to_string(),
                player_info: PlayerInfo {
                    name: "UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                },
                ..default_test_player()
            }
        );
    }

    #[test]
    fn pyconstructor_with_empty_name() {
        let result = Player::py_new(
            2,
            Some(PlayerInfo {
                userinfo: "\\name\\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }),
        );
        assert_eq!(
            result.expect("result was not OK"),
            Player {
                player_info: PlayerInfo {
                    userinfo: "\\name\\UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                },
                user_info: "\\name\\UnnamedPlayer".to_string(),
                name: "UnnamedPlayer".to_string(),
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
                name: "UnnamedPlayer".to_string(),
                player_info: PlayerInfo {
                    name: "UnnamedPlayer".to_string(),
                    ..default_test_player_info()
                },
                ..default_test_player()
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
                    player_info: PlayerInfo {
                        name: "UnnamedPlayer".to_string(),
                        ..default_test_player_info()
                    },
                    name: "UnnamedPlayer".to_string(),
                    ..default_test_player()
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
            player_info: PlayerInfo {
                name: "^1Unnamed^2Player".to_string(),
                ..default_test_player_info()
            },
            name: "^1Unnamed^2Player".to_string(),
            ..default_test_player()
        };
        assert_eq!(player.__str__(), "^1Unnamed^2Player");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn contains_with_invalid_player() {
        let player = Player {
            valid: false,
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.__contains__(py, "asdf".into());
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn contains_where_value_is_part_of_userinfo() {
        let player = Player {
            player_info: PlayerInfo {
                userinfo: "\\asdf\\some value".to_string(),
                ..default_test_player_info()
            },
            user_info: "\\asdf\\some value".to_string(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.__contains__(py, "asdf".into()));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn contains_where_value_is_not_in_userinfo() {
        let player = Player {
            player_info: PlayerInfo {
                userinfo: "\\name\\^1Unnamed^2Player".to_string(),
                ..default_test_player_info()
            },
            user_info: "\\name\\^1Unnamed^2Player".to_string(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.__contains__(py, "asdf".into()));
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn getitem_with_invalid_player() {
        let player = Player {
            valid: false,
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.__getitem__(py, "asdf".into());
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn getitem_where_value_is_part_of_userinfo() {
        let player = Player {
            player_info: PlayerInfo {
                userinfo: "\\asdf\\some value".to_string(),
                ..default_test_player_info()
            },
            user_info: "\\asdf\\some value".to_string(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.__getitem__(py, "asdf".into()));
        assert_eq!(result.expect("result was not OK"), "some value");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn getitem_where_value_is_not_in_userinfo() {
        let player = Player {
            player_info: PlayerInfo {
                userinfo: "\\name\\^1Unnamed^2Player".to_string(),
                ..default_test_player_info()
            },
            user_info: "\\name\\^1Unnamed^2Player".to_string(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.__getitem__(py, "asdf".into());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)))
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cvars_with_invalid_player() {
        let player = Player {
            valid: false,
            ..default_test_player()
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
            player_info: PlayerInfo {
                userinfo: "\\asdf\\some value".to_string(),
                ..default_test_player_info()
            },
            user_info: "\\asdf\\some value".to_string(),
            ..default_test_player()
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
            steam_id: 1234567890,
            ..default_test_player_info()
        };
        let player_info2 = PlayerInfo {
            client_id: 42,
            steam_id: 41,
            ..default_test_player_info()
        };
        let result = Python::with_gil(|py| {
            py.run(
                r#"
import _shinqlx
assert(_shinqlx.Player(42, player_info) == _shinqlx.Player(42, player_info))
assert((_shinqlx.Player(42, player_info) == _shinqlx.Player(41, player_info2)) == False)
            "#,
                None,
                Some(
                    [
                        ("player_info", player_info.into_py(py)),
                        ("player_info2", player_info2.into_py(py)),
                    ]
                    .into_py_dict(py),
                ),
            )
        });
        assert!(result.is_ok());
    }

    #[cfg(not(miri))]
    #[rstest]
    fn player_can_be_compared_for_equality_with_steam_id_in_python(_pyshinqlx_setup: ()) {
        let player_info = PlayerInfo {
            client_id: 42,
            steam_id: 1234567890,
            ..default_test_player_info()
        };
        let result = Python::with_gil(|py| {
            py.run(
                r#"
import _shinqlx
assert(_shinqlx.Player(42, player_info) == 1234567890)
assert((_shinqlx.Player(42, player_info) == 1234567891) == False)
assert((_shinqlx.Player(42, player_info) == "asdf") == False)
            "#,
                None,
                Some([("player_info", player_info.into_py(py))].into_py_dict(py)),
            )
        });
        assert!(result.is_ok());
    }

    #[cfg(not(miri))]
    #[rstest]
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
        let result = Python::with_gil(|py| {
            py.run(
                r#"
import _shinqlx
assert((_shinqlx.Player(42, player_info) != _shinqlx.Player(42, player_info)) == False)
assert(_shinqlx.Player(42, player_info) != _shinqlx.Player(41, player_info2))
            "#,
                None,
                Some(
                    [
                        ("player_info", player_info.into_py(py)),
                        ("player_info2", player_info2.into_py(py)),
                    ]
                    .into_py_dict(py),
                ),
            )
        });
        assert!(result.is_ok());
    }

    #[cfg(not(miri))]
    #[rstest]
    fn player_can_be_compared_for_inequality_with_steam_id_in_python(_pyshinqlx_setup: ()) {
        let player_info = PlayerInfo {
            client_id: 42,
            steam_id: 1234567890,
            ..default_test_player_info()
        };
        let result = Python::with_gil(|py| {
            py.run(
                r#"
import _shinqlx
assert((_shinqlx.Player(42, player_info) != 1234567890) == False)
assert(_shinqlx.Player(42, player_info) != 1234567891)
assert(_shinqlx.Player(42, player_info) != "asdf")
            "#,
                None,
                Some([("player_info", player_info.into_py(py))].into_py_dict(py)),
            )
        });
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn update_with_different_steam_id() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity
                    .expect_get_player_name()
                    .return_const("UnnamedPlayer");
                mock_entity
                    .expect_get_team()
                    .return_const(team_t::TEAM_SPECTATOR);
                mock_entity
                    .expect_get_privileges()
                    .return_const(privileges_t::PRIV_NONE);
                mock_entity
            });
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

        let mut player = Player {
            steam_id: 1234567890,
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.update(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
        assert_eq!(player.valid, false);
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn update_can_be_called_from_python() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity
                    .expect_get_player_name()
                    .return_const("UnnamedPlayer");
                mock_entity
                    .expect_get_team()
                    .return_const(team_t::TEAM_SPECTATOR);
                mock_entity
                    .expect_get_privileges()
                    .return_const(privileges_t::PRIV_NONE);
                mock_entity
            });
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

        let result = Python::with_gil(|py| {
            py.run(
                r#"
player.update()
assert(player._valid)
            "#,
                None,
                Some([("player", player.into_py(py))].into_py_dict(py)),
            )
        });
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn update_updates_new_player_name() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity
                    .expect_get_player_name()
                    .return_const("NewUnnamedPlayer");
                mock_entity
                    .expect_get_team()
                    .return_const(team_t::TEAM_SPECTATOR);
                mock_entity
                    .expect_get_privileges()
                    .return_const(privileges_t::PRIV_NONE);
                mock_entity
            });
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

        let mut player = Player {
            steam_id: 1234567890,
            ..default_test_player()
        };

        Python::with_gil(|py| player.update(py).unwrap());
        assert_eq!(player.valid, true);
        assert_eq!(player.name, "NewUnnamedPlayer");
    }

    #[test]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn update_updates_new_player_name_from_userinfo() {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_entity = MockGameEntity::new();
                mock_entity.expect_get_player_name().return_const("");
                mock_entity
                    .expect_get_team()
                    .return_const(team_t::TEAM_SPECTATOR);
                mock_entity
                    .expect_get_privileges()
                    .return_const(privileges_t::PRIV_NONE);
                mock_entity
            });
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
                    .return_const("\\name\\NewUnnamedPlayer");
                mock_client
                    .expect_get_steam_id()
                    .return_const(1234567890u64);
                mock_client
            });

        let mut player = Player {
            steam_id: 1234567890,
            ..default_test_player()
        };

        Python::with_gil(|py| player.update(py).unwrap());
        assert_eq!(player.valid, true);
        assert_eq!(player.name, "NewUnnamedPlayer");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn invalidate_invalidates_player() {
        let mut player = default_test_player();
        let result = player.invalidate("invalid player".into());
        assert_eq!(player.valid, false);
        Python::with_gil(|py| {
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_cvars_sets_client_cvars() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

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
                    && cmd == "userinfo \\asdf\\qwertz\\name\\UnnamedPlayer"
                    && client_ok
            })
            .times(1);

        let player = default_test_player();
        let result = Python::with_gil(|py| {
            player.set_cvars(
                py,
                [("asdf", "qwertz"), ("name", "UnnamedPlayer")].into_py_dict(py),
            )
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_ip_where_no_ip_is_set() {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };
        assert_eq!(Python::with_gil(|py| player.get_ip(py)), "");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_ip_for_ip_with_no_port() {
        let player = Player {
            user_info: "\\ip\\127.0.0.1".to_string(),
            player_info: PlayerInfo {
                userinfo: "\\ip\\127.0.0.1".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        assert_eq!(Python::with_gil(|py| player.get_ip(py)), "127.0.0.1");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_ip_for_ip_with_port() {
        let player = Player {
            user_info: "\\ip\\127.0.0.1:27666".to_string(),
            player_info: PlayerInfo {
                userinfo: "\\ip\\127.0.0.1:27666".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        assert_eq!(Python::with_gil(|py| player.get_ip(py)), "127.0.0.1");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_clan_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        let player = default_test_player();
        let result = Python::with_gil(|py| player.get_clan(py));
        assert_eq!(result, "");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_clan_with_no_clan_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(531))
            .returning(|_| "".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let player = default_test_player();
        let result = Python::with_gil(|py| player.get_clan(py));
        assert_eq!(result, "");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_clan_with_clan_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(531))
            .returning(|_| "\\cn\\asdf".into());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let player = default_test_player();
        let result = Python::with_gil(|py| player.get_clan(py));
        assert_eq!(result, "asdf");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_clan_with_no_main_engine() {
        MAIN_ENGINE.store(None);
        let player = default_test_player();
        Python::with_gil(|py| player.set_clan(py, "asdf".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_clan_with_no_clan_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(531))
            .returning(|_| "".into());
        mock_engine
            .expect_set_configstring()
            .withf(|index, value| {
                *index == 531i32 && value.contains("\\cn\\clan") && value.contains("\\xcn\\clan")
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let player = default_test_player();
        Python::with_gil(|py| player.set_clan(py, "clan".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_clan_with_clan_set() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(531))
            .returning(|_| "\\xcn\\asdf\\cn\\asdf".into());
        mock_engine
            .expect_set_configstring()
            .withf(|index, value| {
                *index == 531i32
                    && value.contains("\\cn\\clan")
                    && value.contains("\\xcn\\clan")
                    && !value.contains("\\cn\\asdf")
                    && !value.contains("\\xcn\\asdf")
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let player = default_test_player();
        Python::with_gil(|py| player.set_clan(py, "clan".into()));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_name_for_color_terminated_name() {
        let player = Player {
            name: "UnnamedPlayer^7".into(),
            player_info: PlayerInfo {
                name: "UnnamedPlayer^7".into(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        assert_eq!(
            Python::with_gil(|py| player.get_name(py)),
            "UnnamedPlayer^7"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_name_for_color_unterminated_name() {
        let player = Player {
            name: "UnnamedPlayer".into(),
            player_info: PlayerInfo {
                name: "UnnamedPlayer".into(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        assert_eq!(
            Python::with_gil(|py| player.get_name(py)),
            "UnnamedPlayer^7"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_name_updated_client_cvars() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

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
                    && cmd == "userinfo \\asdf\\qwertz\\name\\^1Unnamed^2Player"
                    && client_ok
            })
            .times(1);

        let player = Player {
            user_info: "\\asdf\\qwertz\\name\\UnnamedPlayer".into(),
            player_info: PlayerInfo {
                userinfo: "\\asdf\\qwertz\\name\\UnnamedPlayer".into(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.set_name(py, "^1Unnamed^2Player".into()));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_qport_where_no_port_is_set() {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_qport(py), -1);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_qport_for_port_set() {
        let player = Player {
            user_info: "\\qport\\27666".to_string(),
            player_info: PlayerInfo {
                userinfo: "\\qport\\27666".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_qport(py), 27666);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_qport_for_invalid_port_set() {
        let player = Player {
            user_info: "\\qport\\asdf".to_string(),
            player_info: PlayerInfo {
                userinfo: "\\qport\\asdf".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_qport(py), -1);
        });
    }

    #[rstest]
    #[case(team_t::TEAM_FREE, "free")]
    #[case(team_t::TEAM_RED, "red")]
    #[case(team_t::TEAM_BLUE, "blue")]
    #[case(team_t::TEAM_SPECTATOR, "spectator")]
    #[cfg_attr(miri, ignore)]
    fn get_team_for_team_t_values(#[case] team: team_t, #[case] return_value: &str) {
        let player = Player {
            player_info: PlayerInfo {
                team: team as i32,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(
                player.get_team(py).expect("result was not OK"),
                return_value
            )
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_team_for_invalid_team() {
        let player = Player {
            player_info: PlayerInfo {
                team: 42,
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert!(player
                .get_team(py)
                .is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_team_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = default_test_player().set_team(py, "invalid team".into());
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
    fn set_team_puts_player_on_a_specific_team(#[case] new_team: &str) {
        let put_cmd = format!("put 2 {}", new_team.to_lowercase());
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .withf(move |cmd| cmd == put_cmd)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| default_test_player().set_team(py, new_team.into()));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_colors_where_no_colors_are_set() {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_colors(py), (0.0, 0.0));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_colors_for_colors_set() {
        let player = Player {
            user_info: "\\color1\\42\\color2\\21".to_string(),
            player_info: PlayerInfo {
                userinfo: "\\color1\\42\\colors2\\21".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_colors(py), (42.0, 21.0));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_colors_for_invalid_color1_set() {
        let player = Player {
            user_info: "\\color1\\asdf\\color2\\42".to_string(),
            player_info: PlayerInfo {
                userinfo: "\\color1\\asdf\\color2\\42".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_colors(py), (0.0, 42.0));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_colors_for_invalid_color2_set() {
        let player = Player {
            user_info: "\\color1\\42\\color2\\asdf".to_string(),
            player_info: PlayerInfo {
                userinfo: "\\color1\\42\\color2\\asdf".to_string(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_colors(py), (42.0, 0.0));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_colors_updated_client_cvars() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine.expect_get_max_clients().returning(|| 16);
        MAIN_ENGINE.store(Some(mock_engine.into()));

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
                    && cmd == "userinfo \\asdf\\qwertz\\name\\UnnamedPlayer\\color1\\0\\color2\\3"
                    && client_ok
            })
            .times(1);

        let player = Player {
            user_info: "\\asdf\\qwertz\\color1\\7.0\\color2\\5\\name\\UnnamedPlayer".into(),
            player_info: PlayerInfo {
                userinfo: "\\asdf\\qwertz\\color1\\7.0\\color2\\5\\name\\UnnamedPlayer".into(),
                ..default_test_player_info()
            },
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.set_colors(py, (0, 3)));
        assert!(result.is_ok());
    }
}
