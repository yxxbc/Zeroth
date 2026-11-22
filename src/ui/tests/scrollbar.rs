//! The screen-owning conversation's scrollbar (`docs/ui/full-screen.md`).

use super::super::scrollbar::{Cell, render, rows, thumb, top_after_drag, top_for_row};
use super::*;

fn shape(cells: &[Cell]) -> String {
    cells
        .iter()
        .map(|cell| match cell {
            Cell::Blank => ' ',
            Cell::Track => '-',
            Cell::Thumb => '#',
        })
        .collect()
}

#[test]
fn nothing_to_scroll_paints_nothing() {
    // A full-length thumb on a conversation that all fits says nothing — and a
    // blank column is one less thing to read.
    assert_eq!(rows(20, 20, 0, 10), None);
    assert_eq!(rows(5, 20, 0, 10), None);
    assert_eq!(rows(20, 0, 0, 10), None, "no window, no bar");
    assert_eq!(rows(20, 10, 0, 0), None, "no rows, no bar");
}

#[test]
fn the_thumb_sits_at_the_top_at_the_start_and_at_the_bottom_at_the_end() {
    // 100 rows in a 10-row window: a 10% thumb, one cell, that travels the
    // whole track.
    assert_eq!(shape(&rows(100, 10, 0, 10).unwrap()), "#---------");
    assert_eq!(shape(&rows(100, 10, 45, 10).unwrap()), "-----#----");
    assert_eq!(shape(&rows(100, 10, 90, 10).unwrap()), "---------#");
}

#[test]
fn a_short_conversation_gets_a_long_thumb() {
    // The thumb is the share of the conversation on screen: 50 of 100 rows
    // fills half the track and travels half of it.
    assert_eq!(shape(&rows(100, 50, 0, 8).unwrap()), "####----");
    assert_eq!(shape(&rows(100, 50, 50, 8).unwrap()), "----####");
    assert_eq!(shape(&rows(100, 50, 25, 8).unwrap()), "--####--");
}

#[test]
fn the_thumb_is_never_empty_and_never_past_the_track() {
    for (len, window) in [(1000, 1), (101, 100), (10_000, 3)] {
        let cells = rows(len, window, 0, 12).expect("scrolling");
        assert_eq!(cells.len(), 12);
        assert!(cells.contains(&Cell::Thumb), "len {len}: no thumb at all");
        let count = cells.iter().filter(|c| **c == Cell::Thumb).count();
        assert!((1..=12).contains(&count), "len {len}: {count} thumb cells");
    }
}

#[test]
fn a_top_past_the_end_is_clamped_like_every_other_scroll() {
    // The scroll keys already clamp; the bar clamps again rather than painting
    // a thumb off the end of the track.
    let clamped = rows(100, 10, 90, 10).unwrap();
    assert_eq!(rows(100, 10, 9_999, 10).unwrap(), clamped);
}

#[test]
fn the_thumb_helper_agrees_with_the_painted_cells() {
    // The click and drag mappings need the thumb's own rows, so the two must
    // not drift apart from what `rows` paints.
    assert_eq!(thumb(100, 10, 45, 10), Some((5, 1)));
    assert_eq!(thumb(100, 50, 0, 8), Some((0, 4)));
    assert_eq!(thumb(20, 20, 0, 10), None, "nothing to scroll, no thumb");
    let cells = rows(100, 10, 45, 10).expect("scrolling");
    let (start, size) = thumb(100, 10, 45, 10).expect("scrolling");
    assert!(cells[start..start + size].iter().all(|c| *c == Cell::Thumb));
}

#[test]
fn a_track_click_puts_the_pointer_row_at_the_thumb() {
    // 100 rows in a 10-row window: a one-cell thumb that travels 9 rows of
    // track, so the clicked row is the share of the conversation above it.
    assert_eq!(top_for_row(100, 10, 10, 0), Some(0));
    assert_eq!(top_for_row(100, 10, 10, 5), Some(50));
    assert_eq!(top_for_row(100, 10, 10, 9), Some(90));
    // A short thumb (half the conversation on screen) travels 4 rows of 8.
    assert_eq!(top_for_row(100, 50, 8, 0), Some(0));
    assert_eq!(top_for_row(100, 50, 8, 2), Some(25));
    assert_eq!(top_for_row(100, 50, 8, 4), Some(50));
}

#[test]
fn a_track_click_beyond_either_end_clamps_to_the_end() {
    // A pointer past the last track row asks for the tail, not a scroll past
    // it, and a click on the first row is the top.
    assert_eq!(top_for_row(100, 10, 10, 9_999), Some(90));
    assert_eq!(top_for_row(100, 10, 10, 0), Some(0));
}

#[test]
fn dragging_the_thumb_moves_the_window_proportionally() {
    // A press with no travel must not jump: the thumb stays where it was
    // grabbed (the rounding in `rows` is not invertible exactly).
    assert_eq!(top_after_drag(100, 10, 10, 50, 0), Some(50));
    assert_eq!(top_after_drag(100, 10, 10, 0, 3), Some(30));
    assert_eq!(top_after_drag(100, 10, 10, 50, -3), Some(20));
    // A short thumb moves 12.5 rows of conversation per row of track.
    assert_eq!(top_after_drag(100, 50, 8, 0, 1), Some(13));
    assert_eq!(top_after_drag(100, 50, 8, 50, -1), Some(37));
}

#[test]
fn dragging_beyond_either_end_clamps() {
    assert_eq!(top_after_drag(100, 10, 10, 85, 2), Some(90));
    assert_eq!(top_after_drag(100, 10, 10, 5, -50), Some(0));
    assert_eq!(top_after_drag(100, 50, 8, 30, 99), Some(50));
}

#[test]
fn a_conversation_that_fits_has_no_bar_to_aim_at() {
    // Nothing to scroll: the column stays blank, so neither a click nor a
    // drag maps to a position.
    assert_eq!(top_for_row(20, 20, 10, 5), None);
    assert_eq!(top_for_row(5, 20, 10, 5), None);
    assert_eq!(top_after_drag(20, 20, 10, 0, 3), None);
    assert_eq!(top_for_row(20, 10, 0, 0), None, "no rows, no bar");
    assert_eq!(top_for_row(20, 0, 10, 0), None, "no window, no bar");
}

#[test]
fn the_ends_start_the_window_and_resume_the_tail() {
    // The mapping feeds `scroll_anchor`, the one rule every scroll uses: the
    // first track row is the top (anchored), the last is the tail (`None`), so
    // a click or drag to the bottom resumes follow-tail.
    let (len, window) = (100, 10);
    let top = top_for_row(len, window, 10, 0).unwrap();
    assert_eq!(scroll_anchor(top, len, window), Some(0));
    let bottom = top_for_row(len, window, 10, 9).unwrap();
    assert_eq!(bottom, len - window);
    assert_eq!(scroll_anchor(bottom, len, window), None);
    // A drag that runs past the end lands on the same tail anchor.
    let past = top_after_drag(len, window, 10, 50, 999).unwrap();
    assert_eq!(scroll_anchor(past, len, window), None);
}

#[test]
fn the_bar_never_draws_past_its_column() {
    // The right-edge column is one wide; `render` writes down it and stops at
    // the area's last row, so a short area cannot scribble over the composer.
    let area = Rect::new(79, 0, 1, 4);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 10));
    let cells = rows(100, 10, 0, 10).expect("scrolling");
    render(area, &mut buf, &cells, 0);
    let drawn: String = (0..10)
        .map(|row| buf[(79, row)].symbol().to_string())
        .collect();
    assert_eq!(
        drawn, "▐         ",
        "the thumb only: the track is blank, so it cannot read as a second \
         divider beside the sidebar's seam"
    );
}
