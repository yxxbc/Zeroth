//! The walk: try each source in order, honour the cooldown, tidy what comes
//! back (`docs/tools/web-search.md` *The walk*).
//!
//! The I/O is a parameter. [`Asker`] is a trait the caller implements — the
//! boundary sends a request, the tests answer from a table — so this module
//! decides *which* source is tried, *when* one is skipped, and *what* the
//! caller sees, with nothing that can block.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::web::cooldown::Cooldowns;
use crate::web::rules::{Needs, Rules};
use crate::web::sources::{Failure, Hit};
use crate::web::tidy::{Filter, tidy};

/// What a search asked for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Query {
    /// The words.
    pub text: String,
    /// Domains to keep, and to drop — [`Filter`]'s shape.
    pub filter: Filter,
}

/// What one source's attempt consists of: everything the walk needs to know,
/// with no reference to how the request is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt<'a> {
    /// The source id, as configured.
    pub id: &'a str,
    /// Its kind — which transport, and therefore which reply reader.
    pub kind: crate::web::rules::Kind,
    /// The endpoint.
    pub url: &'a str,
    /// Extra headers the source needs.
    pub headers: &'a std::collections::BTreeMap<String, String>,
    /// The query text, after any `site:` narrowing the source understands.
    pub text: &'a str,
    /// How many hits to ask for — doubled when the filter must run locally.
    pub want: usize,
    /// One request's deadline.
    pub timeout: Duration,
    /// The browser `user_agent`, for a scraped source.
    pub user_agent: &'a str,
}

/// Sends one request and reads the reply. The boundary implements this over
/// `reqwest`; a test implements it over a table.
pub trait Asker {
    /// Ask `attempt` and return its hits.
    ///
    /// # Errors
    ///
    /// The transport failed, the provider refused, or the reply did not parse
    /// ([`Failure`]).
    fn ask(&self, attempt: &Attempt<'_>) -> Result<Vec<Hit>, Failure>;
}

/// What a search produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The source that answered — its display name, for the user's cell.
    pub source: String,
    /// The tidied hits, possibly none.
    pub hits: Vec<Hit>,
}

/// Why a search produced nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missed {
    /// Every source was tried and every one failed; this is the last reason.
    Failed(String),
    /// Every source was resting, so nothing was tried.
    Cooling,
    /// Nothing in the configuration could be tried at all.
    NoSource,
}

impl std::fmt::Display for Missed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Missed::Failed(why) => f.write_str(why),
            Missed::Cooling => f.write_str(
                "every search source is cooling down after a rate limit — \
                 try again in a little while",
            ),
            Missed::NoSource => f.write_str("no search source is configured"),
        }
    }
}

/// The walk's state: the cooldowns, which outlive one search.
#[derive(Debug)]
pub struct Search {
    rules: Rules,
    cooldowns: Mutex<Cooldowns>,
    /// This process's Parallel session id — generated once, so its free-tier
    /// accounting stays stable across calls (`docs/tools/web-search.md`
    /// *Measured* §7: never hand-assembled by the model).
    session: String,
}

impl Search {
    /// A walk over `rules`, with a fresh cooldown ladder.
    #[must_use]
    pub fn new(rules: Rules) -> Self {
        let cooldowns = Cooldowns::new(rules.cooldown.clone());
        Self {
            rules,
            cooldowns: Mutex::new(cooldowns),
            session: session_id(),
        }
    }

    /// The rules this walk uses.
    #[must_use]
    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// The session id the sources see.
    #[must_use]
    pub fn session(&self) -> &str {
        &self.session
    }

    /// Every source cooling at `now`, soonest return first — what the user's
    /// "all sources are resting" message names.
    #[must_use]
    pub fn cooling(&self, now: Instant) -> Vec<(String, Instant)> {
        self.lock().cooling(now)
    }

    /// Search, walking the sources in order.
    ///
    /// # Errors
    ///
    /// Nothing answered — see [`Missed`].
    pub fn search(&self, query: &Query, asker: &dyn Asker) -> Result<Found, Missed> {
        self.search_at(query, asker, Instant::now())
    }

    /// [`search`](Self::search) with the clock passed in, so a test can move
    /// the cooldown boundaries without sleeping.
    ///
    /// # Errors
    ///
    /// Nothing answered — see [`Missed`].
    pub fn search_at(
        &self,
        query: &Query,
        asker: &dyn Asker,
        now: Instant,
    ) -> Result<Found, Missed> {
        let order = self.order(query);
        if order.is_empty() {
            return Err(Missed::NoSource);
        }
        let mut last: Option<String> = None;
        let mut attempted = false;
        for id in &order {
            if self.lock().until(id, now).is_some() {
                continue;
            }
            let source = &self.rules.sources[id];
            attempted = true;
            let attempt = Attempt {
                id,
                kind: source.kind,
                url: &source.url,
                headers: &source.headers,
                text: &query.text,
                want: self.want(query),
                timeout: self.rules.timeout,
                user_agent: &self.rules.browser_user_agent,
            };
            match asker.ask(&attempt) {
                Ok(hits) => {
                    // A keyless answer with **no** hits is still an answer: it
                    // clears the cooldown and stops the walk, so an obscure
                    // query does not burn every source's quota looking for a
                    // hit that is not there (docs/tools/web-search.md *The
                    // walk*).
                    self.lock().succeeded(id);
                    return Ok(Found {
                        source: source.name.clone(),
                        hits: tidy(
                            hits,
                            &query.filter,
                            self.rules.results,
                            self.rules.excerpt_chars,
                        ),
                    });
                }
                Err(Failure::Limited) => {
                    self.lock().limited(id, now);
                    last = Some(format!("{} is rate limited", source.name));
                }
                Err(Failure::Failed(why)) => {
                    last = Some(format!("{}: {why}", source.name));
                }
            }
        }
        if !attempted {
            return Err(Missed::Cooling);
        }
        Err(match last {
            Some(why) => Missed::Failed(why),
            None => Missed::Cooling,
        })
    }

    /// The order this search walks: the configured order, minus the sources
    /// whose `needs` is unmet.
    fn order(&self, _query: &Query) -> Vec<String> {
        self.rules
            .order
            .iter()
            .filter(|id| {
                self.rules.sources.get(*id).is_some_and(|source| {
                    // A `needs = "key"` source is skipped until a secret
                    // supplies one, and `needs = "address"` until the user
                    // configures an address. Neither has a source of truth in
                    // this module — keys are `/secrets`, the address is a
                    // setting — so both are treated as unmet here and the
                    // boundary will pass what it has when it lands. `nothing`
                    // and `key_optional` are always usable.
                    matches!(source.needs, Needs::Nothing | Needs::KeyOptional)
                })
            })
            .cloned()
            .collect()
    }

    /// How many hits to ask for: doubled when the filter has to run locally,
    /// so the count still comes out right after it does.
    fn want(&self, query: &Query) -> usize {
        if query.filter.is_empty() {
            self.rules.results
        } else {
            self.rules.results * 2
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Cooldowns> {
        self.cooldowns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// This process's session id: the pid and the wall clock, hex — miyu's shape.
/// Stable for the process, never user-supplied.
fn session_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{:016x}{:016x}", std::process::id(), nanos as u64)
}

#[cfg(test)]
mod tests;
