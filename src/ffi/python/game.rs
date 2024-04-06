use super::client_id;
use super::prelude::*;

use crate::{
    ffi::c::prelude::{CS_SCORES1, CS_SCORES2, CS_SERVERINFO, CS_STEAM_WORKSHOP_IDS},
    quake_live_engine::{GetConfigstring, SetConfigstring},
    MAIN_ENGINE,
};

use itertools::Itertools;
use log::*;

use pyo3::exceptions::PyEnvironmentError;
use pyo3::{
    create_exception,
    exceptions::{PyException, PyKeyError, PyValueError},
    types::{IntoPyDict, PyDict, PyType},
};

create_exception!(pyshinqlx_module, NonexistentGameError, PyException);

/// A class representing the game. That is, stuff like what map is being played,
/// if it's in warmup, and so on. It also has methods to call in timeins, aborts,
/// pauses, and so on.
#[pyclass(module = "_game", name = "Game", get_all)]
#[derive(PartialEq, Debug)]
pub(crate) struct Game {
    #[pyo3(name = "cached")]
    cached: bool,
    #[pyo3(name = "_valid")]
    valid: bool,
}

#[pymethods]
impl Game {
    #[new]
    #[pyo3(signature = (cached = true))]
    pub(crate) fn py_new(py: Python<'_>, cached: bool) -> PyResult<Self> {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            let configstring = main_engine.get_configstring(CS_SERVERINFO as u16);

            if configstring.is_empty() {
                return Err(NonexistentGameError::new_err(
                    "Tried to instantiate a game while no game is active.",
                ));
            }

            Ok(Game {
                cached,
                valid: true,
            })
        })
    }

    fn __repr__(slf: &Bound<'_, Self>) -> String {
        let Ok(classname) = slf.get_type().qualname() else {
            return "Game(N/A@N/A)".to_string();
        };
        let Ok(factory_type) = slf.getattr("type") else {
            return format!("{}(N/A@N/A)", classname);
        };
        let Ok(mapname) = slf.getattr("map") else {
            return format!("{}(N/A@N/A)", classname);
        };
        format!("{}({}@{})", classname, factory_type, mapname)
    }

    fn __str__(&mut self, py: Python<'_>) -> String {
        let Ok(factory_type) = self.get_type(py) else {
            return "Invalid game".to_string();
        };
        let Ok(mapname) = self.get_map(py) else {
            return "Invalid game".to_string();
        };
        format!("{} on {}", factory_type, mapname)
    }

    fn __contains__(&mut self, py: Python<'_>, item: &str) -> PyResult<bool> {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            let configstring = main_engine.get_configstring(CS_SERVERINFO as u16);

            if configstring.is_empty() {
                self.valid = false;
                return Err(NonexistentGameError::new_err(
                    "Invalid game. Is the server loading a new map?",
                ));
            }

            Ok(parse_variables(&configstring).get(item).is_some())
        })
    }

    fn __getitem__(&mut self, py: Python<'_>, item: &str) -> PyResult<String> {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            let configstring = main_engine.get_configstring(CS_SERVERINFO as u16);

            if configstring.is_empty() {
                self.valid = false;
                return Err(NonexistentGameError::new_err(
                    "Invalid game. Is the server loading a new map?",
                ));
            }

            parse_variables(&configstring)
                .get(item)
                .map_or_else(|| Err(PyKeyError::new_err(format!("'{}'", item))), Ok)
        })
    }

    /// A dictionary of unprocessed cvars. Use attributes whenever possible, but since some cvars
    /// might not have attributes on this class, this could be useful.
    #[getter(cvars)]
    fn get_cvars<'b>(&mut self, py: Python<'b>) -> PyResult<Bound<'b, PyDict>> {
        let configstring = py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            Ok(main_engine.get_configstring(CS_SERVERINFO as u16))
        })?;

        if configstring.is_empty() {
            self.valid = false;
            return Err(NonexistentGameError::new_err(
                "Invalid game. Is the server loading a new map?",
            ));
        }

        Ok(parse_variables(&configstring).into_py_dict_bound(py))
    }

    #[getter]
    fn get_type(&mut self, py: Python<'_>) -> PyResult<String> {
        let factory_type = self.__getitem__(py, "g_gametype")?;
        match factory_type.parse::<i32>() {
            Ok(0) => Ok("Free for All".to_string()),
            Ok(1) => Ok("Duel".to_string()),
            Ok(2) => Ok("Race".to_string()),
            Ok(3) => Ok("Team Deathmatch".to_string()),
            Ok(4) => Ok("Clan Arena".to_string()),
            Ok(5) => Ok("Capture the Flag".to_string()),
            Ok(6) => Ok("One Flag".to_string()),
            Ok(8) => Ok("Harvester".to_string()),
            Ok(9) => Ok("Freeze Tag".to_string()),
            Ok(10) => Ok("Domination".to_string()),
            Ok(11) => Ok("Attack and Defend".to_string()),
            Ok(12) => Ok("Red Rover".to_string()),
            _ => Ok("unknown".to_string()),
        }
    }

    #[getter(type_short)]
    fn get_type_short(&mut self, py: Python<'_>) -> PyResult<String> {
        let factory_type = self.__getitem__(py, "g_gametype")?;
        match factory_type.parse::<i32>() {
            Ok(0) => Ok("ffa".to_string()),
            Ok(1) => Ok("duel".to_string()),
            Ok(2) => Ok("race".to_string()),
            Ok(3) => Ok("tdm".to_string()),
            Ok(4) => Ok("ca".to_string()),
            Ok(5) => Ok("ctf".to_string()),
            Ok(6) => Ok("1f".to_string()),
            Ok(8) => Ok("har".to_string()),
            Ok(9) => Ok("ft".to_string()),
            Ok(10) => Ok("dom".to_string()),
            Ok(11) => Ok("ad".to_string()),
            Ok(12) => Ok("rr".to_string()),
            _ => Ok("N/A".to_string()),
        }
    }

    #[getter(map)]
    fn get_map(&mut self, py: Python<'_>) -> PyResult<String> {
        self.__getitem__(py, "mapname")
    }

    #[setter(map)]
    fn set_map(&mut self, py: Python<'_>, value: &str) -> PyResult<()> {
        let mapchange_command = format!("map {}", value);
        pyshinqlx_console_command(py, &mapchange_command)
    }

    /// The full name of the map. Ex.: ``Longest Yard``.
    #[getter(map_title)]
    fn get_map_title(&self, py: Python<'_>) -> PyResult<String> {
        let base_module = py.import_bound("_shinqlx")?;
        let map_title = base_module.getattr("_map_title")?;
        map_title.extract::<String>()
    }

    /// The map's subtitle. Usually either empty or has the author's name.
    #[getter(map_subtitle1)]
    fn get_map_subtitle1(&self, py: Python<'_>) -> PyResult<String> {
        let base_module = py.import_bound("_shinqlx")?;
        let map_title = base_module.getattr("_map_subtitle1")?;
        map_title.extract::<String>()
    }

    /// The map's second subtitle. Usually either empty or has the author's name.
    #[getter(map_subtitle2)]
    fn get_map_subtitle2(&self, py: Python<'_>) -> PyResult<String> {
        let base_module = py.import_bound("_shinqlx")?;
        let map_title = base_module.getattr("_map_subtitle2")?;
        map_title.extract::<String>()
    }

    #[getter(red_score)]
    fn get_red_score(&self, py: Python<'_>) -> PyResult<i32> {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            let configstring = main_engine.get_configstring(CS_SCORES1 as u16);
            Ok(configstring.parse::<i32>().unwrap_or_default())
        })
    }

    #[getter(blue_score)]
    fn get_blue_score(&self, py: Python<'_>) -> PyResult<i32> {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            let configstring = main_engine.get_configstring(CS_SCORES2 as u16);
            Ok(configstring.parse::<i32>().unwrap_or_default())
        })
    }

    #[getter(state)]
    fn get_state(&mut self, py: Python<'_>) -> PyResult<String> {
        let game_state = self.__getitem__(py, "g_gameState")?;
        if game_state == "PRE_GAME" {
            return Ok("warmup".to_string());
        }

        if game_state == "COUNT_DOWN" {
            return Ok("countdown".to_string());
        }

        if game_state == "IN_PROGRESS" {
            return Ok("in_progress".to_string());
        }

        warn!(target: "shinqlx", "Got unknown game state: {}", game_state);

        Ok(game_state)
    }

    #[getter(factory)]
    fn get_factory(&mut self, py: Python<'_>) -> PyResult<String> {
        self.__getitem__(py, "g_factory")
    }

    #[setter(factory)]
    fn set_factory(&mut self, py: Python<'_>, value: String) -> PyResult<()> {
        let mapchange_command = format!("map {} {}", self.get_map(py)?, value);
        pyshinqlx_console_command(py, &mapchange_command)
    }

    #[getter(hostname)]
    fn get_hostname(&mut self, py: Python<'_>) -> PyResult<String> {
        self.__getitem__(py, "sv_hostname")
    }

    #[setter(hostname)]
    fn set_hostname(&mut self, py: Python<'_>, value: String) -> PyResult<()> {
        pyshinqlx_set_cvar(py, "sv_hostname", &value, None)?;
        Ok(())
    }

    #[getter(instagib)]
    fn get_instagib(&mut self, py: Python<'_>) -> PyResult<bool> {
        let insta_cvar = self.__getitem__(py, "g_instagib")?;
        Ok(insta_cvar.parse::<i32>().is_ok_and(|value| value != 0))
    }

    #[setter(instagib)]
    fn set_instagib(&mut self, py: Python<'_>, value: Py<PyAny>) -> PyResult<()> {
        let string_cvar_value = match value.extract::<bool>(py) {
            Ok(true) => "1",
            Ok(false) => "0",
            Err(_) => match value.extract::<i32>(py) {
                Ok(1) => "1",
                Ok(0) => "0",
                _ => {
                    return Err(PyValueError::new_err(
                        "instagib needs to be 0, 1, or a bool.",
                    ));
                }
            },
        };
        pyshinqlx_set_cvar(py, "g_instagib", string_cvar_value, None)?;
        Ok(())
    }

    #[getter(loadout)]
    fn get_loadout(&mut self, py: Python<'_>) -> PyResult<bool> {
        let loadout_cvar = self.__getitem__(py, "g_loadout")?;
        Ok(loadout_cvar.parse::<i32>().is_ok_and(|value| value != 0))
    }

    #[setter(loadout)]
    fn set_loadout(&mut self, py: Python<'_>, value: Py<PyAny>) -> PyResult<()> {
        let string_cvar_value = match value.extract::<bool>(py) {
            Ok(true) => "1",
            Ok(false) => "0",
            Err(_) => match value.extract::<i32>(py) {
                Ok(1) => "1",
                Ok(0) => "0",
                _ => {
                    return Err(PyValueError::new_err(
                        "loadout needs to be 0, 1, or a bool.",
                    ));
                }
            },
        };
        pyshinqlx_set_cvar(py, "g_loadout", string_cvar_value, None)?;
        Ok(())
    }

    #[getter(maxclients)]
    fn get_maxclients(&mut self, py: Python<'_>) -> PyResult<i32> {
        let maxclients_cvar = self.__getitem__(py, "sv_maxclients")?;
        Ok(maxclients_cvar.parse::<i32>().unwrap_or_default())
    }

    #[setter(maxclients)]
    fn set_maxclients(&mut self, py: Python<'_>, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(py, "sv_maxclients", &value_str, None)?;
        Ok(())
    }

    #[getter(timelimit)]
    fn get_timelimit(&mut self, py: Python<'_>) -> PyResult<i32> {
        let timelimit_cvar = self.__getitem__(py, "timelimit")?;
        Ok(timelimit_cvar.parse::<i32>().unwrap_or_default())
    }

    #[setter(timelimit)]
    fn set_timelimit(&mut self, py: Python<'_>, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(py, "timelimit", &value_str, None)?;
        Ok(())
    }

    #[getter(fraglimit)]
    fn get_fraglimit(&mut self, py: Python<'_>) -> PyResult<i32> {
        let fraglimit_cvar = self.__getitem__(py, "fraglimit")?;
        Ok(fraglimit_cvar.parse::<i32>().unwrap_or_default())
    }

    #[setter(fraglimit)]
    fn set_fraglimit(&mut self, py: Python<'_>, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(py, "fraglimit", &value_str, None)?;
        Ok(())
    }

    #[getter(roundlimit)]
    fn get_roundlimit(&mut self, py: Python<'_>) -> PyResult<i32> {
        let roundlimit_cvar = self.__getitem__(py, "roundlimit")?;
        Ok(roundlimit_cvar.parse::<i32>().unwrap_or_default())
    }

    #[setter(roundlimit)]
    fn set_roundlimit(&mut self, py: Python<'_>, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(py, "roundlimit", &value_str, None)?;
        Ok(())
    }

    #[getter(roundtimelimit)]
    fn get_roundtimelimit(&mut self, py: Python<'_>) -> PyResult<i32> {
        let roundtimelimit_cvar = self.__getitem__(py, "roundtimelimit")?;
        Ok(roundtimelimit_cvar.parse::<i32>().unwrap_or_default())
    }

    #[setter(roundtimelimit)]
    fn set_roundtimelimit(&mut self, py: Python<'_>, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(py, "roundtimelimit", &value_str, None)?;
        Ok(())
    }

    #[getter(scorelimit)]
    fn get_scorelimit(&mut self, py: Python<'_>) -> PyResult<i32> {
        let scorelimit_cvar = self.__getitem__(py, "scorelimit")?;
        Ok(scorelimit_cvar.parse::<i32>().unwrap_or_default())
    }

    #[setter(scorelimit)]
    fn set_scorelimit(&mut self, py: Python<'_>, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(py, "scorelimit", &value_str, None)?;
        Ok(())
    }

    #[getter(capturelimit)]
    fn get_capturelimit(&mut self, py: Python<'_>) -> PyResult<i32> {
        let capturelimit_cvar = self.__getitem__(py, "capturelimit")?;
        Ok(capturelimit_cvar.parse::<i32>().unwrap_or_default())
    }

    #[setter(capturelimit)]
    fn set_capturelimit(&mut self, py: Python<'_>, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(py, "capturelimit", &value_str, None)?;
        Ok(())
    }

    #[getter(teamsize)]
    fn get_teamsize(&mut self, py: Python<'_>) -> PyResult<i32> {
        let teamsize_cvar = self.__getitem__(py, "teamsize")?;
        Ok(teamsize_cvar.parse::<i32>().unwrap_or_default())
    }

    #[setter(teamsize)]
    pub(crate) fn set_teamsize(&mut self, py: Python<'_>, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(py, "teamsize", &value_str, None)?;
        Ok(())
    }

    #[getter(tags)]
    fn get_tags(&mut self, py: Python<'_>) -> PyResult<Vec<String>> {
        let tags_cvar = self.__getitem__(py, "sv_tags")?;
        Ok(tags_cvar.split(',').map(|value| value.into()).collect())
    }

    #[setter(tags)]
    fn set_tags(&mut self, py: Python<'_>, value: Py<PyAny>) -> PyResult<()> {
        let string_cvar_value = match value.extract::<String>(py) {
            Ok(new_tags) => new_tags,
            Err(_) => match value.extract::<Vec<Py<PyAny>>>(py) {
                Ok(new_tags_list) => new_tags_list
                    .iter()
                    .map(|value| value.to_string())
                    .join(","),
                Err(_e) => {
                    return Err(PyValueError::new_err(
                        "tags need to be a string or an iterable returning strings.",
                    ));
                }
            },
        };
        pyshinqlx_set_cvar(py, "sv_tags", &string_cvar_value, None)?;
        Ok(())
    }

    #[getter(workshop_items)]
    fn get_workshop_items(&self, py: Python<'_>) -> PyResult<Vec<u64>> {
        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            let configstring = main_engine.get_configstring(CS_STEAM_WORKSHOP_IDS as u16);
            Ok(configstring
                .split(' ')
                .filter_map(|value| value.parse::<u64>().ok())
                .collect())
        })
    }

    #[setter(workshop_items)]
    fn set_workshop_items(&mut self, py: Python<'_>, value: Py<PyAny>) -> PyResult<()> {
        let workshop_items_str = match value.extract::<Vec<Py<PyAny>>>(py) {
            Ok(new_workshop_items) => new_workshop_items
                .iter()
                .map(|value| value.to_string())
                .join(" "),
            Err(_) => {
                return Err(PyValueError::new_err("The value needs to be an iterable."));
            }
        };

        py.allow_threads(|| {
            let Some(ref main_engine) = *MAIN_ENGINE.load() else {
                return Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                ));
            };

            main_engine.set_configstring(CS_STEAM_WORKSHOP_IDS as i32, &workshop_items_str);
            Ok(())
        })
    }

    #[classmethod]
    pub(crate) fn shuffle(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "forceshuffle")
    }

    #[classmethod]
    fn timeout(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "timeout")
    }

    #[classmethod]
    fn timein(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "timein")
    }

    #[classmethod]
    fn allready(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "allready")
    }

    #[classmethod]
    fn pause(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "pause")
    }

    #[classmethod]
    fn unpause(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "unpause")
    }

    #[classmethod]
    #[pyo3(signature = (team = None))]
    pub(crate) fn lock(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        team: Option<String>,
    ) -> PyResult<()> {
        match team {
            None => pyshinqlx_console_command(py, "lock"),
            Some(team_name) => {
                if !["free", "red", "blue", "spectator"].contains(&&*team_name.to_lowercase()) {
                    Err(PyValueError::new_err("Invalid team."))
                } else {
                    let lock_cmd = format!("lock {}", team_name.to_lowercase());
                    pyshinqlx_console_command(py, &lock_cmd)
                }
            }
        }
    }

    #[classmethod]
    #[pyo3(signature = (team = None))]
    pub(crate) fn unlock(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        team: Option<String>,
    ) -> PyResult<()> {
        match team {
            None => pyshinqlx_console_command(py, "unlock"),
            Some(team_name) => {
                if !["free", "red", "blue", "spectator"].contains(&&*team_name.to_lowercase()) {
                    Err(PyValueError::new_err("Invalid team."))
                } else {
                    let unlock_cmd = format!("unlock {}", team_name.to_lowercase());
                    pyshinqlx_console_command(py, &unlock_cmd)
                }
            }
        }
    }

    #[classmethod]
    fn put(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        player: Py<PyAny>,
        team: &str,
    ) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        if !["free", "red", "blue", "spectator"].contains(&&*team.to_lowercase()) {
            return Err(PyValueError::new_err("Invalid team."));
        }

        let team_change_cmd = format!("put {} {}", player_id, team.to_lowercase());
        pyshinqlx_console_command(py, &team_change_cmd)
    }

    #[classmethod]
    fn mute(_cls: &Bound<'_, PyType>, py: Python<'_>, player: Py<PyAny>) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let mute_cmd = format!("mute {}", player_id);
        pyshinqlx_console_command(py, &mute_cmd)
    }

    #[classmethod]
    fn unmute(_cls: &Bound<'_, PyType>, py: Python<'_>, player: Py<PyAny>) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let unmute_cmd = format!("unmute {}", player_id);
        pyshinqlx_console_command(py, &unmute_cmd)
    }

    #[classmethod]
    fn tempban(_cls: &Bound<'_, PyType>, py: Python<'_>, player: Py<PyAny>) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let tempban_cmd = format!("tempban {}", player_id);
        pyshinqlx_console_command(py, &tempban_cmd)
    }

    #[classmethod]
    fn ban(_cls: &Bound<'_, PyType>, py: Python<'_>, player: Py<PyAny>) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let ban_cmd = format!("ban {}", player_id);
        pyshinqlx_console_command(py, &ban_cmd)
    }

    #[classmethod]
    fn unban(_cls: &Bound<'_, PyType>, py: Python<'_>, player: Py<PyAny>) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let unban_cmd = format!("unban {}", player_id);
        pyshinqlx_console_command(py, &unban_cmd)
    }

    #[classmethod]
    fn opsay(_cls: &Bound<'_, PyType>, py: Python<'_>, msg: &str) -> PyResult<()> {
        let opsay_cmd = format!("opsay {}", msg);
        pyshinqlx_console_command(py, &opsay_cmd)
    }

    #[classmethod]
    fn addadmin(_cls: &Bound<'_, PyType>, py: Python<'_>, player: Py<PyAny>) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let addadmin_cmd = format!("addadmin {}", player_id);
        pyshinqlx_console_command(py, &addadmin_cmd)
    }

    #[classmethod]
    fn addmod(_cls: &Bound<'_, PyType>, py: Python<'_>, player: Py<PyAny>) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let addmod_cmd = format!("addmod {}", player_id);
        pyshinqlx_console_command(py, &addmod_cmd)
    }

    #[classmethod]
    fn demote(_cls: &Bound<'_, PyType>, py: Python<'_>, player: Py<PyAny>) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let demote_cmd = format!("demote {}", player_id);
        pyshinqlx_console_command(py, &demote_cmd)
    }

    #[classmethod]
    fn abort(_cls: &Bound<'_, PyType>, py: Python<'_>) -> PyResult<()> {
        pyshinqlx_console_command(py, "map_restart")
    }

    #[classmethod]
    fn addscore(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        player: Py<PyAny>,
        score: i32,
    ) -> PyResult<()> {
        let Some(player_id) = client_id(py, player, None) else {
            return Err(PyValueError::new_err("Invalid player."));
        };

        let addscore_cmd = format!("addscore {} {}", player_id, score);
        pyshinqlx_console_command(py, &addscore_cmd)
    }

    #[classmethod]
    fn addteamscore(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        team: &str,
        score: i32,
    ) -> PyResult<()> {
        if !["free", "red", "blue", "spectator"].contains(&&*team.to_lowercase()) {
            return Err(PyValueError::new_err("Invalid team."));
        }

        let addteamscore_cmd = format!("addteamscore {} {}", team.to_lowercase(), score);
        pyshinqlx_console_command(py, &addteamscore_cmd)
    }

    #[classmethod]
    fn setmatchtime(_cls: &Bound<'_, PyType>, py: Python<'_>, time: i32) -> PyResult<()> {
        let setmatchtime_cmd = format!("setmatchtime {}", time);
        pyshinqlx_console_command(py, &setmatchtime_cmd)
    }
}

#[cfg(test)]
mod pyshinqlx_game_tests {
    use super::NonexistentGameError;
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;
    use crate::MAIN_ENGINE;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyKeyError, PyValueError};
    use rstest::rstest;

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pyconstructor_when_no_main_engine_loaded() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::py_new(py, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pyconstructor_with_empty_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let result = Game::py_new(py, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pyconstructor_with_nonempty_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "asdf".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::py_new(py, true));
        assert_eq!(
            result.expect("result was not OK"),
            Game {
                cached: true,
                valid: true,
            }
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_when_no_main_engine_loaded() {
        MAIN_ENGINE.store(None);

        let result = Python::with_gil(|py| {
            let game = Bound::new(
                py,
                Game {
                    cached: true,
                    valid: true,
                },
            )
            .expect("this should not happen");
            Game::__repr__(&game)
        });
        assert_eq!(result, "Game(N/A@N/A)");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_empty_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let game = Bound::new(
                py,
                Game {
                    cached: true,
                    valid: true,
                },
            )
            .expect("this should not happen");
            Game::__repr__(&game)
        });
        assert_eq!(result, "Game(N/A@N/A)");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_empty_map_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\g_gametype\4".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let game = Bound::new(
                py,
                Game {
                    cached: true,
                    valid: true,
                },
            )
            .expect("this should not happen");
            Game::__repr__(&game)
        });
        assert_eq!(result, "Game(N/A@N/A)");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_empty_gametype_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\mapname\thunderstruck".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let game = Bound::new(
                py,
                Game {
                    cached: true,
                    valid: true,
                },
            )
            .expect("this should not happen");
            Game::__repr__(&game)
        });
        assert_eq!(result, "Game(N/A@N/A)");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_nonempty_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\mapname\thunderstruck\g_gametype\4".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let game = Bound::new(
                py,
                Game {
                    cached: true,
                    valid: true,
                },
            )
            .expect("this should not happen");
            Game::__repr__(&game)
        });
        assert_eq!(result, "Game(Clan Arena@thunderstruck)");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_when_no_main_engine_loaded() {
        MAIN_ENGINE.store(None);

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };
            game.__str__(py)
        });
        assert_eq!(result, "Invalid game");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_empty_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };
            game.__str__(py)
        });
        assert_eq!(result, "Invalid game");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_empty_map_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\g_gametype\4".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };
            game.__str__(py)
        });
        assert_eq!(result, "Invalid game");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_empty_gametype_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\mapname\thunderstruck".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };
            game.__str__(py)
        });
        assert_eq!(result, "Invalid game");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_nonempty_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\mapname\thunderstruck\g_gametype\4".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };
            game.__str__(py)
        });
        assert_eq!(result, "Clan Arena on thunderstruck");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.__contains__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_configstring_variables_are_unparseable() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.__contains__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_value_is_in_configstring_variables() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\asdf\12".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.__contains__(py, "asdf")
        });
        assert_eq!(result.expect("result was not OK"), true);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_value_is_not_in_configstring_variables() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\asdf\12".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.__contains__(py, "qwertz")
        });
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_configstring_parses_empty() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.__contains__(py, "asdf")
        });
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_configstring_parses_to_none() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "qwertz".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.__contains__(py, "asdf")
        });
        assert_eq!(result.expect("result was not OK"), false);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.__getitem__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)),);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_when_configstring_variables_are_unparseable() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.__getitem__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_when_value_is_in_configstring_variables() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\asdf\12".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.__getitem__(py, "asdf")
        });
        assert_eq!(result.expect("result was not OK"), "12");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_when_value_is_not_in_configstring_variables() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\asdf\12".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.__getitem__(py, "qwertz");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_when_configstring_parses_empty() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.__getitem__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitems_when_configstring_parses_to_none() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "qwertz".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.__getitem__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvars_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let cvars_result = game.get_cvars(py);
            assert!(cvars_result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvars_with_empty_configstring() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| "".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let cvars_result = game.get_cvars(py);
            assert!(cvars_result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvars_contains_parsed_configstring_zero() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\asdf\42".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let cvars_result = game.get_cvars(py);
            assert!(
                cvars_result.is_ok_and(|cvars| cvars.get_item("asdf").is_ok_and(|opt_value| {
                    opt_value.is_some_and(|value| {
                        value.extract::<String>().expect("this should not happen") == "42"
                    })
                }))
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_type(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_for_unparseable_gametype() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\g_gametype\asdf".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_type(py);
            assert_eq!(result.expect("result was not OK"), "unknown");
        });
    }

    #[rstest]
    #[case(0, "Free for All")]
    #[case(1, "Duel")]
    #[case(2, "Race")]
    #[case(3, "Team Deathmatch")]
    #[case(4, "Clan Arena")]
    #[case(5, "Capture the Flag")]
    #[case(6, "One Flag")]
    #[case(8, "Harvester")]
    #[case(9, "Freeze Tag")]
    #[case(10, "Domination")]
    #[case(11, "Attack and Defend")]
    #[case(12, "Red Rover")]
    #[case(- 1, "unknown")]
    #[case(13, "unknown")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_returns_parsed_long_factory_name(
        #[case] g_gametype: i32,
        #[case] expected_string: &str,
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(move |_| format!(r"\g_gametype\{}", g_gametype));
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_type(py);
            assert_eq!(result.expect("result was not OK"), expected_string);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_short_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_type_short(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_short_for_unparseable_gametype() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\g_gametype\asdf".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_type_short(py);
            assert_eq!(result.expect("result was not OK"), "N/A");
        });
    }

    #[rstest]
    #[case(0, "ffa")]
    #[case(1, "duel")]
    #[case(2, "race")]
    #[case(3, "tdm")]
    #[case(4, "ca")]
    #[case(5, "ctf")]
    #[case(6, "1f")]
    #[case(8, "har")]
    #[case(9, "ft")]
    #[case(10, "dom")]
    #[case(11, "ad")]
    #[case(12, "rr")]
    #[case(- 1, "N/A")]
    #[case(13, "N/A")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_short_returns_parsed_long_factory_name(
        #[case] g_gametype: i32,
        #[case] expected_string: &str,
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(move |_| format!(r"\g_gametype\{}", g_gametype));
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_type_short(py);
            assert_eq!(result.expect("result was not OK"), expected_string);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_map_returns_current_map() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\mapname\thunderstruck".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_map(py);
            assert_eq!(result.expect("result was not OK"), "thunderstruck");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_map_changes_current_map() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("map campgrounds"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_map(py, "campgrounds")
                .expect("this should not happen");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_map_title_gets_current_map(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            py.run_bound(
                r#"
import _shinqlx
_shinqlx._map_title = "eyetoeye"
            "#,
                None,
                None,
            )
            .expect("this should not happen");
            let game = Game {
                cached: true,
                valid: true,
            };

            assert_eq!(
                game.get_map_title(py).expect("result was not OK"),
                "eyetoeye"
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_map_subtitle1_gets_current_subtitle1(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            py.run_bound(
                r#"
import _shinqlx
_shinqlx._map_subtitle1 = "Clan Arena"
            "#,
                None,
                None,
            )
            .expect("this should not happen");
            let game = Game {
                cached: true,
                valid: true,
            };

            assert_eq!(
                game.get_map_subtitle1(py).expect("result was not OK"),
                "Clan Arena"
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_map_subtitle2_gets_current_subtitle2(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            py.run_bound(
                r#"
import _shinqlx
_shinqlx._map_subtitle2 = "Awesome map!"
            "#,
                None,
                None,
            )
            .expect("this should not happen");
            let game = Game {
                cached: true,
                valid: true,
            };

            assert_eq!(
                game.get_map_subtitle2(py).expect("result was not OK"),
                "Awesome map!"
            );
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_red_score_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_red_score(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_red_score_returns_red_score() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(6))
            .returning(|_| "7".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_red_score(py);
            assert_eq!(result.expect("result was not OK"), 7);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_red_score_defaults_when_unpareable() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(6))
            .returning(|_| "asdf".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_red_score(py);
            assert_eq!(result.expect("result was not OK"), 0);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_blue_score_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_blue_score(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_blue_score_returns_blue_score() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(7))
            .returning(|_| "5".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_blue_score(py);
            assert_eq!(result.expect("result was not OK"), 5);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_blue_score_defaults_when_unparsable() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(7))
            .returning(|_| "asdf".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_blue_score(py);
            assert_eq!(result.expect("result was not OK"), 0);
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_state_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_state(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[case("PRE_GAME", "warmup")]
    #[case("COUNT_DOWN", "countdown")]
    #[case("IN_PROGRESS", "in_progress")]
    #[case("ASDF", "ASDF")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_state_converts_gamestate_cvar(
        #[case] cvar_value: String,
        #[case] expected_return: &str,
    ) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(move |_| format!(r"\g_gameState\{}", cvar_value));
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_state(py)
        });
        assert_eq!(result.expect("result was not OK"), expected_return);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_factory_with_no_main_engine() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_factory(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_factory_returns_factory() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\g_factory\ca".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_factory(py)
        });
        assert_eq!(result.expect("result was not OK"), "ca");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_factory_sets_factory_and_reloads() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\mapname\theatreofpain".to_string());
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("map theatreofpain ffa"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_factory(py, "ffa".to_string())
                .expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_hostname_returns_hostname() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\sv_hostname\Awesome server!".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_hostname(py)
        });
        assert_eq!(result.expect("result was not OK"), "Awesome server!");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_hostname_sets_new_hostname() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_hostname"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| {
                cvar == "sv_hostname" && value == "More awesome server!" && flags.is_none()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_hostname(py, "More awesome server!".to_string())
                .expect("this should not happen");
        });
    }

    #[rstest]
    #[case(0, false)]
    #[case(1, true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_instagib_returns_instagib_setting(#[case] mode: i32, #[case] expected: bool) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(move |_| format!(r"\g_instagib\{}", mode));
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_instagib(py)
        });
        assert_eq!(result.expect("result was not OK"), expected);
    }

    #[rstest]
    #[case("0", false)]
    #[case("1", true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_instagib_with_bool_value(#[case] instagib: &'static str, #[case] value_set: bool) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_instagib"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(move |cvar, value, flags| {
                cvar == "g_instagib" && value == instagib && flags.is_none()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_instagib(py, value_set.into_py(py))
                .expect("this should not happen");
        });
    }

    #[rstest]
    #[case("0", 0)]
    #[case("1", 1)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_instagib_with_integer_value(#[case] instagib: &'static str, #[case] value_set: i32) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_instagib"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(move |cvar, value, flags| {
                cvar == "g_instagib" && value == instagib && flags.is_none()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_instagib(py, value_set.into_py(py))
                .expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_instagib_with_invalid_value() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_instagib"))
            .times(0);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, _value, _flags| cvar == "g_instagib")
            .times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.set_instagib(py, "asdf".into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[case(0, false)]
    #[case(1, true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_loadout_returns_instagib_setting(#[case] mode: i32, #[case] expected: bool) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(move |_| format!(r"\g_loadout\{}", mode));
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_loadout(py)
        });
        assert_eq!(result.expect("result was not OK"), expected);
    }

    #[rstest]
    #[case("0", false)]
    #[case("1", true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_loadout_with_bool_value(#[case] loadout: &'static str, #[case] value_set: bool) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_loadout"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(move |cvar, value, flags| {
                cvar == "g_loadout" && value == loadout && flags.is_none()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_loadout(py, value_set.into_py(py))
                .expect("this should not happen");
        });
    }

    #[rstest]
    #[case("0", 0)]
    #[case("1", 1)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_loadout_with_integer_value(#[case] loadout: &'static str, #[case] value_set: i32) {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_loadout"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(move |cvar, value, flags| {
                cvar == "g_loadout" && value == loadout && flags.is_none()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_loadout(py, value_set.into_py(py))
                .expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_loadout_with_invalid_value() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("g_loadout"))
            .times(0);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, _value, _flags| cvar == "g_loadout")
            .times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.set_loadout(py, "asdf".into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_maxclients_returns_maxclients() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\sv_maxclients\8".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_maxclients(py)
        });
        assert_eq!(result.expect("result was not OK"), 8);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_maxclients_sets_new_maxclients_value() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_maxclients"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| cvar == "sv_maxclients" && value == "32" && flags.is_none())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_maxclients(py, 32).expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_timelimit_returns_timelimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\timelimit\20".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_timelimit(py)
        });
        assert_eq!(result.expect("result was not OK"), 20);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_timelimit_sets_new_timelimit_value() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("timelimit"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| cvar == "timelimit" && value == "30" && flags.is_none())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_timelimit(py, 30).expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_fraglimit_returns_fraglimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\fraglimit\10".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_fraglimit(py)
        });
        assert_eq!(result.expect("result was not OK"), 10);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_fraglimit_sets_new_fraglimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("fraglimit"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| cvar == "fraglimit" && value == "20" && flags.is_none())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_fraglimit(py, 20).expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_roundlimit_returns_roundlimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\roundlimit\11".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_roundlimit(py)
        });
        assert_eq!(result.expect("result was not OK"), 11);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_roundlimit_sets_new_roundlimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("roundlimit"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| cvar == "roundlimit" && value == "13" && flags.is_none())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_roundlimit(py, 13).expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_roundtimelimit_returns_roundtimelimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\roundtimelimit\240".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_roundtimelimit(py)
        });
        assert_eq!(result.expect("result was not OK"), 240);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_roundtimelimit_sets_new_roundtimelimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("roundtimelimit"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| {
                cvar == "roundtimelimit" && value == "150" && flags.is_none()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_roundtimelimit(py, 150)
                .expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_scorelimit_returns_scorelimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\scorelimit\10".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_scorelimit(py)
        });
        assert_eq!(result.expect("result was not OK"), 10);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_scorelimit_sets_new_scorelimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("scorelimit"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| cvar == "scorelimit" && value == "8" && flags.is_none())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_scorelimit(py, 8).expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_capturelimit_returns_capturelimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\capturelimit\10".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_capturelimit(py)
        });
        assert_eq!(result.expect("result was not OK"), 10);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_capturelimit_sets_new_capturelimit() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("capturelimit"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| cvar == "capturelimit" && value == "20" && flags.is_none())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_capturelimit(py, 20)
                .expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_teamsize_returns_teamsize() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\teamsize\4".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_teamsize(py)
        });
        assert_eq!(result.expect("result was not OK"), 4);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_teamsize_sets_new_teamsize() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("teamsize"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| cvar == "teamsize" && value == "8" && flags.is_none())
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_teamsize(py, 8).expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_tags_returns_tags() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(0))
            .returning(|_| r"\sv_tags\tag1,tag2,tag3".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.get_tags(py)
        });
        assert_eq!(
            result.expect("result was not OK"),
            vec!["tag1", "tag2", "tag3"]
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_tags_with_string_tags() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_tags"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| {
                cvar == "sv_tags" && value == "tag1,tag2,tag3" && flags.is_none()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_tags(py, "tag1,tag2,tag3".into_py(py))
                .expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_tags_with_iterable_tags() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_tags"))
            .times(1);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, value, flags| {
                cvar == "sv_tags" && value == "tag1,tag2,tag3" && flags.is_none()
            })
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_tags(py, ["tag1", "tag2", "tag3"].into_py(py))
                .expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_tags_with_invalid_value() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_find_cvar()
            .with(predicate::eq("sv_tags"))
            .times(0);
        mock_engine
            .expect_get_cvar()
            .withf(|cvar, _value, _flags| cvar == "sv_tags")
            .times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.set_tags(py, 42.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_workshop_items_returns_workshop_items() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_get_configstring()
            .with(predicate::eq(715))
            .returning(|_| "1234 5678 9101".to_string());
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            let game = Game {
                cached: true,
                valid: true,
            };

            game.get_workshop_items(py)
        });
        assert_eq!(result.expect("result was not OK"), vec![1234, 5678, 9101]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_workshop_items_with_iterable_items() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_set_configstring()
            .with(predicate::eq(715), predicate::eq("1234 5678 9101"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            game.set_workshop_items(py, [1234, 5678, 9101].into_py(py))
                .expect("this should not happen");
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_workshop_items_with_invalid_value() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_set_configstring()
            .with(predicate::eq(715), predicate::always())
            .times(0);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.set_workshop_items(py, 42.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn shuffle_forces_shuffle() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("forceshuffle"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::shuffle(&py.get_type_bound::<Game>(), py));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn timeout_pauses_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("timeout"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::timeout(&py.get_type_bound::<Game>(), py));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn timein_unpauses_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("timein"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::timein(&py.get_type_bound::<Game>(), py));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn allready_readies_all_players() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("allready"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::allready(&py.get_type_bound::<Game>(), py));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pause_pauses_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("pause"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::pause(&py.get_type_bound::<Game>(), py));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unpause_unpauses_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("unpause"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::unpause(&py.get_type_bound::<Game>(), py));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::lock(
                &py.get_type_bound::<Game>(),
                py,
                Some("invalid_team".to_string()),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_with_no_team() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("lock"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::lock(&py.get_type_bound::<Game>(), py, None));
        assert!(result.is_ok());
    }

    #[rstest]
    #[case("red")]
    #[case("RED")]
    #[case("free")]
    #[case("blue")]
    #[case("spectator")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_a_specific_team(#[case] locked_team: &str) {
        let lock_cmd = format!("lock {}", locked_team.to_lowercase());
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .withf(move |cmd| cmd == lock_cmd)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Game::lock(
                &py.get_type_bound::<Game>(),
                py,
                Some(locked_team.to_string()),
            )
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::unlock(
                &py.get_type_bound::<Game>(),
                py,
                Some("invalid_team".to_string()),
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_no_team() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("unlock"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::unlock(&py.get_type_bound::<Game>(), py, None));
        assert!(result.is_ok());
    }

    #[rstest]
    #[case("red")]
    #[case("RED")]
    #[case("free")]
    #[case("blue")]
    #[case("spectator")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_a_specific_team(#[case] locked_team: &str) {
        let unlock_cmd = format!("unlock {}", locked_team.to_lowercase());
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .withf(move |cmd| cmd == unlock_cmd)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Game::unlock(
                &py.get_type_bound::<Game>(),
                py,
                Some(locked_team.to_string()),
            )
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::put(
                &py.get_type_bound::<Game>(),
                py,
                2.into_py(py),
                "invalid team",
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::put(&py.get_type_bound::<Game>(), py, 2048.into_py(py), "red");
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
    fn put_put_player_on_a_specific_team(#[case] new_team: &str) {
        let put_cmd = format!("put 2 {}", new_team.to_lowercase());
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .withf(move |cmd| cmd == put_cmd)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Game::put(&py.get_type_bound::<Game>(), py, 2.into_py(py), new_team)
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::mute(&py.get_type_bound::<Game>(), py, 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_mutes_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("mute 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Game::mute(&py.get_type_bound::<Game>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::unmute(&py.get_type_bound::<Game>(), py, 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_unmutes_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("unmute 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Game::unmute(&py.get_type_bound::<Game>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::tempban(&py.get_type_bound::<Game>(), py, 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_tempbans_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("tempban 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Game::tempban(&py.get_type_bound::<Game>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::ban(&py.get_type_bound::<Game>(), py, 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_bans_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("ban 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Game::ban(&py.get_type_bound::<Game>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::unban(&py.get_type_bound::<Game>(), py, 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_unbans_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("unban 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Game::unban(&py.get_type_bound::<Game>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn opsay_sends_op_message() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("opsay asdf"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::opsay(&py.get_type_bound::<Game>(), py, "asdf"));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::addadmin(&py.get_type_bound::<Game>(), py, 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_adds_player_to_admins() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("addadmin 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Game::addadmin(&py.get_type_bound::<Game>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::addmod(&py.get_type_bound::<Game>(), py, 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_adds_player_to_moderators() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("addmod 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Game::addmod(&py.get_type_bound::<Game>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::demote(&py.get_type_bound::<Game>(), py, 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_demotes_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("demote 2"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Game::demote(&py.get_type_bound::<Game>(), py, 2.into_py(py)));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn abort_aborts_game() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("map_restart"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| Game::abort(&py.get_type_bound::<Game>(), py));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_with_invalid_player() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::addscore(&py.get_type_bound::<Game>(), py, 2048.into_py(py), 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_adds_score_to_player() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("addscore 2 42"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Game::addscore(&py.get_type_bound::<Game>(), py, 2.into_py(py), 42)
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addteamscore_with_invalid_team() {
        MAIN_ENGINE.store(None);

        Python::with_gil(|py| {
            let result = Game::addteamscore(&py.get_type_bound::<Game>(), py, "invalid_team", 42);
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
    fn addteamscore_adds_score_to_team(#[case] locked_team: &str) {
        let unlock_cmd = format!("addteamscore {} 42", locked_team.to_lowercase());
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .withf(move |cmd| cmd == unlock_cmd)
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result = Python::with_gil(|py| {
            Game::addteamscore(&py.get_type_bound::<Game>(), py, locked_team, 42)
        });
        assert!(result.is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setmatchtime_sets_match_time() {
        let mut mock_engine = MockQuakeEngine::new();
        mock_engine
            .expect_execute_console_command()
            .with(predicate::eq("setmatchtime 42"))
            .times(1);
        MAIN_ENGINE.store(Some(mock_engine.into()));

        let result =
            Python::with_gil(|py| Game::setmatchtime(&py.get_type_bound::<Game>(), py, 42));
        assert!(result.is_ok());
    }
}
