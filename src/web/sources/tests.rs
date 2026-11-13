//! Reply reading and the limit classifier. The fixtures are the **real**
//! strings the providers returned on 2026-11-13 (`docs/tools/web-search.md`
//! *Measured* §1, §4), including the successful body that broke the first
//! keyword list.

use super::*;

// ===== the limit classifier =====

#[test]
fn exas_real_429_message_is_limited() {
    let text = "You've hit Exa's free MCP rate limit. Create a free Exa account to get \
                your own usage allowance and rate limits. No payment method required.";
    assert_eq!(Failure::from_text(text), Failure::Limited);
}

#[test]
fn tavilys_real_monthly_cap_is_limited() {
    let text = "You reached the monthly keyless Tavily limit. To continue immediately, \
                pay via x402 agentic payment or sign up for a Tavily API key.";
    assert_eq!(Failure::from_text(text), Failure::Limited);
}

#[test]
fn tavalys_bare_401_is_a_failure_with_its_reason() {
    // 401 with an empty body: the status classifies it, the body cannot.
    assert_eq!(
        Failure::from_status(401),
        Some(Failure::Failed("HTTP 401".into()))
    );
}

#[test]
fn firecrawls_iserror_in_a_200_is_not_a_limit() {
    let text = "Anonymous keyless access is unavailable for this request. Fix: Create an \
                API key at https://firecrawl.dev/k/x and then set the header.";
    assert_eq!(
        Failure::from_text(text),
        Failure::Failed(text.chars().take(200).collect::<String>())
    );
}

#[test]
fn a_successful_body_containing_quota_is_never_consulted() {
    // Measured §4: this excerpt sits inside a GOOD Parallel answer (query
    // "smol"), and a substring scan fires on `quotations`. Two defences, and
    // this test pins both: the walk classifies only a failure, never a
    // successful body, and the word list is anchored so the near miss cannot
    // match even if it were scanned.
    let good = "English terms with quotations -- DoggoLingo -- internet slang";
    assert!(good.contains("quota"), "the fixture must carry the trap");

    assert_eq!(
        Failure::from_text(good),
        Failure::Failed(good.to_string()),
        "`quotations` must not read as a quota"
    );
    // The unanchored form the first draft would have shipped, for contrast:
    // it is why the boundary is there.
    assert!(
        good.to_lowercase().contains("quota"),
        "the substring form fires; the anchored form must not"
    );
}

#[test]
fn the_classifier_needs_a_word_boundary() {
    // Substring matching fired on "quotations"; a boundary does not.
    assert_eq!(
        Failure::from_text("English terms with quotations"),
        Failure::Failed("English terms with quotations".into())
    );
    assert_eq!(Failure::from_text("a quota of 10"), Failure::Limited);
    assert_eq!(Failure::from_text("quotas"), Failure::Limited);
    assert_eq!(Failure::from_text("no credits left"), Failure::Limited);
}

#[test]
fn a_status_classifies_before_the_body() {
    assert_eq!(Failure::from_status(429), Some(Failure::Limited));
    assert_eq!(Failure::from_status(402), Some(Failure::Limited));
    assert_eq!(Failure::from_status(200), None);
    assert_eq!(
        Failure::from_status(503),
        Some(Failure::Failed("HTTP 503".into()))
    );
}

#[test]
fn an_unrecognised_failure_keeps_the_providers_words() {
    let long = "x".repeat(400);
    match Failure::from_text(&long) {
        Failure::Failed(text) => assert_eq!(text.chars().count(), 200),
        other => panic!("{other:?}"),
    }
}

// ===== Exa's text form =====

const EXA: &str = "\
Title: select in tokio - Rust
URL: https://docs.rs/tokio/latest/tokio/macro.select.html
Published: 2026-09-01
Author: N/A
Highlights:
Waits on multiple concurrent branches, driving them concurrently.
---
Title: Futures - Asynchronous Programming in Rust
URL: https://rust-lang.github.io/async-book/02_execution/02_future.html
Published: N/A
Highlights:
The Future trait.
";

#[test]
fn exas_text_form_reads_both_records() {
    let hits = reply_hits(Kind::Exa, EXA).expect("reads");
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].title, "select in tokio - Rust");
    assert_eq!(hits[0].date.as_deref(), Some("2026-09-01"));
    assert!(hits[0].snippet.as_deref().unwrap().starts_with("Waits on"));
    assert_eq!(hits[1].date, None, "Published: N/A is no date");
    assert!(hits.iter().all(Hit::is_complete));
}

#[test]
fn a_record_without_a_url_is_dropped() {
    let text = "Title: orphan\nHighlights:\nx\n";
    assert!(reply_hits(Kind::Exa, text).expect("reads").is_empty());
}

// ===== the JSON shapes =====

#[test]
fn parallel_reads_excerpts_and_a_date() {
    let text = r#"{"results":[{"url":"https://a/1","title":"A","publish_date":"2026-06-11",
                    "excerpts":["one","two"]},{"url":"https://a/2","title":"B","excerpts":[]}]}"#;
    let hits = reply_hits(Kind::Parallel, text).expect("reads");
    assert_eq!(hits[0].snippet.as_deref(), Some("one\ntwo"));
    assert_eq!(hits[0].date.as_deref(), Some("2026-06-11"));
    assert_eq!(hits[1].snippet, None);
}

#[test]
fn anysearch_reads_content_then_snippet() {
    let text = r#"{"code":0,"message":"success","data":{"results":[
                     {"title":"T","url":"https://a","snippet":"s","content":"c"}]}}"#;
    let hits = reply_hits(Kind::Anysearch, text).expect("reads");
    assert_eq!(hits[0].snippet.as_deref(), Some("c"), "content wins");
}

#[test]
fn anysearchs_non_zero_code_is_a_failure() {
    let text = r#"{"code":7,"message":"too many requests","data":{"results":[]}}"#;
    assert_eq!(reply_hits(Kind::Anysearch, text), Err(Failure::Limited));
}

#[test]
fn tavily_reads_its_content_field() {
    let text = r#"{"query":"q","results":[{"url":"https://a","title":"T","content":"body",
                     "published_date":"2026-01-02T10:00:00"}]}"#;
    let hits = reply_hits(Kind::Tavily, text).expect("reads");
    assert_eq!(
        hits[0].date.as_deref(),
        Some("2026-01-02"),
        "the day is cut out"
    );
}

#[test]
fn an_unparsable_body_is_a_failure_naming_its_head() {
    match reply_hits(Kind::Parallel, "not json at all") {
        Err(Failure::Failed(why)) => assert!(why.contains("not json at all")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn bings_html_is_not_read_here() {
    assert!(matches!(
        reply_hits(Kind::Bing, "<html>"),
        Err(Failure::Failed(_))
    ));
}

#[test]
fn an_empty_result_list_is_not_an_error() {
    // A keyless source with no hits is success — it stops the walk.
    assert_eq!(reply_hits(Kind::Parallel, r#"{"results":[]}"#), Ok(vec![]));
}

// ===== dates =====

#[test]
fn only_an_iso_day_reads_as_a_date() {
    assert_eq!(iso_day("2026-11-13"), Some("2026-11-13".into()));
    assert_eq!(iso_day("2026-11-13T10:00:00Z"), Some("2026-11-13".into()));
    assert_eq!(iso_day("Mon, 01 Jan 2024"), None);
    assert_eq!(iso_day("2026-1-3"), None);
    assert_eq!(iso_day(""), None);
}
