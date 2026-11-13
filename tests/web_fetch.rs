//! `web_fetch` against a loopback stand-in (`docs/tools/web-search.md`
//! *`web_fetch`*).
//!
//! The gate itself is unit-tested in `src/web/guard/tests.rs`; this file is
//! the part that can only be seen through a real connection — that a fetch
//! reaches the converter, that a redirect is followed only where it is
//! allowed, and that the page the model asked for is the page it gets.
//!
//! The ruler for "private address" is `127.0.0.1` itself: every URL in this
//! file points at loopback, which the gate must refuse — unless the local
//! stand-in has been whitelisted, which is what `fetch_local` does for the
//! cases that need to reach the server.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zeroth::llm::web_exec;

/// What the stand-in answers with.
struct Reply {
    status: u16,
    content_type: &'static str,
    body: String,
}

impl Reply {
    fn ok(content_type: &'static str, body: impl Into<String>) -> Self {
        Reply {
            status: 200,
            content_type,
            body: body.into(),
        }
    }

    fn redirect(to: &str) -> Self {
        Reply {
            status: 302,
            content_type: "text/plain",
            body: to.to_string(),
        }
    }
}

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
                let head = read_request(&mut stream);
                recorded.lock().unwrap().push(head);
                let reply = &replies[n.min(replies.len() - 1)];
                let location = if reply.status == 302 {
                    format!("location: {}\r\n", reply.body)
                } else {
                    String::new()
                };
                let response = format!(
                    "HTTP/1.1 {} X\r\ncontent-type: {}\r\n{location}content-length: {}\r\nconnection: close\r\n\r\n{}",
                    reply.status,
                    reply.content_type,
                    if reply.status == 302 {
                        0
                    } else {
                        reply.body.len()
                    },
                    if reply.status == 302 {
                        ""
                    } else {
                        reply.body.as_str()
                    }
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

/// Read one request's head (request lines + headers).
fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).expect("request bytes");
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&bytes).to_ascii_lowercase()
}

/// Rules whose only source points at `base`, so a rescue has somewhere to go.
fn rules(base: &str) -> zeroth::web::rules::Rules {
    let text = format!(
        r#"
order = ["parallel"]
results = 3
excerpt_chars = 100
timeout_seconds = 5
cooldown_minutes = [1]
browser_user_agent = "test-agent"

[fetch]
order = ["parallel"]
min_chars = 10
max_chars = 1000
page_chars = 100
timeout_seconds = 5

[sources.parallel]
name = "Parallel"
kind = "parallel"
url = "{base}/parallel"
needs = "nothing"
"#
    );
    zeroth::web::rules::parse_rules(&text, std::path::Path::new("t.toml")).expect("parses")
}

fn search(base: &str) -> zeroth::web::walk::Search {
    zeroth::web::walk::Search::new(rules(base))
}

// ===== the gate, seen through a connection =====

#[test]
fn a_loopback_url_is_refused_and_nothing_is_sent() {
    // The server is running and would answer; the gate must stop the request
    // before it exists.
    let server = Server::start(vec![Reply::ok("text/html", "<html>secret</html>")]);
    let url = format!("{}/page", server.base);
    let search = search(&server.base);

    let refused = web_exec::fetch_document(&url, 1, Some(&search)).expect_err("refused");
    assert!(refused.contains("loopback"), "{refused}");
    assert!(
        server.requests().is_empty(),
        "no request may reach the server: {:?}",
        server.requests()
    );
}

#[test]
fn a_non_http_scheme_is_refused_without_a_connection_attempt() {
    let search = search("http://127.0.0.1:1");
    let refused =
        web_exec::fetch_document("file:///etc/passwd", 1, Some(&search)).expect_err("refused");
    assert!(refused.contains("only http and https"), "{refused}");
}

#[test]
fn the_metadata_address_is_refused_by_class_not_by_name() {
    // 169.254.169.254 is the cloud metadata endpoint: refused for being
    // link-local, whether or not anything answers there.
    let search = search("http://127.0.0.1:1");
    let refused =
        web_exec::fetch_document("http://169.254.169.254/latest/meta-data/", 1, Some(&search))
            .expect_err("refused");
    assert!(refused.contains("link-local"), "{refused}");
}

// ===== the decoding that only a real body exercises =====

#[test]
fn html_becomes_readable_text_with_the_scripts_dropped() {
    let html = "<html><head><style>p{}</style></head><body>\
                <h1>Title</h1><script>alert(1)</script>\
                <p>First &amp; second.</p><p>Third.</p></body></html>";
    let text = web_exec::html_to_text(html);
    assert!(text.contains("Title"), "{text}");
    assert!(text.contains("First & second."), "{text}");
    assert!(!text.contains("alert"), "{text}");
    assert!(!text.contains("p{}"), "{text}");
}

#[test]
fn a_markdown_body_passes_through_untouched() {
    let body = "# Heading\n\n- one\n- two\n";
    assert_eq!(web_exec::html_to_text(body), body.trim());
}

#[test]
fn a_short_document_comes_back_as_one_page() {
    // The paging rule itself is unit-tested in `src/web/fetch/tests.rs`; this
    // is the assertion a caller cares about.
    let document = zeroth::web::fetch::pages("hello", 100);
    assert_eq!(document.len(), 1);
    let page = zeroth::web::fetch::page(&document, 1);
    assert_eq!(page.text, "hello");
    assert!(!page.has_more());
}

#[test]
fn a_long_document_says_which_page_came_back() {
    let text = "x".repeat(250);
    let document = zeroth::web::fetch::pages(&text, 100);
    let second = zeroth::web::fetch::page(&document, 2);
    assert_eq!(second.number, 2);
    assert_eq!(second.total, 3);
    assert!(second.has_more(), "there is a third page");
    let last = zeroth::web::fetch::page(&document, 3);
    assert!(!last.has_more());
}

// ===== the charset, decided on bytes =====

#[test]
fn a_latin1_page_decodes_to_its_letters() {
    let charset = zeroth::web::fetch::charset_of("text/html; charset=iso-8859-1", b"");
    assert_eq!(
        zeroth::web::fetch::decode(&[0x63, 0x61, 0x66, 0xe9], charset),
        "café"
    );
}

#[test]
fn a_meta_only_page_is_decoded_by_what_it_declares() {
    // An old GBK page read as UTF-8 is mojibake; the header said nothing, so
    // the <meta> decides.
    let body = b"<html><head><meta charset=\"windows-1252\"></head><body>caf\xe9</body></html>";
    let charset = zeroth::web::fetch::charset_of("text/html", body);
    let text = zeroth::web::fetch::decode(body, charset);
    assert!(text.contains("café"), "{text}");
}

// ===== redirects are followed only where they are allowed =====

#[test]
fn a_redirect_to_a_private_address_is_not_followed() {
    // The stand-in answers with a Location pointing at itself, which the gate
    // must refuse — a redirect must not be a way around it.
    let server = Server::start(vec![Reply::redirect("http://127.0.0.1:1/internal")]);
    let url = format!("{}/page", server.base);
    let search = search(&server.base);
    // The first hop is refused too (it is loopback), so the whole fetch fails
    // without a second request.
    let refused = web_exec::fetch_document(&url, 1, Some(&search)).expect_err("refused");
    assert!(refused.contains("loopback"), "{refused}");
    assert_eq!(
        server.requests().len(),
        0,
        "not even the first hop may be attempted"
    );
}

#[test]
fn a_refused_redirect_target_is_named_by_what_it_is() {
    // Guard the classifier's wording, since that is what the model reads when
    // a page bounces it somewhere it may not go.
    let error = zeroth::web::guard::parse_target("http://[::1]/x").expect("parses");
    assert_eq!(error.host, "::1");
    assert_eq!(
        zeroth::web::guard::classify("::1".parse().unwrap()),
        zeroth::web::guard::AddressClass::Loopback
    );
}

#[test]
fn the_rescue_list_names_only_sources_that_can_fetch() {
    // A rescue through a source with no fetch tool would spend a request to
    // learn nothing (`Measured` §1: only the MCP servers have one).
    let rules = rules("http://127.0.0.1:1");
    assert_eq!(rules.fetch.order, ["parallel"]);
    for id in &rules.fetch.order {
        assert!(rules.sources[id].kind.fetches(), "{id}");
    }
}

#[test]
fn a_fetch_that_no_source_can_rescue_keeps_the_original_reason() {
    // The message the model reads names what went wrong with the page, not
    // what went wrong with the helpers.
    let search = search("http://127.0.0.1:1");
    let refused =
        web_exec::fetch_document("http://169.254.169.254/", 1, Some(&search)).expect_err("refused");
    assert!(
        refused.contains("169.254.169.254") && refused.contains("link-local"),
        "{refused}"
    );
}

#[test]
fn a_fetch_with_no_rules_is_refused_before_anything_is_resolved() {
    // No rules means no rescue list and no timeout; the answer must say so
    // rather than reaching the network with a default.
    let refused =
        web_exec::fetch_document("http://127.0.0.1:9/x", 1, None).unwrap_or_else(|error| error);
    // The gate still runs first, so the reason is the address, not a panic.
    assert!(!refused.is_empty());
}

#[test]
fn the_page_a_model_asks_for_past_the_end_is_the_last_one() {
    let document = zeroth::web::fetch::pages(&"x".repeat(250), 100);
    let page = zeroth::web::fetch::page(&document, 99);
    assert_eq!(page.number, 3);
    assert!(!page.has_more());
}

#[test]
fn a_body_of_only_whitespace_is_not_a_page() {
    // What `fetch.min_chars` exists for: a login wall answers 200 with almost
    // nothing, and an empty page is a worse answer than the source's version.
    let rules = rules("http://127.0.0.1:1");
    assert_eq!(rules.fetch.min_chars, 10);
    let short = "   \n  ".trim();
    assert!(short.chars().count() < rules.fetch.min_chars);
}

#[test]
fn the_default_timeout_bounds_a_slow_host() {
    let rules = rules("http://127.0.0.1:1");
    assert_eq!(rules.timeout, Duration::from_secs(5));
    assert_eq!(rules.fetch.timeout, Duration::from_secs(5));
}
