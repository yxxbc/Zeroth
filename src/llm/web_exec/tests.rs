//! The boundary's pure half: how a source is asked, and how its reply is
//! unwrapped. The transport itself is covered by the loopback tests.

use super::*;
use crate::web::rules::parse_rules;
use crate::web::walk::Search;

const CONFIG: &str = r#"
order = ["exa", "parallel", "tavily", "anysearch", "bing"]
results = 8
excerpt_chars = 100
timeout_seconds = 1
cooldown_minutes = [1]
browser_user_agent = "test-agent"

[fetch]
order = ["exa"]
min_chars = 10
max_chars = 1000
page_chars = 100
timeout_seconds = 1

[sources.exa]
name = "Exa"
kind = "exa"
url = "https://mcp.exa.ai/mcp"
needs = "key_optional"

[sources.parallel]
name = "Parallel"
kind = "parallel"
url = "https://search.parallel.ai/mcp"
needs = "nothing"

[sources.tavily]
name = "Tavily"
kind = "tavily"
url = "https://mcp.tavily.com/mcp/"
needs = "nothing"
headers = { X-Tavily-Access-Mode = "keyless" }

[sources.anysearch]
name = "AnySearch"
kind = "anysearch"
url = "https://api.anysearch.com/v1/search"
needs = "nothing"

[sources.bing]
name = "Bing"
kind = "bing"
url = "https://www.bing.com/search"
needs = "nothing"
"#;

fn search() -> Search {
    Search::new(parse_rules(CONFIG, std::path::Path::new("t.toml")).expect("parses"))
}

fn attempt<'a>(search: &'a Search, id: &'a str, want: usize) -> crate::web::walk::Attempt<'a> {
    let source = &search.rules().sources[id];
    crate::web::walk::Attempt {
        id,
        kind: source.kind,
        url: &source.url,
        headers: &source.headers,
        text: "rust async",
        want,
        timeout: std::time::Duration::from_secs(1),
        user_agent: &search.rules().browser_user_agent,
    }
}

// ===== the request shape =====

#[test]
fn a_mcp_source_sends_a_tools_call() {
    let search = search();
    let body = request_body(&attempt(&search, "parallel", 8), search.session()).expect("a body");
    let value: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(value["method"], "tools/call");
    assert_eq!(value["params"]["name"], "web_search");
    assert_eq!(
        value["params"]["arguments"]["search_queries"][0],
        "rust async"
    );
    assert_eq!(
        value["params"]["arguments"]["session_id"],
        search.session(),
        "the process session id, never the model's"
    );
}

#[test]
fn exa_asks_for_its_own_result_count_and_an_objective() {
    let search = search();
    let body = request_body(&attempt(&search, "exa", 8), "").expect("a body");
    let value: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(value["params"]["name"], "web_search_exa");
    assert_eq!(value["params"]["arguments"]["numResults"], 8);
    assert_eq!(value["params"]["arguments"]["objective"], "rust async");
}

#[test]
fn anysearch_is_a_plain_rest_body_with_its_own_ceiling() {
    let search = search();
    let body = request_body(&attempt(&search, "anysearch", 40), "").expect("a body");
    let value: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(value["query"], "rust async");
    assert_eq!(value["max_results"], 10, "its API refuses more than ten");
}

#[test]
fn a_scraped_source_sends_no_body_and_asks_by_get() {
    let search = search();
    assert!(request_body(&attempt(&search, "bing", 8), "").is_none());
    assert_eq!(
        http_method(crate::web::rules::Kind::Bing),
        reqwest::Method::GET
    );
    assert_eq!(
        http_method(crate::web::rules::Kind::Parallel),
        reqwest::Method::POST
    );
}

#[test]
fn a_request_carries_the_sources_own_headers() {
    // Tavily's keyless tier is a 401 without this one (Measured §1).
    let search = search();
    let request = build_request(&attempt(&search, "tavily", 8), "", None).expect("builds");
    let built = request.build().expect("a request");
    assert_eq!(
        built
            .headers()
            .get("X-Tavily-Access-Mode")
            .map(|v| v.to_str().unwrap()),
        Some("keyless")
    );
}

#[test]
fn a_key_is_sent_where_its_source_reads_it() {
    let search = search();
    let exa_key = String::from("exa-secret");
    let exa = build_request(&attempt(&search, "exa", 8), "", Some(&exa_key))
        .expect("builds")
        .build()
        .expect("a request");
    assert!(
        exa.headers().get("Authorization").is_none(),
        "Exa takes its key in the URL, not a header"
    );

    let tavily_key = String::from("tvly-secret");
    let tavily = build_request(&attempt(&search, "tavily", 8), "", Some(&tavily_key))
        .expect("builds")
        .build()
        .expect("a request");
    assert_eq!(
        tavily
            .headers()
            .get("Authorization")
            .map(|v| v.to_str().unwrap()),
        Some("Bearer tvly-secret")
    );
}

// ===== reading a reply =====

#[test]
fn a_plain_json_answer_is_read() {
    let body = r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"{\"results\":[{\"url\":\"https://a/1\",\"title\":\"A\"}]}"}]}}"#;
    let hits = read_reply(crate::web::rules::Kind::Parallel, body).expect("reads");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].url, "https://a/1");
}

#[test]
fn an_sse_answer_is_unwrapped_to_its_last_result() {
    // Measured: Tavily answers SSE, and earlier events may be noise.
    let body = concat!(
        "event: message\n",
        "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"content\":[{\"text\":\"{\\\"results\\\":[{\\\"url\\\":\\\"https://a/1\\\",\\\"title\\\":\\\"A\\\"}]}\"}]}}\n\n",
    );
    let inner = last_sse_result(body).expect("unwraps");
    let hits = read_reply(crate::web::rules::Kind::Tavily, &inner).expect("reads");
    assert_eq!(hits[0].url, "https://a/1");
}

#[test]
fn an_iserror_result_in_a_200_is_a_failure() {
    // The trap the shipped skill documented for Tavily: HTTP 200, isError true.
    let body = r#"{"jsonrpc":"2.0","id":1,"result":{"isError":true,"content":[{"type":"text","text":"Anonymous keyless access is unavailable"}]}}"#;
    match read_reply(crate::web::rules::Kind::Tavily, body) {
        Err(Failure::Failed(why)) => assert!(why.contains("keyless access")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_json_rpc_error_naming_a_rate_limit_is_limited() {
    let body = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"You've hit Exa's free MCP rate limit."}}"#;
    assert_eq!(
        read_reply(crate::web::rules::Kind::Exa, body),
        Err(Failure::Limited)
    );
}

#[test]
fn an_sse_body_with_no_answer_carries_its_text_as_the_reason() {
    match last_sse_result("event: error\ndata: something broke\n\n") {
        Err(Failure::Failed(why)) => assert!(why.contains("something broke")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_bing_page_is_parsed_rather_than_read_as_json() {
    let html = r#"<li class="b_algo"><h2><a href="https://www.bing.com/ck/a?u=a1aHR0cHM6Ly9hLnRlc3QvMQ">One</a></h2></li>"#;
    let hits = read_reply(crate::web::rules::Kind::Bing, html).expect("parses");
    assert_eq!(hits[0].url, "https://a.test/1");
}
