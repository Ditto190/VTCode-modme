//! Session-level prompt-cache health monitoring.
//!
//! The harness already warns about individual cache-breaking events (reasoning
//! effort changes, idle-gap expiry, planning transitions). This module adds the
//! missing aggregate view: it observes normalized per-turn usage and fires a
//! session-scoped alert when the *hit rate* stays degraded, mirroring the
//! "monitor cache hit rate like uptime" discipline. Both runloops (the
//! `vtcode-core` `AgentRunner` and the binary unified runloop) feed the same
//! monitor through their usage-merge paths so thresholds and wording stay
//! identical.
//!
//! The monitor consumes the canonical harness [`Usage`] (as produced by
//! `normalized_turn_usage`), where `input_tokens` is always the total prompt
//! volume and `cached_input_tokens`/`cache_creation_tokens` carry the cache
//! signal, so provider reporting differences are already normalized away.

use crate::exec::events::Usage;

/// Minimum input tokens for a turn to count as cache signal.
///
/// Tiny turns re-pay little even on a full miss; counting them would let
/// cheap noise trip (or mask) the sustained-miss detector.
pub const MIN_INPUT_TOKENS_FOR_SIGNAL: u64 = 1_024;

/// Consecutive degraded turns that fire the sustained-miss alert.
pub const SUSTAINED_MISS_TURNS: u32 = 3;

/// Measured turns before the hit-rate alert arms.
pub const HIT_RATE_WINDOW_TURNS: u32 = 8;

/// Hit-rate floor (percent) that fires the low-hit-rate alert.
pub const HIT_RATE_FLOOR_PERCENT: f64 = 25.0;

/// A turn counts as degraded when its cache hit rate is below this floor.
/// Partial reuse still counts as healthy; only turns that re-pay (nearly)
/// everything move the needle.
pub const DEGRADED_TURN_HIT_RATE_PERCENT: f64 = 50.0;

/// Session-scoped prompt-cache health alert.
#[derive(Debug, Clone, PartialEq)]
pub enum CacheHealthAlert {
    /// Several consecutive measured turns re-paid (nearly) full input cost.
    SustainedMisses { consecutive: u32 },
    /// The hit rate over recent measured turns fell below the floor.
    LowHitRate { hit_rate: f64, measured_turns: u32 },
}

impl CacheHealthAlert {
    /// User-facing warning text. Mirrors the tone of the existing cache-gap
    /// and reasoning-effort advisories: what happened, what it costs, and the
    /// most likely causes to check.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::SustainedMisses { consecutive } => format!(
                "Prompt cache suffered {consecutive} consecutive near-full misses; recent requests re-paid full input cost. \
                 Check for prompt/tool-catalog churn (model switches, MCP refreshes, planning toggles) or long idle gaps expiring the provider cache."
            ),
            Self::LowHitRate { hit_rate, measured_turns } => format!(
                "Prompt cache hit rate is {hit_rate:.0}% over {measured_turns} measured turns (floor {HIT_RATE_FLOOR_PERCENT:.0}%). \
                 Requests are re-paying input cost; check for prompt/tool-catalog churn or idle gaps expiring the provider cache."
            ),
        }
    }
}

/// Rolling prompt-cache health for one session.
///
/// Fed with normalized per-turn [`Usage`]; turns without provider cache
/// metrics (providers that do not report them) or below the volume floor are
/// ignored so unmeasurable providers never trip the detector. Each alert
/// fires at most once per session, matching the one-shot style of the budget
/// and cache-gap warnings.
#[derive(Debug, Clone, Default)]
pub struct PromptCacheHealthMonitor {
    measured_turns: u32,
    window_read_tokens: u64,
    window_creation_tokens: u64,
    consecutive_degraded: u32,
    sustained_miss_fired: bool,
    low_rate_fired: bool,
}

impl PromptCacheHealthMonitor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one turn's normalized usage. Returns an alert the first time a
    /// degraded pattern is confirmed, `None` otherwise.
    pub fn record_turn(&mut self, usage: &Usage) -> Option<CacheHealthAlert> {
        if !Self::is_measured(usage) {
            return None;
        }
        self.measured_turns = self.measured_turns.saturating_add(1);
        self.window_read_tokens = self.window_read_tokens.saturating_add(usage.cached_input_tokens);
        self.window_creation_tokens = self.window_creation_tokens.saturating_add(usage.cache_creation_tokens);

        if Self::turn_hit_rate(usage) < DEGRADED_TURN_HIT_RATE_PERCENT {
            self.consecutive_degraded = self.consecutive_degraded.saturating_add(1);
        } else {
            self.consecutive_degraded = 0;
        }

        if !self.sustained_miss_fired && self.consecutive_degraded >= SUSTAINED_MISS_TURNS {
            self.sustained_miss_fired = true;
            return Some(CacheHealthAlert::SustainedMisses { consecutive: self.consecutive_degraded });
        }

        if !self.low_rate_fired
            && self.measured_turns >= HIT_RATE_WINDOW_TURNS
            && self.rolling_hit_rate() < HIT_RATE_FLOOR_PERCENT
        {
            self.low_rate_fired = true;
            return Some(CacheHealthAlert::LowHitRate {
                hit_rate: self.rolling_hit_rate(),
                measured_turns: self.measured_turns,
            });
        }

        None
    }

    /// Hit rate over all measured turns this session.
    #[must_use]
    pub fn rolling_hit_rate(&self) -> f64 {
        let total = self.window_read_tokens.saturating_add(self.window_creation_tokens);
        if total == 0 {
            return 100.0;
        }
        (self.window_read_tokens as f64 / total as f64) * 100.0
    }

    fn is_measured(usage: &Usage) -> bool {
        usage.input_tokens >= MIN_INPUT_TOKENS_FOR_SIGNAL
            && (usage.cached_input_tokens > 0 || usage.cache_creation_tokens > 0)
    }

    fn turn_hit_rate(usage: &Usage) -> f64 {
        usage.cache_hit_rate().map_or(100.0, |rate| rate * 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input_tokens: u64, cached: u64, creation: u64) -> Usage {
        Usage {
            input_tokens,
            cached_input_tokens: cached,
            cache_creation_tokens: creation,
            output_tokens: 10,
        }
    }

    #[test]
    fn ignores_turns_without_cache_metrics() {
        let mut monitor = PromptCacheHealthMonitor::new();
        for _ in 0..10 {
            assert_eq!(monitor.record_turn(&usage(50_000, 0, 0)), None);
        }
        assert_eq!(monitor.measured_turns, 0);
    }

    #[test]
    fn ignores_small_turns_below_volume_floor() {
        let mut monitor = PromptCacheHealthMonitor::new();
        for _ in 0..10 {
            assert_eq!(monitor.record_turn(&usage(100, 0, 200)), None);
        }
        assert_eq!(monitor.measured_turns, 0);
    }

    #[test]
    fn fires_sustained_miss_after_consecutive_degraded_turns() {
        let mut monitor = PromptCacheHealthMonitor::new();
        assert_eq!(monitor.record_turn(&usage(50_000, 0, 5_000)), None);
        assert_eq!(monitor.record_turn(&usage(50_000, 0, 5_000)), None);
        let alert = monitor.record_turn(&usage(50_000, 0, 5_000));
        assert_eq!(alert, Some(CacheHealthAlert::SustainedMisses { consecutive: 3 }));
    }

    #[test]
    fn healthy_turn_resets_consecutive_counter() {
        let mut monitor = PromptCacheHealthMonitor::new();
        assert_eq!(monitor.record_turn(&usage(50_000, 0, 5_000)), None);
        assert_eq!(monitor.record_turn(&usage(50_000, 0, 5_000)), None);
        assert_eq!(monitor.record_turn(&usage(50_000, 45_000, 5_000)), None);
        assert_eq!(monitor.record_turn(&usage(50_000, 0, 5_000)), None);
        assert_eq!(monitor.record_turn(&usage(50_000, 0, 5_000)), None);
        assert_eq!(
            monitor.record_turn(&usage(50_000, 0, 5_000)),
            Some(CacheHealthAlert::SustainedMisses { consecutive: 3 })
        );
    }

    #[test]
    fn alerts_are_one_shot_per_session() {
        let mut monitor = PromptCacheHealthMonitor::new();
        for _ in 0..3 {
            monitor.record_turn(&usage(50_000, 0, 5_000));
        }
        // Stay below the hit-rate window so only the sustained-miss alert
        // (already fired) could recur; it must not.
        for _ in 0..4 {
            assert_eq!(monitor.record_turn(&usage(50_000, 0, 5_000)), None);
        }
    }

    #[test]
    fn no_alert_when_rolling_rate_stays_above_floor() {
        let mut monitor = PromptCacheHealthMonitor::new();
        // Interleaved healthy turns keep the consecutive counter (needs 3 in a
        // row) at bay: D,D,H,D,D,H,D,D → max streak 2, rolling rate
        // (6*10000 + 2*30000) / (8*50000) = 30%, above the 25% floor.
        let mut alert = None;
        for index in 0..8 {
            let turn = if index % 3 == 2 {
                usage(50_000, 30_000, 20_000)
            } else {
                usage(50_000, 10_000, 40_000)
            };
            alert = monitor.record_turn(&turn).or(alert);
        }
        assert_eq!(alert, None);
    }

    #[test]
    fn low_hit_rate_fires_below_floor() {
        let mut monitor = PromptCacheHealthMonitor::new();
        // Barely-healthy turns (60%) reset the consecutive counter while the
        // rolling rate stays near zero: max streak 2, rolling rate after 8
        // measured turns (60000 read / 400000) = 15%.
        let mut alert = None;
        for index in 0..10 {
            let turn = if index % 3 == 2 {
                usage(50_000, 30_000, 20_000)
            } else {
                usage(50_000, 0, 50_000)
            };
            alert = monitor.record_turn(&turn).or(alert);
        }
        assert!(matches!(alert, Some(CacheHealthAlert::LowHitRate { .. })));
    }

    #[test]
    fn alert_messages_mention_likely_causes() {
        let sustained = CacheHealthAlert::SustainedMisses { consecutive: 3 }.message();
        assert!(sustained.contains("consecutive"));
        assert!(sustained.contains("planning toggles"));
        let low = CacheHealthAlert::LowHitRate { hit_rate: 10.0, measured_turns: 9 }.message();
        assert!(low.contains("10%"));
        assert!(low.contains("idle gaps"));
    }
}
