//! Records the matches of the running server as its cvars ask.

use core::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use std::{sync::LazyLock, time::Instant};

use parking_lot::Mutex;
use ql_sqx::Event;

use super::{
    catalog::{catalog, mark_game_thread},
    disk, engine_state,
    lifecycle::{GameState, Lifecycle, Step},
    session::Session,
    settings::Settings,
};
use crate::{QuakeLiveEngine, python::demo_finished_dispatcher};

const SETTINGS_INTERVAL: Duration = Duration::from_secs(1);
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// Whether a session exists, so that the hooks cost next to nothing otherwise.
static RECORDING: AtomicBool = AtomicBool::new(false);
/// Configstrings are only complete once the map is loaded.
static MAP_LOADED: AtomicBool = AtomicBool::new(false);
static RECORDER: LazyLock<Mutex<Recorder>> = LazyLock::new(Mutex::default);

#[derive(Default)]
struct Recorder {
    lifecycle: Lifecycle,
    session: Option<Session>,
    settings: Option<(Instant, Settings)>,
    swept: Option<Instant>,
}

impl Recorder {
    fn frame(&mut self, main_engine: &QuakeLiveEngine, server_time: i32) {
        let started = Instant::now();
        let settings = self.settings(main_engine, started);
        if disk::take_sweep_request()
            || self
                .swept
                .is_none_or(|time| started.duration_since(time) >= SWEEP_INTERVAL)
        {
            disk::start_sweep(&settings);
            self.swept = Some(started);
        }
        catalog().configure(
            &settings.directory,
            settings.rules.enabled,
            disk::has_room(),
        );
        if !settings.rules.enabled && self.session.is_none() {
            return;
        }

        let observation = engine_state::observe(main_engine, disk::has_room());
        catalog().set_countdown(observation.game_state == Some(GameState::Countdown));
        match self.lifecycle.observe(&settings.rules, &observation) {
            Some(Step::Start) => self.start(main_engine, &settings),
            Some(Step::Finish(reason)) => self.end(&reason.to_string(), false),
            Some(Step::Split(reason)) => {
                self.end(&reason.to_string(), false);
                self.start(main_engine, &settings);
            }
            None => {}
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.note(
            started,
            observation.game_state == Some(GameState::InProgress),
            || engine_state::players(main_engine),
        );

        let capture_started = Instant::now();
        let frame = engine_state::capture(main_engine, server_time);
        self.send(Event::Frame(frame));
        if let Some(session) = self.session.as_mut() {
            session.count_frame(capture_started.elapsed());
        }
    }

    fn settings(&mut self, main_engine: &QuakeLiveEngine, now: Instant) -> Settings {
        match &self.settings {
            Some((read, settings)) if now.duration_since(*read) < SETTINGS_INTERVAL => {
                settings.clone()
            }
            _ => {
                let settings = Settings::from_cvars(|name| engine_state::cvar(main_engine, name));
                self.settings = Some((now, settings.clone()));
                settings
            }
        }
    }

    fn start(&mut self, main_engine: &QuakeLiveEngine, settings: &Settings) {
        match Session::start(main_engine, settings) {
            Ok(session) => {
                self.session = Some(session);
                RECORDING.store(true, Ordering::Release);
            }
            Err(error) => {
                log::error!(target: "shinqlx", func = "demo"; "Could not record: {error}");
                self.lifecycle.failed(Instant::now());
            }
        }
    }

    fn end(&mut self, reason: &str, wait_for_file: bool) {
        RECORDING.store(false, Ordering::Release);
        if let Some(session) = self.session.take() {
            session.end(reason, wait_for_file);
        }
    }

    fn send(&mut self, event: Event) {
        let delivered = self
            .session
            .as_ref()
            .is_none_or(|session| session.send(event));
        if !delivered {
            self.give_up();
        }
    }

    fn give_up(&mut self) {
        self.end("the writer cannot keep up", false);
        self.lifecycle.failed(Instant::now());
    }
}

fn send(event: impl FnOnce() -> Event) {
    if RECORDING.load(Ordering::Acquire) {
        RECORDER.lock().send(event());
    }
}

/// To be called at the end of every game frame.
pub(crate) fn frame(main_engine: &QuakeLiveEngine, server_time: i32) {
    mark_game_thread();
    if MAP_LOADED.load(Ordering::Acquire) {
        RECORDER.lock().frame(main_engine, server_time);
    }
    let finished = catalog().take_finished();
    for recording in &finished {
        demo_finished_dispatcher(recording);
    }
}

pub(crate) fn map_loaded() {
    mark_game_thread();
    MAP_LOADED.store(true, Ordering::Release);
    disk::request_sweep();
}

/// A map change or the shutdown of the server ends the recording.
pub(crate) fn map_ended() {
    MAP_LOADED.store(false, Ordering::Release);
    let mut recorder = RECORDER.lock();
    recorder.end("the map ended", true);
    recorder.lifecycle.ended();
}

pub(crate) fn server_command(recipient: Option<i32>, command: &str) {
    let recipient = match recipient.map(u8::try_from) {
        None => None,
        Some(Ok(client)) => Some(client),
        Some(Err(_)) => return,
    };
    send(|| Event::Command {
        recipient,
        command: command.as_bytes().to_vec(),
    });
}

pub(crate) fn configstring(index: i32, value: &str) {
    let Ok(index) = u16::try_from(index) else {
        return;
    };
    send(|| Event::Configstring {
        index,
        value: value.as_bytes().to_vec(),
    });
}

pub(crate) fn client_dropped(client_id: i32) {
    if let Ok(client) = u8::try_from(client_id) {
        send(|| Event::PlayerLeft { client });
    }
}

/// Takes a stats message of the game, in the form the stats socket carries it.
pub(crate) fn stats(message: &str) {
    if !is_recording() {
        return;
    }
    let mut recorder = RECORDER.lock();
    let delivered = recorder
        .session
        .as_mut()
        .is_none_or(|session| session.stats(message));
    if !delivered {
        recorder.give_up();
    }
}

pub(crate) fn is_recording() -> bool {
    RECORDING.load(Ordering::Acquire)
}

/// The engine hands the restart command to the clients without passing the send hook.
pub(crate) fn map_restarted() {
    server_command(None, "map_restart\n");
}
