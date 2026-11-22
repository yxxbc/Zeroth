//! The pinned prompt row: which message it shows and how it is drawn
//! (`docs/ui/conversation-tool-details-and-pin.md`).

use super::super::conversation::{UserPrompt, pinned_prompt, pinned_prompt_line};
use super::super::wrap::cols;

/// One prompt occupying `[start, end)` rows.
fn prompt(start: usize, end: usize, text: &str) -> UserPrompt {
    UserPrompt {
        start,
        end,
        text: text.to_string(),
    }
}

fn text_of(line: &ratatui::text::Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn a_prompt_wholly_above_the_window_is_pinned_and_a_partly_visible_one_is_not() {
    // The rule is the message's *last* row, not its first: a question whose
    // second line is still on screen needs no copy of itself above it
    // (`docs/ui/conversation-tool-details-and-pin.md`).
    let prompts = [prompt(0, 1, "first"), prompt(3, 7, "second")];
    assert_eq!(
        pinned_prompt(&prompts, 8).map(|p| p.text.as_str()),
        Some("second"),
        "wholly above the window: the newest turn is pinned"
    );
    assert_eq!(
        pinned_prompt(&prompts, 7).map(|p| p.text.as_str()),
        Some("second"),
        "the first row past the message pins it"
    );
    for inside in 3..7 {
        assert_eq!(
            pinned_prompt(&prompts, inside),
            None,
            "row {inside} is inside the message (3..7): nothing is pinned"
        );
    }
    assert_eq!(
        pinned_prompt(&prompts, 3),
        None,
        "the newest turn's first row on top: the reader is looking at it, and \
         falling back to the older turn would name the wrong question"
    );
    assert_eq!(
        pinned_prompt(&prompts, 2).map(|p| p.text.as_str()),
        Some("first"),
        "scrolled past the newest turn's start, the turn before is pinned"
    );
    assert_eq!(
        pinned_prompt(&prompts, 1).map(|p| p.text.as_str()),
        Some("first")
    );
    assert_eq!(pinned_prompt(&prompts, 0), None, "nothing above the top");
    assert_eq!(pinned_prompt(&[], 42), None, "no prompts at all");
}

#[test]
fn one_row_prompts_keep_the_old_rule() {
    // A single-line message scrolls off all at once, so "wholly above" and
    // "its first row is above" agree — the change is only for wrapped ones.
    let prompts = [prompt(2, 3, "one line")];
    assert_eq!(pinned_prompt(&prompts, 2), None, "its own row on top");
    assert!(pinned_prompt(&prompts, 3).is_some(), "the row above it");
}

#[test]
fn the_pinned_row_folds_every_line_onto_one() {
    // A prompt's wording, not its layout: the row is the whole message with
    // its breaks folded, so the words that asked the question are what
    // survives — the first drawn line of a long prompt is usually its least
    // informative one.
    let text = "look at this:\n\n- one\n-   two\tthree";
    let line = pinned_prompt_line(text, 200, false).expect("a prompt with text");
    let rendered = text_of(&line);
    assert_eq!(
        rendered.trim_end(),
        "❯ look at this: - one - two three",
        "every break and run of blanks folds to one space"
    );
    assert_eq!(cols(&rendered), 200, "the strip fills the row");
}

#[test]
fn a_long_prompt_is_cut_with_an_ellipsis_at_the_width() {
    let text = "explain why the parser rejects that input and what to change";
    let line = pinned_prompt_line(text, 24, false).expect("a prompt with text");
    let rendered = text_of(&line);
    assert_eq!(cols(&rendered), 24, "the row fills the width exactly");
    assert!(rendered.ends_with('…'), "{rendered:?}");
    assert!(rendered.starts_with("❯ explain why"), "{rendered:?}");
    // The cut happens on a grapheme boundary, so a wide glyph is never split.
    let wide = pinned_prompt_line("漢字漢字漢字漢字", 7, false).expect("a prompt");
    assert_eq!(cols(&text_of(&wide)), 7);
}

#[test]
fn a_narrow_row_keeps_the_bullet_and_nothing_else() {
    // Too narrow for a bullet plus one column of text: the row is cut rather
    // than overflowing the frame.
    for width in 0..3 {
        let line = pinned_prompt_line("something to say", width, false);
        assert!(
            line.is_none_or(|line| cols(&text_of(&line)) <= usize::from(width)),
            "width {width} overflowed"
        );
    }
}

#[test]
fn a_blank_prompt_has_no_row_to_pin() {
    // Nothing to say, no strip: better an empty row of conversation than a
    // bullet over blanks.
    assert!(pinned_prompt_line("", 40, false).is_none());
    assert!(pinned_prompt_line("  \n\t ", 40, false).is_none());
}

#[test]
fn the_pointed_at_row_is_lit() {
    // Hovering says the strip is a control: the same ground, brighter ink.
    let resting = pinned_prompt_line("ask a question", 40, false).expect("a prompt");
    let hovered = pinned_prompt_line("ask a question", 40, true).expect("a prompt");
    let style_of = |line: &ratatui::text::Line<'static>| line.spans[1].style;
    assert_ne!(
        style_of(&resting).fg,
        style_of(&hovered).fg,
        "a hovered row must read as a target"
    );
    assert_eq!(
        style_of(&resting).bg,
        style_of(&hovered).bg,
        "the ground stays the user bubble's"
    );
    assert_eq!(
        style_of(&resting).bg,
        Some(super::super::theme::user_bg_color()),
        "the strip wears the user message's ground"
    );
}
