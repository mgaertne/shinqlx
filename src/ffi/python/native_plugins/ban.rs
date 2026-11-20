use std::collections::HashMap;

use parking_lot::Mutex;
use pyo3::{
    PyTraverseError,
    exceptions::{PyKeyError, PyValueError},
    gc::PyVisit,
    intern,
    prelude::*,
    types::{PyDict, PyInt},
};

use super::{
    create_cvar, create_cvar_limit, cvar_flag, cvar_float, cvar_int, register_command,
    register_hook,
    target::{INVALID_CLIENT_ID, Resolution, display_name, reply, resolve_text, stop_all, usage},
    timed_entries::{
        Entry, active_entries, expiry_datetime, latest_entry, length_in_seconds, revoke_entries,
        store_entry,
    },
};
use crate::ffi::python::{
    AbstractChannel, GameMethods, NonexistentPlayerError, Player, PlayerMethods, Plugin,
    commands::CommandPriorities,
    delay,
    plugin::{PluginMethods, chat},
};

const LEAVER_BAN_CVAR: &str = "qlx_leaverBan";
const MINIMUM_PERMISSION_TO_BE_PROTECTED: i32 = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
enum LeaveAction {
    Warn,
    Ban,
}

/// Bans players temporarily, also across server restarts, and optionally for leaving too many games.
#[pyclass(name = "ban", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Ban {
    players_start: Mutex<Vec<(i64, Py<Player>)>>,
    pending_warnings: Mutex<HashMap<i64, f64>>,
}

fn player_key(steam_id: i64) -> String {
    format!("minqlx:players:{steam_id}")
}

fn bans_key(steam_id: i64) -> String {
    format!("{}:bans", player_key(steam_id))
}

fn counter_key(steam_id: i64, counter: &str) -> String {
    format!("{}:{counter}", player_key(steam_id))
}

fn ban_notice(banned: &Entry) -> String {
    if banned.reason.is_empty() {
        format!("You are banned until {}.", banned.expires)
    } else {
        format!("You are banned until {}: {}", banned.expires, banned.reason)
    }
}

fn stored_count(database: &Bound<'_, PyAny>, key: &str) -> PyResult<Option<i64>> {
    let py = database.py();
    match database.get_item(key) {
        Ok(value) => py.get_type::<PyInt>().call1((value,))?.extract().map(Some),
        Err(err) if err.is_instance_of::<PyKeyError>(py) => Ok(None),
        Err(err) => Err(err),
    }
}

/// Decides whether a player has left too many games, and with which share of the games completed.
fn leave_action(
    completed: i64,
    left: i64,
    minimum_games: i64,
    warn_threshold: f64,
    ban_threshold: f64,
) -> Option<(Option<LeaveAction>, f64)> {
    let total = completed + left;
    if total == 0 {
        return None;
    }

    let ratio = if total < minimum_games {
        (completed + (minimum_games - total)) as f64 / minimum_games as f64
    } else {
        completed as f64 / total as f64
    };

    let action = if ratio <= warn_threshold && (ratio > ban_threshold || total < minimum_games) {
        Some(LeaveAction::Warn)
    } else if ratio <= ban_threshold && total >= minimum_games {
        Some(LeaveAction::Ban)
    } else {
        None
    };
    Some((action, completed as f64 / total as f64))
}

fn leave_status(
    database: &Bound<'_, PyAny>,
    steam_id: i64,
) -> PyResult<Option<(Option<LeaveAction>, f64)>> {
    let py = database.py();
    if !cvar_flag(py, LEAVER_BAN_CVAR)? {
        return Ok(None);
    }

    let (Some(completed), Some(left)) = (
        stored_count(database, &counter_key(steam_id, "games_completed"))?,
        stored_count(database, &counter_key(steam_id, "games_left"))?,
    ) else {
        return Ok(None);
    };

    Ok(leave_action(
        completed,
        left,
        cvar_int(py, "qlx_leaverBanMinimumGames")?,
        cvar_float(py, "qlx_leaverBanWarnThreshold")?,
        cvar_float(py, "qlx_leaverBanThreshold")?,
    ))
}

fn playing(py: Python<'_>) -> PyResult<Vec<Bound<'_, Player>>> {
    Plugin::players(&py.get_type::<Plugin>())?
        .into_iter()
        .filter(|player| {
            player
                .get_team(py)
                .is_ok_and(|team| team == "red" || team == "blue")
        })
        .map(|player| Bound::new(py, player))
        .collect()
}

fn is_in_progress(plugin: &Bound<'_, Ban>) -> PyResult<bool> {
    let Some(game) = plugin.as_super().get_game() else {
        return Ok(false);
    };
    Ok(Bound::new(plugin.py(), game)?.get_state()? == "in_progress")
}

#[pymethods]
impl Ban {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        self.players_start
            .lock()
            .iter()
            .try_for_each(|(_, started)| visit.call(started))
    }

    /// The expiry and the reason of the longest active ban of the player, for other plugins.
    fn is_banned<'py>(
        slf: &Bound<'py, Self>,
        steam_id: i64,
    ) -> PyResult<Option<(Bound<'py, PyAny>, String)>> {
        let database = slf.as_super().get_db()?;
        latest_entry(&database, &bans_key(steam_id))?
            .map(|banned| Ok((expiry_datetime(slf.py(), &banned.expires)?, banned.reason)))
            .transpose()
    }

    /// The action (`"warn"`, `"ban"` or `None`) and the share of completed games, for other plugins.
    fn leave_status(
        slf: &Bound<'_, Self>,
        steam_id: i64,
    ) -> PyResult<Option<(Option<&'static str>, f64)>> {
        let database = slf.as_super().get_db()?;
        Ok(
            leave_status(&database, steam_id)?.map(|(action, ratio)| match action {
                Some(LeaveAction::Warn) => (Some("warn"), ratio),
                Some(LeaveAction::Ban) => (Some("ban"), ratio),
                None => (None, ratio),
            }),
        )
    }

    fn warn_player(_slf: &Bound<'_, Self>, player: &Bound<'_, Player>, ratio: f64) -> PyResult<()> {
        player.tell(
            &format!(
                "^7You have only completed ^6{:.1}^7 percent of your games.",
                ratio * 100.0
            ),
            None,
        )?;
        player.tell("^7If you keep leaving you ^6will^7 be banned.", None)
    }

    fn handle_player_connect(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
    ) -> PyResult<Option<String>> {
        let steam_id = player.get_steam_id();
        let database = slf.as_super().get_db()?;
        match leave_status(&database, steam_id)? {
            Some((Some(LeaveAction::Ban), _)) => {
                return Ok(Some(
                    "You have been banned from this server for leaving too many games.".to_owned(),
                ));
            }
            Some((Some(LeaveAction::Warn), ratio)) => {
                slf.get().pending_warnings.lock().insert(steam_id, ratio);
            }
            _ => {}
        }

        Ok(latest_entry(&database, &bans_key(steam_id))?.map(|banned| ban_notice(&banned)))
    }

    fn handle_player_loaded(slf: &Bound<'_, Self>, player: &Bound<'_, Player>) -> PyResult<()> {
        let py = slf.py();
        delay(py, 4.0)?
            .call1((slf.getattr(intern!(py, "deliver_warning"))?,))?
            .call1((player,))
            .map(|_| ())
    }

    fn deliver_warning(slf: &Bound<'_, Self>, player: &Bound<'_, Player>) -> PyResult<()> {
        let py = slf.py();
        let steam_id = player.get_steam_id();
        if !slf.get().pending_warnings.lock().contains_key(&steam_id) {
            return Ok(());
        }

        match player.update() {
            Err(err) if err.is_instance_of::<NonexistentPlayerError>(py) => return Ok(()),
            result => result?,
        };
        let Some(ratio) = slf.get().pending_warnings.lock().remove(&steam_id) else {
            return Ok(());
        };

        Self::warn_player(slf, player, ratio)
    }

    fn handle_player_disconnect(
        &self,
        player: &Bound<'_, Player>,
        _reason: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let steam_id = player.get_steam_id();
        if playing(player.py())?.len() % 2 != 0 {
            self.players_start.lock().retain(|(id, _)| *id != steam_id);
        }
        Ok(())
    }

    fn handle_game_countdown(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        if cvar_flag(py, LEAVER_BAN_CVAR)? {
            chat(
                py,
                "Leavers are being kept track of. Repeat offenders ^6will^7 be banned.",
                100,
                " ",
            )?;
        }
        Ok(())
    }

    fn handle_game_start(slf: &Bound<'_, Self>, _game: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = slf.py();
        delay(py, 1.0)?
            .call1((slf.getattr(intern!(py, "remember_players"))?,))?
            .call0()
            .map(|_| ())
    }

    fn remember_players(&self, py: Python<'_>) -> PyResult<()> {
        let players = playing(py)?
            .into_iter()
            .map(|player| Ok((player.get_steam_id(), player.unbind())))
            .collect::<PyResult<Vec<_>>>()?;
        *self.players_start.lock() = players;
        Ok(())
    }

    fn handle_game_end(slf: &Bound<'_, Self>, data: &Bound<'_, PyDict>) -> PyResult<()> {
        let py = slf.py();
        let aborted = data
            .get_item(intern!(py, "ABORTED"))?
            .map(|aborted| aborted.is_truthy())
            .transpose()?
            .unwrap_or(false);
        if aborted {
            slf.get().players_start.lock().clear();
            return Ok(());
        }

        let players_end = playing(py)?
            .iter()
            .map(PlayerMethods::get_steam_id)
            .collect::<Vec<_>>();
        let (completed, leavers): (Vec<_>, Vec<_>) = slf
            .get()
            .players_start
            .lock()
            .iter()
            .map(|(steam_id, player)| (*steam_id, player.clone_ref(py)))
            .partition(|(steam_id, _)| players_end.contains(steam_id));

        let database = slf.as_super().get_db()?;
        let pipeline = database.call_method0(intern!(py, "pipeline"))?;
        for (steam_id, _) in &completed {
            pipeline.call_method1(
                intern!(py, "incr"),
                (counter_key(*steam_id, "games_completed"),),
            )?;
        }
        for (steam_id, _) in &leavers {
            pipeline.call_method1(intern!(py, "incr"), (counter_key(*steam_id, "games_left"),))?;
        }
        pipeline.call_method0(intern!(py, "execute"))?;

        *slf.get().players_start.lock() = completed;
        if leavers.is_empty() {
            return Ok(());
        }

        let names = leavers
            .iter()
            .map(|(_, player)| player.bind(py).get_clean_name())
            .collect::<Vec<_>>();
        chat(py, &format!("^7Leavers: ^6{}", names.join(" ")), 100, " ")?;
        slf.get().players_start.lock().clear();
        Ok(())
    }

    fn handle_team_switch(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        old_team: &str,
        new_team: &str,
    ) -> PyResult<()> {
        let py = slf.py();
        let steam_id = player.get_steam_id();
        let is_player_team = |team: &str| team == "red" || team == "blue";

        if is_player_team(old_team) && new_team == "spectator" && playing(py)?.len() % 2 == 0 {
            slf.get()
                .players_start
                .lock()
                .retain(|(id, _)| *id != steam_id);
        }

        if old_team == "spectator"
            && is_player_team(new_team)
            && is_in_progress(slf)?
            && !slf
                .get()
                .players_start
                .lock()
                .iter()
                .any(|(id, _)| *id == steam_id)
        {
            slf.get()
                .players_start
                .lock()
                .push((steam_id, player.clone().unbind()));
        }
        Ok(())
    }

    fn cmd_ban(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 4 {
            return usage();
        }
        let py = slf.py();
        let target = match resolve_text(py, player, &msg[1])? {
            Resolution::Target(target) => target,
            Resolution::UnknownClientId => return reply(channel, INVALID_CLIENT_ID),
            Resolution::Reported => return stop_all(),
        };
        let name = display_name(&target)?;
        let steam_id = target.steam_id();
        let database = slf.as_super().get_db()?;

        if database
            .call_method1(
                intern!(py, "has_permission"),
                (steam_id, MINIMUM_PERMISSION_TO_BE_PROTECTED),
            )?
            .is_truthy()?
        {
            return reply(
                channel,
                &format!(
                    "^6{name}^7 has permission level {MINIMUM_PERMISSION_TO_BE_PROTECTED} and cannot be banned."
                ),
            );
        }

        let Some(seconds) = length_in_seconds(&format!("{} {}", msg[2], msg[3])) else {
            return usage();
        };
        let banned = store_entry(
            &database,
            &bans_key(steam_id),
            &player.get_steam_id().into_pyobject(py)?.into_any(),
            seconds,
            &msg[4..].join(" "),
        )?;

        let kick_reason = format!(
            "has been banned until ^6{}^7: {}",
            banned.expires, banned.reason
        );
        match target.player().map(|player| player.kick(&kick_reason)) {
            Some(Ok(())) => Ok(None),
            Some(Err(err)) if !err.is_instance_of::<PyValueError>(py) => Err(err),
            _ => reply(
                channel,
                &format!(
                    "^6{name} ^7has been banned. Ban expires on ^6{}^7.",
                    banned.expires
                ),
            ),
        }
    }

    fn cmd_unban(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 2 {
            return usage();
        }
        let py = slf.py();
        let target = match resolve_text(py, player, &msg[1])? {
            Resolution::Target(target) => target,
            Resolution::UnknownClientId => return reply(channel, INVALID_CLIENT_ID),
            Resolution::Reported => return stop_all(),
        };
        let name = display_name(&target)?;

        let database = slf.as_super().get_db()?;
        let key = bans_key(target.steam_id());
        let bans = active_entries(&database, &key)?;
        if bans.is_empty() {
            return reply(channel, &format!("No active bans on ^6{name}^7 found."));
        }
        revoke_entries(&database, &key, &bans)?;
        reply(channel, &format!("^6{name}^7 has been unbanned."))
    }

    fn cmd_checkban(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 2 {
            return usage();
        }
        let py = slf.py();
        let target = match resolve_text(py, player, &msg[1])? {
            Resolution::Target(target) => target,
            Resolution::UnknownClientId => return reply(channel, INVALID_CLIENT_ID),
            Resolution::Reported => return stop_all(),
        };
        let name = display_name(&target)?;
        let steam_id = target.steam_id();
        let database = slf.as_super().get_db()?;

        let message = match latest_entry(&database, &bans_key(steam_id))? {
            Some(banned) if banned.reason.is_empty() => {
                format!("^6{name}^7 is banned until ^6{}^7.", banned.expires)
            }
            Some(banned) => format!(
                "^6{name}^7 is banned until ^6{}^7 for the following reason:^6 {}",
                banned.expires, banned.reason
            ),
            None if matches!(
                leave_status(&database, steam_id)?,
                Some((Some(LeaveAction::Ban), _))
            ) =>
            {
                format!("^6{name} ^7is banned for having left too many games.")
            }
            None => format!("^6{name} ^7is not banned."),
        };
        reply(channel, &message)
    }

    fn cmd_forgive(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 2 {
            return usage();
        }
        let py = slf.py();
        let target = match resolve_text(py, player, &msg[1])? {
            Resolution::Target(target) => target,
            Resolution::UnknownClientId => return reply(channel, INVALID_CLIENT_ID),
            Resolution::Reported => return stop_all(),
        };
        let name = display_name(&target)?;
        let steam_id = target.steam_id();
        let database = slf.as_super().get_db()?;

        if !database.contains(player_key(steam_id))? {
            return reply(channel, &format!("I do not know ^6{name}^7."));
        }

        let leaves_key = counter_key(steam_id, "games_left");
        let leaves = stored_count(&database, &leaves_key)?.unwrap_or(0);
        if leaves <= 0 {
            return reply(
                channel,
                &format!("^6{name}^7's leaves are already at ^6{leaves}^7."),
            );
        }

        let leaves_to_forgive = match msg.get(2) {
            None => 1,
            Some(count) => match count.parse::<i64>() {
                Ok(count) => count,
                Err(_) => {
                    return reply(
                        channel,
                        "Unintelligible number of leaves to forgive. Please use numbers.",
                    );
                }
            },
        };

        let new_leaves = leaves - leaves_to_forgive;
        if new_leaves <= 0 {
            database.set_item(&leaves_key, 0)?;
            return reply(
                channel,
                &format!("^6{name}^7's leaves have been reduced to ^60^7."),
            );
        }
        database.set_item(&leaves_key, new_leaves)?;
        reply(
            channel,
            &format!(
                "^6{leaves_to_forgive}^7 games have been forgiven, putting ^6{name}^7 at ^6{new_leaves}^7 leaves."
            ),
        )
    }
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Ban {
            players_start: Mutex::new(vec![]),
            pending_warnings: Mutex::new(HashMap::new()),
        }),
    )?;

    create_cvar(py, LEAVER_BAN_CVAR, "0")?;
    for (name, value) in [
        ("qlx_leaverBanThreshold", "0.63"),
        ("qlx_leaverBanWarnThreshold", "0.78"),
    ] {
        create_cvar_limit(py, name, value, "0", "1")?;
    }
    create_cvar(py, "qlx_leaverBanMinimumGames", "15")?;

    for (event, handler, priority) in [
        (
            "player_connect",
            "handle_player_connect",
            CommandPriorities::PRI_HIGH,
        ),
        (
            "player_loaded",
            "handle_player_loaded",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            "player_disconnect",
            "handle_player_disconnect",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            "game_countdown",
            "handle_game_countdown",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            "game_start",
            "handle_game_start",
            CommandPriorities::PRI_NORMAL,
        ),
        ("game_end", "handle_game_end", CommandPriorities::PRI_NORMAL),
        (
            "team_switch",
            "handle_team_switch",
            CommandPriorities::PRI_NORMAL,
        ),
    ] {
        register_hook(plugin.as_super(), event, handler, priority)?;
    }
    for (name, handler, permission, usage) in [
        (
            "ban",
            "cmd_ban",
            2,
            "<id> <length> seconds|minutes|hours|days|... [reason]",
        ),
        ("unban", "cmd_unban", 2, "<id>"),
        ("checkban", "cmd_checkban", 0, "<id>"),
        ("forgive", "cmd_forgive", 2, "<id> [leaves_to_forgive]"),
    ] {
        register_command(plugin.as_super(), &[name], handler, permission, usage, None)?;
    }

    Ok(plugin.into_any())
}

#[cfg(test)]
mod ban_tests {
    use core::ffi::CStr;
    use std::collections::HashMap;

    use pyo3::{
        prelude::*,
        types::{PyDict, PyTuple},
    };
    use rstest::rstest;

    use super::{Ban, LeaveAction, ban_notice, bans_key, counter_key, leave_action, player_key};
    use crate::{
        ffi::{
            c::prelude::{CS_SERVERINFO, team_t},
            python::{
                AbstractChannel, Player, PlayerMethods, Plugin, PythonReturnCodes,
                native_plugins::{
                    test_support::{
                        FakeRedis, REDIS_2, REDIS_5, ServerCommands, database_module,
                        real_player_with_id,
                    },
                    timed_entries::{Entry, now},
                },
                plugin::PluginMethods,
                pyshinqlx_setup_fixture::pyshinqlx_setup,
                pyshinqlx_test_support::{
                    RestoredAttribute, engine_with_cvars, recorded_replies, recording_channel,
                    with_connected_teamed_players,
                },
            },
        },
        prelude::*,
    };

    const CALLER_ID: i64 = 1001;
    const TARGET_ID: i64 = 1003;
    const OTHER_ID: i64 = 1004;
    const FAR_FUTURE: f64 = 4_000_000_000.0;
    const USAGE: i32 = PythonReturnCodes::RET_USAGE as i32;
    const LEAVER_CVARS: [(&str, &CStr); 4] = [
        ("qlx_leaverBan", c"1"),
        ("qlx_leaverBanMinimumGames", c"15"),
        ("qlx_leaverBanWarnThreshold", c"0.78"),
        ("qlx_leaverBanThreshold", c"0.63"),
    ];

    const SPECTATING: [team_t; 3] = [team_t::TEAM_SPECTATOR; 3];

    struct Fixtures<'py> {
        plugin: Bound<'py, Ban>,
        database: Bound<'py, PyAny>,
        channel: Bound<'py, AbstractChannel>,
        caller: Bound<'py, Player>,
        target: Bound<'py, Player>,
        other: Bound<'py, Player>,
        commands: ServerCommands,
    }

    impl Fixtures<'_> {
        fn replies(&self) -> Vec<String> {
            recorded_replies(&self.channel)
        }

        fn printed(&self) -> Vec<String> {
            self.commands.printed(self.plugin.py())
        }

        fn zsets(&self) -> HashMap<String, HashMap<String, f64>> {
            self.database
                .getattr("zsets")
                .and_then(|zsets| zsets.extract())
                .expect("could not get the sorted sets")
        }

        fn hashes(&self) -> HashMap<String, HashMap<String, String>> {
            self.database
                .getattr("hashes")
                .and_then(|hashes| hashes.extract())
                .expect("could not get the hashes")
        }

        fn strings_in_database(&self) -> HashMap<String, String> {
            self.database
                .getattr("strings")
                .and_then(|strings| strings.extract())
                .expect("could not get the stored strings")
        }

        fn store(&self, key: &str, value: &str) {
            self.database
                .getattr("strings")
                .and_then(|strings| strings.set_item(key, value))
                .expect("could not store the value");
        }

        fn ban(&self, steam_id: i64, ban_id: u32, score: f64, expires: &str, reason: &str) {
            let key = bans_key(steam_id);
            self.database
                .getattr("zsets")
                .and_then(|zsets| {
                    zsets
                        .call_method1("setdefault", (&key, PyDict::new(zsets.py())))?
                        .set_item(ban_id.to_string(), score)
                })
                .expect("could not store the score");
            self.database
                .getattr("hashes")
                .and_then(|hashes| {
                    let details = PyDict::new(hashes.py());
                    details.set_item("expires", expires)?;
                    details.set_item("reason", reason)?;
                    hashes.set_item(format!("{key}:{ban_id}"), details)
                })
                .expect("could not store the details");
        }

        fn grant(&self, steam_id: i64, level: i32) {
            self.database
                .getattr("permissions")
                .and_then(|permissions| permissions.set_item(steam_id, level))
                .expect("could not grant the permission");
        }

        fn start_with(&self, players: &[&Bound<'_, Player>]) {
            let mut start = self.plugin.get().players_start.lock();
            *start = players
                .iter()
                .map(|player| (player.get_steam_id(), (*player).clone().unbind()))
                .collect();
        }

        fn start_ids(&self) -> Vec<i64> {
            self.plugin
                .get()
                .players_start
                .lock()
                .iter()
                .map(|(id, _)| *id)
                .collect()
        }
    }

    fn with_ban_in<F>(
        engine: MockEngineBuilder,
        redis: &'static CStr,
        players: &[(i32, &'static str)],
        seating: [team_t; 3],
        test: F,
    ) where
        F: FnOnce(Fixtures<'_>),
    {
        let connected = [
            (1, "Caller^7", seating[0]),
            (3, "Eve^7", seating[1]),
            (4, "Mallory^7", seating[2]),
        ]
        .into_iter()
        .chain(
            players
                .iter()
                .map(|(id, name)| (*id, *name, team_t::TEAM_SPECTATOR)),
        )
        .collect::<Vec<_>>();
        with_connected_teamed_players(engine, &connected, || {
            let commands = ServerCommands::capture();
            Python::attach(|py| {
                let _fake_redis = FakeRedis::install(py, redis);
                let module = database_module(py);
                let caller = real_player_with_id(py, 1, CALLER_ID, "Caller^7", "");
                let target = real_player_with_id(py, 3, TARGET_ID, "Eve^7", "");
                let other = real_player_with_id(py, 4, OTHER_ID, "Mallory^7", "");
                for (name, player) in [("caller", &caller), ("target", &target), ("other", &other)]
                {
                    module
                        .setattr(name, player)
                        .expect("could not publish the player");
                }
                let plugin_class = py.get_type::<Plugin>();
                let _restore_database =
                    RestoredAttribute::capture(plugin_class.as_any(), "database");
                plugin_class
                    .setattr("database", module.getattr("Database").expect("no Database"))
                    .expect("could not set the database");

                let plugin = Bound::new(
                    py,
                    PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Ban {
                        players_start: Default::default(),
                        pending_warnings: Default::default(),
                    }),
                )
                .expect("could not create the plugin");
                let database = plugin.as_super().get_db().expect("no database");
                test(Fixtures {
                    plugin,
                    database,
                    channel: recording_channel(py),
                    caller,
                    target,
                    other,
                    commands,
                });
            });
        });
    }

    fn with_ban<F>(redis: &'static CStr, players: &[(i32, &'static str)], test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        with_ban_in(
            engine_with_cvars(&[("qlx_leaverBan", c"0")]),
            redis,
            players,
            SPECTATING,
            test,
        );
    }

    fn with_leaver_ban<F>(game_state: Option<&'static str>, test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        with_seated_leaver_ban(game_state, SPECTATING, test);
    }

    fn with_seated_leaver_ban<F>(game_state: Option<&'static str>, seating: [team_t; 3], test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        let engine = engine_with_cvars(&LEAVER_CVARS);
        let engine = match game_state {
            Some(state) => engine.with_get_configstring(
                CS_SERVERINFO as u16,
                format!(r"\g_gameState\{state}"),
                0..,
            ),
            None => engine,
        };
        with_ban_in(engine, REDIS_5, &[], seating, test);
    }

    fn msg(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    fn run_scheduled_task(py: Python<'_>, before: usize) {
        let scheduler = py
            .import("shinqlx")
            .and_then(|shinqlx| shinqlx.getattr("frame_tasks"))
            .expect("no frame tasks");
        let queue = scheduler.getattr("queue").expect("no queue");
        assert_eq!(queue.len().expect("no length"), before + 1);
        let task = queue.get_item(before).expect("could not get the new task");
        let arguments = task.getattr("argument").expect("no arguments");
        let kwargs = task.getattr("kwargs").expect("no keyword arguments");
        task.getattr("action")
            .and_then(|action| {
                action.call(
                    arguments.cast::<PyTuple>().expect("not a tuple"),
                    kwargs.cast::<PyDict>().ok(),
                )
            })
            .expect("could not run the scheduled task");
        scheduler
            .call_method1("cancel", (task,))
            .expect("could not cancel the task");
    }

    fn scheduled_tasks(py: Python<'_>) -> usize {
        py.import("shinqlx")
            .and_then(|shinqlx| shinqlx.getattr("frame_tasks"))
            .and_then(|scheduler| scheduler.getattr("queue"))
            .and_then(|queue| queue.len())
            .expect("could not count the tasks")
    }

    #[rstest]
    #[case(15, 0, 15, Some((None, 1.0)))]
    #[case(0, 0, 15, None)]
    #[case(14, 1, 15, Some((None, 14.0 / 15.0)))]
    #[case(3, 1, 15, Some((None, 0.75)))]
    #[case(10, 5, 15, Some((Some(LeaveAction::Warn), 10.0 / 15.0)))]
    #[case(9, 6, 15, Some((Some(LeaveAction::Ban), 0.6)))]
    #[case(3, 6, 15, Some((Some(LeaveAction::Warn), 1.0 / 3.0)))]
    #[case(0, 9, 15, Some((Some(LeaveAction::Warn), 0.0)))]
    fn leaving_leads_to_warnings_and_bans(
        #[case] completed: i64,
        #[case] left: i64,
        #[case] minimum: i64,
        #[case] expected: Option<(Option<LeaveAction>, f64)>,
    ) {
        assert_eq!(leave_action(completed, left, minimum, 0.78, 0.63), expected);
    }

    #[rstest]
    #[case("", "You are banned until soon.")]
    #[case("rude", "You are banned until soon: rude")]
    fn the_notice_names_the_expiry_and_the_reason(#[case] reason: &str, #[case] expected: &str) {
        let banned = Entry {
            expires: "soon".to_owned(),
            score: 1.0,
            reason: reason.to_owned(),
        };

        assert_eq!(ban_notice(&banned), expected);
    }

    #[rstest]
    #[case(&["ban"])]
    #[case(&["ban", "3"])]
    #[case(&["ban", "3", "1"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_without_enough_arguments_asks_for_usage(_pyshinqlx_setup: (), #[case] words: &[&str]) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::cmd_ban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(USAGE)));
            assert!(fixtures.zsets().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_of_an_unknown_client_id_replies(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::cmd_ban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["ban", "9", "1", "day"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                ["Invalid client ID. Use either a client ID or a SteamID64."]
            );
        });
    }

    #[rstest]
    #[case(&["ban", "nobody", "1", "day"])]
    #[case(&["unban", "nobody"])]
    #[case(&["checkban", "nobody"])]
    #[case(&["forgive", "nobody"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn commands_stop_all_processing_for_a_name_that_matches_nobody(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
    ) {
        with_ban(REDIS_5, &[], |fixtures| {
            let command = match words[0] {
                "ban" => Ban::cmd_ban,
                "unban" => Ban::cmd_unban,
                "checkban" => Ban::cmd_checkban,
                _ => Ban::cmd_forgive,
            };

            let result = command(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(
                result.ok(),
                Some(Some(PythonReturnCodes::RET_STOP_ALL as i32))
            );
            assert!(fixtures.zsets().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_does_not_touch_players_with_permission_level_five(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            fixtures.grant(TARGET_ID, 5);

            let result = Ban::cmd_ban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["ban", "3", "1", "day"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                ["^6Eve^7 has permission level 5 and cannot be banned."]
            );
            assert!(fixtures.zsets().is_empty());
        });
    }

    #[rstest]
    #[case(&["ban", "3", "0", "days"])]
    #[case(&["ban", "3", "x", "days"])]
    #[case(&["ban", "3", "1", "parsec"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_with_an_invalid_length_asks_for_usage(_pyshinqlx_setup: (), #[case] words: &[&str]) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::cmd_ban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(USAGE)));
            assert!(fixtures.zsets().is_empty());
        });
    }

    #[rstest]
    #[case(REDIS_5, true)]
    #[case(REDIS_2, true)]
    #[case(REDIS_5, false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_stores_the_ban_and_kicks_the_player(
        _pyshinqlx_setup: (),
        #[case] redis: &'static CStr,
        #[case] hset_supports_mapping: bool,
    ) {
        with_ban(redis, &[], |fixtures| {
            fixtures
                .database
                .setattr("hset_supports_mapping", hset_supports_mapping)
                .expect("could not configure the database");
            let before = now();

            let result = Ban::cmd_ban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["ban", "3", "2", "days", "being", "rude"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            let key = bans_key(TARGET_ID);
            let score = fixtures.zsets()[&key]["0"];
            assert!(score >= before + 172_800.0 && score <= now() + 172_800.0);
            let details = &fixtures.hashes()[&format!("{key}:0")];
            assert_eq!(details["reason"], "being rude");
            assert_eq!(details["issued_by"], CALLER_ID.to_string());
            assert_eq!(
                fixtures.commands.dropped(),
                [format!(
                    "has been banned until ^6{}^7: being rude",
                    details["expires"]
                )]
            );
            assert!(fixtures.replies().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn ban_of_a_player_that_cannot_be_kicked_only_stores_the_ban(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::cmd_ban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["ban", "76561198000000099", "1", "day"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert!(
                fixtures
                    .zsets()
                    .contains_key(&bans_key(76_561_198_000_000_099))
            );
            let replies = fixtures.replies();
            assert_eq!(replies.len(), 1);
            assert!(
                replies[0].starts_with("^676561198000000099 ^7has been banned. Ban expires on ^6")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_and_checkban_and_forgive_without_a_target_ask_for_usage(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            let words = msg(&["unban"]);

            for result in [
                Ban::cmd_unban(
                    &fixtures.plugin,
                    &fixtures.caller,
                    words.clone(),
                    &fixtures.channel,
                ),
                Ban::cmd_checkban(
                    &fixtures.plugin,
                    &fixtures.caller,
                    words.clone(),
                    &fixtures.channel,
                ),
                Ban::cmd_forgive(&fixtures.plugin, &fixtures.caller, words, &fixtures.channel),
            ] {
                assert_eq!(result.ok(), Some(Some(USAGE)));
            }
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_without_an_active_ban_says_so(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::cmd_unban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["unban", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), ["No active bans on ^6Eve^7 found."]);
        });
    }

    #[rstest]
    #[case(REDIS_5)]
    #[case(REDIS_2)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn unban_lifts_every_active_ban(_pyshinqlx_setup: (), #[case] redis: &'static CStr) {
        with_ban(redis, &[], |fixtures| {
            fixtures.ban(TARGET_ID, 0, FAR_FUTURE, "2096-10-02 12:00:00", "");
            fixtures.ban(TARGET_ID, 1, FAR_FUTURE + 10.0, "2096-10-02 12:00:10", "");
            fixtures.ban(TARGET_ID, 2, 1.0, "1970-01-01 00:00:01", "");

            let result = Ban::cmd_unban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["unban", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            let scores = &fixtures.zsets()[&bans_key(TARGET_ID)];
            assert_eq!(scores["0"], 0.0);
            assert_eq!(scores["1"], 0.0);
            assert_eq!(scores["2"], 1.0);
            assert_eq!(fixtures.replies(), ["^6Eve^7 has been unbanned."]);
        });
    }

    #[rstest]
    #[case("", "^6Eve^7 is banned until ^62096-10-02 12:00:00^7.")]
    #[case(
        "rude",
        "^6Eve^7 is banned until ^62096-10-02 12:00:00^7 for the following reason:^6 rude"
    )]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn checkban_reports_a_manual_ban(
        _pyshinqlx_setup: (),
        #[case] reason: &str,
        #[case] expected: &str,
    ) {
        with_ban(REDIS_5, &[], |fixtures| {
            fixtures.ban(TARGET_ID, 0, FAR_FUTURE, "2096-10-02 12:00:00", reason);

            let result = Ban::cmd_checkban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["checkban", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), [expected]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn checkban_of_a_player_without_a_ban(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::cmd_checkban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["checkban", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), ["^6Eve ^7is not banned."]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn checkban_reports_a_ban_for_leaving(_pyshinqlx_setup: ()) {
        with_leaver_ban(None, |fixtures| {
            fixtures.store(&counter_key(TARGET_ID, "games_completed"), "9");
            fixtures.store(&counter_key(TARGET_ID, "games_left"), "6");

            let result = Ban::cmd_checkban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["checkban", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                ["^6Eve ^7is banned for having left too many games."]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn checkban_finds_a_player_by_name(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[(2, "Bob")], |fixtures| {
            let result = Ban::cmd_checkban(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["checkban", "bob"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies().len(), 1);
            assert!(fixtures.replies()[0].ends_with("is not banned."));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn forgive_of_an_unknown_player_says_so(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::cmd_forgive(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["forgive", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), ["I do not know ^6Eve^7."]);
        });
    }

    #[rstest]
    #[case(None, "^6Eve^7's leaves are already at ^60^7.")]
    #[case(Some("0"), "^6Eve^7's leaves are already at ^60^7.")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn forgive_without_leaves_says_so(
        _pyshinqlx_setup: (),
        #[case] leaves: Option<&str>,
        #[case] expected: &str,
    ) {
        with_ban(REDIS_5, &[], |fixtures| {
            fixtures.store(&player_key(TARGET_ID), "known");
            if let Some(leaves) = leaves {
                fixtures.store(&counter_key(TARGET_ID, "games_left"), leaves);
            }

            let result = Ban::cmd_forgive(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["forgive", "3"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), [expected]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn forgive_with_an_unintelligible_number_replies(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            fixtures.store(&player_key(TARGET_ID), "known");
            fixtures.store(&counter_key(TARGET_ID, "games_left"), "4");

            let result = Ban::cmd_forgive(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["forgive", "3", "many"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                ["Unintelligible number of leaves to forgive. Please use numbers."]
            );
            assert_eq!(
                fixtures.strings_in_database()[&counter_key(TARGET_ID, "games_left")],
                "4"
            );
        });
    }

    #[rstest]
    #[case(&["forgive", "3"], "3", "^61^7 games have been forgiven, putting ^6Eve^7 at ^63^7 leaves.")]
    #[case(&["forgive", "3", "3"], "1", "^63^7 games have been forgiven, putting ^6Eve^7 at ^61^7 leaves.")]
    #[case(&["forgive", "3", "4"], "0", "^6Eve^7's leaves have been reduced to ^60^7.")]
    #[case(&["forgive", "3", "9"], "0", "^6Eve^7's leaves have been reduced to ^60^7.")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn forgive_reduces_the_leaves(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] remaining: &str,
        #[case] expected: &str,
    ) {
        with_ban(REDIS_5, &[], |fixtures| {
            fixtures.store(&player_key(TARGET_ID), "known");
            fixtures.store(&counter_key(TARGET_ID, "games_left"), "4");

            let result = Ban::cmd_forgive(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), [expected]);
            assert_eq!(
                fixtures.strings_in_database()[&counter_key(TARGET_ID, "games_left")],
                remaining
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connecting_players_without_a_ban_are_let_in(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::handle_player_connect(&fixtures.plugin, &fixtures.target);

            assert_eq!(result.ok(), Some(None));
        });
    }

    #[rstest]
    #[case("", "You are banned until 2096-10-02 12:00:00.")]
    #[case("rude", "You are banned until 2096-10-02 12:00:00: rude")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn banned_players_cannot_connect(
        _pyshinqlx_setup: (),
        #[case] reason: &str,
        #[case] expected: &str,
    ) {
        with_ban(REDIS_5, &[], |fixtures| {
            fixtures.ban(TARGET_ID, 0, FAR_FUTURE, "2096-10-02 12:00:00", reason);

            let result = Ban::handle_player_connect(&fixtures.plugin, &fixtures.target);

            assert_eq!(result.ok(), Some(Some(expected.to_owned())));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn players_that_left_too_many_games_cannot_connect(_pyshinqlx_setup: ()) {
        with_leaver_ban(None, |fixtures| {
            fixtures.store(&counter_key(TARGET_ID, "games_completed"), "9");
            fixtures.store(&counter_key(TARGET_ID, "games_left"), "6");

            let result = Ban::handle_player_connect(&fixtures.plugin, &fixtures.target);

            assert_eq!(
                result.ok(),
                Some(Some(
                    "You have been banned from this server for leaving too many games.".to_owned()
                ))
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn players_that_are_close_to_a_leaver_ban_are_warned_once_they_are_loaded(
        _pyshinqlx_setup: (),
    ) {
        with_leaver_ban(None, |fixtures| {
            fixtures.store(&counter_key(TARGET_ID, "games_completed"), "12");
            fixtures.store(&counter_key(TARGET_ID, "games_left"), "4");

            let connected = Ban::handle_player_connect(&fixtures.plugin, &fixtures.target);
            let delivered = Ban::deliver_warning(&fixtures.plugin, &fixtures.target);
            let delivered_again = Ban::deliver_warning(&fixtures.plugin, &fixtures.target);

            assert_eq!(connected.ok(), Some(None));
            assert!(delivered.is_ok() && delivered_again.is_ok());
            assert_eq!(
                fixtures.printed(),
                [
                    "^7You have only completed ^675.0^7 percent of your games.",
                    "^7If you keep leaving you ^6will^7 be banned."
                ]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn players_without_a_pending_warning_are_not_warned(_pyshinqlx_setup: ()) {
        with_leaver_ban(None, |fixtures| {
            let result = Ban::deliver_warning(&fixtures.plugin, &fixtures.target);

            assert!(result.is_ok());
            assert!(fixtures.printed().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn players_that_are_gone_by_the_time_of_the_warning_are_not_warned(_pyshinqlx_setup: ()) {
        with_leaver_ban(None, |fixtures| {
            fixtures
                .plugin
                .get()
                .pending_warnings
                .lock()
                .insert(TARGET_ID, 0.7);
            let gone = real_player_with_id(fixtures.plugin.py(), 3, TARGET_ID + 1000, "Eve^7", "");
            fixtures
                .plugin
                .get()
                .pending_warnings
                .lock()
                .insert(TARGET_ID + 1000, 0.7);

            let result = Ban::deliver_warning(&fixtures.plugin, &gone);

            assert!(result.is_ok());
            assert!(fixtures.printed().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_loaded_player_gets_the_warning_four_seconds_later(
        #[from(crate::ffi::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
        with_leaver_ban(None, |fixtures| {
            let py = fixtures.plugin.py();
            fixtures
                .plugin
                .get()
                .pending_warnings
                .lock()
                .insert(TARGET_ID, 0.7);
            let before = scheduled_tasks(py);

            let result = Ban::handle_player_loaded(&fixtures.plugin, &fixtures.target);

            assert!(result.is_ok());
            assert!(fixtures.printed().is_empty());
            run_scheduled_task(py, before);
            assert_eq!(fixtures.printed().len(), 2);
        });
    }

    #[rstest]
    #[case(false, 0)]
    #[case(true, 1)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_countdown_announces_that_leavers_are_tracked(
        _pyshinqlx_setup: (),
        #[case] tracking: bool,
        #[case] announcements: usize,
    ) {
        let cvars: &[(&str, &CStr)] = if tracking {
            &LEAVER_CVARS
        } else {
            &[("qlx_leaverBan", c"0")]
        };
        with_ban_in(
            engine_with_cvars(cvars),
            REDIS_5,
            &[],
            SPECTATING,
            |fixtures| {
                let result = Ban::handle_game_countdown(&fixtures.plugin);

                assert!(result.is_ok());
                assert_eq!(fixtures.printed().len(), announcements);
            },
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_players_at_the_start_of_the_game_are_remembered_a_second_later(
        #[from(crate::ffi::python::pyshinqlx_setup_fixture::empty_frame_tasks)] _empty_frame_tasks: (),
    ) {
        let seating = [team_t::TEAM_RED, team_t::TEAM_BLUE, team_t::TEAM_SPECTATOR];
        with_seated_leaver_ban(None, seating, |fixtures| {
            let py = fixtures.plugin.py();
            let before = scheduled_tasks(py);

            let result = Ban::handle_game_start(&fixtures.plugin, py.None().bind(py));

            assert!(result.is_ok());
            assert!(fixtures.start_ids().is_empty());
            run_scheduled_task(py, before);
            assert_eq!(fixtures.start_ids(), [CALLER_ID, TARGET_ID]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn an_aborted_game_forgets_the_players(_pyshinqlx_setup: ()) {
        with_leaver_ban(None, |fixtures| {
            fixtures.start_with(&[&fixtures.caller]);
            let data = PyDict::new(fixtures.plugin.py());
            data.set_item("ABORTED", true)
                .expect("could not set ABORTED");

            let result = Ban::handle_game_end(&fixtures.plugin, &data);

            assert!(result.is_ok());
            assert!(fixtures.start_ids().is_empty());
            assert!(fixtures.strings_in_database().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_finished_game_counts_completed_games_and_announces_the_leavers(_pyshinqlx_setup: ()) {
        let seating = [team_t::TEAM_RED, team_t::TEAM_SPECTATOR, team_t::TEAM_BLUE];
        with_seated_leaver_ban(None, seating, |fixtures| {
            fixtures.start_with(&[&fixtures.caller, &fixtures.target, &fixtures.other]);
            let data = PyDict::new(fixtures.plugin.py());
            data.set_item("ABORTED", false)
                .expect("could not set ABORTED");

            let result = Ban::handle_game_end(&fixtures.plugin, &data);

            assert!(result.is_ok());
            let stored = fixtures.strings_in_database();
            assert_eq!(stored[&counter_key(CALLER_ID, "games_completed")], "1");
            assert_eq!(stored[&counter_key(OTHER_ID, "games_completed")], "1");
            assert_eq!(stored[&counter_key(TARGET_ID, "games_left")], "1");
            assert_eq!(stored.len(), 3);
            assert_eq!(fixtures.printed(), ["^7Leavers: ^6Eve"]);
            assert!(fixtures.start_ids().is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_finished_game_without_leavers_keeps_the_players(_pyshinqlx_setup: ()) {
        let seating = [
            team_t::TEAM_RED,
            team_t::TEAM_SPECTATOR,
            team_t::TEAM_SPECTATOR,
        ];
        with_seated_leaver_ban(None, seating, |fixtures| {
            fixtures.start_with(&[&fixtures.caller]);
            let data = PyDict::new(fixtures.plugin.py());

            let result = Ban::handle_game_end(&fixtures.plugin, &data);

            assert!(result.is_ok());
            assert!(fixtures.printed().is_empty());
            assert_eq!(fixtures.start_ids(), [CALLER_ID]);
        });
    }

    #[rstest]
    #[case(1, false)]
    #[case(2, true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn disconnecting_with_uneven_teams_is_no_leave(
        _pyshinqlx_setup: (),
        #[case] players_seated: usize,
        #[case] still_counted: bool,
    ) {
        let seating = [
            team_t::TEAM_RED,
            team_t::TEAM_SPECTATOR,
            if players_seated == 2 {
                team_t::TEAM_RED
            } else {
                team_t::TEAM_SPECTATOR
            },
        ];
        with_seated_leaver_ban(None, seating, |fixtures| {
            let py = fixtures.plugin.py();
            fixtures.start_with(&[&fixtures.target]);

            let result = Ban::handle_player_disconnect(
                fixtures.plugin.get(),
                &fixtures.target,
                py.None().bind(py),
            );

            assert!(result.is_ok());
            assert_eq!(fixtures.start_ids().is_empty(), !still_counted);
        });
    }

    #[rstest]
    #[case("red", "spectator", 2, false)]
    #[case("red", "spectator", 1, true)]
    #[case("red", "blue", 2, true)]
    #[case("free", "spectator", 2, true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn spectating_with_even_teams_is_no_leave(
        _pyshinqlx_setup: (),
        #[case] old_team: &str,
        #[case] new_team: &str,
        #[case] players_seated: usize,
        #[case] still_counted: bool,
    ) {
        let seating = [
            team_t::TEAM_RED,
            team_t::TEAM_SPECTATOR,
            if players_seated == 2 {
                team_t::TEAM_RED
            } else {
                team_t::TEAM_SPECTATOR
            },
        ];
        with_seated_leaver_ban(None, seating, |fixtures| {
            fixtures.start_with(&[&fixtures.target]);

            let result =
                Ban::handle_team_switch(&fixtures.plugin, &fixtures.target, old_team, new_team);

            assert!(result.is_ok());
            assert_eq!(fixtures.start_ids().is_empty(), !still_counted);
        });
    }

    #[rstest]
    #[case("IN_PROGRESS", "spectator", "red", true)]
    #[case("IN_PROGRESS", "spectator", "blue", true)]
    #[case("IN_PROGRESS", "spectator", "free", false)]
    #[case("IN_PROGRESS", "red", "blue", false)]
    #[case("PRE_GAME", "spectator", "red", false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn joining_a_running_game_makes_a_player_count(
        _pyshinqlx_setup: (),
        #[case] state: &'static str,
        #[case] old_team: &str,
        #[case] new_team: &str,
        #[case] counted: bool,
    ) {
        with_leaver_ban(Some(state), |fixtures| {
            let result =
                Ban::handle_team_switch(&fixtures.plugin, &fixtures.target, old_team, new_team);

            assert!(result.is_ok());
            assert_eq!(
                fixtures.start_ids(),
                if counted { vec![TARGET_ID] } else { vec![] }
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn joining_twice_counts_a_player_once(_pyshinqlx_setup: ()) {
        with_leaver_ban(Some("IN_PROGRESS"), |fixtures| {
            fixtures.start_with(&[&fixtures.target]);

            let result =
                Ban::handle_team_switch(&fixtures.plugin, &fixtures.target, "spectator", "red");

            assert!(result.is_ok());
            assert_eq!(fixtures.start_ids(), [TARGET_ID]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_banned_gives_other_plugins_the_expiry_and_the_reason(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            fixtures.ban(TARGET_ID, 0, FAR_FUTURE, "2096-10-02 12:00:00", "rude");
            let py = fixtures.plugin.py();

            let result = Ban::is_banned(&fixtures.plugin, TARGET_ID);

            let (expires, reason) = result
                .expect("is_banned failed")
                .expect("the player should be banned");
            assert_eq!(reason, "rude");
            assert_eq!(
                expires
                    .call_method1("strftime", ("%Y-%m-%d %H:%M:%S",))
                    .and_then(|expires| expires.extract::<String>())
                    .expect("could not format the expiry"),
                "2096-10-02 12:00:00"
            );
            assert!(
                expires
                    .is_instance(
                        &py.import("datetime")
                            .and_then(|datetime| datetime.getattr("datetime"))
                            .expect("no datetime")
                    )
                    .expect("could not check the type")
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn is_banned_is_none_without_an_active_ban(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::is_banned(&fixtures.plugin, TARGET_ID);

            assert!(result.is_ok_and(|banned| banned.is_none()));
        });
    }

    #[rstest]
    #[case("9", "6", Some(("ban", 0.6)))]
    #[case("10", "5", Some(("warn", 10.0 / 15.0)))]
    #[case("15", "0", None)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn leave_status_gives_other_plugins_the_action_and_the_ratio(
        _pyshinqlx_setup: (),
        #[case] completed: &str,
        #[case] left: &str,
        #[case] expected: Option<(&str, f64)>,
    ) {
        with_leaver_ban(None, |fixtures| {
            fixtures.store(&counter_key(TARGET_ID, "games_completed"), completed);
            fixtures.store(&counter_key(TARGET_ID, "games_left"), left);

            let result = Ban::leave_status(&fixtures.plugin, TARGET_ID);

            let status = result.expect("leave_status failed");
            match expected {
                Some((action, ratio)) => {
                    assert_eq!(status, Some((Some(action), ratio)));
                }
                None => {
                    let (action, ratio) = status.expect("there should be a status");
                    assert_eq!(action, None);
                    assert_eq!(ratio, 1.0);
                }
            }
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn leave_status_is_none_without_games_or_the_leaver_ban(_pyshinqlx_setup: ()) {
        with_leaver_ban(None, |fixtures| {
            let result = Ban::leave_status(&fixtures.plugin, TARGET_ID);

            assert!(result.is_ok_and(|status| status.is_none()));
        });
        with_ban(REDIS_5, &[], |fixtures| {
            fixtures.store(&counter_key(TARGET_ID, "games_completed"), "1");
            fixtures.store(&counter_key(TARGET_ID, "games_left"), "9");

            let result = Ban::leave_status(&fixtures.plugin, TARGET_ID);

            assert!(result.is_ok_and(|status| status.is_none()));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn warn_player_tells_the_share_of_completed_games(_pyshinqlx_setup: ()) {
        with_ban(REDIS_5, &[], |fixtures| {
            let result = Ban::warn_player(&fixtures.plugin, &fixtures.target, 2.0 / 3.0);

            assert!(result.is_ok());
            assert_eq!(
                fixtures.printed(),
                [
                    "^7You have only completed ^666.7^7 percent of your games.",
                    "^7If you keep leaving you ^6will^7 be banned."
                ]
            );
        });
    }
}

#[cfg(test)]
mod ban_creation_tests {
    use pyo3::{intern, prelude::*};
    use rstest::rstest;

    use super::create;
    use crate::{
        ffi::python::{
            COMMANDS, EVENT_DISPATCHERS, Plugin,
            commands::CommandInvoker,
            events::{
                EventDispatcherManager, EventDispatcherManagerMethods, GameCountdownDispatcher,
                GameEndDispatcher, GameStartDispatcher, PlayerConnectDispatcher,
                PlayerDisconnectDispatcher, PlayerLoadedDispatcher, TeamSwitchDispatcher,
            },
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{RestoredAttribute, engine_with_cvars},
        },
        prelude::*,
    };

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn creating_the_plugin_registers_the_hooks_the_commands_and_the_cvars(_pyshinqlx_setup: ()) {
        let mut engine = engine_with_cvars(&[
            ("zmq_stats_enable", c"1"),
            ("qlx_leaverBan", c"0"),
            ("qlx_leaverBanThreshold", c"0.63"),
            ("qlx_leaverBanWarnThreshold", c"0.78"),
            ("qlx_leaverBanMinimumGames", c"15"),
        ]);
        engine.run(|| {
            Python::attach(|py| {
                let module = PyModule::from_code(
                    py,
                    c"
class Database:
    def __init__(self, plugin):
        pass
",
                    c"",
                    c"",
                )
                .expect("could not create the fixtures");
                let plugin_class = py.get_type::<Plugin>();
                let _restore_database =
                    RestoredAttribute::capture(plugin_class.as_any(), "database");
                plugin_class
                    .setattr("database", module.getattr("Database").expect("no Database"))
                    .expect("could not set the database");
                let dispatchers = Bound::new(py, EventDispatcherManager::default())
                    .expect("this should not happen");
                for dispatcher in [
                    py.get_type::<PlayerConnectDispatcher>(),
                    py.get_type::<PlayerLoadedDispatcher>(),
                    py.get_type::<PlayerDisconnectDispatcher>(),
                    py.get_type::<GameCountdownDispatcher>(),
                    py.get_type::<GameStartDispatcher>(),
                    py.get_type::<GameEndDispatcher>(),
                    py.get_type::<TeamSwitchDispatcher>(),
                ] {
                    dispatchers
                        .add_dispatcher(&dispatcher)
                        .expect("could not add the dispatcher");
                }
                EVENT_DISPATCHERS.store(Some(dispatchers.unbind().into()));
                COMMANDS.store(Some(
                    Py::new(py, CommandInvoker::py_new())
                        .expect("could not create the command invoker")
                        .into(),
                ));

                let plugin = create(py).expect("could not create the plugin");

                let count = |attribute: &str| {
                    plugin
                        .getattr(attribute)
                        .and_then(|items| items.len())
                        .expect("could not count")
                };
                assert_eq!(count("hooks"), 7);
                assert_eq!(count("commands"), 4);
                assert!(plugin.hasattr(intern!(py, "cmd_ban")).unwrap_or(false));
            });
        });
    }
}
