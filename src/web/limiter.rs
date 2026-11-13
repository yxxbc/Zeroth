//! The fetch budget and the page cache (`docs/tools/web-search.md`
//! *Concurrency*, decisions 2 and 4).
//!
//! A burst is cheap — 32 concurrent fetches of one host were tolerated, and a
//! per-host cap of 2 tripled a ten-page burst (*Measured* §3). So this is
//! **politeness**, not self-defence, and its numbers are a measured trade: 4
//! per host costs 1.9x where 2 costs 3.6x.
//!
//! Pure: the clock is passed in, so every boundary is a test.
//!
//! Two decisions worth stating where they are made:
//!
//! - **The budget is process-wide, not per session.** One session is bounded
//!   by the worker pool; four sessions are not, and four sessions each fetching
//!   eight pages at one host is indistinguishable from a load test.
//! - **A refusal is counted per call, not per redirect hop.** A hop is not a
//!   request the user asked for, and the ten-hop ceiling already bounds a chain.

use std::collections::HashMap;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// A rendered page, kept for a few minutes so `page: 2` costs nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedPage {
    /// The document, already converted to text.
    pub text: String,
    /// When it was fetched.
    pub at: Instant,
}

/// One fetch waiting to happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    /// When the slot was granted.
    pub at: Instant,
}

/// The budget's configuration — the `[limits]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Concurrent fetches, all sessions together.
    pub max_in_flight: usize,
    /// Concurrent fetches to one host.
    pub max_per_host: usize,
    /// Calls per minute, all hosts together.
    pub requests_per_minute: usize,
    /// How long a rendered page stays in the cache.
    pub cache_ttl: Duration,
    /// How many documents the cache holds.
    pub cache_docs: usize,
    /// How many bytes of rendered text the cache holds in total.
    pub cache_bytes: usize,
}

impl Default for Budget {
    fn default() -> Self {
        // The shipped numbers (`web.toml`), and the measured reasons for them
        // (*Measured* §3).
        Self {
            max_in_flight: 8,
            max_per_host: 4,
            requests_per_minute: 60,
            cache_ttl: Duration::from_secs(600),
            cache_docs: 32,
            cache_bytes: 2 * 1024 * 1024,
        }
    }
}

/// What the budget decided about one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granted {
    /// Go now.
    Now,
    /// Wait until this instant, then go — the caller asked too often.
    At(Instant),
}

/// The budget's mutable state.
#[derive(Debug)]
struct Inner {
    /// Fetches running, by host.
    in_flight: usize,
    per_host: HashMap<String, usize>,
    /// Request timestamps inside the last minute.
    recent: Vec<Instant>,
    /// The cache, oldest first by insertion.
    cache: Vec<(String, CachedPage)>,
    /// Bytes currently held.
    bytes: usize,
}

/// The process-wide budget and page cache.
#[derive(Debug)]
pub struct Limiter {
    budget: Budget,
    inner: Mutex<Inner>,
    /// Woken when a fetch releases its slot.
    freed: Condvar,
}

impl Limiter {
    /// A budget with `budget`'s numbers.
    #[must_use]
    pub fn new(budget: Budget) -> Self {
        Self {
            budget,
            inner: Mutex::new(Inner {
                in_flight: 0,
                per_host: HashMap::new(),
                recent: Vec::new(),
                cache: Vec::new(),
                bytes: 0,
            }),
            freed: Condvar::new(),
        }
    }

    /// The numbers this limiter enforces.
    #[must_use]
    pub fn budget(&self) -> Budget {
        self.budget
    }

    /// Ask for a slot for `host` at `now`.
    ///
    /// Returns [`Granted::Now`] and **takes the slot**, or [`Granted::At`] with
    /// the instant to come back — in which case nothing was taken, so a caller
    /// that gives up (Esc) costs nothing.
    pub fn acquire(&self, host: &str, now: Instant) -> Granted {
        let mut inner = self.lock();
        self.prune(&mut inner, now);
        if inner.in_flight >= self.budget.max_in_flight.max(1) {
            return Granted::At(now + Duration::from_millis(50));
        }
        let host_count = inner.per_host.get(host).copied().unwrap_or(0);
        if host_count >= self.budget.max_per_host.max(1) {
            return Granted::At(now + Duration::from_millis(50));
        }
        if self.budget.requests_per_minute > 0
            && inner.recent.len() >= self.budget.requests_per_minute
        {
            // The oldest request in the window is what frees a place.
            let free_at = inner
                .recent
                .first()
                .copied()
                .map_or(now, |at| at + Duration::from_secs(60));
            return Granted::At(free_at);
        }
        inner.in_flight += 1;
        *inner.per_host.entry(host.to_string()).or_insert(0) += 1;
        inner.recent.push(now);
        Granted::Now
    }

    /// Release the slot `host` held, waking one waiter.
    pub fn release(&self, host: &str) {
        let mut inner = self.lock();
        inner.in_flight = inner.in_flight.saturating_sub(1);
        if let Some(count) = inner.per_host.get_mut(host) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                inner.per_host.remove(host);
            }
        }
        drop(inner);
        self.freed.notify_one();
    }

    /// How many fetches are running right now.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.lock().in_flight
    }

    /// How many fetches to `host` are running right now.
    #[must_use]
    pub fn in_flight_at(&self, host: &str) -> usize {
        self.lock().per_host.get(host).copied().unwrap_or(0)
    }

    /// A cached document, if one is fresh at `now`.
    #[must_use]
    pub fn cached(&self, url: &str, now: Instant) -> Option<String> {
        let mut inner = self.lock();
        self.prune(&mut inner, now);
        inner
            .cache
            .iter()
            .find(|(key, _)| key == url)
            .map(|(_, page)| page.text.clone())
    }

    /// Keep a rendered document, evicting the oldest until both ceilings hold.
    pub fn keep(&self, url: &str, text: String, now: Instant) {
        let mut inner = self.lock();
        if let Some(position) = inner.cache.iter().position(|(key, _)| key == url) {
            let (_, old) = inner.cache.remove(position);
            inner.bytes = inner.bytes.saturating_sub(old.text.len());
        }
        inner.bytes += text.len();
        inner
            .cache
            .push((url.to_string(), CachedPage { text, at: now }));
        while inner.cache.len() > self.budget.cache_docs.max(1) {
            let (_, old) = inner.cache.remove(0);
            inner.bytes = inner.bytes.saturating_sub(old.text.len());
        }
        while inner.bytes > self.budget.cache_bytes && inner.cache.len() > 1 {
            let (_, old) = inner.cache.remove(0);
            inner.bytes = inner.bytes.saturating_sub(old.text.len());
        }
    }

    /// How many documents the cache holds.
    #[must_use]
    pub fn cached_count(&self) -> usize {
        self.lock().cache.len()
    }

    /// How many bytes of rendered text the cache holds.
    #[must_use]
    pub fn cached_bytes(&self) -> usize {
        self.lock().bytes
    }

    /// Drop expired entries and requests that fell out of the minute.
    fn prune(&self, inner: &mut MutexGuard<'_, Inner>, now: Instant) {
        let ttl = self.budget.cache_ttl;
        let mut bytes = 0;
        inner.cache.retain(|(_, page)| {
            let fresh = now.saturating_duration_since(page.at) < ttl;
            if fresh {
                bytes += page.text.len();
            }
            fresh
        });
        inner.bytes = bytes;
        inner
            .recent
            .retain(|at| now.saturating_duration_since(*at) < Duration::from_secs(60));
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests;
