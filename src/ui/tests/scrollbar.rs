//! The screen-owning conversation's scrollbar (`docs/ui/full-screen.md`).

use super::super::scrollbar::{Cell, render, rows};
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
