//! The link preview card (`docs/ui/rich-links.md`): a **lone** URL line in
//! the screen-owning conversation is replaced by `● site — title`, a dim
//! description, and — when the page gave one and the terminal can draw it — a
//! cover thumbnail on the left. Every text span carries the URL carrier, so a
//! click anywhere on the text opens the link with the same hit map as the link
//! line it replaced; the cover is the image pipeline's (`images::place_in`),
//! reserved as marked spaces exactly like a picture in the conversation.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::images;
use crate::link_card::LinkCard;
use crate::links;

use super::theme::*;
use super::wrap::cols;

/// The tallest a card's cover may be, in rows — the reference's
/// `link_cover_rows` idea; the width is a third of the card, and the cell fit
/// keeps the aspect ratio inside both.
const COVER_MAX_ROWS: u16 = 6;
/// The gap between the cover and the text beside it.
const COVER_GAP: u16 = 2;

/// The card rows for `card` at `width`. With a usable cover: a thumbnail on
/// the left (at most a third of the width and [`COVER_MAX_ROWS`] rows) and the
/// head/description to its right. Without one — no image, images off, a
/// terminal that can't draw a picture, a narrow card — the two text rows alone.
#[must_use]
pub fn link_card_lines(card: &LinkCard, url: &str, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    if let Some(cover) = &card.cover {
        let cover_cols = (width / 3).max(1);
        if let Some(place) = images::place_in(&cover.path, cover.px, cover_cols, COVER_MAX_ROWS) {
            let text_width = width.saturating_sub(place.cols + COVER_GAP);
            if text_width >= 8 {
                return with_cover(card, url, place, text_width);
            }
        }
    }
    text_lines(card, url, width)
        .into_iter()
        .map(Line::from)
        .collect()
}

/// The card with its cover: `place` rows of marked spaces on the left, the
/// text rows beside them. A row past either column's height keeps blank cells
/// on that side, so the block stays rectangular.
fn with_cover(
    card: &LinkCard,
    url: &str,
    place: images::Placement,
    text_width: u16,
) -> Vec<Line<'static>> {
    let text: Vec<Vec<Span<'static>>> = text_lines(card, url, text_width);
    let height = usize::from(place.rows).max(text.len());
    let mut rows = Vec::with_capacity(height);
    for row in 0..height {
        let mut spans = Vec::with_capacity(4);
        if u16::try_from(row).is_ok_and(|row| row < place.rows) {
            spans.push(Span::styled(
                " ".repeat(usize::from(place.cols)),
                Style::default().underline_color(images::carrier(place.id, row as u16)),
            ));
        } else {
            spans.push(Span::raw(" ".repeat(usize::from(place.cols))));
        }
        spans.push(Span::raw(" ".repeat(usize::from(COVER_GAP))));
        if let Some(line) = text.get(row) {
            spans.extend(line.clone());
        }
        rows.push(Line::from(spans));
    }
    rows
}

/// The text-only rows, at `width`: the `● site — title` head, then the dim
/// description when there is one.
fn text_lines(card: &LinkCard, url: &str, width: u16) -> Vec<Vec<Span<'static>>> {
    let dim = Style::new().fg(tool_dim_color());
    let title = Style::new()
        .fg(link_url_color())
        .add_modifier(Modifier::BOLD);
    let site = card.site.trim();
    let mut head = Vec::with_capacity(5);
    head.push(Span::styled("● ", links::linked(dim, url)));
    let mut used = 2;
    if !site.is_empty() {
        head.push(Span::styled(format!("{site} "), links::linked(dim, url)));
        head.push(Span::styled("— ", links::linked(dim, url)));
        used += cols(site) + 3;
    }
    let room = usize::from(width).saturating_sub(used).max(1);
    head.push(Span::styled(
        clip(&card.title, room),
        links::linked(title, url),
    ));
    let mut lines = vec![head];
    if !card.description.is_empty() {
        let room = usize::from(width).saturating_sub(2).max(1);
        lines.push(vec![
            Span::styled("  ", links::linked(dim, url)),
            Span::styled(clip(&card.description, room), links::linked(dim, url)),
        ]);
    }
    lines
}

/// `text` cut to `room` display columns, a `…` marking the cut. Wide glyphs
/// are kept whole and never overflow the row.
fn clip(text: &str, room: usize) -> String {
    if cols(text) <= room {
        return text.to_string();
    }
    let room = room.saturating_sub(1).max(1);
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > room {
            break;
        }
        used += width;
        out.push(ch);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::style_link;

    fn card() -> LinkCard {
        LinkCard {
            title: "A very long page title that will not fit the card row".to_string(),
            site: "example.com".to_string(),
            description: "What the page is about".to_string(),
            ..LinkCard::default()
        }
    }

    #[test]
    fn every_cell_carries_the_link_and_rows_stay_in_width() {
        for width in [24u16, 40, 80] {
            let rows = link_card_lines(&card(), "https://example.com/x", width);
            assert!(!rows.is_empty());
            for row in &rows {
                let text: String = row.spans.iter().map(|span| span.content.as_ref()).collect();
                assert!(cols(&text) <= usize::from(width), "width {width}: {text:?}");
                for span in &row.spans {
                    assert_eq!(
                        style_link(&span.style).as_deref(),
                        Some("https://example.com/x"),
                        "every span opens the link"
                    );
                }
            }
            assert!(
                rows[0]
                    .spans
                    .iter()
                    .any(|span| span.content.contains("example.com"))
            );
        }
    }

    #[test]
    fn a_long_title_is_clipped_with_an_ellipsis() {
        let rows = link_card_lines(&card(), "https://example.com", 32);
        let text: String = rows[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.ends_with('…'), "{text:?}");
    }
}
