//! Rate limiting for the packets that the engine hands over to the Steam game server API.
//!
//! Steam answers server queries (`A2S_INFO` and friends) with packets that are much larger than
//! the request, so a spoofed source address turns the server into a reflector. The limiter drops
//! the requests before Steam sees them.
//!
//! The per-source state lives in a fixed table without heap allocation, so a flood from many
//! spoofed addresses cannot grow the memory of the server.

use core::{
    ffi::c_int,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::{sync::LazyLock, time::Instant};

use log::warn;

use crate::engine::{FindCVar, prelude::*};

const TABLE_BITS: u32 = 11;
const TABLE_SIZE: usize = 1 << TABLE_BITS;
const SUMMARY_INTERVAL_MS: u64 = 60_000;

/// Limits applied to the packets handed to Steam. A rate of `0` disables the respective limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct QueryLimits {
    pub(crate) per_source_rate: f64,
    pub(crate) burst: f64,
    pub(crate) global_rate: f64,
}

impl QueryLimits {
    pub(crate) const DEFAULT: Self = Self {
        per_source_rate: 1.0,
        burst: 10.0,
        global_rate: 10.0,
    };
    /// Stricter limits for the rules query, whose answers are the largest. The burst of two lets
    /// one complete query (challenge request and request with the challenge) pass.
    pub(crate) const RULES_DEFAULT: Self = Self {
        per_source_rate: 0.1,
        burst: 2.0,
        global_rate: 10.0,
    };
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    last_refill_ms: u64,
}

impl Bucket {
    const fn full(capacity: f64, now_ms: u64) -> Self {
        Self {
            tokens: capacity,
            last_refill_ms: now_ms,
        }
    }

    fn refilled(&self, rate: f64, capacity: f64, now_ms: u64) -> f64 {
        let elapsed_secs = now_ms.saturating_sub(self.last_refill_ms) as f64 / 1000.0;
        elapsed_secs.mul_add(rate, self.tokens).min(capacity)
    }

    fn try_take(&mut self, rate: f64, capacity: f64, now_ms: u64) -> bool {
        self.tokens = self.refilled(rate, capacity, now_ms);
        self.last_refill_ms = now_ms;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    source: [u8; 4],
    in_use: bool,
    bucket: Bucket,
}

impl Slot {
    const EMPTY: Self = Self {
        source: [0; 4],
        in_use: false,
        bucket: Bucket::full(0.0, 0),
    };
}

/// Token buckets per IPv4 source plus one for all sources together.
///
/// Sources are mapped to slots by hash. A source meeting a slot that is still in use by another
/// source shares its bucket, so colliding sources are limited together and can never reset
/// each other's limit.
#[derive(Debug)]
pub(crate) struct QueryRateLimiter {
    limits: QueryLimits,
    slots: [Slot; TABLE_SIZE],
    global: Bucket,
    dropped: u64,
    last_summary_ms: u64,
}

impl QueryRateLimiter {
    pub(crate) const fn new(limits: QueryLimits) -> Self {
        Self {
            limits,
            slots: [Slot::EMPTY; TABLE_SIZE],
            global: Bucket::full(Self::global_capacity(&limits), 0),
            dropped: 0,
            last_summary_ms: 0,
        }
    }

    pub(crate) const fn set_limits(&mut self, limits: QueryLimits) {
        self.limits = limits;
    }

    /// Returns whether the packet of `source` may be handed to Steam.
    pub(crate) fn allow(&mut self, source: [u8; 4], now_ms: u64) -> bool {
        let allowed = self.source_allows(source, now_ms) && self.global_allows(now_ms);
        if !allowed {
            self.dropped += 1;
        }
        allowed
    }

    /// Number of packets dropped since the last summary, at most once per summary interval.
    pub(crate) fn take_dropped_summary(&mut self, now_ms: u64) -> Option<u64> {
        if now_ms.saturating_sub(self.last_summary_ms) < SUMMARY_INTERVAL_MS {
            return None;
        }
        self.last_summary_ms = now_ms;
        Some(core::mem::take(&mut self.dropped)).filter(|dropped| *dropped > 0)
    }

    const fn global_capacity(limits: &QueryLimits) -> f64 {
        if limits.global_rate > 1.0 {
            limits.global_rate
        } else {
            1.0
        }
    }

    fn slot_index(source: [u8; 4]) -> usize {
        let hash = u32::from_be_bytes(source).wrapping_mul(0x9E37_79B1);
        (hash >> (u32::BITS - TABLE_BITS)) as usize
    }

    fn source_allows(&mut self, source: [u8; 4], now_ms: u64) -> bool {
        let QueryLimits {
            per_source_rate: rate,
            burst,
            ..
        } = self.limits;
        if rate <= 0.0 {
            return true;
        }

        let Some(slot) = self.slots.get_mut(Self::slot_index(source)) else {
            return true;
        };
        let takes_over = !slot.in_use
            || (slot.source != source && slot.bucket.refilled(rate, burst, now_ms) >= burst);
        if takes_over {
            *slot = Slot {
                source,
                in_use: true,
                bucket: Bucket::full(burst, now_ms),
            };
        }
        slot.bucket.try_take(rate, burst, now_ms)
    }

    fn global_allows(&mut self, now_ms: u64) -> bool {
        let rate = self.limits.global_rate;
        if rate <= 0.0 {
            return true;
        }
        self.global
            .try_take(rate, Self::global_capacity(&self.limits), now_ms)
    }
}

/// A rate limiter whose limits are read from three cvars every few seconds.
struct QueryGate {
    limiter: parking_lot::Mutex<QueryRateLimiter>,
    refreshed_ms: AtomicU64,
    cvars: [&'static str; 3],
    defaults: QueryLimits,
    label: &'static str,
}

impl QueryGate {
    const fn new(cvars: [&'static str; 3], defaults: QueryLimits, label: &'static str) -> Self {
        Self {
            limiter: parking_lot::Mutex::new(QueryRateLimiter::new(defaults)),
            refreshed_ms: AtomicU64::new(0),
            cvars,
            defaults,
            label,
        }
    }

    fn limits_from_cvars<T>(&self, main_engine: &T) -> QueryLimits
    where
        T: FindCVar<&'static str>,
    {
        let read = |name: &'static str, default: f64| {
            main_engine
                .find_cvar(name)
                .and_then(|cvar| cvar.get_string().trim().parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value >= 0.0)
                .unwrap_or(default)
        };
        let [rate, burst, global_rate] = self.cvars;
        QueryLimits {
            per_source_rate: read(rate, self.defaults.per_source_rate),
            burst: read(burst, self.defaults.burst),
            global_rate: read(global_rate, self.defaults.global_rate),
        }
    }

    fn allows<T>(&self, main_engine: &T, from: &netadr_t, now_ms: u64) -> bool
    where
        T: FindCVar<&'static str>,
    {
        if from.type_ != netadrtype_t::NA_IP {
            return true;
        }

        let mut limiter = self.limiter.lock();
        let refreshed_ms = self.refreshed_ms.load(Ordering::Acquire);
        if refreshed_ms == 0
            || now_ms.saturating_sub(refreshed_ms) >= QUERY_LIMITS_REFRESH_INTERVAL_MS
        {
            limiter.set_limits(self.limits_from_cvars(main_engine));
            self.refreshed_ms.store(now_ms.max(1), Ordering::Release);
        }

        let allowed = limiter.allow(from.ip, now_ms);
        if let Some(dropped) = limiter.take_dropped_summary(now_ms) {
            warn!(
                target: "shinqlx",
                func = "allows"; "Dropped {dropped} {} packets that exceeded the rate limit.",
                self.label
            );
        }
        allowed
    }
}

static QUERY_GATE: QueryGate = QueryGate::new(
    [
        "qlx_steamQueryRate",
        "qlx_steamQueryBurst",
        "qlx_steamQueryGlobalRate",
    ],
    QueryLimits::DEFAULT,
    "Steam query",
);
static QUERY_RULES_GATE: QueryGate = QueryGate::new(
    [
        "qlx_steamQueryRulesRate",
        "qlx_steamQueryRulesBurst",
        "qlx_steamQueryRulesGlobalRate",
    ],
    QueryLimits::RULES_DEFAULT,
    "Steam rules query",
);
static QUERY_CLOCK: LazyLock<Instant> = LazyLock::new(Instant::now);

const QUERY_LIMITS_REFRESH_INTERVAL_MS: u64 = 5_000;

const STEAM_QUERY_HEADER: [u8; 4] = [0xFF; 4];
const STEAM_QUERY_OPCODES: &[u8; 3] = b"TUV";
const STEAM_RULES_OPCODE: u8 = b'V';
static QUERY_DROP_RULES: AtomicBool = AtomicBool::new(false);
static QUERY_DROP_RULES_REFRESHED_MS: AtomicU64 = AtomicU64::new(0);

/// The message buffer as the engine hands it to Steam; it has no `oob` field between the two
/// flags and `data`, unlike `msg_t`.
#[repr(C)]
pub(crate) struct SteamPacketBuffer {
    pub(crate) allowoverflow: c_int,
    pub(crate) overflowed: c_int,
    pub(crate) data: *const u8,
    pub(crate) maxsize: c_int,
    pub(crate) cursize: c_int,
}

fn steam_query_opcode(msg: *const msg_t) -> Option<u8> {
    // SAFETY: a non-null pointer from the engine points to a valid structure, null yields None.
    let msg = unsafe { msg.cast::<SteamPacketBuffer>().as_ref() }?;
    let size = usize::try_from(msg.cursize).ok()?;
    let fits_buffer = msg.cursize <= msg.maxsize;
    if msg.data.is_null() || !fits_buffer || size <= STEAM_QUERY_HEADER.len() {
        return None;
    }

    // SAFETY: data was checked as non-null and cursize as non-negative and within maxsize.
    let packet = unsafe { core::slice::from_raw_parts(msg.data, size) };
    packet
        .strip_prefix(&STEAM_QUERY_HEADER)
        .and_then(|rest| rest.first())
        .copied()
        .filter(|opcode| STEAM_QUERY_OPCODES.contains(opcode))
}

fn steam_rules_dropped<T>(main_engine: &T, now_ms: u64) -> bool
where
    T: FindCVar<&'static str>,
{
    let refreshed_ms = QUERY_DROP_RULES_REFRESHED_MS.load(Ordering::Acquire);
    if refreshed_ms == 0 || now_ms.saturating_sub(refreshed_ms) >= QUERY_LIMITS_REFRESH_INTERVAL_MS
    {
        let enabled = main_engine
            .find_cvar("qlx_steamQueryDropRules")
            .is_some_and(|cvar| cvar.get_integer() != 0);
        QUERY_DROP_RULES.store(enabled, Ordering::Release);
        QUERY_DROP_RULES_REFRESHED_MS.store(now_ms.max(1), Ordering::Release);
    }
    QUERY_DROP_RULES.load(Ordering::Acquire)
}

pub(crate) fn steam_query_allowed<T>(main_engine: &T, from: &netadr_t, now_ms: u64) -> bool
where
    T: FindCVar<&'static str>,
{
    QUERY_GATE.allows(main_engine, from, now_ms)
}

fn steam_rules_allowed<T>(main_engine: &T, from: &netadr_t, now_ms: u64) -> bool
where
    T: FindCVar<&'static str>,
{
    QUERY_RULES_GATE.allows(main_engine, from, now_ms)
}
/// Whether a packet for Steam is a query that must not be answered.
pub(crate) fn rejects_steam_packet<T>(
    main_engine: &T,
    from: Option<&netadr_t>,
    msg: *const msg_t,
) -> bool
where
    T: FindCVar<&'static str>,
{
    let now_ms = u64::try_from(QUERY_CLOCK.elapsed().as_millis()).unwrap_or(u64::MAX);
    match (from, steam_query_opcode(msg)) {
        (Some(_), Some(STEAM_RULES_OPCODE)) if steam_rules_dropped(main_engine, now_ms) => true,
        (Some(address), Some(STEAM_RULES_OPCODE))
            if !steam_rules_allowed(main_engine, address, now_ms) =>
        {
            true
        }
        (Some(address), Some(_)) => !steam_query_allowed(main_engine, address, now_ms),
        _ => false,
    }
}

#[cfg(test)]
pub(crate) fn reset_query_gates() {
    *QUERY_GATE.limiter.lock() = QueryRateLimiter::new(QueryLimits::DEFAULT);
    *QUERY_RULES_GATE.limiter.lock() = QueryRateLimiter::new(QueryLimits::RULES_DEFAULT);
    QUERY_RULES_GATE.refreshed_ms.store(0, Ordering::Release);
    QUERY_GATE.refreshed_ms.store(0, Ordering::Release);
    QUERY_DROP_RULES_REFRESHED_MS.store(0, Ordering::Release);
}

#[cfg(test)]
mod query_rate_limiter_tests {
    use pretty_assertions::assert_eq;

    use super::{QueryLimits, QueryRateLimiter, SUMMARY_INTERVAL_MS, TABLE_SIZE};

    const SOURCE: [u8; 4] = [192, 0, 2, 1];
    const OTHER_SOURCE: [u8; 4] = [192, 0, 2, 2];

    fn limits(per_source_rate: f64, burst: f64, global_rate: f64) -> QueryLimits {
        QueryLimits {
            per_source_rate,
            burst,
            global_rate,
        }
    }

    fn source_colliding_with(source: [u8; 4]) -> [u8; 4] {
        let index = QueryRateLimiter::slot_index(source);
        (1..)
            .map(|offset: u32| (u32::from_be_bytes(source) + offset).to_be_bytes())
            .find(|candidate| QueryRateLimiter::slot_index(*candidate) == index)
            .expect("a table with finitely many slots has colliding sources")
    }

    #[test]
    fn slot_indices_stay_within_the_table() {
        assert!(
            [[0; 4], [255; 4], SOURCE, OTHER_SOURCE]
                .into_iter()
                .all(|source| QueryRateLimiter::slot_index(source) < TABLE_SIZE)
        );
    }

    #[test]
    fn allows_a_burst_and_drops_the_rest() {
        let mut limiter = QueryRateLimiter::new(limits(1.0, 3.0, 0.0));

        let results = (0..5).map(|_| limiter.allow(SOURCE, 0)).collect::<Vec<_>>();

        assert_eq!(results, vec![true, true, true, false, false]);
    }

    #[test]
    fn refills_tokens_over_time() {
        let mut limiter = QueryRateLimiter::new(limits(2.0, 2.0, 0.0));
        assert!(limiter.allow(SOURCE, 0));
        assert!(limiter.allow(SOURCE, 0));
        assert!(!limiter.allow(SOURCE, 0));

        assert!(limiter.allow(SOURCE, 500));
        assert!(!limiter.allow(SOURCE, 500));
    }

    #[test]
    fn never_refills_beyond_the_burst() {
        let mut limiter = QueryRateLimiter::new(limits(10.0, 2.0, 0.0));

        let results = (0..4)
            .map(|_| limiter.allow(SOURCE, 3_600_000))
            .collect::<Vec<_>>();

        assert_eq!(results, vec![true, true, false, false]);
    }

    #[test]
    fn sources_are_limited_independently() {
        let mut limiter = QueryRateLimiter::new(limits(1.0, 1.0, 0.0));
        assert!(limiter.allow(SOURCE, 0));
        assert!(!limiter.allow(SOURCE, 0));

        assert!(limiter.allow(OTHER_SOURCE, 0));
    }

    #[test]
    fn colliding_sources_share_the_limit() {
        let colliding = source_colliding_with(SOURCE);
        let mut limiter = QueryRateLimiter::new(limits(1.0, 1.0, 0.0));
        assert!(limiter.allow(SOURCE, 0));

        assert!(!limiter.allow(colliding, 0));
        assert!(!limiter.allow(SOURCE, 0));
    }

    #[test]
    fn colliding_sources_cannot_reset_the_limit() {
        let colliding = source_colliding_with(SOURCE);
        let mut limiter = QueryRateLimiter::new(limits(1.0, 2.0, 0.0));

        let results = (0..6)
            .map(|round| {
                let source = if round % 2 == 0 { SOURCE } else { colliding };
                limiter.allow(source, 0)
            })
            .collect::<Vec<_>>();

        assert_eq!(results, vec![true, true, false, false, false, false]);
    }

    #[test]
    fn idle_slots_are_taken_over_by_colliding_sources() {
        let colliding = source_colliding_with(SOURCE);
        let mut limiter = QueryRateLimiter::new(limits(1.0, 1.0, 0.0));
        assert!(limiter.allow(SOURCE, 0));

        assert!(limiter.allow(colliding, 10_000));
        assert!(!limiter.allow(colliding, 10_000));
    }

    #[test]
    fn global_limit_holds_back_many_sources() {
        let mut limiter = QueryRateLimiter::new(limits(1.0, 1.0, 2.0));

        let results = (0..4u8)
            .map(|last| limiter.allow([192, 0, 2, last], 0))
            .collect::<Vec<_>>();

        assert_eq!(results, vec![true, true, false, false]);
    }

    #[test]
    fn a_single_source_cannot_drain_the_global_limit() {
        let mut limiter = QueryRateLimiter::new(limits(1.0, 1.0, 2.0));
        assert!(limiter.allow(SOURCE, 0));
        assert!(!limiter.allow(SOURCE, 0));
        assert!(!limiter.allow(SOURCE, 0));

        assert!(limiter.allow(OTHER_SOURCE, 0));
    }

    #[test]
    fn zero_rates_disable_the_limits() {
        let mut limiter = QueryRateLimiter::new(limits(0.0, 1.0, 0.0));

        assert!((0..100).all(|_| limiter.allow(SOURCE, 0)));
    }

    #[test]
    fn set_limits_applies_to_following_packets() {
        let mut limiter = QueryRateLimiter::new(limits(0.0, 1.0, 0.0));
        assert!(limiter.allow(SOURCE, 0));

        limiter.set_limits(limits(1.0, 1.0, 0.0));

        assert!(limiter.allow(SOURCE, 0));
        assert!(!limiter.allow(SOURCE, 0));
    }

    #[test]
    fn a_flood_of_spoofed_sources_is_held_back_by_the_global_limit() {
        let mut limiter = QueryRateLimiter::new(limits(1.0, 1.0, 5.0));

        let allowed = (0..10_000u32)
            .map(u32::to_be_bytes)
            .filter(|source| limiter.allow(*source, 0))
            .count();

        assert_eq!(allowed, 5);
    }

    #[test]
    fn summary_reports_drops_once_per_interval() {
        let mut limiter = QueryRateLimiter::new(limits(1.0, 1.0, 0.0));
        limiter.allow(SOURCE, 0);
        limiter.allow(SOURCE, 0);
        limiter.allow(SOURCE, 0);

        assert_eq!(limiter.take_dropped_summary(SUMMARY_INTERVAL_MS - 1), None);
        assert_eq!(limiter.take_dropped_summary(SUMMARY_INTERVAL_MS), Some(2));
        assert_eq!(limiter.take_dropped_summary(2 * SUMMARY_INTERVAL_MS), None);
    }

    #[test]
    fn default_limits_are_the_documented_defaults() {
        assert_eq!(QueryLimits::default(), QueryLimits::DEFAULT);
    }
}

#[cfg(test)]
mod query_gate_tests {
    use super::{QUERY_GATE, reset_query_gates, steam_query_allowed};
    use crate::{MAIN_ENGINE, engine::prelude::*, prelude::*};

    fn query_address(type_: netadrtype_t) -> netadr_t {
        NetadrBuilder::default()
            .type_(type_)
            .ip([192, 0, 2, 7])
            .build()
            .expect("this should not happen")
    }

    #[test]
    #[serial]
    fn steam_query_allowed_reports_the_dropped_packets_once_per_interval() {
        reset_query_gates();
        let address = query_address(netadrtype_t::NA_IP);
        let after_interval_ms = 1_000 + 61_000;

        MockEngineBuilder::default()
            .with_find_cvar(|_| true, |_| None, 0..)
            .run(|| {
                let main_engine = MAIN_ENGINE.load();
                let main_engine = main_engine
                    .as_ref()
                    .expect("the mock engine is registered by run");

                let allowed = (0..12)
                    .filter(|_| steam_query_allowed(&**main_engine, &address, 1_000))
                    .count();
                assert_eq!(allowed, 10);

                assert!(steam_query_allowed(
                    &**main_engine,
                    &address,
                    after_interval_ms
                ));
                assert_eq!(
                    QUERY_GATE
                        .limiter
                        .lock()
                        .take_dropped_summary(after_interval_ms),
                    None
                );
            });
    }
}
