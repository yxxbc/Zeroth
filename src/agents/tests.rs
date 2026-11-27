use super::*;
use crate::llm::ChatMessage;
use crate::stream::{TokenUsage, ToolCallSummary};

fn chunk(text: &str) -> StreamEvent {
    StreamEvent::Chunk(text.to_string())
}

#[test]
fn agent_id_is_a_deterministic_a_plus_eight_base36() {
    let id = agent_id(42);
    assert_eq!(id, agent_id(42), "deterministic in the seed");
    assert_eq!(id.len(), 9);
    assert!(id.starts_with('a'));
    assert!(
        id.chars()
            .skip(1)
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    );
    assert_ne!(agent_id(1), agent_id(2), "different seeds differ");
}

#[test]
fn new_agent_seeds_its_transcript_with_the_prompt() {
    let run = AgentRun::new(
        "a1",
        "Fetch weather",
        GENERAL_PURPOSE,
        "Get Warsaw weather",
        true,
    );
    assert_eq!(run.status, AgentStatus::Pending);
    assert_eq!(run.activity(), "Initializing…");
    assert_eq!(run.history.len(), 1);
    let HistoryItem::Message(m) = &run.history[0] else {
        panic!("prompt seeds a user message");
    };
    assert_eq!(m.role, Role::User);
    assert_eq!(m.text, "Get Warsaw weather");
}

#[test]
fn apply_keeps_how_each_of_a_subagents_calls_ran() {
    // The main session's rule (docs/state/rollout-records.md): whichever event
    // resolves the call, its record keeps the facts it carried.
    use crate::stream::{ToolErrorKind, ToolRun};
    let ran = ToolRun {
        exit_code: Some(2),
        duration_ms: Some(31),
        error_kind: Some(ToolErrorKind::ToolError),
    };
    let resolutions = [
        StreamEvent::ToolEnd {
            call_id: None,
            output: "Exit code: 2".into(),
            ok: false,
            truncated: false,
            run: ran,
        },
        StreamEvent::ToolRejected {
            call_id: None,
            display: "User rejected write to a.py".into(),
            result: "denied".into(),
            truncated: false,
            run: ToolRun::refused(ToolErrorKind::Rejected),
        },
        StreamEvent::ToolAnswered {
            call_id: None,
            display: "Wrote 1 line to a.py".into(),
            result: "File created".into(),
            truncated: false,
            run: ToolRun {
                duration_ms: Some(4),
                ..ToolRun::default()
            },
        },
        StreamEvent::ToolBackgrounded {
            call_id: None,
            id: "bash_1".into(),
            output: "started".into(),
            run: ToolRun {
                duration_ms: Some(9),
                ..ToolRun::default()
            },
        },
    ];
    for resolution in resolutions {
        let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
        run.apply(&StreamEvent::ToolStart {
            call_id: None,
            name: "Bash".to_string(),
            args: "ls".to_string(),
            detail: None,
            arguments: None,
        });
        run.apply(&resolution);
        let HistoryItem::Tool(tool) = run.history.last().expect("the call recorded") else {
            panic!("a tool call lands in the transcript");
        };
        let (StreamEvent::ToolEnd { run: carried, .. }
        | StreamEvent::ToolRejected { run: carried, .. }
        | StreamEvent::ToolAnswered { run: carried, .. }
        | StreamEvent::ToolBackgrounded { run: carried, .. }) = resolution
        else {
            unreachable!()
        };
        assert_eq!(tool.run, carried, "{resolution:?}");
    }
}

#[test]
fn apply_keeps_the_classifier_note_on_the_resolved_call() {
    // A subagent's command allowed by the auto mode classifier
    // (docs/state/permissions.md): the ToolNote lands on the running front and
    // rides into the transcript, so the agent's own cells show the same
    // provenance row as the main turn's.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".to_string(),
        args: "ls -la".to_string(),
        detail: None,
        arguments: None,
    });
    run.apply(&StreamEvent::ToolNote {
        call_id: None,
        note: "Allowed by auto mode classifier".to_string(),
    });
    run.apply(&StreamEvent::ToolEnd {
        call_id: None,
        output: "Exit code: 0\ntotal 40".to_string(),
        ok: true,
        truncated: false,
        run: crate::stream::ToolRun::default(),
    });
    let HistoryItem::Tool(tool) = run.history.last().expect("the call recorded") else {
        panic!("a tool call lands in the transcript");
    };
    assert_eq!(
        tool.approval_note.as_deref(),
        Some("Allowed by auto mode classifier")
    );
}

#[test]
fn apply_keeps_every_note_a_subagents_call_collects() {
    // The main session's rule on the agent's own transcript: an approval
    // note and a hook's note share the row instead of the last winning.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".to_string(),
        args: "ls -la".to_string(),
        detail: None,
        arguments: None,
    });
    run.apply(&StreamEvent::ToolNote {
        call_id: None,
        note: "Allowed by auto mode classifier".to_string(),
    });
    run.apply(&StreamEvent::ToolNote {
        call_id: None,
        note: "Context added by hook".to_string(),
    });
    run.apply(&StreamEvent::ToolEnd {
        call_id: None,
        output: "Exit code: 0\ntotal 40".to_string(),
        ok: true,
        truncated: false,
        run: crate::stream::ToolRun::default(),
    });
    let HistoryItem::Tool(tool) = run.history.last().expect("the call recorded") else {
        panic!("a tool call lands in the transcript");
    };
    assert_eq!(
        tool.approval_note.as_deref(),
        Some("Allowed by auto mode classifier · Context added by hook")
    );
}

#[test]
fn apply_keeps_a_subagent_stop_hooks_cell_on_the_agents_transcript() {
    // docs/tools/hooks.md *The hook-run cell*: what an agent's SubagentStop
    // hooks said is its cell, so the fold keeps the report beside the
    // feedback — the session view draws it, inline and in Ctrl+O — after
    // the reply it interrupted.
    let report = crate::hooks::HookReport {
        event: "SubagentStop".to_string(),
        hooks: Vec::new(),
        warnings: vec!["hooks.json: a warning".to_string()],
    };
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("Found it."));
    run.apply(&StreamEvent::HookNote {
        label: "Stop hook".to_string(),
        text: "Stop hook feedback: cite your sources".to_string(),
        report: Some(report.clone()),
    });
    let notes: Vec<&crate::app::HookNote> = run
        .history
        .iter()
        .filter_map(|item| match item {
            HistoryItem::HookNote(note) => Some(note),
            _ => None,
        })
        .collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].report, Some(report));
    assert!(
        matches!(&run.history[run.history.len() - 2], HistoryItem::Message(m) if m.text == "Found it."),
        "the reply settles ahead of the cell: {:?}",
        run.history
    );
}

#[test]
fn a_subagents_cells_keep_the_wire_ids_its_events_carry() {
    // The agent view's permission prompt leads with the asked-about cell,
    // so the cell must know the id its request names — even though the
    // subagent's own queue is serial.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolBatch(vec![
        ToolCallSummary {
            call_id: Some("c1".into()),
            name: "Bash".into(),
            args: "one".into(),
        },
        ToolCallSummary {
            call_id: Some("c2".into()),
            name: "Bash".into(),
            args: "two".into(),
        },
    ]));
    assert_eq!(run.tool_queue[0].call_id.as_deref(), Some("c1"));
    assert_eq!(run.tool_queue[1].call_id.as_deref(), Some("c2"));
    run.apply(&StreamEvent::ToolStart {
        call_id: Some("c1".into()),
        name: "Bash".into(),
        args: "one".into(),
        detail: None,
        arguments: None,
    });
    assert_eq!(
        run.tool_queue[0].call_id.as_deref(),
        Some("c1"),
        "the running cell keeps the id"
    );
    run.apply(&StreamEvent::ToolEnd {
        call_id: Some("c1".into()),
        output: "ok".into(),
        ok: true,
        truncated: false,
        run: crate::stream::ToolRun::default(),
    });
    assert!(
        matches!(run.history.last(), Some(HistoryItem::Tool(tool)) if tool.call_id.as_deref() == Some("c1")),
        "the record keeps it too: {:?}",
        run.history
    );
}

#[test]
fn apply_folds_a_full_turn_into_the_transcript() {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    assert!(!run.apply(&chunk("Let me check. ")));
    assert_eq!(run.status, AgentStatus::Running);
    assert!(run.tokens > 0, "chunks tick the tally");
    // A tool round: batch announced, runs, resolves.
    assert!(!run.apply(&StreamEvent::ToolBatch(vec![ToolCallSummary {
        call_id: None,
        name: "Bash".into(),
        args: "curl wttr.in".into(),
    }])));
    assert!(!run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "curl wttr.in".into(),
        detail: Some("Fetching Warsaw weather".into()),
        arguments: None,
    }));
    assert_eq!(run.tool_uses, 1);
    assert_eq!(
        run.activity(),
        "Bash: Fetching Warsaw weather",
        "the model's own description leads the activity line"
    );
    assert!(!run.apply(&StreamEvent::ToolOutput {
        call_id: None,
        chunk: "+19°C\n".into(),
    }));
    assert!(!run.apply(&StreamEvent::ToolEnd {
        call_id: None,
        output: "+19°C".into(),
        ok: true,
        truncated: false,
        run: crate::stream::ToolRun::default(),
    }));
    // The pre-tool text flushed as its own message, the tool follows it.
    assert!(matches!(
        (&run.history[1], &run.history[2]),
        (HistoryItem::Message(m), HistoryItem::Tool(t))
            if m.role == Role::Assistant && t.status == ToolStatus::Ok
    ));
    assert!(!run.apply(&chunk("It is 19°C.")));
    assert!(run.apply(&StreamEvent::StreamDone), "done settles it");
    assert_eq!(run.status, AgentStatus::Done);
    assert_eq!(run.result.as_deref(), Some("It is 19°C."));
    assert_eq!(run.activity(), "Done");
    // Late events after settling are dropped.
    assert!(!run.apply(&chunk("late")));
    assert_eq!(run.result.as_deref(), Some("It is 19°C."));
}

#[test]
fn the_activity_row_shows_a_file_tools_args_the_way_the_caller_says() {
    // The record keeps the call as it came (`activity` — the roster's
    // own text); a renderer hands `activity_shown` how a file tool's
    // path should read (`docs/tools/tools.md` "Path display") and the row is
    // built from the same grammar. Every other row passes through.
    let shorten = |name: &str, args: &str| {
        if name == "Write" {
            args.replace("/home/u/", "~/")
        } else {
            args.to_string()
        }
    };
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    assert_eq!(run.activity_shown(shorten), "Initializing…");
    assert!(!run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Write".into(),
        args: "/home/u/x.py".into(),
        detail: None,
        arguments: None,
    }));
    assert_eq!(run.activity(), "Write: /home/u/x.py");
    assert_eq!(run.activity_shown(shorten), "Write: ~/x.py");
    assert!(!run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "cat /home/u/x.py".into(),
        detail: Some("Reading /home/u/x.py".into()),
        arguments: None,
    }));
    assert_eq!(run.activity_shown(shorten), "Bash: Reading /home/u/x.py");
}

#[test]
fn a_multiline_command_flattens_on_the_activity_row() {
    // The summary keeps a `bash` command's newlines for the cell header
    // (`docs/tools/tools.md`); the roster's one dim clipped row flattens them.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    assert!(!run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "cd foo\n  ls   -la".into(),
        detail: None,
        arguments: None,
    }));
    assert_eq!(run.activity(), "Bash: cd foo ls -la");
}

#[test]
fn the_sticky_activity_is_always_the_clean_name_colon_detail_row() {
    // Every activity row wears one grammar — `{Name}: {detail}`: the
    // model's own description when it gave one (`Bash: Fetching…`), else
    // the call's args summary (`Write: game.py` — never the header's
    // `Write(game.py)`, whose parens read as clutter on a dim one-liner;
    // `docs/agents/agent-tool.md`).
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    assert!(!run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Write".into(),
        args: "game.py".into(),
        detail: None,
        arguments: None,
    }));
    assert_eq!(run.activity(), "Write: game.py");
    assert!(!run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "curl wttr.in".into(),
        detail: Some("Fetching Warsaw weather".into()),
        arguments: None,
    }));
    assert_eq!(run.activity(), "Bash: Fetching Warsaw weather");
    // Nothing to say after the colon → the bare name, not `Name: `.
    assert!(!run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "TaskList".into(),
        args: String::new(),
        detail: None,
        arguments: None,
    }));
    assert_eq!(run.activity(), "TaskList");
}

#[test]
fn an_agent_talking_shows_its_newest_paragraph_on_the_activity_row() {
    // What the agent says is its activity too — `Agent: {update}`, the
    // newest paragraph on one line (docs/agents/agent-progress.md).
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("I'll start with the profile.\n\nLet me fetch"));
    assert_eq!(run.activity(), "Agent: Let me fetch");
    run.apply(&chunk(" the public\nrepositories next.\n\n"));
    assert_eq!(
        run.activity(),
        "Agent: Let me fetch the public repositories next.",
        "the paragraph's lines join on one row, its trailing blank lines skipped"
    );
    // The batch finalises the text: the row holds it — through a
    // permission prompt, say — until the call starts.
    run.apply(&StreamEvent::ToolBatch(vec![ToolCallSummary {
        call_id: None,
        name: "Bash".into(),
        args: "curl -s https://api.github.com/users/devuser/repos".into(),
    }]));
    assert_eq!(
        run.activity(),
        "Agent: Let me fetch the public repositories next."
    );
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "curl -s https://api.github.com/users/devuser/repos".into(),
        detail: Some("Fetch the repositories".into()),
        arguments: None,
    });
    assert_eq!(run.activity(), "Bash: Fetch the repositories");
    run.apply(&StreamEvent::ToolEnd {
        call_id: None,
        output: "[…]".into(),
        ok: true,
        truncated: false,
        run: crate::stream::ToolRun::default(),
    });
    assert_eq!(
        run.activity(),
        "Bash: Fetch the repositories",
        "the call holds between calls, as it always did"
    );
    // The next round talks: what it says is newer than the call.
    run.apply(&chunk("Found 42 repos."));
    assert_eq!(run.activity(), "Agent: Found 42 repos.");
    // Once it answers, the row says it settled.
    run.apply(&StreamEvent::StreamDone);
    assert_eq!(run.activity(), "Done");
}

#[test]
fn the_activity_row_reads_only_the_head_of_a_long_paragraph() {
    // The row is computed several times a frame while the text streams,
    // and is clipped to the terminal's width: a paragraph-long table or
    // code block must not be copied whole each time.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk(&"word ".repeat(10_000)));
    let activity = run.activity();
    assert!(activity.starts_with("Agent: word word"), "{activity}");
    assert!(
        activity.chars().count() <= "Agent: ".len() + ACTIVITY_UPDATE_MAX_CHARS,
        "kept to the bound: {} chars",
        activity.chars().count()
    );
}

#[test]
fn a_long_unbroken_run_is_read_only_up_to_the_bound() {
    // A URL or a blob with no space in it is one "word": the bound holds
    // inside it too, or the row copies the whole run every frame.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk(&"x".repeat(100_000)));
    let activity = run.activity();
    assert!(activity.starts_with("Agent: xxx"), "{activity}");
    assert!(
        activity.chars().count() <= "Agent: ".len() + ACTIVITY_UPDATE_MAX_CHARS,
        "{} chars",
        activity.chars().count()
    );
}

#[test]
fn a_long_run_of_whitespace_is_read_only_up_to_the_bound() {
    // Whitespace collapses to nothing on the row, but reading it is work
    // all the same: the row's head is read from no more than four times
    // the bound, so a word past a page of blank space is never reached.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk(&format!("Checking.{}far", " ".repeat(100_000))));
    assert_eq!(run.activity(), "Agent: Checking.");
}

#[test]
fn the_activity_row_stays_inside_the_frame_budget_on_a_line_of_blank_space() {
    // The row is built several times a frame while the agent writes, and
    // the line it is writing can end in a page of blank space: whether
    // that line has anything on it is known from its chunks as they
    // arrived, never found by walking the line again on every frame.
    // Driven the way `AgentRun::apply` drives it, minus the token count
    // that would make four megabytes slow to stream in.
    let mut text = String::from("Checking.");
    let mut paragraph = ParagraphStart::default();
    paragraph.extend(&text, 0);
    let spaces = " ".repeat(64 * 1024);
    for _ in 0..64 {
        let from = text.len();
        text.push_str(&spaces);
        paragraph.extend(&text, from);
    }
    let rows = 20;
    let start = std::time::Instant::now();
    for _ in 0..rows {
        assert_eq!(paragraph.head(&text).as_deref(), Some("Checking."));
    }
    let per_row = start.elapsed() / rows;
    println!("an activity row over 4 MB of blank space: {per_row:?}");
    assert!(
        per_row < std::time::Duration::from_millis(1),
        "one activity row took {per_row:?}: the line being written was \
             walked again, a cost paid several times a frame"
    );
}

/// The newest paragraph read the slow way, from the whole text each
/// time: its lines after the last blank one, on one line.
fn whole_text_paragraph(text: &str) -> Option<String> {
    let mut lines: Vec<&str> = text
        .lines()
        .rev()
        .skip_while(|line| line.trim().is_empty())
        .take_while(|line| !line.trim().is_empty())
        .collect();
    lines.reverse();
    let words: Vec<&str> = lines
        .iter()
        .flat_map(|line| line.split_whitespace())
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

#[test]
fn the_paragraph_found_chunk_by_chunk_is_the_one_the_whole_text_holds() {
    // Every text of up to six characters drawn from a letter, a space, a
    // CR and a line break, streamed in every way it can be cut: after
    // each chunk the tracker must name the paragraph a reading of the
    // whole text so far finds.
    const ALPHABET: [char; 4] = ['a', ' ', '\r', '\n'];
    for len in 0..=6u32 {
        for code in 0..ALPHABET.len().pow(len) {
            let text: String = (0..len)
                .map(|i| ALPHABET[code / ALPHABET.len().pow(i) % ALPHABET.len()])
                .collect();
            for cuts in 0..1usize << len.saturating_sub(1) {
                let mut tracker = ParagraphStart::default();
                let mut streamed = String::new();
                let mut from = 0;
                for (i, ch) in text.char_indices() {
                    streamed.push(ch);
                    if i + 1 < text.len() && cuts & (1 << i) == 0 {
                        continue;
                    }
                    tracker.extend(&streamed, from);
                    from = streamed.len();
                    assert_eq!(
                        tracker.head(&streamed),
                        whole_text_paragraph(&streamed),
                        "{streamed:?} streamed with cuts {cuts:b}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_paragraph_streamed_in_pieces_is_found_as_it_arrives() {
    // The newest paragraph is tracked chunk by chunk, so a blank line or
    // a word split across chunks — CRLF endings included — must land
    // exactly where a whole-text reading puts it.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    for piece in [
        "I'll start",
        " with the profile.\r\n",
        " ",
        "\r\n",
        "Let me fetch",
        " the repos.\r\n",
        "\r\n",
    ] {
        run.apply(&chunk(piece));
    }
    assert_eq!(run.activity(), "Agent: Let me fetch the repos.");
    run.apply(&chunk("Now the"));
    assert_eq!(run.activity(), "Agent: Now the");
    run.apply(&chunk(" events.\nThen the stars."));
    assert_eq!(
        run.activity(),
        "Agent: Now the events. Then the stars.",
        "a line break inside a paragraph is a space"
    );
    // Finalised, the same paragraph holds.
    run.apply(&StreamEvent::ToolBatch(Vec::new()));
    assert_eq!(run.activity(), "Agent: Now the events. Then the stars.");
    // The next run of text starts from nothing.
    run.apply(&chunk("Done."));
    assert_eq!(run.activity(), "Agent: Done.");
}

#[test]
fn a_blank_run_of_text_leaves_the_activity_row_alone() {
    // A model's `\n\n` before a call says nothing: the row keeps what it
    // showed — the call, or the last thing the agent did say.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "ls".into(),
        detail: None,
        arguments: None,
    });
    run.apply(&chunk("\n \n"));
    assert_eq!(run.activity(), "Bash: ls");
    run.apply(&StreamEvent::ToolEnd {
        call_id: None,
        output: "a.rs".into(),
        ok: true,
        truncated: false,
        run: crate::stream::ToolRun::default(),
    });
    run.apply(&chunk("Only a.rs is here."));
    run.apply(&StreamEvent::Steered {
        text: "check b.rs too".into(),
    });
    assert_eq!(
        run.activity(),
        "Agent: Only a.rs is here.",
        "the text before a message it read is still the newest thing it said"
    );
    run.apply(&chunk("\n\n"));
    run.apply(&StreamEvent::ToolBatch(vec![ToolCallSummary {
        call_id: None,
        name: "Read".into(),
        args: "b.rs".into(),
    }]));
    assert_eq!(run.activity(), "Agent: Only a.rs is here.");
}

#[test]
fn the_text_before_a_thinking_phase_stays_on_the_activity_row() {
    // The thinking block flushes the text before it (invariant 4); the
    // agent's words are still the newest thing it said.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("Let me think about the layout."));
    run.begin_reasoning();
    run.apply(&StreamEvent::ThinkingChunk("Rows first.".into()));
    assert_eq!(run.activity(), "Agent: Let me think about the layout.");
}

#[test]
fn a_rejected_call_keeps_the_model_facing_result_on_the_agents_transcript() {
    // A subagent's request goes to the same shared prompt, so its
    // transcript needs the same two texts: the cell line, and what the
    // agent itself read — its Ctrl+D view and any continuation run derive
    // from this history (docs/state/permissions.md).
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Write".into(),
        args: "hello.py".into(),
        detail: None,
        arguments: None,
    });
    assert!(!run.apply(&StreamEvent::ToolRejected {
        call_id: None,
        display: "User rejected write to hello.py\nInstructions: use pathlib".into(),
        result: "The user doesn't want to proceed… instructions instead: use pathlib".into(),
        truncated: false,
        run: crate::stream::ToolRun::default(),
    }));
    let Some(HistoryItem::Tool(tool)) = run.history.last() else {
        panic!("the refused call still lands on the transcript");
    };
    assert_eq!(tool.status, ToolStatus::Failed);
    assert_eq!(
        tool.output,
        "User rejected write to hello.py\nInstructions: use pathlib"
    );
    assert_eq!(
        tool.context_text(),
        "The user doesn't want to proceed… instructions instead: use pathlib"
    );
    assert!(run.tokens > 0, "the uploaded instruction ticks the tally");
}

#[test]
fn stream_done_records_the_turns_summary_on_the_transcript() {
    // The agent session view ends its turns the way the main one does: a
    // dim `Done for 59s · 6.1k tokens (2.8k cached)` summary lands on the
    // agent's own transcript at StreamDone, carrying the turn's billed
    // usage (`docs/agents/agent-tool.md`).
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.runtime = Duration::from_secs(59);
    run.apply(&chunk("It is 19°C."));
    run.apply(&StreamEvent::Usage(TokenUsage {
        input: 6000,
        output: 100,
        cached: 2800,
        cache_write: 700,
        ..TokenUsage::default()
    }));
    assert!(run.apply(&StreamEvent::StreamDone));
    let Some(HistoryItem::Summary(summary)) = run.history.last() else {
        panic!("the settle records the summary: {:?}", run.history.last());
    };
    assert_eq!(
        summary.verb, "Worked",
        "the past tense of the verb the view wore (nothing rotated it)"
    );
    assert_eq!(summary.secs, 59);
    assert_eq!(summary.tokens, 6100);
    assert_eq!(summary.cached, 2800);
    assert_eq!(summary.cache_write, 700);
    assert_eq!(summary.shells, 0, "shells are the main session's business");
    // The summary sits AFTER the flushed final reply.
    assert!(matches!(
        &run.history[run.history.len() - 2],
        HistoryItem::Message(m) if m.text == "It is 19°C."
    ));
}

#[test]
fn a_continuation_turns_summary_counts_only_its_own_usage() {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("first"));
    run.apply(&StreamEvent::Usage(TokenUsage {
        input: 900,
        output: 100,
        cached: 400,
        ..TokenUsage::default()
    }));
    run.apply(&StreamEvent::StreamDone);
    run.push_user_message("and Manila?");
    run.reopen();
    run.apply(&chunk("second"));
    run.apply(&StreamEvent::Usage(TokenUsage {
        input: 450,
        output: 50,
        cached: 0,
        ..TokenUsage::default()
    }));
    run.apply(&StreamEvent::StreamDone);
    let summaries: Vec<_> = run
        .history
        .iter()
        .filter_map(|item| match item {
            HistoryItem::Summary(s) => Some((s.tokens, s.cached)),
            _ => None,
        })
        .collect();
    assert_eq!(
        summaries,
        [(1000, 400), (500, 0)],
        "each turn's summary is its own receipt"
    );
    assert_eq!(run.tokens, 1500, "the roster tally stays cumulative");
}

#[test]
fn a_failed_or_interrupted_turn_records_no_summary() {
    // Main parity: an error commits its red notice, an interrupt its
    // record — neither earns a `Done for Ns` receipt.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("partial"));
    run.apply(&StreamEvent::Error("boom".into()));
    assert!(
        !run.history
            .iter()
            .any(|item| matches!(item, HistoryItem::Summary(_))),
        "no summary on a failure"
    );
    let mut run = AgentRun::new("a2", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("partial"));
    run.interrupt();
    assert!(
        !run.history
            .iter()
            .any(|item| matches!(item, HistoryItem::Summary(_))),
        "no summary on an interrupt"
    );
}

#[test]
fn usage_frames_snap_the_estimated_tally() {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("some streamed text here"));
    let est = run.tokens;
    assert!(est > 0);
    run.apply(&StreamEvent::Usage(TokenUsage {
        input: 900,
        output: 100,
        cached: 0,
        cache_write: 0,
        ..TokenUsage::default()
    }));
    assert_eq!(run.tokens, 1000, "snapped to the billed total");
    run.apply(&StreamEvent::Usage(TokenUsage {
        input: 400,
        output: 100,
        cached: 0,
        cache_write: 0,
        ..TokenUsage::default()
    }));
    assert_eq!(run.tokens, 1500, "frames accumulate");
}

#[test]
fn usage_frames_seat_the_runs_context_size_and_a_later_frame_replaces_it() {
    // The agent's own context gauge (docs/agents/agent-context-gauge.md): the
    // round's `input` is the whole re-sent context — the context size,
    // the main session's `apply_usage` rule. Unlike the billed tally it
    // does NOT accumulate: the next frame's `input` already carries
    // everything the previous one did.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    assert_eq!(
        run.context_used(),
        0,
        "nothing seeds it until a frame lands"
    );
    run.apply(&StreamEvent::Usage(TokenUsage {
        input: 5_000,
        output: 200,
        ..TokenUsage::default()
    }));
    assert_eq!(run.context_used(), 5_000);
    run.apply(&StreamEvent::Usage(TokenUsage {
        input: 6_000,
        output: 300,
        ..TokenUsage::default()
    }));
    assert_eq!(run.context_used(), 6_000, "replaced, not summed");
    assert_eq!(
        run.tokens, 11_500,
        "while the billed tally keeps accumulating"
    );
    // A turn that saw a frame settles on the provider's count, plus the
    // estimate of what joined the context after it — the reply.
    run.apply(&chunk("done"));
    run.apply(&StreamEvent::StreamDone);
    assert_eq!(
        run.context_used(),
        6_000 + crate::app::count_tokens("done") as u64,
        "the provider's count stands under the reply"
    );
}

#[test]
fn a_settle_without_a_usage_frame_estimates_the_context_from_the_transcript() {
    // The offline dummy scripts no usage frames on the agent channel, so
    // — the main session's `take_turn_summary` rule — the token
    // estimate over the agent's own derived context stands in at the
    // settle, and the gauge still moves offline.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "Get Warsaw weather", false);
    run.apply(&chunk("It is 12°C and cloudy in Warsaw."));
    assert_eq!(run.context_used(), 0, "mid-turn nothing has re-counted yet");
    run.apply(&StreamEvent::StreamDone);
    let estimate = run.context_used();
    assert!(estimate > 0, "the prompt and the reply are context");
    assert_eq!(
        estimate,
        crate::app::estimate_messages_tokens(&crate::context::context_messages(&run.history))
            as u64,
        "counted by the one rule the main gauge counts with"
    );
}

#[test]
fn error_settles_the_agent_failed_and_resolves_its_tool() {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "sleep 99".into(),
        detail: None,
        arguments: None,
    });
    assert!(run.apply(&StreamEvent::Error("boom".into())));
    assert_eq!(run.status, AgentStatus::Failed);
    assert_eq!(run.error.as_deref(), Some("boom"));
    assert!(matches!(
        run.history.last(),
        Some(HistoryItem::Tool(t)) if t.status == ToolStatus::Failed
    ));
    assert!(run.tool_queue.is_empty());
}

// --- a subagent's `write` streaming into its cell (docs/ui/write-streaming.md) ---

/// One streamed piece of a call under slot `index`.
fn piece(index: usize, name: &str, arguments: &str) -> StreamEvent {
    StreamEvent::ToolCallDelta(crate::stream::ToolCallFragment {
        index,
        name: name.to_string(),
        arguments: arguments.to_string(),
    })
}

#[test]
fn a_subagents_write_previews_as_it_streams_after_its_text() {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::Chunk("Writing it now:".to_string()));
    // That the opening piece is a segment boundary, and that a bash
    // call's is not, is `every_flush_point_is_declared`'s to pin.
    run.apply(&piece(0, "write", r#"{"path":"a.py","content":"x = 1\ny"#));
    let draft = run.drafts().previews().next().expect("a preview");
    assert_eq!(draft.path(), Some("a.py"));
    assert_eq!(draft.content(), "x = 1\ny");
    assert!(
        matches!(
            run.history.last(),
            Some(HistoryItem::Message(m)) if m.role == Role::Assistant && m.text == "Writing it now:"
        ),
        "the text before the cell became its own message"
    );
    assert!(
        !run.will_flush_segment(&piece(0, "", r#" = 2"#)),
        "a later piece of the open preview flushes nothing"
    );
}

#[test]
fn a_subagents_batch_adopts_its_previews_and_each_resolution_drains_its_own() {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&piece(0, "write", r#"{"path":"a.py","content":"A\n"}"#));
    run.apply(&StreamEvent::ToolBatch(vec![ToolCallSummary {
        call_id: None,
        name: "Write".to_string(),
        args: "a.py".to_string(),
    }]));
    assert_eq!(run.drafts().previews().count(), 0);
    assert_eq!(run.drafts().queued().count(), 1, "the queued write owns it");
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Write".to_string(),
        args: "a.py".to_string(),
        detail: None,
        arguments: None,
    });
    run.apply(&StreamEvent::ToolAnswered {
        call_id: None,
        display: "Wrote 1 line to a.py\n1 A".to_string(),
        result: "File created successfully at: a.py".to_string(),
        truncated: false,
        run: crate::stream::ToolRun::default(),
    });
    assert!(run.drafts().is_empty(), "the resolved call took its draft");
}

#[test]
fn a_subagents_lone_call_pushed_ahead_of_a_live_write_borrows_no_draft() {
    // The agent fold pushes a lone call to the queue's FRONT, so pairing
    // by order would hand the earlier write's file to the newcomer.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    let start = |path: &str| StreamEvent::ToolStart {
        call_id: None,
        name: "Write".to_string(),
        args: path.to_string(),
        detail: None,
        arguments: None,
    };
    run.apply(&piece(0, "write", r#"{"path":"a.py","content":"A"}"#));
    run.apply(&start("a.py"));
    run.apply(&piece(0, "write", r#"{"path":"b.py","content":"B"}"#));
    run.apply(&start("b.py"));
    let front = run.tool_queue.front().map(|call| call.args.clone());
    let paired = run.drafts().queued().next().and_then(|draft| draft.path());
    assert!(
        paired.is_none() || paired.map(str::to_string) == front,
        "the front call shows its own file or none: front {front:?}, draft {paired:?}"
    );
}

#[test]
fn a_subagents_turn_edges_drop_its_drafts() {
    let streaming = || {
        let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
        run.apply(&piece(0, "write", r#"{"path":"a.py","content":"op"#));
        assert!(!run.drafts().is_empty());
        run
    };
    let mut run = streaming();
    run.apply(&StreamEvent::StreamDone);
    assert!(run.drafts().is_empty(), "a finished run");
    let mut run = streaming();
    run.apply(&StreamEvent::Error("boom".to_string()));
    assert!(run.drafts().is_empty(), "a failed run");
    let mut run = streaming();
    run.interrupt();
    assert!(run.drafts().is_empty(), "a stopped run");
    let mut run = streaming();
    run.apply(&StreamEvent::RoundCalls(Vec::new()));
    assert!(
        run.drafts().previews().all(crate::app::ToolDraft::finished),
        "the round's announcement seals what streamed"
    );
}

#[test]
fn interrupt_settles_locally_and_keeps_the_partial() {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("partial "));
    assert!(run.interrupt());
    assert_eq!(run.status, AgentStatus::Interrupted);
    assert_eq!(run.activity(), "Interrupted");
    assert!(matches!(
        run.history.last(),
        Some(HistoryItem::Message(m)) if m.text == "partial "
    ));
    assert!(!run.interrupt(), "idempotent");
}

#[test]
fn chat_reopens_a_settled_agent() {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("hi"));
    run.apply(&StreamEvent::StreamDone);
    assert_eq!(run.status, AgentStatus::Done);
    run.push_user_message("and in Manila?");
    run.reopen();
    assert_eq!(run.status, AgentStatus::Running);
    assert!(run.result.is_none());
    assert!(!run.apply(&chunk("28°C")));
    assert!(run.apply(&StreamEvent::StreamDone));
    assert_eq!(run.result.as_deref(), Some("28°C"));
    // The transcript kept the whole exchange in order — each turn closed
    // by its own `Done for Ns` summary (`docs/agents/agent-tool.md`).
    let roles: Vec<&str> = run
        .history
        .iter()
        .map(|item| match item {
            HistoryItem::Message(m) if m.role == Role::User => "user",
            HistoryItem::Message(_) => "assistant",
            HistoryItem::Summary(_) => "summary",
            _ => "other",
        })
        .collect();
    assert_eq!(
        roles,
        [
            "user",
            "assistant",
            "summary",
            "user",
            "assistant",
            "summary"
        ]
    );
}

fn test_registry() -> AgentRegistry {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    AgentRegistry::new(tx)
}

/// `id`'s loop drains its queue and the forwarder relays each message's
/// announcement straight after — the two in step, as on a quiet machine.
fn read_queued(registry: &AgentRegistry, id: &str) -> Vec<String> {
    let read = registry.take_pending_inputs(id);
    for _ in &read {
        registry.announce_read(id);
    }
    read
}

#[test]
fn register_allocates_unique_ids_and_finish_stores_the_outcome() {
    let registry = test_registry();
    let (id1, _c1) = registry.register(GENERAL_PURPOSE);
    let (id2, _c2) = registry.register(GENERAL_PURPOSE);
    assert_ne!(id1, id2);
    assert!(!registry.is_done(&id1), "busy while running");
    registry.finish(&id1, Ok("the answer".into()), vec![ChatMessage::user("p")]);
    assert!(registry.is_done(&id1));
    assert_eq!(registry.outcome(&id1), Some(Ok("the answer".into())));
    assert_eq!(registry.outcome(&id2), None, "still running");
}

#[test]
fn each_agent_gets_its_own_classifier_context() {
    // A subagent's auto-mode verdicts are reviewed against ITS run — the
    // launch prompt and the calls it has made — not the lead's
    // (`docs/state/permissions.md`). The registry is where that context has to
    // live, because the Ctrl+D classifier page reads it from outside the
    // agent's thread while the agent's session view is open.
    let registry = test_registry();
    let (id1, _c1) = registry.register(GENERAL_PURPOSE);
    let (id2, _c2) = registry.register(GENERAL_PURPOSE);
    let handle = registry.classifier_handle(&id1).expect("registered");
    {
        let mut log = handle.lock().expect("poisoned");
        log.push_request("count the rust files");
        log.record_call("bash", r#"{"command":"ls -1 src"}"#);
    }
    let first = registry.classifier_context(&id1).expect("registered");
    assert!(
        first.contains("count the rust files") && first.contains("ls -1 src"),
        "the agent's own request and action are in its context: {first:?}"
    );
    let second = registry.classifier_context(&id2).expect("registered");
    assert!(
        !second.contains("ls -1 src"),
        "a sibling agent's actions never leak into it: {second:?}"
    );
}

#[test]
fn an_unknown_agent_has_no_classifier_context() {
    // The page falls back to its empty placeholder rather than to the
    // lead's context — showing the wrong agent's review log is the bug
    // this exists to fix.
    let registry = test_registry();
    assert!(registry.classifier_context("nope").is_none());
    assert!(registry.classifier_handle("nope").is_none());
}

#[test]
fn removing_an_agent_drops_its_classifier_context() {
    let registry = test_registry();
    let (id, _cancel) = registry.register(GENERAL_PURPOSE);
    assert!(registry.classifier_context(&id).is_some());
    registry.remove(&id);
    assert!(registry.classifier_context(&id).is_none());
}

#[test]
fn kill_settles_at_once_and_wins_over_a_late_finish() {
    let registry = test_registry();
    let (id, cancel) = registry.register(GENERAL_PURPOSE);
    assert!(registry.kill(&id).was_live);
    assert!(cancel.is_cancelled(), "the token cancels the thread");
    assert!(registry.is_done(&id), "resolved without the thread");
    assert!(registry.is_killed(&id));
    assert_eq!(
        registry.outcome(&id),
        Some(Err("stopped by the user".into()))
    );
    // The thread notices later and reports — the kill outcome stands.
    registry.finish(&id, Ok("late result".into()), Vec::new());
    assert_eq!(
        registry.outcome(&id),
        Some(Err("stopped by the user".into()))
    );
    assert!(!registry.kill(&id).was_live, "second kill reports not-live");
}

#[test]
fn chat_inputs_queue_while_busy_and_continuations_resume_the_stored_list() {
    let registry = test_registry();
    let (id, _cancel) = registry.register(GENERAL_PURPOSE);
    assert!(registry.queue_input(&id, "also check Manila"));
    assert_eq!(registry.take_pending_inputs(&id), vec!["also check Manila"]);
    assert!(registry.take_pending_inputs(&id).is_empty(), "drained");
    assert!(
        registry.begin_continuation(&id).is_none(),
        "no continuation while busy"
    );
    registry.finish(&id, Ok("done".into()), vec![ChatMessage::user("p")]);
    assert!(
        !registry.queue_input(&id, "x"),
        "not busy — caller must spawn a continuation"
    );
    let (messages, cancel) = registry.begin_continuation(&id).expect("stored list");
    assert_eq!(messages.len(), 1);
    assert!(!cancel.is_cancelled());
    assert!(!registry.is_done(&id), "busy again");
    registry.finish(&id, Ok("follow-up".into()), messages);
    assert_eq!(registry.outcome(&id), Some(Ok("follow-up".into())));
}

#[test]
fn a_message_queued_into_a_running_agent_waits_on_its_own_transcript() {
    // The main session's queue, one level down (docs/input/queue.md): the
    // message shows above the box in that agent's session view and is
    // NOT on its transcript yet — the agent has not read it.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::Chunk("working".to_string()));
    run.queue_chat("also check Manila");
    assert_eq!(run.queued, ["also check Manila"]);
    assert!(
        !run.history.iter().any(|item| matches!(
            item,
            HistoryItem::Message(m) if m.role == Role::User && m.text == "also check Manila"
        )),
        "nothing is claimed on the transcript before the agent has it"
    );
}

#[test]
fn its_round_boundary_turns_the_queued_message_into_a_user_bubble() {
    // `StreamEvent::Steered` on the agent channel — the same event the
    // main session's queue resolves through.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::Chunk("reading…".to_string()));
    run.queue_chat("also check Manila");
    run.apply(&StreamEvent::Steered {
        text: "also check Manila".to_string(),
    });
    assert!(run.queued.is_empty(), "it stops waiting above the box");
    assert!(
        matches!(
            run.history.last(),
            Some(HistoryItem::Message(m))
                if m.role == Role::User && m.text == "also check Manila"
        ),
        "and lands on the agent's own transcript"
    );
    assert!(
        matches!(run.history.first(), Some(HistoryItem::Message(m)) if m.role == Role::User),
        "…after the launch prompt"
    );
}

#[test]
fn a_taken_message_finalises_the_agents_streamed_reply_before_it() {
    // Invariant 4 on the agent's own transcript: the run of text ahead of
    // the interleaved message is finalised first.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::Chunk("Checking Cebu…".to_string()));
    run.apply(&StreamEvent::Steered {
        text: "also check Manila".to_string(),
    });
    let roles: Vec<Role> = run
        .history
        .iter()
        .filter_map(|item| match item {
            HistoryItem::Message(m) => Some(m.role),
            _ => None,
        })
        .collect();
    assert_eq!(
        roles,
        vec![Role::User, Role::Assistant, Role::User],
        "prompt, the finalised segment, then the message the agent just read"
    );
}

#[test]
fn a_message_delivered_to_a_settled_agent_reopens_it() {
    // The settle window's other half (`docs/input/queue.md`): a message queued
    // between the loop's last drain and `finish` is delivered by a
    // continuation the run's own thread starts — but by then the roster
    // has folded the terminal event, and `apply` drops everything once a
    // status is final. Without the reopen that continuation runs
    // invisibly: the model works, the transcript shows nothing, and the
    // pending row never clears.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("done"));
    run.apply(&StreamEvent::StreamDone);
    assert!(run.status.is_final(), "settled");
    run.queue_chat("one more thing");
    run.apply(&StreamEvent::Steered {
        text: "one more thing".to_string(),
    });
    assert_eq!(run.status, AgentStatus::Running, "the run is live again");
    assert!(run.queued.is_empty(), "the pending row is spent");
    assert!(
        matches!(
            run.history.last(),
            Some(HistoryItem::Message(m))
                if m.role == Role::User && m.text == "one more thing"
        ),
        "…and the message is on its transcript"
    );
}

#[test]
fn a_stopped_agent_is_never_reopened_by_a_late_delivery() {
    // The `x` said stop. Nothing should resurrect it.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("working"));
    run.stopped_by_user = true;
    assert!(run.interrupt());
    run.apply(&StreamEvent::Steered {
        text: "too late".to_string(),
    });
    assert_eq!(run.status, AgentStatus::Interrupted);
    assert!(
        !run.history.iter().any(|item| matches!(
            item,
            HistoryItem::Message(m) if m.text == "too late"
        )),
        "a stopped agent's transcript is closed"
    );
}

#[test]
fn stopping_an_agent_drops_the_messages_its_loop_will_never_read() {
    // The `x` cancels the loop, so nothing will ever take what is still
    // queued for it. Leaving the rows up would promise a delivery that
    // cannot happen — and the roster defers its sweep while the user is
    // inside that agent's view, so the promise would stand indefinitely.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("working"));
    run.queue_chat("one more thing");
    assert!(run.interrupt());
    assert!(run.queued.is_empty(), "a cancelled loop reads nothing more");
}

#[test]
fn the_registry_reports_an_agent_holding_an_undelivered_message() {
    // The settle window (`docs/input/queue.md`): a slot stays `busy` between
    // its loop's last drain and `finish`, so a message typed in there is
    // accepted by `queue_input` and then read by nobody — the loop is
    // over, and the roster's terminal event may already have been folded.
    // The run's own thread checks this before exiting and continues.
    let registry = test_registry();
    let (id, _cancel) = registry.register(GENERAL_PURPOSE);
    assert!(!registry.has_pending_inputs(&id), "nothing queued yet");
    assert!(registry.queue_input(&id, "one more thing"));
    assert!(
        registry.has_pending_inputs(&id),
        "the run must not settle leaving this undelivered"
    );
    assert_eq!(registry.take_pending_inputs(&id), ["one more thing"]);
    assert!(!registry.has_pending_inputs(&id), "drained");
}

#[test]
fn take_last_input_reclaims_only_an_undelivered_message() {
    // Alt+Up inside the agent's session view: the registry is the only
    // side that knows whether the round boundary has read it yet.
    let registry = test_registry();
    let (id, _cancel) = registry.register(GENERAL_PURPOSE);
    assert!(registry.queue_user_input(&id, "first"));
    assert!(registry.queue_user_input(&id, "second"));
    assert_eq!(registry.take_last_input(&id).as_deref(), Some("second"));
    assert_eq!(registry.take_pending_inputs(&id), ["first"]);
    assert_eq!(
        registry.take_last_input(&id),
        None,
        "a message the loop has read cannot come back"
    );
    assert_eq!(registry.take_last_input("nope"), None);
}

#[test]
fn ready_for_turn_is_true_only_for_a_settled_unstopped_agent() {
    // The Tab follow-up queue asks this before dispatching, so a
    // follow-up becomes its own continuation turn rather than a steer
    // into a run still going (docs/input/queue.md).
    let registry = test_registry();
    let (id, _cancel) = registry.register(GENERAL_PURPOSE);
    assert!(!registry.ready_for_turn(&id), "still running");
    registry.finish(&id, Ok("done".into()), vec![ChatMessage::user("hi")]);
    assert!(registry.ready_for_turn(&id), "settled with a conversation");
    assert!(
        !registry.ready_for_turn("nope"),
        "an unknown id takes nothing"
    );
    let (killed, _c2) = registry.register(GENERAL_PURPOSE);
    registry.finish(&killed, Ok("done".into()), vec![ChatMessage::user("hi")]);
    let _ = registry.kill(&killed);
    assert!(
        !registry.ready_for_turn(&killed),
        "the `x` said stop — nothing continues it"
    );
}

#[test]
fn a_stopped_agent_drops_its_follow_up_turns_too() {
    let mut run = AgentRun::new("a1", "Do the thing", GENERAL_PURPOSE, "prompt", false);
    run.queue_chat("read next");
    run.queue_followup("and then this");
    assert!(run.interrupt());
    assert!(run.queued.is_empty() && run.followups.is_empty());
}

#[test]
fn follow_ups_drain_oldest_first_and_alt_up_takes_the_newest() {
    let mut run = AgentRun::new("a1", "Do the thing", GENERAL_PURPOSE, "prompt", false);
    run.queue_followup("first");
    run.queue_followup("second");
    assert_eq!(run.take_last_followup().as_deref(), Some("second"));
    assert_eq!(run.take_followup().as_deref(), Some("first"));
    assert_eq!(run.take_followup(), None);
}

#[test]
fn kill_all_sweeps_every_slot_and_remove_drops_one() {
    let registry = test_registry();
    let (id1, c1) = registry.register(GENERAL_PURPOSE);
    let (id2, c2) = registry.register(GENERAL_PURPOSE);
    registry.kill_all();
    assert!(c1.is_cancelled() && c2.is_cancelled());
    assert!(registry.is_done(&id1) && registry.is_done(&id2));
    registry.remove(&id1);
    assert!(registry.is_done(&id1), "unknown ids read as done");
    assert!(registry.is_killed(&id2));
}

// ===== The agent's thinking stream (docs/agents/agent-view-streaming.md) =====

#[test]
fn every_resolution_leaves_the_resolved_cell_last_on_the_transcript() {
    // The session view commits what the fold **recorded**, not what event
    // arrived (`docs/agents/agent-view-streaming.md`), so every way a call can
    // resolve must leave its cell as the transcript's last item — else
    // the view drops it and only a resize brings it back. That is the
    // reported bug: `write`/`edit` moved onto `ToolAnswered` and the old
    // event-keyed arm never listed it.
    let start = StreamEvent::ToolStart {
        call_id: None,
        name: "Write".to_string(),
        args: "f.py".to_string(),
        detail: None,
        arguments: None,
    };
    let resolutions: [(&str, StreamEvent); 4] = [
        (
            "ToolEnd",
            StreamEvent::ToolEnd {
                call_id: None,
                output: "out".to_string(),
                ok: true,
                truncated: false,
                run: crate::stream::ToolRun::default(),
            },
        ),
        (
            "ToolAnswered",
            StreamEvent::ToolAnswered {
                call_id: None,
                display: "Wrote 1 lines to f.py".to_string(),
                result: "File created successfully at: f.py".to_string(),
                truncated: false,
                run: crate::stream::ToolRun::default(),
            },
        ),
        (
            "ToolRejected",
            StreamEvent::ToolRejected {
                call_id: None,
                display: "User rejected write to f.py".to_string(),
                result: "the user refused".to_string(),
                truncated: false,
                run: crate::stream::ToolRun::default(),
            },
        ),
        (
            "ToolBackgrounded",
            StreamEvent::ToolBackgrounded {
                call_id: None,
                id: "b1".to_string(),
                output: "Running in the background".to_string(),
                run: crate::stream::ToolRun::default(),
            },
        ),
    ];
    for (name, event) in resolutions {
        let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
        run.apply(&start);
        let before = run.history.len();
        run.apply(&event);
        assert_eq!(run.history.len(), before + 1, "{name} recorded no cell");
        assert!(
            matches!(run.history.last(), Some(HistoryItem::Tool(_))),
            "{name} did not leave its cell last: {:?}",
            run.history
        );
        assert!(run.tool_queue.is_empty(), "{name} left the call live");
    }
}

/// Every `StreamEvent` a subagent's channel can carry, one of each — the
/// walk `every_flush_point_is_declared` uses so a new variant cannot slip
/// past the predicate.
fn one_of_every_event() -> Vec<(&'static str, StreamEvent)> {
    vec![
        ("Chunk", StreamEvent::Chunk("hi".to_string())),
        (
            "HookNote",
            StreamEvent::HookNote {
                label: "SubagentStop".to_string(),
                text: "keep going".to_string(),
                report: None,
            },
        ),
        (
            "Steered",
            StreamEvent::Steered {
                text: "one more thing".to_string(),
            },
        ),
        ("ThinkingChunk", StreamEvent::ThinkingChunk("t".to_string())),
        (
            "ToolCallDelta",
            StreamEvent::ToolCallDelta(crate::stream::ToolCallFragment {
                arguments: "{".to_string(),
                ..Default::default()
            }),
        ),
        (
            "ToolCallDelta naming a bash call",
            StreamEvent::ToolCallDelta(crate::stream::ToolCallFragment {
                index: 0,
                name: "bash".to_string(),
                arguments: "{".to_string(),
            }),
        ),
        // The piece that opens a previewed `write` is a segment boundary
        // of its own: the live cell goes up under the text before it
        // (docs/ui/write-streaming.md).
        (
            "ToolCallDelta opening a write",
            StreamEvent::ToolCallDelta(crate::stream::ToolCallFragment {
                index: 0,
                name: "write".to_string(),
                arguments: String::new(),
            }),
        ),
        (
            "ToolBatch",
            StreamEvent::ToolBatch(vec![ToolCallSummary {
                call_id: None,
                name: "Bash".to_string(),
                args: "ls".to_string(),
            }]),
        ),
        (
            "ToolStart",
            StreamEvent::ToolStart {
                call_id: None,
                name: "Bash".to_string(),
                args: "ls".to_string(),
                detail: None,
                arguments: None,
            },
        ),
        (
            "ToolNote",
            StreamEvent::ToolNote {
                call_id: None,
                note: "noted".to_string(),
            },
        ),
        (
            "ToolOutput",
            StreamEvent::ToolOutput {
                call_id: None,
                chunk: "out".to_string(),
            },
        ),
        (
            "ToolScreen",
            StreamEvent::ToolScreen {
                call_id: None,
                settled: "done\n".to_string(),
                live: "45%".to_string(),
            },
        ),
        (
            "ToolTitle",
            StreamEvent::ToolTitle {
                call_id: None,
                title: "sudo x".to_string(),
            },
        ),
        (
            "ToolEnd",
            StreamEvent::ToolEnd {
                call_id: None,
                output: "out".to_string(),
                ok: true,
                truncated: false,
                run: crate::stream::ToolRun::default(),
            },
        ),
        (
            "ToolRejected",
            StreamEvent::ToolRejected {
                call_id: None,
                display: "no".to_string(),
                result: "refused".to_string(),
                truncated: false,
                run: crate::stream::ToolRun::default(),
            },
        ),
        (
            "ToolAnswered",
            StreamEvent::ToolAnswered {
                call_id: None,
                display: "done".to_string(),
                result: "ack".to_string(),
                truncated: false,
                run: crate::stream::ToolRun::default(),
            },
        ),
        (
            "ToolBackgrounded",
            StreamEvent::ToolBackgrounded {
                call_id: None,
                id: "b1".to_string(),
                output: "backgrounded".to_string(),
                run: crate::stream::ToolRun::default(),
            },
        ),
        ("StreamDone", StreamEvent::StreamDone),
        ("Error", StreamEvent::Error("boom".to_string())),
    ]
}

#[test]
fn a_subagents_terminal_screen_redraws_in_place() {
    // The session view folds a subagent's terminal output exactly as the
    // main session does (docs/tools/interactive-shell.md): settled text
    // appends, live rows replace the last ones, a refined header lands.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "sudo pacman -Syy".into(),
        detail: None,
        arguments: None,
    });
    run.apply(&StreamEvent::ToolScreen {
        call_id: None,
        settled: String::new(),
        live: " extra  10%".into(),
    });
    run.apply(&StreamEvent::ToolScreen {
        call_id: None,
        settled: ":: Synchronizing\n".into(),
        live: " extra  50%".into(),
    });
    assert_eq!(
        run.tool_queue.front().expect("running").output,
        ":: Synchronizing\n extra  50%"
    );
    run.apply(&StreamEvent::ToolTitle {
        call_id: None,
        title: "sudo pacman -Syy ← y⏎".into(),
    });
    assert_eq!(
        run.tool_queue.front().expect("running").args,
        "sudo pacman -Syy ← y⏎"
    );
    // The next call starts with no live tail of its own.
    run.apply(&StreamEvent::ToolEnd {
        call_id: None,
        output: "Exit code: 0".into(),
        ok: true,
        truncated: false,
        run: crate::stream::ToolRun::default(),
    });
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "ls".into(),
        detail: None,
        arguments: None,
    });
    run.apply(&StreamEvent::ToolScreen {
        call_id: None,
        settled: String::new(),
        live: "x".into(),
    });
    assert_eq!(run.tool_queue.front().expect("running").output, "x");
}

#[test]
fn a_subagents_window_follows_its_moving_rows() {
    // The session view's running cell anchors on the rows still moving
    // exactly as the main one does (docs/ui/tool-streaming.md): each live
    // row is stamped with the agent's own command clock, and a new call
    // starts from scratch, clock included.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "sudo pacman -Syy".into(),
        detail: None,
        arguments: None,
    });
    run.set_command_elapsed(Some(std::time::Duration::from_secs(1)));
    run.apply(&StreamEvent::ToolScreen {
        call_id: None,
        settled: String::new(),
        live: " core 10%\n extra 100%".into(),
    });
    run.set_command_elapsed(Some(std::time::Duration::from_secs(4)));
    run.apply(&StreamEvent::ToolScreen {
        call_id: None,
        settled: String::new(),
        live: " core 20%\n extra 100%".into(),
    });
    assert_eq!(
        run.live_tail().anchor(2),
        0,
        "the window follows the row still moving"
    );
    run.apply(&StreamEvent::ToolEnd {
        call_id: None,
        output: "Exit code: 0".into(),
        ok: true,
        truncated: false,
        run: crate::stream::ToolRun::default(),
    });
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: "ls".into(),
        detail: None,
        arguments: None,
    });
    assert_eq!(
        run.command_elapsed, None,
        "a new command has run no time yet"
    );
    assert_eq!(*run.live_tail(), crate::app::LiveTail::default());
}

#[test]
fn every_flush_point_is_declared() {
    // `AgentRun::will_flush_segment` is the boundary's only source of
    // truth for where the session view must finish + reset its
    // `StreamRender` (`docs/agents/agent-view-streaming.md`). It has to agree
    // with what `apply` actually does to the buffer for **every** event,
    // or the view falls behind the fold again — which is how `HookNote`
    // came to drop a reply's withheld tail and leave the render pointing
    // at a buffer that no longer existed. Asked of the run before the
    // fold, as the boundary asks it: one boundary, the piece opening a
    // `write`'s preview, is a matter of the run's state.
    for (name, event) in one_of_every_event() {
        let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
        run.apply(&StreamEvent::Chunk("partial answer".to_string()));
        let flushes = run.will_flush_segment(&event);
        run.apply(&event);
        // `flush_segment` *takes* the buffer, so a finalised segment
        // leaves `None` — a `Chunk` merely appending to it has not
        // finalised anything.
        let consumed = run.streaming.is_none();
        assert_eq!(
            flushes, consumed,
            "{name}: the predicate and the fold disagree about the segment"
        );
    }
}

#[test]
fn a_batch_announcement_is_the_rounds_whole_queue() {
    // `App::start_tool_batch` *replaces* the queue — the announcement is
    // the round's whole set of calls (`docs/tools/parallel-tools.md`). The
    // agent's fold appended instead, so an entry left over from an
    // earlier round would sit at the front and every resolution in the
    // new round would land on the wrong cell.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolBatch(vec![ToolCallSummary {
        call_id: None,
        name: "Bash".to_string(),
        args: "stale".to_string(),
    }]));
    run.apply(&StreamEvent::ToolBatch(vec![
        ToolCallSummary {
            call_id: None,
            name: "Read".to_string(),
            args: "a.md".to_string(),
        },
        ToolCallSummary {
            call_id: None,
            name: "Read".to_string(),
            args: "b.md".to_string(),
        },
    ]));
    let queued: Vec<&str> = run.tool_queue.iter().map(|t| t.args.as_str()).collect();
    assert_eq!(queued, ["a.md", "b.md"], "the new batch is the queue");
}

/// A run mid-batch: `args` announced, the first call started — or,
/// with `started` false, still waiting on its permission prompt.
fn run_mid_batch(args: &[&str], started: bool) -> AgentRun {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolBatch(
        args.iter()
            .map(|args| ToolCallSummary {
                call_id: None,
                name: "Bash".to_string(),
                args: (*args).to_string(),
            })
            .collect(),
    ));
    if started {
        run.apply(&StreamEvent::ToolStart {
            call_id: None,
            name: "Bash".to_string(),
            args: args[0].to_string(),
            detail: None,
            arguments: None,
        });
    }
    run
}

/// Every tool record on `run`'s transcript as `(args, status, output)`.
fn resolved(run: &AgentRun) -> Vec<(String, ToolStatus, String)> {
    run.history
        .iter()
        .filter_map(|item| match item {
            HistoryItem::Tool(t) => Some((t.args.clone(), t.status, t.output.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn stopping_an_agent_mid_batch_resolves_every_call_in_it() {
    // The main session's rule, one level down (docs/input/interrupt.md): the
    // running call AND every `⎿ Waiting…` sibling behind it resolve as
    // `Interrupted by user` on the agent's own transcript, so its session
    // view and its Ctrl+D keep a cell per call the agent made.
    let interrupted = crate::app::INTERRUPT_TOOL_OUTPUT.to_string();
    for started in [true, false] {
        let mut run = run_mid_batch(&["ping a", "ls"], started);
        assert!(run.interrupt());
        assert_eq!(
            resolved(&run),
            vec![
                (
                    "ping a".to_string(),
                    ToolStatus::Failed,
                    interrupted.clone()
                ),
                ("ls".to_string(), ToolStatus::Failed, interrupted.clone()),
            ],
            "front started: {started}"
        );
        assert!(run.tool_queue.is_empty());
    }
}

#[test]
fn an_agent_error_mid_batch_resolves_every_call_in_it() {
    let mut run = run_mid_batch(&["ping a", "ls"], true);
    assert!(run.apply(&StreamEvent::Error("boom".into())));
    let failed: Vec<(String, ToolStatus)> = resolved(&run)
        .into_iter()
        .map(|(args, status, _)| (args, status))
        .collect();
    assert_eq!(
        failed,
        vec![
            ("ping a".to_string(), ToolStatus::Failed),
            ("ls".to_string(), ToolStatus::Failed),
        ]
    );
    assert!(run.tool_queue.is_empty());
}

#[test]
fn every_resolution_records_a_truncated_result() {
    // The main boundary flags `truncated` on all three resolutions
    // (`tui::stream`'s ToolEnd/ToolRejected/ToolAnswered arms), so the
    // expanded cell appends its dim `…` marker. The agent's fold honoured
    // it on `ToolEnd` alone, so a subagent's capped `write`/`skill`
    // result silently claimed to be whole.
    let start = StreamEvent::ToolStart {
        call_id: None,
        name: "Write".to_string(),
        args: "f.py".to_string(),
        detail: None,
        arguments: None,
    };
    let resolutions: [(&str, StreamEvent); 3] = [
        (
            "ToolEnd",
            StreamEvent::ToolEnd {
                call_id: None,
                output: "out".to_string(),
                ok: true,
                truncated: true,
                run: crate::stream::ToolRun::default(),
            },
        ),
        (
            "ToolRejected",
            StreamEvent::ToolRejected {
                call_id: None,
                display: "no".to_string(),
                result: "refused".to_string(),
                truncated: true,
                run: crate::stream::ToolRun::default(),
            },
        ),
        (
            "ToolAnswered",
            StreamEvent::ToolAnswered {
                call_id: None,
                display: "done".to_string(),
                result: "ack".to_string(),
                truncated: true,
                run: crate::stream::ToolRun::default(),
            },
        ),
    ];
    for (name, event) in resolutions {
        let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
        run.apply(&start);
        run.apply(&event);
        let Some(HistoryItem::Tool(tool)) = run.history.last() else {
            panic!("{name} recorded no cell");
        };
        assert!(tool.truncated, "{name} dropped the truncation marker");
    }
}

#[test]
fn a_backgrounded_call_charges_its_acknowledgement() {
    // The main session's `resolve_front_tool` folds every resolution's
    // output into the `↑` tally — a `run_in_background` launch included,
    // since its acknowledgement is text the model reads. The agent's fold
    // charged nothing for it.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".to_string(),
        args: "sleep 30".to_string(),
        detail: None,
        arguments: None,
    });
    let before = run.tokens;
    run.apply(&StreamEvent::ToolBackgrounded {
        call_id: None,
        id: "b1".to_string(),
        output: "Command running in the background with id b1".to_string(),
        run: crate::stream::ToolRun::default(),
    });
    assert!(
        run.tokens > before,
        "the launch acknowledgement is uploaded text: {before} -> {}",
        run.tokens
    );
}

#[test]
fn a_backend_error_leaves_the_call_it_killed_last() {
    // The failure resolves the running call red — and the session view
    // commits that cell before the red notice, which is the one thing a
    // failure does *not* record (`docs/agents/agent-view-streaming.md`).
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".to_string(),
        args: "sleep 30".to_string(),
        detail: None,
        arguments: None,
    });
    run.apply(&StreamEvent::Error("boom".to_string()));
    let Some(HistoryItem::Tool(tool)) = run.history.last() else {
        panic!("the killed call is the last item: {:?}", run.history);
    };
    assert_eq!(tool.status, ToolStatus::Failed);
    assert!(
        !run.history
            .iter()
            .any(|item| matches!(item, HistoryItem::Message(m) if m.role == Role::Error)),
        "the notice is the view's, not the transcript's"
    );
}

#[test]
fn a_retrying_agent_says_so_and_the_next_content_clears_it() {
    // Main parity (`docs/providers/llm.md`): a subagent reconnecting shows
    // `retrying {n}/{max}` in its session view's status line instead of a
    // bare spinner, and the first streamed content takes it back down.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::Retrying { attempt: 2, max: 5 });
    assert_eq!(
        run.retry,
        Some(crate::app::RetryInfo { attempt: 2, max: 5 })
    );
    run.apply(&chunk("here we go"));
    assert!(run.retry.is_none(), "content ends the reconnect");
}

#[test]
fn a_retry_withdraws_what_the_agents_failed_attempt_streamed() {
    // Main parity (docs/providers/offline.md *A connection lost mid-reply*): the
    // agent's connection died under its reply, and the restart takes the
    // partial back off its transcript and strip — the text a thinking
    // phase finalised and the thought it settled included — so the reply
    // that gets through is the only one its session view shows.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("First, "));
    // The boundary's half of a shown phase: opened (the text before it
    // finalised), then settled into a cell.
    run.begin_reasoning();
    run.apply(&StreamEvent::ThinkingStart);
    run.apply(&StreamEvent::ThinkingChunk("weighing".into()));
    assert!(run.finish_reasoning(2).is_some());
    run.apply(&StreamEvent::ThinkingEnd);
    run.apply(&chunk("so the\n\nparser"));
    assert_eq!(
        run.history.len(),
        3,
        "the prompt, the finalised text, the thought"
    );
    run.apply(&StreamEvent::Retrying { attempt: 1, max: 3 });
    assert_eq!(run.history.len(), 1, "the prompt alone: {:?}", run.history);
    assert!(
        run.streaming.as_deref().is_none_or(str::is_empty),
        "the partial reply went with it"
    );
    assert_eq!(
        run.retry,
        Some(crate::app::RetryInfo { attempt: 1, max: 3 })
    );
    assert_eq!(
        run.activity(),
        "Working…",
        "the roster row no longer quotes what was withdrawn"
    );
    run.apply(&chunk("The whole reply."));
    assert_eq!(run.streaming.as_deref(), Some("The whole reply."));
    assert_eq!(
        run.activity(),
        format!("{AGENT_UPDATE_LABEL}: The whole reply."),
        "the row reads the new attempt from its start"
    );
}

#[test]
fn a_retry_drops_the_agents_open_thought_and_half_made_call() {
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.begin_reasoning();
    run.apply(&StreamEvent::ThinkingStart);
    run.apply(&StreamEvent::ThinkingChunk("weighing".into()));
    run.set_thinking(Some(Duration::from_secs(3)));
    run.apply(&StreamEvent::ToolCallDelta(
        crate::stream::ToolCallFragment {
            index: 0,
            name: "write".into(),
            arguments: r##"{"path": "notes.md", "content": "# No"##.into(),
        },
    ));
    run.apply(&StreamEvent::Retrying { attempt: 1, max: 3 });
    assert_eq!(run.reasoning(), None, "dropped, never settled");
    assert_eq!(run.thinking, None, "its `Thinking for Ns` clause goes too");
    assert!(run.drafts().is_empty(), "the half-made call goes too");
    assert_eq!(run.history.len(), 1, "nothing was recorded for it");
}

#[test]
fn an_agent_waiting_for_the_connection_says_so_until_content_arrives() {
    // Main parity (`docs/providers/offline.md`): the wait begins at the runtime the
    // first announcement lands on, later announcements move the count
    // on and keep that start, a retry announcement or any content ends
    // it, and it never shows beside the bounded retry clause.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.runtime = Duration::from_secs(7);
    run.apply(&StreamEvent::Retrying { attempt: 1, max: 3 });
    run.apply(&StreamEvent::Offline {
        host: "api.venice.ai".into(),
        attempts: 1,
    });
    assert_eq!(
        run.offline,
        Some(crate::app::OfflineInfo {
            host: "api.venice.ai".into(),
            attempts: 1,
            began: Duration::from_secs(7),
        })
    );
    assert!(run.retry.is_none(), "the outage replaces the retry clause");
    run.runtime = Duration::from_secs(19);
    run.apply(&StreamEvent::Offline {
        host: "api.venice.ai".into(),
        attempts: 2,
    });
    let info = run.offline.as_ref().expect("still waiting");
    assert_eq!((info.attempts, info.began), (2, Duration::from_secs(7)));
    run.apply(&chunk("back"));
    assert!(run.offline.is_none(), "content ends the wait");

    run.apply(&StreamEvent::Offline {
        host: "api.venice.ai".into(),
        attempts: 1,
    });
    run.apply(&StreamEvent::Retrying { attempt: 1, max: 3 });
    assert!(
        run.offline.is_none(),
        "the host answered: a retry ends the wait"
    );
    assert!(run.retry.is_some());
}

#[test]
fn a_thinking_delta_is_only_buffered_while_a_phase_is_open() {
    // The display gate lives at the boundary: with it off nothing calls
    // `begin_reasoning`, so a delta is counted and dropped, exactly as
    // before (docs/ui/thinking-stream.md).
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&StreamEvent::ThinkingChunk("unseen".into()));
    assert!(run.reasoning().is_none());
    assert!(run.tokens > 0, "still counted");
    run.begin_reasoning();
    run.apply(&StreamEvent::ThinkingChunk("weighing ".into()));
    run.apply(&StreamEvent::ThinkingChunk("it".into()));
    assert_eq!(run.reasoning(), Some("weighing it"));
}

#[test]
fn opening_a_phase_finalises_the_text_before_it() {
    // Flush before you interleave: a model that reasons *after* it has
    // begun answering must not have the cell spliced into the paragraph
    // that was streaming (CLAUDE.md invariant 4).
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.apply(&chunk("Let me think. "));
    run.begin_reasoning();
    assert!(run.streaming.is_none(), "the segment was flushed");
    let HistoryItem::Message(m) = &run.history[1] else {
        panic!("the text became its own message");
    };
    assert_eq!(m.text, "Let me think. ");
}

#[test]
fn the_rounds_usage_frame_snaps_the_thought_to_the_providers_count() {
    // The main session's snap (docs/ui/thinking-stream.md): the token
    // estimate gives way to `completion_tokens_details.reasoning_tokens`.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.begin_reasoning();
    run.apply(&StreamEvent::ThinkingChunk(
        "a long chain of thought".into(),
    ));
    let settled = run.finish_reasoning(2).expect("text settles");
    assert!(settled.tokens > 0, "estimated first");
    run.apply(&StreamEvent::Usage(TokenUsage {
        input: 10,
        output: 20,
        cached: 0,
        cache_write: 0,
        reasoning: 169,
    }));
    let HistoryItem::Reasoning(r) = &run.history[1] else {
        panic!("the cell is on the transcript");
    };
    assert_eq!(r.tokens, 169, "snapped to the provider's own count");
}

#[test]
fn an_interrupt_keeps_the_partial_thought() {
    // An Esc/`x` mid-thought settles the phase rather than dropping it —
    // what streamed is what the user saw.
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.begin_reasoning();
    run.apply(&StreamEvent::ThinkingChunk("half a thought".into()));
    run.set_thinking(Some(Duration::from_secs(4)));
    assert!(run.interrupt());
    let HistoryItem::Reasoning(r) = &run.history[1] else {
        panic!("the partial thought settled: {:?}", run.history);
    };
    assert_eq!(r.text, "half a thought");
    assert_eq!(r.secs, 4, "the injected elapsed is the phase's");
    assert!(run.reasoning().is_none());
}

#[test]
fn a_backend_error_settles_the_open_phase_first() {
    // The thought is recorded ahead of the partial reply and the failure
    // — and *ahead of any text still buffered*, which is the order the
    // boundary commits in too (`Session::settle_agent_reasoning` runs
    // before the segment flush). Screen and history must agree or a
    // rebuild reorders them (CLAUDE.md invariant 4).
    let mut run = AgentRun::new("a1", "d", GENERAL_PURPOSE, "p", false);
    run.begin_reasoning();
    run.apply(&StreamEvent::ThinkingChunk("thinking".into()));
    run.apply(&chunk("half an answer"));
    run.apply(&StreamEvent::Error("boom".into()));
    let kinds: Vec<&str> = run
        .history
        .iter()
        .map(|item| match item {
            HistoryItem::Reasoning(_) => "reasoning",
            HistoryItem::Message(_) => "message",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        ["message", "reasoning", "message"],
        "prompt, the thought, then the partial: {:?}",
        run.history
    );
}

// --- The lead's companions (`docs/agents/agent-tools.md`) ---

/// A registry and the receiving end of its channel, for the tests that
/// read what it tells the loop.
fn watched_registry() -> (
    AgentRegistry,
    tokio::sync::mpsc::UnboundedReceiver<AgentEvent>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    (AgentRegistry::new(tx), rx)
}

fn drain(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

#[test]
fn a_registered_agent_keeps_what_it_was_launched_as() {
    let registry = test_registry();
    let (id, _cancel) =
        registry.register_agent("explore", "Fetch GitHub profile", "Get devuser's profile");
    assert_eq!(
        registry.description(&id).as_deref(),
        Some("Fetch GitHub profile")
    );
    let snap = registry.snapshot(&id).expect("registered");
    assert_eq!(snap.id, id);
    assert_eq!(snap.agent_type, "explore");
    assert_eq!(snap.description, "Fetch GitHub profile");
    assert_eq!(snap.state, AgentState::Running);
    assert_eq!(snap.tool_uses, 0);
    assert!(registry.snapshot("nope").is_none());
    assert!(registry.description("nope").is_none());
}

#[test]
fn a_snapshot_lists_the_calls_made_and_how_the_agent_settled() {
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.record_call(&id, "Bash(curl -s https://api.github.com/users/devuser)");
    registry.record_call(&id, "Read(notes.md)");
    let running = registry.snapshot(&id).unwrap();
    assert_eq!(running.tool_uses, 2);
    assert_eq!(
        running.calls,
        [
            "Bash(curl -s https://api.github.com/users/devuser)",
            "Read(notes.md)"
        ]
    );
    registry.finish(&id, Ok("the answer".into()), vec![ChatMessage::user("p")]);
    assert_eq!(
        registry.snapshot(&id).unwrap().state,
        AgentState::Done("the answer".into())
    );
    let (failed, _c) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.finish(&failed, Err("rate limited".into()), Vec::new());
    assert_eq!(
        registry.snapshot(&failed).unwrap().state,
        AgentState::Failed("rate limited".into())
    );
}

#[test]
fn settling_with_nobody_waiting_sends_the_terminal_event_at_once() {
    // The foreground group's wait loop sees `is_done` under the same lock
    // the event is sent under, so its resolution always finds the event
    // already queued (docs/agents/agent-tools.md *One notice per answer*).
    let (registry, mut rx) = watched_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.settle(
        &id,
        Ok("done".into()),
        Vec::new(),
        Some(StreamEvent::StreamDone),
    );
    assert!(registry.is_done(&id));
    assert_eq!(
        drain(&mut rx),
        [AgentEvent::Settled {
            id: id.clone(),
            event: StreamEvent::StreamDone,
            observed: false,
            run: 0,
        }]
    );
}

#[test]
fn a_waiter_holds_the_terminal_event_and_releases_it_observed() {
    // An `agentoutput` waiting on the agent reports its outcome itself, so
    // the settle it saw owes no completion notice.
    let (registry, mut rx) = watched_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.begin_wait(&id));
    registry.settle(
        &id,
        Ok("done".into()),
        Vec::new(),
        Some(StreamEvent::StreamDone),
    );
    assert!(drain(&mut rx).is_empty(), "held for the waiter");
    let snap = registry.end_wait(&id, true).expect("still known");
    assert_eq!(snap.state, AgentState::Done("done".into()));
    assert_eq!(
        drain(&mut rx),
        [AgentEvent::Settled {
            id: id.clone(),
            event: StreamEvent::StreamDone,
            observed: true,
            run: 0,
        }]
    );
}

#[test]
fn a_waiter_that_reports_nothing_releases_the_event_unobserved() {
    // Esc cancelled the wait: the lead never read the outcome, so the
    // notice is still owed.
    let (registry, mut rx) = watched_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.begin_wait(&id));
    registry.settle(
        &id,
        Err("boom".into()),
        Vec::new(),
        Some(StreamEvent::Error("boom".into())),
    );
    let _ = registry.end_wait(&id, false);
    assert_eq!(
        drain(&mut rx),
        [AgentEvent::Settled {
            id,
            event: StreamEvent::Error("boom".into()),
            observed: false,
            run: 0,
        }]
    );
}

#[test]
fn a_wait_that_ends_first_leaves_the_settle_its_own_notice() {
    // The wait passed with the agent still running: what settles later
    // was never reported, so it goes out unobserved, at once.
    let (registry, mut rx) = watched_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.begin_wait(&id));
    let snap = registry.end_wait(&id, true).expect("known");
    assert_eq!(snap.state, AgentState::Running);
    registry.settle(
        &id,
        Ok("done".into()),
        Vec::new(),
        Some(StreamEvent::StreamDone),
    );
    assert_eq!(
        drain(&mut rx),
        [AgentEvent::Settled {
            id,
            event: StreamEvent::StreamDone,
            observed: false,
            run: 0,
        }]
    );
    assert!(!registry.begin_wait("nope"), "nothing to wait on");
    assert!(registry.end_wait("nope", true).is_none());
}

#[test]
fn a_users_stop_says_whether_a_waiting_report_covers_it() {
    let registry = test_registry();
    let (watched, _c1) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.begin_wait(&watched));
    let stop = registry.kill(&watched);
    assert!(stop.was_live && stop.watched, "{stop:?}");
    let (alone, _c2) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    let stop = registry.kill(&alone);
    assert!(stop.was_live && !stop.watched, "{stop:?}");
    assert_eq!(
        registry.snapshot(&alone).unwrap().state,
        AgentState::Stopped { by_user: true }
    );
}

#[test]
fn a_stop_names_the_run_it_found() {
    // A note of a run's end is tagged with that run (docs/agents/agent-tools.md
    // *A report covers one run*), and the user's stop posts one: it names
    // the run it stopped, rather than the note taking whichever run is
    // going when it is posted.
    let registry = test_registry();
    let (id, _c1) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.settle(&id, Ok("first".into()), vec![ChatMessage::user("p")], None);
    let _ = registry
        .begin_continuation(&id)
        .expect("a stored conversation");
    assert_eq!(
        registry.kill(&id).run,
        1,
        "the user's stop of the second run"
    );
    let (led, _c2) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.settle(&led, Ok("first".into()), vec![ChatMessage::user("p")], None);
    let _ = registry
        .begin_continuation(&led)
        .expect("a stored conversation");
    assert_eq!(
        registry.stop(&led, None).expect("known").run,
        1,
        "the lead's"
    );
    assert_eq!(
        registry.stop(&led, None).expect("known").run,
        1,
        "the lead's, of a run already over"
    );
}

#[test]
fn the_leads_stop_tells_the_loop_and_keeps_the_agent_resumable() {
    // `agentkill`: no event of the run's own will say it stopped (a
    // cancelled loop returns silently), so the registry says so — and
    // unlike the user's `x`, the conversation stays resumable.
    let (registry, mut rx) = watched_registry();
    let (id, cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    let stop = registry.stop(&id, None).expect("known");
    assert!(stop.was_live);
    assert!(cancel.is_cancelled());
    assert_eq!(drain(&mut rx), [AgentEvent::Stopped { id: id.clone() }]);
    assert_eq!(
        registry.snapshot(&id).unwrap().state,
        AgentState::Stopped { by_user: false }
    );
    assert_eq!(
        registry.resume(&id, "redirect"),
        Resume::Stopping,
        "its thread has not let go yet"
    );
    registry.finish(&id, Err("stopped".into()), vec![ChatMessage::user("p")]);
    assert!(matches!(
        registry.resume(&id, "redirect"),
        Resume::Continue { .. }
    ));
    let again = registry.stop(&id, None).expect("known");
    assert!(again.was_live, "the resumed run is live again");
    assert!(registry.stop("nope", None).is_none());
}

#[test]
fn resume_queues_into_a_running_agent() {
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert_eq!(registry.resume(&id, "also check gists"), Resume::Queued);
    assert_eq!(registry.take_pending_inputs(&id), ["also check gists"]);
}

#[test]
fn resume_continues_a_settled_agent_from_its_stored_conversation() {
    // The follow-up: a finished agent keeps its context. The message
    // rides the pending inputs so the continuation's first round
    // announces it (`Steered`) — which is what records it on the agent's
    // transcript and reopens its roster row.
    let (registry, mut rx) = watched_registry();
    let (id, _cancel) =
        registry.register_agent("explore", "Fetch GitHub profile", "Get the profile");
    registry.finish(
        &id,
        Ok("done".into()),
        vec![ChatMessage::user("Get the profile")],
    );
    let Resume::Continue {
        messages,
        cancel,
        agent_type,
    } = registry.resume(&id, "also list the repos")
    else {
        panic!("a settled agent continues");
    };
    assert_eq!(messages.len(), 1, "the stored conversation");
    assert_eq!(agent_type, "explore");
    assert!(!cancel.is_cancelled());
    assert!(!registry.is_done(&id), "running again");
    assert_eq!(registry.snapshot(&id).unwrap().state, AgentState::Running);
    assert_eq!(registry.take_pending_inputs(&id), ["also list the repos"]);
    assert_eq!(
        drain(&mut rx),
        [AgentEvent::Resumed {
            id: id.clone(),
            description: "Fetch GitHub profile".into(),
            agent_type: "explore".into(),
            prompt: "Get the profile".into(),
        }],
        "the roster is told before the continuation's first event"
    );
}

#[test]
fn resume_refuses_an_agent_the_user_stopped_or_never_had() {
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    let _ = registry.kill(&id);
    registry.finish(&id, Err("stopped".into()), vec![ChatMessage::user("p")]);
    assert_eq!(registry.resume(&id, "go on"), Resume::StoppedByUser);
    assert_eq!(registry.resume("nope", "go on"), Resume::Unknown);
}

#[test]
fn a_resume_releases_a_terminal_event_still_held_for_a_waiter() {
    // The continuation's events must never overtake the previous run's
    // terminal event, or the roster settles a run that is going again.
    let (registry, mut rx) = watched_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.begin_wait(&id));
    registry.settle(
        &id,
        Ok("done".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    assert!(matches!(
        registry.resume(&id, "more"),
        Resume::Continue { .. }
    ));
    let events = drain(&mut rx);
    assert!(
        matches!(
            events.first(),
            Some(AgentEvent::Settled {
                observed: false,
                ..
            })
        ),
        "released first, unobserved: {events:?}"
    );
    let snap = registry.end_wait(&id, true).expect("known");
    assert_eq!(
        snap.state,
        AgentState::Running,
        "the waiter sees the resumed run"
    );
    assert!(drain(&mut rx).is_empty(), "nothing released twice");
}

#[test]
fn the_registry_keeps_only_the_newest_settled_agents() {
    let registry = test_registry();
    let mut ids = Vec::new();
    for n in 0..=AGENT_RETAINED_MAX {
        let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, &format!("task {n}"), "p");
        registry.finish(&id, Ok("done".into()), Vec::new());
        ids.push(id);
    }
    assert!(
        registry.snapshot(&ids[0]).is_none(),
        "the oldest settled agent is let go"
    );
    for id in &ids[1..] {
        assert!(registry.snapshot(id).is_some(), "{id} is kept");
    }
    // A running agent never counts against the cap, and is never evicted.
    let (running, _cancel) = registry.register_agent(GENERAL_PURPOSE, "live", "p");
    assert!(registry.snapshot(&running).is_some());
    assert!(registry.snapshot(&ids[1]).is_some());
}

#[test]
fn snapshots_list_the_agents_in_launch_order() {
    let registry = test_registry();
    let (first, _c1) = registry.register_agent(GENERAL_PURPOSE, "first", "p");
    let (second, _c2) = registry.register_agent("explore", "second", "p");
    let listed: Vec<String> = registry
        .snapshots()
        .into_iter()
        .map(|snap| snap.id)
        .collect();
    assert_eq!(listed, [first, second]);
}

#[test]
fn clear_forgets_every_agent() {
    let registry = test_registry();
    let (id, cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.clear();
    assert!(
        cancel.is_cancelled(),
        "a wiped conversation stops its agents"
    );
    assert!(registry.snapshot(&id).is_none());
    assert!(registry.snapshots().is_empty());
}

#[test]
fn a_settled_agents_runtime_stands_still() {
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.finish(&id, Ok("done".into()), Vec::new());
    let first = registry.snapshot(&id).unwrap().runtime;
    std::thread::sleep(Duration::from_millis(15));
    assert_eq!(registry.snapshot(&id).unwrap().runtime, first);
}

#[test]
fn a_resumed_agents_runtime_and_calls_add_up_over_its_runs() {
    // One agent, one life: its report's numbers cover every run, so a
    // follow-up's frame never reads as that run's alone (a live model read
    // `9s · 5 tool uses` as five calls the follow-up made — it made none).
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.record_call(&id, "Bash(curl one)");
    std::thread::sleep(Duration::from_millis(20));
    registry.finish(&id, Ok("first".into()), vec![ChatMessage::user("p")]);
    let first = registry.snapshot(&id).unwrap().runtime;
    assert!(matches!(
        registry.resume(&id, "and then?"),
        Resume::Continue { .. }
    ));
    let _ = registry.take_pending_inputs(&id);
    std::thread::sleep(Duration::from_millis(20));
    registry.finish(&id, Ok("second".into()), Vec::new());
    let snap = registry.snapshot(&id).unwrap();
    assert!(snap.runtime > first, "{:?} after {first:?}", snap.runtime);
    assert_eq!(snap.tool_uses, 1);
    assert_eq!(
        snap.follow_ups,
        [FollowUp {
            at: 1,
            from_user: None
        }],
        "the message arrived after the first call"
    );
}

#[test]
fn every_message_an_agent_reads_marks_its_timeline() {
    // A message the loop drains (a queued `agentsend`, a resume's, one the
    // user typed into its session) and a chat continuation alike: the
    // report shows where in the agent's calls each arrived.
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.take_pending_inputs(&id).is_empty());
    assert!(
        registry.snapshot(&id).unwrap().follow_ups.is_empty(),
        "nothing read"
    );
    registry.record_call(&id, "Read(a)");
    assert!(registry.queue_input(&id, "also b"));
    let _ = read_queued(&registry, &id);
    registry.record_call(&id, "Read(b)");
    registry.note_user_message(&id, "and c");
    let at: Vec<usize> = registry
        .snapshot(&id)
        .unwrap()
        .follow_ups
        .iter()
        .map(|follow_up| follow_up.at)
        .collect();
    assert_eq!(at, [1, 2]);
}

#[test]
fn a_message_the_user_typed_is_told_apart_from_the_leads() {
    // The lead must learn which messages the *user* sent its agent — a
    // session-view chat it never wrote (docs/agents/agent-tools.md). The report
    // quotes the user's message where it arrived; the lead's own
    // `agentsend` (and a routed shell note) stays the bare mark.
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.queue_input(&id, "lead's own"));
    let _ = read_queued(&registry, &id);
    registry.record_call(&id, "Read(a)");
    assert!(registry.queue_user_input(&id, "also check\nManila"));
    assert_eq!(
        read_queued(&registry, &id),
        ["also check\nManila"],
        "the loop reads the text either way"
    );
    assert_eq!(registry.user_messages(&id), ["also check\nManila"]);
    let report = output_report(&registry.snapshot(&id).unwrap());
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(
        lines[1..4],
        [
            AGENT_FOLLOW_UP_MARK,
            "Read(a)",
            "— message from the user: also check Manila —"
        ],
        "{report}"
    );
}

#[test]
fn the_users_messages_are_the_current_runs_own() {
    // What a run's answer is told it answered: a continuation starts
    // with none, then counts the chat that started it.
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.queue_user_input(&id, "first"));
    let _ = registry.take_pending_inputs(&id);
    registry.finish(&id, Ok("one".into()), vec![ChatMessage::user("p")]);
    assert_eq!(registry.user_messages(&id), ["first"]);
    assert!(registry.begin_continuation(&id).is_some());
    assert!(registry.user_messages(&id).is_empty(), "a new run");
    registry.note_user_message(&id, "second");
    assert_eq!(registry.user_messages(&id), ["second"]);
    assert!(registry.user_messages("nope").is_empty());
}

#[test]
fn alt_up_takes_back_a_users_queued_message() {
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.queue_user_input(&id, "oops"));
    assert!(registry.has_pending_inputs(&id));
    assert_eq!(registry.take_last_input(&id).as_deref(), Some("oops"));
    assert!(registry.take_pending_inputs(&id).is_empty());
    assert!(registry.user_messages(&id).is_empty(), "never read");
}

#[test]
fn alt_up_never_takes_back_the_leads_message() {
    // Alt+Up pulls back what the *user* typed: a message the lead's
    // `agentsend` queued has no row and no composer to return to, so
    // taking it would silently drop it.
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.queue_user_input(&id, "mine"));
    assert!(matches!(registry.resume(&id, "the lead's"), Resume::Queued));
    assert_eq!(registry.take_last_input(&id).as_deref(), Some("mine"));
    assert_eq!(
        registry.take_last_input(&id),
        None,
        "only the lead's is left"
    );
    assert_eq!(registry.take_pending_inputs(&id), ["the lead's"]);
}

#[test]
fn a_stop_drops_the_messages_its_loop_never_read() {
    // A stopped loop reads nothing more, and the roster drops the rows
    // that promised a delivery — so the registry drops the messages too,
    // or a later continuation delivers them unannounced, the user's
    // passing as the lead's (docs/input/queue.md).
    let registry = test_registry();
    let (by_user, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.queue_user_input(&by_user, "too late"));
    let _ = registry.kill(&by_user);
    assert!(!registry.has_pending_inputs(&by_user));

    let (by_lead, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.queue_user_input(&by_lead, "too late"));
    let _ = registry.stop(&by_lead, None);
    assert!(!registry.has_pending_inputs(&by_lead));
    registry.settle(
        &by_lead,
        Ok(String::new()),
        vec![ChatMessage::user("p")],
        None,
    );
    assert!(matches!(
        registry.resume(&by_lead, "go on"),
        Resume::Continue { .. }
    ));
    assert_eq!(registry.take_pending_inputs(&by_lead), ["go on"]);
}

/// A finished agent whose completion notice went out unobserved, with a
/// board to post it on.
fn settled_with_notice() -> (AgentRegistry, String, crate::background::BackgroundRegistry) {
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.settle(
        &id,
        Ok("answer".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let board = crate::background::BackgroundRegistry::new(tx, std::env::temp_dir());
    (registry, id, board)
}

/// A board whose shell `echo done` exited with nobody waiting — its exit
/// unread — reaching `agents` for the notes routed to an agent's queue.
#[cfg(unix)]
/// A shell `agent` launched, exited with nobody watching — its note not
/// yet routed.
fn exited_shell(
    agents: &AgentRegistry,
    agent: &str,
) -> (crate::background::BackgroundRegistry, String) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "zeroth-agents-route-test-{}-{seq}",
        std::process::id()
    ));
    let board = crate::background::BackgroundRegistry::new(tx, dir).with_agents(agents.clone());
    let origin = crate::background::BgOrigin {
        agent_id: agent.to_string(),
        agent_type: GENERAL_PURPOSE.to_string(),
    };
    let task = board
        .launch_from("echo done", None, true, Some(origin))
        .expect("launches");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match rx.try_recv() {
            Ok(crate::background::BgEvent::Exited {
                id,
                observed: false,
                ..
            }) if id == task.id => return (board, id),
            Ok(_) => {}
            Err(_) => {
                assert!(std::time::Instant::now() < deadline, "never exited");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn a_routed_exit_is_claimed_by_its_launcher_from_its_own_queue() {
    // One level down, the reported race: a subagent's shell ended while
    // the subagent was writing a call to it, and the note went to the
    // subagent's own queue, unread. Its call reports the exit itself and
    // takes the note back (docs/tools/bash-tools.md *One notice per exit*).
    let registry = test_registry();
    let (agent, _cancel) = registry.register(GENERAL_PURPOSE);
    let (board, shell) = exited_shell(&registry, &agent);
    assert!(matches!(
        registry.route_shell_note(&agent, &shell, "[background] note", false, &board),
        Route::Routed(_)
    ));
    assert!(
        board.claim_exit(&shell, None).is_none(),
        "the lead cannot take a subagent's note"
    );
    let exit = board
        .claim_exit(&shell, Some(&agent))
        .expect("its launcher claims it from its own queue");
    assert_eq!(exit.code, Some(0));
    assert!(
        registry.take_pending_inputs(&agent).is_empty(),
        "the note it would have read is gone"
    );
}

#[cfg(unix)]
#[test]
fn a_routed_exit_the_agent_already_read_is_not_claimed() {
    let registry = test_registry();
    let (agent, _cancel) = registry.register(GENERAL_PURPOSE);
    let (board, shell) = exited_shell(&registry, &agent);
    registry.route_shell_note(&agent, &shell, "[background] note", false, &board);
    assert_eq!(registry.take_pending_inputs(&agent), ["[background] note"]);
    assert!(board.claim_exit(&shell, Some(&agent)).is_none());
}

#[cfg(unix)]
#[test]
fn an_exit_a_call_already_reported_routes_nothing() {
    let registry = test_registry();
    let (agent, _cancel) = registry.register(GENERAL_PURPOSE);
    let (board, shell) = exited_shell(&registry, &agent);
    assert!(board.claim_exit(&shell, Some(&agent)).is_some());
    assert_eq!(
        registry.route_shell_note(&agent, &shell, "[background] note", false, &board),
        Route::Covered
    );
    assert!(registry.take_pending_inputs(&agent).is_empty());
}

#[cfg(unix)]
#[test]
fn a_settled_launcher_leaves_its_note_to_the_board() {
    let registry = test_registry();
    let (agent, _cancel) = registry.register(GENERAL_PURPOSE);
    registry.settle(&agent, Ok("done".into()), Vec::new(), None);
    let (board, shell) = exited_shell(&registry, &agent);
    assert_eq!(
        registry.route_shell_note(&agent, &shell, "[background] note", false, &board),
        Route::Unheard
    );
    assert!(
        board
            .post_exit_notice(&shell, "[background] note".into(), true)
            .is_some(),
        "the board takes it, still tracked"
    );
}

/// The board number of a notice the loop posted as owed.
fn posted(post: NoticePost) -> u64 {
    match post {
        NoticePost::Posted(Some(seq)) => seq,
        other => panic!("expected an owed, posted notice, got {other:?}"),
    }
}

#[test]
fn a_report_before_the_notice_is_posted_cancels_it() {
    // `agentoutput` read the answer before the loop got to its notice:
    // the loop must not post it (docs/agents/agent-tools.md *One notice per
    // answer*).
    let (registry, id, board) = settled_with_notice();
    assert!(registry.report_settled(&id, 0, Some(&board)).is_empty());
    assert_eq!(
        registry.post_notice(&id, 0, &board, Some("note".into())),
        NoticePost::Covered
    );
    assert!(board.take_pending_notices().is_empty());
}

#[test]
fn a_report_retracts_a_posted_notice_the_lead_has_not_taken() {
    let (registry, id, board) = settled_with_notice();
    let seq = posted(registry.post_notice(&id, 0, &board, Some("note".into())));
    assert!(registry.report_settled(&id, 0, Some(&board)).is_empty());
    assert!(board.take_pending_notices().is_empty(), "off the board");
    assert!(board.take_retracted(seq), "its cell is not owed either");
    assert!(!board.take_retracted(seq), "once");
}

#[test]
fn a_report_after_the_lead_took_the_notice_is_a_re_read() {
    let (registry, id, board) = settled_with_notice();
    let seq = posted(registry.post_notice(&id, 0, &board, Some("note".into())));
    assert_eq!(board.take_pending_notices().len(), 1, "the lead read it");
    assert!(registry.report_settled(&id, 0, Some(&board)).is_empty());
    assert!(!board.take_retracted(seq), "the cell records what it read");
}

#[test]
fn a_report_retracts_only_its_own_agents_notice() {
    let (registry, id, board) = settled_with_notice();
    board.post_notice("a shell finished".into(), true);
    posted(registry.post_notice(&id, 0, &board, Some("note".into())));
    assert!(registry.report_settled(&id, 0, Some(&board)).is_empty());
    let left = board.take_pending_notices();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].context, "a shell finished");
}

#[test]
fn a_settle_that_owed_no_notice_is_still_accounted_for() {
    // A foreground agent's settle posts nothing (its group carried the
    // answer); its resumed run's later settle must still be the one a
    // report retracts.
    let (registry, id, board) = settled_with_notice();
    assert_eq!(
        registry.post_notice(&id, 0, &board, None),
        NoticePost::Posted(None)
    );
    assert!(matches!(
        registry.resume(&id, "again"),
        Resume::Continue { .. }
    ));
    registry.settle(
        &id,
        Ok("second".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    let seq = posted(registry.post_notice(&id, 1, &board, Some("note".into())));
    assert!(registry.report_settled(&id, 1, Some(&board)).is_empty());
    assert!(board.take_pending_notices().is_empty());
    assert!(board.take_retracted(seq));
}

#[test]
fn a_retraction_costs_only_the_notice_it_took_back() {
    // The loop holds a notice's cell until the lead reads it, so a
    // retracted notice is still held when the resumed agent's next one
    // is posted, both settling at the turn's end: the first must stay
    // taken back and the second keep its cell — which a mark kept per
    // agent could not tell apart.
    let (registry, id, board) = settled_with_notice();
    let first = posted(registry.post_notice(&id, 0, &board, Some("first".into())));
    assert!(registry.report_settled(&id, 0, Some(&board)).is_empty());
    assert!(matches!(
        registry.resume(&id, "again"),
        Resume::Continue { .. }
    ));
    registry.settle(
        &id,
        Ok("second".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    let second = posted(registry.post_notice(&id, 1, &board, Some("second".into())));
    assert!(board.take_retracted(first), "the first stays taken back");
    assert!(!board.take_retracted(second), "the second keeps its cell");
}

#[test]
fn a_notice_a_wait_observed_is_never_retracted_or_suppressed() {
    // A held event released observed posts nothing; a report then has
    // nothing owed and nothing on the board.
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.begin_wait(&id));
    registry.settle(
        &id,
        Ok("answer".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    let _ = registry.end_wait(&id, true);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let board = crate::background::BackgroundRegistry::new(tx, std::env::temp_dir());
    assert!(registry.report_settled(&id, 0, Some(&board)).is_empty());
    assert!(board.take_pending_notices().is_empty());
}

/// `settled_with_notice`'s agent after the user started it again: the
/// lead read its first run's completion, and the note that it runs again
/// is on the board — the run it announces still going.
fn resumed_with_note() -> (
    AgentRegistry,
    String,
    crate::background::BackgroundRegistry,
    u64,
) {
    let (registry, id, board) = settled_with_notice();
    posted(registry.post_notice(&id, 0, &board, Some("finished".into())));
    assert_eq!(board.take_pending_notices().len(), 1, "the lead read it");
    let _ = registry
        .begin_continuation(&id)
        .expect("settled, its conversation stored");
    let seq = registry
        .post_resume_notice(&id, &board, "running again".into())
        .expect("the run it announces is going");
    (registry, id, board, seq)
}

#[test]
fn a_resume_note_is_posted_only_while_its_run_is_going() {
    // Under the registry lock, where a report of the run's end and the
    // lead's stop take it back, so neither can cross it
    // (docs/agents/agent-tools.md *A resume is news when it starts*): a run
    // already over is told by its completion notice, which quotes the
    // user's messages too, and one the lead stopped by the stop's report.
    let (registry, id, board) = settled_with_notice();
    assert_eq!(
        registry.post_resume_notice(&id, &board, "running again".into()),
        None,
        "the run is over"
    );
    let _ = registry.begin_continuation(&id).expect("settled");
    let _ = registry.stop(&id, Some(&board));
    assert_eq!(
        registry.post_resume_notice(&id, &board, "running again".into()),
        None,
        "the lead stopped it first"
    );
    assert_eq!(
        registry.post_resume_notice("nope", &board, "running again".into()),
        None
    );
    assert!(board.take_pending_notices().is_empty());
}

#[test]
fn a_report_of_the_resumed_runs_end_takes_its_resume_note_back() {
    // The look lands before the loop has posted the run's completion: the
    // report cancels that notice, and the note that the run is going must
    // go with it — read after the answer, it would say the agent runs and
    // that a notice will come (docs/agents/agent-tools.md *A resume is news when
    // it starts*).
    let (registry, id, board, resume) = resumed_with_note();
    registry.settle(
        &id,
        Ok("second".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    assert!(registry.report_settled(&id, 1, Some(&board)).is_empty());
    assert!(board.take_pending_notices().is_empty(), "off the board");
    assert!(board.take_retracted(resume), "its cell is not owed either");
    assert_eq!(
        registry.post_notice(&id, 1, &board, Some("second".into())),
        NoticePost::Covered
    );
}

#[test]
fn a_report_after_the_completion_was_posted_takes_both_notes_back() {
    let (registry, id, board, resume) = resumed_with_note();
    registry.settle(
        &id,
        Ok("second".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    let completion = posted(registry.post_notice(&id, 1, &board, Some("second".into())));
    assert!(registry.report_settled(&id, 1, Some(&board)).is_empty());
    assert!(board.take_pending_notices().is_empty());
    assert!(board.take_retracted(resume));
    assert!(board.take_retracted(completion));
}

#[test]
fn a_report_of_a_run_the_user_has_since_restarted_keeps_its_resume_note() {
    // The lead's look read the run that ended; the user started it again
    // before the report reached the registry. The completion it read is
    // taken back, but the note that the agent runs again is about a run
    // the lead has not heard of.
    let (registry, id, board) = settled_with_notice();
    let read = posted(registry.post_notice(&id, 0, &board, Some("finished".into())));
    let _ = registry.begin_continuation(&id).expect("settled");
    let resume = registry
        .post_resume_notice(&id, &board, "running again".into())
        .expect("going");
    assert!(registry.report_settled(&id, 0, Some(&board)).is_empty());
    assert!(board.take_retracted(read), "the answer it reported");
    let left = board.take_pending_notices();
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(left[0].seq, resume, "the news it has not had");
}

#[test]
fn the_leads_stop_takes_back_only_the_note_that_the_run_is_going() {
    // `agentkill`'s report is the run's end, and no notice follows a stop
    // the lead made itself: "running again, you will be notified" would
    // reach it after the fact (docs/agents/agent-tools.md). The run before ended
    // in a completion the lead has not read, which says what the stop's
    // report does not.
    let (registry, id, board) = settled_with_notice();
    let earlier = posted(registry.post_notice(&id, 0, &board, Some("finished".into())));
    let _ = registry.begin_continuation(&id).expect("settled");
    let resume = registry
        .post_resume_notice(&id, &board, "running again".into())
        .expect("going");
    let stop = registry.stop(&id, Some(&board)).expect("known");
    assert!(stop.was_live);
    assert!(board.take_retracted(resume));
    let left = board.take_pending_notices();
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(left[0].seq, earlier);
}

#[test]
fn the_leads_stop_of_a_run_already_over_takes_back_its_resume_note() {
    // The resumed run ended before the lead read the note, and the lead
    // stops the agent all the same: nothing is stopped, but the report
    // it gets says the agent is done, so "running again" read after it
    // would contradict it. The run's completion still says the rest.
    let (registry, id, board, resume) = resumed_with_note();
    registry.settle(
        &id,
        Ok("second".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    let stop = registry.stop(&id, Some(&board)).expect("known");
    assert!(!stop.was_live, "nothing was running");
    assert!(board.take_retracted(resume));
    assert!(
        matches!(
            registry.post_notice(&id, 1, &board, Some("second".into())),
            NoticePost::Posted(Some(_))
        ),
        "the run's completion is still owed"
    );
}

#[test]
fn each_continuation_is_a_new_run_and_its_settle_names_it() {
    // A report covers one run, so what it accounts for is counted by run:
    // the launch is run 0, each continuation the next (docs/agents/agent-tools.md
    // *One notice per answer*).
    let (registry, mut rx) = watched_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert_eq!(registry.snapshot(&id).unwrap().run, 0);
    registry.settle(
        &id,
        Ok("first".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    assert!(matches!(
        registry.resume(&id, "more"),
        Resume::Continue { .. }
    ));
    assert_eq!(registry.snapshot(&id).unwrap().run, 1);
    registry.settle(
        &id,
        Ok("second".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    let runs: Vec<u64> = drain(&mut rx)
        .into_iter()
        .filter_map(|event| match event {
            AgentEvent::Settled { run, .. } => Some(run),
            _ => None,
        })
        .collect();
    assert_eq!(runs, [0, 1]);
    let _ = registry.begin_continuation(&id).expect("settled");
    assert_eq!(
        registry.snapshot(&id).unwrap().run,
        2,
        "a chat continuation is a run too"
    );
}

#[test]
fn a_report_cancels_only_its_own_runs_unposted_notice() {
    // Two runs ended unobserved and the loop has posted neither notice: a
    // report of the second cancels the second's, never the first's.
    let (registry, id, board) = settled_with_notice();
    let _ = registry.begin_continuation(&id).expect("settled");
    registry.settle(
        &id,
        Ok("second".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    assert!(
        registry.report_settled(&id, 1, Some(&board)).is_empty(),
        "nothing earlier was posted to hand back"
    );
    assert!(
        matches!(
            registry.post_notice(&id, 0, &board, Some("first".into())),
            NoticePost::Posted(Some(_))
        ),
        "the first run's answer is still owed"
    );
    assert_eq!(
        registry.post_notice(&id, 1, &board, Some("second".into())),
        NoticePost::Covered
    );
}

#[test]
fn a_report_takes_back_every_note_of_its_run() {
    // A note already posted for the run — the user's stop, posted the
    // moment it lands, tagged with the run the stop found — while the
    // run's own end is still owed: the report covers the run, so neither
    // may reach the lead after it.
    let (registry, id, board) = settled_with_notice();
    let posted_note = board.post_agent_notice("stopped".into(), &id, Some(0));
    assert!(registry.report_settled(&id, 0, Some(&board)).is_empty());
    assert!(
        board.take_retracted(posted_note),
        "the posted note is taken back"
    );
    assert_eq!(
        registry.post_notice(&id, 0, &board, Some("note".into())),
        NoticePost::Covered,
        "and the owed one never posts"
    );
}

#[test]
fn a_report_hands_back_an_earlier_runs_unread_answer() {
    // The report covers the run it read. The run before it ended unread
    // on the board, and taking that note back for nothing would lose its
    // answer — so it is handed to the report to carry instead.
    let (registry, id, board) = settled_with_notice();
    let first = posted(registry.post_notice(&id, 0, &board, Some("the first answer".into())));
    let _ = registry.begin_continuation(&id).expect("settled");
    registry.settle(
        &id,
        Ok("second".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    assert_eq!(
        registry.report_settled(&id, 1, Some(&board)),
        ["the first answer"]
    );
    assert!(
        board.take_pending_notices().is_empty(),
        "the report carries it now"
    );
    assert!(board.take_retracted(first), "and its cell is not owed");
    assert_eq!(
        registry.post_notice(&id, 1, &board, Some("second".into())),
        NoticePost::Covered
    );
}

#[test]
fn a_wait_that_follows_the_users_next_run_still_hands_back_the_first_answer() {
    // The lead waits on the agent; the user's message, queued as it wrote
    // its last reply, starts the next run the instant that one ends, so
    // the wait — seeing the agent at work again — reports the second run.
    // The first run's answer, released as a notice by the restart, used
    // to be taken back by that report unread.
    let (registry, mut rx) = watched_registry();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let board = crate::background::BackgroundRegistry::new(tx, std::env::temp_dir());
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    assert!(registry.begin_wait(&id));
    registry.settle(
        &id,
        Ok("first".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    let _ = registry.begin_continuation(&id).expect("settled");
    let first = posted(registry.post_notice(&id, 0, &board, Some("the first answer".into())));
    registry.settle(
        &id,
        Ok("second".into()),
        vec![ChatMessage::user("p")],
        Some(StreamEvent::StreamDone),
    );
    let snap = registry.end_wait(&id, true).expect("known");
    assert_eq!(snap.run, 1);
    assert_eq!(snap.state, AgentState::Done("second".into()));
    assert_eq!(
        registry.report_settled(&id, snap.run, Some(&board)),
        ["the first answer"]
    );
    assert!(board.take_retracted(first));
    let settles: Vec<(u64, bool)> = drain(&mut rx)
        .into_iter()
        .filter_map(|event| match event {
            AgentEvent::Settled { run, observed, .. } => Some((run, observed)),
            _ => None,
        })
        .collect();
    assert_eq!(
        settles,
        [(0, false), (1, true)],
        "the first run owed its notice, the wait reported the second"
    );
}

fn snapshot(state: AgentState, calls: &[&str]) -> AgentSnapshot {
    AgentSnapshot {
        id: "a7k2m9x4q".into(),
        description: "Fetch GitHub profile".into(),
        agent_type: GENERAL_PURPOSE.into(),
        state,
        runtime: Duration::from_secs(42),
        tool_uses: calls.len(),
        calls: calls.iter().map(|call| (*call).to_string()).collect(),
        follow_ups: Vec::new(),
        updates: Vec::new(),
        run: 0,
    }
}

fn update(at: usize, read: usize, text: &str) -> ProgressUpdate {
    ProgressUpdate {
        at,
        read,
        text: text.to_string(),
    }
}

#[test]
fn a_progress_update_is_recorded_where_the_agent_said_it() {
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "Fetch GitHub profile", "p");
    registry.record_update(&id, " \n\n ");
    assert!(
        registry.snapshot(&id).unwrap().updates.is_empty(),
        "a blank run is no update"
    );
    registry.record_update(&id, "\n  I'll start with the profile.  \n");
    registry.record_call(&id, "Bash(curl one)");
    assert!(registry.queue_input(&id, "also check the gists"));
    let _ = read_queued(&registry, &id);
    registry.record_update(&id, "Noted — the gists too.");
    assert_eq!(
        registry.snapshot(&id).unwrap().updates,
        [
            update(0, 0, "I'll start with the profile."),
            update(1, 1, "Noted — the gists too."),
        ],
        "after how many calls, and after how many messages it read"
    );
}

#[test]
fn the_run_recorder_keeps_what_the_agent_said_before_more_work() {
    // The forwarder's bookkeeping (llm::backend::spawn_subagent_run): each
    // call it starts goes on the record, and each run of text more work
    // followed is a progress update — the very points that used to throw
    // the text away (docs/agents/agent-progress.md).
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    let start = |args: &str| StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: args.into(),
        detail: None,
        arguments: None,
    };
    let end = StreamEvent::ToolEnd {
        call_id: None,
        output: "{}".into(),
        ok: true,
        truncated: false,
        run: crate::stream::ToolRun::default(),
    };
    let mut recorder = RunRecorder::new(registry.clone(), id.clone());
    for event in [
        chunk("I'll start "),
        chunk("with the profile."),
        StreamEvent::ToolBatch(vec![ToolCallSummary {
            call_id: None,
            name: "Bash".into(),
            args: "curl one".into(),
        }]),
        start("curl one"),
        end.clone(),
        // No batch announced: the call's own start ends the text first.
        chunk("It lists 42 repos."),
        start("curl two"),
        end,
        // A message read mid-run, a stop hook's continuation: the text
        // before each was said on the way, and is not the answer.
        chunk("Halfway there."),
        StreamEvent::Steered {
            text: "also the gists".into(),
        },
        chunk("Done, I think."),
        StreamEvent::HookNote {
            label: "Stop hook".into(),
            text: "run the tests too".into(),
            report: None,
        },
        chunk("It has 42 repos."),
    ] {
        recorder.observe(&event);
    }
    let snapshot = registry.snapshot(&id).unwrap();
    assert_eq!(snapshot.calls, ["Bash(curl one)", "Bash(curl two)"]);
    assert_eq!(
        snapshot.updates,
        [
            update(0, 0, "I'll start with the profile."),
            update(1, 0, "It lists 42 repos."),
            update(2, 0, "Halfway there."),
            update(2, 0, "Done, I think."),
        ]
    );
    assert_eq!(
        recorder.into_answer(),
        "It has 42 repos.",
        "the run of text nothing followed is the answer, whole"
    );
}

#[test]
fn a_restarted_reply_hands_the_lead_only_the_attempt_that_got_through() {
    // docs/providers/offline.md *A connection lost mid-reply*: the forwarder had
    // been collecting the failed attempt's text as the agent's answer.
    // The retry voids it — neither glued to the reply that replaces it
    // nor kept as something the agent said on the way.
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    let mut recorder = RunRecorder::new(registry.clone(), id.clone());
    recorder.observe(&chunk("It has 4"));
    recorder.observe(&StreamEvent::Retrying { attempt: 1, max: 3 });
    recorder.observe(&chunk("It has 42 repos."));
    assert!(
        registry.snapshot(&id).unwrap().updates.is_empty(),
        "the withdrawn text was never said on the way"
    );
    assert_eq!(recorder.into_answer(), "It has 42 repos.");
}

/// A call's start, as the forwarder relays it.
fn bash_start(args: &str) -> StreamEvent {
    StreamEvent::ToolStart {
        call_id: None,
        name: "Bash".into(),
        args: args.into(),
        detail: None,
        arguments: None,
    }
}

#[test]
fn a_message_read_after_a_call_is_marked_after_it_however_far_behind_the_forwarder_runs() {
    // The agent's loop runs ahead of the forwarder relaying its stream:
    // by the time the forwarder relays the call, the loop has already
    // drained the message queued for the next round. The mark goes where
    // the stream announces the message, after the call and after what
    // the agent said before it — not wherever the forwarder happened to
    // be when the loop drained (docs/agents/agent-progress.md).
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    let mut recorder = RunRecorder::new(registry.clone(), id.clone());
    assert!(registry.queue_input(&id, "also the stars"));
    let read = registry.take_pending_inputs(&id);
    recorder.observe(&chunk("I'll start with the profile."));
    recorder.observe(&StreamEvent::ToolBatch(Vec::new()));
    recorder.observe(&bash_start("curl one"));
    for text in read {
        recorder.observe(&StreamEvent::Steered { text });
    }
    let report = output_report(&registry.snapshot(&id).unwrap());
    assert!(
        report
            .contains("Agent: I'll start with the profile.\nBash(curl one)\n— message received —"),
        "{report}"
    );
}

#[test]
fn each_drain_is_marked_where_its_own_announcement_lands() {
    // The loop can drain twice before the forwarder relays the first
    // drain's announcement: each drain's mark goes where its own
    // announcement lands, never both at the first.
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    let mut recorder = RunRecorder::new(registry.clone(), id.clone());
    assert!(registry.queue_input(&id, "first"));
    let first = registry.take_pending_inputs(&id);
    assert!(registry.queue_user_input(&id, "second"));
    assert!(registry.queue_user_input(&id, "third"));
    let second = registry.take_pending_inputs(&id);
    for text in first {
        recorder.observe(&StreamEvent::Steered { text });
    }
    recorder.observe(&bash_start("curl one"));
    for text in second {
        recorder.observe(&StreamEvent::Steered { text });
    }
    let report = output_report(&registry.snapshot(&id).unwrap());
    assert!(
        report.contains(
            "— message received —\nBash(curl one)\n\
                 — message from the user: second —\n— message from the user: third —"
        ),
        "{report}"
    );
}

#[test]
fn a_message_read_but_never_announced_is_marked_when_the_run_ends() {
    // A run cut short between the drain and the forwarder relaying it:
    // the agent did read the message, so its mark is placed late rather
    // than lost — and the run's answer is told the user's message it read.
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.record_call(&id, "Bash(curl one)");
    assert!(registry.queue_user_input(&id, "also the stars"));
    let _ = registry.take_pending_inputs(&id);
    assert_eq!(registry.user_messages(&id), ["also the stars"]);
    registry.finish(&id, Ok("done".into()), Vec::new());
    let report = output_report(&registry.snapshot(&id).unwrap());
    assert!(
        report.contains("Bash(curl one)\n— message from the user: also the stars —"),
        "{report}"
    );
    assert_eq!(registry.user_messages(&id), ["also the stars"]);
}

#[test]
fn a_long_progress_update_is_kept_up_to_the_cap() {
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.record_update(&id, &"x".repeat(AGENT_REPORT_UPDATE_CHARS + 50));
    let snapshot = registry.snapshot(&id).unwrap();
    let text = &snapshot.updates[0].text;
    assert_eq!(
        text.chars().count(),
        AGENT_REPORT_UPDATE_CHARS + 1,
        "the cap, then the mark"
    );
    assert!(text.ends_with('…'), "{text}");
}

#[test]
fn a_report_lists_each_update_where_the_agent_said_it() {
    // The request's picture (docs/agents/agent-progress.md): what the agent said
    // on its way, among its calls and the messages it read — an update
    // after the messages it had read, a multi-line one indented beneath
    // its first line, its blank lines dropped.
    let mut running = snapshot(AgentState::Running, &["Bash(curl one)", "Bash(curl two)"]);
    running.follow_ups = vec![FollowUp::lead(1)];
    running.updates = vec![
        update(0, 0, "I'll start with the profile."),
        update(
            1,
            0,
            "The profile lists 42 repos.\n\nFetching them next.\n  - newest first",
        ),
        update(1, 1, "Noted — the gists too."),
    ];
    assert_eq!(
        output_report(&running),
        [
            "Running (agent a7k2m9x4q · Fetch GitHub profile · 42s · 2 tool uses)",
            "Agent: I'll start with the profile.",
            "Bash(curl one)",
            "Agent: The profile lists 42 repos.",
            "  Fetching them next.",
            "    - newest first",
            AGENT_FOLLOW_UP_MARK,
            "Agent: Noted — the gists too.",
            "Bash(curl two)",
        ]
        .join("\n")
    );
    // An answer still closes it, after everything it said on the way.
    let mut done = running.clone();
    done.state = AgentState::Done("It has 42 repos.".into());
    done.updates.push(update(2, 1, "Both fetched."));
    let report = output_report(&done);
    assert!(
        report
            .ends_with("Bash(curl two)\nAgent: Both fetched.\n\nFinal response:\nIt has 42 repos."),
        "{report}"
    );
}

#[test]
fn updates_among_the_hidden_calls_are_hidden_with_them() {
    let calls: Vec<String> = (1..=AGENT_REPORT_CALLS_MAX + 1)
        .map(|n| format!("Read(file{n}.rs)"))
        .collect();
    let refs: Vec<&str> = calls.iter().map(String::as_str).collect();
    let mut long = snapshot(AgentState::Running, &refs);
    long.updates = vec![
        update(0, 0, "Before the first call."),
        update(1, 0, "After the first call."),
        update(AGENT_REPORT_CALLS_MAX + 1, 0, "After the last call."),
    ];
    let report = output_report(&long);
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines[1], "… 1 earlier tool call");
    assert_eq!(lines[2], "Agent: After the first call.");
    assert_eq!(lines[3], "Read(file2.rs)");
    assert_eq!(lines.last().copied(), Some("Agent: After the last call."));
    assert!(!report.contains("Before the first call."), "{report}");
}

#[test]
fn a_wait_is_handed_only_what_is_new_one_line_each_in_order() {
    // The waiting cell tails a few rows: what the agent did since the
    // last poll, a line each in the order it did it — an update cut like
    // a call, a message it read marked — and never anything twice.
    let registry = test_registry();
    let (id, _cancel) = registry.register_agent(GENERAL_PURPOSE, "d", "p");
    registry.record_update(&id, "I'll start with the profile.");
    registry.record_call(&id, "Bash(curl one)");
    // What it did before the wait is the report's.
    let mut cursor = ProgressCursor::at(&registry.snapshot(&id).unwrap());
    let quiet = registry.progress(&id, &mut cursor).unwrap();
    assert!(quiet.lines.is_empty(), "{:?}", quiet.lines);
    assert!(quiet.running);
    registry.record_update(&id, "Found it.\n\nNow the repositories.");
    registry.record_call(&id, "Bash(curl two\n  --silent)");
    assert!(registry.queue_input(&id, "also the stars"));
    let _ = read_queued(&registry, &id);
    registry.record_update(&id, &"y".repeat(400));
    let lines = registry.progress(&id, &mut cursor).unwrap().lines;
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(
        lines[..3],
        [
            "Agent: Found it. Now the repositories.",
            "Bash(curl two --silent)",
            AGENT_FOLLOW_UP_MARK,
        ]
    );
    assert!(lines[3].starts_with("Agent: yyy"), "{}", lines[3]);
    assert!(lines[3].ends_with('…'), "{}", lines[3]);
    assert!(
        lines[3].chars().count() <= AGENT_REPORT_CALL_CHARS + 1,
        "{}",
        lines[3]
    );
    assert!(
        registry
            .progress(&id, &mut cursor)
            .unwrap()
            .lines
            .is_empty(),
        "nothing new, nothing handed over"
    );
    registry.finish(&id, Ok("done".into()), Vec::new());
    assert!(!registry.progress(&id, &mut cursor).unwrap().running);
    assert!(registry.progress("a000nope0", &mut cursor).is_none());
}

#[test]
fn a_report_marks_where_each_follow_up_arrived() {
    let mut done = snapshot(
        AgentState::Done("zeroth.".into()),
        &["Bash(curl one)", "Bash(curl two)"],
    );
    done.follow_ups = vec![FollowUp::lead(1), FollowUp::lead(2)];
    let report = output_report(&done);
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(
        lines[1..5],
        [
            "Bash(curl one)",
            AGENT_FOLLOW_UP_MARK,
            "Bash(curl two)",
            AGENT_FOLLOW_UP_MARK
        ],
        "a follow-up that started no calls still shows: {report}"
    );
    // One past the cap: the marks before the kept calls are counted with
    // them, never shown out of place.
    let calls: Vec<String> = (1..=AGENT_REPORT_CALLS_MAX + 1)
        .map(|n| format!("Read(file{n}.rs)"))
        .collect();
    let refs: Vec<&str> = calls.iter().map(String::as_str).collect();
    let mut long = snapshot(AgentState::Running, &refs);
    long.follow_ups = vec![
        FollowUp::lead(0),
        FollowUp::lead(AGENT_REPORT_CALLS_MAX + 1),
    ];
    let report = output_report(&long);
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines[1], "… 1 earlier tool call");
    assert_eq!(lines[2], "Read(file2.rs)");
    assert_eq!(lines.last().copied(), Some(AGENT_FOLLOW_UP_MARK));
    assert_eq!(
        lines
            .iter()
            .filter(|line| **line == AGENT_FOLLOW_UP_MARK)
            .count(),
        1
    );
}

#[test]
fn report_call_lines_are_what_a_wait_streams() {
    // The waiting cell and the report show a call the same way: one line.
    assert_eq!(report_call("Bash(echo a\n  echo b)"), "Bash(echo a echo b)");
}

#[test]
fn an_output_report_frames_the_state_over_the_calls() {
    // The user's picture: the tool calls as one-liners, no outputs.
    let report = output_report(&snapshot(
        AgentState::Running,
        &[
            "Bash(curl -s https://api.github.com/users/devuser)",
            "Bash(curl -s https://api.github.com/users/devuser/events/public)",
        ],
    ));
    assert_eq!(
        report,
        "Running (agent a7k2m9x4q · Fetch GitHub profile · 42s · 2 tool uses)\n\
             Bash(curl -s https://api.github.com/users/devuser)\n\
             Bash(curl -s https://api.github.com/users/devuser/events/public)"
    );
    let quiet = output_report(&snapshot(AgentState::Running, &[]));
    assert_eq!(
        quiet,
        "Running (agent a7k2m9x4q · Fetch GitHub profile · 42s · 0 tool uses)\n\
             No tool calls yet."
    );
}

#[test]
fn a_settled_report_carries_its_answer() {
    let done = output_report(&snapshot(
        AgentState::Done("It has 12 repos.".into()),
        &["Bash(curl …)"],
    ));
    assert_eq!(
        done,
        "Done (agent a7k2m9x4q · Fetch GitHub profile · 42s · 1 tool use)\n\
             Bash(curl …)\n\
             \n\
             Final response:\n\
             It has 12 repos."
    );
    let failed = output_report(&snapshot(AgentState::Failed("rate limited".into()), &[]));
    assert!(failed.starts_with("Failed (agent a7k2m9x4q"), "{failed}");
    assert!(failed.ends_with("Error: rate limited"), "{failed}");
}

#[test]
fn a_stopped_report_says_who_stopped_it_and_whether_it_resumes() {
    let by_lead = output_report(&snapshot(AgentState::Stopped { by_user: false }, &[]));
    assert!(by_lead.starts_with("Stopped (agent a7k2m9x4q"), "{by_lead}");
    assert!(by_lead.contains("agentsend resumes it"), "{by_lead}");
    let by_user = output_report(&snapshot(AgentState::Stopped { by_user: true }, &[]));
    assert!(by_user.contains("by the user"), "{by_user}");
    assert!(!by_user.contains("agentsend resumes"), "{by_user}");
}

#[test]
fn a_long_history_keeps_the_newest_calls_and_counts_the_rest() {
    let calls: Vec<String> = (1..=AGENT_REPORT_CALLS_MAX + 5)
        .map(|n| format!("Read(file{n}.rs)"))
        .collect();
    let refs: Vec<&str> = calls.iter().map(String::as_str).collect();
    let report = output_report(&snapshot(AgentState::Running, &refs));
    let lines: Vec<&str> = report.lines().collect();
    assert_eq!(lines[1], "… 5 earlier tool calls");
    assert_eq!(lines[2], "Read(file6.rs)", "the newest are kept");
    assert_eq!(
        lines.last().copied(),
        Some(format!("Read(file{}.rs)", AGENT_REPORT_CALLS_MAX + 5).as_str())
    );
    assert_eq!(lines.len(), 2 + AGENT_REPORT_CALLS_MAX);
}

#[test]
fn a_call_is_reported_on_one_line_cut_at_the_cap() {
    let long = format!("Bash(echo {}\nls)", "x".repeat(400));
    let report = output_report(&snapshot(AgentState::Running, &[long.as_str()]));
    let line = report.lines().nth(1).expect("the call");
    assert!(line.starts_with("Bash(echo xxx"), "{line}");
    assert!(line.ends_with('…'), "{line}");
    assert!(
        line.chars().count() <= AGENT_REPORT_CALL_CHARS + 1,
        "{line}"
    );
}

#[test]
fn the_list_report_names_every_agent_and_its_state() {
    let mut stopped = snapshot(AgentState::Stopped { by_user: true }, &["Read(a)"]);
    stopped.id = "a9z8y7x6w".into();
    stopped.description = "Fix the tests".into();
    let mut done = snapshot(AgentState::Done("ok".into()), &["Read(a)", "Read(b)"]);
    done.id = "a1b2c3d4e".into();
    done.agent_type = "explore".into();
    done.description = "Search the repo".into();
    done.runtime = Duration::from_secs(65);
    let report = list_report(&[
        snapshot(AgentState::Running, &["Bash(curl …)"]),
        done,
        stopped,
    ]);
    assert_eq!(
        report,
        "3 agents:\n\
             - a7k2m9x4q (general-purpose) Fetch GitHub profile — running 42s · 1 tool use\n\
             - a1b2c3d4e (explore) Search the repo — done in 1m 5s · 2 tool uses\n\
             - a9z8y7x6w (general-purpose) Fix the tests — stopped by the user after 42s · 1 tool use"
    );
    assert_eq!(list_report(&[]), "No agents in this session.");
}
