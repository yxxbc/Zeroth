//! The budget and the cache, on an injected clock — no sleeping, so every
//! boundary (`Measyured` §3, §4) is an assertion.

use super::*;

fn budget() -> Budget {
    Budget {
        max_in_flight: 3,
        max_per_host: 2,
        requests_per_minute: 10,
        cache_ttl: Duration::from_secs(600),
        cache_docs: 3,
        cache_bytes: 100,
    }
}

// ===== the concurrency caps =====

#[test]
fn the_global_cap_holds_across_hosts() {
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    assert_eq!(limiter.acquire("a", now), Granted::Now);
    assert_eq!(limiter.acquire("b", now), Granted::Now);
    assert_eq!(limiter.acquire("c", now), Granted::Now);
    assert!(
        matches!(limiter.acquire("d", now), Granted::At(_)),
        "the fourth waits"
    );
    assert_eq!(limiter.in_flight(), 3);
}

#[test]
fn the_per_host_cap_holds_within_the_global_one() {
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    assert_eq!(limiter.acquire("a", now), Granted::Now);
    assert_eq!(limiter.acquire("a", now), Granted::Now);
    assert!(
        matches!(limiter.acquire("a", now), Granted::At(_)),
        "a third to one host waits even though the global cap has room"
    );
    assert_eq!(limiter.in_flight_at("a"), 2);
    assert_eq!(limiter.in_flight(), 2, "a refusal takes no slot");
}

#[test]
fn releasing_a_slot_lets_the_next_one_in() {
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    limiter.acquire("a", now);
    limiter.acquire("a", now);
    assert!(matches!(limiter.acquire("a", now), Granted::At(_)));
    limiter.release("a");
    assert_eq!(limiter.acquire("a", now), Granted::Now);
}

#[test]
fn releasing_more_than_was_taken_does_not_underflow() {
    let limiter = Limiter::new(budget());
    limiter.release("nobody");
    assert_eq!(limiter.in_flight(), 0);
}

#[test]
fn a_per_minute_cap_names_when_a_place_frees() {
    let limiter = Limiter::new(Budget {
        requests_per_minute: 2,
        ..budget()
    });
    let start = Instant::now();
    assert_eq!(limiter.acquire("a", start), Granted::Now);
    assert_eq!(limiter.acquire("b", start), Granted::Now);
    match limiter.acquire("c", start) {
        Granted::At(at) => assert_eq!(at, start + Duration::from_secs(60)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_minute_window_slides() {
    let limiter = Limiter::new(Budget {
        requests_per_minute: 1,
        ..budget()
    });
    let start = Instant::now();
    limiter.acquire("a", start);
    assert!(matches!(limiter.acquire("a", start), Granted::At(_)));
    // One minute later the first request has left the window.
    assert_eq!(
        limiter.acquire("a", start + Duration::from_secs(61)),
        Granted::Now
    );
}

#[test]
fn the_refused_wait_is_short_so_a_caller_can_give_up_cheaply() {
    // A wait that is polled means Esc costs no slot and no request.
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    for _ in 0..3 {
        limiter.acquire("a", now);
    }
    match limiter.acquire("a", now) {
        Granted::At(at) => assert!(at - now <= Duration::from_millis(100), "{:?}", at - now),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_zero_cap_still_lets_one_through_rather_than_deadlocking() {
    // `max(1)` is deliberate: a misconfigured 0 must not hang every fetch.
    let limiter = Limiter::new(Budget {
        max_in_flight: 0,
        max_per_host: 0,
        requests_per_minute: 0,
        ..budget()
    });
    let now = Instant::now();
    assert_eq!(limiter.acquire("a", now), Granted::Now);
}

// ===== the page cache =====

#[test]
fn a_kept_page_comes_back_while_it_is_fresh() {
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    limiter.keep("https://a.test/x", "hello".into(), now);
    assert_eq!(
        limiter.cached("https://a.test/x", now),
        Some("hello".into())
    );
    assert_eq!(
        limiter.cached("https://a.test/x", now + Duration::from_secs(599)),
        Some("hello".into())
    );
}

#[test]
fn a_page_expires_on_its_own_after_the_ttl() {
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    limiter.keep("https://a.test/x", "hello".into(), now);
    assert_eq!(
        limiter.cached("https://a.test/x", now + Duration::from_secs(601)),
        None,
        "no sweep runs; the read is what notices"
    );
    assert_eq!(limiter.cached_count(), 0);
}

#[test]
fn the_document_count_is_capped_and_the_oldest_goes() {
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    for n in 0..5 {
        limiter.keep(&format!("https://a.test/{n}"), "x".into(), now);
    }
    assert_eq!(limiter.cached_count(), 3);
    assert_eq!(
        limiter.cached("https://a.test/0", now),
        None,
        "oldest evicted"
    );
    assert!(limiter.cached("https://a.test/4", now).is_some());
}

#[test]
fn the_byte_ceiling_evicts_even_below_the_document_count() {
    // The case that actually happens: many small pages.
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    for n in 0..3 {
        limiter.keep(&format!("https://a.test/{n}"), "y".repeat(60), now);
    }
    assert!(limiter.cached_bytes() <= 100, "{}", limiter.cached_bytes());
    assert!(limiter.cached_count() < 3);
}

#[test]
fn one_document_too_large_for_the_ceiling_is_still_served() {
    // Dropping the page just fetched would make the cache useless for the
    // one page a model asked for.
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    limiter.keep("https://a.test/big", "z".repeat(500), now);
    assert!(limiter.cached("https://a.test/big", now).is_some());
}

#[test]
fn re_keeping_a_url_replaces_it_and_adjusts_the_byte_count() {
    let limiter = Limiter::new(budget());
    let now = Instant::now();
    limiter.keep("https://a.test/x", "1".repeat(40), now);
    assert_eq!(limiter.cached_bytes(), 40);
    limiter.keep("https://a.test/x", "2".repeat(10), now);
    assert_eq!(limiter.cached_bytes(), 10);
    assert_eq!(limiter.cached_count(), 1);
    assert_eq!(
        limiter.cached("https://a.test/x", now),
        Some("2".repeat(10))
    );
}

#[test]
fn a_cache_miss_is_none_not_an_empty_page() {
    let limiter = Limiter::new(budget());
    assert_eq!(limiter.cached("https://never.test/", Instant::now()), None);
}
