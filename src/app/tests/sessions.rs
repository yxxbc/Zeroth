//! Focus routing and conversation switching in the full-screen workspace.
use super::*;

fn workspace() -> App {
    let mut app = App::new();
    app.set_fullscreen(true);
    app.start_agent_group(false, &agent_specs(false));
    app
}

#[test]
fn workspace_walk_separates_focus_from_view_and_ignores_shells() {
    let mut app = workspace();
    app.bg_started("shell", "sleep 99", None, false, None);
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.session_bar_focused(), Some(0));
    assert!(!app.background_focused());
    app.on_key(key(KeyCode::Right));
    assert_eq!(app.session_bar_focused(), Some(1));
    assert!(app.agent_view.is_none());
    assert_eq!(
        app.on_key(key(KeyCode::Enter)),
        Action::ViewAgent("a1".into())
    );
    assert_eq!(app.session_bar_focused(), Some(1));
    app.on_key(key(KeyCode::Left));
    assert_eq!(app.on_key(key(KeyCode::Enter)), Action::LeaveAgentView);
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.session_bar_focused(), None);
    assert_eq!(app.sessions().len(), 2);
}

#[test]
fn workspace_down_respects_history_drafts_help_and_modal_precedence() {
    let mut app = workspace();
    app.input_history.record("first");
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.input.text(), "first");
    app.on_key(key(KeyCode::Down));
    assert!(app.input.is_empty());
    assert_eq!(app.session_bar_focused(), None);
    for draft in ["two\nlines", "/", "@file", "!"] {
        let mut app = workspace();
        app.on_paste(draft);
        app.on_key(key(KeyCode::Down));
        assert_eq!(app.session_bar_focused(), None, "{draft}");
    }
    app.on_key(key(KeyCode::Char('?')));
    app.on_key(key(KeyCode::Down));
    assert!(!app.shortcuts_open);
    assert_eq!(app.session_bar_focused(), None);
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
    assert_eq!(app.session_bar_focused(), None);
    app.on_key(key(KeyCode::Down));
    app.open_ask(AskRequest {
        id: "ask".into(),
        questions: vec![],
    });
    assert_eq!(app.session_bar_focused(), None);
    assert_eq!(app.select_session_tab(Some("a1")), Action::None);
}

#[test]
fn workspace_click_uses_identity_and_preserves_each_draft() {
    let mut app = workspace();
    app.on_paste("main draft");
    app.input.set_cursor(2);
    assert_eq!(
        app.select_session_tab(Some("a1")),
        Action::ViewAgent("a1".into())
    );
    assert!(app.input.is_empty());
    app.on_paste("agent draft");
    assert_eq!(app.select_session_tab(None), Action::LeaveAgentView);
    assert_eq!((app.input.text(), app.input.cursor()), ("main draft", 2));
    app.remove_agent("a1");
    assert_eq!(app.select_session_tab(Some("a1")), Action::None);
    assert_eq!(app.input.text(), "main draft");
}

#[test]
fn workspace_focus_tracks_identity_when_roster_changes() {
    let mut app = workspace();
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Right));
    app.on_key(key(KeyCode::Right));
    app.remove_agent("a1");
    assert_eq!(app.session_bar_focused(), Some(1));
    assert_eq!(
        app.on_key(key(KeyCode::Enter)),
        Action::ViewAgent("a2".into())
    );
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(app.input.text(), "\n");
    assert_eq!(app.session_bar_focused(), None);
}

#[test]
fn workspace_main_only_and_inline_have_distinct_focus_paths() {
    let mut app = app_with_shells(&["sleep 99"]);
    app.on_key(key(KeyCode::Down));
    assert!(app.background_focused());
    assert_eq!(app.session_bar_focused(), None);
    let mut app = App::new();
    app.set_fullscreen(true);
    app.bg_started("shell", "sleep 99", None, false, None);
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.session_bar_focused(), Some(0));
    assert!(!app.background_focused());
}
