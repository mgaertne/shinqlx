use std::{
    collections::VecDeque,
    sync::atomic::{AtomicI64, Ordering},
};

use parking_lot::Mutex;
use pyo3::{
    PyTraverseError,
    gc::PyVisit,
    intern,
    prelude::*,
    types::{IntoPyDict, PyFloat, PyInt},
};

use super::{
    cvar_flag, register_client_command, register_command, register_hook,
    target::{find_player, reply, stop_all, usage},
    tell,
    timed_entries::{local_time, now},
};
use crate::ffi::python::{
    Plugin, commands::CommandPriorities, delay, get_cvar, plugin::PluginMethods,
};

mod bookkeeping;
mod lookup;
mod mappool;
mod sounds;

use self::{
    bookkeeping::update_player,
    lookup::{Lookup, act_on_player, lookup_player},
    mappool::{Mappool, load_mappool, mappool_error, mappool_listing},
    sounds::{SOUNDS_FLAG, sounds_enabled, tell_sounds_disabled},
};

const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const TIME_FORMAT: &str = "%H:%M:%S";
const RECENT_COMMANDS: usize = 11;
const RECENT_DISCONNECTS: usize = 10;
const VOTE_RESOLUTION_DELAY: f64 = 29.0;

/// Everyday commands for the admins of a server, and the bookkeeping of players, ips and votes.
#[pyclass(name = "essentials", module = "shinqlx", extends = Plugin, frozen)]
pub(super) struct Essentials {
    vote_count: AtomicI64,
    last_vote: AtomicI64,
    recent_commands: Mutex<VecDeque<(Py<PyAny>, String)>>,
    recent_disconnects: Mutex<VecDeque<(Py<PyAny>, f64)>>,
    mappool: Option<Mappool>,
}

fn plugin_class(py: Python<'_>) -> Bound<'_, pyo3::types::PyType> {
    py.get_type::<Plugin>()
}

fn cvar_int(py: Python<'_>, name: &str) -> PyResult<i64> {
    plugin_class(py)
        .call_method1(intern!(py, "get_cvar"), (name, py.get_type::<PyInt>()))?
        .extract()
}

fn cvar_float(py: Python<'_>, name: &str) -> PyResult<f64> {
    plugin_class(py)
        .call_method1(intern!(py, "get_cvar"), (name, py.get_type::<PyFloat>()))?
        .extract()
}

fn steam_id_of(player: &Bound<'_, PyAny>) -> PyResult<i64> {
    player.getattr(intern!(player.py(), "steam_id"))?.extract()
}

fn player_key(steam_id: i64) -> String {
    format!("minqlx:players:{steam_id}")
}

fn command_prefix() -> PyResult<String> {
    Ok(get_cvar("qlx_commandPrefix")?.unwrap_or_default())
}

fn players_in_teams(py: Python<'_>, teams: &[&str]) -> PyResult<usize> {
    let all_teams = plugin_class(py).call_method0(intern!(py, "teams"))?;
    teams.iter().try_fold(0, |count, team| {
        Ok(count + all_teams.get_item(*team)?.len()?)
    })
}

fn game_state(plugin: &Bound<'_, Essentials>) -> PyResult<Option<String>> {
    let py = plugin.py();
    let game = plugin.getattr(intern!(py, "game"))?;
    if game.is_none() {
        return Ok(None);
    }
    game.getattr(intern!(py, "state"))?.extract().map(Some)
}

impl Essentials {
    fn play_for_everybody(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: &[String],
        play: &str,
    ) -> PyResult<Option<i32>> {
        let Some(path) = msg.get(1) else {
            return usage();
        };
        let database = slf.as_super().get_db()?;
        if !sounds_enabled(&database, player)? {
            tell_sounds_disabled(player)?;
            return stop_all();
        }

        let class = plugin_class(slf.py());
        if !class.call_method1(play, (path, player))?.is_truthy()? {
            tell(player, "Invalid sound.")?;
            return stop_all();
        }

        let everybody = class
            .call_method0(intern!(slf.py(), "players"))?
            .extract::<Vec<Bound<'_, PyAny>>>()?;
        for other in everybody {
            if !other.eq(player)? && sounds_enabled(&database, &other)? {
                class.call_method1(play, (path, &other))?;
            }
        }
        stop_all()
    }

    fn stop_playing(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        stop: &str,
    ) -> PyResult<Option<i32>> {
        if !sounds_enabled(&slf.as_super().get_db()?, player)? {
            tell_sounds_disabled(player)?;
            return stop_all();
        }
        plugin_class(slf.py()).call_method0(stop).map(|_| None)
    }

    fn decide_vote(
        py: Python<'_>,
        channel: &Bound<'_, PyAny>,
        pass_it: bool,
    ) -> PyResult<Option<i32>> {
        let class = plugin_class(py);
        if !class
            .call_method0(intern!(py, "is_vote_active"))?
            .is_truthy()?
        {
            return reply(channel, "There is no active vote!");
        }
        class
            .call_method1(intern!(py, "force_vote"), (pass_it,))
            .map(|_| None)
    }

    fn change_lock(
        py: Python<'_>,
        player: &Bound<'_, PyAny>,
        msg: &[String],
        method: &str,
    ) -> PyResult<Option<i32>> {
        let team = match msg.get(1) {
            None => None,
            Some(team) => match team
                .chars()
                .next()
                .map(|initial| initial.to_ascii_lowercase())
            {
                Some('s') => Some("spectator"),
                Some('r') => Some("red"),
                Some('b') => Some("blue"),
                _ => {
                    tell(player, "Invalid team.")?;
                    return stop_all();
                }
            },
        };
        plugin_class(py).call_method1(method, (team,)).map(|_| None)
    }
}

#[pymethods]
impl Essentials {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        self.recent_commands
            .lock()
            .iter()
            .try_for_each(|(player, _)| visit.call(player))?;
        self.recent_disconnects
            .lock()
            .iter()
            .try_for_each(|(player, _)| visit.call(player))
    }

    fn handle_player_connect(slf: &Bound<'_, Self>, player: &Bound<'_, PyAny>) -> PyResult<()> {
        update_player(&slf.as_super().get_db()?, player)
    }

    fn handle_player_disconnect(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _reason: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = slf.py();
        {
            let mut recent = slf.get().recent_disconnects.lock();
            recent.push_front((player.clone().unbind(), now()));
            recent.truncate(RECENT_DISCONNECTS);
        }

        let key = format!("{}:last_seen", player_key(steam_id_of(player)?));
        slf.as_super()
            .get_db()?
            .set_item(key, local_time(py, now())?)
    }

    fn handle_vote_called(
        slf: &Bound<'_, Self>,
        caller: &Bound<'_, PyAny>,
        vote: &str,
        args: &str,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let vote = vote.to_lowercase();

        if vote == "teamsize" {
            let Ok(size) = args.trim().parse::<i64>() else {
                return Ok(None);
            };
            if size > cvar_int(py, "qlx_teamsizeMaximum")? {
                tell(
                    caller,
                    "The team size is larger than what the server allows.",
                )?;
                return stop_all();
            }
            if size < cvar_int(py, "qlx_teamsizeMinimum")? {
                tell(
                    caller,
                    "The team size is smaller than what the server allows.",
                )?;
                return stop_all();
            }
        }

        if let Some(mappool) = &slf.get().mappool
            && vote == "map"
            && cvar_flag(py, "qlx_enforceMappool")?
        {
            let mut split_args = args.split_whitespace();
            let Some(map_name) = split_args.next() else {
                tell(caller, "Available maps and factories:")?;
                tell(caller, &mappool_listing(mappool, 2))?;
                return stop_all();
            };

            let Some(factories) = mappool.get(&map_name.to_lowercase()) else {
                return mappool_error(caller, "This map is not allowed.");
            };
            let factory = match split_args.next() {
                Some(factory) => Some(factory.to_owned()),
                None => {
                    let game = slf.getattr(intern!(py, "game"))?;
                    if game.is_none() {
                        None
                    } else {
                        game.getattr(intern!(py, "factory"))?.extract()?
                    }
                }
            };
            if factory.is_some_and(|factory| !factory.is_empty() && !factories.contains(&factory)) {
                return mappool_error(caller, "This factory is not allowed on that map.");
            }
        }

        if cvar_flag(py, "qlx_votepass")? {
            let vote_id = slf.get().vote_count.fetch_add(1, Ordering::SeqCst);
            slf.get().last_vote.store(vote_id, Ordering::SeqCst);
            let threshold = cvar_float(py, "qlx_votepassThreshold")?;
            delay(py, VOTE_RESOLUTION_DELAY)?
                .call1((slf.getattr(intern!(py, "force"))?,))?
                .call1((threshold, vote_id))?;
        }
        Ok(None)
    }

    fn force(slf: &Bound<'_, Self>, require: f64, vote_id: i64) -> PyResult<()> {
        let py = slf.py();
        if slf.get().last_vote.load(Ordering::SeqCst) != vote_id {
            return Ok(());
        }

        let class = plugin_class(py);
        if !class
            .call_method0(intern!(py, "is_vote_active"))?
            .is_truthy()?
        {
            return Ok(());
        }
        let votes = class.call_method0(intern!(py, "current_vote_count"))?;
        if votes.is_none() {
            return Ok(());
        }
        let (yes_votes, no_votes) = votes.extract::<(i64, i64)>()?;
        if yes_votes <= no_votes {
            return Ok(());
        }

        if require != 0.0 {
            let players = players_in_teams(py, &["red", "blue", "free"])?;
            if players == 0 || ((yes_votes + no_votes) as f64 / players as f64) < require {
                return Ok(());
            }
        }
        class
            .call_method1(intern!(py, "force_vote"), (true,))
            .map(|_| ())
    }

    fn handle_command(
        slf: &Bound<'_, Self>,
        caller: &Bound<'_, PyAny>,
        _command: &Bound<'_, PyAny>,
        args: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let mut recent = slf.get().recent_commands.lock();
        recent.push_front((caller.clone().unbind(), args.str()?.extract()?));
        recent.truncate(RECENT_COMMANDS);
        Ok(())
    }

    fn cmd_id(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let class = plugin_class(py);
        let connected = class
            .call_method0(intern!(py, "players"))?
            .extract::<Vec<Bound<'_, PyAny>>>()?;

        let list_alternatives = |players: &[Bound<'_, PyAny>]| -> PyResult<()> {
            let lines = players
                .iter()
                .map(|candidate| {
                    Ok(format!(
                        "  {}^6:^7 {}",
                        candidate.getattr(intern!(py, "id"))?,
                        candidate.getattr(intern!(py, "name"))?
                    ))
                })
                .collect::<PyResult<Vec<_>>>()?;
            tell(player, &lines.join("\n"))
        };

        if connected.is_empty() {
            tell(player, "There are no players connected at the moment.")?;
        } else if msg.len() == 1 {
            tell(player, "All connected players:")?;
            list_alternatives(&connected)?;
        } else {
            let mut found: Vec<Bound<'_, PyAny>> = vec![];
            for name in &msg[1..] {
                for candidate in class
                    .call_method1(intern!(py, "find_player"), (name,))?
                    .extract::<Vec<Bound<'_, PyAny>>>()?
                {
                    let steam_id = steam_id_of(&candidate)?;
                    if !found
                        .iter()
                        .any(|known| steam_id_of(known).is_ok_and(|id| id == steam_id))
                    {
                        found.push(candidate);
                    }
                }
            }

            if found.is_empty() {
                tell(player, "Sorry, but no players matched your tokens.")?;
            } else {
                tell(
                    player,
                    &format!("A total of ^6{}^7 players matched:", found.len()),
                )?;
                list_alternatives(&found)?;
            }
        }
        stop_all()
    }

    fn cmd_players(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let connected = plugin_class(py)
            .call_method0(intern!(py, "players"))?
            .extract::<Vec<Bound<'_, PyAny>>>()?;
        if connected.is_empty() {
            tell(player, "There are no players connected at the moment.")?;
            return stop_all();
        }

        let mut table = format!(
            "{:^2} | {:^17} | {:^15} | Name\n",
            "ID", "SteamID64", "IP Address"
        );
        for candidate in &connected {
            table.push_str(&format!(
                "{:>2} | {:>17} | {:<15} | {}\n",
                candidate.getattr(intern!(py, "id"))?.extract::<i64>()?,
                steam_id_of(candidate)?,
                candidate.getattr(intern!(py, "ip"))?.extract::<String>()?,
                candidate.str()?
            ));
        }
        tell(player, &table)?;
        stop_all()
    }

    fn cmd_disconnects(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let recent = slf
            .get()
            .recent_disconnects
            .lock()
            .iter()
            .map(|(disconnected, at)| (disconnected.clone_ref(py), *at))
            .collect::<Vec<_>>();
        if recent.is_empty() {
            tell(player, "No players have disconnected yet.")?;
        } else {
            tell(
                player,
                &format!("The most recent ^6{}^7 player disconnects:", recent.len()),
            )?;
            for (disconnected, at) in &recent {
                let disconnected = disconnected.bind(py);
                tell(
                    player,
                    &format!(
                        "  {} ({}): ^6{}^7 seconds ago",
                        disconnected.getattr(intern!(py, "name"))?,
                        disconnected.getattr(intern!(py, "steam_id"))?,
                        (now() - at).round()
                    ),
                )?;
            }
        }
        stop_all()
    }

    fn cmd_commands(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let earlier = slf
            .get()
            .recent_commands
            .lock()
            .iter()
            .skip(1)
            .map(|(caller, args)| (caller.clone_ref(py), args.clone()))
            .collect::<Vec<_>>();
        if earlier.is_empty() {
            tell(player, "No commands have been recorded yet.")?;
        } else {
            tell(
                player,
                &format!("The most recent ^6{}^7 commands executed:", earlier.len()),
            )?;
            for (caller, args) in &earlier {
                tell(
                    player,
                    &format!(
                        "  {} executed: {args}",
                        caller.bind(py).getattr(intern!(py, "name"))?
                    ),
                )?;
            }
        }
        stop_all()
    }

    fn cmd_db(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let Some(key) = msg.get(1) else {
            return usage();
        };
        let py = slf.py();
        let database = slf.as_super().get_db()?;

        let lookup = || -> PyResult<Option<String>> {
            if !database.contains(key)? {
                return Ok(None);
            }
            let kind = database
                .call_method1(intern!(py, "type"), (key,))?
                .str()?
                .to_string();
            let value = match kind.as_str() {
                "string" => database.get_item(key)?,
                "list" => database.call_method1(intern!(py, "lrange"), (key, 0, -1))?,
                "set" => database.call_method1(intern!(py, "smembers"), (key,))?,
                "zset" => database.call_method(
                    intern!(py, "zrange"),
                    (key, 0, -1),
                    Some(&[("withscores", true)].into_py_dict(py)?),
                )?,
                _ => database.call_method1(intern!(py, "hgetall"), (key,))?,
            };
            Ok(Some(value.str()?.to_string()))
        };

        match lookup() {
            Ok(Some(value)) => reply(channel, &value),
            Ok(None) => reply(channel, "The key is not present in the database."),
            Err(err) => {
                let name = err
                    .get_type(py)
                    .name()
                    .map_or_else(|_| "Exception".to_owned(), |name| name.to_string());
                reply(channel, &format!("^1{name}^7: {}", err.value(py)))?;
                Err(err)
            }
        }
    }

    fn cmd_seen(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let Some(ident) = msg.get(1) else {
            return usage();
        };
        let py = slf.py();

        let Ok(steam_id) = ident.parse::<i64>() else {
            return reply(channel, "Unintelligible SteamID64.");
        };
        if steam_id < 64 {
            return reply(channel, "Invalid SteamID64.");
        }

        if let Some(online) = find_player(py, steam_id)? {
            return reply(
                channel,
                &format!(
                    "That would be {}^7, who is currently on this very server!",
                    online.str()?
                ),
            );
        }

        let owner = py
            .import(intern!(py, "shinqlx"))?
            .getattr(intern!(py, "owner"))?
            .call0()?
            .extract::<Option<i64>>()?;
        let name = if owner == Some(steam_id) {
            "my ^6master^7"
        } else {
            "that player"
        };

        let database = slf.as_super().get_db()?;
        let key = format!("{}:last_seen", player_key(steam_id));
        if !database.contains(&key)? {
            return reply(channel, &format!("^7I have never seen {name} before."));
        }

        let datetime = py
            .import(intern!(py, "datetime"))?
            .getattr(intern!(py, "datetime"))?;
        let then = datetime.call_method1(
            intern!(py, "strptime"),
            (database.get_item(&key)?, DATETIME_FORMAT),
        )?;
        let elapsed = datetime
            .call_method0(intern!(py, "now"))?
            .call_method1(intern!(py, "__sub__"), (then,))?;
        let days = elapsed.getattr(intern!(py, "days"))?.extract::<i64>()?;
        let seconds = elapsed.getattr(intern!(py, "seconds"))?.extract::<i64>()?;
        let (hours, minutes) = (seconds / 3600, seconds % 3600 / 60);

        if days != 0 {
            reply(
                channel,
                &format!(
                    "^7I saw {name} ^6{days}^7 day(s), ^6{hours}^7 hour(s) and ^6{minutes}^7 minute(s) ago."
                ),
            )
        } else {
            reply(
                channel,
                &format!("^7I saw {name} ^6{hours}^7 hour(s) and ^6{minutes}^7 minute(s) ago."),
            )
        }
    }

    fn cmd_time(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let time = py.import(intern!(py, "time"))?;
        let in_daylight_saving = time
            .call_method0(intern!(py, "localtime"))?
            .getattr(intern!(py, "tm_isdst"))?
            .extract::<i64>()?
            != 0;
        let local_seconds = time
            .getattr(if in_daylight_saving {
                "altzone"
            } else {
                "timezone"
            })?
            .extract::<i64>()?;
        let mut offset = -(local_seconds.div_euclid(60).div_euclid(60));

        if let Some(requested) = msg.get(1) {
            match requested.parse::<i64>() {
                Ok(requested) if requested.abs() < 24 => offset = requested,
                _ => return reply(channel, "Unintelligible time zone offset."),
            }
        }

        let datetime = py.import(intern!(py, "datetime"))?;
        let zone = datetime.getattr(intern!(py, "timezone"))?.call1((datetime
            .getattr(intern!(py, "timedelta"))?
            .call1((0, 0, 0, 0, 0, offset))?,))?;
        let current = datetime
            .getattr(intern!(py, "datetime"))?
            .call_method1(intern!(py, "now"), (zone,))?
            .call_method1(intern!(py, "strftime"), (TIME_FORMAT,))?;

        let message = match offset {
            0 => format!("The current time is: ^6{current} UTC"),
            offset if offset > 0 => format!("The current time is: ^6{current} UTC+{offset}"),
            offset => format!("The current time is: ^6{current} UTC{offset}"),
        };
        reply(channel, &message)
    }

    fn cmd_help(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let shinqlx = py.import(intern!(py, "shinqlx"))?;
        tell(
            player,
            &format!(
                "shinqlx: ^6{}^7 - Plugins: ^6{}",
                shinqlx.getattr(intern!(py, "__version__"))?,
                shinqlx.getattr(intern!(py, "__plugins_version__"))?
            ),
        )?;
        tell(
            player,
            "See ^6github.com/mgaertne/shinqlx^7 for more info about the mod and its commands.",
        )?;
        stop_all()
    }

    fn cmd_rcon(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 2 {
            return usage();
        }
        let py = slf.py();
        let shinqlx = py.import(intern!(py, "shinqlx"))?;
        let redirector = shinqlx
            .getattr(intern!(py, "redirect_print"))?
            .call1((channel,))?;
        redirector.call_method0(intern!(py, "__enter__"))?;
        let result = shinqlx
            .getattr(intern!(py, "console_command"))?
            .call1((msg[1..].join(" "),));
        redirector.call_method1(intern!(py, "__exit__"), (py.None(), py.None(), py.None()))?;
        result.map(|_| None)
    }

    fn cmd_shuffle(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        plugin_class(slf.py())
            .call_method0(intern!(slf.py(), "shuffle"))
            .map(|_| None)
    }

    fn cmd_pause(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        plugin_class(slf.py())
            .call_method0(intern!(slf.py(), "pause"))
            .map(|_| None)
    }

    fn cmd_unpause(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        plugin_class(slf.py())
            .call_method0(intern!(slf.py(), "unpause"))
            .map(|_| None)
    }

    fn cmd_slap(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let Some(ident) = msg.get(1) else {
            return usage();
        };
        let Lookup::Found(target) = lookup_player(player, channel, ident, "Invalid ID.", true)?
        else {
            return stop_all();
        };

        let damage = match msg.get(2).map(|damage| damage.parse::<i32>()) {
            None => 0,
            Some(Ok(damage)) => damage,
            Some(Err(_)) => {
                tell(player, "Invalid damage value.")?;
                return stop_all();
            }
        };
        plugin_class(slf.py()).call_method1(intern!(slf.py(), "slap"), (target, damage))?;
        stop_all()
    }

    fn cmd_slay(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let Some(ident) = msg.get(1) else {
            return usage();
        };
        let Lookup::Found(target) = lookup_player(player, channel, ident, "Invalid ID.", true)?
        else {
            return stop_all();
        };

        plugin_class(slf.py()).call_method1(intern!(slf.py(), "slay"), (target,))?;
        stop_all()
    }

    fn cmd_enable_sounds(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let database = slf.as_super().get_db()?;
        let enabled = sounds_enabled(&database, player)?;
        database.call_method1(
            intern!(slf.py(), "set_flag"),
            (player, SOUNDS_FLAG, !enabled),
        )?;

        let (state, next) = if enabled {
            ("disabled", "enable")
        } else {
            ("enabled", "disable")
        };
        tell(
            player,
            &format!(
                "Sounds have been {state}. Use ^6{}sounds^7 to {next} them again.",
                command_prefix()?
            ),
        )?;
        stop_all()
    }

    fn cmd_sound(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        Self::play_for_everybody(slf, player, &msg, "play_sound")
    }

    fn cmd_music(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        Self::play_for_everybody(slf, player, &msg, "play_music")
    }

    fn cmd_stopsound(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        Self::stop_playing(slf, player, "stop_sound")
    }

    fn cmd_stopmusic(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        Self::stop_playing(slf, player, "stop_music")
    }

    fn cmd_kick(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let reason = msg.get(2..).unwrap_or_default().join(" ");
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method1(intern!(target.py(), "kick"), (reason,))
                .map(drop)
        })
    }

    fn cmd_kickban(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method0(intern!(target.py(), "tempban"))
                .map(drop)
        })
    }

    fn cmd_yes(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        Self::decide_vote(slf.py(), channel, true)
    }

    fn cmd_no(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        Self::decide_vote(slf.py(), channel, false)
    }

    fn cmd_random(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let Some(limit) = msg.get(1) else {
            return usage();
        };
        let Ok(limit @ 1..) = limit.parse::<i64>() else {
            tell(player, "Invalid upper limit. Use a positive integer.")?;
            return stop_all();
        };

        let number = slf
            .py()
            .import(intern!(slf.py(), "random"))?
            .call_method1(intern!(slf.py(), "randint"), (1, limit))?;
        reply(channel, &format!("^3Random number is: ^5{number}"))
    }

    fn cmd_switch(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let [_, first, second, ..] = msg.as_slice() else {
            return usage();
        };
        let Lookup::Found(first) =
            lookup_player(player, channel, first, "The first ID is invalid.", false)?
        else {
            return Ok(None);
        };
        let Lookup::Found(second) =
            lookup_player(player, channel, second, "The second ID is invalid.", false)?
        else {
            return Ok(None);
        };

        plugin_class(slf.py())
            .call_method1(intern!(slf.py(), "switch"), (first, second))
            .map(|_| None)
    }

    fn cmd_red(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method1(intern!(target.py(), "put"), ("red",))
                .map(drop)
        })
    }

    fn cmd_blue(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method1(intern!(target.py(), "put"), ("blue",))
                .map(drop)
        })
    }

    fn cmd_spectate(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method1(intern!(target.py(), "put"), ("spectator",))
                .map(drop)
        })
    }

    fn cmd_free(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method1(intern!(target.py(), "put"), ("free",))
                .map(drop)
        })
    }

    fn cmd_addmod(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method0(intern!(target.py(), "addmod"))
                .map(drop)
        })
    }

    fn cmd_addadmin(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method0(intern!(target.py(), "addadmin"))
                .map(drop)
        })
    }

    fn cmd_demote(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method0(intern!(target.py(), "demote"))
                .map(drop)
        })
    }

    fn cmd_mute(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let Some(ident) = msg.get(1) else {
            return usage();
        };
        match lookup_player(player, channel, ident, "Invalid ID.", false)? {
            Lookup::Found(target) if target.eq(player)? => reply(channel, "I refuse."),
            Lookup::Found(target) => target
                .call_method0(intern!(target.py(), "mute"))
                .map(|_| None),
            Lookup::Finished(code) => Ok(code),
        }
    }

    fn cmd_unmute(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target
                .call_method0(intern!(target.py(), "unmute"))
                .map(drop)
        })
    }

    fn cmd_lock(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        Self::change_lock(slf.py(), player, &msg, "lock")
    }

    fn cmd_unlock(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        Self::change_lock(slf.py(), player, &msg, "unlock")
    }

    fn cmd_allready(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        match game_state(slf)?.as_deref() {
            None => reply(channel, "There is no game running."),
            Some("warmup") => plugin_class(slf.py())
                .call_method0(intern!(slf.py(), "allready"))
                .map(|_| None),
            Some(_) => reply(
                channel,
                "But the game's already in progress, you silly goose!",
            ),
        }
    }

    fn cmd_abort(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        match game_state(slf)?.as_deref() {
            None => reply(channel, "There is no game running."),
            Some("warmup") => reply(channel, "But the game isn't even on, you doofus!"),
            Some(_) => plugin_class(slf.py())
                .call_method0(intern!(slf.py(), "abort"))
                .map(|_| None),
        }
    }

    fn cmd_map(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let Some(map_name) = msg.get(1) else {
            return usage();
        };
        plugin_class(slf.py())
            .call_method1(intern!(slf.py(), "change_map"), (map_name, msg.get(2)))
            .map(|_| None)
    }

    fn cmd_teamsize(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let Some(size) = msg.get(1) else {
            return usage();
        };
        let Ok(size) = size.parse::<i64>() else {
            return reply(channel, "^7Unintelligible size.");
        };

        let py = slf.py();
        let game = slf.getattr(intern!(py, "game"))?;
        if game.is_none() {
            return reply(channel, "There is no game running.");
        }
        game.setattr(intern!(py, "teamsize"), size)?;
        plugin_class(py).call_method1(
            intern!(py, "msg"),
            (format!(
                "The teamsize has been set to ^6{size}^7 by {}.",
                player.str()?
            ),),
        )?;
        stop_all()
    }

    fn cmd_mappool(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        _msg: Vec<String>,
        _channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        match &slf.get().mappool {
            Some(mappool) if cvar_flag(py, "qlx_enforceMappool")? => {
                tell(player, &mappool_listing(mappool, 0))?;
            }
            _ => tell(
                player,
                "No map pool is being enforced. You are free to vote any map.",
            )?,
        }
        stop_all()
    }
}

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let class = plugin_class(py);
    class.call_method1(intern!(py, "set_cvar_once"), ("qlx_votepass", "1"))?;
    class.call_method1(
        intern!(py, "set_cvar_limit_once"),
        ("qlx_votepassThreshold", "0.33", "0", "1"),
    )?;
    class.call_method1(intern!(py, "set_cvar_once"), ("qlx_teamsizeMinimum", "1"))?;
    class.call_method1(intern!(py, "set_cvar_once"), ("qlx_teamsizeMaximum", "8"))?;
    class.call_method1(intern!(py, "set_cvar_once"), ("qlx_enforceMappool", "0"))?;

    let plugin = Bound::new(
        py,
        PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Essentials {
            vote_count: AtomicI64::new(0),
            last_vote: AtomicI64::new(0),
            recent_commands: Mutex::new(VecDeque::new()),
            recent_disconnects: Mutex::new(VecDeque::new()),
            mappool: load_mappool()?,
        }),
    )?;

    for (event, handler, priority) in [
        (
            "player_connect",
            "handle_player_connect",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            "player_disconnect",
            "handle_player_disconnect",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            "vote_called",
            "handle_vote_called",
            CommandPriorities::PRI_NORMAL,
        ),
        ("command", "handle_command", CommandPriorities::PRI_LOW),
    ] {
        register_hook(plugin.as_super(), event, handler, priority)?;
    }

    for (names, handler, permission, usage) in [
        (&["id"][..], "cmd_id", 1, "[part_of_name] ..."),
        (&["players"], "cmd_players", 1, ""),
        (&["disconnects", "dcs"], "cmd_disconnects", 1, ""),
        (&["commands", "cmds"], "cmd_commands", 2, ""),
        (&["shuffle"], "cmd_shuffle", 1, ""),
        (&["pause", "timeout"], "cmd_pause", 1, ""),
        (&["unpause", "timein"], "cmd_unpause", 1, ""),
        (&["slap"], "cmd_slap", 2, "<id> [damage]"),
        (&["slay"], "cmd_slay", 2, "<id>"),
        (&["sound"], "cmd_sound", 1, "<path>"),
        (&["music"], "cmd_music", 1, "<path>"),
        (&["stopsound"], "cmd_stopsound", 1, ""),
        (&["stopmusic"], "cmd_stopmusic", 1, ""),
        (&["kick"], "cmd_kick", 2, "<id>"),
        (&["kickban", "tempban"], "cmd_kickban", 2, "<id>"),
        (&["yes"], "cmd_yes", 2, ""),
        (&["no"], "cmd_no", 2, ""),
        (&["random"], "cmd_random", 1, "<limit>"),
        (&["switch"], "cmd_switch", 1, "<id> <id>"),
        (&["red"], "cmd_red", 1, "<id>"),
        (&["blue"], "cmd_blue", 1, "<id>"),
        (
            &["spectate", "spec", "spectator"],
            "cmd_spectate",
            1,
            "<id>",
        ),
        (&["free"], "cmd_free", 1, "<id>"),
        (&["addmod"], "cmd_addmod", 5, "<id>"),
        (&["addadmin"], "cmd_addadmin", 5, "<id>"),
        (&["demote"], "cmd_demote", 5, "<id>"),
        (&["mute"], "cmd_mute", 1, "<id>"),
        (&["unmute"], "cmd_unmute", 1, "<id>"),
        (&["lock"], "cmd_lock", 1, "[team]"),
        (&["unlock"], "cmd_unlock", 1, "[team]"),
        (&["allready"], "cmd_allready", 2, ""),
        (&["abort"], "cmd_abort", 2, ""),
        (&["map", "changemap"], "cmd_map", 2, "<mapname> [factory]"),
        (&["teamsize", "ts"], "cmd_teamsize", 2, "<size>"),
        (&["db"], "cmd_db", 5, "<key> [value]"),
        (&["seen"], "cmd_seen", 0, "<steam_id>"),
        (&["time"], "cmd_time", 0, "[timezone_offset]"),
        (&["help", "about", "version"], "cmd_help", 0, ""),
        (&["rcon"], "cmd_rcon", 5, ""),
    ] {
        register_command(plugin.as_super(), names, handler, permission, usage, None)?;
    }
    register_client_command(plugin.as_super(), &["sounds"], "cmd_enable_sounds", "<0/1>")?;
    register_client_command(
        plugin.as_super(),
        &["mappool", "maps", "maplist"],
        "cmd_mappool",
        "",
    )?;

    Ok(plugin.into_any())
}

#[cfg(test)]
mod creation_tests;
#[cfg(test)]
mod tests;
