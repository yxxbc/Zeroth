//! The screen-owning mode's sidebar (`docs/ui/sidebar.md`).

use super::super::sidebar::{NARROW_COLS, SECTIONS, WIDE_COLS, lines, width};
use super::super::wrap::cols;
use super::*;
use crate::app::{Message, Role};
use crate::tasks::TaskStore;

fn text_of(line: &Line<'static>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

fn rendered(app: &App, width: u16) -> Vec<String> {
    lines(app, width, 40).iter().map(text_of).collect()
}

#[test]
fn the_width_is_three_bands_wide_narrow_and_hidden() {
    // The terminal-sidebar convention: full column on a wide screen, a
    // narrowed one in the middle, nothing at all when the conversation needs
    // the room more (`docs/ui/sidebar.md`).
    assert_eq!(width(80), 0, "80 columns keeps the conversation whole");
    assert_eq!(width(96), NARROW_COLS);
    assert_eq!(width(111), NARROW_COLS);
    assert_eq!(width(112), WIDE_COLS);
    assert_eq!(width(200), WIDE_COLS);
    assert!(NARROW_COLS < WIDE_COLS && width(0) == 0);
}

#[test]
fn the_sections_are_a_registry_not_a_layout() {
    // The modularity contract: the stack is `SECTIONS`, in order, and a
    // section that says nothing is skipped whole — heading and all. A new
    // module is a new entry, never an edit to the layout.
    let ids: Vec<&str> = SECTIONS.iter().map(|section| section.id).collect();
    assert_eq!(ids, ["brand", "session", "running"]);
    let app = App::new();
    let rows = rendered(&app, NARROW_COLS);
    // A fresh session has no history to run anything, so the running section
    // is absent rather than an empty heading.
    assert!(
        !rows.iter().any(|row| row == "running"),
        "an empty section left its heading behind: {rows:?}"
    );
    assert!(rows.iter().any(|row| row.contains("Zeroth")), "{rows:?}");
}

#[test]
fn the_brand_carries_the_name_the_version_and_the_slogan() {
    let app = App::new();
    let rows = rendered(&app, WIDE_COLS);
    assert!(
        rows.iter().any(|row| row.starts_with("Zeroth v")),
        "{rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("coding agent")),
        "the slogan is the sidebar's one line of voice: {rows:?}"
    );
}

#[test]
fn the_session_facts_read_as_a_table() {
    let mut app = App::new();
    app.set_session_info("deepseek-v4", "~/Projects/zeroth");
    let rows = rendered(&app, WIDE_COLS);
    assert!(rows.iter().any(|row| row == "session"), "{rows:?}");
    assert!(
        rows.iter().any(|row| row.starts_with("model deepseek-v4")),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.starts_with("cwd ~/Projects/zeroth")),
        "{rows:?}"
    );
}

#[test]
fn a_long_value_is_cut_inside_the_column() {
    // A long cwd must not push the sidebar over its seam.
    let mut app = App::new();
    app.set_session_info(
        "a-very-long-model-name-that-cannot-fit",
        "/home/someone/a/deeply/nested/project/path",
    );
    for row in &rendered(&app, NARROW_COLS) {
        assert!(
            cols(row) <= usize::from(NARROW_COLS),
            "a row overflowed the column: {row:?}"
        );
    }
}

#[test]
fn what_is_running_shows_up_and_finished_things_do_not() {
    let mut app = App::new();
    app.history.push(HistoryItem::Message(Message {
        role: Role::User,
        text: "go".into(),
        timestamp: String::new(),
        images: Vec::new(),
    }));
    app.record_task_call(
        "TaskCreate",
        "a subject",
        "{}",
        "Task #1 created successfully: write the sidebar",
        true,
        store_of(&["write the sidebar"]),
    );
    let running = rendered(&app, WIDE_COLS);
    assert!(running.iter().any(|row| row == "running"), "{running:?}");
    assert!(
        running.iter().any(|row| row.contains("write the sidebar")),
        "{running:?}"
    );

    // A completed task is not "running": the section counts what is left.
    app.record_task_call("TaskUpdate", "{}", "{}", "done", true, TaskStore::default());
    let idle = rendered(&app, WIDE_COLS);
    assert!(!idle.iter().any(|row| row == "running"), "{idle:?}");
}

#[test]
fn the_stack_is_cut_to_the_height_it_is_given() {
    // A short terminal clips the stack from the bottom, never from the top —
    // the brand is the last thing that may disappear.
    let app = App::new();
    let rows = lines(&app, WIDE_COLS, 3);
    assert_eq!(rows.len(), 3);
    assert!(text_of(&rows[0]).contains("Zeroth"));
}

#[test]
fn a_background_shell_is_counted() {
    let mut app = App::new();
    // One running shell: the section counts what is live, not what exists.
    app.bg_started("b1", "npm run dev", None, true, None);
    let rows = rendered(&app, WIDE_COLS);
    assert!(rows.iter().any(|row| row.contains("1 running")), "{rows:?}");
}

#[test]
fn the_task_marks_distinguish_in_flight_from_waiting() {
    let mut app = App::new();
    app.history.push(HistoryItem::Message(Message {
        role: Role::User,
        text: "go".into(),
        timestamp: String::new(),
        images: Vec::new(),
    }));
    let mut store = store_of(&["first", "second"]);
    store
        .run_update(&serde_json::json!({"taskId": "1", "status": "in_progress"}).to_string())
        .expect("an update lands");
    app.record_task_call("TaskUpdate", "{}", "{}", "started", true, store);
    let rows = rendered(&app, WIDE_COLS);
    assert!(
        rows.iter().any(|row| row.starts_with("▸ first")),
        "the task in flight wears the marker: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.starts_with("· second")),
        "a waiting task wears the dot: {rows:?}"
    );
}

/// `n` tasks, by subject — the same fixture `app::tests::tasks` uses.
fn store_of(subjects: &[&str]) -> TaskStore {
    let mut store = TaskStore::new();
    for subject in subjects {
        store
            .run_create(&serde_json::json!({"subject": subject, "description": "d"}).to_string())
            .expect("create succeeds");
    }
    store
}

#[test]
fn the_column_is_opt_in_and_the_setting_gates_the_bands() {
    // `width` is the band policy; `columns` is what the frame and the wrap
    // width call, and it answers 0 until the **Sidebar** setting is on
    // (`docs/ui/sidebar.md`). The default is off: a new session keeps the
    // conversation's columns.
    let mut app = App::new();
    assert_eq!(sidebar_columns(&app, 140), 0, "off by default");
    assert_eq!(sidebar_columns(&app, 100), 0);
    app.settings_mut().sidebar = true;
    assert_eq!(sidebar_columns(&app, 140), WIDE_COLS);
    assert_eq!(sidebar_columns(&app, 100), NARROW_COLS);
    assert_eq!(sidebar_columns(&app, 80), 0, "a narrow screen still wins");
}
