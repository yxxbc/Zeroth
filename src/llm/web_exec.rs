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

impl HttpAsker {
    /// Ask a source for a page's **body** rather than a search result — the
    /// rescue path (`docs/tools/web-search.md` *`web_fetch`*: a page this
    /// machine cannot read is retried through the sources that can fetch).
    ///
    /// # Errors
    ///
    /// The transport failed or the source refused.
    pub fn fetch_page(&self, attempt: &Attempt<'_>) -> Result<String, Failure> {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {
                "name": match attempt.kind {
                    Kind::Exa => "web_fetch_exa",
                    Kind::Firecrawl => "firecrawl_scrape",
                    _ => "web_fetch",
                },
                "arguments": match attempt.kind {
                    Kind::Exa => serde_json::json!({"urls": [attempt.text], "maxCharacters": 200_000}),
                    Kind::Firecrawl => serde_json::json!({"url": attempt.text}),
                    _ => serde_json::json!({"urls": [attempt.text]}),
                }
            }
        })
        .to_string();
        let request = self
            .client
            .post(attempt.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .timeout(attempt.timeout)
            .body(body);
        let request = attempt
            .headers
            .iter()
            .fold(request, |request, (name, value)| {
                request.header(name.as_str(), value.as_str())
            });
        let response = request
            .send()
            .map_err(|error| Failure::Failed(transport_reason(error)))?;
        let status = response.status().as_u16();
        if let Some(failure) = Failure::from_status(status) {
            return Err(failure);
        }
        let raw = read_capped(response);
        let text = last_sse_result(&raw)?;
        Ok(text)
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

/// Fetch one URL and render it for the model: the address gate, the request,
/// the charset, the conversion and the page
/// (`docs/tools/web-search.md` *`web_fetch`*).
///
/// A failure no rescue source could fix is reported with the **original**
/// reason, not the rescuers'.
///
/// # Errors
///
/// The URL is refused, unreachable, or not something this crate reads as text
/// — the message is what the model reads.
pub fn fetch_document(
    url: &str,
    page_number: usize,
    search: Option<&crate::web::walk::Search>,
) -> Result<String, String> {
    use std::time::Duration;

    let rules = search.map(|search| search.rules());
    let page_chars = rules.map_or(16_000, |rules| rules.fetch.page_chars);
    let timeout = rules.map_or(Duration::from_secs(45), |rules| rules.fetch.timeout);
    let (text, via) = match fetch_once(url, timeout) {
        Ok((text, shown)) => {
            // A body that came back empty or login-wall short is worth asking
            // a source to fetch instead.
            let want_rescue =
                rules.is_some_and(|rules| text.chars().count() < rules.fetch.min_chars);
            if want_rescue {
                match rescue(url, search) {
                    Some((rescued, source)) => (rescued, Some(source)),
                    None => (text, shown),
                }
            } else {
                (text, shown)
            }
        }
        Err(reason) => match rescue(url, search) {
            Some((rescued, source)) => (rescued, Some(source)),
            None => return Err(reason),
        },
    };

    let document = crate::web::fetch::pages(&text, page_chars);
    let page = crate::web::fetch::page(&document, page_number);
    let mut out = String::new();
    if let Some(source) = &via {
        out.push_str(&format!("via {source}\n\n"));
    }
    out.push_str(page.text.trim_end());
    if page.has_more() {
        out.push_str(&format!(
            "\n\n(page {} of {} — ask for page: {})",
            page.number,
            page.total,
            page.number + 1
        ));
    }
    Ok(out)
}

/// One fetch: gate the URL, resolve it, pin the address the gate approved, and
/// return the body as text plus what the user's cell should say about where it
/// went.
fn fetch_once(url: &str, timeout: Duration) -> Result<(String, Option<String>), String> {
    use crate::web::fetch::{Media, charset_of, decode, looks_like_html, media_from_content_type};
    use crate::web::guard::{AddressClass, GuardError, classify, parse_target};

    let target = parse_target(url).map_err(|error| describe_guard(&error))?;
    // The gate resolves, and every address must be routable before anything is
    // sent. The connection is pinned to the address that passed, so a second
    // DNS answer cannot swap it (`docs/tools/web-search.md`).
    let addresses = resolve(&target.host, target.port).map_err(|error| error.to_string())?;
    if addresses.is_empty() {
        return Err(format!("{} did not resolve", target.host));
    }
    for address in &addresses {
        let class = classify(*address);
        if class != AddressClass::Global {
            return Err(describe_guard(&GuardError::NotGlobal {
                host: target.host.clone(),
                address: *address,
                class,
            }));
        }
    }
    let pinned = std::net::SocketAddr::new(addresses[0], target.port);
    let host_for_pin = target.host.clone();
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10))
        .resolve(&host_for_pin, pinned)
        .build()
        .map_err(|error| format!("could not build the request: {error}"))?;

    let response = client
        .get(url)
        .header("User-Agent", crate::web::rules::DEFAULT_BROWSER_USER_AGENT)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.8",
        )
        .send()
        .map_err(|error| format!("could not fetch: {}", transport_reason(error)))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = read_capped(response);
    let bytes = body.as_bytes();

    let charset = charset_of(&content_type, bytes);
    let text = decode(bytes, charset);
    let media = match media_from_content_type(&content_type) {
        Media::Unknown if looks_like_html(&text) => Media::Html,
        media => media,
    };
    match media {
        Media::Html => Ok((html_to_text(&text), None)),
        Media::Text => Ok((text, None)),
        Media::Binary(what) => Err(format!("{url} is {what}, which this tool does not read")),
        Media::Unknown => Err(format!(
            "{url} did not say what it is, and it does not look like text"
        )),
    }
}

/// Ask the configured sources that can fetch a page body.
fn rescue(url: &str, search: Option<&crate::web::walk::Search>) -> Option<(String, String)> {
    let search = search?;
    for id in &search.rules().fetch.order {
        let Some(source) = search.rules().sources.get(id) else {
            continue;
        };
        let attempt = crate::web::walk::Attempt {
            id,
            kind: source.kind,
            url: &source.url,
            headers: &source.headers,
            text: url,
            want: 1,
            timeout: search.rules().fetch.timeout,
            user_agent: &search.rules().browser_user_agent,
        };
        let Ok(asker) = HttpAsker::new(search.session().to_string(), attempt.timeout) else {
            continue;
        };
        if let Ok(body) = asker.fetch_page(&attempt) {
            return Some((body, source.name.clone()));
        }
    }
    None
}

/// A guard refusal as the model reads it.
fn describe_guard(error: &crate::web::guard::GuardError) -> String {
    format!("{error}")
}

/// Resolve `host:port` to its addresses.
fn resolve(host: &str, port: u16) -> std::io::Result<Vec<std::net::IpAddr>> {
    use std::net::ToSocketAddrs;
    if let Ok(address) = host.parse::<std::net::IpAddr>() {
        return Ok(vec![address]);
    }
    Ok((host, port)
        .to_socket_addrs()?
        .map(|address| address.ip())
        .collect())
}

/// HTML to readable text: drop the parts a reader never sees, then the tags.
///
/// Deliberately not a markdown converter: `htmd` and `markup5ever` are not
/// dependencies of this crate, and the measured ratio (220 KB of text from a
/// 948 KB page, *Measured* §6) is what a reader needs — the structure a model
/// uses is the headings and the link targets, both of which survive.
#[must_use]
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 4);
    let mut skip: Option<&str> = None;
    let bytes = html.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'<' {
            let Some(close) = html[at..].find('>').map(|offset| at + offset) else {
                break;
            };
            let tag = &html[at + 1..close];
            let name = tag
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if let Some(watched) = skip {
                if tag.starts_with('/') && name == watched {
                    skip = None;
                }
            } else if tag.starts_with('/') {
                if breaks_line(&name) {
                    out.push('\n');
                }
            } else if matches!(name.as_str(), "script" | "style" | "noscript" | "svg") {
                skip = Some(match name.as_str() {
                    "script" => "script",
                    "style" => "style",
                    "noscript" => "noscript",
                    _ => "svg",
                });
            } else if breaks_line(&name) {
                out.push('\n');
            } else if name == "td" || name == "th" {
                out.push(' ');
            }
            at = close + 1;
            continue;
        }
        let end = html[at..]
            .find('<')
            .map_or(bytes.len(), |offset| at + offset);
        if skip.is_none() {
            out.push_str(&unescape(&html[at..end]));
        }
        at = end;
    }
    let mut cleaned = String::with_capacity(out.len());
    let mut blank = 0;
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        cleaned.push_str(line);
        cleaned.push('\n');
    }
    cleaned.trim().to_string()
}

/// Whether a tag ends a line of reading text.
fn breaks_line(name: &str) -> bool {
    matches!(
        name,
        "p" | "div" | "li" | "tr" | "br" | "section" | "article"
    ) || (name.len() == 2 && name.starts_with('h') && name.as_bytes()[1].is_ascii_digit())
}

/// The five entities a page's text actually carries.
fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests;
