//! The search configuration — the embedded defaults, the user's overlay, and
//! the checks between them (`docs/tools/web-search.md` *Configuration*).
//!
//! Two rules make this different from `llm::config`'s provider overlay:
//!
//! - **Scale keys merge, list keys replace.** A user file that sets `results`
//!   or a source's `url` changes that one value; one that sets `order` or
//!   `cooldown_minutes` replaces the whole list. Appending to or truncating a
//!   fallback order from a partial edit is never what the user meant.
//! - **A document that cannot work is refused; an entry that is merely wrong
//!   is reported and skipped.** `order` naming an unconfigured source makes
//!   every search pay every timeout, so it is a [`RuleError`]. A misspelled
//!   key costs one entry and is a [`RulesIssue`], because the overlay's
//!   promise is that a bad block costs only that block.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

/// The `user_agent` a scraped source must send when the file names none.
/// Without a browser-shaped one, Bing answers a bot check — measured
/// (`docs/tools/web-search.md` §1). The shipped file sets its own; this is the
/// fallback, so **one** of them is the default and they cannot drift.
pub const DEFAULT_BROWSER_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) \
     AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

/// `browser_user_agent` when the file leaves it out.
fn default_browser_user_agent() -> String {
    DEFAULT_BROWSER_USER_AGENT.to_string()
}

/// Which transport a source speaks, and therefore how a request is phrased and
/// a reply read. The lowercase spellings are the `kind` values in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Exa's hosted MCP server (`web_search_exa`).
    Exa,
    /// Parallel's search MCP (`web_search`).
    Parallel,
    /// Firecrawl's MCP (`firecrawl_search`) — measured to refuse keyless
    /// access, so it is a key-required source (`docs/tools/web-search.md` §1).
    Firecrawl,
    /// Tavily's MCP (`tavily_search`, `tavily_extract`). Needs the
    /// `X-Tavily-Access-Mode: keyless` header for its free tier.
    Tavily,
    /// AnySearch's REST endpoint.
    Anysearch,
    /// Brave Search's REST API (needs a subscription token).
    Brave,
    /// DeepSeek's search endpoint (needs a key).
    Deepseek,
    /// A self-hosted SearXNG (needs an address).
    Searxng,
    /// Bing's HTML result page.
    Bing,
}

impl Kind {
    /// Whether this kind has a tool that returns a page's body — what
    /// `fetch.order` may name.
    #[must_use]
    pub fn fetches(self) -> bool {
        matches!(
            self,
            Kind::Exa | Kind::Parallel | Kind::Firecrawl | Kind::Tavily
        )
    }

    /// Whether a request to this kind goes through MCP rather than REST.
    #[must_use]
    pub fn is_mcp(self) -> bool {
        matches!(
            self,
            Kind::Exa | Kind::Parallel | Kind::Firecrawl | Kind::Tavily
        )
    }

    /// Whether this kind reads an HTML result page.
    #[must_use]
    pub fn is_scrape(self) -> bool {
        matches!(self, Kind::Bing)
    }

    /// The `kind` spelling, for an issue that quotes the file back.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Exa => "exa",
            Kind::Parallel => "parallel",
            Kind::Firecrawl => "firecrawl",
            Kind::Tavily => "tavily",
            Kind::Anysearch => "anysearch",
            Kind::Brave => "brave",
            Kind::Deepseek => "deepseek",
            Kind::Searxng => "searxng",
            Kind::Bing => "bing",
        }
    }
}

/// What a source needs before the walk may try it. An unmet need is a skip,
/// not a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Needs {
    /// Nothing.
    Nothing,
    /// Nothing, but a key spends its own quota instead of the shared one.
    KeyOptional,
    /// A key: skipped when absent.
    Key,
    /// An address (SearXNG): skipped when absent.
    Address,
}

/// One source in the file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// The human name — what the user's cell says (`8 results from Exa`).
    pub name: String,
    /// Which transport.
    pub kind: Kind,
    /// The endpoint. Empty only for a `needs = "address"` source, whose
    /// address the user supplies in settings.
    pub url: String,
    /// What it needs to be usable.
    pub needs: Needs,
    /// Whether the source honours the domain filter in the query itself
    /// (`site:`), rather than only on the results.
    #[serde(default)]
    pub site_operator: bool,
    /// Extra request headers, as `headers = { Name = "value" }`. Tavily's
    /// keyless tier needs one; nothing else ships with any, but a proxy or a
    /// private instance may.
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
}

/// Whether the source may fetch a page body — `fetch.order`'s member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetching {
    /// In order: ids of sources whose [`Kind::fetches`] is true.
    pub order: Vec<String>,
    /// A body shorter than this reads as a login wall, not a page.
    pub min_chars: usize,
    /// The absolute ceiling before paging; a page never exceeds it.
    pub max_chars: usize,
    /// Characters per returned page.
    pub page_chars: usize,
    /// One rescue's deadline.
    pub timeout: Duration,
}

/// The process-wide politeness budget (`docs/tools/web-search.md` §3). Values
/// are measured: see *Measured* §3 for the cap's cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Concurrent fetches, all sessions together.
    pub max_in_flight: usize,
    /// Concurrent fetches to one host.
    pub max_per_host: usize,
    /// Calls per minute, all hosts together.
    pub requests_per_minute: usize,
}

/// The search rules: a resolved, validated configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    /// The walk's order, after the merge.
    pub order: Vec<String>,
    /// How many hits to hand back.
    pub results: usize,
    /// Excerpt cap per hit, in characters.
    pub excerpt_chars: usize,
    /// One request's deadline.
    pub timeout: Duration,
    /// The cooldown ladder: the wait after the nth consecutive limit.
    pub cooldown: Vec<Duration>,
    /// The header a scraped source sends.
    pub browser_user_agent: String,
    /// Every configured source, by id.
    pub sources: std::collections::BTreeMap<String, Source>,
    /// The fetch tier.
    pub fetch: Fetching,
    /// The politeness budget.
    pub limits: Limits,
}

/// A configuration that cannot work — refused, with the file and the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError {
    /// The file that was read (or would have been).
    pub file: PathBuf,
    /// Why it cannot be used.
    pub why: String,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file.display(), self.why)
    }
}

/// Something wrong with one entry: reported, and the entry is skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesIssue(pub String);

impl std::fmt::Display for RulesIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The file's shape, before validation.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    order: Vec<String>,
    results: usize,
    excerpt_chars: usize,
    timeout_seconds: u64,
    cooldown_minutes: Vec<u64>,
    #[serde(default = "default_browser_user_agent")]
    browser_user_agent: String,
    sources: std::collections::BTreeMap<String, toml::Value>,
    fetch: FetchFile,
    #[serde(default)]
    limits: LimitsFile,
}

/// `[fetch]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FetchFile {
    order: Vec<String>,
    min_chars: usize,
    max_chars: usize,
    page_chars: usize,
    timeout_seconds: u64,
}

/// `[limits]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitsFile {
    #[serde(default = "default_max_in_flight")]
    max_in_flight: usize,
    #[serde(default = "default_max_per_host")]
    max_per_host: usize,
    #[serde(default = "default_requests_per_minute")]
    requests_per_minute: usize,
}

impl Default for LimitsFile {
    fn default() -> Self {
        Self {
            max_in_flight: default_max_in_flight(),
            max_per_host: default_max_per_host(),
            requests_per_minute: default_requests_per_minute(),
        }
    }
}

const fn default_max_in_flight() -> usize {
    8
}
const fn default_max_per_host() -> usize {
    4
}
const fn default_requests_per_minute() -> usize {
    60
}

/// Why a configuration cannot be used.
fn bad(file: &std::path::Path, why: impl Into<String>) -> RuleError {
    RuleError {
        file: file.to_path_buf(),
        why: why.into(),
    }
}

/// Read and validate the file at `path`.
///
/// # Errors
///
/// The file is missing, is not TOML, or fails a cross-field check
/// ([`RuleError`]).
pub fn load_rules(path: &std::path::Path) -> Result<Rules, RuleError> {
    let file = path.to_path_buf();
    let text = std::fs::read_to_string(&file).map_err(|err| bad(&file, err.to_string()))?;
    parse_rules(&text, &file)
}

/// Read the embedded defaults (the shipped file's own text).
///
/// # Errors
///
/// The embedded text fails to parse — a bug, caught by the unit tests.
pub fn load_builtin() -> Result<Rules, RuleError> {
    parse_rules(include_str!("../../web.toml"), &PathBuf::from("web.toml"))
}

/// Parse and validate one document.
///
/// # Errors
///
/// Not TOML, or a cross-field check fails ([`RuleError`]).
pub fn parse_rules(text: &str, file: &std::path::Path) -> Result<Rules, RuleError> {
    let file = file.to_path_buf();
    let parsed: File = toml::from_str(text).map_err(|err| bad(&file, err.to_string()))?;
    finish(parsed, &file)
}

/// `text` laid over `base`'s **text**, the way `providers.toml` layers a user
/// file over the shipped one. Returns the rules and whatever was reported.
///
/// # Errors
///
/// `base` or `text` fails a cross-field check ([`RuleError`]).
pub fn merge_rules(
    base: &str,
    text: &str,
    file: &std::path::Path,
) -> Result<(Rules, Vec<RulesIssue>), RuleError> {
    let file = file.to_path_buf();
    let mut merged: toml::Table =
        toml::from_str(base).map_err(|err| bad(&file, err.to_string()))?;
    let mut over: toml::Table = toml::from_str(text).map_err(|err| bad(&file, format!("{err}")))?;

    let mut issues = Vec::new();
    // Whatever sits beside the known tables is read by nothing — most often a
    // source written without its `sources.` prefix.
    // Reported, then dropped: the typed parse below would refuse the document
    // over a key that costs only itself.
    let stray: Vec<String> = over
        .keys()
        .filter(|key| {
            !matches!(
                key.as_str(),
                "order"
                    | "results"
                    | "excerpt_chars"
                    | "timeout_seconds"
                    | "cooldown_minutes"
                    | "browser_user_agent"
                    | "sources"
                    | "fetch"
                    | "limits"
            )
        })
        .cloned()
        .collect();
    for key in stray {
        issues.push(unread_key(&key));
        over.remove(&key);
    }

    // List keys replace whole; scale keys are copied over. `sources` and
    // `fetch`/`limits` are tables, merged key by key.
    for (key, value) in over {
        match key.as_str() {
            "sources" => {
                let over_sources = value
                    .as_table()
                    .ok_or_else(|| bad(&file, "sources is not a table"))?;
                let base_sources = merged
                    .get_mut("sources")
                    .and_then(toml::Value::as_table_mut)
                    .ok_or_else(|| bad(&file, "the built-in file has no sources"))?;
                for (id, block) in over_sources {
                    match block.as_table() {
                        Some(block_table) => {
                            let mut merged_block = base_sources
                                .get(id)
                                .and_then(toml::Value::as_table)
                                .cloned()
                                .unwrap_or_default();
                            for (field, field_value) in block_table {
                                if !matches!(
                                    field.as_str(),
                                    "name" | "kind" | "url" | "needs" | "site_operator" | "headers"
                                ) {
                                    issues.push(RulesIssue(format!(
                                        "sources.{id}.{field} is not a known key"
                                    )));
                                    continue;
                                }
                                merged_block.insert(field.clone(), field_value.clone());
                            }
                            base_sources.insert(id.clone(), toml::Value::Table(merged_block));
                        }
                        None => issues.push(RulesIssue(format!("sources.{id} is not a table"))),
                    }
                }
            }
            "fetch" | "limits" => {
                let over_table = value
                    .as_table()
                    .ok_or_else(|| bad(&file, format!("{key} is not a table")))?;
                let base_table = merged
                    .get_mut(&key)
                    .and_then(toml::Value::as_table_mut)
                    .ok_or_else(|| bad(&file, format!("the built-in file has no {key}")))?;
                for (field, field_value) in over_table {
                    base_table.insert(field.clone(), field_value.clone());
                }
            }
            _ => {
                merged.insert(key, value);
            }
        }
    }

    let parsed: File = merged
        .try_into()
        .map_err(|err| bad(&file, format!("{err}")))?;
    let rules = finish(parsed, &file)?;
    Ok((rules, issues))
}

/// The issue a key nothing reads raises.
fn unread_key(key: &str) -> RulesIssue {
    let fix = match key {
        "results_per_source" | "num_results" => " — the key is `results`".to_string(),
        "sources_dir" => " — sources go under [sources.<id>]".to_string(),
        "user_agent" => " — the key is `browser_user_agent`".to_string(),
        _ => String::new(),
    };
    RulesIssue(format!("{key} is not a known key{fix}"))
}

/// Validate the parsed document and resolve every derived value.
fn finish(parsed: File, file: &std::path::Path) -> Result<Rules, RuleError> {
    if parsed.results == 0 || parsed.excerpt_chars == 0 || parsed.timeout_seconds == 0 {
        return Err(bad(
            file,
            "results, excerpt_chars and timeout_seconds must be above 0",
        ));
    }
    if parsed.cooldown_minutes.is_empty() || parsed.cooldown_minutes.contains(&0) {
        return Err(bad(
            file,
            "cooldown_minutes needs at least one step, and no 0",
        ));
    }

    let mut sources = std::collections::BTreeMap::new();
    for (id, value) in &parsed.sources {
        match value.clone().try_into::<Source>() {
            Ok(source) => {
                // Cross-field: a URL and an address need are opposites.
                match (source.needs, source.url.is_empty()) {
                    (Needs::Address, false) => {
                        return Err(bad(
                            file,
                            format!("sources.{id} needs an address, so its url must be empty"),
                        ));
                    }
                    (Needs::Address, true) => {}
                    (_, true) => {
                        return Err(bad(file, format!("sources.{id} has no url")));
                    }
                    (_, false) => {}
                }
                sources.insert(id.clone(), source);
            }
            Err(err) => {
                return Err(bad(file, format!("sources.{id}: {err}")));
            }
        }
    }

    let mut seen = std::collections::BTreeSet::new();
    if let Some(twice) = parsed.order.iter().find(|id| !seen.insert(*id)) {
        return Err(bad(file, format!("order names {twice} twice")));
    }
    if let Some(missing) = parsed.order.iter().find(|id| !sources.contains_key(*id)) {
        return Err(bad(
            file,
            format!("order names an unknown source {missing}"),
        ));
    }

    let fetch = &parsed.fetch;
    if fetch.order.is_empty()
        || fetch.min_chars == 0
        || fetch.max_chars <= fetch.min_chars
        || fetch.page_chars == 0
        || fetch.timeout_seconds == 0
    {
        return Err(bad(
            file,
            "fetch needs an order, min_chars/page_chars/timeout_seconds above 0, \
             and max_chars above min_chars",
        ));
    }
    if let Some(id) = fetch
        .order
        .iter()
        .find(|id| sources.get(*id).is_none_or(|source| !source.kind.fetches()))
    {
        return Err(bad(
            file,
            format!("fetch.order names {id}, which cannot fetch pages"),
        ));
    }

    let limits = parsed.limits;
    if limits.max_in_flight == 0 || limits.max_per_host == 0 || limits.requests_per_minute == 0 {
        return Err(bad(file, "limits must all be above 0"));
    }

    Ok(Rules {
        order: parsed.order,
        results: parsed.results,
        excerpt_chars: parsed.excerpt_chars,
        timeout: Duration::from_secs(parsed.timeout_seconds),
        cooldown: parsed
            .cooldown_minutes
            .iter()
            .map(|minutes| Duration::from_secs(minutes * 60))
            .collect(),
        browser_user_agent: parsed.browser_user_agent,
        sources,
        fetch: Fetching {
            order: fetch.order.clone(),
            min_chars: fetch.min_chars,
            max_chars: fetch.max_chars,
            page_chars: fetch.page_chars,
            timeout: Duration::from_secs(fetch.timeout_seconds),
        },
        limits: Limits {
            max_in_flight: limits.max_in_flight,
            max_per_host: limits.max_per_host,
            requests_per_minute: limits.requests_per_minute,
        },
    })
}

#[cfg(test)]
mod tests;
