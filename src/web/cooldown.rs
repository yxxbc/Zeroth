//! The cooldown ladder (`docs/tools/web-search.md` *The walk*): a source that
//! reports a spent quota is skipped for a while, and comes back on its own.
//!
//! Pure logic — the caller passes the current [`Instant`], so tests do not
//! wait. This is miyu's `search/cooldown.rs` and the same reason it is pure:
//! the ladder's whole job is *time*, and a time-dependent test that sleeps is
//! a flaky test.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How many consecutive limits each source has taken, and until when it is
/// held.
#[derive(Debug, Default, Clone)]
pub struct Cooldowns {
    /// The ladder: the wait after the **nth** consecutive limit. The last rung
    /// repeats for every further one.
    ladder: Vec<Duration>,
    /// Source id → (consecutive limits, held until).
    held: HashMap<String, (usize, Instant)>,
}

impl Cooldowns {
    /// A ladder with these rungs. An empty ladder means no cooldown at all —
    /// every source is always available.
    #[must_use]
    pub fn new(ladder: Vec<Duration>) -> Self {
        Self {
            ladder,
            held: HashMap::new(),
        }
    }

    /// When `id` comes back, if it is cooling at `now`.
    #[must_use]
    pub fn until(&self, id: &str, now: Instant) -> Option<Instant> {
        self.held
            .get(id)
            .map(|(_, until)| *until)
            .filter(|until| *until > now)
    }

    /// Record that `id` reported a spent quota at `now`: step the ladder up
    /// and return when it comes back.
    pub fn limited(&mut self, id: &str, now: Instant) -> Instant {
        let strikes = self.held.get(id).map_or(0, |(strikes, _)| *strikes);
        let wait = if self.ladder.is_empty() {
            Duration::ZERO
        } else {
            self.ladder
                .get(strikes)
                .or_else(|| self.ladder.last())
                .copied()
                .unwrap_or_default()
        };
        let until = now + wait;
        self.held.insert(id.to_string(), (strikes + 1, until));
        until
    }

    /// Record that `id` answered: clear its strikes, so the next limit starts
    /// the ladder again from the bottom.
    pub fn succeeded(&mut self, id: &str) {
        self.held.remove(id);
    }

    /// How many consecutive limits `id` has taken — what the user's row says
    /// when it explains why a source is resting.
    #[must_use]
    pub fn strikes(&self, id: &str) -> usize {
        self.held.get(id).map_or(0, |(strikes, _)| *strikes)
    }

    /// The ids this ladder knows about at all (cooling or recovered).
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.held.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Every source cooling at `now`, oldest deadline first, for the user's
    /// "all sources are resting" message.
    #[must_use]
    pub fn cooling(&self, now: Instant) -> Vec<(String, Instant)> {
        let mut cooling: Vec<(String, Instant)> = self
            .held
            .iter()
            .filter(|(_, (_, until))| *until > now)
            .map(|(id, (_, until))| (id.clone(), *until))
            .collect();
        cooling.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        cooling
    }
}

#[cfg(test)]
mod tests;
