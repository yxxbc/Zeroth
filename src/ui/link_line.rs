//! A **lone** URL line as a one-row link — `✦ github.com/yxxbc/zeroth` — the
//! brand mark, the host, and the path dim, every cell clickable through the
//! same OSC 8 carrier the rest of the conversation uses (`docs/ui/links.md`).
//!
//! This replaced the fetched preview card: a card meant one HTTP request per
//! lone link, on content the model wrote, with a cover download behind it
//! (`docs/ui/rich-links.md` *The ask*, and the privacy review in
//! `docs/research/harness-network-security.md`). The row needs no network at
//! all — the host already says where the link goes, and the terminal owns the
//! click — so a link in a reply costs exactly what the text costs.
//!
//! Pure: a URL in, rows out, with the wrap and the link dress from the modules
//! that already own them.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::brand::Brand;
use crate::links;

use super::theme::*;
use super::wrap::cols;

/// The rows a lone URL becomes. `None` when `text` is not a whole URL — the
/// caller then keeps its own rendering.
///
/// The row is `{mark} {host}{path}`: the site alone when the path is empty or
/// just `/`, else the path dim after it — never the scheme, which no reader
/// needs twice. The host (and anything after it) is cut with a trailing `…`
/// at the row's width.
#[must_use]
pub fn link_line_lines(text: &str, width: u16) -> Option<Vec<Line<'static>>> {
    let url = text.trim();
    if !links::is_whole_url(url) {
        return None;
    }
    let host = crate::web::host_of(url)?;
    let brand = Brand::of_host(&host).unwrap_or(Brand::Docs);
    let mark = brand.glyph();
    // Everything after the authority, verbatim: the path, query and fragment
    // as the URL spelled them.
    let rest = url
        .split_once("://")
        .map(|(_, tail)| tail)
        .unwrap_or_default();
    let after_authority = rest
        .find(['/', '?', '#'])
        .map(|at| &rest[at..])
        .unwrap_or_default();
    let path = if after_authority == "/" {
        ""
    } else {
        after_authority
    };
    // Two spaces of indent are the conversation's bullet column; the mark and
    // its gap follow, and the rest is the address.
    let prefix_cols = cols(LINK_LINE_INDENT) + BRAND_MARK_COLS;
    let room = usize::from(width).saturating_sub(prefix_cols).max(1);
    let host_cols = cols(&host);
    let path_room = room.saturating_sub(host_cols);
    let (host, path) = if host_cols > room {
        (fit(&host, room), String::new())
    } else if path.is_empty() || path_room == 0 {
        (host, String::new())
    } else {
        (host, fit(path, path_room))
    };
    let link_style = Style::new()
        .fg(link_url_color())
        .add_modifier(Modifier::UNDERLINED);
    Some(vec![Line::from(vec![
        Span::styled(
            format!("{LINK_LINE_INDENT}{mark} "),
            Style::new().fg(brand_color(brand.tone())),
        ),
        Span::styled(host, links::linked(link_style, url)),
        Span::styled(path, links::linked(link_style, url)),
    ])])
}

/// `text` cut to `room` display columns, an `…` marking the cut — wide glyphs
/// are kept whole, and the marker takes the room it needs, so the result never
/// outgrows the caller's budget.
fn fit(text: &str, room: usize) -> String {
    if cols(text) <= room {
        return text.to_string();
    }
    let room = room.saturating_sub(1).max(1);
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
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

    #[test]
    fn a_lone_url_becomes_a_marked_one_row_link() {
        let rows = link_line_lines("https://github.com/yxxbc/zeroth", 80).expect("a link row");
        assert_eq!(rows.len(), 1, "one row, never a card");
        let text: String = rows[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "  ⎇ github.com/yxxbc/zeroth");
        assert_eq!(
            style_link(&rows[0].spans[1].style).as_deref(),
            Some("https://github.com/yxxbc/zeroth")
        );
        assert_eq!(
            style_link(&rows[0].spans[2].style).as_deref(),
            Some("https://github.com/yxxbc/zeroth")
        );
    }

    #[test]
    fn a_bare_host_link_spends_no_trailing_slash() {
        let rows = link_line_lines("https://example.com/", 80).expect("a link row");
        let text: String = rows[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "  ↗ example.com");
    }

    #[test]
    fn the_row_never_overflows_its_width() {
        for width in [12u16, 20, 40] {
            let rows = link_line_lines(
                "https://example.com/a/very/long/path/that/keeps/going?q=1",
                width,
            )
            .expect("a link row");
            let text: String = rows[0]
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            assert!(cols(&text) <= usize::from(width), "{width}: {text:?}");
        }
    }

    #[test]
    fn prose_and_inline_urls_keep_their_own_rendering() {
        for text in [
            "see https://example.com/a",
            "just words",
            "file:///etc/passwd",
            "```https://x.test```",
        ] {
            assert!(link_line_lines(text, 80).is_none(), "{text:?}");
        }
    }
}
