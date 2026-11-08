//! Rate limiting for the packets that the engine hands over to the Steam game server API.
//!
//! Steam answers server queries (`A2S_INFO` and friends) with packets that are much larger than
//! the request, so a spoofed source address turns the server into a reflector. The limiter drops
//! the requests before Steam sees them.
//!
//! The per-source state lives in a fixed table without heap allocation, so a flood from many
//! spoofed addresses cannot grow the memory of the server.

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
