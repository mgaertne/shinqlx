use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::atomic::{AtomicI64, Ordering},
};

use log::warn;
use parking_lot::Mutex;
use pyo3::{
    intern,
    prelude::*,
    types::{IntoPyDict, PyFloat, PyInt},
};

use super::{
    cvar_flag, register_client_command, register_command, register_hook,
    target::{Resolution, Target, find_player, reply, resolve_text, stop_all, usage},
    tell,
    timed_entries::{local_time, now},
};
use crate::ffi::python::{
    Plugin, commands::CommandPriorities, delay, get_cvar, plugin::PluginMethods,
};

const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const TIME_FORMAT: &str = "%H:%M:%S";
const RECENT_COMMANDS: usize = 11;
const RECENT_DISCONNECTS: usize = 10;
const VOTE_RESOLUTION_DELAY: f64 = 29.0;

type Mappool = BTreeMap<String, Vec<String>>;

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

fn parse_mappool(contents: &str) -> Mappool {
    let mut mappool = Mappool::new();
    for line in contents.lines() {
        let line = line.trim_start();
        if line.starts_with('#') {
            continue;
        }
        let Some((map, factory)) = line.split_once('|') else {
            continue;
        };
        mappool
            .entry(map.trim().to_lowercase())
            .or_default()
            .push(factory.trim().to_owned());
    }
    mappool
}

fn read_mappool(path: &Path) -> Option<Mappool> {
    std::fs::read_to_string(path)
        .map_err(|err| warn!(target: "shinqlx", "could not read the map pool {path:?}: {err}"))
        .ok()
        .map(|contents| parse_mappool(&contents))
}

fn load_mappool() -> PyResult<Option<Mappool>> {
    let Some(mappool_file) = get_cvar("sv_mappoolfile")? else {
        return Ok(None);
    };
    for cvar in ["fs_homepath", "fs_basepath"] {
        let Some(base) = get_cvar(cvar)? else {
            continue;
        };
        let path = Path::new(&base).join("baseq3").join(&mappool_file);
        if path.is_file() {
            return Ok(read_mappool(&path));
        }
    }
    Ok(None)
}

fn mappool_listing(mappool: &Mappool, indent: usize) -> String {
    mappool
        .iter()
        .map(|(map, factories)| {
            format!(
                "{}{map:25} Factories: {}",
                " ".repeat(indent),
                factories.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn players_in_teams(py: Python<'_>, teams: &[&str]) -> PyResult<usize> {
    let all_teams = plugin_class(py).call_method0(intern!(py, "teams"))?;
    teams.iter().try_fold(0, |count, team| {
        Ok(count + all_teams.get_item(*team)?.len()?)
    })
}

fn update_player(database: &Bound<'_, PyAny>, player: &Bound<'_, PyAny>) -> PyResult<()> {
    let py = database.py();
    let steam_id = steam_id_of(player)?;
    let base_key = player_key(steam_id);
    let pipeline = database.call_method0(intern!(py, "pipeline"))?;

    let ip = player.getattr(intern!(py, "ip"))?.extract::<String>()?;
    if !ip.is_empty() {
        pipeline.call_method1(intern!(py, "sadd"), ("minqlx:ips", &ip))?;
        pipeline.call_method1(intern!(py, "sadd"), (format!("minqlx:ips:{ip}"), steam_id))?;
        pipeline.call_method1(intern!(py, "sadd"), (format!("{base_key}:ips"), &ip))?;
    }

    let name = player.getattr(intern!(py, "name"))?;
    if !database.contains(&base_key)? {
        pipeline.call_method1(intern!(py, "lpush"), (&base_key, &name))?;
        pipeline.call_method1(intern!(py, "sadd"), ("minqlx:players", steam_id))?;
    } else {
        let clean_name = player.getattr(intern!(py, "clean_name"))?;
        let known = database
            .call_method1(intern!(py, "lrange"), (&base_key, 0, -1))?
            .try_iter()?
            .map(|known_name| {
                plugin_class(py).call_method1(intern!(py, "clean_text"), (known_name?,))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let mut is_known = false;
        for known_name in &known {
            is_known |= known_name.eq(&clean_name)?;
        }
        if !is_known {
            pipeline.call_method1(intern!(py, "lpush"), (&base_key, &name))?;
            pipeline.call_method1(intern!(py, "ltrim"), (&base_key, 0, 19))?;
        }
    }

    pipeline.call_method0(intern!(py, "execute")).map(|_| ())
}

const SOUNDS_FLAG: &str = "essentials:sounds_enabled";

enum Lookup<'py> {
    Found(Bound<'py, PyAny>),
    Finished(Option<i32>),
}

fn lookup_player<'py>(
    caller: &Bound<'py, PyAny>,
    channel: &Bound<'py, PyAny>,
    text: &str,
    invalid: &str,
    tell_caller: bool,
) -> PyResult<Lookup<'py>> {
    match resolve_text(caller.py(), caller, text)? {
        Resolution::Target(Target::Player(_, player)) => Ok(Lookup::Found(player)),
        Resolution::Reported => stop_all().map(Lookup::Finished),
        Resolution::Target(Target::Id(_)) | Resolution::UnknownClientId if tell_caller => {
            tell(caller, invalid)?;
            stop_all().map(Lookup::Finished)
        }
        Resolution::Target(Target::Id(_)) | Resolution::UnknownClientId => {
            reply(channel, invalid).map(Lookup::Finished)
        }
    }
}

fn act_on_player<'py>(
    caller: &Bound<'py, PyAny>,
    msg: &[String],
    channel: &Bound<'py, PyAny>,
    action: impl FnOnce(&Bound<'py, PyAny>) -> PyResult<()>,
) -> PyResult<Option<i32>> {
    let Some(ident) = msg.get(1) else {
        return usage();
    };
    match lookup_player(caller, channel, ident, "Invalid ID.", false)? {
        Lookup::Found(player) => action(&player).map(|()| None),
        Lookup::Finished(code) => Ok(code),
    }
}

fn sounds_enabled(database: &Bound<'_, PyAny>, player: &Bound<'_, PyAny>) -> PyResult<bool> {
    let py = database.py();
    database
        .call_method(
            intern!(py, "get_flag"),
            (player, SOUNDS_FLAG),
            Some(&[("default", true)].into_py_dict(py)?),
        )?
        .extract()
}

fn tell_sounds_disabled(player: &Bound<'_, PyAny>) -> PyResult<()> {
    tell(
        player,
        &format!(
            "Sounds are disabled. Use ^6{}sounds^7 to enable them again.",
            command_prefix()?
        ),
    )
}

fn game_state(plugin: &Bound<'_, Essentials>) -> PyResult<Option<String>> {
    let py = plugin.py();
    let game = plugin.getattr(intern!(py, "game"))?;
    if game.is_none() {
        return Ok(None);
    }
    game.getattr(intern!(py, "state"))?.extract().map(Some)
}

fn mappool_error(caller: &Bound<'_, PyAny>, message: &str, command: bool) -> PyResult<Option<i32>> {
    let message = if command {
        format!(
            "{message} Use {}mappool to see available options.",
            command_prefix()?
        )
    } else {
        message.to_owned()
    };
    tell(caller, &message)?;
    stop_all()
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
                return mappool_error(caller, "This map is not allowed.", true);
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
                return mappool_error(caller, "This factory is not allowed on that map.", true);
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

        let datetime = py.import(intern!(py, "datetime"))?.getattr("datetime")?;
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
        let zone = datetime.getattr("timezone")?.call1((datetime
            .getattr("timedelta")?
            .call1((0, 0, 0, 0, 0, offset))?,))?;
        let current = datetime
            .getattr("datetime")?
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
            target.call_method1("kick", (reason,)).map(drop)
        })
    }

    fn cmd_kickban(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target.call_method0("tempban").map(drop)
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
            target.call_method1("put", ("red",)).map(drop)
        })
    }

    fn cmd_blue(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target.call_method1("put", ("blue",)).map(drop)
        })
    }

    fn cmd_spectate(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target.call_method1("put", ("spectator",)).map(drop)
        })
    }

    fn cmd_free(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target.call_method1("put", ("free",)).map(drop)
        })
    }

    fn cmd_addmod(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target.call_method0("addmod").map(drop)
        })
    }

    fn cmd_addadmin(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target.call_method0("addadmin").map(drop)
        })
    }

    fn cmd_demote(
        _slf: &Bound<'_, Self>,
        player: &Bound<'_, PyAny>,
        msg: Vec<String>,
        channel: &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>> {
        act_on_player(player, &msg, channel, |target| {
            target.call_method0("demote").map(drop)
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
            Lookup::Found(target) => target.call_method0("mute").map(|_| None),
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
            target.call_method0("unmute").map(drop)
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
mod essentials_tests {
    use core::ffi::CStr;
    use std::sync::atomic::Ordering;

    use pyo3::{prelude::*, types::PyDict};
    use rstest::rstest;

    use super::{Essentials, SOUNDS_FLAG, mappool_listing, parse_mappool, player_key};
    use crate::{
        ffi::python::{
            Plugin, PythonReturnCodes,
            native_plugins::test_support::{FakeRedis, REDIS_5, fixture_module},
            plugin::PluginMethods,
            pyshinqlx_setup_fixture::pyshinqlx_setup,
            pyshinqlx_test_support::{RestoredAttribute, engine_with_cvars},
        },
        prelude::*,
    };

    const STOP_ALL: i32 = PythonReturnCodes::RET_STOP_ALL as i32;
    const USAGE: i32 = PythonReturnCodes::RET_USAGE as i32;
    const CALLER_ID: i64 = 76_561_198_000_000_001;
    const TARGET_ID: i64 = 76_561_198_000_000_003;

    const FIXTURES: &CStr = c"
import shinqlx


class Player:
    def __init__(self, client_id, steam_id, name, ip):
        self.id = client_id
        self.steam_id = steam_id
        self.name = name
        self.clean_name = name[:-2]
        self.ip = ip
        self.told = []

    def tell(self, message):
        self.told.append(message)

    def kick(self, reason=''):
        actions.append(('kick', self.id, reason))

    def tempban(self):
        actions.append(('tempban', self.id))

    def put(self, team):
        actions.append(('put', self.id, team))

    def addmod(self):
        actions.append(('addmod', self.id))

    def addadmin(self):
        actions.append(('addadmin', self.id))

    def demote(self):
        actions.append(('demote', self.id))

    def mute(self):
        actions.append(('mute', self.id))

    def unmute(self):
        actions.append(('unmute', self.id))

    def __str__(self):
        return self.name


actions = []


caller = Player(1, 76561198000000001, 'Caller^7', '1.1.1.1')
target = Player(3, 76561198000000003, 'Eve^7', '')
online = [caller, target]
team_members = {'red': [caller], 'blue': [target], 'free': [], 'spectator': []}
vote = {'active': False, 'counts': None}
forced = []
console = []
redirected = []
game_info = {'factory': 'ca'}
owner_id = [76561198000000009]


class FakeGame:
    factory = 'ca'
    state = 'warmup'
    teamsize = 4


game_holder = [FakeGame()]
game = property(lambda self: game_holder[0])


def recorder(name, result=None):
    def record(cls, *args):
        actions.append((name,) + tuple(a.id if isinstance(a, Player) else a for a in args))
        return result
    return record


shuffle = recorder('shuffle')
pause = recorder('pause')
unpause = recorder('unpause')
slap = recorder('slap')
slay = recorder('slay')
stop_sound = recorder('stop_sound')
stop_music = recorder('stop_music')
switch = recorder('switch')
lock = recorder('lock')
unlock = recorder('unlock')
allready = recorder('allready')
abort = recorder('abort')
change_map = recorder('change_map')
msg = recorder('msg')


def play_sound(cls, path, player=None):
    actions.append(('play_sound', path, player.id))
    return path != 'bad'


def play_music(cls, path, player=None):
    actions.append(('play_music', path, player.id))
    return path != 'bad'


def players(cls):
    return list(online)


def find_player(cls, name, player_list=None):
    return [p for p in online if name.lower() in p.clean_name.lower()]


def player(cls, ident, player_list=None):
    for candidate in online:
        if ident in (candidate.id, candidate.steam_id):
            return candidate
    return None


def teams(cls, player_list=None):
    return team_members


def clean_text(cls, text):
    return text.replace('^7', '')


def is_vote_active(cls):
    return vote['active']


def current_vote_count(cls):
    return vote['counts']


def force_vote(cls, pass_it):
    forced.append(pass_it)
    return True


def fake_owner():
    return owner_id[0]


def fake_console_command(command):
    console.append(command)
    if command == 'fail':
        raise RuntimeError('boom')


class Redirect:
    def __init__(self, channel):
        redirected.append('created')

    def __enter__(self):
        redirected.append('entered')

    def __exit__(self, *args):
        redirected.append('left')


for _name in ('players', 'find_player', 'player', 'teams', 'clean_text', 'is_vote_active',
              'current_vote_count', 'force_vote', 'shuffle', 'pause', 'unpause', 'slap', 'slay',
              'stop_sound', 'stop_music', 'switch', 'lock', 'unlock', 'allready', 'abort',
              'change_map', 'msg', 'play_sound', 'play_music'):
    globals()[_name] = classmethod(globals()[_name])
";

    const PLUGIN_METHODS: [&str; 24] = [
        "players",
        "find_player",
        "player",
        "teams",
        "clean_text",
        "is_vote_active",
        "current_vote_count",
        "force_vote",
        "shuffle",
        "pause",
        "unpause",
        "slap",
        "slay",
        "stop_sound",
        "stop_music",
        "switch",
        "lock",
        "unlock",
        "allready",
        "abort",
        "change_map",
        "msg",
        "play_sound",
        "play_music",
    ];

    struct Fixtures<'py> {
        plugin: Bound<'py, Essentials>,
        module: Bound<'py, PyModule>,
        database: Bound<'py, PyAny>,
        channel: Bound<'py, PyAny>,
        caller: Bound<'py, PyAny>,
        target: Bound<'py, PyAny>,
    }

    impl Fixtures<'_> {
        fn replies(&self) -> Vec<String> {
            self.channel
                .getattr("replies")
                .and_then(|replies| replies.extract())
                .expect("could not get the replies")
        }

        fn told(&self, player: &Bound<'_, PyAny>) -> Vec<String> {
            player
                .getattr("told")
                .and_then(|told| told.extract())
                .expect("could not get told messages")
        }

        fn strings(&self, name: &str) -> Vec<String> {
            self.module
                .getattr(name)
                .and_then(|values| values.extract())
                .expect("could not get the strings")
        }

        fn forced(&self) -> usize {
            self.module
                .getattr("forced")
                .and_then(|forced| forced.len())
                .expect("could not count the forced votes")
        }

        fn set_vote(&self, active: bool, counts: Option<(i64, i64)>) {
            let vote = self.module.getattr("vote").expect("no vote");
            vote.set_item("active", active).expect("no active");
            vote.set_item("counts", counts).expect("no counts");
        }

        fn store(&self, key: &str, value: &str) {
            self.database
                .getattr("strings")
                .and_then(|strings| strings.set_item(key, value))
                .expect("could not store the value");
        }

        fn stored(&self, key: &str) -> Option<String> {
            self.database
                .getattr("strings")
                .and_then(|strings| strings.call_method1("get", (key,)))
                .and_then(|value| value.extract())
                .expect("could not read the value")
        }

        fn names(&self, steam_id: i64) -> Vec<String> {
            self.database
                .getattr("lists")
                .and_then(|lists| {
                    lists.call_method1("get", (player_key(steam_id), Vec::<String>::new()))
                })
                .and_then(|names| names.extract())
                .expect("could not read the names")
        }

        fn set_members(&self, key: &str) -> Vec<String> {
            self.database
                .getattr("sets")
                .and_then(|sets| sets.call_method1("get", (key, Vec::<String>::new())))
                .and_then(|members| {
                    self.database
                        .py()
                        .import("builtins")?
                        .call_method1("sorted", (members,))?
                        .extract::<Vec<String>>()
                })
                .expect("could not read the set")
        }
    }

    fn with_essentials<F>(cvars: &[(&'static str, &'static CStr)], mappool: Option<&str>, test: F)
    where
        F: FnOnce(Fixtures<'_>),
    {
        let mut engine = engine_with_cvars(cvars);
        engine.run(|| {
            Python::attach(|py| {
                let _fake_redis = FakeRedis::install(py, REDIS_5);
                let module = fixture_module(py, FIXTURES);
                let plugin_class = py.get_type::<Plugin>();
                let _restore_database =
                    RestoredAttribute::capture(plugin_class.as_any(), "database");
                plugin_class
                    .setattr("database", module.getattr("Database").expect("no Database"))
                    .expect("could not set the database");
                let _restore_game = RestoredAttribute::capture(plugin_class.as_any(), "game");
                plugin_class
                    .setattr("game", module.getattr("game").expect("no game"))
                    .expect("could not set the game");
                let _restored = PLUGIN_METHODS.map(|name| {
                    let restored = RestoredAttribute::capture(plugin_class.as_any(), name);
                    plugin_class
                        .setattr(name, module.getattr(name).expect("missing fixture"))
                        .expect("could not replace the Plugin method");
                    restored
                });
                let shinqlx = py.import("shinqlx").expect("no shinqlx");
                let _restored_functions = [
                    ("owner", "fake_owner"),
                    ("console_command", "fake_console_command"),
                    ("redirect_print", "Redirect"),
                ]
                .map(|(name, fake)| {
                    let restored = RestoredAttribute::capture(shinqlx.as_any(), name);
                    shinqlx
                        .setattr(name, module.getattr(fake).expect("missing fake"))
                        .expect("could not replace the function");
                    restored
                });

                let plugin = Bound::new(
                    py,
                    PyClassInitializer::from(Plugin::py_new(py)).add_subclass(Essentials {
                        vote_count: Default::default(),
                        last_vote: Default::default(),
                        recent_commands: Default::default(),
                        recent_disconnects: Default::default(),
                        mappool: mappool.map(parse_mappool),
                    }),
                )
                .expect("could not create the plugin");
                let database = plugin.as_super().get_db().expect("no database");
                let get = |name: &str| module.getattr(name).expect("missing fixture");
                test(Fixtures {
                    plugin,
                    database,
                    channel: get("Channel").call0().expect("no channel"),
                    caller: get("caller"),
                    target: get("target"),
                    module: module.clone(),
                });
            });
        });
    }

    fn msg(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    const VOTE_CVARS: [(&str, &CStr); 6] = [
        ("qlx_votepass", c"1"),
        ("qlx_votepassThreshold", c"0.5"),
        ("qlx_teamsizeMinimum", c"2"),
        ("qlx_teamsizeMaximum", c"6"),
        ("qlx_enforceMappool", c"1"),
        ("qlx_commandPrefix", c"!"),
    ];

    const NO_VOTEPASS: [(&str, &CStr); 6] = [
        ("qlx_votepass", c"0"),
        ("qlx_votepassThreshold", c"0.5"),
        ("qlx_teamsizeMinimum", c"2"),
        ("qlx_teamsizeMaximum", c"6"),
        ("qlx_enforceMappool", c"1"),
        ("qlx_commandPrefix", c"!"),
    ];

    const MAPPOOL: &str =
        "# comment\ncampgrounds|ca\nCampgrounds|ffa\n  overkill | ca\nnot a line\n";

    #[test]
    fn the_mappool_file_is_parsed_into_maps_and_factories() {
        let mappool = parse_mappool(MAPPOOL);

        assert_eq!(mappool.len(), 2);
        assert_eq!(mappool["campgrounds"], ["ca", "ffa"]);
        assert_eq!(mappool["overkill"], ["ca"]);
        assert_eq!(
            mappool_listing(&mappool, 2),
            "  campgrounds               Factories: ca, ffa\n  overkill                  Factories: ca"
        );
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn connecting_players_are_added_to_the_player_ip_and_name_lists(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::handle_player_connect(&fixtures.plugin, &fixtures.caller);

            assert!(result.is_ok());
            assert_eq!(fixtures.names(CALLER_ID), ["Caller^7"]);
            assert_eq!(
                fixtures.set_members("minqlx:players"),
                [CALLER_ID.to_string()]
            );
            assert_eq!(fixtures.set_members("minqlx:ips"), ["1.1.1.1"]);
            assert_eq!(
                fixtures.set_members("minqlx:ips:1.1.1.1"),
                [CALLER_ID.to_string()]
            );
            assert_eq!(
                fixtures.set_members(&format!("{}:ips", player_key(CALLER_ID))),
                ["1.1.1.1"]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn players_without_an_ip_are_only_added_to_the_player_list(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::handle_player_connect(&fixtures.plugin, &fixtures.target);

            assert!(result.is_ok());
            assert_eq!(fixtures.names(TARGET_ID), ["Eve^7"]);
            assert!(fixtures.set_members("minqlx:ips").is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn known_players_only_get_new_names_remembered(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            Essentials::handle_player_connect(&fixtures.plugin, &fixtures.target)
                .expect("first connect failed");
            Essentials::handle_player_connect(&fixtures.plugin, &fixtures.target)
                .expect("second connect failed");
            assert_eq!(fixtures.names(TARGET_ID), ["Eve^7"]);

            fixtures
                .target
                .setattr("name", "^1Eve2^7")
                .and_then(|_| fixtures.target.setattr("clean_name", "Eve2"))
                .expect("could not rename");
            let result = Essentials::handle_player_connect(&fixtures.plugin, &fixtures.target);

            assert!(result.is_ok());
            assert_eq!(fixtures.names(TARGET_ID), ["^1Eve2^7", "Eve^7"]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn disconnecting_players_are_remembered_and_stamped(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let py = fixtures.plugin.py();

            let result = Essentials::handle_player_disconnect(
                &fixtures.plugin,
                &fixtures.target,
                py.None().bind(py),
            );

            assert!(result.is_ok());
            assert_eq!(fixtures.plugin.get().recent_disconnects.lock().len(), 1);
            let stamp = fixtures
                .stored(&format!("{}:last_seen", player_key(TARGET_ID)))
                .expect("no last seen");
            assert_eq!(stamp.len(), "2026-10-27 12:00:00".len());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn only_the_ten_latest_disconnects_are_kept(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let py = fixtures.plugin.py();
            for _ in 0..12 {
                Essentials::handle_player_disconnect(
                    &fixtures.plugin,
                    &fixtures.target,
                    py.None().bind(py),
                )
                .expect("disconnect failed");
            }

            assert_eq!(fixtures.plugin.get().recent_disconnects.lock().len(), 10);
        });
    }

    #[rstest]
    #[case("8", "The team size is larger than what the server allows.")]
    #[case("1", "The team size is smaller than what the server allows.")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn teamsize_votes_beyond_the_limits_are_stopped(
        _pyshinqlx_setup: (),
        #[case] size: &str,
        #[case] expected: &str,
    ) {
        with_essentials(&VOTE_CVARS, None, |fixtures| {
            let result = Essentials::handle_vote_called(
                &fixtures.plugin,
                &fixtures.caller,
                "TeamSize",
                size,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(fixtures.told(&fixtures.caller), [expected]);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn teamsize_votes_that_are_no_numbers_are_ignored(_pyshinqlx_setup: ()) {
        with_essentials(&VOTE_CVARS, None, |fixtures| {
            let result =
                Essentials::handle_vote_called(&fixtures.plugin, &fixtures.caller, "teamsize", "x");

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.plugin.get().vote_count.load(Ordering::SeqCst), 0);
        });
    }

    #[rstest]
    #[case("", &["Available maps and factories:", "  campgrounds               Factories: ca, ffa\n  overkill                  Factories: ca"])]
    #[case("unknown", &["This map is not allowed. Use !mappool to see available options."])]
    #[case("overkill ffa", &["This factory is not allowed on that map. Use !mappool to see available options."])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn map_votes_outside_of_the_mappool_are_stopped(
        _pyshinqlx_setup: (),
        #[case] args: &str,
        #[case] expected: &[&str],
    ) {
        with_essentials(&VOTE_CVARS, Some(MAPPOOL), |fixtures| {
            let result =
                Essentials::handle_vote_called(&fixtures.plugin, &fixtures.caller, "map", args);

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(fixtures.told(&fixtures.caller), expected);
        });
    }

    #[rstest]
    #[case("Campgrounds ffa")]
    #[case("overkill")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn map_votes_inside_of_the_mappool_pass_on(_pyshinqlx_setup: (), #[case] args: &str) {
        with_essentials(&NO_VOTEPASS, Some(MAPPOOL), |fixtures| {
            let result =
                Essentials::handle_vote_called(&fixtures.plugin, &fixtures.caller, "map", args);

            assert_eq!(result.ok(), Some(None));
            assert!(fixtures.told(&fixtures.caller).is_empty());
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn votes_are_passed_automatically_after_the_delay(_pyshinqlx_setup: ()) {
        with_essentials(&VOTE_CVARS, None, |fixtures| {
            let py = fixtures.plugin.py();
            fixtures.set_vote(true, Some((2, 0)));
            let scheduler = py
                .import("shinqlx")
                .and_then(|shinqlx| shinqlx.getattr("frame_tasks"))
                .expect("no frame tasks");
            let before = scheduler
                .getattr("queue")
                .and_then(|queue| queue.len())
                .expect("could not count");

            let result =
                Essentials::handle_vote_called(&fixtures.plugin, &fixtures.caller, "kick", "Eve");

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.plugin.get().last_vote.load(Ordering::SeqCst), 0);
            assert_eq!(fixtures.plugin.get().vote_count.load(Ordering::SeqCst), 1);
            let queue = scheduler.getattr("queue").expect("no queue");
            assert_eq!(queue.len().expect("no length"), before + 1);
            let task = queue.get_item(before).expect("no task");
            let delay = task.getattr("time").and_then(|time| time.extract::<f64>());
            assert!(delay.is_ok());
            let arguments = task.getattr("argument").expect("no arguments");
            task.getattr("action")
                .and_then(|action| {
                    action.call(
                        arguments
                            .cast::<pyo3::types::PyTuple>()
                            .expect("not a tuple"),
                        None,
                    )
                })
                .expect("could not run the task");
            scheduler
                .call_method1("cancel", (task,))
                .expect("could not cancel the task");
            assert_eq!(fixtures.forced(), 1);
        });
    }

    #[rstest]
    #[case(false, Some((3, 0)), 0.5, false)]
    #[case(true, None, 0.5, false)]
    #[case(true, Some((1, 1)), 0.5, false)]
    #[case(true, Some((0, 2)), 0.5, false)]
    #[case(true, Some((1, 0)), 0.6, false)]
    #[case(true, Some((2, 0)), 0.5, true)]
    #[case(true, Some((1, 0)), 0.0, true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn a_vote_is_only_forced_when_it_is_leading_and_enough_players_took_part(
        _pyshinqlx_setup: (),
        #[case] active: bool,
        #[case] counts: Option<(i64, i64)>,
        #[case] require: f64,
        #[case] forced: bool,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            fixtures.set_vote(active, counts);

            let result = Essentials::force(&fixtures.plugin, require, 0);

            assert!(result.is_ok());
            assert_eq!(fixtures.forced(), usize::from(forced));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn an_outdated_vote_is_not_forced(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            fixtures.set_vote(true, Some((2, 0)));
            fixtures.plugin.get().last_vote.store(5, Ordering::SeqCst);

            let result = Essentials::force(&fixtures.plugin, 0.0, 4);

            assert!(result.is_ok());
            assert_eq!(fixtures.forced(), 0);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn nobody_in_the_teams_means_no_forced_vote(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            fixtures.set_vote(true, Some((2, 0)));
            fixtures
                .module
                .getattr("team_members")
                .and_then(|members| {
                    members.set_item("red", Vec::<String>::new())?;
                    members.set_item("blue", Vec::<String>::new())
                })
                .expect("could not empty the teams");

            let result = Essentials::force(&fixtures.plugin, 0.5, 0);

            assert!(result.is_ok());
            assert_eq!(fixtures.forced(), 0);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn the_latest_commands_are_recorded_and_listed_without_the_current_one(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let py = fixtures.plugin.py();
            let command = py.None();
            for args in ["first", "second", "listing"] {
                Essentials::handle_command(
                    &fixtures.plugin,
                    &fixtures.caller,
                    command.bind(py),
                    &pyo3::types::PyString::new(py, args),
                )
                .expect("could not record the command");
            }

            let result = Essentials::cmd_commands(
                &fixtures.plugin,
                &fixtures.target,
                msg(&["commands"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.target),
                [
                    "The most recent ^62^7 commands executed:",
                    "  Caller^7 executed: second",
                    "  Caller^7 executed: first"
                ]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn only_eleven_commands_are_recorded(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let py = fixtures.plugin.py();
            let command = py.None();
            for _ in 0..15 {
                Essentials::handle_command(
                    &fixtures.plugin,
                    &fixtures.caller,
                    command.bind(py),
                    &pyo3::types::PyString::new(py, "x"),
                )
                .expect("could not record the command");
            }

            assert_eq!(fixtures.plugin.get().recent_commands.lock().len(), 11);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn no_recorded_commands_are_reported(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_commands(
                &fixtures.plugin,
                &fixtures.target,
                msg(&["commands"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.target),
                ["No commands have been recorded yet."]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn no_disconnects_are_reported(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_disconnects(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["dcs"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.caller),
                ["No players have disconnected yet."]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn recent_disconnects_are_listed(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let py = fixtures.plugin.py();
            Essentials::handle_player_disconnect(
                &fixtures.plugin,
                &fixtures.target,
                py.None().bind(py),
            )
            .expect("disconnect failed");

            let result = Essentials::cmd_disconnects(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["dcs"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.caller),
                [
                    "The most recent ^61^7 player disconnects:".to_owned(),
                    format!("  Eve^7 ({TARGET_ID}): ^60^7 seconds ago")
                ]
            );
        });
    }

    #[rstest]
    #[case(&["id"], &["All connected players:", "  1^6:^7 Caller^7\n  3^6:^7 Eve^7"])]
    #[case(&["id", "eve"], &["A total of ^61^7 players matched:", "  3^6:^7 Eve^7"])]
    #[case(&["id", "e", "call"], &["A total of ^62^7 players matched:", "  1^6:^7 Caller^7\n  3^6:^7 Eve^7"])]
    #[case(&["id", "zzz"], &["Sorry, but no players matched your tokens."])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn id_lists_the_players_matching_the_tokens(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] expected: &[&str],
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_id(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(fixtures.told(&fixtures.caller), expected);
        });
    }

    #[rstest]
    #[case("cmd_id")]
    #[case("cmd_players")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn id_and_players_know_when_nobody_is_connected(_pyshinqlx_setup: (), #[case] command: &str) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            fixtures
                .module
                .getattr("online")
                .and_then(|online| online.call_method0("clear"))
                .expect("could not empty the player list");
            let handler = if command == "cmd_id" {
                Essentials::cmd_id
            } else {
                Essentials::cmd_players
            };

            let result = handler(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["id"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.caller),
                ["There are no players connected at the moment."]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn players_prints_a_table_like_the_console_command(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_players(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["players"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.caller),
                [concat!(
                    "ID |     SteamID64     |   IP Address    | Name\n",
                    " 1 | 76561198000000001 | 1.1.1.1         | Caller^7\n",
                    " 3 | 76561198000000003 |                 | Eve^7\n"
                )]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn db_without_a_key_asks_for_usage(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_db(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["db"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(USAGE)));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn db_reports_missing_keys(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_db(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["db", "nothing"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(
                fixtures.replies(),
                ["The key is not present in the database."]
            );
        });
    }

    #[rstest]
    #[case("string", "value")]
    #[case("list", "['b', 'a']")]
    #[case("set", "{'x'}")]
    #[case("zset", "[('m', 2.0)]")]
    #[case("hash", "{'k': 'v'}")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn db_prints_the_value_depending_on_its_type(
        _pyshinqlx_setup: (),
        #[case] kind: &str,
        #[case] expected: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let py = fixtures.plugin.py();
            let store = |attribute: &str, value: Bound<'_, PyAny>| {
                fixtures
                    .database
                    .getattr(attribute)
                    .and_then(|items| items.set_item("key", value))
                    .expect("could not store");
            };
            match kind {
                "string" => store(
                    "strings",
                    "value".into_pyobject(py).expect("str").into_any(),
                ),
                "list" => store(
                    "lists",
                    vec!["b", "a"].into_pyobject(py).expect("list").into_any(),
                ),
                "set" => store(
                    "sets",
                    pyo3::types::PySet::new(py, ["x"]).expect("set").into_any(),
                ),
                "zset" => {
                    let scores = PyDict::new(py);
                    scores.set_item("m", 2.0).expect("score");
                    store("zsets", scores.into_any());
                }
                _ => {
                    let hash = PyDict::new(py);
                    hash.set_item("k", "v").expect("field");
                    store("hashes", hash.into_any());
                }
            }

            let result = Essentials::cmd_db(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["db", "key"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), [expected]);
        });
    }

    #[rstest]
    #[case(&["seen"], Some(USAGE), &[])]
    #[case(&["seen", "abc"], None, &["Unintelligible SteamID64."])]
    #[case(&["seen", "12"], None, &["Invalid SteamID64."])]
    #[case(&["seen", "76561198000000003"], None, &["That would be Eve^7^7, who is currently on this very server!"])]
    #[case(&["seen", "76561198000000042"], None, &["^7I have never seen that player before."])]
    #[case(&["seen", "76561198000000009"], None, &["^7I have never seen my ^6master^7 before."])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn seen_reports_where_a_steam_id_was_last_seen(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] expected_code: Option<i32>,
        #[case] expected: &[&str],
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_seen(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(expected_code));
            assert_eq!(fixtures.replies(), expected);
        });
    }

    #[rstest]
    #[case("1970-01-01 00:00:00", false)]
    #[case("", true)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn seen_tells_how_long_ago_a_player_was_seen(
        _pyshinqlx_setup: (),
        #[case] stamp: &str,
        #[case] recent: bool,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let py = fixtures.plugin.py();
            let stamp = if recent {
                crate::ffi::python::native_plugins::timed_entries::local_time(
                    py,
                    crate::ffi::python::native_plugins::timed_entries::now() - 3_900.0,
                )
                .expect("no time")
            } else {
                stamp.to_owned()
            };
            fixtures.store(
                &format!("{}:last_seen", player_key(76_561_198_000_000_042)),
                &stamp,
            );

            let result = Essentials::cmd_seen(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["seen", "76561198000000042"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            let replies = fixtures.replies();
            assert_eq!(replies.len(), 1);
            if recent {
                assert_eq!(
                    replies[0],
                    "^7I saw that player ^61^7 hour(s) and ^65^7 minute(s) ago."
                );
            } else {
                assert!(replies[0].contains("day(s)"));
            }
        });
    }

    #[rstest]
    #[case(&["time", "x"], "Unintelligible time zone offset.")]
    #[case(&["time", "24"], "Unintelligible time zone offset.")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn time_rejects_unintelligible_offsets(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] expected: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_time(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), [expected]);
        });
    }

    #[rstest]
    #[case(&["time", "0"], "UTC")]
    #[case(&["time", "5"], "UTC+5")]
    #[case(&["time", "-3"], "UTC-3")]
    #[case(&["time"], "UTC")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn time_reports_the_time_in_the_requested_zone(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] zone: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_time(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            let replies = fixtures.replies();
            assert_eq!(replies.len(), 1);
            assert!(replies[0].starts_with("The current time is: ^6"));
            if words.len() > 1 {
                assert!(replies[0].ends_with(zone));
            } else {
                assert!(replies[0].contains(" UTC"));
            }
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn help_tells_about_the_version(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_help(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["help"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            let told = fixtures.told(&fixtures.caller);
            assert_eq!(told.len(), 2);
            assert!(told[0].starts_with("shinqlx: ^6"));
            assert!(told[1].contains("github.com/mgaertne/shinqlx"));
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn rcon_without_a_command_asks_for_usage(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_rcon(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["rcon"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(USAGE)));
        });
    }

    #[rstest]
    #[case("status", true)]
    #[case("fail", false)]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn rcon_runs_the_command_with_the_output_redirected(
        _pyshinqlx_setup: (),
        #[case] command: &str,
        #[case] succeeds: bool,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_rcon(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["rcon", command]),
                &fixtures.channel,
            );

            assert_eq!(result.is_ok(), succeeds);
            assert_eq!(fixtures.strings("console"), [command]);
            assert_eq!(
                fixtures.strings("redirected"),
                ["created", "entered", "left"]
            );
        });
    }

    #[rstest]
    #[case(None, "No map pool is being enforced. You are free to vote any map.")]
    #[case(Some(MAPPOOL), "  campgrounds")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mappool_lists_the_enforced_pool(
        _pyshinqlx_setup: (),
        #[case] mappool: Option<&str>,
        #[case] expected: &str,
    ) {
        with_essentials(&VOTE_CVARS, mappool, |fixtures| {
            let result = Essentials::cmd_mappool(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["mappool"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            let told = fixtures.told(&fixtures.caller);
            assert_eq!(told.len(), 1);
            if mappool.is_some() {
                assert!(told[0].starts_with("campgrounds"));
            } else {
                assert_eq!(told[0], expected);
            }
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn mappool_is_not_listed_when_it_is_not_enforced(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS_UNENFORCED, Some(MAPPOOL), |fixtures| {
            let result = Essentials::cmd_mappool(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["mappool"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.caller),
                ["No map pool is being enforced. You are free to vote any map."]
            );
        });
    }

    const NO_VOTEPASS_UNENFORCED: [(&str, &CStr); 6] = [
        ("qlx_votepass", c"0"),
        ("qlx_votepassThreshold", c"0.5"),
        ("qlx_teamsizeMinimum", c"2"),
        ("qlx_teamsizeMaximum", c"6"),
        ("qlx_enforceMappool", c"0"),
        ("qlx_commandPrefix", c"!"),
    ];

    type Handler = fn(
        &Bound<'_, Essentials>,
        &Bound<'_, PyAny>,
        Vec<String>,
        &Bound<'_, PyAny>,
    ) -> PyResult<Option<i32>>;

    fn handler(name: &str) -> Handler {
        match name {
            "kick" => Essentials::cmd_kick,
            "kickban" => Essentials::cmd_kickban,
            "red" => Essentials::cmd_red,
            "blue" => Essentials::cmd_blue,
            "spectate" => Essentials::cmd_spectate,
            "free" => Essentials::cmd_free,
            "addmod" => Essentials::cmd_addmod,
            "addadmin" => Essentials::cmd_addadmin,
            "demote" => Essentials::cmd_demote,
            "mute" => Essentials::cmd_mute,
            "unmute" => Essentials::cmd_unmute,
            "slap" => Essentials::cmd_slap,
            "slay" => Essentials::cmd_slay,
            "sound" => Essentials::cmd_sound,
            "music" => Essentials::cmd_music,
            "random" => Essentials::cmd_random,
            "switch" => Essentials::cmd_switch,
            "map" => Essentials::cmd_map,
            "teamsize" => Essentials::cmd_teamsize,
            other => panic!("no handler for {other}"),
        }
    }

    impl Fixtures<'_> {
        fn run(
            &self,
            name: &str,
            caller: &Bound<'_, PyAny>,
            words: &[&str],
        ) -> Option<Option<i32>> {
            handler(name)(&self.plugin, caller, msg(words), &self.channel).ok()
        }

        fn actions(&self) -> String {
            self.module
                .getattr("actions")
                .and_then(|actions| actions.repr())
                .map(|repr| repr.to_string())
                .expect("could not read the actions")
        }

        fn set_game(&self, state: Option<&str>) {
            let holder = self.module.getattr("game_holder").expect("no game holder");
            match state {
                Some(state) => holder
                    .get_item(0)
                    .and_then(|game| game.setattr("state", state)),
                None => holder.set_item(0, self.module.py().None()),
            }
            .expect("could not set the game");
        }
    }

    #[rstest]
    #[case("kick", &["kick", "3", "bad", "manners"], "[('kick', 3, 'bad manners')]")]
    #[case("kick", &["kick", "3"], "[('kick', 3, '')]")]
    #[case("kickban", &["kickban", "3"], "[('tempban', 3)]")]
    #[case("red", &["red", "3"], "[('put', 3, 'red')]")]
    #[case("blue", &["blue", "3"], "[('put', 3, 'blue')]")]
    #[case("spectate", &["spec", "3"], "[('put', 3, 'spectator')]")]
    #[case("free", &["free", "3"], "[('put', 3, 'free')]")]
    #[case("addmod", &["addmod", "3"], "[('addmod', 3)]")]
    #[case("addadmin", &["addadmin", "3"], "[('addadmin', 3)]")]
    #[case("demote", &["demote", "3"], "[('demote', 3)]")]
    #[case("mute", &["mute", "3"], "[('mute', 3)]")]
    #[case("unmute", &["unmute", "3"], "[('unmute', 3)]")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_commands_act_on_the_resolved_player(
        _pyshinqlx_setup: (),
        #[case] command: &str,
        #[case] words: &[&str],
        #[case] expected: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run(command, &fixtures.caller, words);

            assert_eq!(result, Some(None));
            assert_eq!(fixtures.actions(), expected);
        });
    }

    #[rstest]
    #[case("kick")]
    #[case("kickban")]
    #[case("red")]
    #[case("blue")]
    #[case("spectate")]
    #[case("free")]
    #[case("addmod")]
    #[case("addadmin")]
    #[case("demote")]
    #[case("mute")]
    #[case("unmute")]
    #[case("slap")]
    #[case("slay")]
    #[case("sound")]
    #[case("music")]
    #[case("random")]
    #[case("switch")]
    #[case("map")]
    #[case("teamsize")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn commands_without_arguments_ask_for_usage(_pyshinqlx_setup: (), #[case] command: &str) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run(command, &fixtures.caller, &[command]);

            assert_eq!(result, Some(Some(USAGE)));
            assert_eq!(fixtures.actions(), "[]");
        });
    }

    #[rstest]
    #[case("7")]
    #[case("76561198000000042")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn player_commands_reject_unknown_ids(_pyshinqlx_setup: (), #[case] ident: &str) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run("kick", &fixtures.caller, &["kick", ident]);

            assert_eq!(result, Some(None));
            assert_eq!(fixtures.replies(), ["Invalid ID."]);
            assert_eq!(fixtures.actions(), "[]");
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn muting_oneself_is_refused(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run("mute", &fixtures.caller, &["mute", "1"]);

            assert_eq!(result, Some(None));
            assert_eq!(fixtures.replies(), ["I refuse."]);
            assert_eq!(fixtures.actions(), "[]");
        });
    }

    #[rstest]
    #[case(&["slap", "3"], "[('slap', 3, 0)]")]
    #[case(&["slap", "3", "-5"], "[('slap', 3, -5)]")]
    #[case(&["slay", "3"], "[('slay', 3)]")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slap_and_slay_stop_all_processing(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] expected: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run(words[0], &fixtures.caller, words);

            assert_eq!(result, Some(Some(STOP_ALL)));
            assert_eq!(fixtures.actions(), expected);
        });
    }

    #[rstest]
    #[case(&["slap", "7"], "Invalid ID.")]
    #[case(&["slay", "7"], "Invalid ID.")]
    #[case(&["slap", "3", "lots"], "Invalid damage value.")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn slap_and_slay_tell_the_caller_about_invalid_values(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] expected: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run(words[0], &fixtures.caller, words);

            assert_eq!(result, Some(Some(STOP_ALL)));
            assert_eq!(fixtures.told(&fixtures.caller), [expected]);
            assert_eq!(fixtures.actions(), "[]");
        });
    }

    #[rstest]
    #[case("cmd_shuffle", "[('shuffle',)]")]
    #[case("cmd_pause", "[('pause',)]")]
    #[case("cmd_unpause", "[('unpause',)]")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn game_control_commands_call_the_plugin(
        _pyshinqlx_setup: (),
        #[case] command: &str,
        #[case] expected: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let handler = match command {
                "cmd_shuffle" => Essentials::cmd_shuffle,
                "cmd_pause" => Essentials::cmd_pause,
                _ => Essentials::cmd_unpause,
            };

            let result = handler(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["x"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.actions(), expected);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn sounds_toggles_the_flag_of_the_player(_pyshinqlx_setup: ()) {
        with_essentials(&VOTE_CVARS, None, |fixtures| {
            let toggle = || {
                Essentials::cmd_enable_sounds(
                    &fixtures.plugin,
                    &fixtures.caller,
                    msg(&["sounds"]),
                    &fixtures.channel,
                )
            };

            assert_eq!(toggle().ok(), Some(Some(STOP_ALL)));
            assert_eq!(toggle().ok(), Some(Some(STOP_ALL)));

            assert_eq!(
                fixtures.told(&fixtures.caller),
                [
                    "Sounds have been disabled. Use ^6!sounds^7 to enable them again.",
                    "Sounds have been enabled. Use ^6!sounds^7 to disable them again."
                ]
            );
        });
    }

    #[rstest]
    #[case("sound", "play_sound")]
    #[case("music", "play_music")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn sounds_are_played_for_everybody_who_enabled_them(
        _pyshinqlx_setup: (),
        #[case] command: &str,
        #[case] play: &str,
    ) {
        with_essentials(&VOTE_CVARS, None, |fixtures| {
            let result = fixtures.run(command, &fixtures.caller, &[command, "boom"]);

            assert_eq!(result, Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.actions(),
                format!("[('{play}', 'boom', 1), ('{play}', 'boom', 3)]")
            );
        });
    }

    #[rstest]
    #[case("sound")]
    #[case("music")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn sounds_skip_players_who_disabled_them(_pyshinqlx_setup: (), #[case] command: &str) {
        with_essentials(&VOTE_CVARS, None, |fixtures| {
            fixtures
                .database
                .call_method1("set_flag", (&fixtures.target, SOUNDS_FLAG, false))
                .expect("could not set the flag");

            let result = fixtures.run(command, &fixtures.caller, &[command, "boom"]);

            assert_eq!(result, Some(Some(STOP_ALL)));
            assert_eq!(fixtures.actions().matches("boom").count(), 1);
        });
    }

    #[rstest]
    #[case("sound")]
    #[case("music")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn sounds_are_refused_for_players_who_disabled_them(
        _pyshinqlx_setup: (),
        #[case] command: &str,
    ) {
        with_essentials(&VOTE_CVARS, None, |fixtures| {
            fixtures
                .database
                .call_method1("set_flag", (&fixtures.caller, SOUNDS_FLAG, false))
                .expect("could not set the flag");

            let result = fixtures.run(command, &fixtures.caller, &[command, "boom"]);

            assert_eq!(result, Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.caller),
                ["Sounds are disabled. Use ^6!sounds^7 to enable them again."]
            );
            assert_eq!(fixtures.actions(), "[]");
        });
    }

    #[rstest]
    #[case("sound")]
    #[case("music")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn invalid_sounds_are_reported_only_to_the_caller(_pyshinqlx_setup: (), #[case] command: &str) {
        with_essentials(&VOTE_CVARS, None, |fixtures| {
            let result = fixtures.run(command, &fixtures.caller, &[command, "bad"]);

            assert_eq!(result, Some(Some(STOP_ALL)));
            assert_eq!(fixtures.told(&fixtures.caller), ["Invalid sound."]);
            assert_eq!(fixtures.actions().matches("bad").count(), 1);
        });
    }

    #[rstest]
    #[case(true, "stopsound", "[('stop_sound',)]")]
    #[case(true, "stopmusic", "[('stop_music',)]")]
    #[case(false, "stopsound", "[]")]
    #[case(false, "stopmusic", "[]")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn stopping_sounds_respects_the_flag(
        _pyshinqlx_setup: (),
        #[case] enabled: bool,
        #[case] command: &str,
        #[case] expected: &str,
    ) {
        with_essentials(&VOTE_CVARS, None, |fixtures| {
            fixtures
                .database
                .call_method1("set_flag", (&fixtures.caller, SOUNDS_FLAG, enabled))
                .expect("could not set the flag");
            let handler = if command == "stopsound" {
                Essentials::cmd_stopsound
            } else {
                Essentials::cmd_stopmusic
            };

            let result = handler(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&[command]),
                &fixtures.channel,
            );

            assert_eq!(
                result.ok(),
                Some(if enabled { None } else { Some(STOP_ALL) })
            );
            assert_eq!(fixtures.actions(), expected);
        });
    }

    #[rstest]
    #[case(true, true, &[])]
    #[case(true, false, &[])]
    #[case(false, true, &["There is no active vote!"])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn yes_and_no_decide_the_active_vote(
        _pyshinqlx_setup: (),
        #[case] active: bool,
        #[case] yes: bool,
        #[case] expected_replies: &[&str],
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            fixtures.set_vote(active, None);
            let handler = if yes {
                Essentials::cmd_yes
            } else {
                Essentials::cmd_no
            };

            let result = handler(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["x"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.replies(), expected_replies);
            assert_eq!(fixtures.forced(), usize::from(active));
        });
    }

    #[rstest]
    #[case("0")]
    #[case("-3")]
    #[case("many")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn random_needs_a_positive_limit(_pyshinqlx_setup: (), #[case] limit: &str) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run("random", &fixtures.caller, &["random", limit]);

            assert_eq!(result, Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.told(&fixtures.caller),
                ["Invalid upper limit. Use a positive integer."]
            );
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn random_announces_a_number_up_to_the_limit(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run("random", &fixtures.caller, &["random", "1"]);

            assert_eq!(result, Some(None));
            assert_eq!(fixtures.replies(), ["^3Random number is: ^51"]);
        });
    }

    #[rstest]
    #[case(&["switch", "1"], Some(USAGE), &[], "[]")]
    #[case(&["switch", "7", "3"], None, &["The first ID is invalid."], "[]")]
    #[case(&["switch", "1", "7"], None, &["The second ID is invalid."], "[]")]
    #[case(&["switch", "1", "3"], None, &[], "[('switch', 1, 3)]")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn switch_swaps_the_teams_of_two_players(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] expected_code: Option<i32>,
        #[case] expected_replies: &[&str],
        #[case] expected_actions: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run("switch", &fixtures.caller, words);

            assert_eq!(result, Some(expected_code));
            assert_eq!(fixtures.replies(), expected_replies);
            assert_eq!(fixtures.actions(), expected_actions);
        });
    }

    #[rstest]
    #[case("cmd_lock", &["lock"], "[('lock', None)]")]
    #[case("cmd_lock", &["lock", "red"], "[('lock', 'red')]")]
    #[case("cmd_lock", &["lock", "Blue"], "[('lock', 'blue')]")]
    #[case("cmd_unlock", &["unlock", "s"], "[('unlock', 'spectator')]")]
    #[case("cmd_unlock", &["unlock"], "[('unlock', None)]")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn teams_are_locked_and_unlocked_by_their_initial(
        _pyshinqlx_setup: (),
        #[case] command: &str,
        #[case] words: &[&str],
        #[case] expected: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let handler = if command == "cmd_lock" {
                Essentials::cmd_lock
            } else {
                Essentials::cmd_unlock
            };

            let result = handler(
                &fixtures.plugin,
                &fixtures.caller,
                msg(words),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.actions(), expected);
        });
    }

    #[rstest]
    #[case("x")]
    #[case("")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn locking_an_unknown_team_is_refused(_pyshinqlx_setup: (), #[case] team: &str) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = Essentials::cmd_lock(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["lock", team]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(Some(STOP_ALL)));
            assert_eq!(fixtures.told(&fixtures.caller), ["Invalid team."]);
            assert_eq!(fixtures.actions(), "[]");
        });
    }

    #[rstest]
    #[case(Some("warmup"), "[('allready',)]", &[])]
    #[case(Some("in_progress"), "[]", &["But the game's already in progress, you silly goose!"])]
    #[case(None, "[]", &["There is no game running."])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn allready_only_works_during_warmup(
        _pyshinqlx_setup: (),
        #[case] state: Option<&str>,
        #[case] expected_actions: &str,
        #[case] expected_replies: &[&str],
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            fixtures.set_game(state);

            let result = Essentials::cmd_allready(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["allready"]),
                &fixtures.channel,
            );

            assert_eq!(result.as_ref().ok(), Some(&None), "{result:?}");
            assert_eq!(fixtures.actions(), expected_actions);
            assert_eq!(fixtures.replies(), expected_replies);
        });
    }

    #[rstest]
    #[case(Some("in_progress"), "[('abort',)]", &[])]
    #[case(Some("warmup"), "[]", &["But the game isn't even on, you doofus!"])]
    #[case(None, "[]", &["There is no game running."])]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn abort_only_works_during_a_game(
        _pyshinqlx_setup: (),
        #[case] state: Option<&str>,
        #[case] expected_actions: &str,
        #[case] expected_replies: &[&str],
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            fixtures.set_game(state);

            let result = Essentials::cmd_abort(
                &fixtures.plugin,
                &fixtures.caller,
                msg(&["abort"]),
                &fixtures.channel,
            );

            assert_eq!(result.ok(), Some(None));
            assert_eq!(fixtures.actions(), expected_actions);
            assert_eq!(fixtures.replies(), expected_replies);
        });
    }

    #[rstest]
    #[case(&["map", "campgrounds"], "[('change_map', 'campgrounds', None)]")]
    #[case(&["map", "campgrounds", "ffa"], "[('change_map', 'campgrounds', 'ffa')]")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn map_changes_the_map_with_an_optional_factory(
        _pyshinqlx_setup: (),
        #[case] words: &[&str],
        #[case] expected: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run("map", &fixtures.caller, words);

            assert_eq!(result, Some(None));
            assert_eq!(fixtures.actions(), expected);
        });
    }

    #[rstest]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn teamsize_sets_the_size_and_announces_it(_pyshinqlx_setup: ()) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            let result = fixtures.run("teamsize", &fixtures.caller, &["ts", "6"]);

            assert_eq!(result, Some(Some(STOP_ALL)));
            assert_eq!(
                fixtures.actions(),
                "[('msg', 'The teamsize has been set to ^66^7 by Caller^7.')]"
            );
            let size: i64 = fixtures
                .module
                .getattr("game_holder")
                .and_then(|holder| holder.get_item(0))
                .and_then(|game| game.getattr("teamsize"))
                .and_then(|size| size.extract())
                .expect("no teamsize");
            assert_eq!(size, 6);
        });
    }

    #[rstest]
    #[case(Some("warmup"), "^7Unintelligible size.", "x")]
    #[case(None, "There is no game running.", "4")]
    #[cfg_attr(miri, ignore)]
    #[serial]
    fn teamsize_reports_problems(
        _pyshinqlx_setup: (),
        #[case] state: Option<&str>,
        #[case] expected: &str,
        #[case] size: &str,
    ) {
        with_essentials(&NO_VOTEPASS, None, |fixtures| {
            fixtures.set_game(state);

            let result = fixtures.run("teamsize", &fixtures.caller, &["ts", size]);

            assert_eq!(result, Some(None));
            assert_eq!(fixtures.replies(), [expected]);
            assert_eq!(fixtures.actions(), "[]");
        });
    }
}
