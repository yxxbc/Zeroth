//! Full-screen session tabs and the hit map of the cells actually painted.
use super::theme::*;
use super::wrap::{cols, ellipsize};
use super::*;

/// Click targets from the last painted frame: the session bar's chips **and**
/// the open permission prompt's options.
///
/// Both are read back from the frame that was actually painted rather than
/// recomputed at click time. That is not a refinement: the prompt's page is
/// built for the live region's height *when it is painted*, and the region can
/// change size between the paint and the click — recomputing then mapped a
/// click on "No" past the end of the option list, which read as "nothing
/// happened".
#[derive(Default)]
pub struct SessionBarHits {
    row: u16,
    tabs: Vec<(Range<u16>, Option<String>)>,
    /// The permission prompt's option rows: each option's screen row range
    /// and its index, as painted.
    options: Vec<(Range<u16>, usize)>,
}

impl SessionBarHits {
    /// `Some(None)` hits main; `None` misses the bar or falls in a gap.
    #[must_use]
    pub fn tab_at(&self, col: u16, row: u16) -> Option<Option<&str>> {
        if self.row != row {
            return None;
        }
        self.tabs
            .iter()
            .find(|(range, _)| range.contains(&col))
            .map(|(_, id)| id.as_deref())
    }

    /// The permission option whose painted rows contain `row`, if any.
    #[must_use]
    pub fn permission_option_at(&self, row: u16) -> Option<usize> {
        self.options
            .iter()
            .find(|(range, _)| range.contains(&row))
            .map(|(_, index)| *index)
    }

    /// Record the open prompt's option rows for this frame.
    pub(super) fn with_options(mut self, options: Vec<(Range<u16>, usize)>) -> Self {
        self.options = options;
        self
    }
}

/// Paint the bar and return the click targets from the same layout.
pub fn render_session_bar(area: Rect, buf: &mut Buffer, app: &App) -> SessionBarHits {
    let mut hits = SessionBarHits {
        row: area.y,
        ..SessionBarHits::default()
    };
    if area.height == 0 || area.width == 0 || !app.fullscreen() {
        return hits;
    }
    let agents = app.sessions();
    let viewed = app.viewed_tab_index();
    let focused = app.session_bar_focused();
    let target = focused.unwrap_or(viewed);
    let mut spans = Vec::new();
    let mut x = 0;
    let width = usize::from(area.width);
    let stride = SESSION_TAB_WIDTH + cols(SESSION_BAR_CHIP_GAP);
    let fit = width.div_ceil(stride).max(1);
    let start = (target + 1).saturating_sub(fit);
    for index in start..=agents.len() {
        let gap = if index == start {
            ""
        } else {
            SESSION_BAR_CHIP_GAP
        };
        let room = width.saturating_sub(x + cols(gap)).min(SESSION_TAB_WIDTH);
        if room == 0 {
            break;
        }
        let run = index.checked_sub(1).map(|i| agents[i]);
        let label = run.map_or(AGENT_MAIN_LABEL, |run| run.description.as_str());
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        let bullet = if index == viewed {
            AGENT_MAIN_BULLET
        } else {
            AGENT_ROW_BULLET
        };
        let prefix = if cols(bullet) < room { bullet } else { "" };
        let text = format!(
            "{prefix}{}",
            ellipsize(
                if label.is_empty() {
                    run.map_or(AGENT_MAIN_LABEL, |run| run.id.as_str())
                } else {
                    &label
                },
                room - cols(prefix)
            )
        );
        let style = if focused == Some(index) {
            Style::new().fg(footer_focus_fg()).bg(footer_focus_bg())
        } else {
            Style::new().fg(match run.map(|run| run.status) {
                Some(crate::agents::AgentStatus::Done) => tool_ok_color(),
                Some(status) if status.is_final() => error_color(),
                _ => footer_color(),
            })
        };
        spans.push(Span::raw(gap));
        x += cols(gap);
        let end = x + cols(&text);
        hits.tabs.push((
            area.x + x as u16..area.x + end as u16,
            run.map(|run| run.id.clone()),
        ));
        spans.push(Span::styled(
            text,
            if index == viewed {
                style.add_modifier(Modifier::BOLD)
            } else {
                style
            },
        ));
        x = end;
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
    hits
}
