use alloc::collections::VecDeque;
use core::{
    hint::cold_path,
    sync::atomic::{AtomicI64, Ordering},
};

use chrono::{FixedOffset, Local, NaiveDateTime, Utc};
use parking_lot::Mutex;
use pyo3::{
    PyTraverseError, exceptions::PyValueError, gc::PyVisit, intern, prelude::*, types::IntoPyDict,
};
use rand::RngExt;

use super::{
    create_cvar, create_cvar_limit, cvar_flag, cvar_float, cvar_int, register_client_command,
    register_command, register_hook,
    target::{find_player, reply, stop_all, usage},
    tell,
    timed_entries::{local_time, now},
};
use crate::python::{
    AbstractChannel, Game, GameMethods, Player, PlayerMethods, Plugin, abort, allready, change_map,
    commands::CommandPriorities,
    console_command, defer_delay,
    events_enum::EventName,
    get_cvar,
    handlers::{PrintRedirectorMethods, redirect_print},
    is_vote_active, lock, pause,
    plugin::{PluginMethods, chat},
    pyshinqlx_force_vote, pyshinqlx_owner, shuffle,
    string_enums::VoteType,
    unlock, unpause,
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

type PlaySound = fn(&Bound<'_, pyo3::types::PyType>, &str, Option<Player>) -> PyResult<bool>;
type StopSound = fn(&Bound<'_, pyo3::types::PyType>, Option<Player>) -> PyResult<()>;

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
    recent_commands: Mutex<VecDeque<(Py<Player>, String)>>,
    recent_disconnects: Mutex<VecDeque<(Py<Player>, f64)>>,
    mappool: Option<Mappool>,
}

fn plugin_class(py: Python<'_>) -> Bound<'_, pyo3::types::PyType> {
    py.get_type::<Plugin>()
}

fn player_key(steam_id: i64) -> String {
    format!("minqlx:players:{steam_id}")
}

fn command_prefix() -> PyResult<String> {
    Ok(get_cvar("qlx_commandPrefix")?.unwrap_or_default())
}

fn connected_players(py: Python<'_>) -> PyResult<Vec<Bound<'_, Player>>> {
    Plugin::players(&plugin_class(py))?
        .into_iter()
        .map(|player| Bound::new(py, player))
        .collect()
}

fn players_in_teams(py: Python<'_>, teams: &[&str]) -> PyResult<usize> {
    Ok(Plugin::players(&plugin_class(py))?
        .iter()
        .filter(|player| {
            player
                .get_team(py)
                .is_ok_and(|team| teams.contains(&team.as_str()))
        })
        .count())
}

fn current_game(py: Python<'_>) -> PyResult<Option<Bound<'_, Game>>> {
    Game::py_new(py, true)
        .ok()
        .map(|game| Bound::new(py, game))
        .transpose()
}

fn game_state(py: Python<'_>) -> PyResult<Option<String>> {
    current_game(py)?.map(|game| game.get_state()).transpose()
}

impl Essentials {
    fn play_for_everybody(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: &[String],
        play: PlaySound,
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
        if !play(&class, path, Some(player.get().clone()))? {
            tell(player, "Invalid sound.")?;
            return stop_all();
        }

        for other in connected_players(slf.py())? {
            if !other.as_any().eq(player)? && sounds_enabled(&database, &other)? {
                play(&class, path, Some(other.get().clone()))?;
            }
        }
        stop_all()
    }

    fn stop_playing(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        stop: StopSound,
    ) -> PyResult<Option<i32>> {
        if !sounds_enabled(&slf.as_super().get_db()?, player)? {
            tell_sounds_disabled(player)?;
            return stop_all();
        }
        stop(&plugin_class(slf.py()), None).map(|()| None)
    }

    fn decide_vote(
        py: Python<'_>,
        channel: &Bound<'_, AbstractChannel>,
        pass_it: bool,
    ) -> PyResult<Option<i32>> {
        if !is_vote_active() {
            return reply(channel, "There is no active vote!");
        }
        pyshinqlx_force_vote(py, pass_it).map(|_| None)
    }

    fn change_lock(
        player: &Bound<'_, Player>,
        msg: &[String],
        change: fn(Option<&str>) -> PyResult<()>,
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
        change(team).map(|()| None)
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

    fn handle_player_connect(slf: &Bound<'_, Self>, player: &Bound<'_, Player>) -> PyResult<()> {
        update_player(&slf.as_super().get_db()?, player)
    }

    fn handle_player_disconnect(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        _reason: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        {
            let mut recent = slf.get().recent_disconnects.lock();
            recent.push_front((player.clone().unbind(), now()));
            recent.truncate(RECENT_DISCONNECTS);
        }

        let key = format!("{}:last_seen", player_key(player.get_steam_id()));
        slf.as_super().get_db()?.set_item(key, local_time(now())?)
    }

    fn handle_vote_called(
        slf: &Bound<'_, Self>,
        caller: &Bound<'_, Player>,
        vote: &str,
        args: &str,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let vote = VoteType::from(vote.to_lowercase().as_str());

        if vote == VoteType::Teamsize {
            let Ok(size) = args.trim().parse::<i64>() else {
                return Ok(None);
            };
            if size > cvar_int("qlx_teamsizeMaximum")? {
                tell(
                    caller,
                    "The team size is larger than what the server allows.",
                )?;
                return stop_all();
            }
            if size < cvar_int("qlx_teamsizeMinimum")? {
                tell(
                    caller,
                    "The team size is smaller than what the server allows.",
                )?;
                return stop_all();
            }
        }

        if let Some(mappool) = &slf.get().mappool
            && vote == VoteType::Map
            && cvar_flag("qlx_enforceMappool")?
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
                None => current_game(py)?
                    .map(|game| game.get_factory())
                    .transpose()?,
            };
            if factory.is_some_and(|factory| !factory.is_empty() && !factories.contains(&factory)) {
                return mappool_error(caller, "This factory is not allowed on that map.");
            }
        }

        if cvar_flag("qlx_votepass")? {
            let vote_id = slf.get().vote_count.fetch_add(1, Ordering::SeqCst);
            slf.get().last_vote.store(vote_id, Ordering::SeqCst);
            let threshold = cvar_float("qlx_votepassThreshold")?;
            defer_delay(
                &slf.getattr(intern!(py, "force"))?,
                VOTE_RESOLUTION_DELAY,
                (threshold, vote_id),
            )?;
        }
        Ok(None)
    }

    fn force(slf: &Bound<'_, Self>, require: f64, vote_id: i64) -> PyResult<()> {
        let py = slf.py();
        if slf.get().last_vote.load(Ordering::SeqCst) != vote_id {
            return Ok(());
        }

        if !is_vote_active() {
            return Ok(());
        }
        let votes = Plugin::current_vote_count(&plugin_class(py))?;
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
        pyshinqlx_force_vote(py, true).map(|_| ())
    }

    fn handle_command(
        slf: &Bound<'_, Self>,
        caller: &Bound<'_, Player>,
        _command: &Bound<'_, PyAny>,
        args: &str,
    ) -> PyResult<()> {
        let mut recent = slf.get().recent_commands.lock();
        recent.push_front((caller.clone().unbind(), args.to_owned()));
        recent.truncate(RECENT_COMMANDS);
        Ok(())
    }

    fn cmd_id(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let class = plugin_class(py);
        let connected = connected_players(py)?;

        let list_alternatives = |players: &[Bound<'_, Player>]| -> PyResult<()> {
            let lines = players
                .iter()
                .map(|candidate| format!("  {}^6:^7 {}", candidate.get_id(), candidate.get_name()))
                .collect::<Vec<_>>();
            tell(player, &lines.join("\n"))
        };

        if connected.is_empty() {
            tell(player, "There are no players connected at the moment.")?;
        } else if msg.len() == 1 {
            tell(player, "All connected players:")?;
            list_alternatives(&connected)?;
        } else {
            let mut found: Vec<Bound<'_, Player>> = vec![];
            for name in &msg[1..] {
                for candidate in Plugin::find_player(&class, name, None) {
                    let candidate = Bound::new(py, candidate)?;
                    let steam_id = candidate.get_steam_id();
                    if !found.iter().any(|known| known.get_steam_id() == steam_id) {
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
        player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let connected = connected_players(py)?;
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
                candidate.get_id(),
                candidate.get_steam_id(),
                candidate.get_ip(),
                candidate.str()?
            ));
        }
        tell(player, &table)?;
        stop_all()
    }

    fn cmd_disconnects(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
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
                        disconnected.get_name(),
                        disconnected.get_steam_id(),
                        (now() - at).round()
                    ),
                )?;
            }
        }
        stop_all()
    }

    fn cmd_commands(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
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
                    &format!("  {} executed: {args}", caller.bind(py).get_name()),
                )?;
            }
        }
        stop_all()
    }

    fn cmd_db(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
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
        _player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
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

        let name = if pyshinqlx_owner(py)? == Some(steam_id) {
            "my ^6master^7"
        } else {
            "that player"
        };

        let database = slf.as_super().get_db()?;
        let key = format!("{}:last_seen", player_key(steam_id));
        if !database.contains(&key)? {
            return reply(channel, &format!("^7I have never seen {name} before."));
        }

        let last_seen = database.get_item(&key)?.extract::<String>()?;
        let then = NaiveDateTime::parse_from_str(&last_seen, DATETIME_FORMAT).map_err(|_| {
            PyValueError::new_err(format!(
                "time data '{last_seen}' does not match format '{DATETIME_FORMAT}'"
            ))
        })?;
        let elapsed = Local::now().naive_local() - then;
        let days = elapsed.num_days();
        let seconds = elapsed.num_seconds() - days * 86_400;
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
        _slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let mut offset = i64::from(Local::now().offset().local_minus_utc()).div_euclid(3600);

        if let Some(requested) = msg.get(1) {
            match requested.parse::<i64>() {
                Ok(requested) if requested.abs() < 24 => offset = requested,
                _ => return reply(channel, "Unintelligible time zone offset."),
            }
        }

        let zone = i32::try_from(offset * 3600)
            .ok()
            .and_then(FixedOffset::east_opt)
            .ok_or_else(|| {
                cold_path();
                PyValueError::new_err("time zone offset out of range")
            })?;
        let current = Utc::now().with_timezone(&zone).format(TIME_FORMAT);

        let message = match offset {
            0 => format!("The current time is: ^6{current} UTC"),
            offset if offset > 0 => format!("The current time is: ^6{current} UTC+{offset}"),
            offset => format!("The current time is: ^6{current} UTC{offset}"),
        };
        reply(channel, &message)
    }

    fn cmd_help(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let py = slf.py();
        let shinqlx = py.import(intern!(py, "shinqlx"))?;
        tell(
            player,
            &format!(
                "shinqlx: ^6{}^7 - Plugins: ^6{}",
                env!("SHINQLX_VERSION"),
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
        _player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        if msg.len() < 2 {
            return usage();
        }
        let py = slf.py();
        let redirector = Bound::new(py, redirect_print(py, channel.clone().into_any())?)?;
        let redirector = redirector.context_manager_enter();
        let result = console_command(&msg[1..].join(" "));
        redirector.context_manager_exit(py.None(), py.None(), py.None())?;
        result.map(|()| None)
    }

    fn cmd_shuffle(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        shuffle(slf.py()).map(|_| None)
    }

    fn cmd_pause(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        pause(slf.py()).map(|_| None)
    }

    fn cmd_unpause(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        unpause(slf.py()).map(|_| None)
    }

    fn cmd_slap(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
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
        target.slap(damage)?;
        stop_all()
    }

    fn cmd_slay(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let Some(ident) = msg.get(1) else {
            return usage();
        };
        let Lookup::Found(target) = lookup_player(player, channel, ident, "Invalid ID.", true)?
        else {
            return stop_all();
        };

        target.slay()?;
        stop_all()
    }

    fn cmd_enable_sounds(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
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
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        Self::play_for_everybody(slf, player, &msg, Plugin::play_sound)
    }

    fn cmd_music(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        Self::play_for_everybody(slf, player, &msg, Plugin::play_music)
    }

    fn cmd_stopsound(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        Self::stop_playing(slf, player, Plugin::stop_sound)
    }

    fn cmd_stopmusic(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        Self::stop_playing(slf, player, Plugin::stop_music)
    }

    fn cmd_kick(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let reason = msg.get(2..).unwrap_or_default().join(" ");
        act_on_player(player, &msg, channel, |target| target.kick(&reason))
    }

    fn cmd_kickban(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| target.tempban())
    }

    fn cmd_yes(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        Self::decide_vote(slf.py(), channel, true)
    }

    fn cmd_no(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        Self::decide_vote(slf.py(), channel, false)
    }

    fn cmd_random(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let Some(limit) = msg.get(1) else {
            return usage();
        };
        let Ok(limit @ 1..) = limit.parse::<i64>() else {
            tell(player, "Invalid upper limit. Use a positive integer.")?;
            return stop_all();
        };

        let number = rand::rng().random_range(1..=limit);
        reply(channel, &format!("^3Random number is: ^5{number}"))
    }

    fn cmd_switch(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
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

        first.switch(&second).map(|()| None)
    }

    fn cmd_red(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| target.put("red"))
    }

    fn cmd_blue(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| target.put("blue"))
    }

    fn cmd_spectate(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| target.put("spectator"))
    }

    fn cmd_free(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| target.put("free"))
    }

    fn cmd_addmod(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| target.addmod())
    }

    fn cmd_addadmin(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| target.addadmin())
    }

    fn cmd_demote(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| target.demote())
    }

    fn cmd_mute(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let Some(ident) = msg.get(1) else {
            return usage();
        };
        match lookup_player(player, channel, ident, "Invalid ID.", false)? {
            Lookup::Found(target) if target.as_any().eq(player)? => reply(channel, "I refuse."),
            Lookup::Found(target) => target.mute().map(|()| None),
            Lookup::Finished(code) => Ok(code),
        }
    }

    fn cmd_unmute(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| target.unmute())
    }

    fn cmd_lock(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        Self::change_lock(player, &msg, lock)
    }

    fn cmd_unlock(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        Self::change_lock(player, &msg, unlock)
    }

    fn cmd_allready(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        match game_state(slf.py())?.as_deref() {
            None => reply(channel, "There is no game running."),
            Some("warmup") => allready(slf.py()).map(|_| None),
            Some(_) => reply(
                channel,
                "But the game's already in progress, you silly goose!",
            ),
        }
    }

    fn cmd_abort(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        _msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        match game_state(slf.py())?.as_deref() {
            None => reply(channel, "There is no game running."),
            Some("warmup") => reply(channel, "But the game isn't even on, you doofus!"),
            Some(_) => abort(slf.py()).map(|_| None),
        }
    }

    fn cmd_map(
        slf: &Bound<'_, Self>,
        _player: &Bound<'_, Player>,
        msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let Some(map_name) = msg.get(1) else {
            return usage();
        };
        change_map(slf.py(), map_name, msg.get(2).map(String::as_str)).map(|_| None)
    }

    fn cmd_teamsize(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        msg: Vec<String>,
        channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        let Some(size) = msg.get(1) else {
            return usage();
        };
        let Ok(size) = size.parse::<i32>() else {
            return reply(channel, "^7Unintelligible size.");
        };

        let py = slf.py();
        let Some(game) = current_game(py)? else {
            return reply(channel, "There is no game running.");
        };
        game.set_teamsize(size)?;
        chat(
            py,
            &format!(
                "The teamsize has been set to ^6{size}^7 by {}.",
                player.str()?
            ),
            100,
            " ",
        )?;
        stop_all()
    }

    fn cmd_mappool(
        slf: &Bound<'_, Self>,
        player: &Bound<'_, Player>,
        _msg: Vec<String>,
        _channel: &Bound<'_, AbstractChannel>,
    ) -> PyResult<Option<i32>> {
        match &slf.get().mappool {
            Some(mappool) if cvar_flag("qlx_enforceMappool")? => {
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

pub(super) fn create(py: Python<'_>) -> PyResult<Bound<'_, Plugin>> {
    create_cvar(py, "qlx_votepass", "1")?;
    create_cvar_limit(py, "qlx_votepassThreshold", "0.33", "0", "1")?;
    create_cvar(py, "qlx_teamsizeMinimum", "1")?;
    create_cvar(py, "qlx_teamsizeMaximum", "8")?;
    create_cvar(py, "qlx_enforceMappool", "0")?;

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
            EventName::PlayerConnect,
            "handle_player_connect",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            EventName::PlayerDisconnect,
            "handle_player_disconnect",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            EventName::VoteCalled,
            "handle_vote_called",
            CommandPriorities::PRI_NORMAL,
        ),
        (
            EventName::Command,
            "handle_command",
            CommandPriorities::PRI_LOW,
        ),
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

    Ok(plugin.into_super())
}

#[cfg(test)]
mod creation_tests;
#[cfg(test)]
mod tests;
