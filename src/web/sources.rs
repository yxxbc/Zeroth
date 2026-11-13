//! Reading a source's reply into [`Hit`]s (`docs/tools/web-search.md`
//! *Measured* §1, §7): the MCP text form Exa uses, the JSON shapes the other
//! MCP servers and REST endpoints return, and the failure classifier.
//!
//! Nothing here sends a request. A body in, hits or a [`Failure`] out.

use crate::web::rules::Kind;

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hit {
    /// The page's title.
    pub title: String,
    /// Its URL. A hit without one is dropped.
    pub url: String,
    /// An excerpt, cut to the cap by the caller's tidier.
    pub snippet: Option<String>,
    /// The publication date, `YYYY-MM-DD`, when the source gave one.
    pub date: Option<String>,
}

/// Why a source's attempt failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The provider said its quota is spent. The walk puts the source in
    /// cooldown and moves on.
    Limited,
    /// Anything else, carrying the provider's own words.
    Failed(String),
}

/// The provider word list that means "quota spent" (`docs/tools/web-search.md`
/// *Measured* §4). Anchored on word boundaries because a substring match fired
/// on `quota` inside a Wikipedia category list in a **successful** answer.
const LIMITED_PATTERNS: &[&str] = &[
    r"(?i)\brate[ -]?limit",
    r"(?i)\btoo many requests",
    r"(?i)\bquotas?\b",
    r"(?i)\bcredits?\b",
    r"(?i)\busage limit",
    r"(?i)\blimit reached",
    r"(?i)\bmonthly\b.*\blimit\b",
    r"(?i)\bhttp 40[29]\b",
    r"(?i)\binsufficient balance",
];

impl Failure {
    /// Classify a message. **Only call this for a failure** — a non-2xx, a
    /// JSON-RPC `error`, or `isError: true` — never for a successful body: a
    /// good answer can contain any of these words (Measured §4).
    #[must_use]
    pub fn from_text(text: &str) -> Failure {
        let limited = LIMITED_PATTERNS
            .iter()
            .any(|pattern| matches_limit(pattern, text));
        if limited {
            Failure::Limited
        } else {
            Failure::Failed(text.chars().take(200).collect())
        }
    }

    /// Classify an HTTP status, before any body is read.
    #[must_use]
    pub fn from_status(status: u16) -> Option<Failure> {
        match status {
            429 | 402 => Some(Failure::Limited),
            200..=299 => None,
            other => Some(Failure::Failed(format!("HTTP {other}"))),
        }
    }
}

/// Whether `text` matches one anchored limit pattern.
///
/// The patterns are the ones in [`LIMITED_PATTERNS`]; a pattern that fails to
/// compile can match nothing, so a typo there degrades to "not limited" rather
/// than panicking on a tool call.
fn matches_limit(pattern: &str, text: &str) -> bool {
    regex::Regex::new(pattern).is_ok_and(|re| re.is_match(text))
}

/// Read a source's reply.
///
/// # Errors
///
/// A body that is not the shape its `kind` promises ([`Failure`]).
pub fn reply_hits(kind: Kind, text: &str) -> Result<Vec<Hit>, Failure> {
    match kind {
        Kind::Exa => Ok(exa_text(text)),
        Kind::Parallel => {
            let value = json(text)?;
            Ok(list(&value["results"])
                .map(|one| {
                    hit(
                        one,
                        "url",
                        "title",
                        joined(&one["excerpts"]),
                        day(&one["publish_date"]),
                    )
                })
                .collect())
        }
        Kind::Firecrawl => {
            let value = json(text)?;
            let web = if value["data"].is_array() {
                &value["data"]
            } else {
                &value["data"]["web"]
            };
            Ok(list(web)
                .map(|one| hit(one, "url", "title", string(&one["description"]), None))
                .collect())
        }
        Kind::Tavily => {
            let value = json(text)?;
            Ok(list(&value["results"])
                .map(|one| {
                    hit(
                        one,
                        "url",
                        "title",
                        string(&one["content"]),
                        day(&one["published_date"]),
                    )
                })
                .collect())
        }
        Kind::Anysearch => {
            let value = json(text)?;
            let code = value["code"].as_i64();
            if code.is_some_and(|code| code != 0) {
                let said = value["message"].as_str().unwrap_or("not ok");
                return Err(Failure::from_text(said));
            }
            Ok(list(&value["data"]["results"])
                .map(|one| {
                    let snippet = string(&one["content"]).or_else(|| string(&one["snippet"]));
                    hit(one, "url", "title", snippet, None)
                })
                .collect())
        }
        Kind::Brave => {
            let value = json(text)?;
            Ok(list(&value["web"]["results"])
                .map(|one| {
                    hit(
                        one,
                        "url",
                        "title",
                        string(&one["description"]),
                        day(&one["page_age"]),
                    )
                })
                .collect())
        }
        Kind::Deepseek => {
            let value = json(text)?;
            Ok(list(&value["results"])
                .map(|one| {
                    let snippet = string(&one["summary"]).or_else(|| string(&one["snippet"]));
                    hit(one, "url", "title", snippet, day(&one["date"]))
                })
                .collect())
        }
        Kind::Searxng => {
            let value = json(text)?;
            Ok(list(&value["results"])
                .map(|one| {
                    hit(
                        one,
                        "url",
                        "title",
                        string(&one["content"]),
                        day(&one["publishedDate"]),
                    )
                })
                .collect())
        }
        Kind::Bing => Err(Failure::Failed(
            "bing results are read from HTML, not this path".to_string(),
        )),
    }
}

/// Exa's text form: a record per `Title: ` line, the next line `URL: `,
/// `Published: ` and then `Highlights: ` up to the next record.
fn exa_text(text: &str) -> Vec<Hit> {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = (0..lines.len())
        .filter(|&at| {
            lines[at].starts_with("Title: ")
                && lines
                    .get(at + 1)
                    .is_some_and(|next| next.starts_with("URL: "))
        })
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(n, &start)| {
            let end = starts.get(n + 1).copied().unwrap_or(lines.len());
            let record = &lines[start..end];
            let field = |name: &str| {
                record
                    .iter()
                    .find_map(|line| line.strip_prefix(name))
                    .map(str::trim)
                    .filter(|value| !value.is_empty() && *value != "N/A")
                    .map(str::to_string)
            };
            let snippet = record
                .iter()
                .position(|line| matches!(line.trim(), "Highlights:" | "Text:"))
                .map(|at| record[at + 1..].join("\n").trim().to_string())
                .filter(|text| !text.is_empty());
            Hit {
                title: field("Title: ").unwrap_or_default(),
                url: field("URL: ").unwrap_or_default(),
                snippet,
                date: field("Published: ").and_then(|text| iso_day(&text)),
            }
        })
        .filter(|hit| !hit.url.is_empty())
        .collect()
}

/// Read a body as JSON, or fail with the body's head for the message.
fn json(text: &str) -> Result<serde_json::Value, Failure> {
    serde_json::from_str(text.trim()).map_err(|error| {
        Failure::Failed(format!(
            "{error}: {}",
            text.chars().take(200).collect::<String>()
        ))
    })
}

/// Informational — the test-only witness that one hit is well formed.
#[cfg(test)]
impl Hit {
    fn is_complete(&self) -> bool {
        !self.url.is_empty() && !self.title.is_empty()
    }
}

/// The items of an array value; a non-array is empty.
fn list(value: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    value.as_array().into_iter().flatten()
}

/// One hit, taking the fields by key.
fn hit(
    one: &serde_json::Value,
    url: &str,
    title: &str,
    snippet: Option<String>,
    date: Option<String>,
) -> Hit {
    Hit {
        title: string(&one[title]).unwrap_or_default(),
        url: string(&one[url]).unwrap_or_default(),
        snippet,
        date,
    }
}

/// A non-empty trimmed string.
fn string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// Several strings joined by newlines.
fn joined(value: &serde_json::Value) -> Option<String> {
    let parts: Vec<&str> = list(value)
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .collect();
    let text = parts.join("\n");
    (!text.trim().is_empty()).then_some(text)
}

/// A date from a JSON value, as `YYYY-MM-DD`.
fn day(value: &serde_json::Value) -> Option<String> {
    string(value).and_then(|text| iso_day(&text))
}

/// The first ten characters when they are `YYYY-MM-DD`; anything else is not
/// a date the cell can show.
fn iso_day(text: &str) -> Option<String> {
    let day: String = text.chars().take(10).collect();
    let shaped = day.char_indices().all(|(at, c)| match at {
        4 | 7 => c == '-',
        _ => c.is_ascii_digit(),
    });
    (day.len() == 10 && shaped).then_some(day)
}

#[cfg(test)]
mod tests;
