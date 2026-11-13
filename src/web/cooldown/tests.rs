//! The ladder's behaviour. Every test passes an `Instant` — none sleeps, so
//! the suite stays at milliseconds.

use super::*;
use std::time::Duration;

/// The shipped ladder: 30 min, 1 h, 2 h, then 6 h forever.
fn shipped() -> Cooldowns {
    Cooldowns::new(
        [30, 60, 120, 360]
            .map(|minutes| Duration::from_secs(minutes * 60))
            .to_vec(),
    )
}

const MIN: u64 = 60;

#[test]
fn a_fresh_ladder_holds_nothing() {
    let now = Instant::now();
    let cooldowns = shipped();
    assert_eq!(cooldowns.until("exa", now), None);
    assert_eq!(cooldowns.strikes("exa"), 0);
    assert!(cooldowns.cooling(now).is_empty());
}

#[test]
fn the_first_limit_holds_for_the_first_rung() {
    let now = Instant::now();
    let mut cooldowns = shipped();
    let until = cooldowns.limited("exa", now);
    assert_eq!(until, now + Duration::from_secs(30 * MIN));
    assert_eq!(cooldowns.until("exa", now), Some(until));
    assert_eq!(cooldowns.strikes("exa"), 1);
}

#[test]
fn every_limit_steps_up_one_rung() {
    let mut cooldowns = shipped();
    let now = Instant::now();
    for expected in [30u64, 60, 120] {
        let until = cooldowns.limited("exa", now);
        assert_eq!(
            until,
            now + Duration::from_secs(expected * MIN),
            "{expected}"
        );
    }
}

#[test]
fn past_the_last_rung_the_ladder_stays_at_the_top() {
    let mut cooldowns = shipped();
    let now = Instant::now();
    for _ in 0..4 {
        cooldowns.limited("exa", now);
    }
    assert_eq!(cooldowns.strikes("exa"), 4);
    for _ in 0..10 {
        assert_eq!(
            cooldowns.limited("exa", now),
            now + Duration::from_secs(360 * MIN),
            "the top rung repeats forever"
        );
    }
}

#[test]
fn a_source_that_answers_goes_back_to_the_bottom_of_the_ladder() {
    // The rule that makes the ladder self-healing: one good answer means the
    // next limit is a fresh 30 minutes, not a 6-hour wait.
    let mut cooldowns = shipped();
    let now = Instant::now();
    for _ in 0..3 {
        cooldowns.limited("exa", now);
    }
    cooldowns.succeeded("exa");
    assert_eq!(cooldowns.strikes("exa"), 0);
    assert_eq!(cooldowns.until("exa", now), None);
    assert_eq!(
        cooldowns.limited("exa", now),
        now + Duration::from_secs(30 * MIN)
    );
}

#[test]
fn a_deadline_passes_on_its_own_with_no_call() {
    // `until` is a pure comparison against `now`, so a source comes back
    // without anything having to run.
    let start = Instant::now();
    let mut cooldowns = shipped();
    let until = cooldowns.limited("exa", start);
    assert!(
        cooldowns
            .until("exa", until - Duration::from_secs(1))
            .is_some()
    );
    assert_eq!(
        cooldowns.until("exa", until),
        None,
        "the deadline is exclusive: at that instant it is back"
    );
    assert_eq!(
        cooldowns.until("exa", until + Duration::from_secs(3600)),
        None
    );
}

#[test]
fn one_source_cooling_does_not_hold_another() {
    let mut cooldowns = shipped();
    let now = Instant::now();
    cooldowns.limited("exa", now);
    assert!(cooldowns.until("exa", now).is_some());
    assert_eq!(cooldowns.until("parallel", now), None);
}

#[test]
fn an_empty_ladder_never_holds_anything() {
    // The file may say so; a source is then always tried.
    let mut cooldowns = Cooldowns::new(Vec::new());
    let now = Instant::now();
    let until = cooldowns.limited("exa", now);
    assert_eq!(until, now);
    assert_eq!(cooldowns.until("exa", now), None);
}

#[test]
fn the_user_sees_the_soonest_return_first() {
    let mut cooldowns = shipped();
    let now = Instant::now();
    cooldowns.limited("exa", now);
    // Two strikes for bing: a longer wait than exa's.
    cooldowns.limited("bing", now);
    cooldowns.limited("bing", now);
    let cooling = cooldowns.cooling(now);
    assert_eq!(
        cooling
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["exa", "bing"],
        "the source coming back soonest is named first"
    );
    assert_eq!(
        cooling[1].1 - now,
        Duration::from_secs(60 * MIN),
        "a second strike is the second rung"
    );
}

#[test]
fn the_ids_are_sorted_so_a_message_is_stable() {
    let mut cooldowns = shipped();
    let now = Instant::now();
    for id in ["tavily", "exa", "bing"] {
        cooldowns.limited(id, now);
    }
    assert_eq!(cooldowns.ids(), ["bing", "exa", "tavily"]);
}
