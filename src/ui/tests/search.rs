use super::*;
use crate::ui::tool::tool_full_lines;
use crate::ui::tool_group::tool_group_lines;
use crate::ui::wrap::cols;

fn search(command: &str, output: &str) -> ToolCall {
    let mut call = tool("Bash", command, ToolStatus::Ok, output);
    call.run.exit_code = Some(0);
    call.run.duration_ms = Some(18);
    call
}

fn text(lines: &[Line<'_>]) -> String {
    lines.iter().map(plain).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_search_group_counts_calls_and_keeps_the_query_and_results() {
    let items = [
        HistoryItem::Tool(search(
            "rg -n --no-heading 'card' src",
            "src/a.rs:12:fn card() {}\nsrc/a.rs:18:card();",
        )),
        HistoryItem::Tool(search("rg --files src", "src/a.rs\nsrc/b.rs")),
    ];
    let rendered = text(&tool_group_lines(&items, 100, &PathDisplay::VERBATIM));
    assert!(rendered.contains("Ran 2 searches"), "{rendered}");
    assert!(
        !rendered.contains("pattern"),
        "calls are not pattern counts: {rendered}"
    );
    assert!(rendered.contains("card"), "{rendered}");
    assert!(rendered.contains("2 matches in 1 file"), "{rendered}");
    assert!(rendered.contains("2 files"), "{rendered}");
}

#[test]
fn expanded_search_groups_matches_and_separates_the_original_command() {
    let call = search(
        "rg -n --no-heading 'card' src",
        "src/a.rs:12:fn card() {}\nsrc/a.rs:18:card();\nsrc/b.rs:5:use card;",
    );
    let original = call.clone();
    let rendered = text(&tool_full_lines(&call, 100, &PathDisplay::VERBATIM));
    assert!(rendered.contains("3 matches in 2 files"), "{rendered}");
    assert_eq!(rendered.matches("src/a.rs").count(), 1, "{rendered}");
    for value in [
        "12 │ fn card() {}",
        "18 │ card();",
        "5 │ use card;",
        "Command",
        &call.args,
        "exit 0",
    ] {
        assert!(rendered.contains(value), "missing {value}: {rendered}");
    }
    assert_eq!(
        call, original,
        "presentation must not rewrite execution records"
    );
}

#[test]
fn unfamiliar_search_output_stays_verbatim_and_does_not_claim_a_match_count() {
    let call = search(
        "rg -n card src | head -5",
        "src/a.rs:12:first\nnot a match row\n",
    );
    let rendered = text(&tool_full_lines(&call, 100, &PathDisplay::VERBATIM));
    assert!(rendered.contains("src/a.rs:12:first"), "{rendered}");
    assert!(rendered.contains("not a match row"), "{rendered}");
    assert!(!rendered.contains("matches in"), "{rendered}");
}

#[test]
fn web_search_details_distinguish_titles_links_and_excerpts() {
    let output = crate::llm::tools::format_hits(
        "fixture",
        &[
            crate::web::Hit {
                title: "Tokio select".into(),
                url: "https://example.test/select".into(),
                date: Some("2026-11-17".into()),
                snippet: Some("Wait for concurrent branches.".into()),
            },
            crate::web::Hit {
                title: "Tokio tasks".into(),
                url: "https://example.test/tasks".into(),
                date: None,
                snippet: Some("Spawn a task.".into()),
            },
        ],
    );
    let call = tool("WebSearch", "tokio", ToolStatus::Ok, &output);
    let original = call.clone();
    let rendered = text(&tool_full_lines(&call, 100, &PathDisplay::VERBATIM));
    for value in [
        "2 results",
        "Tokio select",
        "https://example.test/select",
        "2026-11-17",
        "Wait for concurrent branches.",
        "Tokio tasks",
    ] {
        assert!(rendered.contains(value), "missing {value}: {rendered}");
    }
    assert!(
        !rendered.contains("change your identity"),
        "model instructions are not a result excerpt: {rendered}"
    );
    assert_eq!(call, original);
}

#[test]
fn search_details_wrap_unicode_paths_and_source_without_losing_content() {
    let call = search(
        "rg -n --no-heading 测试 src",
        "src/很长的目录/模块.rs:12:let message = \"测试中文路径和比较长的源代码行\";",
    );
    for width in [24, 40, 80, 120] {
        let lines = tool_full_lines(&call, width, &PathDisplay::VERBATIM);
        assert!(
            lines
                .iter()
                .all(|row| cols(&plain(row)) <= usize::from(width)),
            "width {width}: {}",
            text(&lines)
        );
        let joined: String = lines
            .iter()
            .map(plain)
            .collect::<String>()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect();
        assert!(joined.contains("比较长的源代码行"), "{joined}");
    }
}

#[test]
fn search_errors_stay_visible_instead_of_becoming_a_successful_group() {
    let mut call = search(
        "rg -n needle missing",
        "Exit code: 2\nrg: missing: No such file or directory",
    );
    call.status = ToolStatus::Failed;
    call.run.exit_code = Some(2);
    assert!(!crate::app::is_group_member(&call, &PathDisplay::VERBATIM));
    let rendered = text(&tool_full_lines(&call, 80, &PathDisplay::VERBATIM));
    assert!(rendered.contains("No such file or directory"), "{rendered}");
    assert!(!rendered.contains("No matches"), "{rendered}");
}

#[test]
fn a_known_no_match_exit_is_not_presented_as_an_execution_error() {
    let mut call = search("rg -n needle src", "Exit code: 1\n");
    call.status = ToolStatus::Failed;
    call.run.exit_code = Some(1);
    let lines = tool_full_lines(&call, 80, &PathDisplay::VERBATIM);
    assert!(text(&lines).contains("No matches"), "{}", text(&lines));
    assert_ne!(
        lines[0].spans[0].style.fg,
        Some(crate::ui::theme::tool_fail_color())
    );
}

#[test]
fn partial_search_results_are_not_claimed_to_be_exhaustive() {
    let mut call = search("rg -n needle src", "src/a.rs:1:needle");
    call.truncated = true;
    let rendered = text(&tool_full_lines(&call, 100, &PathDisplay::VERBATIM));
    assert!(rendered.contains("retained results"), "{rendered}");
    assert!(
        rendered.contains(crate::ui::theme::TOOL_TRUNCATED_MARKER),
        "{rendered}"
    );
}

#[test]
fn unsupported_search_modes_and_malformed_web_results_keep_all_output() {
    for command in [
        "rg -c needle src",
        "rg -n -C 3 needle src",
        "rg -q needle src",
        "rg --json needle src",
    ] {
        let call = search(command, "raw output that must survive");
        let rendered = text(&tool_full_lines(&call, 100, &PathDisplay::VERBATIM));
        assert!(rendered.contains(command), "{rendered}");
        assert!(
            rendered.contains("raw output that must survive"),
            "{rendered}"
        );
        assert!(!rendered.contains("matches in"), "{rendered}");
    }
    let call = tool(
        "WebSearch",
        "query",
        ToolStatus::Ok,
        "[1] title\nmalformed link\nkeep this too",
    );
    assert!(text(&tool_full_lines(&call, 80, &PathDisplay::VERBATIM)).contains("keep this too"));
}

#[test]
fn many_searches_keep_a_bounded_preview_of_the_latest_queries() {
    let items: Vec<_> = (0..20)
        .map(|at| HistoryItem::Tool(search(&format!("rg -n query{at} src"), "src/a.rs:1:match")))
        .collect();
    let lines = tool_group_lines(&items, 100, &PathDisplay::VERBATIM);
    let rendered = text(&lines);
    assert!(lines.len() <= 7, "{} rows: {rendered}", lines.len());
    assert!(rendered.contains("query19"), "{rendered}");
    assert!(!rendered.contains("query0 "), "{rendered}");
}
