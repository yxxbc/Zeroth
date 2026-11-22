//! Drag-selection: the highlight and the copy agree, and neither splits a
//! grapheme (`docs/ui/full-screen.md` *Selection semantics*).

use super::super::selection::{highlight, slice, text};
use super::super::wrap::cols;
use super::*;
use ratatui::layout::Alignment;

fn line(text: &str) -> Line<'static> {
    Line::from(Span::raw(text.to_string()))
}

fn two_spans(a: &str, b: &str) -> Line<'static> {
    Line::from(vec![Span::raw(a.to_string()), Span::raw(b.to_string())])
}

fn text_of(line: &Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn reversed_spans(line: &Line<'static>) -> Vec<String> {
    line.spans
        .iter()
        .filter(|span| span.style.add_modifier == Modifier::REVERSED)
        .map(|span| span.content.to_string())
        .collect()
}

#[test]
fn a_selection_inside_one_span_splits_it_at_the_edges() {
    // The defect this replaces: reversing every touched span meant copying
    // "ell" out of "hello world" flashed the whole line as selected.
    let painted = highlight(&line("hello world"), 1, 4);
    let runs: Vec<String> = painted
        .spans
        .iter()
        .map(|span| span.content.to_string())
        .collect();
    assert_eq!(runs, ["h", "ell", "o world"], "split at both edges");
    assert_eq!(reversed_spans(&painted), ["ell"]);
    assert_eq!(text_of(&painted), "hello world", "the text is unchanged");
    // …and the copy is exactly what was reversed.
    assert_eq!(slice(&line("hello world"), 1, 4), "ell");
}

#[test]
fn a_selection_across_spans_keeps_each_spans_style() {
    let painted = highlight(&two_spans("bold", " plain"), 2, 8);
    assert_eq!(reversed_spans(&painted), ["ld", " pla"]);
    assert_eq!(slice(&two_spans("bold", " plain"), 2, 8), "ld pla");
}

#[test]
fn a_wide_glyph_is_taken_whole_or_left_out() {
    // Two columns a glyph: a range starting in the middle of one cannot take
    // half of it, so the glyph is left out — and the highlight says so before
    // the copy does.
    // 漢 occupies columns 0–1, 字 2–3, then a, b, c each one column.
    let wide = line("漢字abc");
    assert_eq!(
        slice(&wide, 1, 4),
        "字",
        "a range starting inside 漢 leaves it out rather than half-taking it"
    );
    assert_eq!(slice(&wide, 0, 2), "漢", "exactly its two columns");
    assert_eq!(
        slice(&wide, 0, 3),
        "漢字",
        "columns 0..3 hold 漢 and 字's first column, so 字 is taken whole — \
         the rule is where a glyph starts, never a half glyph"
    );
    assert_eq!(slice(&wide, 2, 6), "字ab");
    let painted = highlight(&wide, 1, 4);
    assert_eq!(reversed_spans(&painted), ["字"], "only the whole glyph");
}

#[test]
fn a_grapheme_cluster_is_never_split() {
    // A ZWJ family and an accented letter are one unit each: a per-char walk
    // (what the old slice did) would have produced a dangling joiner.
    let family = line("👨‍👩‍👧x");
    assert_eq!(cols("👨‍👩‍👧"), 2);
    assert_eq!(slice(&family, 0, 2), "👨‍👩‍👧");
    assert_eq!(slice(&family, 2, 3), "x");
    assert_eq!(slice(&family, 0, 3), "👨‍👩‍👧x");

    let accented = line("éa");
    assert_eq!(slice(&accented, 0, 1), "é");
    assert_eq!(slice(&accented, 1, 2), "a");
}

#[test]
fn a_selection_covers_rows_and_drops_blank_edges() {
    let rows = [line(""), line("first row"), line("second row"), line("")];
    let picked = text(rows.iter(), (0, 0), (3, 5)).expect("a selection");
    assert_eq!(picked, "first row\nsecond row", "blank edges trimmed");
}

#[test]
fn selecting_backwards_is_the_same_selection() {
    let rows = [line("alpha"), line("beta")];
    let forward = text(rows.iter(), (0, 2), (1, 1));
    let backward = text(rows.iter(), (1, 1), (0, 2));
    assert_eq!(forward, backward);
    assert_eq!(forward.as_deref(), Some("pha\nbe"));
}

#[test]
fn a_selection_of_blanks_copies_nothing() {
    let rows = [line("   "), line("")];
    assert_eq!(text(rows.iter(), (0, 0), (1, 2)), None);
}

#[test]
fn highlighting_keeps_the_alignment_and_the_other_styles() {
    let mut base = Line::from(vec![Span::styled(
        "link".to_string(),
        Style::new()
            .fg(Color::Red)
            .add_modifier(Modifier::UNDERLINED),
    )]);
    base = base.alignment(Alignment::Center);
    let painted = highlight(&base, 0, 2);
    assert_eq!(painted.alignment, Some(Alignment::Center));
    let inside = &painted.spans[0];
    assert!(inside.style.add_modifier.contains(Modifier::REVERSED));
    assert!(
        inside.style.add_modifier.contains(Modifier::UNDERLINED),
        "a selected link keeps its underline"
    );
    assert_eq!(inside.style.fg, Some(Color::Red), "and its colour");
}

#[test]
fn the_reversed_cells_in_a_painted_frame_are_the_copied_text() {
    // The strongest form of the rule, checked on a real Buffer: paint the
    // highlighted line, read back which cells came out reversed, and compare
    // that with what `slice` copies. The old code reversed whole spans, so
    // this comparison is exactly what it failed.
    let source = line("hello world");
    let (from, to) = (1u16, 4u16);
    let painted = highlight(&source, from, to);

    let area = Rect::new(0, 0, 20, 1);
    let mut buffer = Buffer::empty(area);
    Paragraph::new(painted).render(area, &mut buffer);
    let mut shown = String::new();
    for x in 0..20 {
        let cell = &buffer[(x, 0)];
        if cell.modifier.contains(Modifier::REVERSED) {
            shown.push_str(cell.symbol());
        }
    }
    assert_eq!(shown, "ell", "exactly the selected cells are reversed");
    assert_eq!(
        slice(&source, from, to),
        shown,
        "and that is what is copied"
    );
}
