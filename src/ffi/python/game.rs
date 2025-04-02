use super::prelude::*;
use super::{
    addadmin, addmod, addscore, addteamscore, ban, console_command, demote, lock, mute, opsay, put,
    set_teamsize, tempban, unban, unlock, unmute,
};

use crate::{
    MAIN_ENGINE,
    ffi::c::prelude::{CS_SCORES1, CS_SCORES2, CS_SERVERINFO, CS_STEAM_WORKSHOP_IDS},
    quake_live_engine::{GetConfigstring, SetConfigstring},
};

use core::sync::atomic::AtomicBool;
use itertools::Itertools;
use log::*;
use std::sync::atomic::Ordering;

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
#[pyclass(module = "_game", name = "Game", frozen)]
#[derive(Debug)]
pub(crate) struct Game {
    cached: AtomicBool,
    valid: AtomicBool,
}

impl PartialEq for Game {
    fn eq(&self, other: &Self) -> bool {
        self.cached.load(Ordering::SeqCst) == other.cached.load(Ordering::SeqCst)
            && self.valid.load(Ordering::SeqCst) == other.valid.load(Ordering::SeqCst)
    }
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
                        cached: cached.into(),
                        valid: true.into(),
                    })
                },
            )
        })
    }

    fn __repr__(slf: &Bound<'_, Self>) -> String {
        let Ok(classname) = slf.get_type().qualname() else {
            return "Game(N/A@N/A)".to_string();
        };
        let Ok(factory_type) = slf.get_gametype() else {
            return format!("{}(N/A@N/A)", classname);
        };
        let Ok(mapname) = slf.get_map() else {
            return format!("{}(N/A@N/A)", classname);
        };
        format!("{}({}@{})", classname, factory_type, mapname)
    }

    fn __str__(slf: &Bound<'_, Self>) -> String {
        let Ok(factory_type) = slf.get_gametype() else {
            return "Invalid game".to_string();
        };
        let Ok(mapname) = slf.get_map() else {
            return "Invalid game".to_string();
        };
        format!("{} on {}", factory_type, mapname)
    }

    fn __contains__(slf: &Bound<'_, Self>, item: &str) -> PyResult<bool> {
        MAIN_ENGINE.load().as_ref().map_or(
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            )),
            |main_engine| {
                let configstring = main_engine.get_configstring(CS_SERVERINFO as u16);

                if configstring.is_empty() {
                    slf.borrow().valid.store(false, Ordering::SeqCst);
                    return Err(NonexistentGameError::new_err(
                        "Invalid game. Is the server loading a new map?",
                    ));
                }

                Ok(parse_variables(&configstring).get(item).is_some())
            },
        )
    }

    fn __getitem__(slf: &Bound<'_, Self>, item: &str) -> PyResult<String> {
        MAIN_ENGINE.load().as_ref().map_or(
            Err(PyEnvironmentError::new_err(
                "main quake live engine not set",
            )),
            |main_engine| {
                let configstring = main_engine.get_configstring(CS_SERVERINFO as u16);

                if configstring.is_empty() {
                    slf.borrow().valid.store(false, Ordering::SeqCst);
                    return Err(NonexistentGameError::new_err(
                        "Invalid game. Is the server loading a new map?",
                    ));
                }

                parse_variables(&configstring)
                    .get(item)
                    .map_or_else(|| Err(PyKeyError::new_err(format!("'{}'", item))), Ok)
            },
        )
    }

    #[getter(cached)]
    fn get_cached(slf: &Bound<'_, Self>) -> bool {
        slf.get_cached()
    }

    #[getter(_valid)]
    fn get_valid(slf: &Bound<'_, Self>) -> bool {
        slf.get_valid()
    }

    /// A dictionary of unprocessed cvars. Use attributes whenever possible, but since some cvars
    /// might not have attributes on this class, this could be useful.
    #[getter(cvars)]
    fn get_cvars<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyDict>> {
        slf.get_cvars()
    }

    #[getter]
    fn get_gametype(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_gametype()
    }

    #[getter(type_short)]
    fn get_gametype_short(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_gametype_short()
    }

    #[getter(map)]
    fn get_map(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_map()
    }

    #[setter(map)]
    fn set_map(slf: &Bound<'_, Self>, value: &str) -> PyResult<()> {
        slf.set_map(value)
    }

    /// The full name of the map. Ex.: ``Longest Yard``.
    #[getter(map_title)]
    fn get_map_title(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_map_title()
    }

    /// The map's subtitle. Usually either empty or has the author's name.
    #[getter(map_subtitle1)]
    fn get_map_subtitle1(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_map_subtitle1()
    }

    /// The map's second subtitle. Usually either empty or has the author's name.
    #[getter(map_subtitle2)]
    fn get_map_subtitle2(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_map_subtitle2()
    }

    #[getter(red_score)]
    fn get_red_score(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_red_score()
    }

    #[getter(blue_score)]
    fn get_blue_score(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_blue_score()
    }

    #[getter(state)]
    fn get_state(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_state()
    }

    #[getter(factory)]
    fn get_factory(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_factory()
    }

    #[setter(factory)]
    fn set_factory(slf: &Bound<'_, Self>, value: &str) -> PyResult<()> {
        slf.set_factory(value)
    }

    #[getter(factory_title)]
    fn get_factory_title(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_factory_title()
    }

    #[getter(hostname)]
    fn get_hostname(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_hostname()
    }

    #[setter(hostname)]
    fn set_hostname(slf: &Bound<'_, Self>, value: &str) -> PyResult<()> {
        slf.set_hostname(value)
    }

    #[getter(instagib)]
    fn get_instagib(slf: &Bound<'_, Self>) -> PyResult<bool> {
        slf.get_instagib()
    }

    #[setter(instagib)]
    fn set_instagib(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_instagib(value)
    }

    #[getter(loadout)]
    fn get_loadout(slf: &Bound<'_, Self>) -> PyResult<bool> {
        slf.get_loadout()
    }

    #[setter(loadout)]
    fn set_loadout(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_loadout(value)
    }

    #[getter(maxclients)]
    fn get_maxclients(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_maxclients()
    }

    #[setter(maxclients)]
    fn set_maxclients(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_maxclients(value)
    }

    #[getter(timelimit)]
    fn get_timelimit(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_timelimit()
    }

    #[setter(timelimit)]
    fn set_timelimit(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_timelimit(value)
    }

    #[getter(fraglimit)]
    fn get_fraglimit(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_fraglimit()
    }

    #[setter(fraglimit)]
    fn set_fraglimit(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_fraglimit(value)
    }

    #[getter(roundlimit)]
    fn get_roundlimit(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_roundlimit()
    }

    #[setter(roundlimit)]
    fn set_roundlimit(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_roundlimit(value)
    }

    #[getter(roundtimelimit)]
    fn get_roundtimelimit(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_roundtimelimit()
    }

    #[setter(roundtimelimit)]
    fn set_roundtimelimit(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_roundtimelimit(value)
    }

    #[getter(scorelimit)]
    fn get_scorelimit(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_scorelimit()
    }

    #[setter(scorelimit)]
    fn set_scorelimit(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_scorelimit(value)
    }

    #[getter(capturelimit)]
    fn get_capturelimit(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_capturelimit()
    }

    #[setter(capturelimit)]
    fn set_capturelimit(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_capturelimit(value)
    }

    #[getter(teamsize)]
    fn get_teamsize(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_teamsize()
    }

    #[setter(teamsize)]
    fn set_teamsize(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_teamsize(value)
    }

    #[getter(tags)]
    fn get_tags(slf: &Bound<'_, Self>) -> PyResult<Vec<String>> {
        slf.get_tags()
    }

    #[setter(tags)]
    fn set_tags(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_tags(value)
    }

    #[getter(workshop_items)]
    fn get_workshop_items(slf: &Bound<'_, Self>) -> PyResult<Vec<u64>> {
        slf.get_workshop_items()
    }

    #[setter(workshop_items)]
    fn set_workshop_items(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_workshop_items(value)
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
    fn put(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>, team: &str) -> PyResult<()> {
        put(cls.py(), player, team)
    }

    #[classmethod]
    fn mute(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        mute(cls.py(), player)
    }

    #[classmethod]
    fn unmute(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        unmute(cls.py(), player)
    }

    #[classmethod]
    fn tempban(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        tempban(cls.py(), player)
    }

    #[classmethod]
    fn ban(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        ban(cls.py(), player)
    }

    #[classmethod]
    fn unban(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        unban(cls.py(), player)
    }

    #[classmethod]
    fn opsay(cls: &Bound<'_, PyType>, msg: &str) -> PyResult<()> {
        cls.py().allow_threads(|| opsay(msg))
    }

    #[classmethod]
    fn addadmin(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        addadmin(cls.py(), player)
    }

    #[classmethod]
    fn addmod(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        addmod(cls.py(), player)
    }

    #[classmethod]
    fn demote(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        demote(cls.py(), player)
    }

    #[classmethod]
    fn abort(cls: &Bound<'_, PyType>) -> PyResult<()> {
        cls.py().allow_threads(|| console_command("map_restart"))
    }

    #[classmethod]
    fn addscore(cls: &Bound<'_, PyType>, player: &Bound<'_, PyAny>, score: i32) -> PyResult<()> {
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

pub(crate) trait GameMethods<'py> {
    fn get_cached(&self) -> bool;
    fn get_valid(&self) -> bool;
    fn get_cvars(&self) -> PyResult<Bound<'py, PyDict>>;
    fn get_gametype(&self) -> PyResult<String>;
    fn get_gametype_short(&self) -> PyResult<String>;
    fn get_map(&self) -> PyResult<String>;
    fn set_map(&self, value: &str) -> PyResult<()>;
    fn get_map_title(&self) -> PyResult<String>;
    fn get_map_subtitle1(&self) -> PyResult<String>;
    fn get_map_subtitle2(&self) -> PyResult<String>;
    fn get_red_score(&self) -> PyResult<i32>;
    fn get_blue_score(&self) -> PyResult<i32>;
    fn get_state(&self) -> PyResult<String>;
    fn get_factory(&self) -> PyResult<String>;
    fn set_factory(&self, value: &str) -> PyResult<()>;
    fn get_factory_title(&self) -> PyResult<String>;
    fn get_hostname(&self) -> PyResult<String>;
    fn set_hostname(&self, value: &str) -> PyResult<()>;
    fn get_instagib(&self) -> PyResult<bool>;
    fn set_instagib(&self, value: &Bound<'_, PyAny>) -> PyResult<()>;
    fn get_loadout(&self) -> PyResult<bool>;
    fn set_loadout(&self, value: &Bound<'_, PyAny>) -> PyResult<()>;
    fn get_maxclients(&self) -> PyResult<i32>;
    fn set_maxclients(&self, value: i32) -> PyResult<()>;
    fn get_timelimit(&self) -> PyResult<i32>;
    fn set_timelimit(&self, value: i32) -> PyResult<()>;
    fn get_fraglimit(&self) -> PyResult<i32>;
    fn set_fraglimit(&self, value: i32) -> PyResult<()>;
    fn get_roundlimit(&self) -> PyResult<i32>;
    fn set_roundlimit(&self, value: i32) -> PyResult<()>;
    fn get_roundtimelimit(&self) -> PyResult<i32>;
    fn set_roundtimelimit(&self, value: i32) -> PyResult<()>;
    fn get_scorelimit(&self) -> PyResult<i32>;
    fn set_scorelimit(&self, value: i32) -> PyResult<()>;
    fn get_capturelimit(&self) -> PyResult<i32>;
    fn set_capturelimit(&self, value: i32) -> PyResult<()>;
    fn get_teamsize(&self) -> PyResult<i32>;
    fn set_teamsize(&self, value: i32) -> PyResult<()>;
    fn get_tags(&self) -> PyResult<Vec<String>>;
    fn set_tags(&self, value: &Bound<'_, PyAny>) -> PyResult<()>;
    fn get_workshop_items(&self) -> PyResult<Vec<u64>>;
    fn set_workshop_items(&self, value: &Bound<'_, PyAny>) -> PyResult<()>;
}

impl<'py> GameMethods<'py> for Bound<'py, Game> {
    fn get_cached(&self) -> bool {
        self.borrow().cached.load(Ordering::SeqCst)
    }

    fn get_valid(&self) -> bool {
        self.borrow().valid.load(Ordering::SeqCst)
    }

    fn get_cvars(&self) -> PyResult<Bound<'py, PyDict>> {
        MAIN_ENGINE
            .load()
            .as_ref()
            .map_or(
                Err(PyEnvironmentError::new_err(
                    "main quake live engine not set",
                )),
                |main_engine| {
                    let configstring = main_engine.get_configstring(CS_SERVERINFO as u16);
                    if configstring.is_empty() {
                        self.borrow().valid.store(false, Ordering::SeqCst);
                        return Err(NonexistentGameError::new_err(
                            "Invalid game. Is the server loading a new map?",
                        ));
                    }
                    Ok(parse_variables(&configstring))
                },
            )
            .and_then(|parsed_variables| parsed_variables.into_py_dict(self.py()))
    }

    fn get_gametype(&self) -> PyResult<String> {
        let factory_type = self.get_item("g_gametype")?.to_string();
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

    fn get_gametype_short(&self) -> PyResult<String> {
        let factory_type = self.get_item("g_gametype")?.to_string();
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

    fn get_map(&self) -> PyResult<String> {
        self.get_item("mapname").map(|value| value.to_string())
    }

    fn set_map(&self, value: &str) -> PyResult<()> {
        self.py().allow_threads(|| {
            let mapchange_command = format!("map {value}");
            console_command(&mapchange_command)
        })
    }

    fn get_map_title(&self) -> PyResult<String> {
        let base_module = self.py().import("shinqlx")?;
        let map_title = base_module.getattr("_map_title")?;
        map_title.extract::<String>()
    }

    fn get_map_subtitle1(&self) -> PyResult<String> {
        let base_module = self.py().import("shinqlx")?;
        let map_title = base_module.getattr("_map_subtitle1")?;
        map_title.extract::<String>()
    }

    fn get_map_subtitle2(&self) -> PyResult<String> {
        let base_module = self.py().import("shinqlx")?;
        let map_title = base_module.getattr("_map_subtitle2")?;
        map_title.extract::<String>()
    }

    fn get_red_score(&self) -> PyResult<i32> {
        self.py().allow_threads(|| {
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

    fn get_blue_score(&self) -> PyResult<i32> {
        self.py().allow_threads(|| {
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

    fn get_state(&self) -> PyResult<String> {
        let game_state = self.get_item("g_gameState")?.to_string();
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

    fn get_factory(&self) -> PyResult<String> {
        self.get_item("g_factory").map(|value| value.to_string())
    }

    fn set_factory(&self, value: &str) -> PyResult<()> {
        let mapname = self.get_map()?;
        self.py().allow_threads(|| {
            let mapchange_command = format!("map {mapname} {value}");
            console_command(&mapchange_command)
        })
    }

    fn get_factory_title(&self) -> PyResult<String> {
        self.get_item("g_factoryTitle")
            .map(|value| value.to_string())
    }

    fn get_hostname(&self) -> PyResult<String> {
        self.get_item("sv_hostname").map(|value| value.to_string())
    }

    fn set_hostname(&self, value: &str) -> PyResult<()> {
        pyshinqlx_set_cvar(self.py(), "sv_hostname", value, None)?;
        Ok(())
    }

    fn get_instagib(&self) -> PyResult<bool> {
        let insta_cvar = self.get_item("g_instagib")?.to_string();
        Ok(insta_cvar.parse::<i32>().is_ok_and(|value| value != 0))
    }

    fn set_instagib(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let string_cvar_value = match value.extract::<bool>() {
            Ok(true) => "1",
            Ok(false) => "0",
            Err(_) => match value.extract::<i32>() {
                Ok(1) => "1",
                Ok(0) => "0",
                _ => {
                    return Err(PyValueError::new_err(
                        "instagib needs to be 0, 1, or a bool.",
                    ));
                }
            },
        };
        pyshinqlx_set_cvar(self.py(), "g_instagib", string_cvar_value, None).map(|_| ())
    }

    fn get_loadout(&self) -> PyResult<bool> {
        let loadout_cvar = self.get_item("g_loadout")?.to_string();
        Ok(loadout_cvar.parse::<i32>().is_ok_and(|value| value != 0))
    }

    fn set_loadout(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let string_cvar_value = match value.extract::<bool>() {
            Ok(true) => "1",
            Ok(false) => "0",
            Err(_) => match value.extract::<i32>() {
                Ok(1) => "1",
                Ok(0) => "0",
                _ => {
                    return Err(PyValueError::new_err(
                        "loadout needs to be 0, 1, or a bool.",
                    ));
                }
            },
        };
        pyshinqlx_set_cvar(self.py(), "g_loadout", string_cvar_value, None).map(|_| ())
    }

    fn get_maxclients(&self) -> PyResult<i32> {
        let maxclients_cvar = self.get_item("sv_maxclients")?.to_string();
        Ok(maxclients_cvar.parse::<i32>().unwrap_or_default())
    }

    fn set_maxclients(&self, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(self.py(), "sv_maxclients", &value_str, None)?;
        Ok(())
    }

    fn get_timelimit(&self) -> PyResult<i32> {
        let timelimit_cvar = self.get_item("timelimit")?.to_string();
        Ok(timelimit_cvar.parse::<i32>().unwrap_or_default())
    }

    fn set_timelimit(&self, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(self.py(), "timelimit", &value_str, None)?;
        Ok(())
    }

    fn get_fraglimit(&self) -> PyResult<i32> {
        let fraglimit_cvar = self.get_item("fraglimit")?.to_string();
        Ok(fraglimit_cvar.parse::<i32>().unwrap_or_default())
    }

    fn set_fraglimit(&self, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(self.py(), "fraglimit", &value_str, None)?;
        Ok(())
    }

    fn get_roundlimit(&self) -> PyResult<i32> {
        let roundlimit_cvar = self.get_item("roundlimit")?.to_string();
        Ok(roundlimit_cvar.parse::<i32>().unwrap_or_default())
    }

    fn set_roundlimit(&self, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(self.py(), "roundlimit", &value_str, None)?;
        Ok(())
    }

    fn get_roundtimelimit(&self) -> PyResult<i32> {
        let roundtimelimit_cvar = self.get_item("roundtimelimit")?.to_string();
        Ok(roundtimelimit_cvar.parse::<i32>().unwrap_or_default())
    }

    fn set_roundtimelimit(&self, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(self.py(), "roundtimelimit", &value_str, None)?;
        Ok(())
    }

    fn get_scorelimit(&self) -> PyResult<i32> {
        let scorelimit_cvar = self.get_item("scorelimit")?.to_string();
        Ok(scorelimit_cvar.parse::<i32>().unwrap_or_default())
    }

    fn set_scorelimit(&self, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(self.py(), "scorelimit", &value_str, None)?;
        Ok(())
    }

    fn get_capturelimit(&self) -> PyResult<i32> {
        let capturelimit_cvar = self.get_item("capturelimit")?.to_string();
        Ok(capturelimit_cvar.parse::<i32>().unwrap_or_default())
    }

    fn set_capturelimit(&self, value: i32) -> PyResult<()> {
        let value_str = format!("{}", value);
        pyshinqlx_set_cvar(self.py(), "capturelimit", &value_str, None)?;
        Ok(())
    }

    fn get_teamsize(&self) -> PyResult<i32> {
        let teamsize_cvar = self.get_item("teamsize")?.to_string();
        Ok(teamsize_cvar.parse::<i32>().unwrap_or_default())
    }

    fn set_teamsize(&self, value: i32) -> PyResult<()> {
        self.py().allow_threads(|| set_teamsize(value))
    }

    fn get_tags(&self) -> PyResult<Vec<String>> {
        let tags_cvar = self.get_item("sv_tags")?.to_string();
        Ok(tags_cvar.split(',').map(|value| value.into()).collect())
    }

    fn set_tags(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let string_cvar_value = match value.extract::<String>() {
            Ok(new_tags) => new_tags,
            Err(_) => match value.extract::<Vec<Py<PyAny>>>() {
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
        pyshinqlx_set_cvar(self.py(), "sv_tags", &string_cvar_value, None).map(|_| ())
    }

    fn get_workshop_items(&self) -> PyResult<Vec<u64>> {
        self.py().allow_threads(|| {
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

    fn set_workshop_items(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let workshop_items_str = match value.extract::<Vec<Py<PyAny>>>() {
            Ok(new_workshop_items) => new_workshop_items
                .iter()
                .map(|value| value.to_string())
                .join(" "),
            Err(_) => {
                return Err(PyValueError::new_err("The value needs to be an iterable."));
            }
        };

        self.py().allow_threads(|| {
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
}

#[cfg(test)]
mod pyshinqlx_game_tests {
    use super::{Game, GameMethods, NonexistentGameError};
    use crate::ffi::c::prelude::{CS_SCORES1, CS_SCORES2, CS_SERVERINFO, CS_STEAM_WORKSHOP_IDS};
    use crate::ffi::python::prelude::*;
    use crate::prelude::*;

    use mockall::predicate;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use pyo3::{
        exceptions::{PyEnvironmentError, PyKeyError, PyValueError},
        types::{PyBool, PyInt, PyList, PyString},
    };

    fn default_game() -> Game {
        Game {
            valid: true.into(),
            cached: true.into(),
        }
    }

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
                    assert!(
                        result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py))
                    );
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
                assert_eq!(result.expect("result was not OK"), default_game());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_when_no_main_engine_loaded(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.repr();

            assert!(result.is_ok_and(|repr| repr == "Game(N/A@N/A)"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_empty_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.repr();

                    assert!(result.is_ok_and(|repr| repr == "Game(N/A@N/A)"));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_empty_map_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gametype\4", 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.repr();

                    assert!(result.is_ok_and(|repr| repr == "Game(N/A@N/A)"));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn repr_with_empty_gametype_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\mapname\thunderstruck", 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.repr();

                    assert!(result.is_ok_and(|repr| repr == "Game(N/A@N/A)"));
                });
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.repr();

                    assert!(result.is_ok_and(|repr| repr == "Game(Clan Arena@thunderstruck)"));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_when_no_main_engine_loaded(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.str();

            assert!(result.is_ok_and(|game_str| game_str == "Invalid game"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_empty_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.str();

                    assert!(result.is_ok_and(|game_str| game_str == "Invalid game"));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_empty_map_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\g_gametype\4", 1..)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.str();

                    assert!(result.is_ok_and(|game_str| game_str == "Invalid game"));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn str_with_empty_gametype_configstring(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\mapname\thunderstruck", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.str();

                    assert!(result.is_ok_and(|game_str| game_str == "Invalid game"));
                });
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.str();

                    assert!(result.is_ok_and(|game_str| game_str == "Clan Arena on thunderstruck"));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.contains("asdf");

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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.contains("asdf");

                    assert!(
                        result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py))
                    );
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.contains("asdf");

                    assert_eq!(result.expect("result was not OK"), true);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_value_is_not_in_configstring_variables(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\asdf\12", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.contains("qwertz");

                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_configstring_parses_empty(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.contains("asdf");

                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn contains_when_configstring_parses_to_none(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, "qwertz", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.contains("asdf");

                    assert_eq!(result.expect("result was not OK"), false);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn getitem_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.get_item("asdf");

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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_item("asdf");

                    assert!(
                        result.is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py))
                    );
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_item("asdf");

                    assert!(result.is_ok_and(|py_result| py_result.to_string() == "12"));
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_item("qwertz");

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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_item("asdf");

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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_item("asdf");

                    assert!(result.is_err_and(|err| err.is_instance_of::<PyKeyError>(py)));
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn cvars_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let cvars_result = game.get_cvars();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let cvars_result = game.get_cvars();
                    assert!(
                        cvars_result
                            .is_err_and(|err| err.is_instance_of::<NonexistentGameError>(py))
                    );
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let cvars_result = game.get_cvars();
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
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.get_gametype();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_gametype();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_gametype();
                    assert_eq!(result.expect("result was not OK"), expected_string);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_type_short_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.get_gametype_short();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_gametype_short();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_gametype_short();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_map();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_map("campgrounds").expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_map_title_gets_current_map(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
shinqlx._map_title = "eyetoeye"
            "#,
                None,
                None,
            )
            .expect("this should not happen");

            let game = Bound::new(py, default_game()).expect("this should not happen");

            assert_eq!(game.get_map_title().expect("result was not OK"), "eyetoeye");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_map_subtitle1_gets_current_subtitle1(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
shinqlx._map_subtitle1 = "Clan Arena"
            "#,
                None,
                None,
            )
            .expect("this should not happen");

            let game = Bound::new(py, default_game()).expect("this should not happen");

            assert_eq!(
                game.get_map_subtitle1().expect("result was not OK"),
                "Clan Arena"
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_map_subtitle2_gets_current_subtitle2(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            py.run(
                cr#"
import shinqlx
shinqlx._map_subtitle2 = "Awesome map!"
            "#,
                None,
                None,
            )
            .expect("this should not happen");

            let game = Bound::new(py, default_game()).expect("this should not happen");

            assert_eq!(
                game.get_map_subtitle2().expect("result was not OK"),
                "Awesome map!"
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_red_score_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.get_red_score();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_red_score();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_red_score();
                    assert_eq!(result.expect("result was not OK"), 0);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_blue_score_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.get_blue_score();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_blue_score();
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_blue_score();
                    assert_eq!(result.expect("result was not OK"), 0);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_state_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.get_state();
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_state();

                    assert_eq!(result.expect("result was not OK"), expected_return);
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_factory_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.get_factory();

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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_factory();

                    assert_eq!(result.expect("result was not OK"), "ca");
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_factory("ffa").expect("this should not happen");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_factory_title_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let game = Bound::new(py, default_game()).expect("this should not happen");

            let result = game.get_factory_title();
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_factory_title();

                    assert_eq!(result.expect("result was not OK"), "Clan Arena");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_hostname_returns_hostname(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default()
            .with_get_configstring(CS_SERVERINFO as u16, r"\sv_hostname\Awesome server!", 1)
            .run(|| {
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_hostname();

                    assert_eq!(result.expect("result was not OK"), "Awesome server!");
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_hostname("More awesome server!")
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_instagib();

                    assert_eq!(result.expect("result was not OK"), expected);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_instagib(PyBool::new(py, value_set).as_any())
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_instagib(PyInt::new(py, value_set).as_any())
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.set_instagib(PyString::new(py, "asdf").as_any());

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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_loadout();

                    assert_eq!(result.expect("result was not OK"), expected);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_loadout(PyBool::new(py, value_set).as_any())
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_loadout(PyInt::new(py, value_set).as_any())
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.set_loadout(PyString::new(py, "asdf").as_any());

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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_maxclients();

                    assert_eq!(result.expect("result was not OK"), 8);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_maxclients(32).expect("this should not happen");
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_timelimit();

                    assert_eq!(result.expect("result was not OK"), 20);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_timelimit(30).expect("this should not happen");
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_fraglimit();

                    assert_eq!(result.expect("result was not OK"), 10);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_fraglimit(20).expect("this should not happen");
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_roundlimit();

                    assert_eq!(result.expect("result was not OK"), 11);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_roundlimit(13).expect("this should not happen");
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_roundtimelimit();

                    assert_eq!(result.expect("result was not OK"), 240);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_roundtimelimit(150)
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_scorelimit();

                    assert_eq!(result.expect("result was not OK"), 10);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_scorelimit(8).expect("this should not happen");
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_capturelimit();

                    assert_eq!(result.expect("result was not OK"), 10);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.set_capturelimit(20);

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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_teamsize();

                    assert_eq!(result.expect("result was not OK"), 4);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_teamsize(8).expect("this should not happen");
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_tags();

                    assert_eq!(
                        result.expect("result was not OK"),
                        vec!["tag1", "tag2", "tag3"]
                    );
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_tags(PyString::new(py, "tag1,tag2,tag3").as_any())
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_tags(
                        PyList::new(py, ["tag1", "tag2", "tag3"])
                            .expect("this should not happen")
                            .as_any(),
                    )
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.set_tags(PyInt::new(py, 42i32).as_any());
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
                Python::with_gil(|py| {
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.get_workshop_items();

                    assert_eq!(result.expect("result was not OK"), vec![1234, 5678, 9101]);
                });
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    game.set_workshop_items(
                        PyList::new(py, [1234, 5678, 9101])
                            .expect("this should not happen")
                            .as_any(),
                    )
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
                    let game = Bound::new(py, default_game()).expect("this should not happen");

                    let result = game.set_workshop_items(PyInt::new(py, 42i32).as_any());
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
                let result = Python::with_gil(|py| Game::shuffle(&py.get_type::<Game>()));
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
                let result = Python::with_gil(|py| Game::timeout(&py.get_type::<Game>()));
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
                let result = Python::with_gil(|py| Game::timein(&py.get_type::<Game>()));
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
                let result = Python::with_gil(|py| Game::allready(&py.get_type::<Game>()));
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
                let result = Python::with_gil(|py| Game::pause(&py.get_type::<Game>()));
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
                let result = Python::with_gil(|py| Game::unpause(&py.get_type::<Game>()));
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
                    let result = Game::lock(&py.get_type::<Game>(), Some("invalid_team"));
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
                let result = Python::with_gil(|py| Game::lock(&py.get_type::<Game>(), None));
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
                let result =
                    Python::with_gil(|py| Game::lock(&py.get_type::<Game>(), Some(locked_team)));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unlock_with_invalid_team(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().run(|| {
            Python::with_gil(|py| {
                let result = Game::unlock(&py.get_type::<Game>(), Some("invalid_team"));
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
                let result = Python::with_gil(|py| Game::unlock(&py.get_type::<Game>(), None));
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
                let result =
                    Python::with_gil(|py| Game::unlock(&py.get_type::<Game>(), Some(locked_team)));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_team(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::put(
                &py.get_type::<Game>(),
                PyInt::new(py, 2i32).as_any(),
                "invalid team",
            );
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn put_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::put(
                &py.get_type::<Game>(),
                PyInt::new(py, 2048i32).as_any(),
                "red",
            );
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
                    Game::put(
                        &py.get_type::<Game>(),
                        PyInt::new(py, 2i32).as_any(),
                        new_team,
                    )
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mute_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::mute(&py.get_type::<Game>(), PyInt::new(py, 2048i32).as_any());
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
                let result = Python::with_gil(|py| {
                    Game::mute(&py.get_type::<Game>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unmute_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::unmute(&py.get_type::<Game>(), PyInt::new(py, 2048i32).as_any());
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
                    Game::unmute(&py.get_type::<Game>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn tempban_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::tempban(&py.get_type::<Game>(), PyInt::new(py, 2048i32).as_any());
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
                    Game::tempban(&py.get_type::<Game>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::ban(&py.get_type::<Game>(), PyInt::new(py, 2048i32).as_any());
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
                let result = Python::with_gil(|py| {
                    Game::ban(&py.get_type::<Game>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::unban(&py.get_type::<Game>(), PyInt::new(py, 2048i32).as_any());
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
                let result = Python::with_gil(|py| {
                    Game::unban(&py.get_type::<Game>(), PyInt::new(py, 2i32).as_any())
                });
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
                let result = Python::with_gil(|py| Game::opsay(&py.get_type::<Game>(), "asdf"));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addadmin_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::addadmin(&py.get_type::<Game>(), PyInt::new(py, 2048i32).as_any());
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
                    Game::addadmin(&py.get_type::<Game>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addmod_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::addmod(&py.get_type::<Game>(), PyInt::new(py, 2048i32).as_any());
            assert!(
                result
                    .as_ref()
                    .is_err_and(|err| err.is_instance_of::<PyValueError>(py)),
                "{:?}",
                result.as_ref()
            );
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
                    Game::addmod(&py.get_type::<Game>(), PyInt::new(py, 2i32).as_any())
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn demote_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::demote(&py.get_type::<Game>(), PyInt::new(py, 2048i32).as_any());
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
                    Game::demote(&py.get_type::<Game>(), PyInt::new(py, 2i32).as_any())
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
                let result = Python::with_gil(|py| Game::abort(&py.get_type::<Game>()));
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addscore_with_invalid_player(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result =
                Game::addscore(&py.get_type::<Game>(), PyInt::new(py, 2048i32).as_any(), 42);
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
                    Game::addscore(&py.get_type::<Game>(), PyInt::new(py, 2i32).as_any(), 42)
                });
                assert!(result.is_ok());
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn addteamscore_with_invalid_team(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let result = Game::addteamscore(&py.get_type::<Game>(), "invalid_team", 42);
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
                    Game::addteamscore(&py.get_type::<Game>(), locked_team, 42)
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
                let result = Python::with_gil(|py| Game::setmatchtime(&py.get_type::<Game>(), 42));
                assert!(result.is_ok());
            });
    }
}
