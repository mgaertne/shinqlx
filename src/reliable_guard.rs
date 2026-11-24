//! Spreads bursts of low-priority server commands over several frames.
//!
//! The engine keeps 64 reliable commands per client. A client that gets more than that before it
//! acknowledges them loses the oldest ones ("reliable command was cycled out"). The guard holds back
//! commands whose loss does not matter much while the ring of a client is filling up, and hands them
//! out again in later frames.

use alloc::collections::VecDeque;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::ffi::c::prelude::MAX_CLIENTS;

/// The most commands that wait for one client; further commands are sent straight through.
const MAX_QUEUED_PER_CLIENT: usize = 128;

/// The frames a command may wait before it goes out regardless of the backlog. A client whose
/// acknowledge stopped would otherwise hold its commands forever.
const MAX_HOLD_FRAMES: u64 = 40;

/// The commands that may wait: statistics, chat and prints. Everything else, like configstrings,
/// goes out immediately because the clients rely on the order.
const DEFERRABLE_COMMANDS: [&str; 6] = ["castats", "print", "chat", "tchat", "cp", "scores"];

/// The ring of reliable commands the engine keeps for a client.
pub(crate) const RELIABLE_RING: i32 = 64;

/// The cvar that switches the guard on or off.
pub(crate) const GUARD_CVAR: &str = "qlx_reliableGuard";
/// The cvar for the backlog from which commands are held back.
pub(crate) const WATERMARK_CVAR: &str = "qlx_reliableWatermark";
/// The cvar for the most commands that are released to one client in a frame.
pub(crate) const BURST_CVAR: &str = "qlx_reliableBurst";
/// The cvar for the ring size the guard assumes. Lower values make a test rig overflow earlier.
pub(crate) const RING_CVAR: &str = "qlx_reliableRing";
/// The cvar for the most deferrable commands that go out to all clients together in a frame.
pub(crate) const GLOBAL_BURST_CVAR: &str = "qlx_reliableGlobalBurst";
/// The cvar that lets held `print` commands of a client be merged into one command.
pub(crate) const MERGE_PRINTS_CVAR: &str = "qlx_reliableMergePrints";

/// The most characters of text in a merged `print` command; the engine drops a command of 1023 bytes or more.
const PRINT_PAYLOAD_MAX: usize = 980;

/// The values of the cvars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuardSettings {
    pub(crate) enabled: bool,
    pub(crate) watermark: i32,
    pub(crate) burst: usize,
    pub(crate) ring: i32,
    pub(crate) global_burst: usize,
    pub(crate) merge_prints: bool,
}

impl GuardSettings {
    pub(crate) const DEFAULT: Self = Self {
        enabled: true,
        watermark: 32,
        burst: 8,
        ring: RELIABLE_RING,
        global_burst: 64,
        merge_prints: true,
    };

    /// The same settings with the merging of held prints switched on or off.
    pub(crate) const fn with_merge_prints(mut self, merge_prints: bool) -> Self {
        self.merge_prints = merge_prints;
        self
    }

    /// Takes the cvar values and keeps them in the range the ring allows.
    pub(crate) fn from_values(
        enabled: i32,
        watermark: i32,
        burst: i32,
        ring: i32,
        global_burst: i32,
    ) -> Self {
        let ring = ring.clamp(16, RELIABLE_RING);
        Self {
            enabled: enabled != 0,
            watermark: watermark.clamp(8, ring - 8),
            burst: usize::try_from(burst.clamp(1, 32)).unwrap_or(1),
            ring,
            global_burst: usize::try_from(global_burst.clamp(8, 1024)).unwrap_or(8),
            merge_prints: true,
        }
    }
}

/// What happens to a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The command goes to the engine right away.
    SendNow,
    /// The guard keeps the command until the ring of the client has room.
    Held,
}

/// The counters the guard keeps, for diagnosing a server.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GuardStats {
    pub(crate) held: u64,
    pub(crate) released: u64,
    pub(crate) passed_through: u64,
    pub(crate) forced: u64,
    pub(crate) peak_backlog: u64,
    pub(crate) peak_queue: u64,
    pub(crate) at_ring: u64,
    pub(crate) held_by_global: u64,
    pub(crate) merged: u64,
}

#[derive(Debug, Default)]
struct Counters {
    held: AtomicU64,
    released: AtomicU64,
    passed_through: AtomicU64,
    forced: AtomicU64,
    peak_backlog: AtomicU64,
    peak_queue: AtomicU64,
    at_ring: AtomicU64,
    held_by_global: AtomicU64,
    merged: AtomicU64,
}

/// The text of a `print` command, `None` for another command or a text that holds a quote.
fn print_payload(command: &str) -> Option<&str> {
    let text = command.strip_prefix("print \"")?.strip_suffix('"')?;
    (!text.contains('"')).then_some(text)
}

/// Two `print` commands as one, if the text of both fits into a single command.
fn merge_prints(held: &str, next: &str) -> Option<String> {
    let (first, second) = (print_payload(held)?, print_payload(next)?);
    (first.len() + second.len() <= PRINT_PAYLOAD_MAX).then(|| format!("print \"{first}{second}\""))
}

/// The first word of a command.
fn command_name(command: &str) -> &str {
    command
        .split(|c: char| c.is_whitespace() || c == '"')
        .next()
        .unwrap_or_default()
}

/// Whether a command may wait for a later frame.
pub(crate) fn is_deferrable(command: &str) -> bool {
    let name = command_name(command);
    DEFERRABLE_COMMANDS.contains(&name) || name.starts_with("scores_")
}

/// A command that waits, with the frame in which it was held back.
type HeldCommand = (String, u64);

/// The queues for all clients.
#[derive(Debug)]
pub(crate) struct ReliableGuard {
    queues: Mutex<[VecDeque<HeldCommand>; MAX_CLIENTS as usize]>,
    counters: Counters,
    frame_sent: AtomicUsize,
    rotation: AtomicUsize,
    frame: AtomicU64,
}

impl ReliableGuard {
    pub(crate) const fn new() -> Self {
        Self {
            queues: Mutex::new([const { VecDeque::new() }; MAX_CLIENTS as usize]),
            counters: Counters {
                held: AtomicU64::new(0),
                released: AtomicU64::new(0),
                passed_through: AtomicU64::new(0),
                forced: AtomicU64::new(0),
                peak_backlog: AtomicU64::new(0),
                peak_queue: AtomicU64::new(0),
                at_ring: AtomicU64::new(0),
                held_by_global: AtomicU64::new(0),
                merged: AtomicU64::new(0),
            },
            frame_sent: AtomicUsize::new(0),
            rotation: AtomicUsize::new(0),
            frame: AtomicU64::new(0),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, [VecDeque<HeldCommand>; MAX_CLIENTS as usize]> {
        self.queues
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn note_backlog(&self, backlog: i32) {
        self.counters.peak_backlog.fetch_max(
            u64::try_from(backlog).unwrap_or_default(),
            Ordering::Relaxed,
        );
    }

    /// Decides what happens to a command for a client whose ring holds `backlog` unacknowledged
    /// commands, and keeps the command if it has to wait.
    pub(crate) fn submit(
        &self,
        client_id: i32,
        command: &str,
        backlog: i32,
        settings: GuardSettings,
    ) -> Verdict {
        self.note_backlog(backlog);
        if backlog >= settings.ring {
            self.counters.at_ring.fetch_add(1, Ordering::Relaxed);
        }
        let Some(index) = usize::try_from(client_id)
            .ok()
            .filter(|index| *index < MAX_CLIENTS as usize)
        else {
            return Verdict::SendNow;
        };
        if !settings.enabled || !is_deferrable(command) {
            return Verdict::SendNow;
        }

        let mut queues = self.lock();
        let queue = &mut queues[index];
        let over_global = self.frame_sent.load(Ordering::Relaxed) >= settings.global_burst;
        if queue.is_empty() && backlog < settings.watermark && !over_global {
            self.frame_sent.fetch_add(1, Ordering::Relaxed);
            return Verdict::SendNow;
        }
        if over_global {
            self.counters.held_by_global.fetch_add(1, Ordering::Relaxed);
        }

        if settings.merge_prints
            && let Some((last, _)) = queue.back_mut()
            && let Some(merged) = merge_prints(last, command)
        {
            *last = merged;
            self.counters.merged.fetch_add(1, Ordering::Relaxed);
            return Verdict::Held;
        }
        if queue.len() >= MAX_QUEUED_PER_CLIENT {
            self.counters.passed_through.fetch_add(1, Ordering::Relaxed);
            return Verdict::SendNow;
        }
        queue.push_back((command.to_owned(), self.frame.load(Ordering::Relaxed)));
        self.counters.held.fetch_add(1, Ordering::Relaxed);
        self.counters
            .peak_queue
            .fetch_max(queue.len() as u64, Ordering::Relaxed);
        Verdict::Held
    }

    /// The commands of a client that fit into its ring now, at most `settings.burst` of them.
    pub(crate) fn release(
        &self,
        client_id: i32,
        backlog: i32,
        settings: GuardSettings,
    ) -> Vec<String> {
        self.note_backlog(backlog);
        let Some(index) = usize::try_from(client_id)
            .ok()
            .filter(|index| *index < MAX_CLIENTS as usize)
        else {
            return Vec::new();
        };

        let room = usize::try_from(settings.watermark.saturating_sub(backlog)).unwrap_or(0);
        let global_room = settings
            .global_burst
            .saturating_sub(self.frame_sent.load(Ordering::Relaxed));
        let frame = self.frame.load(Ordering::Relaxed);
        let mut queues = self.lock();
        let queue = &mut queues[index];
        let aged = queue
            .iter()
            .take_while(|(_, since)| frame.saturating_sub(*since) >= MAX_HOLD_FRAMES)
            .count();
        let regular = room.min(settings.burst).min(global_room);
        let take = if settings.enabled {
            regular.max(aged.min(settings.burst))
        } else {
            usize::MAX
        };
        let released: Vec<String> = queue
            .drain(..take.min(queue.len()))
            .map(|(command, _)| command)
            .collect();
        self.frame_sent.fetch_add(released.len(), Ordering::Relaxed);
        self.counters.forced.fetch_add(
            released.len().saturating_sub(regular) as u64,
            Ordering::Relaxed,
        );
        self.counters
            .released
            .fetch_add(released.len() as u64, Ordering::Relaxed);
        released
    }

    /// Starts a new frame: the global budget is available again.
    pub(crate) fn begin_frame(&self) {
        self.frame.fetch_add(1, Ordering::Relaxed);
        self.frame_sent.store(0, Ordering::Relaxed);
    }

    /// The client the release of this frame starts with; it changes every frame so that no client
    /// is always served last.
    pub(crate) fn rotation_start(&self, clients: i32) -> i32 {
        let clients = usize::try_from(clients).unwrap_or(0).max(1);
        i32::try_from(self.rotation.fetch_add(1, Ordering::Relaxed) % clients).unwrap_or(0)
    }

    /// Forgets what waits for a client, e.g. because it left.
    pub(crate) fn clear(&self, client_id: i32) {
        if let Some(index) = usize::try_from(client_id)
            .ok()
            .filter(|index| *index < MAX_CLIENTS as usize)
        {
            self.lock()[index].clear();
        }
    }

    /// Whether anything waits for any client.
    pub(crate) fn has_waiting(&self) -> bool {
        self.lock().iter().any(|queue| !queue.is_empty())
    }

    pub(crate) fn stats(&self) -> GuardStats {
        GuardStats {
            held: self.counters.held.load(Ordering::Relaxed),
            released: self.counters.released.load(Ordering::Relaxed),
            passed_through: self.counters.passed_through.load(Ordering::Relaxed),
            forced: self.counters.forced.load(Ordering::Relaxed),
            peak_backlog: self.counters.peak_backlog.load(Ordering::Relaxed),
            peak_queue: self.counters.peak_queue.load(Ordering::Relaxed),
            at_ring: self.counters.at_ring.load(Ordering::Relaxed),
            held_by_global: self.counters.held_by_global.load(Ordering::Relaxed),
            merged: self.counters.merged.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn reset_stats(&self) {
        [
            &self.counters.held,
            &self.counters.released,
            &self.counters.passed_through,
            &self.counters.forced,
            &self.counters.peak_backlog,
            &self.counters.peak_queue,
            &self.counters.at_ring,
            &self.counters.held_by_global,
            &self.counters.merged,
        ]
        .into_iter()
        .for_each(|counter| counter.store(0, Ordering::Relaxed));
    }
}

/// The guard of the running server.
pub(crate) static RELIABLE_GUARD: ReliableGuard = ReliableGuard::new();

#[cfg(test)]
mod reliable_guard_tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    use super::{
        GuardSettings, MAX_QUEUED_PER_CLIENT, ReliableGuard, Verdict, command_name, is_deferrable,
    };

    const SETTINGS: GuardSettings = GuardSettings {
        enabled: true,
        watermark: 32,
        burst: 3,
        ring: 64,
        global_burst: 1000,
        merge_prints: true,
    };

    #[rstest]
    #[case("castats 0 1 2", "castats")]
    #[case("print \"hello\"", "print")]
    #[case("cs 5 \"x\"", "cs")]
    #[case("", "")]
    fn command_name_is_the_first_word(#[case] command: &str, #[case] expected: &str) {
        assert_eq!(command_name(command), expected);
    }

    #[rstest]
    #[case("castats 0 1 2", true)]
    #[case("print \"hi\"", true)]
    #[case("tchat \"hi\"", true)]
    #[case("scores_ca 8 0", true)]
    #[case("scores 8 0", true)]
    #[case("cs 5 \"x\"", false)]
    #[case("map_restart", false)]
    #[case("disconnect", false)]
    #[case("tinfo 1 2", false)]
    fn only_low_priority_commands_may_wait(#[case] command: &str, #[case] deferrable: bool) {
        assert_eq!(is_deferrable(command), deferrable);
    }

    #[test]
    fn commands_go_out_while_the_ring_has_room() {
        let guard = ReliableGuard::new();

        assert_eq!(guard.submit(0, "castats 0", 31, SETTINGS), Verdict::SendNow);
        assert!(!guard.has_waiting());
    }

    #[test]
    fn commands_wait_from_the_watermark_on() {
        let guard = ReliableGuard::new();

        assert_eq!(guard.submit(0, "castats 0", 32, SETTINGS), Verdict::Held);
        assert!(guard.has_waiting());
    }

    #[test]
    fn held_prints_of_a_client_are_merged_into_one_command() {
        let guard = ReliableGuard::new();
        guard.submit(0, "print \"one\n\"", 40, SETTINGS);

        assert_eq!(
            guard.submit(0, "print \"two\n\"", 40, SETTINGS),
            Verdict::Held
        );
        assert_eq!(
            guard.submit(0, "print \"three\n\"", 40, SETTINGS),
            Verdict::Held
        );

        assert_eq!(
            guard.release(0, 0, SETTINGS),
            ["print \"one\ntwo\nthree\n\""]
        );
        let stats = guard.stats();
        assert_eq!((stats.held, stats.released, stats.merged), (1, 1, 2));
    }

    #[test]
    fn prints_are_only_merged_with_a_print_directly_before() {
        let guard = ReliableGuard::new();
        guard.submit(0, "print \"a\"", 40, SETTINGS);
        guard.submit(0, "chat \"b\"", 40, SETTINGS);
        guard.submit(0, "print \"c\"", 40, SETTINGS);

        assert_eq!(
            guard.release(0, 0, SETTINGS),
            ["print \"a\"", "chat \"b\"", "print \"c\""]
        );
        assert_eq!(guard.stats().merged, 0);
    }

    #[test]
    fn prints_are_not_merged_beyond_the_size_of_one_command() {
        let guard = ReliableGuard::new();
        let long = format!("print \"{}\"", "x".repeat(600));
        guard.submit(0, &long, 40, SETTINGS);
        guard.submit(0, &long, 40, SETTINGS);

        assert_eq!(guard.release(0, 0, SETTINGS), [long.clone(), long]);
    }

    #[test]
    fn prints_with_a_quote_inside_are_not_merged() {
        let guard = ReliableGuard::new();
        guard.submit(0, "print \"a\"", 40, SETTINGS);
        guard.submit(0, "print \"say \"hi\"\"", 40, SETTINGS);

        assert_eq!(guard.release(0, 0, SETTINGS).len(), 2);
    }

    #[test]
    fn the_merging_can_be_switched_off() {
        let guard = ReliableGuard::new();
        let settings = SETTINGS.with_merge_prints(false);
        guard.submit(0, "print \"a\"", 40, settings);
        guard.submit(0, "print \"b\"", 40, settings);

        assert_eq!(guard.release(0, 0, settings).len(), 2);
        assert_eq!(guard.stats().merged, 0);
    }

    #[test]
    fn a_full_queue_still_takes_up_prints_by_merging() {
        let guard = ReliableGuard::new();
        (0..MAX_QUEUED_PER_CLIENT - 1).for_each(|index| {
            guard.submit(0, &format!("castats {index}"), 40, SETTINGS);
        });
        guard.submit(0, "print \"a\"", 40, SETTINGS);

        assert_eq!(guard.submit(0, "print \"b\"", 40, SETTINGS), Verdict::Held);
        assert_eq!(guard.stats().passed_through, 0);
        assert_eq!(guard.stats().merged, 1);
    }

    #[test]
    fn important_commands_never_wait() {
        let guard = ReliableGuard::new();

        assert_eq!(
            guard.submit(0, "cs 3 \"x\"", 60, SETTINGS),
            Verdict::SendNow
        );
    }

    #[test]
    fn order_is_kept_once_something_waits() {
        let guard = ReliableGuard::new();
        guard.submit(0, "castats 0", 40, SETTINGS);

        assert_eq!(guard.submit(0, "castats 1", 0, SETTINGS), Verdict::Held);
        assert_eq!(
            guard.release(
                0,
                0,
                GuardSettings {
                    burst: 8,
                    ..SETTINGS
                }
            ),
            ["castats 0", "castats 1"]
        );
    }

    #[test]
    fn a_disabled_guard_lets_everything_through() {
        let guard = ReliableGuard::new();
        let disabled = GuardSettings {
            enabled: false,
            ..SETTINGS
        };

        assert_eq!(guard.submit(0, "castats 0", 63, disabled), Verdict::SendNow);
    }

    #[test]
    fn release_obeys_burst_and_room() {
        let guard = ReliableGuard::new();
        (0..10).for_each(|i| {
            guard.submit(1, &format!("castats {i}"), 40, SETTINGS);
        });

        assert_eq!(guard.release(1, 40, SETTINGS), Vec::<String>::new());
        assert_eq!(
            guard.release(1, 20, SETTINGS),
            ["castats 0", "castats 1", "castats 2"]
        );
        assert_eq!(guard.release(1, 31, SETTINGS), ["castats 3"]);
    }

    #[test]
    fn commands_at_the_ring_size_are_counted_even_when_sent() {
        let guard = ReliableGuard::new();
        let small_ring = GuardSettings::from_values(1, 99, 8, 16, 1000);

        guard.submit(0, "cs 1 \"x\"", 15, small_ring);
        guard.submit(0, "cs 1 \"x\"", 16, small_ring);
        guard.submit(0, "castats 0", 20, small_ring);

        assert_eq!(small_ring.ring, 16);
        assert_eq!(small_ring.watermark, 8);
        assert_eq!(guard.stats().at_ring, 2);
    }

    #[test]
    fn the_global_limit_holds_commands_for_all_clients_together() {
        let guard = ReliableGuard::new();
        let limited = GuardSettings {
            global_burst: 8,
            ..SETTINGS
        };

        let verdicts: Vec<Verdict> = (0..12)
            .map(|client| guard.submit(client, "castats 0", 0, limited))
            .collect();

        assert_eq!(
            verdicts.iter().filter(|v| **v == Verdict::SendNow).count(),
            8
        );
        assert_eq!(verdicts.iter().filter(|v| **v == Verdict::Held).count(), 4);
        assert_eq!(guard.stats().held_by_global, 4);
        assert_eq!(guard.release(8, 0, limited), Vec::<String>::new());

        guard.begin_frame();
        assert_eq!(guard.release(8, 0, limited), ["castats 0"]);
        assert_eq!(guard.release(9, 0, limited), ["castats 0"]);
    }

    #[test]
    fn released_commands_use_up_the_global_budget() {
        let guard = ReliableGuard::new();
        let limited = GuardSettings {
            global_burst: 8,
            burst: 6,
            ..SETTINGS
        };
        (0..2).for_each(|client| {
            (0..6).for_each(|i| {
                guard.submit(client, &format!("castats {i}"), 40, limited);
            });
        });

        assert_eq!(guard.release(0, 0, limited).len(), 6);
        assert_eq!(guard.release(1, 0, limited).len(), 2);
        assert_eq!(guard.release(1, 0, limited).len(), 0);
    }

    #[test]
    fn the_rotation_start_changes_every_frame() {
        let guard = ReliableGuard::new();

        let starts: Vec<i32> = (0..5).map(|_| guard.rotation_start(3)).collect();

        assert_eq!(starts, [0, 1, 2, 0, 1]);
    }

    #[test]
    fn queues_are_independent_per_client() {
        let guard = ReliableGuard::new();
        guard.submit(2, "castats 0", 40, SETTINGS);

        assert_eq!(guard.submit(3, "castats 0", 0, SETTINGS), Verdict::SendNow);
        guard.clear(2);
        assert!(!guard.has_waiting());
    }

    #[test]
    fn a_full_queue_sends_straight_through_and_loses_nothing() {
        let guard = ReliableGuard::new();
        let verdicts: Vec<Verdict> = (0..200)
            .map(|i| guard.submit(4, &format!("castats {i}"), 40, SETTINGS))
            .collect();

        assert_eq!(
            verdicts.iter().filter(|v| **v == Verdict::SendNow).count(),
            72
        );
        let stats = guard.stats();
        assert_eq!(stats.held, 128);
        assert_eq!(stats.passed_through, 72);
        assert_eq!(stats.peak_queue, 128);
        assert_eq!(stats.peak_backlog, 40);
        guard.reset_stats();
        assert_eq!(guard.stats(), Default::default());
    }

    #[test]
    fn commands_that_waited_too_long_go_out_regardless_of_the_backlog() {
        let guard = ReliableGuard::new();
        let settings = GuardSettings {
            burst: 2,
            ..SETTINGS
        };
        (0..5).for_each(|i| {
            guard.submit(5, &format!("castats {i}"), 60, settings);
        });

        (0..39).for_each(|_| guard.begin_frame());
        assert_eq!(guard.release(5, 60, settings), Vec::<String>::new());

        guard.begin_frame();
        assert_eq!(guard.release(5, 60, settings), ["castats 0", "castats 1"]);
        assert_eq!(guard.stats().forced, 2);
    }

    #[test]
    fn invalid_client_ids_are_passed_through() {
        let guard = ReliableGuard::new();

        assert_eq!(
            guard.submit(-1, "castats 0", 63, SETTINGS),
            Verdict::SendNow
        );
        assert_eq!(
            guard.submit(9999, "castats 0", 63, SETTINGS),
            Verdict::SendNow
        );
        assert_eq!(guard.release(-1, 0, SETTINGS), Vec::<String>::new());
    }

    #[rstest]
    #[case(0, 0, 0, 8, 1)]
    #[case(1, 99, 99, 56, 32)]
    #[case(1, 32, 8, 32, 8)]
    fn settings_are_clamped(
        #[case] enabled: i32,
        #[case] watermark: i32,
        #[case] burst: i32,
        #[case] expected_watermark: i32,
        #[case] expected_burst: usize,
    ) {
        let settings = GuardSettings::from_values(enabled, watermark, burst, 64, 1000);

        assert_eq!(settings.enabled, enabled != 0);
        assert_eq!(settings.watermark, expected_watermark.clamp(8, 56));
        assert_eq!(settings.burst, expected_burst.min(32));
    }
}
