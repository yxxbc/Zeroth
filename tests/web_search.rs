//! `web_search` against a loopback stand-in (`docs/tools/web-search.md`).
//!
//! The pure halves are tested where they live — the walk in
//! `src/web/walk/tests.rs`, the reply readers in `src/web/sources/tests.rs`.
//! This file covers the seam between them: what actually goes over the wire,
//! and that a real HTTP failure travels back as the source's cooldown.
//!
//! No network: `TcpListener` on `127.0.0.1:0`, the `round_budget.rs` shape.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zeroth::llm::web_exec::HttpAsker;
use zeroth::web::rules::parse_rules;
use zeroth::web::walk::{Query, Search};

/// What the stand-in answers with.
#[derive(Clone)]
struct Reply {
    status: u16,
    content_type: &'static str,
    body: String,
}

impl Reply {
    fn json(body: impl Into<String>) -> Self {
        Reply {
            status: 200,
            content_type: "application/json",
            body: body.into(),
        }
    }

    fn sse(body: impl Into<String>) -> Self {
        Reply {
            status: 200,
            content_type: "text/event-stream",
            body: body.into(),
        }
    }

    fn status(status: u16, body: impl Into<String>) -> Self {
        Reply {
            status,
            content_type: "application/json",
            body: body.into(),
        }
    }
}

/// A stand-in that serves `replies` in order and records every request body.
struct Server {
    base: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(replies: Vec<Reply>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&seen);
        std::thread::spawn(move || {
            for (n, stream) in listener.incoming().enumerate() {
                let Ok(mut stream) = stream else { return };
                let body = read_request(&mut stream);
                recorded.lock().unwrap().push(body);
                let reply = &replies[n.min(replies.len() - 1)];
                let response = format!(
                    "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    reply.status,
                    if reply.status == 200 { "OK" } else { "Error" },
                    reply.content_type,
                    reply.body.len(),
                    reply.body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Server { base, seen }
    }

    fn requests(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

/// Read one request: headers, then its sized body. Returns the body.
fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut chunk).expect("request bytes");
        if n == 0 {
            return String::new();
        }
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]).to_ascii_lowercase();
    let length: usize = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    while bytes.len() < header_end + length {
        let n = stream.read(&mut chunk).expect("body bytes");
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    String::from_utf8_lossy(&bytes[header_end..]).into_owned()
}

/// Rules naming `sources` in `order`, all pointing at `base`.
fn rules(order: &[&str], base: &str) -> String {
    let list = order
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mut text = format!(
        r#"
order = [{list}]
results = 3
excerpt_chars = 100
timeout_seconds = 5
cooldown_minutes = [1]
browser_user_agent = "test-agent"

[fetch]
order = ["{first}"]
min_chars = 10
max_chars = 1000
page_chars = 100
timeout_seconds = 5
"#,
        first = order[0]
    );
    for id in order {
        // The kind follows the id, so a fixture reads like the shipped file.
        let kind = match *id {
            "exa" => "exa",
            "tavily" => "tavily",
            "anysearch" => "anysearch",
            "bing" => "bing",
            _ => "parallel",
        };
        text.push_str(&format!(
            "\n[sources.{id}]\nname = \"{id}\"\nkind = \"{kind}\"\nurl = \"{base}/{id}\"\nneeds = \"nothing\"\n"
        ));
    }
    text
}

fn search(order: &[&str], base: &str) -> Search {
    let text = rules(order, base);
    Search::new(parse_rules(&text, std::path::Path::new("t.toml")).expect("parses"))
}

fn asker(search: &Search) -> HttpAsker {
    HttpAsker::new(search.session().to_string(), Duration::from_secs(5)).expect("a client")
}

fn query() -> Query {
    Query {
        text: "rust async".to_string(),
        filter: Default::default(),
    }
}

/// The `tools/call` result an MCP server sends, wrapping `payload` as text.
fn mcp(payload: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {"content": [{"type": "text", "text": payload}]}
    })
    .to_string()
}

// ===== what goes over the wire =====

#[test]
fn a_mcp_source_is_asked_with_a_tools_call_carrying_the_query_and_session() {
    let payload = r#"{"results":[{"url":"https://a.test/1","title":"A","excerpts":["hi"]}]}"#;
    let server = Server::start(vec![Reply::json(mcp(payload))]);
    let search = search(&["parallel"], &server.base);

    let found = search.search(&query(), &asker(&search)).expect("finds");
    assert_eq!(found.source, "parallel");
    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].url, "https://a.test/1");

    let sent = server.requests();
    assert_eq!(sent.len(), 1);
    let body: serde_json::Value = serde_json::from_str(&sent[0]).expect("json");
    assert_eq!(body["method"], "tools/call");
    assert_eq!(body["params"]["name"], "web_search");
    assert_eq!(
        body["params"]["arguments"]["search_queries"][0],
        "rust async"
    );
    assert_eq!(
        body["params"]["arguments"]["session_id"],
        search.session(),
        "the process id, not the model's"
    );
}

#[test]
fn an_sse_reply_is_read_as_well_as_a_plain_one() {
    // Measured: Tavily answers SSE where Parallel answers JSON (`Measured` §7).
    let payload = r#"{"results":[{"url":"https://b.test/1","title":"B","content":"body"}]}"#;
    let event = format!("event: message\ndata: {}\n\n", mcp(payload));
    let server = Server::start(vec![Reply::sse(event)]);
    let search = search(&["tavily"], &server.base);

    let found = search.search(&query(), &asker(&search)).expect("finds");
    assert_eq!(found.hits[0].url, "https://b.test/1");
    assert_eq!(found.hits[0].snippet.as_deref(), Some("body"));
}

#[test]
fn the_result_count_reaches_the_source_and_the_cap_trims_what_returns() {
    let hits = (0..6)
        .map(|n| format!(r#"{{"url":"https://a.test/{n}","title":"T{n}","excerpts":["x"]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let server = Server::start(vec![Reply::json(mcp(&format!("{{\"results\":[{hits}]}}")))]);
    let search = search(&["parallel"], &server.base);

    let found = search.search(&query(), &asker(&search)).expect("finds");
    assert_eq!(found.hits.len(), 3, "results = 3 in the fixture");
    let sent: serde_json::Value = serde_json::from_str(&server.requests()[0]).expect("json");
    assert_eq!(sent["params"]["arguments"]["objective"], "rust async");
}

// ===== failures travel =====

#[test]
fn a_429_rests_the_source_and_the_next_one_answers() {
    let payload = r#"{"results":[{"url":"https://b.test/1","title":"B","excerpts":[]}]}"#;
    let server = Server::start(vec![
        Reply::status(429, r#"{"error":"slow down"}"#),
        Reply::json(mcp(payload)),
    ]);
    let search = search(&["parallel", "anysearch"], &server.base);

    let found = search.search(&query(), &asker(&search)).expect("finds");
    assert_eq!(found.source, "anysearch", "the second source answered");
    assert_eq!(server.requests().len(), 2);
    assert_eq!(
        search.cooling(std::time::Instant::now()).len(),
        1,
        "the limited source is resting"
    );
}

#[test]
fn a_limit_named_in_the_body_is_recognised_at_this_layer_too() {
    // HTTP 200, a JSON-RPC error, the provider's own words — Exa's real shape.
    let limited = serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "error": {"code": -32000, "message": "You've hit Exa's free MCP rate limit."}
    })
    .to_string();
    let server = Server::start(vec![
        Reply::json(limited),
        Reply::json(mcp(r#"{"results":[]}"#)),
    ]);
    let search = search(&["exa", "parallel"], &server.base);

    let found = search.search(&query(), &asker(&search)).expect("finds");
    assert_eq!(found.source, "parallel");
    assert_eq!(search.cooling(std::time::Instant::now()).len(), 1);
}

#[test]
fn an_iserror_result_inside_a_200_hands_over_to_the_next_source() {
    // The trap the shipped skill documented: HTTP 200 with isError true.
    let refused = serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "result": {"isError": true, "content": [{"type": "text",
                   "text": "Anonymous keyless access is unavailable for this request."}]}
    })
    .to_string();
    let server = Server::start(vec![
        Reply::json(refused),
        Reply::json(mcp(
            r#"{"results":[{"url":"https://c.test/1","title":"C","excerpts":[]}]}"#,
        )),
    ]);
    let search = search(&["tavily", "parallel"], &server.base);

    let found = search.search(&query(), &asker(&search)).expect("finds");
    assert_eq!(found.source, "parallel");
}

#[test]
fn every_source_failing_reports_the_last_reason_not_the_first() {
    let server = Server::start(vec![
        Reply::status(500, "first broke"),
        Reply::status(503, "second broke"),
    ]);
    let search = search(&["parallel", "anysearch"], &server.base);

    let missed = search
        .search(&query(), &asker(&search))
        .expect_err("misses");
    let said = missed.to_string();
    assert!(said.contains("anysearch"), "{said}");
    assert!(
        said.contains("second broke") || said.contains("503"),
        "{said}"
    );
}

// ===== a filtered search =====

#[test]
fn a_domain_filter_is_applied_to_what_came_back_and_doubled_up_front() {
    let payload = r#"{"results":[
        {"url":"https://other.test/1","title":"O","excerpts":[]},
        {"url":"https://docs.example.com/1","title":"E","excerpts":[]}
    ]}"#;
    let server = Server::start(vec![Reply::json(mcp(payload))]);
    let search = search(&["parallel"], &server.base);
    let filtered = Query {
        text: "rust async".to_string(),
        filter: zeroth::web::Filter {
            allowed: vec!["example.com".to_string()],
            blocked: Vec::new(),
        },
    };

    let found = search.search(&filtered, &asker(&search)).expect("finds");
    assert_eq!(found.hits.len(), 1, "only the allowed domain survives");
    assert!(found.hits[0].url.contains("docs.example.com"));
    let sent: serde_json::Value = serde_json::from_str(&server.requests()[0]).expect("json");
    assert_eq!(
        sent["params"]["arguments"]["objective"], "rust async",
        "the query itself is unchanged"
    );
}
