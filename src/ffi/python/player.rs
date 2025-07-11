use core::{
    borrow::BorrowMut,
    hint::cold_path,
    sync::atomic::{AtomicBool, Ordering},
};

use derive_more::Display;
use itertools::Itertools;
use pyo3::{
    BoundObject, IntoPyObjectExt,
    basic::CompareOp,
    create_exception,
    exceptions::{
        PyAttributeError, PyEnvironmentError, PyException, PyKeyError, PyNotImplementedError,
        PyValueError,
    },
    intern,
    types::{IntoPyDict, PyBool, PyDict, PyInt, PyNotImplemented, PyType},
};
use rayon::prelude::*;
use tap::{TapOptional, TryConv};

use super::{
    CONSOLE_CHANNEL, ConnectionStates, Teams, addadmin, addmod, addscore, ban, console_command,
    demote, mute, owner, prelude::*, put, tempban, unmute,
};
use crate::{
    MAIN_ENGINE,
    ffi::c::prelude::*,
    quake_live_engine::{GameAddEvent, GetConfigstring, SetConfigstring},
};

create_exception!(pyshinqlx_module, NonexistentPlayerError, PyException);

impl TryFrom<&str> for privileges_t {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "none" => Ok(privileges_t::PRIV_NONE),
            "mod" => Ok(privileges_t::PRIV_MOD),
            "admin" => Ok(privileges_t::PRIV_ADMIN),
            _ => {
                cold_path();
                Err("Invalid privilege level.")
            }
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
            _ => {
                cold_path();
                Err("invalid weapon".to_string())
            }
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
#[pyclass(module = "_player", name = "Player", subclass, frozen, str)]
#[derive(Debug, Display)]
#[display("{}", name.read())]
pub(crate) struct Player {
    pub(crate) valid: AtomicBool,
    #[pyo3(name = "_id", get)]
    pub(crate) id: i32,
    pub(crate) player_info: parking_lot::RwLock<PlayerInfo>,
    #[pyo3(name = "_userinfo", get)]
    pub(crate) user_info: String,
    #[pyo3(name = "_steam_id", get)]
    pub(crate) steam_id: i64,
    pub(crate) name: parking_lot::RwLock<String>,
}

impl Clone for Player {
    fn clone(&self) -> Self {
        Self {
            valid: self.valid.load(Ordering::Acquire).into(),
            id: self.id,
            player_info: self.player_info.read().to_owned().into(),
            user_info: self.user_info.to_owned(),
            steam_id: self.steam_id,
            name: self.name.read().to_owned().into(),
        }
    }
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        self.valid.load(Ordering::Acquire) == other.valid.load(Ordering::Acquire)
            && self.id == other.id
            && *self.player_info.read() == *other.player_info.read()
            && self.user_info == other.user_info
            && self.steam_id == other.steam_id
            && *self.name.read() == *other.name.read()
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
            player_info.name.to_owned()
        };

        Ok(Player {
            valid: true.into(),
            id: client_id,
            user_info: player_info.userinfo.to_owned(),
            steam_id: player_info.steam_id,
            player_info: player_info.into(),
            name: name.into(),
        })
    }

    fn __repr__(slf: &Bound<'_, Self>) -> String {
        let Ok(classname) = slf.get_type().qualname() else {
            cold_path();
            return "NonexistentPlayer".to_string();
        };
        let id = slf.get_id();
        let clean_name = slf.get_clean_name();
        let steam_id = slf.get_steam_id();

        if !slf.get().valid.load(Ordering::Acquire) {
            format!("{classname}(INVALID:'{clean_name}':{steam_id})")
        } else {
            format!("{classname}({id}:'{clean_name}':{steam_id})")
        }
    }

    fn __contains__(slf: &Bound<'_, Self>, item: &str) -> PyResult<bool> {
        slf.__contains__(item)
    }

    fn __getitem__(slf: &Bound<'_, Self>, item: &str) -> PyResult<String> {
        slf.__getitem__(item)
    }

    fn __richcmp__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
        op: CompareOp,
    ) -> PyResult<Borrowed<'py, 'py, PyAny>> {
        match op {
            CompareOp::Eq => {
                if let Ok(other_player) = other.extract::<Self>() {
                    Ok(
                        PyBool::new(slf.py(), slf.get().steam_id == other_player.steam_id)
                            .into_any(),
                    )
                } else if let Ok(steam_id) = other.extract::<i64>() {
                    Ok(PyBool::new(slf.py(), slf.get().steam_id == steam_id).into_any())
                } else {
                    Ok(PyBool::new(slf.py(), false).into_any())
                }
            }
            CompareOp::Ne => {
                if let Ok(other_player) = other.extract::<Self>() {
                    Ok(
                        PyBool::new(slf.py(), slf.get().steam_id != other_player.steam_id)
                            .into_any(),
                    )
                } else if let Ok(steam_id) = other.extract::<i64>() {
                    Ok(PyBool::new(slf.py(), slf.get().steam_id != steam_id).into_any())
                } else {
                    Ok(PyBool::new(slf.py(), true).into_any())
                }
            }
            _ => Ok(PyNotImplemented::get(slf.py()).into_any()),
        }
    }

    /// Update the player information with the latest data. If the player
    /// disconnected it will raise an exception and invalidates a player.
    /// The player's name and Steam ID can still be accessed after being
    /// invalidated, but anything else will make it throw an exception too.
    ///
    /// :raises: shinqlx.NonexistentPlayerError
    fn update(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.update()
    }

    #[pyo3(
    name = "_invalidate",
    signature = (e = "The player does not exist anymore. Did the player disconnect?"),
    text_signature = "(e = \"The player does not exist anymore. Did the player disconnect?\")"
    )]
    fn invalidate(slf: &Bound<'_, Self>, e: &str) -> PyResult<()> {
        slf.invalidate(e)
    }

    #[getter(cvars)]
    fn get_cvars<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyDict>> {
        slf.get_cvars()
    }

    #[setter(cvars)]
    fn set_cvars(slf: &Bound<'_, Self>, new_cvars: &Bound<'_, PyDict>) -> PyResult<()> {
        slf.set_cvars(new_cvars)
    }

    #[getter(steam_id)]
    fn get_steam_id(slf: &Bound<'_, Self>) -> i64 {
        slf.get_steam_id()
    }

    #[getter(id)]
    fn get_id(slf: &Bound<'_, Self>) -> i32 {
        slf.get_id()
    }

    #[getter(ip)]
    fn get_ip(slf: &Bound<'_, Self>) -> String {
        slf.get_ip()
    }

    /// The clan tag. Not actually supported by QL, but it used to be and
    /// fortunately the scoreboard still properly displays it if we manually
    /// set the configstring to use clan tags.
    #[getter(clan)]
    fn get_clan(slf: &Bound<'_, Self>) -> String {
        slf.get_clan()
    }

    #[setter(clan)]
    fn set_clan(slf: Bound<'_, Self>, tag: &str) {
        slf.set_clan(tag)
    }

    #[getter(name)]
    fn get_name(slf: &Bound<'_, Self>) -> String {
        slf.get_name()
    }

    #[setter(name)]
    fn set_name(slf: &Bound<'_, Self>, value: &str) -> PyResult<()> {
        slf.set_name(value)
    }

    /// Removes color tags from the name.
    #[getter(clean_name)]
    fn get_clean_name(slf: &Bound<'_, Self>) -> String {
        slf.get_clean_name()
    }

    #[getter(qport)]
    fn get_qport(slf: &Bound<'_, Self>) -> i32 {
        slf.get_qport()
    }

    #[getter(team)]
    pub(crate) fn get_team(&self, py: Python<'_>) -> PyResult<String> {
        py.allow_threads(|| match Teams::from(self.player_info.read().team) {
            Teams::Invalid => {
                cold_path();
                Err(PyValueError::new_err("invalid team"))
            }
            team => Ok(team.to_string()),
        })
    }

    #[setter(team)]
    fn set_team(slf: &Bound<'_, Self>, new_team: &str) -> PyResult<()> {
        slf.set_team(new_team)
    }

    #[getter(colors)]
    fn get_colors(slf: &Bound<'_, Self>) -> (f32, f32) {
        slf.get_colors()
    }

    #[setter(colors)]
    fn set_colors(slf: &Bound<'_, Self>, new: (i32, i32)) -> PyResult<()> {
        slf.set_colors(new)
    }

    #[getter(model)]
    fn get_model(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_model()
    }

    #[setter(model)]
    fn set_model(slf: &Bound<'_, Self>, value: &str) -> PyResult<()> {
        slf.set_model(value)
    }

    #[getter(headmodel)]
    fn get_headmodel(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_headmodel()
    }

    #[setter(headmodel)]
    fn set_headmodel(slf: &Bound<'_, Self>, value: &str) -> PyResult<()> {
        slf.set_headmodel(value)
    }

    #[getter(handicap)]
    fn get_handicap(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_handicap()
    }

    #[setter(handicap)]
    fn set_handicap(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_handicap(value)
    }

    #[getter(autohop)]
    fn get_autohop(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_autohop()
    }

    #[setter(autohop)]
    fn set_autohop(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_autohop(value)
    }

    #[getter(autoaction)]
    fn get_autoaction(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_autoaction()
    }

    #[setter(autoaction)]
    fn set_autoaction(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_autoaction(value)
    }

    #[getter(predictitems)]
    fn get_predictitems(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_predictitems()
    }

    #[setter(predictitems)]
    fn set_predictitems(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_predictitems(value)
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
    fn get_connection_state(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_connection_state()
    }

    #[getter(state)]
    fn get_state(slf: &Bound<'_, Self>) -> PyResult<Option<PlayerState>> {
        slf.get_state()
    }

    #[getter(privileges)]
    fn get_privileges(slf: &Bound<'_, Self>) -> Option<String> {
        slf.get_privileges()
    }

    #[setter(privileges)]
    fn set_privileges(slf: &Bound<'_, Self>, value: Option<&str>) -> PyResult<()> {
        slf.set_privileges(value)
    }

    #[getter(country)]
    fn get_country(slf: &Bound<'_, Self>) -> PyResult<String> {
        slf.get_country()
    }

    #[setter(country)]
    fn set_country(slf: &Bound<'_, Self>, value: &str) -> PyResult<()> {
        slf.set_country(value)
    }

    #[getter(_valid)]
    fn get_valid(slf: &Bound<'_, Self>) -> bool {
        slf.get_valid()
    }

    #[getter(stats)]
    fn get_stats(slf: &Bound<'_, Self>) -> PyResult<Option<PlayerStats>> {
        slf.get_stats()
    }

    #[getter(ping)]
    fn get_ping(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_ping()
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn position<'py>(
        slf: &Bound<'py, Self>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.position(reset, kwargs)
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn velocity<'py>(
        slf: &Bound<'py, Self>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.velocity(reset, kwargs)
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn weapons<'py>(
        slf: &Bound<'py, Self>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.weapons(reset, kwargs)
    }

    #[pyo3(signature = (new_weapon = None), text_signature = "(new_weapon = None)")]
    fn weapon<'py>(
        slf: &Bound<'py, Self>,
        new_weapon: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.weapon(new_weapon)
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn ammo<'py>(
        slf: &Bound<'py, Self>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.ammo(reset, kwargs)
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn powerups<'py>(
        slf: &Bound<'py, Self>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.powerups(reset, kwargs)
    }

    #[getter(holdable)]
    fn get_holdable(slf: &Bound<'_, Self>) -> PyResult<Option<String>> {
        slf.get_holdable()
    }

    #[setter(holdable)]
    fn set_holdable(slf: &Bound<'_, Self>, holdable: Option<&str>) -> PyResult<()> {
        slf.set_holdable(holdable)
    }

    fn drop_holdable(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.drop_holdable()
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = false, **kwargs)")]
    fn flight<'py>(
        slf: &Bound<'py, Self>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.flight(reset, kwargs)
    }

    #[getter(noclip)]
    fn get_noclip(slf: &Bound<'_, Self>) -> PyResult<bool> {
        slf.get_noclip()
    }

    #[setter(noclip)]
    fn set_noclip(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_noclip(value)
    }

    #[getter(health)]
    fn get_health(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_health()
    }

    #[setter(health)]
    fn set_health(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_health(value)
    }

    #[getter(armor)]
    fn get_armor(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_armor()
    }

    #[setter(armor)]
    fn set_armor(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_armor(value)
    }

    #[getter(is_alive)]
    fn get_is_alive(slf: &Bound<'_, Self>) -> PyResult<bool> {
        slf.get_is_alive()
    }

    #[setter(is_alive)]
    fn set_is_alive(slf: &Bound<'_, Self>, value: bool) -> PyResult<()> {
        slf.set_is_alive(value)
    }

    #[getter(is_frozen)]
    fn get_is_frozen(slf: &Bound<'_, Self>) -> PyResult<bool> {
        slf.get_is_frozen()
    }

    #[getter(is_chatting)]
    fn get_is_chatting(slf: &Bound<'_, Self>) -> PyResult<bool> {
        slf.get_is_chatting()
    }

    #[getter(score)]
    fn get_score(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_score()
    }

    #[setter(score)]
    fn set_score(slf: &Bound<'_, Self>, value: i32) -> PyResult<()> {
        slf.set_score(value)
    }

    #[getter(channel)]
    fn get_channel<'py>(slf: &Bound<'py, Self>) -> Option<Bound<'py, TellChannel>> {
        slf.get_channel()
    }

    fn center_print(slf: &Bound<'_, Self>, msg: &str) -> PyResult<()> {
        slf.center_print(msg)
    }

    #[pyo3(signature = (msg, **kwargs))]
    fn tell<'py>(
        slf: &Bound<'py, Self>,
        msg: &str,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<()> {
        slf.tell(msg, kwargs)
    }

    #[pyo3(signature = (reason = ""), text_signature = "(reason = \"\")")]
    fn kick(slf: &Bound<'_, Self>, reason: &str) -> PyResult<()> {
        slf.kick(reason)
    }

    fn ban(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.ban()
    }

    fn tempban(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.tempban()
    }

    fn addadmin(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.addadmin()
    }

    fn addmod(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.addmod()
    }

    fn demote(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.demote()
    }

    fn mute(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.mute()
    }

    fn unmute(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.unmute()
    }

    pub(crate) fn put(slf: &Bound<'_, Self>, team: &str) -> PyResult<()> {
        slf.put(team)
    }

    fn addscore(slf: &Bound<'_, Self>, score: i32) -> PyResult<()> {
        slf.addscore(score)
    }

    fn switch(slf: &Bound<'_, Self>, other_player: &Bound<'_, Player>) -> PyResult<()> {
        slf.switch(other_player)
    }

    #[pyo3(signature = (damage = 0), text_signature = "(damage = 0)")]
    fn slap(slf: &Bound<'_, Self>, damage: i32) -> PyResult<()> {
        slf.slap(damage)
    }

    fn slay(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.slay()
    }

    fn slay_with_mod(slf: &Bound<'_, Self>, means_of_death: i32) -> PyResult<()> {
        slf.slay_with_mod(means_of_death)
    }

    #[classmethod]
    pub(crate) fn all_players(cls: &Bound<'_, PyType>) -> PyResult<Vec<Player>> {
        let players_info = pyshinqlx_players_info(cls.py())?;
        cls.py().allow_threads(|| {
            Ok(players_info
                .par_iter()
                .filter_map(|opt_player_info| {
                    opt_player_info.as_ref().map(|player_info| Player {
                        valid: true.into(),
                        id: player_info.client_id,
                        user_info: player_info.userinfo.to_owned(),
                        steam_id: player_info.steam_id,
                        name: player_info.name.to_owned().into(),
                        player_info: player_info.to_owned().into(),
                    })
                })
                .collect())
        })
    }
}

pub(crate) trait PlayerMethods<'py> {
    fn __contains__(&self, item: &str) -> PyResult<bool>;
    fn __getitem__(&self, item: &str) -> PyResult<String>;
    fn update(&self) -> PyResult<()>;
    fn invalidate(&self, e: &str) -> PyResult<()>;
    fn get_cvars(&self) -> PyResult<Bound<'py, PyDict>>;
    fn set_cvars(&self, new_cvars: &Bound<'_, PyDict>) -> PyResult<()>;
    fn get_steam_id(&self) -> i64;
    fn get_id(&self) -> i32;
    fn get_ip(&self) -> String;
    fn get_clan(&self) -> String;
    fn set_clan(&self, tag: &str);
    fn get_name(&self) -> String;
    fn set_name(&self, value: &str) -> PyResult<()>;
    fn get_clean_name(&self) -> String;
    fn get_qport(&self) -> i32;
    #[cfg_attr(not(test), allow(dead_code))]
    fn get_team(&self) -> PyResult<String>;
    fn set_team(&self, new_team: &str) -> PyResult<()>;
    fn get_colors(&self) -> (f32, f32);
    fn set_colors(&self, new: (i32, i32)) -> PyResult<()>;
    fn get_model(&self) -> PyResult<String>;
    fn set_model(&self, value: &str) -> PyResult<()>;
    fn get_headmodel(&self) -> PyResult<String>;
    fn set_headmodel(&self, value: &str) -> PyResult<()>;
    fn get_handicap(&self) -> PyResult<String>;
    fn set_handicap(&self, value: &Bound<'py, PyAny>) -> PyResult<()>;
    fn get_autohop(&self) -> PyResult<i32>;
    fn set_autohop(&self, value: &Bound<'py, PyAny>) -> PyResult<()>;
    fn get_autoaction(&self) -> PyResult<i32>;
    fn set_autoaction(&self, value: &Bound<'py, PyAny>) -> PyResult<()>;
    fn get_predictitems(&self) -> PyResult<i32>;
    fn set_predictitems(&self, value: &Bound<'py, PyAny>) -> PyResult<()>;
    fn get_connection_state(&self) -> PyResult<String>;
    fn get_state(&self) -> PyResult<Option<PlayerState>>;
    fn get_privileges(&self) -> Option<String>;
    fn set_privileges(&self, value: Option<&str>) -> PyResult<()>;
    fn get_country(&self) -> PyResult<String>;
    fn set_country(&self, value: &str) -> PyResult<()>;
    fn get_valid(&self) -> bool;
    fn get_stats(&self) -> PyResult<Option<PlayerStats>>;
    fn get_ping(&self) -> PyResult<i32>;
    fn position(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn velocity(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn weapons(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn weapon(&self, new_weapon: Option<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>>;
    fn ammo(&self, reset: bool, kwargs: Option<&Bound<'py, PyDict>>)
    -> PyResult<Bound<'py, PyAny>>;
    fn powerups(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn get_holdable(&self) -> PyResult<Option<String>>;
    fn set_holdable(&self, holdable: Option<&str>) -> PyResult<()>;
    fn drop_holdable(&self) -> PyResult<()>;
    fn flight(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>>;
    fn get_noclip(&self) -> PyResult<bool>;
    fn set_noclip(&self, value: &Bound<'py, PyAny>) -> PyResult<()>;
    fn get_health(&self) -> PyResult<i32>;
    fn set_health(&self, value: i32) -> PyResult<()>;
    fn get_armor(&self) -> PyResult<i32>;
    fn set_armor(&self, value: i32) -> PyResult<()>;
    fn get_is_alive(&self) -> PyResult<bool>;
    fn set_is_alive(&self, value: bool) -> PyResult<()>;
    fn get_is_frozen(&self) -> PyResult<bool>;
    fn get_is_chatting(&self) -> PyResult<bool>;
    fn get_score(&self) -> PyResult<i32>;
    fn set_score(&self, value: i32) -> PyResult<()>;
    fn get_channel(&self) -> Option<Bound<'py, TellChannel>>;
    fn center_print(&self, msg: &str) -> PyResult<()>;
    fn tell(&self, msg: &str, kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<()>;
    fn kick(&self, reason: &str) -> PyResult<()>;
    fn ban(&self) -> PyResult<()>;
    fn tempban(&self) -> PyResult<()>;
    fn addadmin(&self) -> PyResult<()>;
    fn addmod(&self) -> PyResult<()>;
    fn demote(&self) -> PyResult<()>;
    fn mute(&self) -> PyResult<()>;
    fn unmute(&self) -> PyResult<()>;
    fn put(&self, team: &str) -> PyResult<()>;
    fn addscore(&self, score: i32) -> PyResult<()>;
    fn switch(&self, other_player: &Bound<'py, Player>) -> PyResult<()>;
    fn slap(&self, damage: i32) -> PyResult<()>;
    fn slay(&self) -> PyResult<()>;
    fn slay_with_mod(&self, means_of_death: i32) -> PyResult<()>;
}

impl<'py> PlayerMethods<'py> for Bound<'py, Player> {
    fn __contains__(&self, item: &str) -> PyResult<bool> {
        if !self.get().valid.load(Ordering::Acquire) {
            cold_path();
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        Ok(parse_variables(&self.get().user_info).get(item).is_some())
    }

    fn __getitem__(&self, item: &str) -> PyResult<String> {
        if !self.get().valid.load(Ordering::Acquire) {
            cold_path();
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        parse_variables(&self.get().user_info)
            .get(item)
            .ok_or(PyKeyError::new_err(format!("'{item}'")))
    }

    fn update(&self) -> PyResult<()> {
        *self.get().player_info.write() = PlayerInfo::from(self.get().id);

        if self.get().player_info.read().steam_id != self.get().steam_id {
            cold_path();
            self.get().valid.store(false, Ordering::Release);
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        let name = if self.get().player_info.read().name.is_empty() {
            parse_variables(&self.get().player_info.read().userinfo)
                .get("name")
                .unwrap_or_default()
        } else {
            self.get().player_info.read().name.to_owned()
        };
        *self.get().name.write() = name;

        Ok(())
    }

    fn invalidate(&self, e: &str) -> PyResult<()> {
        self.get().valid.store(false, Ordering::Release);
        Err(NonexistentPlayerError::new_err(e.to_string()))
    }

    fn get_cvars(&self) -> PyResult<Bound<'py, PyDict>> {
        if !self.get().valid.load(Ordering::Acquire) {
            cold_path();
            return Err(NonexistentPlayerError::new_err(
                "The player does not exist anymore. Did the player disconnect?",
            ));
        }

        parse_variables(&self.get().user_info).into_py_dict(self.py())
    }

    fn set_cvars(&self, new_cvars: &Bound<'_, PyDict>) -> PyResult<()> {
        let new = new_cvars
            .iter()
            .map(|(key, value)| format!(r"\{key}\{value}"))
            .join("");
        let client_command = format!(r#"userinfo "{new}""#);
        pyshinqlx_client_command(self.py(), self.get().id, &client_command).map(|_| ())
    }

    fn get_steam_id(&self) -> i64 {
        self.get().steam_id
    }

    fn get_id(&self) -> i32 {
        self.get().id
    }

    fn get_ip(&self) -> String {
        parse_variables(&self.get().user_info)
            .get("ip")
            .map(|value| value.split(':').next().unwrap_or("").to_string())
            .unwrap_or("".to_string())
    }

    fn get_clan(&self) -> String {
        MAIN_ENGINE
            .load()
            .as_ref()
            .and_then(|main_engine| {
                let configstring =
                    main_engine.get_configstring(CS_PLAYERS as u16 + self.get().id as u16);
                parse_variables(&configstring).get("cn")
            })
            .unwrap_or("".to_string())
    }

    fn set_clan(&self, tag: &str) {
        let config_index = CS_PLAYERS as u16 + self.get().id as u16;

        MAIN_ENGINE.load().as_ref().tap_some(|main_engine| {
            let configstring = main_engine.get_configstring(config_index);
            let mut parsed_variables = parse_variables(&configstring);
            parsed_variables.set("xcn", tag);
            parsed_variables.set("cn", tag);

            let new_configstring: String = parsed_variables.into();
            main_engine.set_configstring(config_index as i32, &new_configstring);
        });
    }

    fn get_name(&self) -> String {
        format!("{}^7", self.get().name.read().trim_end_matches("^7"))
    }

    fn set_name(&self, value: &str) -> PyResult<()> {
        let mut new_cvars = parse_variables(&self.get().user_info);
        new_cvars.set("name", value);
        let new: String = new_cvars.into();

        let client_command = format!("userinfo \"{new}\"");
        pyshinqlx_client_command(self.py(), self.get().id, &client_command).map(|_| ())
    }

    fn get_clean_name(&self) -> String {
        clean_text(&(&*self.get().name.read()))
    }

    fn get_qport(&self) -> i32 {
        parse_variables(&self.get().user_info)
            .get("qport")
            .map(|value| value.parse::<i32>().unwrap_or(-1))
            .unwrap_or(-1)
    }

    fn get_team(&self) -> PyResult<String> {
        self.get().get_team(self.py())
    }

    fn set_team(&self, new_team: &str) -> PyResult<()> {
        let new_team_lower = new_team.to_lowercase();
        match Teams::from(new_team_lower.as_str()) {
            Teams::Invalid => {
                cold_path();
                Err(PyValueError::new_err("Invalid team."))
            }
            team => {
                let team_change_cmd = format!("put {} {team}", self.get().id);
                console_command(&team_change_cmd)
            }
        }
    }

    fn get_colors(&self) -> (f32, f32) {
        let cvars = parse_variables(&self.get().user_info);
        let color1 = cvars
            .get("color1")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let color2 = cvars
            .get("color2")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        (color1, color2)
    }

    fn set_colors(&self, new: (i32, i32)) -> PyResult<()> {
        let mut new_cvars = parse_variables(&self.get().player_info.read().userinfo);
        new_cvars.set("color1", &format!("{}", new.0));
        new_cvars.set("color2", &format!("{}", new.1));
        let new_cvars_string: String = new_cvars.into();

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(self.py(), self.get().id, &client_command)?;
        Ok(())
    }

    fn get_model(&self) -> PyResult<String> {
        parse_variables(&self.get().user_info)
            .get("model")
            .ok_or(PyKeyError::new_err("'model'"))
    }

    fn set_model(&self, value: &str) -> PyResult<()> {
        let mut new_cvars = parse_variables(&self.get().player_info.read().userinfo);
        new_cvars.set("model", value);
        let new_cvars_string: String = new_cvars.into();

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(self.py(), self.get().id, &client_command)?;
        Ok(())
    }

    fn get_headmodel(&self) -> PyResult<String> {
        parse_variables(&self.get().user_info)
            .get("headmodel")
            .ok_or(PyKeyError::new_err("'headmodel'"))
    }

    fn set_headmodel(&self, value: &str) -> PyResult<()> {
        let mut new_cvars = parse_variables(&self.get().player_info.read().userinfo);
        new_cvars.set("headmodel", value);
        let new_cvars_string: String = new_cvars.into();

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(self.py(), self.get().id, &client_command).map(|_| ())
    }

    fn get_handicap(&self) -> PyResult<String> {
        parse_variables(&self.get().user_info)
            .get("handicap")
            .ok_or(PyKeyError::new_err("'handicap'"))
    }

    fn set_handicap(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let new_handicap = value.str()?.to_string();
        if new_handicap.parse::<i32>().is_err() {
            cold_path();
            let error_msg = format!("invalid literal for int() with base 10: '{new_handicap}'");
            return Err(PyValueError::new_err(error_msg));
        }

        let mut new_cvars = parse_variables(&self.get().player_info.read().userinfo);
        new_cvars.set("handicap", &new_handicap);
        let new_cvars_string: String = new_cvars.into();

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(self.py(), self.get().id, &client_command).map(|_| ())
    }

    fn get_autohop(&self) -> PyResult<i32> {
        parse_variables(&self.get().user_info)
            .get("autohop")
            .map_or(
                {
                    cold_path();
                    Err(PyKeyError::new_err("'autohop'"))
                },
                |value| {
                    value.parse::<i32>().map_err(|_| {
                        let error_msg =
                            format!("invalid literal for int() with base 10: '{value}'");
                        PyValueError::new_err(error_msg)
                    })
                },
            )
    }

    fn set_autohop(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let new_autohop = value.str()?.to_string();
        if new_autohop.parse::<i32>().is_err() {
            cold_path();
            let error_msg = format!("invalid literal for int() with base 10: '{new_autohop}'");
            return Err(PyValueError::new_err(error_msg));
        }

        let mut new_cvars = parse_variables(&self.get().player_info.read().userinfo);
        new_cvars.set("autohop", &new_autohop);
        let new_cvars_string: String = new_cvars.into();

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(self.py(), self.get().id, &client_command).map(|_| ())
    }

    fn get_autoaction(&self) -> PyResult<i32> {
        parse_variables(&self.get().user_info)
            .get("autoaction")
            .map_or(
                {
                    cold_path();
                    Err(PyKeyError::new_err("'autoaction'"))
                },
                |value| {
                    value.parse::<i32>().map_err(|_| {
                        let error_msg =
                            format!("invalid literal for int() with base 10: '{value}'");
                        PyValueError::new_err(error_msg)
                    })
                },
            )
    }

    fn set_autoaction(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let new_autoaction = value.str()?.to_string();
        if new_autoaction.parse::<i32>().is_err() {
            cold_path();
            let error_msg = format!("invalid literal for int() with base 10: '{new_autoaction}'");
            return Err(PyValueError::new_err(error_msg));
        }

        let mut new_cvars = parse_variables(&self.get().player_info.read().userinfo);
        new_cvars.set("autoaction", &new_autoaction);
        let new_cvars_string: String = new_cvars.into();

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(self.py(), self.get().id, &client_command).map(|_| ())
    }

    fn get_predictitems(&self) -> PyResult<i32> {
        parse_variables(&self.get().user_info)
            .get("cg_predictitems")
            .map_or(
                {
                    cold_path();
                    Err(PyKeyError::new_err("'cg_predictitems'"))
                },
                |value| {
                    value.parse::<i32>().map_err(|_| {
                        let error_msg =
                            format!("invalid literal for int() with base 10: '{value}'");
                        PyValueError::new_err(error_msg)
                    })
                },
            )
    }

    fn set_predictitems(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let new_predictitems = value.str()?.to_string();
        if new_predictitems.parse::<i32>().is_err() {
            cold_path();
            let error_msg = format!("invalid literal for int() with base 10: '{new_predictitems}'");
            return Err(PyValueError::new_err(error_msg));
        }

        let mut new_cvars = parse_variables(&self.get().player_info.read().userinfo);
        new_cvars.set("cg_predictitems", &new_predictitems);
        let new_cvars_string: String = new_cvars.into();

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(self.py(), self.get().id, &client_command).map(|_| ())
    }

    fn get_connection_state(&self) -> PyResult<String> {
        match clientState_t::try_from(self.get().player_info.read().connection_state)
            .map(ConnectionStates::from)
        {
            Err(_) => {
                cold_path();
                Err(PyValueError::new_err("invalid clientState"))
            }
            Ok(state) => Ok(state.to_string()),
        }
    }

    fn get_state(&self) -> PyResult<Option<PlayerState>> {
        pyshinqlx_player_state(self.py(), self.get().id)
    }

    fn get_privileges(&self) -> Option<String> {
        match privileges_t::from(self.get().player_info.read().privileges) {
            privileges_t::PRIV_MOD => Some("mod".to_string()),
            privileges_t::PRIV_ADMIN => Some("admin".to_string()),
            privileges_t::PRIV_ROOT => Some("root".to_string()),
            privileges_t::PRIV_BANNED => Some("banned".to_string()),
            _ => None,
        }
    }

    fn set_privileges(&self, value: Option<&str>) -> PyResult<()> {
        let new_privileges = self
            .py()
            .allow_threads(|| privileges_t::try_from(value.unwrap_or("none")));

        new_privileges.map_or(
            {
                cold_path();
                Err(PyValueError::new_err("Invalid privilege level."))
            },
            |new_privilege| {
                pyshinqlx_set_privileges(self.py(), self.get().id, new_privilege as i32).map(|_| ())
            },
        )
    }

    fn get_country(&self) -> PyResult<String> {
        parse_variables(&self.get().user_info)
            .get("country")
            .ok_or(PyKeyError::new_err("'country'"))
    }

    fn set_country(&self, value: &str) -> PyResult<()> {
        let mut new_cvars = parse_variables(&self.get().player_info.read().userinfo.to_owned());
        new_cvars.set("country", value);
        let new_cvars_string: String = new_cvars.into();

        let client_command = format!("userinfo \"{new_cvars_string}\"");
        pyshinqlx_client_command(self.py(), self.get().id, &client_command).map(|_| ())
    }

    fn get_valid(&self) -> bool {
        self.get().valid.load(Ordering::Acquire)
    }

    fn get_stats(&self) -> PyResult<Option<PlayerStats>> {
        pyshinqlx_player_stats(self.py(), self.get().id)
    }

    fn get_ping(&self) -> PyResult<i32> {
        pyshinqlx_player_stats(self.py(), self.get().id)
            .map(|opt_stats| opt_stats.map(|stats| stats.ping).unwrap_or(999))
    }

    fn position(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let pos = if reset {
            Vector3(0, 0, 0)
        } else {
            pyshinqlx_player_state(self.py(), self.get().id)?
                .map_or(Vector3(0, 0, 0), |state| state.position)
        };

        kwargs.map_or(pos.into_bound_py_any(self.py()), |py_kwargs| {
            let x = py_kwargs
                .get_item(intern!(self.py(), "x"))?
                .map_or(Ok(pos.0), |value| value.extract::<i32>())?;
            let y = py_kwargs
                .get_item(intern!(self.py(), "y"))?
                .map_or(Ok(pos.1), |value| value.extract::<i32>())?;
            let z = py_kwargs
                .get_item(intern!(self.py(), "z"))?
                .map_or(Ok(pos.2), |value| value.extract::<i32>())?;

            let vector = Vector3(x, y, z);

            pyshinqlx_set_position(self.py(), self.get().id, &vector)
                .map(|value| PyBool::new(self.py(), value).into_any().to_owned())
        })
    }

    fn velocity(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let vel = if reset {
            Vector3(0, 0, 0)
        } else {
            pyshinqlx_player_state(self.py(), self.get().id)?
                .map_or(Vector3(0, 0, 0), |state| state.velocity)
        };

        kwargs.map_or(Ok(vel.into_bound_py_any(self.py())?), |py_kwargs| {
            let x = py_kwargs
                .get_item(intern!(self.py(), "x"))?
                .map_or(Ok(vel.0), |value| value.extract::<i32>())?;
            let y = py_kwargs
                .get_item(intern!(self.py(), "y"))?
                .map_or(Ok(vel.1), |value| value.extract::<i32>())?;
            let z = py_kwargs
                .get_item(intern!(self.py(), "z"))?
                .map_or(Ok(vel.2), |value| value.extract::<i32>())?;

            let vector = Vector3(x, y, z);

            pyshinqlx_set_velocity(self.py(), self.get().id, &vector)
                .map(|value| PyBool::new(self.py(), value).into_any().to_owned())
        })
    }

    fn weapons(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let weaps = if reset {
            Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        } else {
            pyshinqlx_player_state(self.py(), self.get().id)?.map_or(
                Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                |state| state.weapons,
            )
        };

        kwargs.map_or(Ok(weaps.into_bound_py_any(self.py())?), |py_kwargs| {
            let g = py_kwargs
                .get_item(intern!(self.py(), "g"))?
                .map_or(Ok(weaps.0), |value| value.extract::<i32>())?;
            let mg = py_kwargs
                .get_item(intern!(self.py(), "mg"))?
                .map_or(Ok(weaps.1), |value| value.extract::<i32>())?;
            let sg = py_kwargs
                .get_item(intern!(self.py(), "sg"))?
                .map_or(Ok(weaps.2), |value| value.extract::<i32>())?;
            let gl = py_kwargs
                .get_item(intern!(self.py(), "gl"))?
                .map_or(Ok(weaps.3), |value| value.extract::<i32>())?;
            let rl = py_kwargs
                .get_item(intern!(self.py(), "rl"))?
                .map_or(Ok(weaps.4), |value| value.extract::<i32>())?;
            let lg = py_kwargs
                .get_item(intern!(self.py(), "lg"))?
                .map_or(Ok(weaps.5), |value| value.extract::<i32>())?;
            let rg = py_kwargs
                .get_item(intern!(self.py(), "rg"))?
                .map_or(Ok(weaps.6), |value| value.extract::<i32>())?;
            let pg = py_kwargs
                .get_item(intern!(self.py(), "pg"))?
                .map_or(Ok(weaps.7), |value| value.extract::<i32>())?;
            let bfg = py_kwargs
                .get_item(intern!(self.py(), "bfg"))?
                .map_or(Ok(weaps.8), |value| value.extract::<i32>())?;
            let gh = py_kwargs
                .get_item(intern!(self.py(), "gh"))?
                .map_or(Ok(weaps.9), |value| value.extract::<i32>())?;
            let ng = py_kwargs
                .get_item(intern!(self.py(), "ng"))?
                .map_or(Ok(weaps.10), |value| value.extract::<i32>())?;
            let pl = py_kwargs
                .get_item(intern!(self.py(), "pl"))?
                .map_or(Ok(weaps.11), |value| value.extract::<i32>())?;
            let cg = py_kwargs
                .get_item(intern!(self.py(), "cg"))?
                .map_or(Ok(weaps.12), |value| value.extract::<i32>())?;
            let hmg = py_kwargs
                .get_item(intern!(self.py(), "hmg"))?
                .map_or(Ok(weaps.13), |value| value.extract::<i32>())?;
            let hands = py_kwargs
                .get_item(intern!(self.py(), "hands"))?
                .map_or(Ok(weaps.14), |value| value.extract::<i32>())?;

            let weapons = Weapons(
                g, mg, sg, gl, rl, lg, rg, pg, bfg, gh, ng, pl, cg, hmg, hands,
            );

            pyshinqlx_set_weapons(self.py(), self.get().id, &weapons)
                .map(|value| PyBool::new(self.py(), value).into_any().to_owned())
        })
    }

    fn weapon(&self, new_weapon: Option<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
        new_weapon
            .map(|weapon| {
                weapon
                    .extract::<i32>()
                    .map_or(
                        weapon.extract::<String>().map_or(
                            {
                                cold_path();
                                Err("invalid weapon".to_string())
                            },
                            |py_string| weapon_t::try_from(py_string.as_str()),
                        ),
                        weapon_t::try_from,
                    )
                    .map_or(
                        {
                            cold_path();
                            Err(PyValueError::new_err("invalid new_weapon"))
                        },
                        |converted_weapon| {
                            pyshinqlx_set_weapon(self.py(), self.get().id, converted_weapon as i32)
                                .map(|value| PyBool::new(self.py(), value).into_any().to_owned())
                        },
                    )
            })
            .unwrap_or_else(|| {
                let weapon = pyshinqlx_player_state(self.py(), self.get().id)?
                    .map_or(weapon_t::WP_HANDS as i32, |state| state.weapon);

                Ok(PyInt::new(self.py(), weapon).into_any())
            })
    }

    fn ammo(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let ammos = if reset {
            Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        } else {
            pyshinqlx_player_state(self.py(), self.get().id)?.map_or(
                Weapons(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                |state| state.ammo,
            )
        };

        kwargs.map_or(Ok(ammos.into_bound_py_any(self.py())?), |py_kwargs| {
            let g = py_kwargs
                .get_item(intern!(self.py(), "g"))?
                .map_or(Ok(ammos.0), |value| value.extract::<i32>())?;
            let mg = py_kwargs
                .get_item(intern!(self.py(), "mg"))?
                .map_or(Ok(ammos.1), |value| value.extract::<i32>())?;
            let sg = py_kwargs
                .get_item(intern!(self.py(), "sg"))?
                .map_or(Ok(ammos.2), |value| value.extract::<i32>())?;
            let gl = py_kwargs
                .get_item(intern!(self.py(), "gl"))?
                .map_or(Ok(ammos.3), |value| value.extract::<i32>())?;
            let rl = py_kwargs
                .get_item(intern!(self.py(), "rl"))?
                .map_or(Ok(ammos.4), |value| value.extract::<i32>())?;
            let lg = py_kwargs
                .get_item(intern!(self.py(), "lg"))?
                .map_or(Ok(ammos.5), |value| value.extract::<i32>())?;
            let rg = py_kwargs
                .get_item(intern!(self.py(), "rg"))?
                .map_or(Ok(ammos.6), |value| value.extract::<i32>())?;
            let pg = py_kwargs
                .get_item(intern!(self.py(), "pg"))?
                .map_or(Ok(ammos.7), |value| value.extract::<i32>())?;
            let bfg = py_kwargs
                .get_item(intern!(self.py(), "bfg"))?
                .map_or(Ok(ammos.8), |value| value.extract::<i32>())?;
            let gh = py_kwargs
                .get_item(intern!(self.py(), "gh"))?
                .map_or(Ok(ammos.9), |value| value.extract::<i32>())?;
            let ng = py_kwargs
                .get_item(intern!(self.py(), "ng"))?
                .map_or(Ok(ammos.10), |value| value.extract::<i32>())?;
            let pl = py_kwargs
                .get_item(intern!(self.py(), "pl"))?
                .map_or(Ok(ammos.11), |value| value.extract::<i32>())?;
            let cg = py_kwargs
                .get_item(intern!(self.py(), "cg"))?
                .map_or(Ok(ammos.12), |value| value.extract::<i32>())?;
            let hmg = py_kwargs
                .get_item(intern!(self.py(), "hmg"))?
                .map_or(Ok(ammos.13), |value| value.extract::<i32>())?;
            let hands = py_kwargs
                .get_item(intern!(self.py(), "hands"))?
                .map_or(Ok(ammos.14), |value| value.extract::<i32>())?;

            let weapons = Weapons(
                g, mg, sg, gl, rl, lg, rg, pg, bfg, gh, ng, pl, cg, hmg, hands,
            );

            pyshinqlx_set_ammo(self.py(), self.get().id, &weapons)
                .map(|value| PyBool::new(self.py(), value).into_any().to_owned())
        })
    }

    fn powerups(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let powerups = if reset {
            Powerups(0, 0, 0, 0, 0, 0)
        } else {
            pyshinqlx_player_state(self.py(), self.get().id)?
                .map_or(Powerups(0, 0, 0, 0, 0, 0), |state| state.powerups)
        };

        kwargs.map_or(Ok(powerups.into_bound_py_any(self.py())?), |py_kwargs| {
            let quad = py_kwargs.get_item(intern!(self.py(), "quad"))?.map_or(
                Ok(powerups.0),
                |value| {
                    value
                        .extract::<f32>()
                        .map(|float_value| (float_value * 1000.0).floor() as i32)
                },
            )?;
            let bs = py_kwargs
                .get_item(intern!(self.py(), "battlesuit"))?
                .map_or(Ok(powerups.1), |value| {
                    value
                        .extract::<f32>()
                        .map(|float_value| (float_value * 1000.0).floor() as i32)
                })?;
            let haste = py_kwargs.get_item(intern!(self.py(), "haste"))?.map_or(
                Ok(powerups.2),
                |value| {
                    value
                        .extract::<f32>()
                        .map(|float_value| (float_value * 1000.0).floor() as i32)
                },
            )?;
            let invis = py_kwargs
                .get_item(intern!(self.py(), "invisibility"))?
                .map_or(Ok(powerups.3), |value| {
                    value
                        .extract::<f32>()
                        .map(|float_value| (float_value * 1000.0).floor() as i32)
                })?;
            let regen = py_kwargs
                .get_item(intern!(self.py(), "regeneration"))?
                .map_or(Ok(powerups.4), |value| {
                    value
                        .extract::<f32>()
                        .map(|float_value| (float_value * 1000.0).floor() as i32)
                })?;
            let invul = py_kwargs
                .get_item(intern!(self.py(), "invulnerability"))?
                .map_or(Ok(powerups.5), |value| {
                    value
                        .extract::<f32>()
                        .map(|float_value| (float_value * 1000.0).floor() as i32)
                })?;

            let powerups = Powerups(quad, bs, haste, invis, regen, invul);

            pyshinqlx_set_powerups(self.py(), self.get().id, &powerups)
                .map(|value| PyBool::new(self.py(), value).into_any().to_owned())
        })
    }

    fn get_holdable(&self) -> PyResult<Option<String>> {
        pyshinqlx_player_state(self.py(), self.get().id).map(|opt_state| {
            opt_state
                .filter(|state| state.holdable != Holdable::None)
                .map(|state| state.holdable.to_string())
        })
    }

    fn set_holdable(&self, holdable: Option<&str>) -> PyResult<()> {
        match Holdable::from(holdable) {
            Holdable::Unknown => {
                cold_path();
                Err(PyValueError::new_err("Invalid holdable item."))
            }
            Holdable::Flight => {
                pyshinqlx_set_holdable(self.py(), self.get().id, Holdable::Flight.into())?;
                let flight = Flight(16000, 16000, 1200, 0);
                pyshinqlx_set_flight(self.py(), self.get().id, &flight).map(|_| ())
            }
            value => pyshinqlx_set_holdable(self.py(), self.get().id, value.into()).map(|_| ()),
        }
    }

    fn drop_holdable(&self) -> PyResult<()> {
        pyshinqlx_drop_holdable(self.py(), self.get().id).map(|_| ())
    }

    fn flight(
        &self,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let opt_state = pyshinqlx_player_state(self.py(), self.get().id)?;
        let init_flight = if !opt_state
            .as_ref()
            .is_some_and(|state| state.holdable == Holdable::Flight)
        {
            self.set_holdable(Some("flight"))?;
            true
        } else {
            reset
        };

        let flight = if init_flight {
            Flight(16_000, 16_000, 1_200, 0)
        } else {
            opt_state.map_or(Flight(16_000, 16_000, 1_200, 0), |state| state.flight)
        };

        kwargs.map_or(Ok(flight.into_bound_py_any(self.py())?), |py_kwargs| {
            let fuel = py_kwargs
                .get_item(intern!(self.py(), "fuel"))?
                .map_or(Ok(flight.0), |value| value.extract::<i32>())?;
            let max_fuel = py_kwargs
                .get_item(intern!(self.py(), "max_fuel"))?
                .map_or(Ok(flight.1), |value| value.extract::<i32>())?;
            let thrust = py_kwargs
                .get_item(intern!(self.py(), "thrust"))?
                .map_or(Ok(flight.2), |value| value.extract::<i32>())?;
            let refuel = py_kwargs
                .get_item(intern!(self.py(), "refuel"))?
                .map_or(Ok(flight.3), |value| value.extract::<i32>())?;

            let flight = Flight(fuel, max_fuel, thrust, refuel);

            pyshinqlx_set_flight(self.py(), self.get().id, &flight)
                .map(|value| PyBool::new(self.py(), value).into_any().to_owned())
        })
    }

    fn get_noclip(&self) -> PyResult<bool> {
        pyshinqlx_player_state(self.py(), self.get().id)
            .map(|opt_state| opt_state.map(|state| state.noclip).unwrap_or(false))
    }

    fn set_noclip(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let noclip_value = value.extract::<bool>().unwrap_or_else(|_| {
            value
                .extract::<i64>()
                .map(|value| value != 0)
                .unwrap_or_else(|_| {
                    value
                        .extract::<String>()
                        .map(|value| !value.is_empty())
                        .unwrap_or(!value.is_none())
                })
        });
        pyshinqlx_noclip(self.py(), self.get().id, noclip_value).map(|_| ())
    }

    fn get_health(&self) -> PyResult<i32> {
        pyshinqlx_player_state(self.py(), self.get().id)
            .map(|opt_state| opt_state.map(|state| state.health).unwrap_or(0))
    }

    fn set_health(&self, value: i32) -> PyResult<()> {
        pyshinqlx_set_health(self.py(), self.get().id, value).map(|_| ())
    }

    fn get_armor(&self) -> PyResult<i32> {
        pyshinqlx_player_state(self.py(), self.get().id)
            .map(|opt_state| opt_state.map(|state| state.armor).unwrap_or(0))
    }

    fn set_armor(&self, value: i32) -> PyResult<()> {
        pyshinqlx_set_armor(self.py(), self.get().id, value).map(|_| ())
    }

    fn get_is_alive(&self) -> PyResult<bool> {
        pyshinqlx_player_state(self.py(), self.get().id)
            .map(|opt_state| opt_state.map(|state| state.is_alive).unwrap_or(false))
    }

    fn set_is_alive(&self, value: bool) -> PyResult<()> {
        let current = self.get_is_alive()?;

        match (current, value) {
            (false, true) => pyshinqlx_player_spawn(self.py(), self.get().id).map(|_| ()),
            (true, false) => {
                self.set_health(0)?;
                #[allow(irrefutable_let_patterns)]
                if let Ok(mut client_entity) = self.get().id.try_conv::<GameEntity>() {
                    let client_number = client_entity.get_client_number();
                    MAIN_ENGINE.load().as_ref().tap_some(|main_engine| {
                        main_engine.game_add_event(
                            client_entity.borrow_mut(),
                            entity_event_t::EV_DEATH1,
                            client_number,
                        );
                    });
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn get_is_frozen(&self) -> PyResult<bool> {
        pyshinqlx_player_state(self.py(), self.get().id)
            .map(|opt_state| opt_state.map(|state| state.is_frozen).unwrap_or(false))
    }

    fn get_is_chatting(&self) -> PyResult<bool> {
        pyshinqlx_player_state(self.py(), self.get().id)
            .map(|opt_state| opt_state.map(|state| state.is_chatting).unwrap_or(false))
    }

    fn get_score(&self) -> PyResult<i32> {
        pyshinqlx_player_stats(self.py(), self.get().id)
            .map(|opt_stats| opt_stats.map(|stats| stats.score).unwrap_or(0))
    }

    fn set_score(&self, value: i32) -> PyResult<()> {
        pyshinqlx_set_score(self.py(), self.get().id, value).map(|_| ())
    }

    fn get_channel(&self) -> Option<Bound<'py, TellChannel>> {
        Bound::new(
            self.py(),
            TellChannel::py_new(
                self.py(),
                self.get(),
                self.py().None().bind(self.py()),
                None,
            ),
        )
        .ok()
    }

    fn center_print(&self, msg: &str) -> PyResult<()> {
        let cmd = format!(r#"cp "{msg}""#);
        pyshinqlx_send_server_command(self.py(), Some(self.get().id), &cmd).map(|_| ())
    }

    fn tell(&self, msg: &str, kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<()> {
        self.get_channel().map_or(
            {
                cold_path();
                Err(PyNotImplementedError::new_err("Player TellChannel"))
            },
            |tell_channel| {
                let limit = kwargs
                    .and_then(|pydict| {
                        pydict
                            .get_item(intern!(self.py(), "limit"))
                            .ok()
                            .flatten()
                            .and_then(|value| value.extract::<i32>().ok())
                    })
                    .unwrap_or(100i32);

                let delimiter = kwargs
                    .and_then(|pydict| {
                        pydict
                            .get_item(intern!(self.py(), "delimiter"))
                            .ok()
                            .flatten()
                            .and_then(|value| value.extract::<String>().ok())
                    })
                    .unwrap_or(" ".to_owned());

                tell_channel.as_super().reply(msg, limit, &delimiter)
            },
        )
    }

    fn kick(&self, reason: &str) -> PyResult<()> {
        pyshinqlx_kick(self.py(), self.get().id, Some(reason))
    }

    fn ban(&self) -> PyResult<()> {
        ban(self.py(), self.as_any())
    }

    fn tempban(&self) -> PyResult<()> {
        tempban(self.py(), self.as_any())
    }

    fn addadmin(&self) -> PyResult<()> {
        addadmin(self.py(), self.as_any())
    }

    fn addmod(&self) -> PyResult<()> {
        addmod(self.py(), self.as_any())
    }

    fn demote(&self) -> PyResult<()> {
        demote(self.py(), self.as_any())
    }

    fn mute(&self) -> PyResult<()> {
        mute(self.py(), self.as_any())
    }

    fn unmute(&self) -> PyResult<()> {
        unmute(self.py(), self.as_any())
    }

    fn put(&self, team: &str) -> PyResult<()> {
        put(self.py(), self.as_any(), team)
    }

    fn addscore(&self, score: i32) -> PyResult<()> {
        addscore(self.py(), self.as_any(), score)
    }

    fn switch(&self, other_player: &Bound<'_, Player>) -> PyResult<()> {
        let own_team = self.get_team()?;
        let other_team = other_player.get_team()?;

        if own_team == other_team {
            return Err(PyValueError::new_err("Both players are on the same team."));
        }

        self.put(&other_team)?;
        other_player.put(&own_team)
    }

    fn slap(&self, damage: i32) -> PyResult<()> {
        let slap_cmd = format!("slap {} {}", self.get().id, damage);
        console_command(&slap_cmd)
    }

    fn slay(&self) -> PyResult<()> {
        let slay_cmd = format!("slay {}", self.get().id);
        console_command(&slay_cmd)
    }

    fn slay_with_mod(&self, means_of_death: i32) -> PyResult<()> {
        pyshinqlx_slay_with_mod(self.py(), self.get().id, means_of_death).map(|_| ())
    }
}

#[cfg(test)]
mod pyshinqlx_player_tests {
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

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn repr_with_an_invalidated_player(_pyshinqlx_setup: ()) {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        let result = Python::with_gil(|py| {
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
        let result = Python::with_gil(|py| {
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
                Python::with_gil(|py| {
                    let player = Bound::new(
                        py,
                        Player {
                            steam_id: 1234567890,
                            ..default_test_player()
                        },
                    )
                    .expect("this should not happen");

                    let result = player.update();
                    assert!(
                        result.is_err_and(|err| err.is_instance_of::<NonexistentPlayerError>(py))
                    );
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
                let result = Python::with_gil(|py| {
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
                Python::with_gil(|py| {
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
                Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                client.is_some()
                    && cmd == r#"userinfo "\asdf\qwertz\name\UnnamedPlayer""#
                    && client_ok
            })
            .times(1);

        MockEngineBuilder::default().with_max_clients(16).run(|| {
            Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");
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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");
                    let result = player.get_clan();
                    assert_eq!(result, "asdf");
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_clan_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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
                        *index == 531i32
                            && value.contains(r"\cn\clan")
                            && value.contains(r"\xcn\clan")
                    })
                    .times(1);
            })
            .run(|| {
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    player.set_clan("clan")
                });
            });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_name_for_color_terminated_name(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
                let player = Bound::new(
                    py,
                    Player {
                        user_info: r"\asdf\qwertz\color1\7.0\color2\5\name\UnnamedPlayer"
                            .to_string(),
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
                let player = Bound::new(
                    py,
                    Player {
                        user_info: r"\asdf\qwertz\headmodel\Anarki\name\UnnamedPlayer".to_string(),
                        player_info: PlayerInfo {
                            userinfo: r"\asdf\qwertz\headmodel\Anarki\name\UnnamedPlayer"
                                .to_string(),
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
                let player = Bound::new(
                    py,
                    Player {
                        user_info: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer".to_string(),
                        player_info: PlayerInfo {
                            userinfo: r"\asdf\qwertz\cg_predictitems\1\name\UnnamedPlayer"
                                .to_string(),
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.set_privileges(Some("root"));
            assert!(result.is_err_and(|err| err.is_instance_of::<PyValueError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_country_when_country_is_set(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
                        let player =
                            Bound::new(py, default_test_player()).expect("this should not happen");

                        let result =
                            player.weapon(Some(PyString::intern(py, weapon_str).into_any()));
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
    fn set_noclip_set_players_noclip_value_by_bool(
        _pyshinqlx_setup: (),
        #[case] noclip_value: bool,
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.get_is_alive();
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn set_is_alive_with_no_main_engine(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
    fn get_is_frozen_returns_players_is_frozen_state(
        _pyshinqlx_setup: (),
        #[case] is_frozen: bool,
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
            let player = Bound::new(py, default_test_player()).expect("this should not happen");

            let result = player.get_is_chatting();
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn get_score_when_main_engine_not_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
                    Python::with_gil(|py| {
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
        Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
            Python::with_gil(|py| {
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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.unmute();
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn put_with_invalid_team(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

                    let result = player.addscore(42);
                    assert!(result.is_ok());
                });
            });
    }

    #[rstest]
    #[serial]
    #[cfg_attr(miri, ignore)]
    fn switch_with_player_on_same_team(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
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
                Python::with_gil(|py| {
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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                Python::with_gil(|py| {
                    let player =
                        Bound::new(py, default_test_player()).expect("this should not happen");

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
                    Python::with_gil(|py| {
                        let player =
                            Bound::new(py, default_test_player()).expect("this should not happen");

                        let result =
                            player.slay_with_mod(meansOfDeath_t::MOD_PROXIMITY_MINE as i32);
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
            let all_players = Python::with_gil(|py| Player::all_players(&py.get_type::<Player>()));
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
    #[pyo3(signature = (name = "DummyPlayer", *args, **kwargs), text_signature = "(name = \"DummyPlayer\")")]
    fn py_new(
        name: &str,
        #[allow(unused_variables)] args: &Bound<'_, PyAny>,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyAny>>,
    ) -> PyClassInitializer<Self> {
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

    #[pyo3(name = "__init__", signature = (name = "DummyPlayer"), text_signature = "(name = \"DummyPlayer\")")]
    pub(crate) fn initialize(slf: &Bound<'_, Self>, name: &str) {
        let player = slf.as_super();
        *player.get().name.write() = name.into();
    }

    #[getter(id)]
    fn get_id(slf: &Bound<'_, Self>) -> PyResult<i32> {
        slf.get_id()
    }

    #[getter(steam_id)]
    fn get_steam_id(slf: &Bound<'_, Self>) -> PyResult<i64> {
        slf.get_steam_id()
    }

    fn update(slf: &Bound<'_, Self>) -> PyResult<()> {
        slf.update()
    }

    #[getter(channel)]
    fn get_channel<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        slf.get_channel()
    }

    #[pyo3(signature = (msg, **kwargs))]
    fn tell(slf: &Bound<'_, Self>, msg: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        slf.tell(msg, kwargs)
    }
}

pub(crate) trait AbstractDummyPlayerMethods<'py> {
    fn get_id(&self) -> PyResult<i32>;
    fn get_steam_id(&self) -> PyResult<i64>;
    fn update(&self) -> PyResult<()>;
    fn get_channel(&self) -> PyResult<Bound<'py, PyAny>>;
    fn tell(&self, msg: &str, kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<()>;
}

impl<'py> AbstractDummyPlayerMethods<'py> for Bound<'py, AbstractDummyPlayer> {
    fn get_id(&self) -> PyResult<i32> {
        Err(PyAttributeError::new_err(
            "Dummy players do not have client IDs.",
        ))
    }

    fn get_steam_id(&self) -> PyResult<i64> {
        Err(PyNotImplementedError::new_err(
            "steam_id property needs to be implemented.",
        ))
    }

    fn update(&self) -> PyResult<()> {
        Ok(())
    }

    fn get_channel(&self) -> PyResult<Bound<'py, PyAny>> {
        Err(PyNotImplementedError::new_err(
            "channel property needs to be implemented.",
        ))
    }

    fn tell(
        &self,
        #[allow(unused_variables)] msg: &str,
        #[allow(unused_variables)] kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "tell() needs to be implemented.",
        ))
    }
}

#[cfg(test)]
mod pyshinqlx_abstract_dummy_player_tests {
    use pyo3::exceptions::{PyAttributeError, PyNotImplementedError};
    use rstest::*;

    use super::{AbstractDummyPlayer, AbstractDummyPlayerMethods};
    use crate::{
        ffi::python::prelude::*,
        prelude::{MockEngineBuilder, serial},
    };

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
    #[serial]
    fn dummy_player_can_be_subclassed(_pyshinqlx_setup: ()) {
        MockEngineBuilder::default().with_max_clients(16).run(|| {
            let result = Python::with_gil(|py| {
                py.run(
                    cr#"
from shinqlx import AbstractDummyPlayer

class ConcreteDummyPlayer(AbstractDummyPlayer):
    def __init__(self, name):
        super().__init__(name)

player = ConcreteDummyPlayer("asdf")
            "#,
                    None,
                    None,
                )
            });
            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_id_returns_attribute_error(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.get_id();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyAttributeError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_steam_id_returns_not_implemented_error(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.get_steam_id();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn update_does_nothing(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.update();

            assert!(result.is_ok());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn get_channel_returns_not_implemented_error(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.get_channel();

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    fn tell_returns_not_implemented_error(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let player = Bound::new(
                py,
                AbstractDummyPlayer::py_new("DummyPlayer", py.None().bind(py), None),
            )
            .expect("this should not happen");

            let result = player.tell("asdf", None);

            assert!(result.is_err_and(|err| err.is_instance_of::<PyNotImplementedError>(py)));
        });
    }
}

#[pyclass(module = "_player", name = "RconDummyPlayer", extends = AbstractDummyPlayer, frozen)]
pub(crate) struct RconDummyPlayer;

#[pymethods]
impl RconDummyPlayer {
    #[new]
    #[pyo3(signature = (*args, **kwargs), text_signature = "()")]
    pub(crate) fn py_new(
        py: Python<'_>,
        #[allow(unused_variables)] args: &Bound<'_, PyAny>,
        #[allow(unused_variables)] kwargs: Option<&Bound<'_, PyAny>>,
    ) -> PyClassInitializer<Self> {
        AbstractDummyPlayer::py_new("RconDummyPlayer", py.None().bind(py), None)
            .add_subclass(Self {})
    }

    #[getter(steam_id)]
    fn get_steam_id(slf: &Bound<'_, Self>) -> PyResult<i64> {
        slf.get_steam_id()
    }

    #[getter(channel)]
    fn get_channel<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, ConsoleChannel>> {
        slf.get_channel()
    }

    #[pyo3(signature = (msg, **kwargs))]
    fn tell<'py>(
        slf: &Bound<'py, Self>,
        msg: &str,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<()> {
        slf.tell(msg, kwargs)
    }
}

pub(crate) trait RconDummyPlayerMethods<'py> {
    fn get_steam_id(&self) -> PyResult<i64>;
    fn get_channel(&self) -> PyResult<Bound<'py, ConsoleChannel>>;
    fn tell(&self, msg: &str, kwargs: Option<&Bound<'py, PyDict>>) -> PyResult<()>;
}

impl<'py> RconDummyPlayerMethods<'py> for Bound<'py, RconDummyPlayer> {
    fn get_steam_id(&self) -> PyResult<i64> {
        self.py()
            .allow_threads(|| owner().map(|opt_value| opt_value.unwrap_or_default()))
    }

    fn get_channel(&self) -> PyResult<Bound<'py, ConsoleChannel>> {
        CONSOLE_CHANNEL.load().as_ref().map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to CONSOLE_CHANNEL",
                ))
            },
            |console_channel| Ok(console_channel.bind(self.py()).to_owned()),
        )
    }

    fn tell(
        &self,
        msg: &str,
        #[allow(unused_variables)] kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<()> {
        CONSOLE_CHANNEL.load().as_ref().map_or(
            {
                cold_path();
                Err(PyEnvironmentError::new_err(
                    "could not get access to CONSOLE_CHANNEL",
                ))
            },
            |console_channel| console_channel.bind(self.py()).reply(msg, 100, " "),
        )
    }
}

#[cfg(test)]
mod pyshinqlx_rcon_dummy_player_tests {
    use core::borrow::BorrowMut;

    use mockall::predicate;
    use pyo3::{exceptions::PyEnvironmentError, prelude::*};
    use rstest::*;

    use super::{RconDummyPlayer, RconDummyPlayerMethods};
    use crate::{
        ffi::{
            c::prelude::{CVar, CVarBuilder, cvar_t},
            python::{CONSOLE_CHANNEL, prelude::*},
        },
        hooks::mock_hooks::shinqlx_com_printf_context,
        prelude::*,
    };

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
    fn dummy_player_has_default_name(_pyshinqlx_setup: ()) {
        let result = Python::with_gil(|py| {
            py.run(
                cr#"
from shinqlx import RconDummyPlayer

rcon_player = RconDummyPlayer()

assert(rcon_player.name == "RconDummyPlayer^7")
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
                        Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                            .expect("this should not happen");

                    let result = rcon_dummy_player.get_steam_id();
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
                Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");

            let result = rcon_dummy_player.get_channel();
            assert!(result.is_err_and(|err| err.is_instance_of::<PyEnvironmentError>(py)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn get_channel_with_console_channel_properly_initialized(_pyshinqlx_setup: ()) {
        Python::with_gil(|py| {
            let console_channel = Py::new(py, ConsoleChannel::py_new(py, py.None().bind(py), None))
                .expect("this should not happen");
            CONSOLE_CHANNEL.store(Some(console_channel.into()));

            let rcon_dummy_player =
                Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");

            let result = rcon_dummy_player.get_channel();
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
                Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");

            let result = rcon_dummy_player.tell("asdf", None);
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
                Bound::new(py, ConsoleChannel::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");
            CONSOLE_CHANNEL.store(Some(console_channel.unbind().into()));

            let rcon_dummy_player =
                Bound::new(py, RconDummyPlayer::py_new(py, py.None().bind(py), None))
                    .expect("this should not happen");

            let result = rcon_dummy_player.tell("asdf", None);
            assert!(result.is_ok());
        });
    }
}
