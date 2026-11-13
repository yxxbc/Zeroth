//! The rename contract — `docs/architecture/naming.md`.
//!
//! Zeroth is a fork of Alter Zero. This file pins the names the program uses
//! for *itself*, so the rename cannot silently drift back. It says nothing
//! about upstream history: the published npm package, the fork note, `NOTICE`
//! and the one-time home migration are deliberately left naming the old app
//! (the design page lists every exemption).
//!
//! The old spellings below are built with `concat!` so a mechanical rename of
//! the tree cannot rewrite the very test that checks it.

use std::path::{Path, PathBuf};

use zeroth::scratchpad::{is_session_scratchpad, scratchpad_dir, session_root};

const OLD_ENV_PREFIX: &str = concat!("ALTER", "_ZERO");
const OLD_TEMP_PREFIX: &str = concat!("alter", "-zero-");

#[test]
fn the_session_root_is_named_for_this_app() {
    assert_eq!(zeroth::APP_NAME, "Zeroth");
    assert_eq!(
        session_root(Path::new("/tmp"), 1000, "s1"),
        PathBuf::from("/tmp/zeroth-1000/s1")
    );
    assert_eq!(
        scratchpad_dir(&session_root(Path::new("/tmp"), 1000, "s1")),
        PathBuf::from("/tmp/zeroth-1000/s1/scratchpad")
    );
}

#[test]
fn an_old_session_scratchpad_is_no_longer_recognised() {
    // No compatibility shim — "Breaking changes" in the design page. A
    // resumed conversation's old files are ordinary files again.
    assert!(is_session_scratchpad(Path::new(
        "/tmp/zeroth-1000/s1/scratchpad/notes.md"
    )));
    let old = format!("/tmp/{OLD_TEMP_PREFIX}1000/s1/scratchpad/notes.md");
    assert!(!is_session_scratchpad(Path::new(&old)), "{old}");
}

#[test]
fn the_public_environment_overrides_are_renamed() {
    assert_eq!(zeroth::update::UPDATE_ENV, "ZEROTH_UPDATE_CHECK");
    assert_eq!(zeroth::update::REPO_URL_ENV, "ZEROTH_UPDATE_URL");
    assert_eq!(zeroth::telemetry::TELEMETRY_ENV, "ZEROTH_TELEMETRY");
    assert_eq!(zeroth::herdr::DISABLE_ENV, "ZEROTH_HERDR");
    assert_eq!(zeroth::links::HYPERLINKS_ENV, "ZEROTH_HYPERLINKS");
    assert_eq!(zeroth::tips::TIPS_ENV, "ZEROTH_TIPS");
}

#[test]
fn no_old_environment_prefix_remains_in_the_source_tree() {
    // 73 overrides once carried the fork's prefix. One grep-style assertion
    // keeps a forgotten constant from reintroducing it.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    collect(&src, &mut offenders);
    assert!(
        offenders.is_empty(),
        "old environment prefix {OLD_ENV_PREFIX} still in: {offenders:#?}"
    );
}

fn collect(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if text.contains(OLD_ENV_PREFIX) {
                out.push(path.to_string_lossy().into_owned());
            }
        }
    }
}
