//! The web tools' I/O boundary: the request a source is asked with, and the
//! reply read back (`docs/tools/web-search.md`).
//!
//! Everything decidable lives in [`crate::web`] — the order, the cooldown, the
//! domain filter, the address gate, the reply shapes. This module is the one
//! place a socket is opened, and it is deliberately thin: build a request from
//! an [`Attempt`], send it, and hand the body to the pure reader.
//!
//! Boundary code like `llm::exec` and `term`: the shapes are unit-tested
//! through [`crate::web::sources::reply_hits`], and the transport itself is
//! covered by the loopback tests in `tests/web_search.rs`.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::blocking::Response;

use crate::web::rules::Kind;
use crate::web::sources::{Failure, Hit, reply_hits};
use crate::web::walk::{Asker, Attempt};

/// How much of any reply is read before it is cut. Search answers are tens of
/// kilobytes; a page can be megabytes, and the cap is what keeps a hostile
/// reply from filling memory before the size check runs.
const REPLY_MAX_BYTES: usize = 4 * 1024 * 1024;

/// The client a request goes through — the crate's cached one, so a fetch does
/// not pay a fresh TLS handshake (`llm::http_client`).
pub struct HttpAsker {
    client: reqwest::blocking::Client,
    /// This process's session id, sent to the sources whose free tier is
    /// accounted per session.
    session: String,
    /// A key per source id, from `/secrets` — the boundary's to supply.
    keys: BTreeMap<String, String>,
}

impl HttpAsker {
    /// Build the asker over the crate's shared HTTP client.
    ///
    /// # Errors
    ///
    /// The client cannot be built (a broken CA bundle).
    pub fn new(session: String, timeout: Duration) -> Result<Self, crate::llm::LlmError> {
        Ok(Self {
            client: crate::llm::http_client(timeout)?,
            session,
            keys: BTreeMap::new(),
        })
    }

    /// Supply `source_id`'s key. Read from `/secrets` by the caller — never
    /// from this file, and never printed (`docs/state/secrets.md`).
    #[must_use]
    pub fn with_key(mut self, source_id: &str, key: impl Into<String>) -> Self {
        self.keys.insert(source_id.to_string(), key.into());
        self
    }
}

impl Asker for HttpAsker {
    fn ask(&self, attempt: &Attempt<'_>) -> Result<Vec<Hit>, Failure> {
        let request = build_request_with(
            &self.client,
            attempt,
            &self.session,
            self.keys.get(attempt.id),
        )?;
        let response = request
            .send()
            .map_err(|error| Failure::Failed(transport_reason(error)))?;
        let status = response.status().as_u16();
        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/event-stream"));
        let body = read_capped(response);
        if let Some(failure) = Failure::from_status(status) {
            // The body may carry the provider's own wording (Exa's 429 says
            // which limit it hit); a body that says nothing keeps the status.
            return Err(match failure {
                Failure::Limited => Failure::Limited,
                other => {
                    let said = if body.trim().is_empty() {
                        other
                    } else {
                        Failure::from_text(&body)
                    };
                    match said {
                        Failure::Limited => Failure::Limited,
                        Failure::Failed(why) => Failure::Failed(why),
                    }
                }
            });
        }
        let text = if is_sse {
            last_sse_result(&body)?
        } else {
            body
        };
        read_reply(attempt.kind, &text)
    }
}

/// Build the request for one attempt. Pure, so the shapes are testable without
/// a socket.
///
/// # Errors
///
/// The URL is not usable ([`Failure::Failed`]).
pub fn build_request(
    attempt: &Attempt<'_>,
    session: &str,
    key: Option<&String>,
) -> Result<reqwest::blocking::RequestBuilder, Failure> {
    let client = crate::llm::http_client(attempt.timeout)
        .map_err(|error| Failure::Failed(error.to_string()))?;
    build_request_with(&client, attempt, session, key)
}

/// [`build_request`] over a client the caller owns — the asker's, so every
/// call shares one connection pool and one TLS setup (`llm::http_client`).
///
/// # Errors
///
/// The URL is not usable ([`Failure::Failed`]).
pub fn build_request_with(
    client: &reqwest::blocking::Client,
    attempt: &Attempt<'_>,
    session: &str,
    key: Option<&String>,
) -> Result<reqwest::blocking::RequestBuilder, Failure> {
    let method = http_method(attempt.kind);
    let mut headers: Vec<(&str, String)> = Vec::new();
    for (name, value) in attempt.headers {
        headers.push((name.as_str(), value.clone()));
    }
    if let Some(key) = key {
        match attempt.kind {
            Kind::Brave => headers.push(("X-Subscription-Token", key.clone())),
            Kind::Exa => {}
            _ => headers.push(("Authorization", format!("Bearer {key}"))),
        }
    }
    let mut builder = client.request(method, attempt.url);
    for (name, value) in &headers {
        builder = builder.header(*name, value.as_str());
    }
    if attempt.kind.is_scrape() {
        builder = builder.header("User-Agent", attempt.user_agent);
    }
    match request_body(attempt, session) {
        Some(body) => Ok(builder
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .body(body)),
        None => Ok(builder.header("Accept", "application/json")),
    }
}

/// Which HTTP method a kind uses.
#[must_use]
pub fn http_method(kind: Kind) -> reqwest::Method {
    match kind {
        // Every REST search endpoint this crate knows takes a POST body; the
        // scraped pages are plain GETs, and Brave's is a GET with a token.
        Kind::Brave | Kind::Searxng | Kind::Bing => reqwest::Method::GET,
        _ => reqwest::Method::POST,
    }
}

/// The JSON body a kind sends, or `None` for a GET.
#[must_use]
pub fn request_body(attempt: &Attempt<'_>, session: &str) -> Option<String> {
    let text = attempt.text;
    if attempt.kind.is_scrape() {
        return None;
    }
    let value = match attempt.kind {
        Kind::Exa => serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "web_search_exa",
                       "arguments": {"query": text, "numResults": attempt.want, "objective": text}}
        }),
        Kind::Parallel => serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "web_search",
                       "arguments": {"objective": text, "search_queries": [text],
                                     "session_id": session}}
        }),
        Kind::Tavily => serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "tavily_search",
                       "arguments": {"query": text, "max_results": attempt.want}}
        }),
        Kind::Firecrawl => serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "firecrawl_search",
                       "arguments": {"query": text, "limit": attempt.want}}
        }),
        Kind::Anysearch => serde_json::json!({
            "query": text,
            // Its own ceiling: anything above ten is refused or clamped
            // (docs/tools/web-search.md *Measured* §1).
            "max_results": attempt.want.min(10)
        }),
        Kind::Deepseek => serde_json::json!({"query": text, "count": attempt.want.min(50)}),
        Kind::Brave | Kind::Searxng | Kind::Bing => return None,
    };
    Some(value.to_string())
}

/// A reply body, capped before it is parsed.
fn read_capped(response: Response) -> String {
    use std::io::Read;
    let mut text = String::new();
    let _ = response
        .take(REPLY_MAX_BYTES as u64)
        .read_to_string(&mut text);
    text
}

/// The last SSE event carrying a `result` or an `error` — measured: Tavily
/// answers SSE and Parallel answers plain JSON (`Measured` §7).
///
/// # Errors
///
/// No event carried a JSON-RPC answer ([`Failure`]).
pub fn last_sse_result(body: &str) -> Result<String, Failure> {
    let mut answer: Option<serde_json::Value> = None;
    for line in body.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(data)
            && (value.get("result").is_some() || value.get("error").is_some())
        {
            answer = Some(value);
        }
    }
    let Some(value) = answer else {
        return Err(Failure::from_text(body));
    };
    if let Some(error) = value.get("error") {
        let said = error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("the server reported an error");
        return Err(Failure::from_text(said));
    }
    // An MCP `tools/call` result wraps its payload as text content; a REST
    // endpoint's body is the payload itself.
    let result = &value["result"];
    if result.get("isError").and_then(serde_json::Value::as_bool) == Some(true) {
        let said = result
            .get("content")
            .and_then(|content| content.get(0))
            .and_then(|first| first.get("text"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("the server reported an error");
        return Err(Failure::from_text(said));
    }
    if let Some(text) = result
        .get("content")
        .and_then(|content| content.get(0))
        .and_then(|first| first.get("text"))
        .and_then(serde_json::Value::as_str)
    {
        return Ok(text.to_string());
    }
    Ok(result.to_string())
}

/// Read a reply's body into hits — the pure reader, plus the MCP envelope.
///
/// # Errors
///
/// The body is not the shape its kind promises ([`Failure`]).
pub fn read_reply(kind: Kind, text: &str) -> Result<Vec<Hit>, Failure> {
    let trimmed = text.trim();
    // A JSON-RPC answer arrives either bare or wrapped; unwrap it the same way
    // an SSE body is unwrapped, so an MCP server that answers `application/json`
    // and one that answers `text/event-stream` take one path.
    if trimmed.starts_with('{')
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed)
        && (value.get("result").is_some() || value.get("error").is_some())
    {
        let inner = last_sse_result(&format!("data: {trimmed}"))?;
        return read_payload(kind, &inner);
    }
    read_payload(kind, trimmed)
}

/// The payload itself: a scraped page, or whatever the source's own reader
/// wants.
fn read_payload(kind: Kind, text: &str) -> Result<Vec<Hit>, Failure> {
    if kind.is_scrape() {
        return Ok(crate::web::scrape::bing(text));
    }
    reply_hits(kind, text)
}

/// A transport error as the model reads it — reqwest's own text, minus the
/// URL it repeats back (a search URL carries the query, and the query is
/// already in the transcript).
fn transport_reason(error: reqwest::Error) -> String {
    let timeout = error.is_timeout();
    let connect = error.is_connect();
    let without_url = error.without_url();
    if timeout {
        "the request timed out".to_string()
    } else if connect {
        "could not connect".to_string()
    } else {
        without_url.to_string()
    }
}

#[cfg(test)]
mod tests;
