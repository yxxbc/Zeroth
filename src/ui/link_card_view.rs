//! The link preview card's two rows (`docs/ui/rich-links.md`): a **lone** URL
//! line in the screen-owning conversation is replaced by `● site — title` and
//! a dim `description`, fetched and cached by the boundary. Every span carries
//! the URL carrier, so a click anywhere on the card opens the link with the
//! same hit map as the link line it replaced.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::link_card::LinkCard;
use crate::links;

use super::theme::*;
use super::wrap::cols;

/// The card rows for `card` at `width`: a head row (`● site — title`) and,
/// when the page said something, one dim description row. The title and the
/// description are clipped to the row; the carrier is on every cell.
#[must_use]
pub fn link_card_lines(card: &LinkCard, url: &str, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
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
    let mut lines = vec![Line::from(head)];
    if !card.description.is_empty() {
        let room = usize::from(width).saturating_sub(2).max(1);
        lines.push(Line::from(vec![
            Span::styled("  ", links::linked(dim, url)),
            Span::styled(clip(&card.description, room), links::linked(dim, url)),
        ]));
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
