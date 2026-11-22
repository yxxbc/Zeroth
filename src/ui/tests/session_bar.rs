use super::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn workspace_bar_hit_map_matches_display_cells_and_windows_to_focus() {
    let mut app = App::new();
    app.set_fullscreen(true);
    app.start_agent_group(
        false,
        &[crate::stream::AgentSpec {
            id: "a1".into(),
            description: "界面測試".into(),
            agent_type: "test".into(),
            prompt: "task".into(),
            background: false,
            call_id: None,
            arguments: None,
        }],
    );
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let area = Rect::new(2, 3, 7, 1);
    let mut buf = Buffer::empty(area);
    let hits = super::super::session_bar::render_session_bar(area, &mut buf, &app);
    assert_eq!(hits.tab_at(2, 3), Some(Some("a1")));
    assert_eq!(hits.tab_at(5, 3), Some(Some("a1")));
    assert_eq!(hits.tab_at(9, 3), None);
    assert_eq!(hits.tab_at(2, 2), None);
    assert_eq!(buf[(4, 3)].symbol(), "界");
}
