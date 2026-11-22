//! Command-line arguments — the pure core of `--help`, the `[PROMPT]`
//! shortcut, `--continue` and `--resume` (see `docs/cli/cli.md`).
//!
//! Claude-Code-style session flags: `--continue` reopens the newest
//! conversation recorded in the current directory, `--resume {id}` a
//! specific one, bare `--resume` boots into the `/resume` picker, a quoted
//! `[PROMPT]` is sent as the first turn of whichever session those open —
//! and a quit that recorded anything prints [`resume_hint`]'s copy-paste
//! command.
//!
//! This module owns the parse (`argv` → [`Cli`], with usage errors as
//! `Err(message)`), the `--help` page ([`help`], rendered plain or in
//! the app's colour by [`HelpStyle`]), the usage-error trailer
//! ([`usage_error`]), the colour rule ([`colour_enabled`]) and the exit
//! hint's exact shape. Everything impure — reading `std::env::args`,
//! asking whether stdout is a terminal, resolving a flag to a rollout
//! *path* (the sessions-dir scan), printing, exiting — stays at the
//! boundary (`tui::startup`), which runs the parse *after* the
//! detached-exec hook (invariant 1: a helper re-exec never parses TUI
//! flags).
//!
use std::num::NonZeroUsize;
use std::path::PathBuf;

use unicode_width::UnicodeWidthStr;

use crate::headless::OutputFormat;
use crate::permission::PermissionMode;

/// One parsed invocation: a TUI run ([`Cli::Session`] — the flagless
/// default included), one of the two print-and-exit flags, or the update subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cli {
    /// Run the TUI: how the session starts, plus the optional `[PROMPT]`
    /// sent as its first turn.
    Session(SessionArgs),
    /// `--help` / `-h`: print the main [`help`] page and exit.
    Help,
    /// `--version` / `-V`: print the version line and exit.
    Version,
    /// `update` as the first argument: install the newest release over this
    /// binary when one is out (`docs/ops/update.md`). Takes nothing.
    Update,
}

/// A session run's arguments (`docs/cli/cli.md`): which conversation to open,
/// the message to send into it first — `None` opens on an empty composer —
/// where to run, and whether to run with a screen at all (`-p`,
/// `docs/cli/headless.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionArgs {
    /// How the session starts.
    pub start: SessionStart,
    /// The `[PROMPT]` positional, verbatim: the first turn of a fresh
    /// session, or the next turn of the one `--continue`/`--resume {id}`
    /// reopens. Never blank (the parse refuses one), never combined with
    /// the bare picker (likewise). A headless run without one reads its
    /// prompt from stdin.
    pub prompt: Option<String>,
    /// `-C` / `--cwd`: the directory the run starts in — `chdir`ed to before
    /// anything reads the working directory, `--continue`'s search included.
    pub cwd: Option<PathBuf>,
    /// `--permission-mode`: the run's permission posture — the flag form of
    /// `ZEROTH_PERMISSION_MODE`, outranking it and the project's saved
    /// mode, and never saved (`docs/state/permissions.md`).
    pub permission_mode: Option<PermissionMode>,
    /// `-p` / `--print`: run headless with these options — no screen, the
    /// answer on stdout (`docs/cli/headless.md`). `None` is the TUI.
    pub print: Option<PrintOptions>,
    /// Which viewport the run asked for, if it said anything:
    /// `Some(true)` is `--fullscreen` (the screen-owning alternate-screen
    /// mode), `Some(false)` is `--inline` (the conversation in the terminal's
    /// scrollback), and `None` leaves it to `ZEROTH_TUI`, then to the default
    /// (`docs/ui/full-screen.md`). Three states rather than a bool because
    /// "not asked" is not "asked for inline".
    pub fullscreen: Option<bool>,
}

/// A headless run's own options (`docs/cli/headless.md`) — the flags that mean
/// nothing with a screen, so the parse refuses them without `-p`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrintOptions {
    /// `--output-format`: how the run reports.
    pub output_format: OutputFormat,
    /// `--max-turns`: the most model replies the lead agent may make over
    /// the whole run — Claude Code's agentic turns. `None` is no limit.
    pub max_turns: Option<NonZeroUsize>,
    /// `--verbose`: log each step of the run to stderr. Without it stderr
    /// says only what went wrong or needs attention.
    pub verbose: bool,
    /// `--jobs`: a JSONL file of jobs, each `{"id"?, "prompt": "..."}`, run
    /// as independent sessions in one process (`docs/cli/batch.md`).
    pub jobs: Option<PathBuf>,
    /// `--concurrency`: the most jobs in flight at once. `None` uses the
    /// batch runner's default.
    pub concurrency: Option<NonZeroUsize>,
    /// `--out-dir`: keep each job's full reporter output under this
    /// directory, one `<id>.log` per job.
    pub out_dir: Option<PathBuf>,
}

/// How a session run begins — the session flags, one-to-one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SessionStart {
    /// No session flag: a fresh conversation.
    #[default]
    Fresh,
    /// `--continue` / `-c`: resume the newest session recorded in this cwd.
    Continue,
    /// `--resume [id]` / `-r`: resume the named session — or, bare, open
    /// the `/resume` picker as the first screen.
    Resume(Option<String>),
}

// ===== the --help pages (docs/cli/cli.md) =====

/// The `--help` page's description: what running the command *does*, in
/// one line — both references' shape (codex's "If no subcommand is
/// specified, options will be forwarded to the interactive CLI", Claude
/// Code's "starts an interactive session by default, use -p/--print for
/// non-interactive output"). A help page is opened to find out how to
/// invoke something, so the line answers that and leaves what the app *is*
/// to the README; the sections below carry the rest. Under 80 columns like
/// every row on the page — nothing is wrapped at render time, so the page
/// reads in the source exactly as it prints.
pub const DESCRIPTION: &str = "\
Starts an interactive session by default; with -p, answers once and exits.";

/// The binary's `--help` page, rendered through [`help`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpPage {
    /// `zeroth --help`.
    Main,
}

/// How a page is dressed. `Plain` emits no escape sequence at all — what a
/// pipe, a log file and the tests receive; `Ansi` colours the title and the
/// section headings **bold cyan** (the app's accent hue in the terminal's
/// own palette — the banner's cyan, the pickers' selection colour), bolds
/// the literals (`zeroth`, `update`, the flags) and leaves the
/// placeholders (`[PROMPT]`, `[ID]`) bare, the way clap and
/// cargo dress theirs. The two renderings differ by escapes alone; the
/// boundary picks one per stream with [`colour_enabled`] and `IsTerminal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpStyle {
    Plain,
    Ansi,
}

impl HelpStyle {
    /// A section heading, or the page title.
    fn heading(self, text: &str) -> String {
        self.wrap("1;36", text)
    }

    /// A command word or a flag.
    fn literal(self, text: &str) -> String {
        self.wrap("1", text)
    }

    /// A usage error's `error:` lead.
    fn error(self, text: &str) -> String {
        self.wrap("1;31", text)
    }

    /// `ESC[{sgr}m{text}ESC[0m` under `Ansi`, the bare text under `Plain`.
    /// An empty text emits nothing either way — an empty escape pair is
    /// noise a stripped comparison would still catch.
    fn wrap(self, sgr: &str, text: &str) -> String {
        match self {
            Self::Ansi if !text.is_empty() => format!("\x1b[{sgr}m{text}\x1b[0m"),
            Self::Ansi | Self::Plain => text.to_string(),
        }
    }
}

/// Columns of indent before every section row.
const HELP_INDENT: usize = 2;
/// Columns between a section's widest literal cell and the descriptions.
const HELP_GAP: usize = 3;
/// The continuation indent under `Usage: ` — the label's own width, so
/// every further form aligns with the first.
const USAGE_CONTINUATION: &str = "       ";

/// One help page's content — rendered by [`help`], never printed as-is.
struct HelpDoc {
    /// The first line, in the heading style: the product's name
    /// (`Zeroth`, [`crate::APP_NAME`]).
    title: &'static str,
    /// The paragraph under it, plain and pre-wrapped.
    description: &'static str,
    /// The `Usage:` forms as `(literal, rest)` — the command words bolded,
    /// the placeholders after them bare.
    usage: &'static [(&'static str, &'static str)],
    /// The sections in print order.
    sections: &'static [Section],
}

/// A heading over aligned rows.
struct Section {
    heading: &'static str,
    rows: &'static [Row],
}

/// One `  {literal}{placeholder}   {description}` row. The description's
/// further lines continue at the description column.
struct Row {
    /// The bold part (`-r, --resume`, `update`) — empty for a bare positional.
    literal: &'static str,
    /// The bare part beside it (` [ID]`, `[PROMPT]`), leading space included.
    placeholder: &'static str,
    description: &'static str,
}

const MAIN_DOC: HelpDoc = HelpDoc {
    title: crate::APP_NAME,
    description: DESCRIPTION,
    usage: &[
        ("zeroth", " [OPTIONS] [PROMPT]"),
        ("zeroth -p", " [OPTIONS] [PROMPT]"),
        ("zeroth update", ""),
    ],
    sections: &[
        Section {
            heading: "Commands:",
            rows: &[Row {
                literal: "update",
                placeholder: "",
                description: "Install the newest release over this binary,\n\
                                  if one is out (the checksum-verified installer)",
            }],
        },
        Section {
            heading: "Arguments:",
            rows: &[Row {
                literal: "",
                placeholder: "[PROMPT]",
                description: "First message of a new conversation, or the\n\
                              next of the one --continue/--resume reopens",
            }],
        },
        Section {
            heading: "Options:",
            rows: &[
                Row {
                    literal: "-c, --continue",
                    placeholder: "",
                    description: "Continue the most recent conversation recorded\n\
                                  in this directory",
                },
                Row {
                    literal: "-r, --resume",
                    placeholder: " [ID]",
                    description: "Resume a conversation by the id the exit hint\n\
                                  prints — or pick it from a list, given no ID",
                },
                Row {
                    literal: "-C, --cwd",
                    placeholder: " <DIR>",
                    description: "Run in DIR instead of the current directory",
                },
                // A long-only flag lines its `--` up under the others'
                // (clap's four-column pad), so the pad rides the literal.
                Row {
                    literal: "    --permission-mode",
                    placeholder: " <MODE>",
                    description: "manual, edit, auto or master, for this run only",
                },
                Row {
                    literal: "    --fullscreen",
                    placeholder: "",
                    description: "Run the terminal UI on the alternate screen\n\
                                  (conversation laid out inside the window;\n\
                                  the default; ZEROTH_TUI=full)",
                },
                Row {
                    literal: "    --inline",
                    placeholder: "",
                    description: "Keep the conversation in the terminal's own\n\
                                  scrollback instead (ZEROTH_TUI=inline)",
                },
                Row {
                    literal: "-h, --help",
                    placeholder: "",
                    description: "Print help",
                },
                Row {
                    literal: "-V, --version",
                    placeholder: "",
                    description: "Print version",
                },
            ],
        },
        // The headless run's own flags (docs/cli/headless.md) — a section of
        // their own, since they mean nothing without -p.
        Section {
            heading: "Print mode:",
            rows: &[
                Row {
                    literal: "-p, --print",
                    placeholder: "",
                    description: "Answer PROMPT (or stdin) with no screen: the\n\
                                  answer on stdout, problems on stderr, then exit",
                },
                Row {
                    literal: "    --output-format",
                    placeholder: " <FORMAT>",
                    description: "text (the answer), json (one result object) or\n\
                                  stream-json (one event per line)",
                },
                Row {
                    literal: "    --max-turns",
                    placeholder: " <N>",
                    description: "Stop after N model replies",
                },
                Row {
                    literal: "    --verbose",
                    placeholder: "",
                    description: "Log each step of the run to stderr",
                },
                Row {
                    literal: "    --jobs",
                    placeholder: " <FILE>",
                    description: "Run FILE's JSONL jobs as concurrent sessions,\n\
                                  one result object per job on stdout",
                },
                Row {
                    literal: "    --concurrency",
                    placeholder: " <N>",
                    description: "At most N jobs at once (default 4)",
                },
                Row {
                    literal: "    --out-dir",
                    placeholder: " <DIR>",
                    description: "Keep each job's full log in DIR/<id>.log",
                },
            ],
        },
    ],
};

impl HelpPage {
    const fn doc(self) -> &'static HelpDoc {
        match self {
            Self::Main => &MAIN_DOC,
        }
    }
}

/// Render a `--help` page (`docs/cli/cli.md`): the title, the description, the
/// `Usage:` forms (each further one indented to the first), then each
/// section's heading over its rows — two columns of indent, the literal
/// cell padded to the page's widest, three more columns, the description,
/// its further lines continuing at that column. Padding is
/// measured on the plain text, so the columns line up under either style.
/// No trailing newline: the caller's `println!` supplies it.
#[must_use]
pub fn help(page: HelpPage, style: HelpStyle) -> String {
    let doc = page.doc();
    let mut out = style.heading(doc.title);
    out.push_str("\n\n");
    out.push_str(doc.description);
    out.push_str("\n\n");
    out.push_str(&usage_block(doc, style));
    // One description column for the whole page — the widest cell of ANY
    // section — so `update`, `[PROMPT]` and the options read as one table.
    let column = HELP_INDENT
        + HELP_GAP
        + doc
            .sections
            .iter()
            .flat_map(|section| section.rows)
            .map(|row| row.literal.width() + row.placeholder.width())
            .max()
            .unwrap_or(0);
    for section in doc.sections {
        out.push_str("\n\n");
        out.push_str(&style.heading(section.heading));
        for row in section.rows {
            let cell_width = HELP_INDENT + row.literal.width() + row.placeholder.width();
            for (i, line) in row.description.lines().enumerate() {
                out.push('\n');
                if i == 0 {
                    out.push_str(&" ".repeat(HELP_INDENT));
                    out.push_str(&style.literal(row.literal));
                    out.push_str(row.placeholder);
                    out.push_str(&" ".repeat(column - cell_width));
                } else {
                    out.push_str(&" ".repeat(column));
                }
                out.push_str(line);
            }
        }
    }
    out
}

/// The `Usage:` block alone — the page's forms, no title or sections.
fn usage_block(doc: &HelpDoc, style: HelpStyle) -> String {
    let mut out = String::new();
    for (i, (literal, rest)) in doc.usage.iter().enumerate() {
        if i == 0 {
            out.push_str(&style.heading("Usage:"));
            out.push(' ');
        } else {
            out.push('\n');
            out.push_str(USAGE_CONTINUATION);
        }
        out.push_str(&style.literal(literal));
        out.push_str(rest);
    }
    out
}

/// The trailer under a grammar error, clap's shape: `error: {message}`, the
/// page's `Usage:` block, and where the rest is. Printed to stderr with
/// exit 2 by the boundary; the `error:` lead is bold red under `Ansi`.
#[must_use]
pub fn usage_error(page: HelpPage, message: &str, style: HelpStyle) -> String {
    format!(
        "{} {message}\n\n{}\n\nFor more information, try '--help'.",
        style.error("error:"),
        usage_block(page.doc(), style)
    )
}

/// Whether a terminal may be coloured, given the two environment variables
/// that say otherwise: `NO_COLOR` set to anything non-empty
/// (<https://no-color.org>) or `TERM=dumb`. The boundary ANDs this with
/// whether the stream is actually a terminal — a pipe gets [`HelpStyle::Plain`]
/// without being asked.
#[must_use]
pub fn colour_enabled(no_color: Option<&str>, term: Option<&str>) -> bool {
    no_color.is_none_or(str::is_empty) && term != Some("dumb")
}

// ===== the argument grammar (docs/cli/cli.md) =====

/// Parse the process arguments (without `argv[0]`). Usage errors — unknown
/// flags, a second positional, a blank prompt, a value on `--continue`,
/// more than one session flag, a prompt with the bare picker, a flag given
/// twice, a headless-only flag without `-p` — come back as `Err(message)`;
/// the boundary prints [`usage_error`] to stderr and exits 2.
/// `--help`/`--version` win wherever they appear before a `--` (so
/// `--resume --help` prints help rather than eating `--help` as an id);
/// `--resume`'s id may be the next argument or `=`-attached, and a
/// following `-`-leading argument is never taken as an id. Every
/// value-taking flag (`--output-format`, `--max-turns`,
/// `--permission-mode`, `-C`/`--cwd`) reads its value the same way. The
/// one positional is the `[PROMPT]`, anywhere among the flags; `--` ends
/// the flags so a dash-leading prompt is reachable.
pub fn parse<I>(args: I) -> Result<Cli, String>
where
    I: IntoIterator<Item = String>,
{
    let mut start: Option<SessionStart> = None;
    let mut prompt: Option<String> = None;
    // The headless switch and its companions (docs/cli/headless.md), each
    // allowed once.
    let mut print: Option<()> = None;
    let mut output_format: Option<OutputFormat> = None;
    let mut max_turns: Option<NonZeroUsize> = None;
    let mut verbose: Option<()> = None;
    let mut jobs: Option<PathBuf> = None;
    let mut concurrency: Option<NonZeroUsize> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut permission_mode: Option<PermissionMode> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut fullscreen: Option<()> = None;
    let mut inline: Option<()> = None;
    let mut args = args.into_iter().peekable();
    // `update` routes as the first argument only (docs/ops/update.md) and
    // takes nothing: a flag or word after it is a mistake, not a prompt.
    if args.peek().is_some_and(|first| first == "update") {
        args.next();
        return match args.next() {
            None => Ok(Cli::Update),
            Some(extra) => Err(format!("update takes no arguments (got '{extra}')")),
        };
    }
    while let Some(arg) = args.next() {
        if arg == "--" {
            // The flags end here: what follows is the prompt — and exactly
            // one argument of it.
            for rest in args.by_ref() {
                take_prompt(&mut prompt, rest)?;
            }
            break;
        }
        if !is_flag(&arg) {
            take_prompt(&mut prompt, arg)?;
            continue;
        }
        // `--flag=value` splits here; a bare flag carries no value.
        let (flag, value) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
            None => (arg.clone(), None),
        };
        let picked = match flag.as_str() {
            "--help" | "-h" => return Ok(Cli::Help),
            "--version" | "-V" => return Ok(Cli::Version),
            "--continue" | "-c" => {
                if value.is_some() {
                    return Err(format!("{flag} takes no value"));
                }
                SessionStart::Continue
            }
            "--resume" | "-r" => {
                // The id is the attached value, else the next argument —
                // unless that argument is itself a flag. An explicitly
                // empty attached value (`--resume=`) is bare and never goes
                // hunting for the next argument. Otherwise greedy, like
                // Claude Code's: `--resume "fix the bug"` names a session,
                // not a prompt.
                let id = match value {
                    Some(id) if !id.is_empty() => Some(id),
                    Some(_) => None,
                    None => args.next_if(|next| !next.starts_with('-')),
                };
                SessionStart::Resume(id)
            }
            "--print" | "-p" => {
                if value.is_some() {
                    return Err(format!("{flag} takes no value"));
                }
                set_once(&mut print, (), "--print")?;
                continue;
            }
            "--verbose" => {
                if value.is_some() {
                    return Err(format!("{flag} takes no value"));
                }
                set_once(&mut verbose, (), "--verbose")?;
                continue;
            }
            "--fullscreen" => {
                if value.is_some() {
                    return Err(format!("{flag} takes no value"));
                }
                set_once(&mut fullscreen, (), "--fullscreen")?;
                continue;
            }
            // The inline viewport said out loud. It is the default today, so
            // the flag changes nothing yet — its job is to let a script or an
            // alias pin the mode that will stop being the default
            // (`docs/ui/full-screen.md` *Default decision*).
            "--inline" => {
                if value.is_some() {
                    return Err(format!("{flag} takes no value"));
                }
                set_once(&mut inline, (), "--inline")?;
                continue;
            }
            "--output-format" => {
                let raw = flag_value(&flag, value, &mut args)?;
                let format = OutputFormat::parse(&raw).ok_or_else(|| {
                    format!("invalid --output-format \"{raw}\" (use text, json or stream-json)")
                })?;
                set_once(&mut output_format, format, "--output-format")?;
                continue;
            }
            "--max-turns" => {
                let raw = flag_value(&flag, value, &mut args)?;
                let turns = raw
                    .parse::<usize>()
                    .ok()
                    .and_then(NonZeroUsize::new)
                    .ok_or_else(|| {
                        format!(
                            "--max-turns needs a number of model replies above 0, not \"{raw}\""
                        )
                    })?;
                set_once(&mut max_turns, turns, "--max-turns")?;
                continue;
            }
            "--jobs" => {
                let raw = flag_value(&flag, value, &mut args)?;
                set_once(&mut jobs, PathBuf::from(raw), "--jobs")?;
                continue;
            }
            "--concurrency" => {
                let raw = flag_value(&flag, value, &mut args)?;
                let workers = raw
                    .parse::<usize>()
                    .ok()
                    .and_then(NonZeroUsize::new)
                    .ok_or_else(|| {
                        format!("--concurrency needs a number of jobs above 0, not \"{raw}\"")
                    })?;
                set_once(&mut concurrency, workers, "--concurrency")?;
                continue;
            }
            "--out-dir" => {
                let raw = flag_value(&flag, value, &mut args)?;
                set_once(&mut out_dir, PathBuf::from(raw), "--out-dir")?;
                continue;
            }
            "--permission-mode" => {
                let raw = flag_value(&flag, value, &mut args)?;
                let mode = PermissionMode::parse(&raw).ok_or_else(|| {
                    format!(
                        "invalid --permission-mode \"{raw}\" (use manual, edit, auto or master)"
                    )
                })?;
                set_once(&mut permission_mode, mode, "--permission-mode")?;
                continue;
            }
            "--cwd" | "-C" => {
                let dir = flag_value(&flag, value, &mut args)?;
                set_once(&mut cwd, PathBuf::from(dir), "--cwd")?;
                continue;
            }
            _ => return Err(format!("unrecognized argument: {arg}")),
        };
        if start.replace(picked).is_some() {
            return Err("pass at most one of --continue / --resume".to_string());
        }
    }
    let start = start.unwrap_or(SessionStart::Fresh);
    // The headless run's own flags mean nothing with a screen
    // (docs/cli/headless.md): refused rather than silently ignored.
    if concurrency.is_some() && jobs.is_none() {
        return Err("--concurrency needs --jobs <FILE>".to_string());
    }
    if out_dir.is_some() && jobs.is_none() {
        return Err("--out-dir needs --jobs <FILE>".to_string());
    }
    // A batch reads its prompts from the file and starts every job fresh;
    // a positional prompt or a session to continue has no job to attach to.
    if jobs.is_some() {
        if prompt.is_some() {
            return Err("--jobs takes its prompts from the file, not [PROMPT]".to_string());
        }
        if start != SessionStart::Fresh {
            return Err(
                "--jobs starts fresh sessions: --continue / --resume can't be used".to_string(),
            );
        }
    }
    let print = match print {
        Some(()) => Some(PrintOptions {
            output_format: output_format.unwrap_or_default(),
            max_turns,
            verbose: verbose.is_some(),
            jobs,
            concurrency,
            out_dir,
        }),
        None => {
            if output_format.is_some() {
                return Err("--output-format needs --print (-p)".to_string());
            }
            if max_turns.is_some() {
                return Err("--max-turns needs --print (-p)".to_string());
            }
            if verbose.is_some() {
                return Err("--verbose needs --print (-p)".to_string());
            }
            if jobs.is_some() {
                return Err("--jobs needs --print (-p)".to_string());
            }
            if concurrency.is_some() {
                return Err("--concurrency needs --jobs <FILE>".to_string());
            }
            if out_dir.is_some() {
                return Err("--out-dir needs --jobs <FILE>".to_string());
            }
            None
        }
    };
    // The picker is a screen: a headless run has to name the session it
    // continues (docs/cli/headless.md).
    if print.is_some() && start == SessionStart::Resume(None) {
        return Err(
            "--print can't open the session picker: name the session with --resume <ID>, \
             or use --continue"
                .to_string(),
        );
    }
    // The picker is interactive and dismissable: a prompt parked behind it
    // would either start a fresh conversation on Esc or have to be dropped,
    // so the grammar says no up front (docs/cli/cli.md).
    if prompt.is_some() && start == SessionStart::Resume(None) {
        return Err(
            "--resume needs a session id to send a prompt into (or use --continue)".to_string(),
        );
    }
    // The screen-owning mode is a TUI, not a print run: the two are
    // contradictory, so the grammar says no rather than silently ignoring one.
    if fullscreen.is_some() && print.is_some() {
        return Err("--fullscreen needs the terminal UI (not --print)".to_string());
    }
    // Two viewports, one screen: asking for both is a mistake worth naming
    // rather than silently resolving in either direction.
    if fullscreen.is_some() && inline.is_some() {
        return Err("--fullscreen and --inline name two different viewports".to_string());
    }
    Ok(Cli::Session(SessionArgs {
        start,
        prompt,
        cwd,
        permission_mode,
        print,
        fullscreen: fullscreen.is_some().then_some(true).or_else(|| {
            // `--inline` is the other half of the same question; the two are
            // refused together above, so at most one of them is set.
            inline.is_some().then_some(false)
        }),
    }))
}

/// Fill a flag's slot, refusing a second time — clap's rule for a
/// single-value argument. `name` is the flag's long form, whichever spelling
/// was typed.
fn set_once<T>(slot: &mut Option<T>, value: T, name: &str) -> Result<(), String> {
    if slot.replace(value).is_some() {
        return Err(format!("{name} was given more than once"));
    }
    Ok(())
}

/// A flag is a dash plus something; a lone `-` is a positional.
fn is_flag(arg: &str) -> bool {
    arg.starts_with('-') && arg.len() > 1
}

/// Take `arg` as the `[PROMPT]`: the one positional, non-blank. A second
/// one is refused by name with the quoting hint — never joined, since
/// `zeroth write a -c program` would otherwise read `-c` as
/// `--continue` and lose a word of the prompt (Claude Code errors here too).
fn take_prompt(slot: &mut Option<String>, arg: String) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!(
            "unexpected argument: {arg} (quote the prompt so the shell passes it as one argument)"
        ));
    }
    if arg.trim().is_empty() {
        return Err("the prompt is empty".to_string());
    }
    *slot = Some(arg);
    Ok(())
}

/// A value-taking flag's value: the `=`-attached one, else the
/// next argument — unless that argument leads with `-`. Empty is missing
/// (`--url=` must not go hunting for the next token).
fn flag_value<I>(
    flag: &str,
    value: Option<String>,
    args: &mut std::iter::Peekable<I>,
) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    let value = match value {
        Some(value) => value,
        None => args
            .next_if(|next| !next.starts_with('-'))
            .unwrap_or_default(),
    };
    if value.is_empty() {
        return Err(format!("{flag} needs a value"));
    }
    Ok(value)
}

/// The exit hint printed below the restored terminal after a quit that
/// recorded a conversation (`docs/cli/cli.md`) — exactly two lines, no indent:
///
/// ```text
/// Resume this session with:
/// zeroth --resume 18a9f2c33d41e5b6-1a2b
/// ```
#[must_use]
pub fn resume_hint(bin: &str, session_id: &str) -> String {
    format!("Resume this session with:\n{bin} --resume {session_id}")
}

/// What a launch of the interface prints when it cannot start: no terminal
/// to open (cron, CI, a pipe), or one that never answered the cursor query
/// (`docs/cli/headless.md`). The cause verbatim, then print mode, which needs no
/// terminal — named, never switched to: a run in which nobody can approve
/// anything is the caller's choice to make.
///
/// ```text
/// error: could not start the interface: No such device or address (os error 6)
/// It needs a terminal; to run without one (cron, CI, a script), use print mode:
/// zeroth -p "your prompt"
/// ```
#[must_use]
pub fn no_terminal_error(bin: &str, cause: &str) -> String {
    format!(
        "error: could not start the interface: {cause}\n\
         It needs a terminal; to run without one (cron, CI, a script), use print mode:\n\
         {bin} -p \"your prompt\""
    )
}

/// The program name the hint prints — how the user actually invoked us
/// (`argv[0]`'s basename, so a renamed or symlinked install echoes the name
/// that was actually run), falling back to the canonical `zeroth` when
/// `argv[0]` is absent or empty.
#[must_use]
pub fn bin_name(arg0: Option<&str>) -> String {
    arg0.map(std::path::Path::new)
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .filter(|name| !name.is_empty())
        .map_or_else(|| "zeroth".to_string(), str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `parse` over string literals.
    fn parsed(args: &[&str]) -> Result<Cli, String> {
        parse(args.iter().map(ToString::to_string))
    }

    // ===== parse (docs/cli/cli.md) =====

    /// A TUI run: how it starts, plus its optional `[PROMPT]`.
    fn session(start: SessionStart, prompt: Option<&str>) -> Result<Cli, String> {
        Ok(Cli::Session(SessionArgs {
            start,
            prompt: prompt.map(str::to_string),
            ..SessionArgs::default()
        }))
    }

    #[test]
    fn no_args_is_a_plain_run() {
        assert_eq!(parsed(&[]), session(SessionStart::Fresh, None));
    }

    #[test]
    fn continue_parses_long_and_short() {
        assert_eq!(
            parsed(&["--continue"]),
            session(SessionStart::Continue, None)
        );
        assert_eq!(parsed(&["-c"]), session(SessionStart::Continue, None));
    }

    #[test]
    fn bare_resume_parses_to_the_picker() {
        let picker = session(SessionStart::Resume(None), None);
        assert_eq!(parsed(&["--resume"]), picker);
        assert_eq!(parsed(&["-r"]), picker);
        // An empty attached id is bare too — `--resume=` must not go
        // hunting for a session whose id is "".
        assert_eq!(parsed(&["--resume="]), picker);
    }

    #[test]
    fn resume_takes_an_id_separate_attached_or_short() {
        let want = session(SessionStart::Resume(Some("18a9f2c3-1a2b".into())), None);
        assert_eq!(parsed(&["--resume", "18a9f2c3-1a2b"]), want);
        assert_eq!(parsed(&["--resume=18a9f2c3-1a2b"]), want);
        assert_eq!(parsed(&["-r", "18a9f2c3-1a2b"]), want);
    }

    #[test]
    fn a_following_flag_is_not_eaten_as_a_resume_id() {
        // `--resume --help` asks for help about resume, not for a session
        // literally named "--help".
        assert_eq!(parsed(&["--resume", "--help"]), Ok(Cli::Help));
    }

    #[test]
    fn help_and_version_win_wherever_they_appear() {
        assert_eq!(parsed(&["--help"]), Ok(Cli::Help));
        assert_eq!(parsed(&["-h"]), Ok(Cli::Help));
        assert_eq!(parsed(&["--version"]), Ok(Cli::Version));
        assert_eq!(parsed(&["-V"]), Ok(Cli::Version));
        assert_eq!(parsed(&["--continue", "--help"]), Ok(Cli::Help));
        assert_eq!(parsed(&["a prompt", "--version"]), Ok(Cli::Version));
    }

    #[test]
    fn unknown_flags_are_usage_errors_naming_the_culprit() {
        let err = parsed(&["--frobnicate"]).expect_err("unknown flag");
        assert!(err.contains("--frobnicate"), "{err}");
        let err = parsed(&["-x"]).expect_err("unknown short flag");
        assert!(err.contains("-x"), "{err}");
    }

    #[test]
    fn continue_takes_no_value() {
        let err = parsed(&["--continue=now"]).expect_err("a value on --continue");
        assert!(err.contains("--continue"), "{err}");
    }

    #[test]
    fn session_flags_do_not_combine() {
        assert!(parsed(&["--continue", "--resume"]).is_err());
        assert!(parsed(&["-r", "abc", "-c"]).is_err());
        assert!(parsed(&["-c", "-c"]).is_err());
    }

    // ===== the [PROMPT] shortcut (docs/cli/cli.md) =====

    #[test]
    fn a_positional_is_the_prompt_of_a_fresh_session() {
        assert_eq!(
            parsed(&["fix the failing test"]),
            session(SessionStart::Fresh, Some("fix the failing test"))
        );
        // Verbatim: a leading `!` or `/` is message text here, not the
        // composer's shell or slash grammar.
        assert_eq!(
            parsed(&["/init please"]),
            session(SessionStart::Fresh, Some("/init please"))
        );
    }

    #[test]
    fn mcp_is_an_ordinary_positional_prompt_not_a_subcommand() {
        for args in [&["mcp"][..], &["--", "mcp"]] {
            assert_eq!(parsed(args), session(SessionStart::Fresh, Some("mcp")));
        }
        assert_eq!(
            parsed(&["--continue", "mcp"]),
            session(SessionStart::Continue, Some("mcp"))
        );
        assert_eq!(parsed(&["mcp", "--help"]), Ok(Cli::Help));
        for command in ["add", "add-json", "remove", "get", "list"] {
            assert_eq!(
                parsed(&["mcp", command]),
                Err(format!(
                    "unexpected argument: {command} (quote the prompt so the shell passes it as one argument)"
                ))
            );
        }
        assert!(!help(HelpPage::Main, HelpStyle::Plain).contains("mcp"));
    }

    #[test]
    fn the_prompt_rides_with_continue_and_resume_before_or_after_the_flag() {
        let id = || SessionStart::Resume(Some("18a9f2c3-1a2b".into()));
        assert_eq!(
            parsed(&["-c", "and the docs"]),
            session(SessionStart::Continue, Some("and the docs"))
        );
        assert_eq!(
            parsed(&["and the docs", "--continue"]),
            session(SessionStart::Continue, Some("and the docs"))
        );
        assert_eq!(
            parsed(&["--resume", "18a9f2c3-1a2b", "and the docs"]),
            session(id(), Some("and the docs"))
        );
        assert_eq!(
            parsed(&["and the docs", "-r", "18a9f2c3-1a2b"]),
            session(id(), Some("and the docs"))
        );
        assert_eq!(
            parsed(&["--resume=18a9f2c3-1a2b", "and the docs"]),
            session(id(), Some("and the docs"))
        );
    }

    #[test]
    fn double_dash_ends_the_flags_so_a_dash_leading_prompt_is_reachable() {
        assert_eq!(
            parsed(&["--", "-v is not a flag here"]),
            session(SessionStart::Fresh, Some("-v is not a flag here"))
        );
        assert_eq!(
            parsed(&["-c", "--", "--continue"]),
            session(SessionStart::Continue, Some("--continue"))
        );
        // Even `--help` is prose after the separator.
        assert_eq!(
            parsed(&["--", "--help"]),
            session(SessionStart::Fresh, Some("--help"))
        );
    }

    #[test]
    fn a_second_positional_is_a_usage_error_with_the_quote_hint() {
        // Never joined: `zeroth write a -c program` would otherwise
        // read `-c` as --continue and lose a word of the prompt.
        let err = parsed(&["fix", "the bug"]).expect_err("two positionals");
        assert!(err.contains("the bug") && err.contains("quote"), "{err}");
        let err = parsed(&["--", "a", "b"]).expect_err("two after --");
        assert!(err.contains('b') && err.contains("quote"), "{err}");
        let err = parsed(&["a", "-c", "b"]).expect_err("one each side of a flag");
        assert!(err.contains("unexpected argument: b"), "{err}");
        let err = parsed(&["--resume", "id", "extra", "more"]).expect_err("trailing junk");
        assert!(err.contains("more"), "{err}");
    }

    #[test]
    fn a_blank_prompt_is_a_usage_error() {
        let err = parsed(&[""]).expect_err("empty");
        assert!(err.contains("empty"), "{err}");
        let err = parsed(&["   "]).expect_err("blank");
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn a_prompt_with_the_bare_picker_is_refused() {
        let err = parsed(&["fix it", "--resume"]).expect_err("picker + prompt");
        assert!(
            err.contains("--resume") && err.contains("session id") && err.contains("--continue"),
            "{err}"
        );
        let err = parsed(&["--resume=", "fix it"]).expect_err("picker + prompt");
        assert!(err.contains("--resume"), "{err}");
    }

    #[test]
    fn resume_still_takes_the_next_argument_as_its_id_greedily() {
        // Claude Code's rule: `--resume "fix the bug"` looks "fix the bug"
        // up as an id (and fails at resolution) rather than guessing it was
        // a prompt — the prompt for a reopened session names the session
        // first.
        assert_eq!(
            parsed(&["--resume", "fix the bug"]),
            session(SessionStart::Resume(Some("fix the bug".into())), None)
        );
    }

    // ===== headless mode (docs/cli/headless.md) =====

    /// A run with these arguments.
    fn run(args: SessionArgs) -> Result<Cli, String> {
        Ok(Cli::Session(args))
    }

    /// A headless run of `prompt` with `options`.
    fn printed(prompt: Option<&str>, options: PrintOptions) -> Result<Cli, String> {
        run(SessionArgs {
            prompt: prompt.map(str::to_string),
            print: Some(options),
            ..SessionArgs::default()
        })
    }

    #[test]
    fn print_runs_the_prompt_headless_in_text_by_default() {
        let want = printed(Some("Reply with OK"), PrintOptions::default());
        assert_eq!(parsed(&["-p", "Reply with OK"]), want);
        assert_eq!(parsed(&["--print", "Reply with OK"]), want);
        // A switch, not a value-taker: the prompt may come first too.
        assert_eq!(parsed(&["Reply with OK", "-p"]), want);
        assert_eq!(
            PrintOptions::default().output_format,
            OutputFormat::Text,
            "text is the default format"
        );
    }

    #[test]
    fn print_without_a_prompt_leaves_it_to_stdin() {
        assert_eq!(parsed(&["-p"]), printed(None, PrintOptions::default()));
        // A lone `-` is a positional the boundary reads as "stdin".
        assert_eq!(
            parsed(&["-p", "-"]),
            printed(Some("-"), PrintOptions::default())
        );
    }

    #[test]
    fn print_takes_no_value() {
        let err = parsed(&["--print=yes", "x"]).expect_err("a value on --print");
        assert!(err.contains("--print") && err.contains("no value"), "{err}");
    }

    #[test]
    fn inline_is_the_opt_out_and_the_two_viewports_cannot_be_asked_for_at_once() {
        // Neither flag: **nothing asked**, which is not the same as "inline" —
        // the default lives in `term::DEFAULT_VIEWPORT`, and main resolves the
        // three states in order (flag, environment, default).
        let Ok(Cli::Session(args)) = parsed(&["x"]) else {
            panic!("a session run parses");
        };
        assert_eq!(args.fullscreen, None, "a bare run asks for no viewport");

        // The explicit opt-out parses and means what it says.
        let Ok(Cli::Session(args)) = parsed(&["--inline", "x"]) else {
            panic!(
                "--inline parses as a session run: {:?}",
                parsed(&["--inline", "x"]).err()
            );
        };
        assert_eq!(
            args.fullscreen,
            Some(false),
            "inline is not the screen mode"
        );

        // Two viewports, one screen: named, not silently resolved.
        let err = parsed(&["--fullscreen", "--inline", "x"]).expect_err("contradictory");
        assert!(
            err.contains("--fullscreen") && err.contains("--inline"),
            "{err}"
        );
        // …and each still refuses a value and a repeat.
        assert!(parsed(&["--inline=1", "x"]).is_err());
        assert!(parsed(&["--inline", "--inline", "x"]).is_err());
    }

    #[test]
    fn fullscreen_is_a_tui_flag_and_refuses_a_screenless_run() {
        let Ok(Cli::Session(args)) = parsed(&["--fullscreen", "x"]) else {
            panic!("--fullscreen parses as a session run");
        };
        assert_eq!(
            args.fullscreen,
            Some(true),
            "the flag reaches the session args"
        );
        assert_eq!(args.prompt.as_deref(), Some("x"));
        assert!(matches!(
            parsed(&["--fullscreen"]),
            Ok(Cli::Session(SessionArgs {
                fullscreen: Some(true),
                ..
            }))
        ));
        let err = parsed(&["-p", "--fullscreen", "x"]).expect_err("headless has no screen");
        assert!(
            err.contains("--fullscreen") && err.contains("--print"),
            "{err}"
        );
        let err = parsed(&["--fullscreen=1"]).expect_err("a value on --fullscreen");
        assert!(
            err.contains("--fullscreen") && err.contains("no value"),
            "{err}"
        );
        let err = parsed(&["--fullscreen", "--fullscreen"]).expect_err("twice");
        assert!(err.contains("more than once"), "{err}");
    }

    #[test]
    fn verbose_turns_on_the_step_log_of_a_print_run() {
        let verbose = printed(
            Some("x"),
            PrintOptions {
                verbose: true,
                ..PrintOptions::default()
            },
        );
        assert_eq!(parsed(&["-p", "--verbose", "x"]), verbose);
        assert_eq!(parsed(&["x", "--verbose", "-p"]), verbose);
        // Quiet unless asked.
        assert_eq!(
            parsed(&["-p", "x"]),
            printed(Some("x"), PrintOptions::default())
        );
        let err = parsed(&["--verbose=yes", "-p", "x"]).expect_err("a value");
        assert!(
            err.contains("--verbose") && err.contains("no value"),
            "{err}"
        );
        let err = parsed(&["-p", "--verbose", "--verbose", "x"]).expect_err("twice");
        assert!(err.contains("more than once"), "{err}");
    }

    #[test]
    fn jobs_take_their_prompts_from_a_file_and_a_bounded_pool() {
        let want = printed(
            None,
            PrintOptions {
                jobs: Some(PathBuf::from("jobs.jsonl")),
                concurrency: NonZeroUsize::new(8),
                out_dir: Some(PathBuf::from("logs")),
                ..PrintOptions::default()
            },
        );
        assert_eq!(
            parsed(&[
                "-p",
                "--jobs",
                "jobs.jsonl",
                "--concurrency",
                "8",
                "--out-dir",
                "logs"
            ]),
            want
        );
        assert_eq!(
            parsed(&[
                "-p",
                "--jobs=jobs.jsonl",
                "--concurrency=8",
                "--out-dir=logs"
            ]),
            want
        );

        let err = parsed(&["--jobs", "jobs.jsonl"]).expect_err("needs -p");
        assert!(err.contains("--jobs needs --print"), "{err}");
        let err = parsed(&["-p", "--concurrency", "2"]).expect_err("needs jobs");
        assert!(err.contains("--concurrency needs --jobs"), "{err}");
        let err = parsed(&["-p", "--out-dir", "logs"]).expect_err("needs jobs");
        assert!(err.contains("--out-dir needs --jobs"), "{err}");
        let err = parsed(&["-p", "--jobs", "j", "prompt"]).expect_err("no positional prompt");
        assert!(err.contains("takes its prompts from the file"), "{err}");
        let err = parsed(&["-p", "--jobs", "j", "--continue"]).expect_err("no session");
        assert!(err.contains("fresh sessions"), "{err}");
        let err = parsed(&["-p", "--jobs", "j", "--concurrency", "0"]).expect_err("zero");
        assert!(err.contains("above 0"), "{err}");
    }

    #[test]
    fn output_format_takes_every_format_separate_or_attached() {
        for format in OutputFormat::ALL {
            let want = printed(
                Some("x"),
                PrintOptions {
                    output_format: format,
                    ..PrintOptions::default()
                },
            );
            assert_eq!(
                parsed(&["-p", "--output-format", format.label(), "x"]),
                want
            );
            let attached = format!("--output-format={}", format.label());
            assert_eq!(parsed(&["x", &attached, "-p"]), want);
        }
    }

    #[test]
    fn an_unknown_output_format_names_the_choices() {
        let err = parsed(&["-p", "--output-format", "xml", "x"]).expect_err("xml");
        assert!(
            err.contains("\"xml\"") && err.contains("text, json or stream-json"),
            "{err}"
        );
    }

    #[test]
    fn a_value_flag_with_no_value_is_a_usage_error() {
        for args in [
            &["-p", "--output-format"][..],
            &["-p", "--output-format="],
            &["-p", "--output-format", "--max-turns", "3"],
            &["-p", "--max-turns"],
            &["--permission-mode"],
            &["-C"],
            &["--cwd="],
        ] {
            let err = parsed(args).expect_err("a missing value");
            assert!(err.contains("needs a value"), "{args:?}: {err}");
        }
    }

    #[test]
    fn max_turns_takes_a_positive_count() {
        let want = |n: usize| {
            printed(
                Some("x"),
                PrintOptions {
                    max_turns: NonZeroUsize::new(n),
                    ..PrintOptions::default()
                },
            )
        };
        assert_eq!(parsed(&["-p", "--max-turns", "5", "x"]), want(5));
        assert_eq!(parsed(&["-p", "--max-turns=12", "x"]), want(12));
    }

    #[test]
    fn max_turns_refuses_zero_and_anything_but_a_count() {
        for value in ["0", "many", "2.5", "-3"] {
            let attached = format!("--max-turns={value}");
            let err = parsed(&["-p", &attached, "x"]).expect_err("not a count");
            assert!(
                err.contains("--max-turns") && err.contains(value),
                "{value}: {err}"
            );
        }
    }

    #[test]
    fn the_print_only_flags_need_print() {
        for args in [
            &["--output-format", "json", "x"][..],
            &["--max-turns", "3", "x"],
            &["-c", "--output-format=stream-json"],
            &["--verbose", "x"],
        ] {
            let err = parsed(args).expect_err("a headless flag without -p");
            assert!(err.contains("needs --print"), "{args:?}: {err}");
        }
    }

    #[test]
    fn permission_mode_seeds_either_kind_of_run() {
        for mode in [
            PermissionMode::Manual,
            PermissionMode::Edit,
            PermissionMode::Auto,
            PermissionMode::Master,
        ] {
            assert_eq!(
                parsed(&["--permission-mode", mode.label()]),
                run(SessionArgs {
                    permission_mode: Some(mode),
                    ..SessionArgs::default()
                })
            );
            let attached = format!("--permission-mode={}", mode.label());
            assert_eq!(
                parsed(&["-p", &attached, "x"]),
                run(SessionArgs {
                    prompt: Some("x".into()),
                    permission_mode: Some(mode),
                    print: Some(PrintOptions::default()),
                    ..SessionArgs::default()
                })
            );
        }
    }

    #[test]
    fn an_unknown_permission_mode_names_the_choices() {
        let err = parsed(&["--permission-mode", "yolo"]).expect_err("yolo");
        assert!(
            err.contains("\"yolo\"") && err.contains("manual, edit, auto or master"),
            "{err}"
        );
    }

    #[test]
    fn cwd_takes_a_directory_short_long_or_attached_in_either_run() {
        let want = |print: Option<PrintOptions>| {
            run(SessionArgs {
                cwd: Some(PathBuf::from("/srv/repo")),
                print,
                ..SessionArgs::default()
            })
        };
        assert_eq!(parsed(&["-C", "/srv/repo"]), want(None));
        assert_eq!(parsed(&["--cwd", "/srv/repo"]), want(None));
        assert_eq!(parsed(&["--cwd=/srv/repo"]), want(None));
        assert_eq!(
            parsed(&["-p", "-C", "/srv/repo"]),
            want(Some(PrintOptions::default()))
        );
        // `-C` is the directory; `-c` is still --continue.
        assert_eq!(
            parsed(&["-c", "-C", "/srv/repo"]),
            run(SessionArgs {
                start: SessionStart::Continue,
                cwd: Some(PathBuf::from("/srv/repo")),
                ..SessionArgs::default()
            })
        );
    }

    #[test]
    fn a_flag_given_twice_is_a_usage_error() {
        for args in [
            &["-p", "-p", "x"][..],
            &["-p", "--print", "x"],
            &["-C", "a", "--cwd", "b"],
            &["--permission-mode", "auto", "--permission-mode=master"],
            &["-p", "--output-format", "json", "--output-format", "text"],
            &["-p", "--max-turns", "1", "--max-turns=2"],
        ] {
            let err = parsed(args).expect_err("twice");
            assert!(err.contains("more than once"), "{args:?}: {err}");
        }
    }

    #[test]
    fn print_with_the_bare_picker_is_refused() {
        for args in [&["-p", "--resume"][..], &["-r", "-p"], &["--resume=", "-p"]] {
            let err = parsed(args).expect_err("the picker headless");
            assert!(
                err.contains("--resume") && err.contains("--continue"),
                "{args:?}: {err}"
            );
        }
    }

    #[test]
    fn print_sends_the_prompt_into_a_continued_or_resumed_session() {
        assert_eq!(
            parsed(&["-p", "-c", "and the docs"]),
            run(SessionArgs {
                start: SessionStart::Continue,
                prompt: Some("and the docs".into()),
                print: Some(PrintOptions::default()),
                ..SessionArgs::default()
            })
        );
        assert_eq!(
            parsed(&["--resume", "18a9f2c3-1a2b", "-p"]),
            run(SessionArgs {
                start: SessionStart::Resume(Some("18a9f2c3-1a2b".into())),
                print: Some(PrintOptions::default()),
                ..SessionArgs::default()
            })
        );
    }

    // ===== the exit hint (docs/cli/cli.md) =====

    #[test]
    fn resume_hint_is_the_two_line_command() {
        assert_eq!(
            resume_hint("zeroth", "18a9f2c3-1a2b"),
            "Resume this session with:\nzeroth --resume 18a9f2c3-1a2b",
        );
    }

    #[test]
    fn no_terminal_error_names_the_cause_and_the_mode_that_needs_none() {
        // The interactive launch with no terminal behind it (cron, CI, a
        // pipe) used to end on main's Debug print of the io::Error. The
        // cause stays verbatim, then the way to run without a terminal, its
        // command alone on the last line like the resume hint's.
        assert_eq!(
            no_terminal_error("az", "No such device or address (os error 6)"),
            "error: could not start the interface: No such device or address (os error 6)\n\
             It needs a terminal; to run without one (cron, CI, a script), use print mode:\n\
             az -p \"your prompt\"",
        );
    }

    #[test]
    fn bin_name_basenames_argv0_and_falls_back() {
        assert_eq!(bin_name(Some("target/debug/zeroth")), "zeroth");
        assert_eq!(bin_name(Some("/usr/local/bin/zeroth")), "zeroth");
        assert_eq!(bin_name(Some("az")), "az");
        assert_eq!(bin_name(Some("")), "zeroth");
        assert_eq!(bin_name(None), "zeroth");
    }

    // ===== the --help page (docs/cli/cli.md) =====

    /// The plain main page — exactly what a pipe receives, and what the
    /// design doc shows.
    const MAIN_PAGE: &str = "\
Zeroth

Starts an interactive session by default; with -p, answers once and exits.

Usage: zeroth [OPTIONS] [PROMPT]
       zeroth -p [OPTIONS] [PROMPT]
       zeroth update

Commands:
  update                         Install the newest release over this binary,
                                 if one is out (the checksum-verified installer)

Arguments:
  [PROMPT]                       First message of a new conversation, or the
                                 next of the one --continue/--resume reopens

Options:
  -c, --continue                 Continue the most recent conversation recorded
                                 in this directory
  -r, --resume [ID]              Resume a conversation by the id the exit hint
                                 prints — or pick it from a list, given no ID
  -C, --cwd <DIR>                Run in DIR instead of the current directory
      --permission-mode <MODE>   manual, edit, auto or master, for this run only
      --fullscreen               Run the terminal UI on the alternate screen
                                 (conversation laid out inside the window;
                                 the default; ZEROTH_TUI=full)
      --inline                   Keep the conversation in the terminal's own
                                 scrollback instead (ZEROTH_TUI=inline)
  -h, --help                     Print help
  -V, --version                  Print version

Print mode:
  -p, --print                    Answer PROMPT (or stdin) with no screen: the
                                 answer on stdout, problems on stderr, then exit
      --output-format <FORMAT>   text (the answer), json (one result object) or
                                 stream-json (one event per line)
      --max-turns <N>            Stop after N model replies
      --verbose                  Log each step of the run to stderr
      --jobs <FILE>              Run FILE's JSONL jobs as concurrent sessions,
                                 one result object per job on stdout
      --concurrency <N>          At most N jobs at once (default 4)
      --out-dir <DIR>            Keep each job's full log in DIR/<id>.log";

    /// Strip every `ESC[…m` sequence.
    fn strip_ansi(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find("\x1b[") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let end = after.find('m').expect("a closed SGR sequence");
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn the_plain_main_page_is_the_documented_one() {
        assert_eq!(help(HelpPage::Main, HelpStyle::Plain), MAIN_PAGE);
    }

    #[test]
    fn the_main_page_opens_on_the_product_name() {
        // `Zeroth` — the one constant the app speaks its name from —
        // not the binary's `zeroth`, which the Usage line names.
        let page = help(HelpPage::Main, HelpStyle::Plain);
        assert_eq!(page.lines().next(), Some(crate::APP_NAME));
        assert!(page.contains("\nUsage: zeroth "), "{page}");
    }

    #[test]
    fn main_help_fits_eighty_columns_with_no_trailing_blanks() {
        use unicode_width::UnicodeWidthStr;
        for line in help(HelpPage::Main, HelpStyle::Plain).lines() {
            assert!(line.width() <= 80, "{line:?}");
            assert_eq!(line.trim_end(), line, "{line:?}");
        }
    }

    #[test]
    fn ansi_help_strips_back_to_the_plain_page() {
        let styled = help(HelpPage::Main, HelpStyle::Ansi);
        let plain = help(HelpPage::Main, HelpStyle::Plain);
        assert_ne!(styled, plain, "the main page is dressed");
        assert_eq!(strip_ansi(&styled), plain);
    }

    #[test]
    fn ansi_help_dresses_the_title_headings_and_literals() {
        let styled = help(HelpPage::Main, HelpStyle::Ansi);
        let heading = |text: &str| format!("\x1b[1;36m{text}\x1b[0m");
        // The title wears the heading style — the same colour as Usage /
        // Commands / Options.
        assert!(styled.starts_with(&heading("Zeroth")), "{styled}");
        for h in ["Usage:", "Commands:", "Arguments:", "Options:"] {
            assert!(
                styled.contains(&heading(h)),
                "{h} styled as a heading:\n{styled}"
            );
        }
        // Literals are bold; placeholders wear nothing.
        assert!(
            styled.contains("\x1b[1mzeroth\x1b[0m [OPTIONS] [PROMPT]"),
            "{styled}"
        );
        assert!(styled.contains("\x1b[1mzeroth update\x1b[0m"), "{styled}");
        assert!(
            styled.contains("\x1b[1m-r, --resume\x1b[0m [ID]              Resume"),
            "{styled}"
        );
        assert!(
            styled.contains("\n  [PROMPT]                       First"),
            "{styled}"
        );
        // The headless section is a section like any other, and a long-only
        // flag's pad rides inside its literal.
        assert!(styled.contains("\x1b[1;36mPrint mode:\x1b[0m"), "{styled}");
        assert!(
            styled.contains("\x1b[1mzeroth -p\x1b[0m [OPTIONS] [PROMPT]"),
            "{styled}"
        );
        assert!(
            styled.contains("\x1b[1m    --max-turns\x1b[0m <N>"),
            "{styled}"
        );
        assert!(!styled.contains("\x1b[1m[PROMPT]"), "{styled}");
        // An empty literal never emits an empty escape pair.
        assert!(!styled.contains("\x1b[1m\x1b[0m"), "{styled}");
    }

    #[test]
    fn a_usage_error_is_the_error_line_the_usage_block_and_the_help_pointer() {
        let text = usage_error(
            HelpPage::Main,
            "unrecognized argument: --frob",
            HelpStyle::Plain,
        );
        assert_eq!(
            text,
            "error: unrecognized argument: --frob\n\n\
             Usage: zeroth [OPTIONS] [PROMPT]\n       zeroth -p [OPTIONS] [PROMPT]\n       \
             zeroth update\n\n\
             For more information, try '--help'."
        );
        let styled = usage_error(HelpPage::Main, "x", HelpStyle::Ansi);
        assert!(
            styled.starts_with("\x1b[1;31merror:\x1b[0m x\n"),
            "{styled}"
        );
        assert_eq!(
            strip_ansi(&styled),
            usage_error(HelpPage::Main, "x", HelpStyle::Plain)
        );
    }

    #[test]
    fn colour_is_on_unless_no_color_is_set_or_the_terminal_is_dumb() {
        assert!(colour_enabled(None, Some("xterm-256color")));
        assert!(colour_enabled(None, None));
        // https://no-color.org: only a NON-empty NO_COLOR disables.
        assert!(colour_enabled(Some(""), Some("xterm")));
        assert!(!colour_enabled(Some("1"), Some("xterm")));
        assert!(!colour_enabled(None, Some("dumb")));
    }

    // ===== the update subcommand (docs/ops/update.md) =====

    #[test]
    fn update_routes_as_the_first_argument_only_and_takes_nothing() {
        assert_eq!(parsed(&["update"]), Ok(Cli::Update));
        let err = parsed(&["update", "now"]).unwrap_err();
        assert!(err.contains("update takes no arguments"), "{err}");
        let err = parsed(&["update", "--help"]).unwrap_err();
        assert!(err.contains("update takes no arguments"), "{err}");
        // Anywhere else the word is a prompt like any other.
        assert!(matches!(
            parsed(&["-c", "update"]),
            Ok(Cli::Session(SessionArgs { prompt: Some(p), .. })) if p == "update"
        ));
    }
}
