//! One recording that is being written.

use alloc::vec::Vec;
use core::time::Duration;
use std::{
    io::{self, BufWriter},
    path::PathBuf,
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use ql_sqx::{ArchiveWriter, DemoWriter, Event};

use super::{
    catalog::catalog,
    disk, engine_state,
    settings::Settings,
    sidecar::{PlayerEntry, Sidecar},
    storage::create_recording,
};
use crate::QuakeLiveEngine;

/// How long the end of a map waits for the rest of a recording to be written.
const END_PATIENCE: Duration = Duration::from_secs(1);
const PLAYERS_INTERVAL: Duration = Duration::from_secs(1);

pub(super) struct Session {
    writer: DemoWriter,
    path: PathBuf,
    players_read: Option<Instant>,
    frames: u32,
    total: Duration,
    longest: Duration,
}

impl Session {
    pub(super) fn start(
        main_engine: &QuakeLiveEngine,
        settings: &Settings,
    ) -> anyhow::Result<Self> {
        let begun = Instant::now();
        let now = SystemTime::now();
        let configstrings = engine_state::configstrings(main_engine);
        let map = engine_state::map_name(main_engine);
        let (file, path) = create_recording(&settings.directory, now, &map)?;
        let archive = ArchiveWriter::start(
            BufWriter::new(file),
            &engine_state::meta(main_engine, now),
            &configstrings,
        )?;
        let name = path.clone();
        let writer = DemoWriter::start(archive, configstrings, move |outcome| {
            catalog().finished(name.clone());
            match outcome {
                Ok(stats) => log::info!(
                    target: "shinqlx",
                    func = "demo";
                    "Finished {}: {} frames, {} bytes",
                    name.display(), stats.frames, stats.bytes
                ),
                Err(error) => log::error!(
                    target: "shinqlx",
                    func = "demo";
                    "Failed {}: {error}",
                    name.display()
                ),
            }
            disk::request_sweep();
        })?;
        log::info!(
            target: "shinqlx",
            func = "demo";
            "Recording to {} (started in {:?})",
            path.display(), begun.elapsed()
        );
        let sidecar = Sidecar {
            map,
            factory: engine_state::cvar(main_engine, "g_factory").unwrap_or_default(),
            started: seconds_since_epoch(now),
            ..Sidecar::default()
        };
        let session = Self {
            writer,
            path,
            players_read: None,
            frames: 0,
            total: Duration::ZERO,
            longest: Duration::ZERO,
        };
        let described = catalog().begin(session.path.clone(), sidecar);
        session.report_description(described);
        Ok(session)
    }

    /// Keeps the sidecar up to date with who plays and whether a match runs.
    pub(super) fn note(
        &mut self,
        now: Instant,
        in_match: bool,
        players: impl FnOnce() -> Vec<PlayerEntry>,
    ) {
        let due = self
            .players_read
            .is_none_or(|read| now.duration_since(read) >= PLAYERS_INTERVAL);
        let seen = due.then(|| {
            self.players_read = Some(now);
            players()
        });
        let described = catalog().update(|sidecar| {
            let news = in_match && sidecar.note_match();
            seen.is_some_and(|seen| sidecar.add_players(seen)) || news
        });
        self.report_description(described);
    }

    fn report_description(&self, described: io::Result<()>) {
        if let Err(error) = described {
            log::error!(
                target: "shinqlx",
                func = "demo";
                "Could not describe {}: {error}",
                self.path.display()
            );
        }
    }

    /// Hands over an event without waiting, returning `false` if the writer cannot take it.
    pub(super) fn send(&self, event: Event) -> bool {
        self.writer.send(event)
    }

    /// Counts the time the capture of a frame took on the game thread.
    pub(super) fn count_frame(&mut self, elapsed: Duration) {
        self.frames += 1;
        self.total += elapsed;
        self.longest = self.longest.max(elapsed);
    }

    /// Ends the recording. Waiting for the file is only meant for the end of a map, because the
    /// server may quit right after it.
    pub(super) fn end(self, reason: &str, wait_for_file: bool) {
        let described = catalog().end(seconds_since_epoch(SystemTime::now()));
        self.report_description(described);
        let ended = self.writer.send(Event::End {
            reason: reason.into(),
        });
        let average = self.total.checked_div(self.frames).unwrap_or_default();
        log::info!(
            target: "shinqlx",
            func = "demo";
            "Stopping {} ({reason}, end written: {ended}): {} frames captured, {average:?} on average, {:?} at most",
            self.path.display(), self.frames, self.longest
        );
        let thread = self.writer.finish();
        if wait_for_file {
            let deadline = Instant::now() + END_PATIENCE;
            while !thread.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn seconds_since_epoch(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
