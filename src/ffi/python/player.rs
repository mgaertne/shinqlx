use super::prelude::*;
use super::{console_command, owner, CONSOLE_CHANNEL};

use crate::ffi::c::prelude::*;
use crate::quake_live_engine::{GetConfigstring, SetConfigstring};
use crate::MAIN_ENGINE;

use core::sync::atomic::AtomicBool;
use itertools::Itertools;
use std::sync::atomic::Ordering;

use pyo3::{
    basic::CompareOp,
    create_exception,
    exceptions::{
        PyAttributeError, PyEnvironmentError, PyException, PyKeyError, PyNotImplementedError,
        PyValueError,
    },
    types::{IntoPyDict, PyBool, PyDict, PyNotImplemented, PyType},
    BoundObject, IntoPyObject,
};

create_exception!(pyshinqlx_module, NonexistentPlayerError, PyException);

impl TryFrom<&str> for privileges_t {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "none" => Ok(privileges_t::PRIV_NONE),
            "mod" => Ok(privileges_t::PRIV_MOD),
            "admin" => Ok(privileges_t::PRIV_ADMIN),
            _ => Err("Invalid privilege level."),
        }
    }
}

impl TryFrom<&str> for weapon_t {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "g" => Ok(weapon_t::WP_GAUNTLET),
            "mg" => Ok(weapon_t::WP_MACHINEGUN),
            "sg" => Ok(weapon_t::WP_SHOTGUN),
            "gl" => Ok(weapon_t::WP_GRENADE_LAUNCHER),
            "rl" => Ok(weapon_t::WP_ROCKET_LAUNCHER),
            "lg" => Ok(weapon_t::WP_LIGHTNING),
            "rg" => Ok(weapon_t::WP_RAILGUN),
            "pg" => Ok(weapon_t::WP_PLASMAGUN),
            "bfg" => Ok(weapon_t::WP_BFG),
            "gh" => Ok(weapon_t::WP_GRAPPLING_HOOK),
            "ng" => Ok(weapon_t::WP_NAILGUN),
            "pl" => Ok(weapon_t::WP_PROX_LAUNCHER),
            "cg" => Ok(weapon_t::WP_CHAINGUN),
            "hmg" => Ok(weapon_t::WP_HMG),
            "hands" => Ok(weapon_t::WP_HANDS),
            _ => Err("invalid weapon".to_string()),
        }
    }
}

/// A class that represents a player on the server. As opposed to minqlbot,
///    attributes are all the values from when the class was instantiated. This
///    means for instance if a player is on the blue team when you check, but
///    then moves to red, it will still be blue when you check a second time.
///    To update it, use :meth:`~.Player.update`. Note that if you update it
///    and the player has disconnected, it will raise a
///    :exc:`shinqlx.NonexistentPlayerError` exception.
#[pyclass(module = "_player", name = "Player", subclass, frozen)]
#[derive(Debug)]
pub(crate) struct Player {
    pub(crate) valid: AtomicBool,
    #[pyo3(name = "_id", get)]
    pub(crate) id: i32,
    pub(crate) player_info: spin::mutex::FairMutex<PlayerInfo>,
    #[pyo3(name = "_userinfo", get)]
    pub(crate) user_info: String,
    #[pyo3(name = "_steam_id", get)]
    pub(crate) steam_id: i64,
    pub(crate) name: spin::mutex::FairMutex<String>,
}

impl Clone for Player {
    fn clone(&self) -> Self {
        Self {
            valid: self.valid.load(Ordering::SeqCst).into(),
            id: self.id,
            player_info: spin::mutex::FairMutex::from(self.player_info.lock().clone()),
            user_info: self.user_info.clone(),
            steam_id: self.steam_id,
            name: spin::mutex::FairMutex::from(self.name.lock().clone()),
        }
    }
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        self.valid.load(Ordering::SeqCst) == other.valid.load(Ordering::SeqCst)
            && self.id == other.id
            && *self.player_info.lock() == *other.player_info.lock()
            && self.user_info == other.user_info
            && self.steam_id == other.steam_id
            && *self.name.lock() == *other.name.lock()
    }
}

#[pymethods]
impl Player {
    #[new]
    #[pyo3(signature = (client_id, info = None), text_signature = "(client_id, info = None)")]
    pub(crate) fn py_new(client_id: i32, info: Option<PlayerInfo>) -> PyResult<Self> {
        let player_info = info.unwrap_or_else(|| PlayerInfo::from(client_id));

        // When a player connects, the name field in the client struct has yet to be initialized,
        // so we fall back to the userinfo and try parse it ourselves to get the name if needed.
        let name = if player_info.name.is_empty() {
            let cvars = parse_variables(&player_info.userinfo);
            cvars.get("name").unwrap_or_default()
        } else {
            player_info.name.clone()
        };

        Ok(Player {
            valid: true.into(),
            id: client_id,
            user_info: player_info.userinfo.clone(),
            steam_id: player_info.steam_id,
            player_info: spin::mutex::FairMutex::from(player_info),
            name: spin::mutex::FairMutex::from(name),
        })
    }

    fn __repr__(slf: &Bound<'_, Self>) -> String {
        let Ok(classname) = slf.get_type().qualname() else {
            return "NonexistentPlayer".to_string();
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
        self.name.lock().clone()
    }

    fn __contains__(&self, py: Python<'_>, item: &str) -> PyResult<bool> {
        if !self.valid.load(Ordering::SeqCst) {
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            Ok(cvars.get(item).is_some())
        })
    }

    fn __getitem__(&self, py: Python<'_>, item: &str) -> PyResult<String> {
        if !self.valid.load(Ordering::SeqCst) {
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars
                .get(item)
                .map_or_else(|| Err(PyKeyError::new_err(format!("'{item}'"))), Ok)
        })
    }

    fn __richcmp__<'py>(
        &self,
        other: &Bound<'py, PyAny>,
        op: CompareOp,
        py: Python<'py>,
    ) -> PyResult<Borrowed<'py, 'py, PyAny>> {
        match op {
            CompareOp::Eq => {
                if let Ok(other_player) = other.extract::<Self>() {
                    Ok(PyBool::new(py, self.steam_id == other_player.steam_id).into_any())
                } else if let Ok(steam_id) = other.extract::<i64>() {
                    Ok(PyBool::new(py, self.steam_id == steam_id).into_any())
                } else {
                    Ok(PyBool::new(py, false).into_any())
                }
            }
            CompareOp::Ne => {
                if let Ok(other_player) = other.extract::<Self>() {
                    Ok(PyBool::new(py, self.steam_id != other_player.steam_id).into_any())
                } else if let Ok(steam_id) = other.extract::<i64>() {
                    Ok(PyBool::new(py, self.steam_id != steam_id).into_any())
                } else {
                    Ok(PyBool::new(py, true).into_any())
                }
            }
            _ => Ok(PyNotImplemented::get(py).into_any()),
        }
    }

    ///Update the player information with the latest data. If the player
    ///         disconnected it will raise an exception and invalidates a player.
    ///         The player's name and Steam ID can still be accessed after being
    ///         invalidated, but anything else will make it throw an exception too.
    ///
    ///         :raises: shinqlx.NonexistentPlayerError
    fn update(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| *self.player_info.lock() = PlayerInfo::from(self.id));

        if self.player_info.lock().steam_id != self.steam_id {
            self.valid.store(false, Ordering::SeqCst);
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        py.allow_threads(|| {
            let name = if self.player_info.lock().name.is_empty() {
                let cvars = parse_variables(&self.player_info.lock().userinfo);
                cvars.get("name").unwrap_or_default()
            } else {
                self.player_info.lock().name.clone()
            };
            *self.name.lock() = name;
        });

        Ok(())
    }

    #[pyo3(
    name = "_invalidate",
    signature = (e = "The player does not exist anymore. Did the player disconnect?"),
    text_signature = "(e = \"The player does not exist anymore. Did the player disconnect?\")"
    )]
    fn invalidate(&self, e: &str) -> PyResult<()> {
        self.valid.store(false, Ordering::SeqCst);
        Err(NonexistentPlayerError::new_err(e.to_string()))
    }

    #[getter(cvars)]
    fn get_cvars<'a>(&self, py: Python<'a>) -> PyResult<Bound<'a, PyDict>> {
        if !self.valid.load(Ordering::SeqCst) {
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        parse_variables(&self.user_info).into_py_dict(py)
    }

    #[setter(cvars)]
    fn set_cvars(&self, py: Python<'_>, new_cvars: Bound<'_, PyDict>) -> PyResult<()> {
        let new = new_cvars
            .iter()
            .map(|(key, value)| format!(r"\{key}\{value}"))
            .join("");
        let client_command = format!(r#"userinfo "{new}""#);
        pyshinqlx_client_command(py, self.id, &client_command)?;
        Ok(())
    }

    #[getter(steam_id)]
    fn get_steam_id(&self) -> i64 {
        self.steam_id
    }

    #[getter(id)]
    fn get_id(&self) -> i32 {
        self.id
    }

    #[getter(ip)]
    fn get_ip(&self, py: Python<'_>) -> String {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars
                .get("ip")
                .map(|value| value.split(':').next().unwrap_or("").to_string())
                .unwrap_or("".to_string())
        })
    }

    /// The clan tag. Not actually supported by QL, but it used to be and
    /// fortunately the scoreboard still properly displays it if we manually
    /// set the configstring to use clan tags.
    #[getter(clan)]
    fn get_clan(&self, py: Python<'_>) -> String {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return "".to_string();
            };

            let configstring = main_engine.get_configstring(CS_PLAYERS as u16 + self.id as u16);
            let parsed_cs = parse_variables(&configstring);
            parsed_cs.get("cn").unwrap_or("".to_string())
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
            let mut parsed_variables = parse_variables(&configstring);
            parsed_variables.set("xcn", &tag);
            parsed_variables.set("cn", &tag);

            let new_configstring: String = parsed_variables.into();
            main_engine.set_configstring(config_index as i32, &new_configstring);
        })
    }

    #[getter(name)]
    fn get_name(&self, py: Python<'_>) -> String {
        py.allow_threads(|| {
            if self.name.lock().ends_with("^7") {
                self.name.lock().clone()
            } else {
                format!("{}^7", self.name.lock())
            }
        })
    }

    #[setter(name)]
    fn set_name(&self, py: Python<'_>, value: String) -> PyResult<()> {
        let new: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(&self.user_info);
            new_cvars.set("name", &value);
            new_cvars.into()
        });

        let client_command = format!("userinfo \"{new}\"");
        pyshinqlx_client_command(py, self.id, &client_command)?;
        Ok(())
    }

    /// Removes color tags from the name.
    #[getter(clean_name)]
    pub(crate) fn get_clean_name(&self, py: Python<'_>) -> String {
        py.allow_threads(|| clean_text(&(&*self.name.lock())))
    }

    #[getter(qport)]
    fn get_qport(&self, py: Python<'_>) -> i32 {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars
                .get("qport")
                .map(|value| value.parse::<i32>().unwrap_or(-1))
                .unwrap_or(-1)
        })
    }

    #[getter(team)]
    pub(crate) fn get_team(&self, py: Python<'_>) -> PyResult<String> {
        py.allow_threads(|| match team_t::try_from(self.player_info.lock().team) {
            Ok(team_t::TEAM_FREE) => Ok("free".to_string()),
            Ok(team_t::TEAM_RED) => Ok("red".to_string()),
            Ok(team_t::TEAM_BLUE) => Ok("blue".to_string()),
            Ok(team_t::TEAM_SPECTATOR) => Ok("spectator".to_string()),
            _ => Err(PyValueError::new_err("invalid team")),
        })
    }

    #[setter(team)]
    fn set_team(&self, py: Python<'_>, new_team: String) -> PyResult<()> {
        py.allow_threads(|| {
            if !["free", "red", "blue", "spectator"].contains(&&*new_team.to_lowercase()) {
                return Err(PyValueError::new_err("Invalid team."));
            }

            let team_change_cmd = format!("put {} {}", self.id, new_team.to_lowercase());
            console_command(&team_change_cmd)
        })
    }

    #[getter(colors)]
    fn get_colors(&self, py: Python<'_>) -> (f32, f32) {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
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
            let mut new_cvars = parse_variables(&self.player_info.lock().userinfo);
            new_cvars.set("color1", &format!("{}", new.0));
            new_cvars.set("color2", &format!("{}", new.1));
            new_cvars.into()
        });

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(py, self.id, &client_command)?;
        Ok(())
    }

    #[getter(model)]
    fn get_model(&self, py: Python<'_>) -> PyResult<String> {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars
                .get("model")
                .map_or_else(|| Err(PyKeyError::new_err("'model'")), Ok)
        })
    }

    #[setter(model)]
    fn set_model(&self, py: Python<'_>, value: String) -> PyResult<()> {
        let new_cvars_string: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(&self.player_info.lock().userinfo);
            new_cvars.set("model", &value);
            new_cvars.into()
        });

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(py, self.id, &client_command)?;
        Ok(())
    }

    #[getter(headmodel)]
    fn get_headmodel(&self, py: Python<'_>) -> PyResult<String> {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars
                .get("headmodel")
                .map_or_else(|| Err(PyKeyError::new_err("'headmodel'")), Ok)
        })
    }

    #[setter(headmodel)]
    fn set_headmodel(&self, py: Python<'_>, value: String) -> PyResult<()> {
        let new_cvars_string: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(&self.player_info.lock().userinfo);
            new_cvars.set("headmodel", &value);
            new_cvars.into()
        });

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(py, self.id, &client_command)?;
        Ok(())
    }

    #[getter(handicap)]
    fn get_handicap(&self, py: Python<'_>) -> PyResult<String> {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars
                .get("handicap")
                .map_or_else(|| Err(PyKeyError::new_err("'handicap'")), Ok)
        })
    }

    #[setter(handicap)]
    fn set_handicap(&self, py: Python<'_>, value: Bound<'_, PyAny>) -> PyResult<()> {
        let new_handicap = value.str()?.to_string();
        if new_handicap.parse::<i32>().is_err() {
            let error_msg = format!("invalid literal for int() with base 10: '{new_handicap}'");
            return Err(PyValueError::new_err(error_msg));
        }

        let new_cvars_string: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(&self.player_info.lock().userinfo);
            new_cvars.set("handicap", &new_handicap);
            new_cvars.into()
        });

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(py, self.id, &client_command).map(|_| ())
    }

    #[getter(autohop)]
    fn get_autohop(&self, py: Python<'_>) -> PyResult<i32> {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars.get("autohop").map_or_else(
                || Err(PyKeyError::new_err("'autohop'")),
                |value| {
                    value.parse::<i32>().map_err(|_| {
                        let error_msg =
                            format!("invalid literal for int() with base 10: '{value}'");
                        PyValueError::new_err(error_msg)
                    })
                },
            )
        })
    }

    #[setter(autohop)]
    fn set_autohop(&self, py: Python<'_>, value: Bound<'_, PyAny>) -> PyResult<()> {
        let new_autohop = value.str()?.to_string();
        if new_autohop.parse::<i32>().is_err() {
            let error_msg = format!("invalid literal for int() with base 10: '{new_autohop}'");
            return Err(PyValueError::new_err(error_msg));
        }

        let new_cvars_string: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(&self.player_info.lock().userinfo);
            new_cvars.set("autohop", &new_autohop);
            new_cvars.into()
        });

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(py, self.id, &client_command).map(|_| ())
    }

    #[getter(autoaction)]
    fn get_autoaction(&self, py: Python<'_>) -> PyResult<i32> {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars.get("autoaction").map_or_else(
                || Err(PyKeyError::new_err("'autoaction'")),
                |value| {
                    value.parse::<i32>().map_err(|_| {
                        let error_msg =
                            format!("invalid literal for int() with base 10: '{value}'");
                        PyValueError::new_err(error_msg)
                    })
                },
            )
        })
    }

    #[setter(autoaction)]
    fn set_autoaction(&self, py: Python<'_>, value: Bound<'_, PyAny>) -> PyResult<()> {
        let new_autoaction = value.str()?.to_string();
        if new_autoaction.parse::<i32>().is_err() {
            let error_msg = format!("invalid literal for int() with base 10: '{new_autoaction}'");
            return Err(PyValueError::new_err(error_msg));
        }

        let new_cvars_string: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(&self.player_info.lock().userinfo);
            new_cvars.set("autoaction", &new_autoaction);
            new_cvars.into()
        });

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(py, self.id, &client_command).map(|_| ())
    }

    #[getter(predictitems)]
    fn get_predictitems(&self, py: Python<'_>) -> PyResult<i32> {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars.get("cg_predictitems").map_or_else(
                || Err(PyKeyError::new_err("'cg_predictitems'")),
                |value| {
                    value.parse::<i32>().map_err(|_| {
                        let error_msg =
                            format!("invalid literal for int() with base 10: '{value}'");
                        PyValueError::new_err(error_msg)
                    })
                },
            )
        })
    }

    #[setter(predictitems)]
    fn set_predictitems(&self, py: Python<'_>, value: Bound<'_, PyAny>) -> PyResult<()> {
        let new_predictitems = value.str()?.to_string();
        if new_predictitems.parse::<i32>().is_err() {
            let error_msg = format!("invalid literal for int() with base 10: '{new_predictitems}'");
            return Err(PyValueError::new_err(error_msg));
        }

        let new_cvars_string: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(&self.player_info.lock().userinfo);
            new_cvars.set("cg_predictitems", &new_predictitems);
            new_cvars.into()
        });

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(py, self.id, &client_command).map(|_| ())
    }

    /// A string describing the connection state of a player.
    ///
    /// Possible values:
    ///   - *free* -- The player has disconnected and the slot is free to be used by someone else.
    ///   - *zombie* -- The player disconnected and his/her slot will be available to other players shortly.
    ///   - *connected* -- The player connected, but is currently loading the game.
    ///   - *primed* -- The player was sent the necessary information to play, but has yet to send commands.
    ///   - *active* -- The player finished loading and is actively sending commands to the server.
    ///
    /// In other words, if you need to make sure a player is in-game, check if ``player.connection_state == "active"``.
    #[getter(connection_state)]
    fn get_connection_state(&self, py: Python<'_>) -> PyResult<String> {
        py.allow_threads(|| {
            match clientState_t::try_from(self.player_info.lock().connection_state) {
                Ok(clientState_t::CS_FREE) => Ok("free".to_string()),
                Ok(clientState_t::CS_ZOMBIE) => Ok("zombie".to_string()),
                Ok(clientState_t::CS_CONNECTED) => Ok("connected".to_string()),
                Ok(clientState_t::CS_PRIMED) => Ok("primed".to_string()),
                Ok(clientState_t::CS_ACTIVE) => Ok("active".to_string()),
                _ => Err(PyValueError::new_err("invalid clientState")),
            }
        })
    }

    #[getter(state)]
    fn get_state(&self, py: Python<'_>) -> PyResult<Option<PlayerState>> {
        pyshinqlx_player_state(py, self.id)
    }

    #[getter(privileges)]
    fn get_privileges(&self, py: Python<'_>) -> Option<String> {
        py.allow_threads(
            || match privileges_t::from(self.player_info.lock().privileges) {
                privileges_t::PRIV_MOD => Some("mod".to_string()),
                privileges_t::PRIV_ADMIN => Some("admin".to_string()),
                privileges_t::PRIV_ROOT => Some("root".to_string()),
                privileges_t::PRIV_BANNED => Some("banned".to_string()),
                _ => None,
            },
        )
    }

    #[setter(privileges)]
    fn set_privileges(&self, py: Python<'_>, value: Option<String>) -> PyResult<()> {
        let new_privileges = py
            .allow_threads(|| privileges_t::try_from(value.unwrap_or("none".to_string()).as_str()));

        new_privileges.map_or(
            Err(PyValueError::new_err("Invalid privilege level.")),
            |new_privilege| {
                pyshinqlx_set_privileges(py, self.id, new_privilege as i32)?;
                Ok(())
            },
        )
    }

    #[getter(country)]
    fn get_country(&self, py: Python<'_>) -> PyResult<String> {
        py.allow_threads(|| {
            let cvars = parse_variables(&self.user_info);
            cvars
                .get("country")
                .map_or_else(|| Err(PyKeyError::new_err("'country'")), Ok)
        })
    }

    #[setter(country)]
    fn set_country(&self, py: Python<'_>, value: String) -> PyResult<()> {
        let new_cvars_string: String = py.allow_threads(|| {
            let mut new_cvars = parse_variables(&self.player_info.lock().userinfo.clone());
            new_cvars.set("country", &value);
            new_cvars.into()
        });

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(py, self.id, &client_command)?;
        Ok(())
    }

    #[getter(_valid)]
    fn get_valid(&self, py: Python<'_>) -> bool {
        py.allow_threads(|| self.valid.load(Ordering::SeqCst))
    }

    #[getter(stats)]
    fn get_stats(&self, py: Python<'_>) -> PyResult<Option<PlayerStats>> {
        pyshinqlx_player_stats(py, self.id)
    }

    #[getter(ping)]
    fn get_ping(&self, py: Python<'_>) -> PyResult<i32> {
        pyshinqlx_player_stats(py, self.id)
            .map(|opt_stats| opt_stats.map(|stats| stats.ping).unwrap_or(999))
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn position<'py>(
        &self,
        py: Python<'py>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let pos = if reset {
            Vector3(0, 0, 0)
        } else {
            match pyshinqlx_player_state(py, self.id)? {
                None => Vector3(0, 0, 0),
                Some(state) => state.position,
            }
        };

        match kwargs {
            None => Ok(pos.into_pyobject(py)?.into_any()),
            Some(py_kwargs) => {
                let x = match py_kwargs.get_item("x")? {
                    None => pos.0,
                    Some(value) => value.extract::<i32>()?,
                };
                let y = match py_kwargs.get_item("y")? {
                    None => pos.1,
                    Some(value) => value.extract::<i32>()?,
                };
                let z = match py_kwargs.get_item("z")? {
                    None => pos.2,
                    Some(value) => value.extract::<i32>()?,
                };

                let vector = Vector3(x, y, z);

                pyshinqlx_set_position(py, self.id, &vector)
                    .map(|value| PyBool::new(py, value).into_any().to_owned())
            }
        }
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn velocity<'py>(
        &self,
        py: Python<'py>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let vel = if reset {
            Vector3(0, 0, 0)
        } else {
            match pyshinqlx_player_state(py, self.id)? {
                None => Vector3(0, 0, 0),
                Some(state) => state.velocity,
            }
        };

        match kwargs {
            None => Ok(vel.into_pyobject(py)?.into_any()),
            Some(py_kwargs) => {
                let x = match py_kwargs.get_item("x")? {
                    None => vel.0,
                    Some(value) => value.extract::<i32>()?,
                };
                let y = match py_kwargs.get_item("y")? {
                    None => vel.1,
                    Some(value) => value.extract::<i32>()?,
                };
                let z = match py_kwargs.get_item("z")? {
                    None => vel.2,
                    Some(value) => value.extract::<i32>()?,
                };

                let vector = Vector3(x, y, z);

                pyshinqlx_set_velocity(py, self.id, &vector)
                    .map(|value| PyBool::new(py, value).into_any().to_owned())
            }
        }
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn weapons<'py>(
        &self,
        py: Python<'py>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let weaps = if reset {
            Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        } else {
            match pyshinqlx_player_state(py, self.id)? {
                None => Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                Some(state) => state.weapons,
            }
        };

        match kwargs {
            None => Ok(weaps.into_pyobject(py)?.into_any()),
            Some(py_kwargs) => {
                let g = match py_kwargs.get_item("g")? {
                    None => weaps.0,
                    Some(value) => value.extract::<i32>()?,
                };
                let mg = match py_kwargs.get_item("mg")? {
                    None => weaps.1,
                    Some(value) => value.extract::<i32>()?,
                };
                let sg = match py_kwargs.get_item("sg")? {
                    None => weaps.2,
                    Some(value) => value.extract::<i32>()?,
                };
                let gl = match py_kwargs.get_item("gl")? {
                    None => weaps.3,
                    Some(value) => value.extract::<i32>()?,
                };
                let rl = match py_kwargs.get_item("rl")? {
                    None => weaps.4,
                    Some(value) => value.extract::<i32>()?,
                };
                let lg = match py_kwargs.get_item("lg")? {
                    None => weaps.5,
                    Some(value) => value.extract::<i32>()?,
                };
                let rg = match py_kwargs.get_item("rg")? {
                    None => weaps.6,
                    Some(value) => value.extract::<i32>()?,
                };
                let pg = match py_kwargs.get_item("pg")? {
                    None => weaps.7,
                    Some(value) => value.extract::<i32>()?,
                };
                let bfg = match py_kwargs.get_item("bfg")? {
                    None => weaps.8,
                    Some(value) => value.extract::<i32>()?,
                };
                let gh = match py_kwargs.get_item("gh")? {
                    None => weaps.9,
                    Some(value) => value.extract::<i32>()?,
                };
                let ng = match py_kwargs.get_item("ng")? {
                    None => weaps.10,
                    Some(value) => value.extract::<i32>()?,
                };
                let pl = match py_kwargs.get_item("pl")? {
                    None => weaps.11,
                    Some(value) => value.extract::<i32>()?,
                };
                let cg = match py_kwargs.get_item("cg")? {
                    None => weaps.12,
                    Some(value) => value.extract::<i32>()?,
                };
                let hmg = match py_kwargs.get_item("hmg")? {
                    None => weaps.13,
                    Some(value) => value.extract::<i32>()?,
                };
                let hands = match py_kwargs.get_item("hands")? {
                    None => weaps.14,
                    Some(value) => value.extract::<i32>()?,
                };

                let weapons = Weapons(
                    g, mg, sg, gl, rl, lg, rg, pg, bfg, gh, ng, pl, cg, hmg, hands,
                );

                pyshinqlx_set_weapons(py, self.id, &weapons)
                    .map(|value| PyBool::new(py, value).into_any().to_owned())
            }
        }
    }

    #[pyo3(signature = (new_weapon = None), text_signature = "(new_weapon = None)")]
    fn weapon<'py>(
        &self,
        py: Python<'py>,
        new_weapon: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let Some(weapon) = new_weapon else {
            let weapon = match pyshinqlx_player_state(py, self.id)? {
                None => weapon_t::WP_HANDS as i32,
                Some(state) => state.weapon,
            };

            return Ok(weapon.into_pyobject(py)?.into_any());
        };

        let Ok(converted_weapon) = (match weapon.extract::<i32>() {
            Ok(value) => weapon_t::try_from(value),
            Err(_) => match weapon.extract::<String>() {
                Ok(value) => weapon_t::try_from(value.as_str()),
                Err(_) => Err("invalid weapon".to_string()),
            },
        }) else {
            return Err(PyValueError::new_err("invalid new_weapon"));
        };

        pyshinqlx_set_weapon(py, self.id, converted_weapon as i32)
            .map(|value| PyBool::new(py, value).into_any().to_owned())
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn ammo<'py>(
        &self,
        py: Python<'py>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let ammos = if reset {
            Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        } else {
            match pyshinqlx_player_state(py, self.id)? {
                None => Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                Some(state) => state.ammo,
            }
        };

        match kwargs {
            None => Ok(ammos.into_pyobject(py)?.into_any()),
            Some(py_kwargs) => {
                let g = match py_kwargs.get_item("g")? {
                    None => ammos.0,
                    Some(value) => value.extract::<i32>()?,
                };
                let mg = match py_kwargs.get_item("mg")? {
                    None => ammos.1,
                    Some(value) => value.extract::<i32>()?,
                };
                let sg = match py_kwargs.get_item("sg")? {
                    None => ammos.2,
                    Some(value) => value.extract::<i32>()?,
                };
                let gl = match py_kwargs.get_item("gl")? {
                    None => ammos.3,
                    Some(value) => value.extract::<i32>()?,
                };
                let rl = match py_kwargs.get_item("rl")? {
                    None => ammos.4,
                    Some(value) => value.extract::<i32>()?,
                };
                let lg = match py_kwargs.get_item("lg")? {
                    None => ammos.5,
                    Some(value) => value.extract::<i32>()?,
                };
                let rg = match py_kwargs.get_item("rg")? {
                    None => ammos.6,
                    Some(value) => value.extract::<i32>()?,
                };
                let pg = match py_kwargs.get_item("pg")? {
                    None => ammos.7,
                    Some(value) => value.extract::<i32>()?,
                };
                let bfg = match py_kwargs.get_item("bfg")? {
                    None => ammos.8,
                    Some(value) => value.extract::<i32>()?,
                };
                let gh = match py_kwargs.get_item("gh")? {
                    None => ammos.9,
                    Some(value) => value.extract::<i32>()?,
                };
                let ng = match py_kwargs.get_item("ng")? {
                    None => ammos.10,
                    Some(value) => value.extract::<i32>()?,
                };
                let pl = match py_kwargs.get_item("pl")? {
                    None => ammos.11,
                    Some(value) => value.extract::<i32>()?,
                };
                let cg = match py_kwargs.get_item("cg")? {
                    None => ammos.12,
                    Some(value) => value.extract::<i32>()?,
                };
                let hmg = match py_kwargs.get_item("hmg")? {
                    None => ammos.13,
                    Some(value) => value.extract::<i32>()?,
                };
                let hands = match py_kwargs.get_item("hands")? {
                    None => ammos.14,
                    Some(value) => value.extract::<i32>()?,
                };

                let weapons = Weapons(
                    g, mg, sg, gl, rl, lg, rg, pg, bfg, gh, ng, pl, cg, hmg, hands,
                );

                pyshinqlx_set_ammo(py, self.id, &weapons)
                    .map(|value| PyBool::new(py, value).into_any().to_owned())
            }
        }
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn powerups<'py>(
        &self,
        py: Python<'py>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let powerups = if reset {
            Powerups(0, 0, 0, 0, 0, 0)
        } else {
            match pyshinqlx_player_state(py, self.id)? {
                None => Powerups(0, 0, 0, 0, 0, 0),
                Some(state) => state.powerups,
            }
        };

        match kwargs {
            None => Ok(powerups.into_pyobject(py)?.into_any()),
            Some(py_kwargs) => {
                let quad = match py_kwargs.get_item("quad")? {
                    None => powerups.0,
                    Some(value) => (value.extract::<f32>()? * 1000.0).floor() as i32,
                };
                let bs = match py_kwargs.get_item("battlesuit")? {
                    None => powerups.1,
                    Some(value) => (value.extract::<f32>()? * 1000.0).floor() as i32,
                };
                let haste = match py_kwargs.get_item("haste")? {
                    None => powerups.2,
                    Some(value) => (value.extract::<f32>()? * 1000.0).floor() as i32,
                };
                let invis = match py_kwargs.get_item("invisibility")? {
                    None => powerups.3,
                    Some(value) => (value.extract::<f32>()? * 1000.0).floor() as i32,
                };
                let regen = match py_kwargs.get_item("regeneration")? {
                    None => powerups.4,
                    Some(value) => (value.extract::<f32>()? * 1000.0).floor() as i32,
                };
                let invul = match py_kwargs.get_item("invulnerability")? {
                    None => powerups.5,
                    Some(value) => (value.extract::<f32>()? * 1000.0).floor() as i32,
                };

                let powerups = Powerups(quad, bs, haste, invis, regen, invul);

                pyshinqlx_set_powerups(py, self.id, &powerups)
                    .map(|value| PyBool::new(py, value).into_any().to_owned())
            }
        }
    }

    #[getter(holdable)]
    fn get_holdable(&self, py: Python<'_>) -> PyResult<Option<String>> {
        pyshinqlx_player_state(py, self.id).map(|opt_state| {
            opt_state
                .filter(|state| state.holdable != Holdable::None)
                .map(|state| state.holdable.to_string())
        })
    }

    #[setter(holdable)]
    fn set_holdable(&self, py: Python<'_>, holdable: Option<String>) -> PyResult<()> {
        match Holdable::from(holdable) {
            Holdable::Unknown => Err(PyValueError::new_err("Invalid holdable item.")),
            value => {
                pyshinqlx_set_holdable(py, self.id, value.into())?;
                if value == Holdable::Flight {
                    let flight = Flight(16000, 16000, 1200, 0);
                    pyshinqlx_set_flight(py, self.id, &flight)?;
                }
                Ok(())
            }
        }
    }

    fn drop_holdable(&self, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_drop_holdable(py, self.id)?;
        Ok(())
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn flight<'py>(
        &self,
        py: Python<'py>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let opt_state = pyshinqlx_player_state(py, self.id)?;
        let init_flight = if !opt_state
            .as_ref()
            .is_some_and(|state| state.holdable == Holdable::Flight)
        {
            self.set_holdable(py, Some("flight".to_string()))?;
            true
        } else {
            reset
        };

        let flight = if init_flight {
            Flight(16_000, 16_000, 1_200, 0)
        } else {
            match opt_state {
                None => Flight(16_000, 16_000, 1_200, 0),
                Some(state) => state.flight,
            }
        };

        match kwargs {
            None => Ok(flight.into_pyobject(py)?.into_any()),
            Some(py_kwargs) => {
                let fuel = match py_kwargs.get_item("fuel")? {
                    None => flight.0,
                    Some(value) => value.extract::<i32>()?,
                };
                let max_fuel = match py_kwargs.get_item("max_fuel")? {
                    None => flight.1,
                    Some(value) => value.extract::<i32>()?,
                };
                let thrust = match py_kwargs.get_item("thrust")? {
                    None => flight.2,
                    Some(value) => value.extract::<i32>()?,
                };
                let refuel = match py_kwargs.get_item("refuel")? {
                    None => flight.3,
                    Some(value) => value.extract::<i32>()?,
                };

                let flight = Flight(fuel, max_fuel, thrust, refuel);

                pyshinqlx_set_flight(py, self.id, &flight)
                    .map(|value| PyBool::new(py, value).into_any().to_owned())
            }
        }
    }

    #[getter(noclip)]
    fn get_noclip(&self, py: Python<'_>) -> PyResult<bool> {
        pyshinqlx_player_state(py, self.id)
            .map(|opt_state| opt_state.map(|state| state.noclip).unwrap_or(false))
    }

    #[setter(noclip)]
    fn set_noclip(&self, py: Python<'_>, value: Bound<'_, PyAny>) -> PyResult<()> {
        let noclip_value = match value.extract::<bool>() {
            Ok(value) => value,
            Err(_) => match value.extract::<i128>() {
                Ok(value) => value != 0,
                Err(_) => match value.extract::<String>() {
                    Ok(value) => !value.is_empty(),
                    Err(_) => !value.is_none(),
                },
            },
        };
        pyshinqlx_noclip(py, self.id, noclip_value).map(|_| ())
    }

    #[getter(health)]
    fn get_health(&self, py: Python<'_>) -> PyResult<i32> {
        pyshinqlx_player_state(py, self.id)
            .map(|opt_state| opt_state.map(|state| state.health).unwrap_or(0))
    }

    #[setter(health)]
    fn set_health(&self, py: Python<'_>, value: i32) -> PyResult<()> {
        pyshinqlx_set_health(py, self.id, value)?;
        Ok(())
    }

    #[getter(armor)]
    fn get_armor(&self, py: Python<'_>) -> PyResult<i32> {
        pyshinqlx_player_state(py, self.id)
            .map(|opt_state| opt_state.map(|state| state.armor).unwrap_or(0))
    }

    #[setter(armor)]
    fn set_armor(&self, py: Python<'_>, value: i32) -> PyResult<()> {
        pyshinqlx_set_armor(py, self.id, value)?;
        Ok(())
    }

    #[getter(is_alive)]
    fn get_is_alive(&self, py: Python<'_>) -> PyResult<bool> {
        pyshinqlx_player_state(py, self.id)
            .map(|opt_state| opt_state.map(|state| state.is_alive).unwrap_or(false))
    }

    #[setter(is_alive)]
    fn set_is_alive(&self, py: Python<'_>, value: bool) -> PyResult<()> {
        let current = self.get_is_alive(py)?;

        if !current && value {
            pyshinqlx_player_spawn(py, self.id)?;
        }

        if current && !value {
            // TODO: Proper death and not just setting health to 0.
            self.set_health(py, 0)?;
        }
        Ok(())
    }

    #[getter(is_frozen)]
    fn get_is_frozen(&self, py: Python<'_>) -> PyResult<bool> {
        pyshinqlx_player_state(py, self.id)
            .map(|opt_state| opt_state.map(|state| state.is_frozen).unwrap_or(false))
    }

    #[getter(is_chatting)]
    fn get_is_chatting(&self, py: Python<'_>) -> PyResult<bool> {
        pyshinqlx_player_state(py, self.id)
            .map(|opt_state| opt_state.map(|state| state.is_chatting).unwrap_or(false))
    }

    #[getter(score)]
    fn get_score(&self, py: Python<'_>) -> PyResult<i32> {
        pyshinqlx_player_stats(py, self.id)
            .map(|opt_stats| opt_stats.map(|stats| stats.score).unwrap_or(0))
    }

    #[setter(score)]
    fn set_score(&self, py: Python<'_>, value: i32) -> PyResult<()> {
        pyshinqlx_set_score(py, self.id, value)?;
        Ok(())
    }

    #[getter(channel)]
    fn get_channel(&self, py: Python<'_>) -> Option<Py<TellChannel>> {
        Py::new(py, TellChannel::py_new(self)).ok()
    }

    fn center_print(&self, py: Python<'_>, msg: &str) -> PyResult<()> {
        let cmd = format!(r#"cp "{msg}""#);
        pyshinqlx_send_server_command(py, Some(self.id), &cmd).map(|_| ())
    }

    #[pyo3(signature = (msg, **kwargs))]
    fn tell<'py>(
        &self,
        py: Python<'py>,
        msg: &str,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<()> {
        self.get_channel(py).map_or(
            Err(PyNotImplementedError::new_err("Player TellChannel")),
            |tell_channel| {
                let limit = kwargs
                    .and_then(|pydict| {
                        pydict
                            .get_item("limit")
                            .ok()
                            .flatten()
                            .and_then(|value| value.extract::<i32>().ok())
                    })
                    .unwrap_or(100i32);

                let delimiter = kwargs
                    .and_then(|pydict| {
                        pydict
                            .get_item("delimiter")
                            .ok()
                            .flatten()
                            .and_then(|value| value.extract::<String>().ok())
                    })
                    .unwrap_or(" ".to_owned());

                tell_channel
                    .bind(py)
                    .as_super()
                    .reply(msg, limit, &delimiter)
            },
        )
    }

    #[pyo3(signature = (reason = ""), text_signature = "(reason = \"\")")]
    fn kick(&self, py: Python<'_>, reason: &str) -> PyResult<()> {
        pyshinqlx_kick(py, self.id, Some(reason))
    }

    fn ban(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| {
            let ban_cmd = format!("ban {}", self.id);
            console_command(&ban_cmd)
        })
    }

    fn tempban(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| {
            let tempban_cmd = format!("tempban {}", self.id);
            console_command(&tempban_cmd)
        })
    }

    fn addadmin(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| {
            let addadmin_cmd = format!("addadmin {}", self.id);
            console_command(&addadmin_cmd)
        })
    }

    fn addmod(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| {
            let addmod_cmd = format!("addmod {}", self.id);
            console_command(&addmod_cmd)
        })
    }

    fn demote(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| {
            let demote_cmd = format!("demote {}", self.id);
            console_command(&demote_cmd)
        })
    }

    fn mute(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| {
            let mute_cmd = format!("mute {}", self.id);
            console_command(&mute_cmd)
        })
    }

    fn unmute(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| {
            let unmute_cmd = format!("unmute {}", self.id);
            console_command(&unmute_cmd)
        })
    }

    pub(crate) fn put(&self, py: Python<'_>, team: &str) -> PyResult<()> {
        py.allow_threads(|| {
            if !["free", "red", "blue", "spectator"].contains(&&*team.to_lowercase()) {
                return Err(PyValueError::new_err("Invalid team."));
            }

            let team_change_cmd = format!("put {} {}", self.id, team.to_lowercase());
            console_command(&team_change_cmd)
        })
    }

    fn addscore(&self, py: Python<'_>, score: i32) -> PyResult<()> {
        py.allow_threads(|| {
            let addscore_cmd = format!("addscore {} {}", self.id, score);
            console_command(&addscore_cmd)
        })
    }

    fn switch(&self, py: Python<'_>, other_player: Player) -> PyResult<()> {
        let own_team = self.get_team(py)?;
        let other_team = other_player.get_team(py)?;

        if own_team == other_team {
            return Err(PyValueError::new_err("Both players are on the same team."));
        }

        self.put(py, &other_team)?;
        other_player.put(py, &own_team)
    }

    #[pyo3(signature = (damage = 0), text_signature = "(damage = 0)")]
    fn slap(&self, py: Python<'_>, damage: i32) -> PyResult<()> {
        py.allow_threads(|| {
            let slap_cmd = format!("slap {} {}", self.id, damage);
            console_command(&slap_cmd)
        })
    }

    fn slay(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| {
            let slay_cmd = format!("slay {}", self.id);
            console_command(&slay_cmd)
        })
    }

    fn slay_with_mod(&self, py: Python<'_>, means_of_death: i32) -> PyResult<()> {
        pyshinqlx_slay_with_mod(py, self.id, means_of_death).map(|_| ())
    }

    #[classmethod]
    pub(crate) fn all_players(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<Vec<Player>> {
        let players_info = pyshinqlx_players_info(py)?;
        py.allow_threads(|| {
            Ok(players_info
                .iter()
                .filter_map(|opt_player_info| {
                    opt_player_info.as_ref().map(|player_info| Player {
                        valid: true.into(),
                        id: player_info.client_id,
                        user_info: player_info.userinfo.clone(),
                        steam_id: player_info.steam_id,
                        name: spin::mutex::FairMutex::new(player_info.name.clone()),
                        player_info: spin::mutex::FairMutex::new(player_info.clone()),
                    })
                })
                .collect())
        })
    }
}

#[cfg(test)]
mod pyshinqlx_player_tests {
    use super::NonexistentPlayerError;
    use crate::ffi::c::prelude::*;
    use crate::ffi::python::prelude::*;
    use crate::ffi::python::pyshinqlx_test_support::*;
    use crate::hooks::mock_hooks::{
        shinqlx_client_spawn_context, shinqlx_drop_client_context,
        shinqlx_execute_client_command_context, shinqlx_send_server_command_context,
    };
    use crate::prelude::*;
    use std::sync::atomic::Ordering;

    use mockall::{predicate, Sequence};
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use pyo3::{
        exceptions::{PyEnvironmentError, PyKeyError, PyTypeError, PyValueError},
        types::{IntoPyDict, PyBool, PyString},
        IntoPyObject,
    };

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
        let result = Python::with_gil(|py| {
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
        assert_eq!(player.__str__(), "^1Unnamed^2Player");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn contains_with_invalid_player(_pyshinqlx_setup: ()) {
        let player = Player {
            valid: false.into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.__contains__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn contains_where_value_is_part_of_userinfo(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                userinfo: r"\asdf\some value".to_string(),
                ..default_test_player_info()
            }
            .into(),
            user_info: r"\asdf\some value".to_string(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.__contains__(py, "asdf"));
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn contains_where_value_is_not_in_userinfo(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                userinfo: r"\name\^1Unnamed^2Player".to_string(),
                ..default_test_player_info()
            }
            .into(),
            user_info: r"\name\^1Unnamed^2Player".to_string(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.__contains__(py, "asdf"));
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn getitem_with_invalid_player(_pyshinqlx_setup: ()) {
        let player = Player {
            valid: false.into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.__getitem__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn getitem_where_value_is_part_of_userinfo(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                userinfo: r"\asdf\some value".to_string(),
                ..default_test_player_info()
            }
            .into(),
            user_info: r"\asdf\some value".to_string(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.__getitem__(py, "asdf"));
        assert_eq!(result.expect("result was not OK"), "some value");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn getitem_where_value_is_not_in_userinfo(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                userinfo: r"\name\^1Unnamed^2Player".to_string(),
                ..default_test_player_info()
            }
            .into(),
            user_info: r"\name\^1Unnamed^2Player".to_string(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.__getitem__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)))
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn cvars_with_invalid_player(_pyshinqlx_setup: ()) {
        let player = Player {
            valid: false.into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_cvars(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn cvars_where_value_is_part_of_userinfo(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                userinfo: r"\asdf\some value".to_string(),
                ..default_test_player_info()
            }
            .into(),
            user_info: r"\asdf\some value".to_string(),
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
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert(shinqlx.Player(42, player_info) == shinqlx.Player(42, player_info))
assert((shinqlx.Player(42, player_info) == shinqlx.Player(41, player_info2)) == False)
            "#,
                None,
                Some(
                    &[
                        ("player_info", player_info.into_pyobject(py)?),
                        ("player_info2", player_info2.into_pyobject(py)?),
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
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert(shinqlx.Player(42, player_info) == 1234567890)
assert((shinqlx.Player(42, player_info) == 1234567891) == False)
assert((shinqlx.Player(42, player_info) == "asdf") == False)
            "#,
                None,
                Some(&[("player_info", player_info.into_pyobject(py)?)].into_py_dict(py)?),
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
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert((shinqlx.Player(42, player_info) != shinqlx.Player(42, player_info)) == False)
assert(shinqlx.Player(42, player_info) != shinqlx.Player(41, player_info2))
            "#,
                None,
                Some(
                    &[
                        ("player_info", player_info.into_pyobject(py)?),
                        ("player_info2", player_info2.into_pyobject(py)?),
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
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert((shinqlx.Player(42, player_info) != 1234567890) == False)
assert(shinqlx.Player(42, player_info) != 1234567891)
assert(shinqlx.Player(42, player_info) != "asdf")
            "#,
                None,
                Some(&[("player_info", player_info.into_pyobject(py)?)].into_py_dict(py)?),
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

        Python::with_gil(|py| {
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
                                .into_pyobject(py)
                                .expect("this should not happen"),
                        ),
                        (
                            "player_info2",
                            player_info2
                                .into_pyobject(py)
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

        let player = Player {
            steam_id: 1234567890,
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.update(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py)));
        });
        assert_eq!(player.valid.load(Ordering::SeqCst), false);
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn update_can_be_called_from_python(_pyshinqlx_setup: ()) {
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
                cr#"
player.update()
assert(player._valid)
            "#,
                None,
                Some(&[("player", player.into_pyobject(py)?)].into_py_dict(py)?),
            )
        });
        assert!(result.as_ref().is_ok(), "{:?}", result.as_ref());
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn update_updates_new_player_name(_pyshinqlx_setup: ()) {
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

        let player = Player {
            steam_id: 1234567890,
            ..default_test_player()
        };

        Python::with_gil(|py| player.update(py).unwrap());
        assert_eq!(player.valid.load(Ordering::SeqCst), true);
        assert_eq!(&*player.name.lock(), "NewUnnamedPlayer");
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn update_updates_new_player_name_from_userinfo(_pyshinqlx_setup: ()) {
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
                    .return_const(r"\name\NewUnnamedPlayer");
                mock_client
                    .expect_get_steam_id()
                    .return_const(1234567890u64);
                mock_client
            });

        let player = Player {
            steam_id: 1234567890,
            ..default_test_player()
        };

        Python::with_gil(|py| player.update(py).unwrap());
        assert_eq!(player.valid.load(Ordering::SeqCst), true);
        assert_eq!(&*player.name.lock(), "NewUnnamedPlayer");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn invalidate_invalidates_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();
        let result = player.invalidate("invalid player");
        assert_eq!(player.valid.load(Ordering::SeqCst), false);
        Python::with_gil(|py| {
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
                client.is_some()
                    && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer""#
                    && client_ok
            })
            .times(1);

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                player.set_cvars(
                    py,
                    [("asdf", "qwertz"), ("name", "UnnamedPlayer")]
                        .into_py_dict(py)
                        .expect("this should not happen"),
                )
            });
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_ip_where_no_ip_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        assert_eq!(Python::with_gil(|py| player.get_ip(py)), "");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_ip_for_ip_with_no_port(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\ip\127.0.0.1".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\ip\127.0.0.1".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        assert_eq!(Python::with_gil(|py| player.get_ip(py)), "127.0.0.1");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_ip_for_ip_with_port(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\ip\127.0.0.1:27666".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\ip\127.0.0.1:27666".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        assert_eq!(Python::with_gil(|py| player.get_ip(py)), "127.0.0.1");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_clan_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();
        let result = Python::with_gil(|py| player.get_clan(py));
        assert_eq!(result, "");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_clan_with_no_clan_set(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_get_configstring((CS_PLAYERS + 2) as u16, "", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.get_clan(py));
                assert_eq!(result, "");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_clan_with_clan_set(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_get_configstring((CS_PLAYERS + 2) as u16, r"\cn\asdf", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.get_clan(py));
                assert_eq!(result, "asdf");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_clan_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();
        Python::with_gil(|py| player.set_clan(py, "asdf".to_string()));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_clan_with_no_clan_set(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_get_configstring((CS_PLAYERS + 2) as u16, "", 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .withf(|index, value| {
                        *index == 531i32
                            && value.contains(r"\cn\clan")
                            && value.contains(r"\xcn\clan")
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| player.set_clan(py, "clan".to_string()));
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_clan_with_clan_set(_pyshinqlx_setup: ()) {
        let player = default_test_player();

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
                Python::with_gil(|py| player.set_clan(py, "clan".to_string()));
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_name_for_color_terminated_name(_pyshinqlx_setup: ()) {
        let player = Player {
            name: "UnnamedPlayer^7".to_string().into(),
            player_info: PlayerInfo {
                name: "UnnamedPlayer^7".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        assert_eq!(
            Python::with_gil(|py| player.get_name(py)),
            "UnnamedPlayer^7"
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_name_for_color_unterminated_name(_pyshinqlx_setup: ()) {
        let player = Player {
            name: "UnnamedPlayer".to_string().into(),
            player_info: PlayerInfo {
                name: "UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        assert_eq!(
            Python::with_gil(|py| player.get_name(py)),
            "UnnamedPlayer^7"
        );
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

        let player = Player {
            user_info: r"\asdf\qwertz\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result =
                Python::with_gil(|py| player.set_name(py, "^1Unnamed^2Player".to_string()));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_clean_name_returns_cleaned_name(_pyshinqlx_setup: ()) {
        let player = Player {
            name: "^7^1S^3hi^4N^10^7".to_string().into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_clean_name(py));
        assert_eq!(result, "ShiN0");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_qport_where_no_port_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_qport(py), -1);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_qport_for_port_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\qport\27666".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\qport\27666".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_qport(py), 27666);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_qport_for_invalid_port_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\qport\asdf".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\qport\asdf".to_string(),
                ..default_test_player_info()
            }
            .into(),
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
    fn get_team_for_team_t_values(
        _pyshinqlx_setup: (),
        #[case] team: team_t,
        #[case] return_value: &str,
    ) {
        let player = Player {
            player_info: PlayerInfo {
                team: team as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(
                player.get_team(py).expect("result was not OK"),
                return_value
            )
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_team_for_invalid_team(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                team: 42,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert!(player
                .get_team(py)
                .is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_team_with_invalid_team(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = default_test_player().set_team(py, "invalid team".to_string());
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
                let result =
                    Python::with_gil(|py| default_test_player().set_team(py, new_team.to_string()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_colors_where_no_colors_are_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_colors(py), (0.0, 0.0));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_colors_for_colors_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\color1\42\color2\21".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\color1\42\colors2\21".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_colors(py), (42.0, 21.0));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_colors_for_invalid_color1_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\color1\asdf\color2\42".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\color1\asdf\color2\42".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_colors(py), (0.0, 42.0));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_colors_for_invalid_color2_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\color1\42\color2\asdf".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\color1\42\color2\asdf".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            assert_eq!(player.get_colors(py), (42.0, 0.0));
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

        let player = Player {
            user_info: r"\asdf\qwertz\color1\7.0\color2\5\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\color1\7.0\color2\5\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_colors(py, (0, 3)));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_model_when_no_model_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_model(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_model_when_model_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\model\asdf".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\model\asdf".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_model(py));
        assert_eq!(result.expect("result was not OK"), "asdf");
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

        let player = Player {
            user_info: r"\asdf\qwertz\model\Anarki\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\model\Anarki\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_model(py, "Uriel".to_string()));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_headmodel_when_no_headmodel_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_headmodel(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_headmodel_when_headmodel_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\headmodel\asdf".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\headmodel\asdf".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_headmodel(py));
        assert_eq!(result.expect("result was not OK"), "asdf");
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

        let player = Player {
            user_info: r"\asdf\qwertz\headmodel\Anarki\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\headmodel\Anarki\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_headmodel(py, "Uriel".to_string()));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_handicap_when_no_handicap_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_handicap(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_handicap_when_handicap_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\handicap\42".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\handicap\42".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_handicap(py));
        assert_eq!(result.expect("result was not OK"), "42");
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

        let player = Player {
            user_info: r"\asdf\qwertz\handicap\100\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\handicap\100\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result =
                Python::with_gil(|py| player.set_handicap(py, PyString::new(py, "50").into_any()));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_handicap_for_unparseable_value(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\asdf\qwertz\handicap\100\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\handicap\100\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.set_handicap(py, PyString::new(py, "asdf").into_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_autohop_when_no_autohop_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_autohop(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_autohop_when_autohop_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\autohop\1".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\autohop\1".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_autohop(py));
        assert_eq!(result.expect("result was not OK"), 1);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_autohop_when_autohop_is_disabled(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\autohop\0".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\autohop\0".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_autohop(py));
        assert_eq!(result.expect("result was not OK"), 0);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_autohop_when_autohop_cannot_be_parsed(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\autohop\asdf".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\autohop\asdf".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_autohop(py);
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

        let player = Player {
            user_info: r"\asdf\qwertz\autohop\1\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\autohop\1\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result =
                Python::with_gil(|py| player.set_autohop(py, 0i32.into_pyobject(py)?.into_any()));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_autohop_for_unparseable_value(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\asdf\qwertz\autohop\1\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\autohop\1\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.set_autohop(py, PyString::new(py, "asdf").into_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_autoaction_when_no_autoaction_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_autoaction(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_autoaction_when_autohop_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\autoaction\1".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\autoaction\1".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_autoaction(py));
        assert_eq!(result.expect("result was not OK"), 1);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_autoaction_when_autoaction_is_disabled(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\autoaction\0".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\autoaction\0".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_autoaction(py));
        assert_eq!(result.expect("result was not OK"), 0);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_autoaction_when_autoaction_cannot_be_parsed(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\autoaction\asdf".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\autoaction\asdf".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_autoaction(py);
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

        let player = Player {
            user_info: r"\asdf\qwertz\autoaction\1\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\autoaction\1\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                player.set_autoaction(py, 0i32.into_pyobject(py)?.into_any())
            });
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_autoaction_with_unparseable_value(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\asdf\qwertz\autoaction\1\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\autoaction\1\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.set_autoaction(py, PyString::new(py, "asdf").into_any());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_predictitems_when_no_predictitems_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: "".to_string(),
            player_info: PlayerInfo {
                userinfo: "".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_predictitems(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_predictitems_when_predictitems_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\cg_predictitems\1".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\cg_predictitems\1".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_predictitems(py));
        assert_eq!(result.expect("result was not OK"), 1);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_predititems_when_predictitems_is_disabled(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\cg_predictitems\0".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\cg_predictitems\0".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_predictitems(py));
        assert_eq!(result.expect("result was not OK"), 0);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_predititems_when_predictitems_is_unparseable(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\cg_predictitems\asdf".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\cg_predictitems\asdf".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_predictitems(py);
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

        let player = Player {
            user_info: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                player.set_predictitems(py, 0i32.into_pyobject(py)?.into_any())
            });
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_predictitems_with_unparseable_value(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.set_predictitems(py, PyString::new(py, "asdf").into_any());
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
        let player = Player {
            player_info: PlayerInfo {
                connection_state: client_state as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_connection_state(py));
        assert_eq!(result.expect("result was not Ok"), expected_value);
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_connection_state_for_invalid_value(_pyshinqlx_setup: ()) {
        let player = Player {
            player_info: PlayerInfo {
                connection_state: 42,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.get_connection_state(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_state_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_state(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_state_for_client_without_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity
                    .expect_get_game_client()
                    .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
                mock_game_entity
            });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_state(py));
            assert_eq!(result.expect("result was not OK"), None);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_state_transforms_from_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx
            .expect()
            .with(predicate::eq(2))
            .returning(|_| {
                let mut mock_game_entity = MockGameEntity::new();
                mock_game_entity.expect_get_game_client().returning(|| {
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
                });
                mock_game_entity.expect_get_health().returning(|| 123);
                mock_game_entity
            });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_state(py));
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
    }

    #[rstest]
    #[case(privileges_t::PRIV_MOD as i32, Some("mod".to_string()))]
    #[case(privileges_t::PRIV_ADMIN as i32, Some("admin".to_string()))]
    #[case(privileges_t::PRIV_ROOT as i32, Some("root".to_string()))]
    #[case(privileges_t::PRIV_BANNED as i32, Some("banned".to_string()))]
    #[case(privileges_t::PRIV_NONE as i32, None)]
    #[case(42, None)]
    #[cfg_attr(miri, ignore)]
    fn get_privileges_various_values(
        _pyshinqlx_setup: (),
        #[case] privileges: i32,
        #[case] expected_value: Option<String>,
    ) {
        let player = Player {
            player_info: PlayerInfo {
                privileges,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_privileges(py));
        assert_eq!(result, expected_value);
    }

    #[rstest]
    #[case(None, & privileges_t::PRIV_NONE)]
    #[case(Some("none".to_string()), & privileges_t::PRIV_NONE)]
    #[case(Some("mod".to_string()), & privileges_t::PRIV_MOD)]
    #[case(Some("admin".to_string()), & privileges_t::PRIV_ADMIN)]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn set_privileges_for_valid_values(
        _pyshinqlx_setup: (),
        #[case] opt_priv: Option<String>,
        #[case] privileges: &'static privileges_t,
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_privileges()
                    .with(predicate::eq(*privileges as i32))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_privileges(py, opt_priv));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn set_privileges_for_invalid_string(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.set_privileges(py, Some("root".to_string()));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_country_when_country_is_set(_pyshinqlx_setup: ()) {
        let player = Player {
            user_info: r"\country\de".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\country\de".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        let result = Python::with_gil(|py| player.get_country(py));
        assert_eq!(result.expect("result was not OK"), "de");
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

        let player = Player {
            user_info: r"\asdf\qwertz\country\de\name\UnnamedPlayer".to_string(),
            player_info: PlayerInfo {
                userinfo: r"\asdf\qwertz\country\de\name\UnnamedPlayer".to_string(),
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_country(py, "uk".to_string()));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_valid_for_valid_player(_pyshinqlx_setup: ()) {
        let player = Player {
            valid: true.into(),
            ..default_test_player()
        };
        Python::with_gil(|py| assert_eq!(player.get_valid(py), true));
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_valid_for_invalid_player(_pyshinqlx_setup: ()) {
        let player = Player {
            valid: false.into(),
            ..default_test_player()
        };
        Python::with_gil(|py| assert_eq!(player.get_valid(py), false));
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_stats_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_stats(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_stats_for_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
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
            });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_stats(py));

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
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_stats_for_game_entiy_with_no_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_stats(py));

            assert_eq!(result.expect("result was not OK"), None);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_ping_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_ping(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_ping_for_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
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
            });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_ping(py));

            assert_eq!(result.expect("result was not OK"), 42);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_ping_for_game_entiy_with_no_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_ping(py));

            assert_eq!(result.expect("result was not OK"), 999);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn position_gathers_players_position_with_no_kwargs(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.position(py, false, None);
                assert!(result.is_ok_and(|value| value
                    .extract::<Vector3>()
                    .expect("result was not a Vector3")
                    == Vector3(1, 2, 3)));
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.position(
                    py,
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
    #[case([("x".to_string(), 42)], (42.0, 0.0, 0.0))]
    #[case([("y".to_string(), 42)], (0.0, 42.0, 0.0))]
    #[case([("z".to_string(), 42)], (0.0, 0.0, 42.0))]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn position_resets_players_position_with_single_value(
        _pyshinqlx_setup: (),
        #[case] position: [(String, i32); 1],
        #[case] expected_position: (f32, f32, f32),
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_position()
                        .with(predicate::eq(expected_position))
                        .times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.position(
                    py,
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.position(
                    py,
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.velocity(py, false, None);
                assert!(result.is_ok_and(|value| value
                    .extract::<Vector3>()
                    .expect("result was not a Vector3")
                    == Vector3(1, 2, 3)));
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.velocity(
                    py,
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
    #[case([("x".to_string(), 42)], (42.0, 0.0, 0.0))]
    #[case([("y".to_string(), 42)], (0.0, 42.0, 0.0))]
    #[case([("z".to_string(), 42)], (0.0, 0.0, 42.0))]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn velocity_resets_players_veloity_with_single_value(
        _pyshinqlx_setup: (),
        #[case] velocity: [(String, i32); 1],
        #[case] expected_velocity: (f32, f32, f32),
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_velocity()
                        .with(predicate::eq(expected_velocity))
                        .times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.velocity(
                    py,
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.velocity(
                    py,
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.weapons(py, false, None);
                assert!(result.is_ok_and(|value| value
                    .extract::<Weapons>()
                    .expect("result was not Weapons")
                    == Weapons(1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1)));
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.weapons(
                    py,
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
    #[case([("g".to_string(), 1)], [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("mg".to_string(), 1)], [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("sg".to_string(), 1)], [0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("gl".to_string(), 1)], [0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("rl".to_string(), 1)], [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("lg".to_string(), 1)], [0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("rg".to_string(), 1)], [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("pg".to_string(), 1)], [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("bfg".to_string(), 1)], [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0])]
    #[case([("gh".to_string(), 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0])]
    #[case([("ng".to_string(), 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0])]
    #[case([("pl".to_string(), 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0])]
    #[case([("cg".to_string(), 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0])]
    #[case([("hmg".to_string(), 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0])]
    #[case([("hands".to_string(), 1)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn weapons_resets_players_weapons_with_single_value(
        _pyshinqlx_setup: (),
        #[case] weapons: [(String, i32); 1],
        #[case] expected_weapons: [i32; 15],
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_weapons()
                        .with(predicate::eq(expected_weapons))
                        .times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.weapons(
                    py,
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.weapons(
                    py,
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.weapon(py, None);
                assert_eq!(
                    result
                        .expect("result was not Ok")
                        .extract::<i32>()
                        .expect("result was not an integer"),
                    weapon as i32
                )
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn weapon_gets_currently_held_weapon_with_no_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.weapon(py, None);
                assert_eq!(
                    result
                        .expect("result was not Ok")
                        .extract::<i32>()
                        .expect("result was not an integer"),
                    weapon_t::WP_HANDS as i32
                )
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_weapon()
                        .with(predicate::eq(expected_weapon as i32))
                        .times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.weapon(py, Some(PyString::new(py, weapon_str).into_any()));
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
    fn weapon_sets_players_weapon_from_invalid_str(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.weapon(py, Some(PyString::new(py, "invalid weapon").into_any()));
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_weapon()
                        .with(predicate::eq(expected_weapon as i32))
                        .times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.weapon(
                    py,
                    Some(
                        weapon_index
                            .into_pyobject(py)
                            .expect("this should not happen")
                            .into_any(),
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
    fn weapon_sets_players_weapon_from_invalid_int(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.weapon(
                py,
                Some(
                    42i32
                        .into_pyobject(py)
                        .expect("this should not happen")
                        .into_any(),
                ),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ammo_gathers_players_ammo_with_no_kwargs(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.ammo(py, false, None);
                assert!(result.is_ok_and(|value| value
                    .extract::<Weapons>()
                    .expect("result was not Weapons")
                    == Weapons(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15)));
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.ammo(
                    py,
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
    #[case([("g".to_string(), 42)], [42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("mg".to_string(), 42)], [0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("sg".to_string(), 42)], [0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("gl".to_string(), 42)], [0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("rl".to_string(), 42)], [0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("lg".to_string(), 42)], [0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("rg".to_string(), 42)], [0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("pg".to_string(), 42)], [0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0])]
    #[case([("bfg".to_string(), 42)], [0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0])]
    #[case([("gh".to_string(), 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0])]
    #[case([("ng".to_string(), 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0])]
    #[case([("pl".to_string(), 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0])]
    #[case([("cg".to_string(), 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0])]
    #[case([("hmg".to_string(), 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0])]
    #[case([("hands".to_string(), 42)], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ammo_resets_players_ammos_with_single_value(
        _pyshinqlx_setup: (),
        #[case] ammos: [(String, i32); 1],
        #[case] expected_ammos: [i32; 15],
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_ammos()
                        .with(predicate::eq(expected_ammos))
                        .times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.ammo(
                    py,
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.ammo(
                    py,
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.powerups(py, false, None);
                assert!(result.is_ok_and(|value| value
                    .extract::<Powerups>()
                    .expect("result was not a Powerups")
                    == Powerups(1000, 2000, 3000, 4000, 5000, 6000)));
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.powerups(
                    py,
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
    #[case([("quad".to_string(), 42)], [42000, 0, 0, 0, 0, 0])]
    #[case([("battlesuit".to_string(), 42)], [0, 42000, 0, 0, 0, 0])]
    #[case([("haste".to_string(), 42)], [0, 0, 42000, 0, 0, 0])]
    #[case([("invisibility".to_string(), 42)], [0, 0, 0, 42000, 0, 0])]
    #[case([("regeneration".to_string(), 42)], [0, 0, 0, 0, 42000, 0])]
    #[case([("invulnerability".to_string(), 42)], [0, 0, 0, 0, 0, 42000])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn powerups_resets_players_powerups_with_single_value(
        _pyshinqlx_setup: (),
        #[case] powerups: [(String, i32); 1],
        #[case] expected_powerups: [i32; 6],
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_powerups()
                        .with(predicate::eq(expected_powerups))
                        .times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.powerups(
                    py,
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.powerups(
                    py,
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
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_holdable(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[case(Holdable::None, None)]
    #[case(Holdable::Teleporter, Some("teleporter".to_string()))]
    #[case(Holdable::MedKit, Some("medkit".to_string()))]
    #[case(Holdable::Flight, Some("flight".to_string()))]
    #[case(Holdable::Kamikaze, Some("kamikaze".to_string()))]
    #[case(Holdable::Portal, Some("portal".to_string()))]
    #[case(Holdable::Invulnerability, Some("invulnerability".to_string()))]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_holdable_with_various_values(
        _pyshinqlx_setup: (),
        #[case] holdable: Holdable,
        #[case] expected_result: Option<String>,
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_holdable(py));
            assert_eq!(result.expect("result was not Ok"), expected_result);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_holdable_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.set_holdable(py, Some("kamikaze".to_string()));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)))
        });
    }

    #[rstest]
    #[case("unknown")]
    #[case("asdf")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_holdable_for_unknown_values(_pyshinqlx_setup: (), #[case] invalid_str: &str) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.set_holdable(py, Some(invalid_str.to_string()));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)))
        });
    }

    #[rstest]
    #[case(None, Holdable::None)]
    #[case(Some("none".to_string()), Holdable::None)]
    #[case(Some("teleporter".to_string()), Holdable::Teleporter)]
    #[case(Some("medkit".to_string()), Holdable::MedKit)]
    #[case(Some("kamikaze".to_string()), Holdable::Kamikaze)]
    #[case(Some("portal".to_string()), Holdable::Portal)]
    #[case(Some("invulnerability".to_string()), Holdable::Invulnerability)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_holdable_for_various_values(
        _pyshinqlx_setup: (),
        #[case] new_holdable: Option<String>,
        #[case] expected_holdable: Holdable,
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
                    let mut mock_game_client = MockGameClient::new();
                    mock_game_client
                        .expect_set_holdable()
                        .with(predicate::eq(expected_holdable))
                        .times(1);
                    Ok(mock_game_client)
                });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_holdable(py, new_holdable));
            assert!(result.is_ok());
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_holdable(py, Some("flight".to_string())));
            assert!(result.is_ok());
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.drop_holdable(py));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn flight_gathers_players_flight_parameters_with_no_kwargs(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.flight(py, false, None);
                assert!(result.is_ok_and(|value| value
                    .extract::<Flight>()
                    .expect("result was not a Flight")
                    == Flight(1, 2, 3, 4)));
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
        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.flight(
                    py,
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
    #[case([("fuel".to_string(), 42)], Flight(42, 16_000, 1_200, 0))]
    #[case([("max_fuel".to_string(), 42)], Flight(16_000, 42, 1_200, 0))]
    #[case([("thrust".to_string(), 42)], Flight(16_000, 16_000, 42, 0))]
    #[case([("refuel".to_string(), 42)], Flight(16_000, 16_000, 1_200, 42))]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn flight_resets_players_flight_with_single_value(
        _pyshinqlx_setup: (),
        #[case] flight_opts: [(String, i32); 1],
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

        let game_entity_from_ctx = MockGameEntity::from_context();
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.flight(
                    py,
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.flight(
                    py,
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_noclip(py));
            assert_eq!(result.expect("result was not Ok"), noclip_state.clone());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_noclip_for_player_without_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_noclip(py));
            assert_eq!(result.expect("result was not Ok"), false);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_noclip_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_noclip(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[case(true)]
    #[case(false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_noclip_set_players_noclip_value_by_bool(
        _pyshinqlx_setup: (),
        #[case] noclip_value: bool,
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                player.set_noclip(py, PyBool::new(py, noclip_value).to_owned().into_any())
            });

            assert!(result.is_ok());
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                player.set_noclip(py, noclip_value.into_pyobject(py)?.into_any())
            });

            assert!(result.is_ok());
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                player.set_noclip(py, PyString::new(py, noclip_value).into_any())
            });

            assert!(result.as_ref().is_ok(), "{:?}", result.err());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_noclip_set_players_noclip_value_by_none(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_noclip(py, py.None().into_bound(py)));

            assert!(result.as_ref().is_ok(), "{:?}", result.err());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_health_returns_players_health_state(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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
            mock_game_entity.expect_get_health().returning(|| 42);
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_health(py));
            assert_eq!(result.expect("result was not Ok"), 42);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_health_for_player_without_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_health(py));
            assert_eq!(result.expect("result was not Ok"), 0);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_health_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_health(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_health_set_players_health(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_set_health()
                .with(predicate::eq(666))
                .times(1);
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_health(py, 666));

            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_armor_returns_players_armor_state(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
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
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_armor(py));
            assert_eq!(result.expect("result was not Ok"), 42);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_armor_for_player_without_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_health(py));
            assert_eq!(result.expect("result was not Ok"), 0);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_armor_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_armor(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_armor_set_players_armor(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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
                mock_game_client
                    .expect_set_armor()
                    .with(predicate::eq(666))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_armor(py, 666));

            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[case(true)]
    #[case(false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_is_alive_returns_players_is_alive_state(_pyshinqlx_setup: (), #[case] is_alive: bool) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_is_alive(py));
            assert_eq!(result.expect("result was not Ok"), is_alive);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_is_alive_for_player_without_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_is_alive(py));
            assert_eq!(result.expect("result was not Ok"), false);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_is_alive_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_is_alive(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_is_alive_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_is_alive(py);
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_is_alive(py, false));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_is_alive_for_dead_player_with_false(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(|_| {
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
        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_is_alive(py, false));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_is_alive_for_alive_player_with_true(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().times(1).returning(|_| {
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_is_alive(py, true));
            assert!(result.is_ok());
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_is_alive(py, true));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[case(true)]
    #[case(false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_is_frozen_returns_players_is_frozen_state(
        _pyshinqlx_setup: (),
        #[case] is_frozen: bool,
    ) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_is_frozen(py));
            assert_eq!(result.expect("result was not Ok"), is_frozen);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_is_frozen_for_player_without_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_is_frozen(py));
            assert_eq!(result.expect("result was not Ok"), false);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_is_frozen_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_is_frozen(py);
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(move |_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(move || {
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
                });
            mock_game_entity.expect_get_health();
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_is_chatting(py));
            assert_eq!(result.expect("result was not Ok"), is_chatting);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_is_chatting_for_player_without_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_is_chatting(py));
            assert_eq!(result.expect("result was not Ok"), false);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_is_chatting_with_no_main_engine(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_is_chatting(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_score_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_score(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_score_for_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
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
            });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_score(py));
            assert_eq!(result.expect("result was not OK"), 42);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_score_for_game_entiy_with_no_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.get_score(py));
            assert_eq!(result.expect("result was not OK"), 0);
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn set_score_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.set_score(py, 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn set_score_for_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mut mock_game_client = MockGameClient::new();
                mock_game_client
                    .expect_set_score()
                    .with(predicate::eq(42))
                    .times(1);
                Ok(mock_game_client)
            });
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_score(py, 42));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn set_score_for_game_entiy_with_no_game_client(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity
                .expect_get_game_client()
                .returning(|| Err(QuakeLiveEngineError::MainEngineNotInitialized));
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.set_score(py, 42));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_channel_returns_tell_channel(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.get_channel(py);
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.center_print(py, "asdf"));
            assert!(result.is_ok());
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

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.tell(py, "asdf", None);
                assert!(result.is_ok());

                let _ = run_all_frame_tasks(py);
            });
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn tell_with_limit_keyword(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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

        let player = Player {
            player_info: PlayerInfo {
                connection_state: clientState_t::CS_ACTIVE as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.tell(
                    py,
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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

        let player = Player {
            player_info: PlayerInfo {
                connection_state: clientState_t::CS_ACTIVE as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
                let result = player.tell(
                    py,
                    "These_are_four_lines",
                    Some(
                        &[
                            (
                                "limit",
                                5i32.into_pyobject(py)
                                    .expect("this should not happen")
                                    .into_any(),
                            ),
                            ("delimiter", PyString::new(py, "_").into_any()),
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
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
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

        let player = Player {
            player_info: PlayerInfo {
                connection_state: clientState_t::CS_ACTIVE as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| player.kick(py, "you stink, go away!"));
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn ban_bans_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("ban 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.ban(py));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn tempban_tempbans_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("tempban 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.tempban(py));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn addadmin_adds_player_to_admins(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("addadmin 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.addadmin(py));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn addmod_adds_player_to_mods(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("addmod 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.addmod(py));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn demote_demotes_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("demote 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.demote(py));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn mute_mutes_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("mute 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.mute(py));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn unmute_unmutes_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("unmute 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.unmute(py));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn put_with_invalid_team(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        Python::with_gil(|py| {
            let result = player.put(py, "invalid team");
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
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command(format!("put 2 {}", new_team.to_lowercase()), 1)
            .run(|| {
                let result = Python::with_gil(|py| player.put(py, new_team));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn addscore_adds_score_to_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("addscore 2 42", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.addscore(py, 42));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn switch_with_player_on_same_team(_pyshinqlx_setup: ()) {
        let player = Player {
            id: 2,
            player_info: PlayerInfo {
                team: team_t::TEAM_SPECTATOR as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let other_player = Player {
            id: 1,
            player_info: PlayerInfo {
                team: team_t::TEAM_SPECTATOR as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        Python::with_gil(|py| {
            let result = player.switch(py, other_player);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn switch_with_player_on_different_team(_pyshinqlx_setup: ()) {
        let player = Player {
            id: 2,
            player_info: PlayerInfo {
                team: team_t::TEAM_RED as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };
        let other_player = Player {
            id: 1,
            player_info: PlayerInfo {
                team: team_t::TEAM_BLUE as i32,
                ..default_test_player_info()
            }
            .into(),
            ..default_test_player()
        };

        MockEngineBuilder::default()
            .with_execute_console_command("put 2 blue", 1)
            .with_execute_console_command("put 1 red", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.switch(py, other_player));
                assert!(result.as_ref().is_ok(), "{:?}", result.as_ref().unwrap());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn slap_slaps_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("slap 2 42", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.slap(py, 42));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn slay_slays_player(_pyshinqlx_setup: ()) {
        let player = default_test_player();

        MockEngineBuilder::default()
            .with_execute_console_command("slay 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| player.slay(py));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn slay_with_mod_slays_with_mod(_pyshinqlx_setup: ()) {
        let game_entity_from_ctx = MockGameEntity::from_context();
        game_entity_from_ctx.expect().returning(|_| {
            let mut mock_game_entity = MockGameEntity::new();
            mock_game_entity.expect_get_game_client().returning(|| {
                let mock_game_client = MockGameClient::new();
                Ok(mock_game_client)
            });
            mock_game_entity.expect_get_health().returning(|| 0);
            mock_game_entity.expect_slay_with_mod().times(0);
            mock_game_entity
        });

        let player = default_test_player();

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                player.slay_with_mod(py, meansOfDeath_t::MOD_PROXIMITY_MINE as i32)
            });
            assert!(result.is_ok());
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
            let all_players =
                Python::with_gil(|py| Player::all_players(&py.get_type::<Player>(), py));
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
}

static _DUMMY_USERINFO: &str = r#"
\ui_singlePlayerActive\0
\cg_autoAction\1
\cg_autoHop\0
\cg_predictItems\1
\model\bitterman/sport_blue
\headmodel\crash/red
\handicap\100
\cl_anonymous\0
\color1\4\color2\23
\sex\male
\teamtask\0
\rate\25000
\country\NO"#;

#[pyclass(module = "_player", name = "AbstractDummyPlayer", extends = Player, subclass, frozen)]
pub(crate) struct AbstractDummyPlayer;

#[pymethods]
impl AbstractDummyPlayer {
    #[new]
    #[pyo3(signature = (name = "DummyPlayer"), text_signature = "(name = \"DummyPlayer\")")]
    fn py_new(name: &str) -> PyClassInitializer<Self> {
        let player_info = PlayerInfo {
            client_id: -1,
            name: name.to_string(),
            connection_state: clientState_t::CS_CONNECTED as i32,
            userinfo: _DUMMY_USERINFO.to_string(),
            steam_id: 0,
            team: team_t::TEAM_SPECTATOR as i32,
            privileges: privileges_t::PRIV_NONE as i32,
        };
        PyClassInitializer::from(Player::py_new(-1, Some(player_info)).unwrap())
            .add_subclass(AbstractDummyPlayer {})
    }

    #[getter(id)]
    fn get_id(&self) -> PyResult<i32> {
        Err(PyAttributeError::new_err(
            "Dummy players do not have client IDs.",
        ))
    }

    #[getter(steam_id)]
    fn get_steam_id(&self) -> PyResult<i64> {
        Err(PyNotImplementedError::new_err(
            "steam_id property needs to be implemented.",
        ))
    }

    fn update(&self) -> PyResult<()> {
        Ok(())
    }

    #[getter(channel)]
    fn get_channel(&self) -> PyResult<Bound<'_, PyAny>> {
        Err(PyNotImplementedError::new_err(
            "channel property needs to be implemented.",
        ))
    }

    #[pyo3(signature = (msg, **kwargs))]
    fn tell(
        &self,
        #[allow(unused_variables)] msg: &str,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "tell() needs to be implemented.",
        ))
    }
}

#[cfg(test)]
mod pyshinqlx_abstract_dummy_player_tests {
    use crate::ffi::python::prelude::*;

    use pyo3::exceptions::{PyAttributeError, PyNotImplementedError};
    use rstest::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dummy_player_is_a_player_instance(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert(isinstance(shinqlx.AbstractDummyPlayer(), shinqlx.Player))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_id_returns_attribute_error(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx
shinqlx.AbstractDummyPlayer().id
            "#,
                None,
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyAttributeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_steam_id_returns_not_implemented_error(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx
shinqlx.AbstractDummyPlayer().steam_id
            "#,
                None,
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn update_does_nothing(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx
shinqlx.AbstractDummyPlayer().update()
            "#,
                None,
                None,
            );
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_channel_returns_not_implemented_error(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx
shinqlx.AbstractDummyPlayer().channel
            "#,
                None,
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn tell_returns_not_implemented_error(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = py.run(
                cr#"
import shinqlx
shinqlx.AbstractDummyPlayer().tell("asdf")
            "#,
                None,
                None,
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }
}

#[pyclass(module = "_player", name = "RconDummyPlayer", extends = AbstractDummyPlayer, frozen)]
pub(crate) struct RconDummyPlayer;

#[pymethods]
impl RconDummyPlayer {
    #[new]
    pub(crate) fn py_new() -> PyClassInitializer<Self> {
        AbstractDummyPlayer::py_new("RconDummyPlayer").add_subclass(Self {})
    }

    #[getter(steam_id)]
    fn get_steam_id(&self, py: Python<'_>) -> PyResult<i64> {
        py.allow_threads(|| owner().map(|opt_value| opt_value.unwrap_or_default()))
    }

    #[getter(channel)]
    fn get_channel(slf: PyRef<'_, Self>) -> PyResult<Py<ConsoleChannel>> {
        CONSOLE_CHANNEL.load().as_ref().map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to CONSOLE_CHANNEL",
            )),
            |console_channel| Ok(console_channel.clone_ref(slf.py())),
        )
    }

    #[pyo3(signature = (msg, **kwargs))]
    fn tell(
        slf: PyRef<'_, Self>,
        msg: &str,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        CONSOLE_CHANNEL.load().as_ref().map_or(
            Err(PyEnvironmentError::new_err(
                "could not get access to CONSOLE_CHANNEL",
            )),
            |console_channel| console_channel.bind(slf.py()).reply(msg, 100, " "),
        )
    }
}

#[cfg(test)]
mod pyshinqlx_rcon_dummy_player_tests {
    use super::RconDummyPlayer;

    use crate::ffi::python::prelude::*;
    use crate::ffi::python::CONSOLE_CHANNEL;

    use crate::ffi::c::prelude::{cvar_t, CVar, CVarBuilder};

    use crate::hooks::mock_hooks::shinqlx_com_printf_context;
    use crate::prelude::*;

    use core::borrow::BorrowMut;

    use mockall::predicate;
    use rstest::*;

    use pyo3::exceptions::PyEnvironmentError;
    use pyo3::prelude::*;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn dummy_player_is_a_player_instance(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
assert(isinstance(shinqlx.RconDummyPlayer(), shinqlx.Player))
assert(isinstance(shinqlx.RconDummyPlayer(), shinqlx.AbstractDummyPlayer))
            "#,
                None,
                None,
            )
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn steam_id_return_owner_id(_pyshinqlx_setup: ()) {
        let owner = c"1234567890";
        let mut raw_cvar = CVarBuilder::default()
            .string(owner.as_ptr().cast_mut())
            .build()
            .expect("this should not happen");

        MockEngineBuilder::default()
            .with_find_cvar(
                |cmd| cmd == "qlx_owner",
                move |_| CVar::try_from(raw_cvar.borrow_mut() as *mut cvar_t).ok(),
                1..,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let rcon_dummy_player =
                        Py::new(py, RconDummyPlayer::py_new()).expect("this should not happen");

                    let result = rcon_dummy_player.bind(py).borrow().get_steam_id(py);
                    assert!(result.is_ok_and(|value| value == 1234567890));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_channel_with_no_console_channel_initialized(_pyshinqlx_setup: ()) {
        CONSOLE_CHANNEL.store(None);

        Python::with_gil(|py| {
            let rcon_dummy_player =
                Py::new(py, RconDummyPlayer::py_new()).expect("this should not happen");

            let result = RconDummyPlayer::get_channel(rcon_dummy_player.bind(py).borrow());
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_channel_with_console_channel_properly_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let console_channel =
                Py::new(py, ConsoleChannel::py_new()).expect("this should not happen");
            CONSOLE_CHANNEL.store(Some(console_channel.into()));

            let rcon_dummy_player =
                Py::new(py, RconDummyPlayer::py_new()).expect("this should not happen");

            let result = RconDummyPlayer::get_channel(rcon_dummy_player.bind(py).borrow());
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tell_with_no_console_channel_initialized(_pyshinqlx_setup: ()) {
        CONSOLE_CHANNEL.store(None);

        Python::with_gil(|py| {
            let rcon_dummy_player =
                Py::new(py, RconDummyPlayer::py_new()).expect("this should not happen");

            let result = RconDummyPlayer::tell(rcon_dummy_player.bind(py).borrow(), "asdf", None);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tell_with_console_channel_properly_initialized(_pyshinqlx_setup: ()) {
        let com_printf_ctx = shinqlx_com_printf_context();
        com_printf_ctx
            .expect()
            .with(predicate::eq("asdf\n"))
            .times(1);

        Python::with_gil(|py| {
            let console_channel =
                Py::new(py, ConsoleChannel::py_new()).expect("this should not happen");
            CONSOLE_CHANNEL.store(Some(console_channel.into()));

            let rcon_dummy_player =
                Py::new(py, RconDummyPlayer::py_new()).expect("this should not happen");

            let result = RconDummyPlayer::tell(rcon_dummy_player.bind(py).borrow(), "asdf", None);
            assert!(result.is_ok());
        });
    }
}
