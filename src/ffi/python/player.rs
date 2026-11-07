use core::{
    borrow::BorrowMut,
    hint::cold_path,
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};

use derive_more::Display;
use itertools::Itertools;
use pyo3::{
    BoundObject, IntoPyObjectExt,
    basic::CompareOp,
    create_exception,
    exceptions::{PyException, PyKeyError, PyNotImplementedError, PyValueError},
    intern,
    types::{IntoPyDict, PyBool, PyDict, PyInt, PyNotImplemented, PyType},
};
use tap::{TapOptional, TryConv};

use super::{
    ConnectionStates, Teams, addadmin, addmod, addscore, ban, console_command, demote, mute,
    prelude::*, put, tempban, unmute,
};
use crate::{
    MAIN_ENGINE,
    ffi::c::prelude::*,
    quake_live_engine::{GameAddEvent, GetConfigstring, SetConfigstring},
};

create_exception!(pyshinqlx_module, NonexistentPlayerError, PyException);

mod dummy;
pub(crate) use self::dummy::{AbstractDummyPlayer, RconDummyPlayer};

impl FromStr for privileges_t {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
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

impl FromStr for weapon_t {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
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
#[pyclass(
    module = "_player",
    name = "Player",
    subclass,
    frozen,
    str,
    from_py_object
)]
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

impl From<PlayerInfo> for Player {
    fn from(player_info: PlayerInfo) -> Self {
        // When a player connects, the name field in the client struct has yet to be initialized,
        // so we fall back to the userinfo and try parse it ourselves to get the name if needed.
        let name = if player_info.name.is_empty() {
            parse_variables(&player_info.userinfo)
                .get("name")
                .unwrap_or_default()
        } else {
            player_info.name.to_owned()
        };

        Player {
            valid: true.into(),
            id: player_info.client_id,
            user_info: player_info.userinfo.to_owned(),
            steam_id: player_info.steam_id,
            player_info: player_info.into(),
            name: name.into(),
        }
    }
}

impl Player {
    /// Creates a player for the client id. Without further information the player data is
    /// read from the engine. Unlike `py_new` this cannot fail.
    pub(crate) fn from_client_id(client_id: i32, info: Option<PlayerInfo>) -> Self {
        Player {
            id: client_id,
            ..Player::from(info.unwrap_or_else(|| PlayerInfo::from(client_id)))
        }
    }
}

#[pymethods]
impl Player {
    #[new]
    #[pyo3(signature = (client_id, info = None), text_signature = "(client_id, info = None)")]
    pub(crate) fn py_new(client_id: i32, info: Option<PlayerInfo>) -> PyResult<Self> {
        Ok(Self::from_client_id(client_id, info))
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
        let negate = match op {
            CompareOp::Eq => false,
            CompareOp::Ne => true,
            _ => return Ok(PyNotImplemented::get(slf.py()).into_any()),
        };
        let steam_id = slf.get().steam_id;
        let equal = other
            .cast::<Self>()
            .map(|other_player| other_player.get().steam_id == steam_id)
            .ok()
            .or_else(|| {
                other
                    .extract::<i64>()
                    .ok()
                    .map(|other_steam_id| other_steam_id == steam_id)
            })
            .unwrap_or(false);
        Ok(PyBool::new(slf.py(), equal != negate).into_any())
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
        py.detach(|| match Teams::from(self.player_info.read().team) {
            Teams::Invalid => {
                cold_path();
                Err(PyValueError::new_err("invalid team"))
            }
            team => Ok(team.to_string()),
        })
    }

    #[setter(team)]
    fn set_team(slf: &Bound<'_, Self>, new_team: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.set_team(&name_of(new_team)?)
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
    fn set_privileges(slf: &Bound<'_, Self>, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let value = value.map(name_of).transpose()?;
        slf.set_privileges(value.as_deref())
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

    #[getter(valid)]
    fn is_valid(slf: &Bound<'_, Self>) -> bool {
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

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = False, **kwargs)")]
    fn position<'py>(
        slf: &Bound<'py, Self>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.position(reset, kwargs)
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = False, **kwargs)")]
    fn velocity<'py>(
        slf: &Bound<'py, Self>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.velocity(reset, kwargs)
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = False, **kwargs)")]
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

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = False, **kwargs)")]
    fn ammo<'py>(
        slf: &Bound<'py, Self>,
        reset: bool,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        slf.ammo(reset, kwargs)
    }

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = False, **kwargs)")]
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

    #[pyo3(signature = (reset = false, **kwargs), text_signature = "(reset = False, **kwargs)")]
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

    pub(crate) fn put(slf: &Bound<'_, Self>, team: &Bound<'_, PyAny>) -> PyResult<()> {
        slf.put(&name_of(team)?)
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
        cls.py().detach(|| {
            Ok(players_info
                .into_iter()
                .flatten()
                .map(Player::from)
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
    #[cfg_attr(not(test), allow(dead_code, reason = "only called from tests"))]
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
            .ok_or_else(|| PyKeyError::new_err(format!("'{item}'")))
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .ok_or_else(|| PyKeyError::new_err("'model'"))
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
            .ok_or_else(|| PyKeyError::new_err("'headmodel'"))
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
            .ok_or_else(|| PyKeyError::new_err("'handicap'"))
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
            .map_or_else(
                || {
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
            .map_or_else(
                || {
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
            .map_or_else(
                || {
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
            .detach(|| value.unwrap_or("none").parse::<privileges_t>());

        new_privileges.map_or_else(
            |_| {
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
            .ok_or_else(|| PyKeyError::new_err("'country'"))
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

        kwargs.map_or(pos.clone().into_bound_py_any(self.py()), |py_kwargs| {
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

        kwargs.map_or(Ok(vel.clone().into_bound_py_any(self.py())?), |py_kwargs| {
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

        kwargs.map_or(
            Ok(weaps.clone().into_bound_py_any(self.py())?),
            |py_kwargs| {
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
            },
        )
    }

    fn weapon(&self, new_weapon: Option<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
        new_weapon
            .map(|weapon| {
                weapon
                    .extract::<i32>()
                    .map_or(
                        weapon.extract::<String>().map_or_else(
                            |_| {
                                cold_path();
                                Err("invalid weapon".to_string())
                            },
                            |py_string| py_string.parse::<weapon_t>(),
                        ),
                        weapon_t::try_from,
                    )
                    .map_or_else(
                        |_| {
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

        kwargs.map_or(
            Ok(ammos.clone().into_bound_py_any(self.py())?),
            |py_kwargs| {
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
            },
        )
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

        kwargs.map_or(
            Ok(powerups.clone().into_bound_py_any(self.py())?),
            |py_kwargs| {
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
            },
        )
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

        kwargs.map_or(
            Ok(flight.clone().into_bound_py_any(self.py())?),
            |py_kwargs| {
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
            },
        )
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
                #[allow(
                    irrefutable_let_patterns,
                    reason = "the conversion is infallible for the mocked types"
                )]
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
        self.get_channel().map_or_else(
            || {
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
                    .unwrap_or_else(|| " ".to_owned());

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
mod property_tests;
#[cfg(test)]
mod tests;
