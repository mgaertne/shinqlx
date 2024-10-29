use super::prelude::*;
use super::{
    addadmin, addmod, addscore, addteamscore, ban, console_command, demote, lock, mute, opsay, put,
    set_teamsize, tempban, unban, unlock, unmute,
};

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
    #[pyo3(signature = (cached = true), text_signature = "(cached = true)")]
    pub(crate) fn py_new(py: Python<'_>, cached: bool) -> PyResult<Self> {
        py.allow_threads(|| {
            MAIN_ENGINE.load().as_ref().map_or(
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                )),
                |main_engine| {
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
                },
            )
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
            MAIN_ENGINE.load().as_ref().map_or(
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                )),
                |main_engine| {
                    let configstring = main_engine.get_configstring(CS_SERVERINFO as u16);

                    if configstring.is_empty() {
                        self.valid = false;
                        return Err(NonexistentGameError::new_err(
                            "Invalid game. Is the server loading a new map?",
                        ));
                    }

                    Ok(parse_variables(&configstring).get(item).is_some())
                },
            )
        })
    }

    fn __getitem__(&mut self, py: Python<'_>, item: &str) -> PyResult<String> {
        py.allow_threads(|| {
            MAIN_ENGINE.load().as_ref().map_or(
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                )),
                |main_engine| {
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
                },
            )
        })
    }

    /// A dictionary of unprocessed cvars. Use attributes whenever possible, but since some cvars
    /// might not have attributes on this class, this could be useful.
    #[getter(cvars)]
    fn get_cvars<'b>(&mut self, py: Python<'b>) -> PyResult<Bound<'b, PyDict>> {
        py.allow_threads(|| {
            MAIN_ENGINE.load().as_ref().map_or(
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                )),
                |main_engine| {
                    let configstring = main_engine.get_configstring(CS_SERVERINFO as u16);
                    if configstring.is_empty() {
                        self.valid = false;
                        return Err(NonexistentGameError::new_err(
                            "Invalid game. Is the server loading a new map?",
                        ));
                    }
                    Ok(parse_variables(&configstring))
                },
            )
        })
        .map(|parsed_variables| parsed_variables.into_py_dict_bound(py))
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
        py.allow_threads(|| {
            let mapchange_command = format!("map {}", value);
            console_command(&mapchange_command)
        })
    }

    /// The full name of the map. Ex.: ``Longest Yard``.
    #[getter(map_title)]
    fn get_map_title(&self, py: Python<'_>) -> PyResult<String> {
        let base_module = py.import_bound("shinqlx")?;
        let map_title = base_module.getattr("_map_title")?;
        map_title.extract::<String>()
    }

    /// The map's subtitle. Usually either empty or has the author's name.
    #[getter(map_subtitle1)]
    fn get_map_subtitle1(&self, py: Python<'_>) -> PyResult<String> {
        let base_module = py.import_bound("shinqlx")?;
        let map_title = base_module.getattr("_map_subtitle1")?;
        map_title.extract::<String>()
    }

    /// The map's second subtitle. Usually either empty or has the author's name.
    #[getter(map_subtitle2)]
    fn get_map_subtitle2(&self, py: Python<'_>) -> PyResult<String> {
        let base_module = py.import_bound("shinqlx")?;
        let map_title = base_module.getattr("_map_subtitle2")?;
        map_title.extract::<String>()
    }

    #[getter(red_score)]
    fn get_red_score(&self, py: Python<'_>) -> PyResult<i32> {
        py.allow_threads(|| {
            MAIN_ENGINE.load().as_ref().map_or(
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                )),
                |main_engine| {
                    let configstring = main_engine.get_configstring(CS_SCORES1 as u16);
                    Ok(configstring.parse::<i32>().unwrap_or_default())
                },
            )
        })
    }

    #[getter(blue_score)]
    fn get_blue_score(&self, py: Python<'_>) -> PyResult<i32> {
        py.allow_threads(|| {
            MAIN_ENGINE.load().as_ref().map_or(
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                )),
                |main_engine| {
                    let configstring = main_engine.get_configstring(CS_SCORES2 as u16);
                    Ok(configstring.parse::<i32>().unwrap_or_default())
                },
            )
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
        let mapname = self.get_map(py)?;
        py.allow_threads(|| {
            let mapchange_command = format!("map {mapname} {value}");
            console_command(&mapchange_command)
        })
    }

    #[getter(factory_title)]
    fn get_factory_title(&mut self, py: Python<'_>) -> PyResult<String> {
        self.__getitem__(py, "g_factoryTitle")
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
        pyshinqlx_set_cvar(py, "g_instagib", string_cvar_value, None).map(|_| ())
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
        pyshinqlx_set_cvar(py, "g_loadout", string_cvar_value, None).map(|_| ())
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
    fn set_teamsize(&mut self, py: Python<'_>, value: i32) -> PyResult<()> {
        py.allow_threads(|| set_teamsize(value))
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
        pyshinqlx_set_cvar(py, "sv_tags", &string_cvar_value, None).map(|_| ())
    }

    #[getter(workshop_items)]
    fn get_workshop_items(&self, py: Python<'_>) -> PyResult<Vec<u64>> {
        py.allow_threads(|| {
            MAIN_ENGINE.load().as_ref().map_or(
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                )),
                |main_engine| {
                    let configstring = main_engine.get_configstring(CS_STEAM_WORKSHOP_IDS as u16);
                    Ok(configstring
                        .split(' ')
                        .filter_map(|value| value.parse::<u64>().ok())
                        .collect())
                },
            )
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
            MAIN_ENGINE.load().as_ref().map_or(
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                )),
                |main_engine| {
                    main_engine.set_configstring(CS_STEAM_WORKSHOP_IDS as i32, &workshop_items_str);
                    Ok(())
                },
            )
        })
    }

    #[classmethod]
    pub(crate) fn shuffle(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("forceshuffle"))
    }

    #[classmethod]
    fn timeout(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("timeout"))
    }

    #[classmethod]
    fn timein(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("timein"))
    }

    #[classmethod]
    fn allready(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("allready"))
    }

    #[classmethod]
    fn pause(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("pause"))
    }

    #[classmethod]
    fn unpause(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("unpause"))
    }

    #[classmethod]
    #[pyo3(signature = (team = None), text_signature = "(team = None)")]
    fn lock(cls: &Bound<'_, PyType>, team: Option<&str>) -> PyResult<()> {
        cls.py().allow_threads(|| lock(team))
    }

    #[classmethod]
    #[pyo3(signature = (team = None), text_signature = "(team = None)")]
    fn unlock(cls: &Bound<'_, PyType>, team: Option<&str>) -> PyResult<()> {
        cls.py().allow_threads(|| unlock(team))
    }

    #[classmethod]
    fn put(cls: &Bound<'_, PyType>, player: Py<PyAny>, team: &str) -> PyResult<()> {
        put(cls.py(), player, team)
    }

    #[classmethod]
    fn mute(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        mute(cls.py(), player)
    }

    #[classmethod]
    fn unmute(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        unmute(cls.py(), player)
    }

    #[classmethod]
    fn tempban(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        tempban(cls.py(), player)
    }

    #[classmethod]
    fn ban(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        ban(cls.py(), player)
    }

    #[classmethod]
    fn unban(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        unban(cls.py(), player)
    }

    #[classmethod]
    fn opsay(cls: &Bound<'_, PyType>, msg: &str) -> PyResult<()> {
        cls.py().allow_threads(|| opsay(msg))
    }

    #[classmethod]
    fn addadmin(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        addadmin(cls.py(), player)
    }

    #[classmethod]
    fn addmod(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        addmod(cls.py(), player)
    }

    #[classmethod]
    fn demote(cls: &Bound<'_, PyType>, player: PyObject) -> PyResult<()> {
        demote(cls.py(), player)
    }

    #[classmethod]
    fn abort(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("map_restart"))
    }

    #[classmethod]
    fn addscore(cls: &Bound<'_, PyType>, player: PyObject, score: i32) -> PyResult<()> {
        addscore(cls.py(), player, score)
    }

    #[classmethod]
    fn addteamscore(cls: &Bound<'_, PyType>, team: &str, score: i32) -> PyResult<()> {
        cls.py().allow_threads(|| addteamscore(team, score))
    }

    #[classmethod]
    fn setmatchtime(cls: &Bound<'_, PyType>, time: i32) -> PyResult<()> {
        cls.py().allow_threads(|| {
            let setmatchtime_cmd = format!("setmatchtime {}", time);
            console_command(&setmatchtime_cmd)
        })
    }
}

#[cfg(test)]
mod pyshinqlx_game_tests {
    use super::NonexistentGameError;
    use crate::ffi::c::prelude::{CS_SCORES1, CS_SCORES2, CS_SERVERINFO, CS_STEAM_WORKSHOP_IDS};
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use pyo3::exceptions::{PyEnvironmentError, PyKeyError, PyValueError};
    use rstest::rstest;

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pyconstructor_when_no_main_engine_loaded(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::py_new(py, true);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pyconstructor_with_empty_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1)
        .run(|| {
            Python::with_gil(|py| {
                let result = Game::py_new(py, true);
                assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pyconstructor_with_nonempty_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "asdf", 1)
            .run(|| {
                let result = Python::with_gil(|py| Game::py_new(py, true));
                assert_eq!(
                    result.expect("result was not OK"),
                    Game {
                        cached: true,
                        valid: true,
                    }
                );
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_when_no_main_engine_loaded(_pyshinqlx_setup: ()) {
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_empty_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1..)
            .run(|| {
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
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_empty_map_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gametype\4", 1..)
            .run(|| {
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
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_empty_gametype_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\mapname\thunderstruck", 1..)
            .run(|| {
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
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_nonempty_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(
                CS_SERVERINFO as u16,
                r"\mapname\thunderstruck\g_gametype\4",
                1..,
            )
            .run(|| {
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
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_when_no_main_engine_loaded(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };
            game.__str__(py)
        });
        assert_eq!(result, "Invalid game");
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_empty_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };
                    game.__str__(py)
                });
                assert_eq!(result, "Invalid game");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_empty_map_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gametype\4", 1..)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };
                    game.__str__(py)
                });
                assert_eq!(result, "Invalid game");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_empty_gametype_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\mapname\thunderstruck", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };
                    game.__str__(py)
                });
                assert_eq!(result, "Invalid game");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_nonempty_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(
                CS_SERVERINFO as u16,
                r"\mapname\thunderstruck\g_gametype\4",
                1..,
            )
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };
                    game.__str__(py)
                });
                assert_eq!(result, "Clan Arena on thunderstruck");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.__contains__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_configstring_variables_are_unparseable(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1)
        .run(|| {
            Python::with_gil(|py| {
                let mut game = Game {
                    cached: true,
                    valid: true,
                };

                let result = game.__contains__(py, "asdf");
                assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_value_is_in_configstring_variables(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\asdf\12", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.__contains__(py, "asdf")
                });
                assert_eq!(result.expect("result was not OK"), true);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_value_is_not_in_configstring_variables(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\asdf\12", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.__contains__(py, "qwertz")
                });
                assert_eq!(result.expect("result was not OK"), false);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_configstring_parses_empty(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.__contains__(py, "asdf")
                });
                assert_eq!(result.expect("result was not OK"), false);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_configstring_parses_to_none(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "qwertz", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.__contains__(py, "asdf")
                });
                assert_eq!(result.expect("result was not OK"), false);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.__getitem__(py, "asdf");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)),);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_when_configstring_variables_are_unparseable(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1)
        .run(|| {
            Python::with_gil(|py| {
                let mut game = Game {
                    cached: true,
                    valid: true,
                };

                let result = game.__getitem__(py, "asdf");
                assert!(result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_when_value_is_in_configstring_variables(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\asdf\12", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.__getitem__(py, "asdf")
                });
                assert_eq!(result.expect("result was not OK"), "12");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_when_value_is_not_in_configstring_variables(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\asdf\12", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.__getitem__(py, "qwertz");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_when_configstring_parses_empty(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.__getitem__(py, "asdf");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitems_when_configstring_parses_to_none(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "qwertz", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.__getitem__(py, "asdf");
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvars_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let cvars_result = game.get_cvars(py);
            assert!(cvars_result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvars_with_empty_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let cvars_result = game.get_cvars(py);
                    assert!(cvars_result
                        .is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvars_contains_parsed_configstring_zero(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\asdf\42", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let cvars_result = game.get_cvars(py);
                    assert!(
                        cvars_result.is_ok_and(|cvars| cvars.get_item("asdf").is_ok_and(
                            |opt_value| {
                                opt_value.is_some_and(|value| {
                                    value.extract::<String>().expect("this should not happen")
                                        == "42"
                                })
                            }
                        ))
                    );
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_type(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_for_unparseable_gametype(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gametype\asdf", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.get_type(py);
                    assert_eq!(result.expect("result was not OK"), "unknown");
                });
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
        _pyshinqlx_setup: (),
        #[case] g_gametype: i32,
        #[case] expected_string: &str,
    ) {
        MockEngineBuilder::default()
            .with_get_configstring(
                CS_SERVERINFO as u16,
                format!(r"\g_gametype\{g_gametype}"),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.get_type(py);
                    assert_eq!(result.expect("result was not OK"), expected_string);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_short_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_type_short(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_short_for_unparseable_gametype(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gametype\asdf", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.get_type_short(py);
                    assert_eq!(result.expect("result was not OK"), "N/A");
                });
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
        _pyshinqlx_setup: (),
        #[case] g_gametype: i32,
        #[case] expected_string: &str,
    ) {
        MockEngineBuilder::default()
            .with_get_configstring(
                CS_SERVERINFO as u16,
                format!(r"\g_gametype\{g_gametype}"),
                1,
            )
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.get_type_short(py);
                    assert_eq!(result.expect("result was not OK"), expected_string);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_map_returns_current_map(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\mapname\thunderstruck", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.get_map(py);
                    assert_eq!(result.expect("result was not OK"), "thunderstruck");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_map_changes_current_map(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("map campgrounds", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_map(py, "campgrounds")
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_map_title_gets_current_map(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            py.run_bound(
                r#"
import shinqlx
shinqlx._map_title = "eyetoeye"
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
import shinqlx
shinqlx._map_subtitle1 = "Clan Arena"
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
import shinqlx
shinqlx._map_subtitle2 = "Awesome map!"
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_red_score_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_red_score(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_red_score_returns_red_score(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SCORES1 as u16, "7", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.get_red_score(py);
                    assert_eq!(result.expect("result was not OK"), 7);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_red_score_defaults_when_unpareable(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SCORES1 as u16, "asdf", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.get_red_score(py);
                    assert_eq!(result.expect("result was not OK"), 0);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_blue_score_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_blue_score(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_blue_score_returns_blue_score(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SCORES2 as u16, "5", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.get_blue_score(py);
                    assert_eq!(result.expect("result was not OK"), 5);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_blue_score_defaults_when_unparsable(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SCORES2 as u16, "asdf", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.get_blue_score(py);
                    assert_eq!(result.expect("result was not OK"), 0);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_state_with_no_main_engine(_pyshinqlx_setup: ()) {
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
        _pyshinqlx_setup: (),
        #[case] cvar_value: String,
        #[case] expected_return: &str,
    ) {
        MockEngineBuilder::default()
            .with_get_configstring(
                CS_SERVERINFO as u16,
                format!(r"\g_gameState\{cvar_value}"),
                1,
            )
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_state(py)
                });
                assert_eq!(result.expect("result was not OK"), expected_return);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_factory_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_factory(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_factory_returns_factory(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_factory\ca", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_factory(py)
                });
                assert_eq!(result.expect("result was not OK"), "ca");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_factory_sets_factory_and_reloads(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("map theatreofpain ffa", 1)
            .with_get_configstring(CS_SERVERINFO as u16, r"\mapname\theatreofpain", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_factory(py, "ffa".to_string())
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_factory_title_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let mut game = Game {
                cached: true,
                valid: true,
            };

            let result = game.get_factory_title(py);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_factory_title_returns_factory_title(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_factoryTitle\Clan Arena", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_factory_title(py)
                });
                assert_eq!(result.expect("result was not OK"), "Clan Arena");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_hostname_returns_hostname(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\sv_hostname\Awesome server!", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_hostname(py)
                });
                assert_eq!(result.expect("result was not OK"), "Awesome server!");
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_hostname_sets_new_hostname(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_hostname", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "sv_hostname" && value == "More awesome server!" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_hostname(py, "More awesome server!".to_string())
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[case(0, false)]
    #[case(1, true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_instagib_returns_instagib_setting(
        _pyshinqlx_setup: (),
        #[case] mode: i32,
        #[case] expected: bool,
    ) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, format!(r"\g_instagib\{mode}"), 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_instagib(py)
                });
                assert_eq!(result.expect("result was not OK"), expected);
            });
    }

    #[rstest]
    #[case("0", false)]
    #[case("1", true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_instagib_with_bool_value(
        _pyshinqlx_setup: (),
        #[case] instagib: &'static str,
        #[case] value_set: bool,
    ) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "g_instagib", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(move |cvar, value, flags| {
                        cvar == "g_instagib" && value == instagib && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_instagib(py, value_set.into_py(py))
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[case("0", 0)]
    #[case("1", 1)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_instagib_with_integer_value(
        _pyshinqlx_setup: (),
        #[case] instagib: &'static str,
        #[case] value_set: i32,
    ) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "g_instagib", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(move |cvar, value, flags| {
                        cvar == "g_instagib" && value == instagib && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_instagib(py, value_set.into_py(py))
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_instagib_with_invalid_value(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "g_instagib", |_| None, 0)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, _value, _flags| cvar == "g_instagib")
                    .times(0);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.set_instagib(py, "asdf".into_py(py));
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[case(0, false)]
    #[case(1, true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_loadout_returns_instagib_setting(
        _pyshinqlx_setup: (),
        #[case] mode: i32,
        #[case] expected: bool,
    ) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, format!(r"\g_loadout\{mode}"), 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_loadout(py)
                });
                assert_eq!(result.expect("result was not OK"), expected);
            });
    }

    #[rstest]
    #[case("0", false)]
    #[case("1", true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_loadout_with_bool_value(
        _pyshinqlx_setup: (),
        #[case] loadout: &'static str,
        #[case] value_set: bool,
    ) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "g_loadout", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(move |cvar, value, flags| {
                        cvar == "g_loadout" && value == loadout && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_loadout(py, value_set.into_py(py))
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[case("0", 0)]
    #[case("1", 1)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_loadout_with_integer_value(
        _pyshinqlx_setup: (),
        #[case] loadout: &'static str,
        #[case] value_set: i32,
    ) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "g_loadout", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(move |cvar, value, flags| {
                        cvar == "g_loadout" && value == loadout && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_loadout(py, value_set.into_py(py))
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_loadout_with_invalid_value(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "g_loadout", |_| None, 0)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, _value, _flags| cvar == "g_loadout")
                    .times(0);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.set_loadout(py, "asdf".into_py(py));
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_maxclients_returns_maxclients(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\sv_maxclients\8", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_maxclients(py)
                });
                assert_eq!(result.expect("result was not OK"), 8);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_maxclients_sets_new_maxclients_value(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_maxclients", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "sv_maxclients" && value == "32" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_maxclients(py, 32).expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_timelimit_returns_timelimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\timelimit\20", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_timelimit(py)
                });
                assert_eq!(result.expect("result was not OK"), 20);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_timelimit_sets_new_timelimit_value(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "timelimit", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "timelimit" && value == "30" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_timelimit(py, 30).expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_fraglimit_returns_fraglimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\fraglimit\10", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_fraglimit(py)
                });
                assert_eq!(result.expect("result was not OK"), 10);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_fraglimit_sets_new_fraglimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "fraglimit", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "fraglimit" && value == "20" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_fraglimit(py, 20).expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_roundlimit_returns_roundlimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\roundlimit\11", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_roundlimit(py)
                });
                assert_eq!(result.expect("result was not OK"), 11);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_roundlimit_sets_new_roundlimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "roundlimit", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "roundlimit" && value == "13" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_roundlimit(py, 13).expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_roundtimelimit_returns_roundtimelimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\roundtimelimit\240", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_roundtimelimit(py)
                });
                assert_eq!(result.expect("result was not OK"), 240);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_roundtimelimit_sets_new_roundtimelimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "roundtimelimit", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "roundtimelimit" && value == "150" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_roundtimelimit(py, 150)
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_scorelimit_returns_scorelimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\scorelimit\10", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_scorelimit(py)
                });
                assert_eq!(result.expect("result was not OK"), 10);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_scorelimit_sets_new_scorelimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "scorelimit", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "scorelimit" && value == "8" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_scorelimit(py, 8).expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_capturelimit_returns_capturelimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\capturelimit\10", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_capturelimit(py)
                });
                assert_eq!(result.expect("result was not OK"), 10);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_capturelimit_sets_new_capturelimit(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "capturelimit", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "capturelimit" && value == "20" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.set_capturelimit(py, 20);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_teamsize_returns_teamsize(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\teamsize\4", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_teamsize(py)
                });
                assert_eq!(result.expect("result was not OK"), 4);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_teamsize_sets_new_teamsize(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "teamsize", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "teamsize" && value == "8" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_teamsize(py, 8).expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_tags_returns_tags(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\sv_tags\tag1,tag2,tag3", 1)
            .run(|| {
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
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_tags_with_string_tags(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_tags", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "sv_tags" && value == "tag1,tag2,tag3" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_tags(py, "tag1,tag2,tag3".into_py(py))
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_tags_with_iterable_tags(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_tags", |_| None, 1)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, value, flags| {
                        cvar == "sv_tags" && value == "tag1,tag2,tag3" && flags.is_none()
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_tags(py, ["tag1", "tag2", "tag3"].into_py(py))
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_tags_with_invalid_value(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_find_cvar(|cmd| cmd == "sv_tags", |_| None, 0)
            .configure(|mock_engine| {
                mock_engine
                    .expect_get_cvar()
                    .withf(|cvar, _value, _flags| cvar == "sv_tags")
                    .times(0);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.set_tags(py, 42.into_py(py));
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_workshop_items_returns_workshop_items(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_STEAM_WORKSHOP_IDS as u16, "1234 5678 9101", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    let game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.get_workshop_items(py)
                });
                assert_eq!(result.expect("result was not OK"), vec![1234, 5678, 9101]);
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_workshop_items_with_iterable_items(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(
                        predicate::eq(CS_STEAM_WORKSHOP_IDS as i32),
                        predicate::eq("1234 5678 9101"),
                    )
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    game.set_workshop_items(py, [1234, 5678, 9101].into_py(py))
                        .expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_workshop_items_with_invalid_value(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .configure(|mock_engine| {
                mock_engine
                    .expect_set_configstring()
                    .with(
                        predicate::eq(CS_STEAM_WORKSHOP_IDS as i32),
                        predicate::always(),
                    )
                    .times(0);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let mut game = Game {
                        cached: true,
                        valid: true,
                    };

                    let result = game.set_workshop_items(py, 42.into_py(py));
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn shuffle_forces_shuffle(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("forceshuffle", 1)
            .run(|| {
                let result = Python::with_gil(|py| Game::shuffle(&py.get_type_bound::<Game>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn timeout_pauses_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("timeout", 1)
            .run(|| {
                let result = Python::with_gil(|py| Game::timeout(&py.get_type_bound::<Game>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn timein_unpauses_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("timein", 1)
            .run(|| {
                let result = Python::with_gil(|py| Game::timein(&py.get_type_bound::<Game>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn allready_readies_all_players(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("allready", 1)
            .run(|| {
                let result = Python::with_gil(|py| Game::allready(&py.get_type_bound::<Game>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn pause_pauses_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("pause", 1)
            .run(|| {
                let result = Python::with_gil(|py| Game::pause(&py.get_type_bound::<Game>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unpause_unpauses_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("unpause", 1)
            .run(|| {
                let result = Python::with_gil(|py| Game::unpause(&py.get_type_bound::<Game>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_with_invalid_team(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .configure(|_mock_engine| {})
            .run(|| {
                Python::with_gil(|py| {
                    let result = Game::lock(&py.get_type_bound::<Game>(), Some("invalid_team"));
                    assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn lock_with_no_team(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("lock", 1)
            .run(|| {
                let result = Python::with_gil(|py| Game::lock(&py.get_type_bound::<Game>(), None));
                assert!(result.is_ok());
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
    fn lock_a_specific_team(_pyshinqlx_setup: (), #[case] locked_team: &str) {
        MockEngineBuilder::default()
            .with_execute_console_command(format!("lock {}", locked_team.to_lowercase()), 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::lock(&py.get_type_bound::<Game>(), Some(locked_team))
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_invalid_team(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::with_gil(|py| {
                let result = Game::unlock(&py.get_type_bound::<Game>(), Some("invalid_team"));
                assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
            });
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_no_team(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("unlock", 1)
            .run(|| {
                let result =
                    Python::with_gil(|py| Game::unlock(&py.get_type_bound::<Game>(), None));
                assert!(result.is_ok());
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
    fn unlock_a_specific_team(_pyshinqlx_setup: (), #[case] locked_team: &str) {
        MockEngineBuilder::default()
            .with_execute_console_command(format!("unlock {}", locked_team.to_lowercase()), 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::unlock(&py.get_type_bound::<Game>(), Some(locked_team))
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_team(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::put(&py.get_type_bound::<Game>(), 2.into_py(py), "invalid team");
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::put(&py.get_type_bound::<Game>(), 2048.into_py(py), "red");
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
    fn put_put_player_on_a_specific_team(_pyshinqlx_setup: (), #[case] new_team: &str) {
        MockEngineBuilder::default()
            .with_execute_console_command(format!("put 2 {}", new_team.to_lowercase()), 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::put(&py.get_type_bound::<Game>(), 2.into_py(py), new_team)
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::mute(&py.get_type_bound::<Game>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_mutes_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("mute 2", 1)
            .run(|| {
                let result =
                    Python::with_gil(|py| Game::mute(&py.get_type_bound::<Game>(), 2.into_py(py)));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::unmute(&py.get_type_bound::<Game>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_unmutes_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("unmute 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::unmute(&py.get_type_bound::<Game>(), 2.into_py(py))
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::tempban(&py.get_type_bound::<Game>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_tempbans_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("tempban 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::tempban(&py.get_type_bound::<Game>(), 2.into_py(py))
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::ban(&py.get_type_bound::<Game>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_bans_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("ban 2", 1)
            .run(|| {
                let result =
                    Python::with_gil(|py| Game::ban(&py.get_type_bound::<Game>(), 2.into_py(py)));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::unban(&py.get_type_bound::<Game>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_unbans_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("unban 2", 1)
            .run(|| {
                let result =
                    Python::with_gil(|py| Game::unban(&py.get_type_bound::<Game>(), 2.into_py(py)));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn opsay_sends_op_message(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("opsay asdf", 1)
            .run(|| {
                let result =
                    Python::with_gil(|py| Game::opsay(&py.get_type_bound::<Game>(), "asdf"));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::addadmin(&py.get_type_bound::<Game>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_adds_player_to_admins(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("addadmin 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::addadmin(&py.get_type_bound::<Game>(), 2.into_py(py))
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::addmod(&py.get_type_bound::<Game>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_adds_player_to_moderators(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("addmod 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::addmod(&py.get_type_bound::<Game>(), 2.into_py(py))
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::demote(&py.get_type_bound::<Game>(), 2048.into_py(py));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_demotes_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("demote 2", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::demote(&py.get_type_bound::<Game>(), 2.into_py(py))
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn abort_aborts_game(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("map_restart", 1)
            .run(|| {
                let result = Python::with_gil(|py| Game::abort(&py.get_type_bound::<Game>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::addscore(&py.get_type_bound::<Game>(), 2048.into_py(py), 42);
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_adds_score_to_player(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("addscore 2 42", 1)
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::addscore(&py.get_type_bound::<Game>(), 2.into_py(py), 42)
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addteamscore_with_invalid_team(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::addteamscore(&py.get_type_bound::<Game>(), "invalid_team", 42);
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
    fn addteamscore_adds_score_to_team(_pyshinqlx_setup: (), #[case] locked_team: &str) {
        MockEngineBuilder::default()
            .with_execute_console_command(
                format!("addteamscore {} 42", locked_team.to_lowercase()),
                1,
            )
            .run(|| {
                let result = Python::with_gil(|py| {
                    Game::addteamscore(&py.get_type_bound::<Game>(), locked_team, 42)
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn setmatchtime_sets_match_time(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_execute_console_command("setmatchtime 42", 1)
            .run(|| {
                let result =
                    Python::with_gil(|py| Game::setmatchtime(&py.get_type_bound::<Game>(), 42));
                assert!(result.is_ok());
            });
    }
}
