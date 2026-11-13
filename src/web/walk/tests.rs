//! The walk's behaviour, with a table in place of the network: which source is
//! tried, when one is skipped, and what the caller ends up holding
//! (`docs/tools/web-search.md` *The walk*).

use super::*;
use crate::web::rules::parse_rules;
use std::cell::RefCell;
use std::time::Duration;

const CONFIG: &str = r#"
order = ["first", "second", "third"]
results = 3
excerpt_chars = 100
timeout_seconds = 1
cooldown_minutes = [1, 2, 4]
browser_user_agent = "test-agent"

[fetch]
order = ["first"]
min_chars = 10
max_chars = 1000
page_chars = 100
timeout_seconds = 1

[sources.first]
name = "First"
kind = "parallel"
url = "https://first.test/mcp"
needs = "nothing"

[sources.second]
name = "Second"
kind = "anysearch"
url = "https://second.test/search"
needs = "nothing"

[sources.third]
name = "Third"
kind = "bing"
url = "https://third.test/search"
needs = "nothing"
"#;

fn rules(text: &str) -> crate::web::rules::Rules {
    parse_rules(text, std::path::Path::new("test.toml")).expect("the fixture parses")
}

/// One source's scripted replies, in order.
type Replies = Vec<Result<Vec<Hit>, Failure>>;

/// A scripted source: one entry per call, in order.
struct Script {
    /// id → the replies to return, in order. An empty list fails the test.
    replies: std::collections::HashMap<String, Replies>,
    /// Every attempt made, in order.
    seen: RefCell<Vec<String>>,
    /// Every width asked for, in order.
    wanted: RefCell<Vec<usize>>,
}

impl Script {
    /// `replies` as `(id, [reply, …])`.
    fn new(replies: &[(&str, Replies)]) -> Self {
        Self {
            replies: replies
                .iter()
                .map(|(id, list)| ((*id).to_string(), list.clone()))
                .collect(),
            seen: RefCell::new(Vec::new()),
            wanted: RefCell::new(Vec::new()),
        }
    }
}

impl Asker for Script {
    fn ask(&self, attempt: &Attempt<'_>) -> Result<Vec<Hit>, Failure> {
        self.seen.borrow_mut().push(attempt.id.to_string());
        self.wanted.borrow_mut().push(attempt.want);
        let mut queue = self.replies.get(attempt.id).cloned().unwrap_or_default();
        if queue.is_empty() {
            panic!("{} was called more times than scripted", attempt.id);
        }
        queue.remove(0)
    }
}

fn hit(url: &str) -> Hit {
    Hit {
        title: "T".into(),
        url: url.into(),
        snippet: None,
        date: None,
    }
}

fn query(text: &str) -> Query {
    Query {
        text: text.to_string(),
        filter: Filter::default(),
    }
}

// ===== the order =====

#[test]
fn the_first_source_that_answers_wins_and_the_rest_are_never_called() {
    let search = Search::new(rules(CONFIG));
    let script = Script::new(&[("first", vec![Ok(vec![hit("https://a.test/1")])])]);
    let found = search.search(&query("q"), &script).expect("finds");
    assert_eq!(found.source, "First", "the user's cell names the source");
    assert_eq!(found.hits.len(), 1);
    assert_eq!(*script.seen.borrow(), ["first"]);
}

#[test]
fn a_failing_source_hands_the_walk_to_the_next() {
    let search = Search::new(rules(CONFIG));
    let script = Script::new(&[
        ("first", vec![Err(Failure::Failed("HTTP 500".into()))]),
        ("second", vec![Ok(vec![hit("https://b.test/1")])]),
    ]);
    let found = search.search(&query("q"), &script).expect("finds");
    assert_eq!(found.source, "Second");
    assert_eq!(*script.seen.borrow(), ["first", "second"]);
}

#[test]
fn a_source_that_looks_empty_is_a_success_and_stops_the_walk() {
    // The rule that keeps an obscure query from burning every quota.
    let search = Search::new(rules(CONFIG));
    let script = Script::new(&[("first", vec![Ok(vec![])])]);
    let found = search
        .search(&query("nothing matches"), &script)
        .expect("finds");
    assert!(found.hits.is_empty());
    assert_eq!(*script.seen.borrow(), ["first"], "second must not be tried");
}

// ===== the cooldown =====

#[test]
fn a_limited_source_rests_and_the_next_answers() {
    let search = Search::new(rules(CONFIG));
    let script = Script::new(&[
        ("first", vec![Err(Failure::Limited)]),
        ("second", vec![Ok(vec![hit("https://b.test/1")])]),
    ]);
    search.search(&query("q"), &script).expect("finds");
    assert_eq!(*script.seen.borrow(), ["first", "second"]);
    assert_eq!(
        search.cooling(Instant::now()).len(),
        1,
        "the limited source is resting"
    );
}

#[test]
fn a_resting_source_is_skipped_on_the_next_search() {
    let search = Search::new(rules(CONFIG));
    let first = Script::new(&[
        ("first", vec![Err(Failure::Limited)]),
        ("second", vec![Ok(vec![hit("https://b.test/1")])]),
    ]);
    search.search(&query("q"), &first).expect("finds");

    let second = Script::new(&[("second", vec![Ok(vec![hit("https://b.test/2")])])]);
    search.search(&query("q"), &second).expect("finds");
    assert_eq!(
        *second.seen.borrow(),
        ["second"],
        "first is resting, so it is not asked again"
    );
}

#[test]
fn a_rested_source_is_tried_again_with_no_clock_tick_of_its_own() {
    let search = Search::new(rules(CONFIG));
    let start = Instant::now();
    let first = Script::new(&[
        ("first", vec![Err(Failure::Limited)]),
        ("second", vec![Ok(vec![])]),
    ]);
    search.search_at(&query("q"), &first, start).expect("finds");

    // Past the first rung (1 minute), with nothing having run.
    let later = start + Duration::from_secs(61);
    let second = Script::new(&[("first", vec![Ok(vec![hit("https://a.test/2")])])]);
    let found = search
        .search_at(&query("q"), &second, later)
        .expect("finds");
    assert_eq!(found.source, "First");
}

#[test]
fn every_source_resting_says_so_rather_than_failing() {
    let search = Search::new(rules(CONFIG));
    let only = Script::new(&[
        ("first", vec![Err(Failure::Limited)]),
        ("second", vec![Err(Failure::Limited)]),
        ("third", vec![Err(Failure::Limited)]),
    ]);
    search.search(&query("q"), &only).expect_err("all limited");

    let empty = Script::new(&[]);
    assert_eq!(search.search(&query("q"), &empty), Err(Missed::Cooling));
}

#[test]
fn the_last_reason_is_what_the_user_reads() {
    let search = Search::new(rules(CONFIG));
    let script = Script::new(&[
        ("first", vec![Err(Failure::Failed("HTTP 503".into()))]),
        ("second", vec![Err(Failure::Failed("timed out".into()))]),
        ("third", vec![Err(Failure::Failed("bot check".into()))]),
    ]);
    assert_eq!(
        search.search(&query("q"), &script),
        Err(Missed::Failed("Third: bot check".into()))
    );
}

#[test]
fn a_limited_source_reads_as_a_rate_limit_not_a_plain_failure() {
    // One source, limited: everybody below it was never configured.
    let text = CONFIG.replace(
        "order = [\"first\", \"second\", \"third\"]",
        "order = [\"first\"]",
    );
    let search = Search::new(rules(&text));
    let script = Script::new(&[("first", vec![Err(Failure::Limited)])]);
    assert_eq!(
        search.search(&query("q"), &script),
        Err(Missed::Failed("First is rate limited".into()))
    );
}

// ===== what the source is asked =====

#[test]
fn an_unfiltered_search_asks_for_the_configured_count() {
    let search = Search::new(rules(CONFIG));
    let script = Script::new(&[("first", vec![Ok(vec![hit("https://a.test/1")])])]);
    search.search(&query("q"), &script).expect("finds");
    assert_eq!(*script.wanted.borrow(), [3]);
}

#[test]
fn a_filtered_search_asks_for_double_so_the_count_survives_the_filter() {
    // Both the `site:` push and the local filter run, so the request is
    // widened (docs/tools/web-search.md *The walk*).
    let search = Search::new(rules(CONFIG));
    let script = Script::new(&[("first", vec![Ok(vec![hit("https://a.test/1")])])]);
    let filtered = Query {
        text: "q".into(),
        filter: Filter {
            allowed: vec!["a.test".into()],
            blocked: Vec::new(),
        },
    };
    search.search(&filtered, &script).expect("finds");
    assert_eq!(*script.wanted.borrow(), [6]);
}

#[test]
fn the_attempt_carries_the_source_s_own_configuration() {
    // The boundary needs everything to send the request; nothing about the
    // transport is decided here.
    struct Inspect;
    impl Asker for Inspect {
        fn ask(&self, attempt: &Attempt<'_>) -> Result<Vec<Hit>, Failure> {
            assert_eq!(attempt.kind, crate::web::rules::Kind::Parallel);
            assert_eq!(attempt.url, "https://first.test/mcp");
            assert_eq!(attempt.user_agent, "test-agent");
            assert_eq!(attempt.timeout, Duration::from_secs(1));
            Ok(vec![])
        }
    }
    Search::new(rules(CONFIG))
        .search(&query("q"), &Inspect)
        .expect("finds");
}

// ===== configuration edges =====

#[test]
fn a_key_source_is_not_tried_when_nothing_supplies_a_key() {
    let text = CONFIG.replace(
        "[sources.second]\nname = \"Second\"\nkind = \"anysearch\"\nurl = \"https://second.test/search\"\nneeds = \"nothing\"",
        "[sources.second]\nname = \"Second\"\nkind = \"anysearch\"\nurl = \"https://second.test/search\"\nneeds = \"key\"",
    );
    let search = Search::new(rules(&text));
    let script = Script::new(&[
        ("first", vec![Err(Failure::Failed("down".into()))]),
        ("third", vec![Ok(vec![hit("https://c.test/1")])]),
    ]);
    let found = search.search(&query("q"), &script).expect("finds");
    assert_eq!(found.source, "Third", "the key source is skipped");
    assert!(!script.seen.borrow().contains(&"second".to_string()));
}

#[test]
fn an_order_of_nothing_tryable_is_named_as_such() {
    let text = CONFIG.replace(
        "needs = \"nothing\"\n\n[sources.second]",
        "needs = \"key\"\n\n[sources.second]",
    );
    let text = text
        .replace(
            "\"https://second.test/search\"\nneeds = \"nothing\"",
            "\"https://second.test/search\"\nneeds = \"key\"",
        )
        .replace(
            "\"https://third.test/search\"\nneeds = \"nothing\"",
            "\"https://third.test/search\"\nneeds = \"key\"",
        );
    let search = Search::new(rules(&text));
    assert_eq!(
        search.search(&query("q"), &Script::new(&[])),
        Err(Missed::NoSource)
    );
}

#[test]
fn the_session_id_is_stable_and_never_empty() {
    let search = Search::new(rules(CONFIG));
    assert_eq!(search.session(), search.session(), "stable per process");
    assert_eq!(search.session().len(), 32, "sixteen hex digits twice");
    assert!(search.session().chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn the_refusals_read_as_sentences() {
    assert!(
        Missed::Cooling
            .to_string()
            .contains("cooling down after a rate limit")
    );
    assert_eq!(
        Missed::NoSource.to_string(),
        "no search source is configured"
    );
}
