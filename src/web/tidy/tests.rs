//! The filter, the dedupe and the caps. The domain cases are the ones that
//! decide whether a `site:`-style narrowing is a filter or a hole.

use super::*;

fn hit(url: &str) -> Hit {
    Hit {
        title: "T".into(),
        url: url.into(),
        snippet: None,
        date: None,
    }
}

fn with_snippet(url: &str, snippet: &str) -> Hit {
    Hit {
        snippet: Some(snippet.into()),
        ..hit(url)
    }
}

fn filter(allowed: &[&str], blocked: &[&str]) -> Filter {
    Filter {
        allowed: allowed.iter().map(|s| Filter::normalize(s)).collect(),
        blocked: blocked.iter().map(|s| Filter::normalize(s)).collect(),
    }
}

// ===== the domain match is on labels =====

#[test]
fn a_domain_admits_itself_and_its_subdomains() {
    let only = filter(&["example.com"], &[]);
    for url in [
        "https://example.com/a",
        "https://www.example.com/a",
        "https://docs.example.com/a",
        "https://a.b.c.example.com/x",
    ] {
        assert!(only.admits(url), "{url} should be admitted");
    }
}

#[test]
fn a_domain_does_not_admit_a_lookalike() {
    // The case that makes this a filter rather than a substring test.
    let only = filter(&["example.com"], &[]);
    for url in [
        "https://notexample.com/a",
        "https://example.com.evil.test/a",
        "https://example.co/a",
        "https://myexample.com/a",
    ] {
        assert!(!only.admits(url), "{url} must not be admitted");
    }
}

#[test]
fn blocking_a_domain_blocks_its_subdomains() {
    let none_blocked = filter(&[], &["reddit.com"]);
    assert!(!none_blocked.admits("https://www.reddit.com/r/rust"));
    assert!(!none_blocked.admits("https://old.reddit.com/r/rust"));
    assert!(none_blocked.admits("https://notreddit.com/x"));
    assert!(none_blocked.admits("https://example.com/x"));
}

#[test]
fn a_block_wins_over_an_allow() {
    // The model can narrow and exclude in one call; the exclusion is the
    // conservative reading of that.
    let both = filter(&["example.com"], &["docs.example.com"]);
    assert!(both.admits("https://example.com/a"));
    assert!(!both.admits("https://docs.example.com/a"));
}

#[test]
fn no_filter_admits_everything_with_a_host() {
    let all = Filter::default();
    assert!(all.is_empty());
    assert!(all.admits("https://anything.test/x"));
}

#[test]
fn an_unreadable_url_cannot_satisfy_an_allow_list() {
    // A hit whose URL cannot be parsed is not evidence of a matching domain,
    // so a narrowed search drops it rather than guessing.
    let only = filter(&["example.com"], &[]);
    assert!(!only.admits("not a url"));
    assert!(Filter::default().admits("not a url"));
}

// ===== normalizing what a model writes =====

#[test]
fn a_domain_is_reduced_to_its_host() {
    for written in [
        "example.com",
        "Example.COM",
        "https://example.com",
        "https://example.com/a/b?c=1",
        "http://user:pw@example.com/x",
        "example.com:8080",
        "  example.com  ",
        "example.com.",
    ] {
        assert_eq!(Filter::normalize(written), "example.com", "{written}");
    }
}

#[test]
fn www_is_stripped_on_both_sides_so_the_two_spellings_are_one_filter() {
    // `host_of` drops `www.` from a hit; a written domain must drop it too, or
    // `www.example.com` and `example.com` become two different filters and a
    // narrowed search returns nothing.
    assert_eq!(
        Filter::normalize("https://www.example.com/x"),
        "example.com"
    );
    assert_eq!(Filter::normalize("www.example.com"), "example.com");
    assert!(filter(&["www.example.com"], &[]).admits("https://example.com/x"));
    assert!(filter(&["example.com"], &[]).admits("https://www.example.com/x"));
}

// ===== dedupe =====

#[test]
fn one_page_under_two_fragments_is_one_hit() {
    let hits = vec![
        hit("https://a.test/page#section-1"),
        hit("https://a.test/page#section-2"),
        hit("https://a.test/page"),
    ];
    assert_eq!(tidy(hits, &Filter::default(), 10, 100).len(), 1);
}

#[test]
fn one_page_with_and_without_a_trailing_slash_is_one_hit() {
    let hits = vec![hit("https://a.test/page/"), hit("https://a.test/page")];
    assert_eq!(tidy(hits, &Filter::default(), 10, 100).len(), 1);
}

#[test]
fn a_capitalised_host_is_the_same_page() {
    let hits = vec![hit("https://A.test/page"), hit("https://a.test/page")];
    assert_eq!(tidy(hits, &Filter::default(), 10, 100).len(), 1);
}

#[test]
fn two_different_pages_survive() {
    let hits = vec![hit("https://a.test/one"), hit("https://a.test/two")];
    assert_eq!(tidy(hits, &Filter::default(), 10, 100).len(), 2);
}

#[test]
fn the_first_of_a_duplicate_pair_wins() {
    // The walk's order is the ranking: the earlier source's copy stays.
    let hits = vec![
        Hit {
            title: "first".into(),
            ..hit("https://a.test/x")
        },
        Hit {
            title: "second".into(),
            ..hit("https://a.test/x")
        },
    ];
    let tidied = tidy(hits, &Filter::default(), 10, 100);
    assert_eq!(tidied[0].title, "first");
}

// ===== the caps =====

#[test]
fn the_result_count_is_applied_after_the_filter() {
    // A narrowed search must still return `most` when it can.
    let hits = vec![
        hit("https://other.test/1"),
        hit("https://example.com/1"),
        hit("https://other.test/2"),
        hit("https://example.com/2"),
    ];
    let tidied = tidy(hits, &filter(&["example.com"], &[]), 2, 100);
    assert_eq!(tidied.len(), 2);
    assert!(tidied.iter().all(|h| h.url.contains("example.com")));
}

#[test]
fn the_excerpt_is_cut_at_the_cap_with_a_marker_inside_it() {
    let long = "a".repeat(2000);
    let tidied = tidy(
        vec![with_snippet("https://a.test/x", &long)],
        &Filter::default(),
        5,
        100,
    );
    let snippet = tidied[0].snippet.as_ref().expect("kept");
    assert_eq!(snippet.chars().count(), 100, "the marker is inside the cap");
    assert!(snippet.ends_with('…'));
}

#[test]
fn a_short_excerpt_is_untouched() {
    let tidied = tidy(
        vec![with_snippet("https://a.test/x", "short")],
        &Filter::default(),
        5,
        100,
    );
    assert_eq!(tidied[0].snippet.as_deref(), Some("short"));
}

#[test]
fn a_multibyte_excerpt_is_cut_on_a_character_boundary() {
    // The bug this prevents: slicing bytes splits a CJK character, and the
    // cell renders mojibake while the wire carries invalid UTF-8.
    let text = "中文摘录".repeat(500);
    let tidied = tidy(
        vec![with_snippet("https://a.test/x", &text)],
        &Filter::default(),
        5,
        50,
    );
    let snippet = tidied[0].snippet.as_ref().expect("kept");
    assert_eq!(snippet.chars().count(), 50);
    assert!(snippet.ends_with('…'));
}

#[test]
fn an_excerpt_never_carries_utf8_split_by_a_byte_style_cut() {
    // Belt and braces: the result must be valid UTF-8 by construction, which
    // taking chars guarantees and keeping the role clear documents.
    let text = "é".repeat(1000);
    let cut = cut_chars(&text, 10);
    assert_eq!(cut.chars().count(), 10);
    assert!(std::str::from_utf8(cut.as_bytes()).is_ok());
}

#[test]
fn a_zero_cap_yields_no_excerpt_rather_than_an_empty_string_in_the_cell() {
    let tidied = tidy(
        vec![with_snippet("https://a.test/x", "text")],
        &Filter::default(),
        5,
        0,
    );
    assert_eq!(tidied[0].snippet, None, "None, not Some(\"\")");
}

#[test]
fn an_excerpt_of_only_whitespace_is_dropped() {
    let tidied = tidy(
        vec![with_snippet("https://a.test/x", "   \n\t ")],
        &Filter::default(),
        5,
        100,
    );
    assert_eq!(tidied[0].snippet, None);
}

#[test]
fn a_hit_with_no_url_is_dropped_before_anything_else() {
    assert!(tidy(vec![hit("")], &Filter::default(), 5, 100).is_empty());
}

#[test]
fn tidy_keeps_the_dates_it_was_given() {
    let dated = Hit {
        date: Some("2026-11-13".into()),
        ..hit("https://a.test/x")
    };
    assert_eq!(
        tidy(vec![dated], &Filter::default(), 5, 100)[0]
            .date
            .as_deref(),
        Some("2026-11-13")
    );
}

// ===== the host reader =====

#[test]
fn a_host_is_read_from_the_common_url_shapes() {
    for (url, expected) in [
        ("https://example.com/x", "example.com"),
        ("https://www.example.com/x", "example.com"),
        ("https://EXAMPLE.com/x", "example.com"),
        ("https://example.com:8080/x", "example.com"),
        ("https://user:pw@example.com/x", "example.com"),
        ("https://[::1]:8080/x", "::1"),
        ("https://example.com", "example.com"),
        ("https://example.com?q=1", "example.com"),
    ] {
        assert_eq!(host_of(url).as_deref(), Some(expected), "{url}");
    }
}

#[test]
fn a_url_without_a_host_has_none() {
    for url in ["not a url", "https://", "https:///path", ""] {
        assert_eq!(host_of(url), None, "{url}");
    }
}
