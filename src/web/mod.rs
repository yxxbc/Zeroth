//! Web search and page-reading: the pure model behind the `web_search` and
//! `web_fetch` tools (`docs/tools/web-search.md`).
//!
//! Nothing here performs I/O. Every function takes text in and gives a value
//! back — the config parse, the cooldown ladder, each source's reply reader,
//! the result tidier, the address gate, the charset decode — so the whole
//! module is testable with `cargo test --lib web::` and no network. The one
//! place a request is sent is the boundary (`crate::llm::web_exec`).
//!
//! The source set and its order are **measured**, not assumed: see
//! `docs/tools/web-search.md` *Measured* §1 for the probe that decided which
//! providers answer keyless from an ordinary address.

pub mod cooldown;
pub mod fetch;
pub mod guard;
pub mod limiter;
pub mod rules;
pub mod scrape;
pub mod sources;
pub mod tidy;
pub mod walk;

pub use cooldown::Cooldowns;
pub use guard::{
    AddressClass, GuardError, classify, is_global, parse_target, unwrap_bing_redirect,
};
pub use limiter::{Budget, Granted, Limiter};
pub use rules::{
    DEFAULT_BROWSER_USER_AGENT, Fetching, Kind, Limits, Needs, RuleError, Rules, RulesIssue,
    Source, load_builtin, load_rules, load_session_rules, merge_rules,
};
pub use scrape::bing;
pub use sources::{Failure, Hit, reply_hits};
pub use tidy::{Filter, cut_chars, host_of, tidy};
pub use walk::{Asker, Attempt, Found, Missed, Query, Search};
