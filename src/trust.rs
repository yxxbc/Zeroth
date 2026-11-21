//! What the user has trusted to run (`docs/state/config-review.md`): the hooks
//! in their own and their projects' `hooks.json`.
//!
//! A hook runs a shell command whenever its event fires, outside the permission
//! prompt, so it waits until the user has reviewed it. This is the pure half:
//! hook identities, their classification, and the `trust.json` store. File I/O
//! lives in the binary boundary (`tui::trust`). Approvals are keyed by the
//! absolute source-file path and kept in the config home, never the project.
//!
//! Malformed active records fail closed and are never written over
//! ([`update_store`]). The obsolete `mcp` key is ignored on read and omitted on
//! write, without changing hook records.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::hooks::{HookEvent, HooksFile};

/// The `sha256:<hex>` fingerprint of `bytes`, used for legacy hook approvals.
#[must_use]
pub fn fingerprint(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(7 + digest.len() * 2);
    out.push_str("sha256:");
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Is `root` a directory the project layer should read config from at all?
///
/// The **home directory is never a project** (the `checkpoint::cwd_scope`
/// instinct): a cwd with no `.git` ancestor falls back to itself as the
/// root, and launched in `~` that makes `{root}/.zeroth` the user's own
/// config home — the layer would rediscover the user's `hooks.json` as
/// "project config". The boundary skips the layer instead (the
/// per-file identity guard in `tui::trust` catches the same collision when
/// `ZEROTH_CONFIG_DIR` moves the config home somewhere else).
#[must_use]
pub fn is_project_root(root: &Path, home: Option<&Path>) -> bool {
    home != Some(root)
}

/// The directory a hook knows as the project — `$ZEROTH_PROJECT_DIR`,
/// and Claude Code's `$CLAUDE_PROJECT_DIR` — given the session's `cwd` and
/// the nearest `.git` ancestor-or-self `found` there: that root when it is
/// one ([`is_project_root`]), else the `cwd`. The root the project layer
/// reads `.zeroth/` from, so a project's `hooks.json` reaching its own
/// scripts as `${ZEROTH_PROJECT_DIR}/.zeroth/hooks/…` finds them
/// from any subdirectory (`docs/tools/hooks.md`).
#[must_use]
pub fn hooks_project_dir(cwd: &Path, found: Option<&Path>, home: Option<&Path>) -> PathBuf {
    match found {
        Some(root) if is_project_root(root, home) => root.to_path_buf(),
        _ => cwd.to_path_buf(),
    }
}

/// The project's hooks file: `{root}/.zeroth/hooks.json`.
#[must_use]
pub fn project_hooks_file(root: &Path) -> PathBuf {
    root.join(".zeroth").join("hooks.json")
}

/// What a hook **is**, for trusting it: the event that fires it, the matcher
/// that selects it, the command it runs, and its resolved timeout. A timeout
/// changes how long the hook can guard a call before failing open, so editing
/// it requires review. `statusMessage` is display-only. The matcher is
/// normalised ([`normalize_matcher`]), so the three spellings of
/// match-everything are one hook — and nothing else is folded together, a
/// padded matcher included.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HookIdentity {
    pub event: String,
    /// Empty for a group that matches everything.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub matcher: String,
    pub command: String,
    /// Effective seconds before the runner times out and fails open. Old trust
    /// records omit this field and retain the historical default.
    #[serde(
        default = "default_hook_timeout_secs",
        skip_serializing_if = "is_default_hook_timeout"
    )]
    pub timeout_secs: u64,
}

fn default_hook_timeout_secs() -> u64 {
    crate::hooks::CommandHook::DEFAULT_TIMEOUT_SECS
}

fn is_default_hook_timeout(timeout_secs: &u64) -> bool {
    *timeout_secs == crate::hooks::CommandHook::DEFAULT_TIMEOUT_SECS
}

impl HookIdentity {
    /// The review's item title: `PreToolUse · bash`, or `Stop` for a hook
    /// that matches everything.
    #[must_use]
    pub fn label(&self) -> String {
        let label = if self.matcher.is_empty() {
            self.event.clone()
        } else if self.matcher.trim() != self.matcher {
            // Padded with spaces — a different hook from the bare name, so
            // the spaces must show.
            format!("{} · \"{}\"", self.event, self.matcher)
        } else {
            format!("{} · {}", self.event, self.matcher)
        };
        if is_default_hook_timeout(&self.timeout_secs) {
            label
        } else {
            format!("{label} · timeout {}s", self.timeout_secs)
        }
    }
}

/// A group's matcher as a hook's identity carries it: exactly as the runner
/// reads it ([`crate::hooks::constraint`] — never trimmed, since ` Bash` and
/// `Bash` select different tools), the match-everything spellings — absent,
/// empty, `*` — all the empty string.
#[must_use]
pub fn normalize_matcher(raw: Option<&str>) -> String {
    crate::hooks::constraint(raw)
        .unwrap_or_default()
        .to_string()
}

/// The hook identity of `handler` under `event`'s `group`, when it is a hook
/// that can run there — a modelled event and a handler the runner would run
/// ([`crate::hooks::HookHandler::runnable_command`]). Anything else is
/// skipped whatever the trust says, so it is never asked about.
#[must_use]
pub fn hook_identity(
    event: &str,
    group: &crate::hooks::MatcherGroup,
    handler: &crate::hooks::HookHandler,
) -> Option<HookIdentity> {
    HookEvent::from_name(event)?;
    Some(HookIdentity {
        event: event.to_string(),
        matcher: normalize_matcher(group.matcher.as_deref()),
        command: handler.runnable_command()?,
        timeout_secs: handler.runnable_timeout_secs()?,
    })
}

/// Every hook in `file` that can run, in the hook registry's event order
/// (the `/hooks` browser's), each once.
#[must_use]
pub fn runnable_hooks(file: &HooksFile) -> Vec<HookIdentity> {
    let mut out: Vec<HookIdentity> = Vec::new();
    for event in HookEvent::ALL {
        let Some(groups) = file.hooks.get(event.name()) else {
            continue;
        };
        for group in groups {
            for handler in &group.hooks {
                if let Some(identity) = hook_identity(event.name(), group, handler)
                    && !out.contains(&identity)
                {
                    out.push(identity);
                }
            }
        }
    }
    out
}

/// `file` keeping only the hooks `trusted` answers yes for — what the
/// runner is given. A handler that cannot run (another type, an unmodelled
/// event) is kept as it was: it never runs either way, and the runner's
/// skip warning about it still says why.
#[must_use]
pub fn trusted_only(file: &HooksFile, trusted: &dyn Fn(&HookIdentity) -> bool) -> HooksFile {
    let mut out = file.clone();
    for (event, groups) in &mut out.hooks {
        for group in groups.iter_mut() {
            let matcher_group = group.clone();
            group.hooks.retain(|handler| {
                hook_identity(event, &matcher_group, handler)
                    .is_none_or(|identity| trusted(&identity))
            });
        }
    }
    out
}

/// Where a hook stands with the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookTrust {
    /// Listed for its file: it runs.
    Trusted,
    /// Never seen: it waits for the review.
    New,
    /// An edit of a trusted hook — a listed hook under the same event and
    /// matcher that the file no longer has. `previous` is that hook's
    /// command, the review's `was:` line.
    Changed { previous: String },
}

/// One runnable hook and where it stands — [`TrustFile::review_hooks`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookReview {
    pub identity: HookIdentity,
    pub trust: HookTrust,
}

/// The parsed `trust.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustFile {
    /// hooks file → the hooks trusted in it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    hooks: BTreeMap<String, Vec<HookIdentity>>,
    /// Accepted only for compatibility: obsolete server decisions cannot
    /// affect hook trust and are discarded without decoding their contents.
    #[serde(
        default,
        rename = "mcp",
        skip_serializing,
        deserialize_with = "ignore_obsolete_mcp"
    )]
    obsolete_mcp: (),
    /// The `/trust` era's approvals not carried over yet: project root →
    /// file → the fingerprint of the bytes it approved. Kept through every
    /// write for the projects not opened since, and dropped file by file as
    /// each is carried over ([`TrustFile::migrate_legacy`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    projects: BTreeMap<String, LegacyProject>,
}

fn ignore_obsolete_mcp<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
    serde::de::IgnoredAny::deserialize(deserializer)?;
    Ok(())
}

/// One project's `/trust`-era approval: file → the fingerprint of the bytes
/// it approved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyProject {
    #[serde(default)]
    files: BTreeMap<String, String>,
}

impl TrustFile {
    /// Parse a `trust.json` body. An empty/whitespace file is an empty store.
    ///
    /// # Errors
    /// The serde message when the text is not valid JSON of this exact shape
    /// — including unknown fields other than the obsolete `mcp` key, whose
    /// value is ignored. The caller fails closed
    /// (nothing trusted) and surfaces the error.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_str(text).map_err(|err| err.to_string())
    }

    /// The store as `trust.json` holds it, its last line ended.
    #[must_use]
    pub fn serialize(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Is `identity` trusted in the hooks file `source`?
    #[must_use]
    pub fn is_hook_trusted(&self, source: &str, identity: &HookIdentity) -> bool {
        self.hooks
            .get(source)
            .is_some_and(|trusted| trusted.contains(identity))
    }

    /// Every hook in `file` (read from `source`) that can run, with where it
    /// stands — in [`runnable_hooks`] order. A hook is trusted while exactly
    /// it is listed for the file, wherever it sits; an untrusted one is
    /// **changed** when a listed hook under the same event and matcher is
    /// gone from the file (each such hook pairs with one untrusted hook, in
    /// order), else **new**.
    #[must_use]
    pub fn review_hooks(&self, source: &str, file: &HooksFile) -> Vec<HookReview> {
        let recorded = self.hooks.get(source).map_or(&[][..], Vec::as_slice);
        let current = runnable_hooks(file);
        let mut gone: Vec<&HookIdentity> = recorded
            .iter()
            .filter(|hook| !current.contains(hook))
            .collect();
        current
            .into_iter()
            .map(|identity| {
                let trust = if recorded.contains(&identity) {
                    HookTrust::Trusted
                } else if let Some(at) = gone
                    .iter()
                    .position(|old| old.event == identity.event && old.matcher == identity.matcher)
                {
                    HookTrust::Changed {
                        previous: gone.remove(at).command.clone(),
                    }
                } else {
                    HookTrust::New
                };
                HookReview { identity, trust }
            })
            .collect()
    }

    /// Record a hooks decision for `source`, whose current parse is `file`:
    /// `trust` become trusted, `distrust` stop being trusted, and the file's
    /// record is rewritten to its current trusted hooks — so a hook removed
    /// from the file is dropped, and the same command added back later is
    /// asked about again. The old version of an edited hook that is still
    /// untrusted is kept, so the next review can still say what it was.
    /// Every other hooks file's record is left alone.
    pub fn record_hooks(
        &mut self,
        source: &str,
        file: &HooksFile,
        trust: &[HookIdentity],
        distrust: &[HookIdentity],
    ) {
        let reviews = self.review_hooks(source, file);
        let mut list: Vec<HookIdentity> = Vec::new();
        for review in &reviews {
            let identity = &review.identity;
            let trusted = (review.trust == HookTrust::Trusted || trust.contains(identity))
                && !distrust.contains(identity);
            if trusted && !list.contains(identity) {
                list.push(identity.clone());
            }
        }
        // Pair vanished records in exactly review order, consuming the old
        // record even when its replacement was accepted. Never reconstruct it
        // from the new identity: its timeout (and command) was not approved.
        let mut gone: Vec<HookIdentity> = self
            .hooks
            .get(source)
            .into_iter()
            .flatten()
            .filter(|old| !reviews.iter().any(|review| review.identity == **old))
            .cloned()
            .collect();
        for review in &reviews {
            if let HookTrust::Changed { .. } = &review.trust
                && let Some(at) = gone.iter().position(|old| {
                    old.event == review.identity.event && old.matcher == review.identity.matcher
                })
            {
                let old = gone.remove(at);
                if !list.contains(&review.identity) && !list.contains(&old) {
                    list.push(old);
                }
            }
        }
        if list.is_empty() {
            self.hooks.remove(source);
        } else {
            self.hooks.insert(source.to_string(), list);
        }
    }

    /// Carry the `/trust` era's approvals over for the project at `root`
    /// (`docs/state/config-review.md`). `/trust` approved a project's files whole,
    /// pinned to their bytes; a file it approved at exactly its current
    /// bytes keeps that approval here — its hooks recorded trusted — and its
    /// old record goes. A file edited since keeps its old record unread,
    /// since what `/trust` approved is not what is there now. `hooks` is the
    /// project's parsed file as `(absolute path, fingerprint of its bytes,
    /// parse)`. Returns whether anything was carried over.
    pub fn migrate_legacy(&mut self, root: &str, hooks: Option<(&str, &str, &HooksFile)>) -> bool {
        let approved = |store: &Self, path: &str, fingerprint: &str| {
            store
                .projects
                .get(root)
                .and_then(|project| project.files.get(path))
                .is_some_and(|recorded| recorded == fingerprint)
        };
        let mut carried = Vec::new();
        if let Some((path, fingerprint, file)) = hooks
            && approved(self, path, fingerprint)
        {
            self.record_hooks(path, file, &runnable_hooks(file), &[]);
            carried.push(path);
        }
        if let Some(project) = self.projects.get_mut(root) {
            for path in &carried {
                project.files.remove(*path);
            }
            if project.files.is_empty() {
                self.projects.remove(root);
            }
        }
        !carried.is_empty()
    }
}

/// Apply `change` to the store `existing` holds and return the text to
/// write — the read-modify-write every decision goes through, over the file
/// as it is **when the decision is made**, so two sessions answering at once
/// never erase each other's answers. An empty `existing` (no file yet) is an
/// empty store.
///
/// # Errors
/// The parse error when `existing` is not a store this module understands:
/// a malformed file is never written over — whatever the user decided before
/// is still in it, waiting for the typo to be fixed — so the caller keeps the
/// decision for the session only, and says so.
pub fn update_store(existing: &str, change: impl FnOnce(&mut TrustFile)) -> Result<String, String> {
    let mut store = TrustFile::parse(existing)?;
    change(&mut store);
    Ok(store.serialize())
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "/home/u/.zeroth/hooks.json";
    const PROJECT: &str = "/repo/.zeroth/hooks.json";

    fn hooks(text: &str) -> HooksFile {
        HooksFile::parse(text).expect("fixture parses")
    }

    fn hook(event: &str, matcher: &str, command: &str) -> HookIdentity {
        HookIdentity {
            event: event.to_string(),
            matcher: matcher.to_string(),
            command: command.to_string(),
            timeout_secs: crate::hooks::CommandHook::DEFAULT_TIMEOUT_SECS,
        }
    }

    /// A hooks decision recorded over the store `existing` holds.
    fn recorded_hooks(
        existing: &str,
        source: &str,
        file: &HooksFile,
        trust: &[HookIdentity],
        distrust: &[HookIdentity],
    ) -> String {
        update_store(existing, |store| {
            store.record_hooks(source, file, trust, distrust);
        })
        .expect("the store parses")
    }

    fn trust_of(reviews: &[HookReview]) -> Vec<(String, HookTrust)> {
        reviews
            .iter()
            .map(|review| (review.identity.command.clone(), review.trust.clone()))
            .collect()
    }

    const STOP_FMT: &str =
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"cargo fmt"}]}]}}"#;

    #[test]
    fn the_hooks_project_dir_is_the_root_the_project_layer_reads() {
        let home = Some(Path::new("/home/u"));
        // Launched in a subdirectory: the repository's root, where the
        // project's `.zeroth/hooks.json` and its scripts live.
        assert_eq!(
            hooks_project_dir(Path::new("/repo/src"), Some(Path::new("/repo")), home),
            PathBuf::from("/repo")
        );
        // No repository above: the directory it was launched in.
        assert_eq!(
            hooks_project_dir(Path::new("/tmp/scratch"), None, home),
            PathBuf::from("/tmp/scratch")
        );
        // A repository at home is never the project (`is_project_root`):
        // `{home}/.zeroth` is the user's own config home.
        assert_eq!(
            hooks_project_dir(
                Path::new("/home/u/code/x"),
                Some(Path::new("/home/u")),
                home
            ),
            PathBuf::from("/home/u/code/x")
        );
    }

    #[test]
    fn fingerprint_is_stable_and_content_sensitive() {
        assert_eq!(fingerprint(b"hello"), fingerprint(b"hello"));
        assert_ne!(fingerprint(b"hello"), fingerprint(b"hello!"));
        // A well-known vector, so the encoding can never silently change.
        assert_eq!(
            fingerprint(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn the_project_paths_hang_off_the_zeroth_dir() {
        let root = Path::new("/repo");
        assert_eq!(
            project_hooks_file(root),
            PathBuf::from("/repo/.zeroth/hooks.json")
        );
    }

    #[test]
    fn the_home_directory_is_never_a_project() {
        // Launching in `~` resolves the fallback root to the home dir, where
        // `{root}/.zeroth` IS the user's own config home.
        assert!(!is_project_root(
            Path::new("/home/me"),
            Some(Path::new("/home/me"))
        ));
        assert!(is_project_root(
            Path::new("/home/me/repo"),
            Some(Path::new("/home/me"))
        ));
        assert!(is_project_root(Path::new("/tmp/proj"), None));
    }

    #[test]
    fn runnable_hooks_lists_every_hook_that_can_run_in_registry_order_once() {
        let file = hooks(
            r#"{"hooks":{
                "SessionStart":[{"hooks":[{"type":"command","command":"./start.sh"}]}],
                "UserPromptSubmit":[
                    {"matcher":"*","hooks":[{"type":"command","command":"./a.sh"}]},
                    {"hooks":[
                        {"type":"command","command":"./a.sh"},
                        {"type":"prompt","command":"./never.sh"}
                    ]}
                ],
                "PreToolUse":[{"matcher":" bash|write ","hooks":[
                    {"type":"command","command":"./guard.sh"},
                    {"type":"command","command":"./scoped.sh","if":"Bash(git *)"}
                ]}],
                "FileChanged":[{"hooks":[{"type":"command","command":"./unmodelled.sh"}]}]
            }}"#,
        );
        assert_eq!(
            runnable_hooks(&file),
            vec![
                // The registry's order, not the file map's alphabetical one —
                // and the matcher as the runner reads it, untrimmed.
                hook("PreToolUse", " bash|write ", "./guard.sh"),
                // `*`, empty and absent are one match-everything matcher, so
                // the same command under both groups is one hook.
                hook("UserPromptSubmit", "", "./a.sh"),
                hook("SessionStart", "", "./start.sh"),
            ]
        );
    }

    #[test]
    fn a_hook_identity_labels_its_event_and_matcher() {
        assert_eq!(hook("PreToolUse", "bash", "x").label(), "PreToolUse · bash");
        assert_eq!(hook("Stop", "", "x").label(), "Stop");
        // A matcher padded with spaces is quoted, or ` Bash` and `Bash` —
        // two different hooks — would read the same.
        assert_eq!(
            hook("PreToolUse", " Bash", "x").label(),
            r#"PreToolUse · " Bash""#
        );
    }

    #[test]
    fn a_matcher_is_kept_exactly_as_the_runner_reads_it() {
        // The runner never trims a matcher (`hooks::constraint`): ` Bash` is
        // a regex no tool name matches, `Bash` fires on every bash call. So
        // trusting the first must not trust the second.
        let quiet = hooks(
            r#"{"hooks":{"PreToolUse":[{"matcher":" Bash","hooks":[{"type":"command","command":"./guard.sh"}]}]}}"#,
        );
        let loud = hooks(
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"./guard.sh"}]}]}}"#,
        );
        let text = recorded_hooks("", PROJECT, &quiet, &runnable_hooks(&quiet), &[]);
        let store = TrustFile::parse(&text).expect("recorded store parses");
        assert_eq!(
            trust_of(&store.review_hooks(PROJECT, &loud)),
            vec![("./guard.sh".to_string(), HookTrust::New)]
        );
        // …and a lone space is a pattern, not the match-everything matcher.
        let space = hooks(
            r#"{"hooks":{"Stop":[{"matcher":" ","hooks":[{"type":"command","command":"./x.sh"}]}]}}"#,
        );
        assert_eq!(runnable_hooks(&space), vec![hook("Stop", " ", "./x.sh")]);
    }

    #[test]
    fn an_empty_store_trusts_no_hook_and_calls_each_one_new() {
        let store = TrustFile::parse("").expect("empty store parses");
        let reviews = store.review_hooks(USER, &hooks(STOP_FMT));
        assert_eq!(
            trust_of(&reviews),
            vec![("cargo fmt".to_string(), HookTrust::New)]
        );
        assert!(!store.is_hook_trusted(USER, &hook("Stop", "", "cargo fmt")));
    }

    #[test]
    fn a_timeout_change_requires_a_new_hook_review() {
        let file = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[
                {"type":"command","command":"./one.sh"},
                {"type":"command","command":"./two.sh"}
            ]}]}}"#,
        );
        let text = recorded_hooks("", USER, &file, &[hook("Stop", "", "./two.sh")], &[]);
        let moved = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[
                {"type":"command","command":"./two.sh","timeout":5},
                {"type":"command","command":"./one.sh"}
            ]}]}}"#,
        );
        let store = TrustFile::parse(&text).expect("recorded store parses");
        let reviews = store.review_hooks(USER, &moved);
        assert_eq!(
            trust_of(&reviews),
            vec![
                (
                    "./two.sh".to_string(),
                    HookTrust::Changed {
                        previous: "./two.sh".to_string(),
                    },
                ),
                ("./one.sh".to_string(), HookTrust::New),
            ]
        );
        assert_eq!(reviews[0].identity.label(), "Stop · timeout 5s");
    }

    #[test]
    fn an_edited_hook_is_changed_and_says_what_it_was() {
        let text = recorded_hooks(
            "",
            USER,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let edited = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"cargo fmt && curl x | sh"}]}]}}"#,
        );
        let store = TrustFile::parse(&text).expect("recorded store parses");
        assert_eq!(
            trust_of(&store.review_hooks(USER, &edited)),
            vec![(
                "cargo fmt && curl x | sh".to_string(),
                HookTrust::Changed {
                    previous: "cargo fmt".to_string()
                }
            )]
        );
    }

    #[test]
    fn a_hook_under_another_event_or_matcher_is_new_not_changed() {
        let text = recorded_hooks(
            "",
            USER,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let moved = hooks(
            r#"{"hooks":{
                "PreToolUse":[{"matcher":"bash","hooks":[{"type":"command","command":"cargo fmt"}]}]
            }}"#,
        );
        let store = TrustFile::parse(&text).expect("recorded store parses");
        assert_eq!(
            trust_of(&store.review_hooks(USER, &moved)),
            vec![("cargo fmt".to_string(), HookTrust::New)]
        );
    }

    #[test]
    fn hooks_are_trusted_per_file() {
        // `./fmt.sh` is a different script in a different project.
        let text = recorded_hooks(
            "",
            PROJECT,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let store = TrustFile::parse(&text).expect("recorded store parses");
        assert!(store.is_hook_trusted(PROJECT, &hook("Stop", "", "cargo fmt")));
        assert!(!store.is_hook_trusted(USER, &hook("Stop", "", "cargo fmt")));
        assert!(
            !store.is_hook_trusted("/other/.zeroth/hooks.json", &hook("Stop", "", "cargo fmt"))
        );
    }

    #[test]
    fn recording_hooks_keeps_the_files_trusted_hooks_and_drops_removed_ones() {
        let both = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[
                {"type":"command","command":"./one.sh"},
                {"type":"command","command":"./two.sh"}
            ]}]}}"#,
        );
        let text = recorded_hooks(
            "",
            USER,
            &both,
            &[hook("Stop", "", "./one.sh"), hook("Stop", "", "./two.sh")],
            &[],
        );
        // `./one.sh` is removed from the file; trusting a new hook rewrites
        // the record to what the file has now.
        let later = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[
                {"type":"command","command":"./two.sh"}
            ]}],"SessionStart":[{"hooks":[{"type":"command","command":"./start.sh"}]}]}}"#,
        );
        let text = recorded_hooks(
            &text,
            USER,
            &later,
            &[hook("SessionStart", "", "./start.sh")],
            &[],
        );
        let store = TrustFile::parse(&text).expect("recorded store parses");
        assert!(store.is_hook_trusted(USER, &hook("Stop", "", "./two.sh")));
        assert!(store.is_hook_trusted(USER, &hook("SessionStart", "", "./start.sh")));
        // The removed hook dropped out: added back, it is asked about again.
        assert!(!store.is_hook_trusted(USER, &hook("Stop", "", "./one.sh")));
    }

    #[test]
    fn an_edited_hook_keeps_saying_what_it_was_until_the_new_one_is_trusted() {
        let text = recorded_hooks(
            "",
            USER,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let edited = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"cargo fmt --all"}]}]},
               "description":"x"}"#,
        );
        // Another decision for the file that leaves the edit untrusted keeps
        // the old version, so the next review still says what changed…
        let text = recorded_hooks(&text, USER, &edited, &[], &[]);
        let store = TrustFile::parse(&text).expect("recorded store parses");
        assert_eq!(
            store.review_hooks(USER, &edited)[0].trust,
            HookTrust::Changed {
                previous: "cargo fmt".to_string()
            }
        );
        // …and trusting the edit lets the old version go.
        let text = recorded_hooks(
            &text,
            USER,
            &edited,
            &[hook("Stop", "", "cargo fmt --all")],
            &[],
        );
        let store = TrustFile::parse(&text).expect("recorded store parses");
        assert_eq!(
            store.review_hooks(USER, &edited)[0].trust,
            HookTrust::Trusted
        );
        assert!(!store.is_hook_trusted(USER, &hook("Stop", "", "cargo fmt")));
    }

    #[test]
    fn declining_a_timeout_change_keeps_the_actual_old_identity() {
        let original = hooks(STOP_FMT);
        let old = runnable_hooks(&original)[0].clone();
        let edited = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[
                {"type":"command","command":"cargo fmt","timeout":5},
                {"type":"command","command":"echo reviewed"}
            ]}]}}"#,
        );
        let changed = runnable_hooks(&edited)[0].clone();
        let other = runnable_hooks(&edited)[1].clone();
        assert_ne!(old, changed);
        let mut store = TrustFile::default();
        store.record_hooks(USER, &original, std::slice::from_ref(&old), &[]);
        // Recording another decision in the same file must not approve the
        // timeout edit that the user left unchecked (or explicitly declined).
        for declined in [Vec::new(), vec![changed.clone()]] {
            store.record_hooks(USER, &edited, std::slice::from_ref(&other), &declined);
            let store = TrustFile::parse(&store.serialize()).unwrap();
            assert!(store.is_hook_trusted(USER, &old));
            assert!(!store.is_hook_trusted(USER, &changed));
            assert_eq!(
                store.review_hooks(USER, &edited)[0].trust,
                HookTrust::Changed {
                    previous: old.command.clone()
                }
            );
            assert_eq!(
                runnable_hooks(&trusted_only(&edited, &|identity| {
                    store.is_hook_trusted(USER, identity)
                })),
                vec![other.clone()]
            );
        }
        store.record_hooks(USER, &edited, std::slice::from_ref(&changed), &[]);
        assert!(store.is_hook_trusted(USER, &changed));
        assert!(!store.is_hook_trusted(USER, &old));
    }

    #[test]
    fn accepting_one_timeout_edit_keeps_the_other_edits_own_old_identity() {
        let original = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[
                {"type":"command","command":"a","timeout":10},
                {"type":"command","command":"b","timeout":20}
            ]}]}}"#,
        );
        let edited = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[
                {"type":"command","command":"a","timeout":11},
                {"type":"command","command":"b","timeout":21}
            ]}]}}"#,
        );
        let old = runnable_hooks(&original);
        let now = runnable_hooks(&edited);
        let mut store = TrustFile::default();
        store.record_hooks(USER, &original, &old, &[]);
        store.record_hooks(USER, &edited, &now[..1], &[]);
        assert!(store.is_hook_trusted(USER, &now[0]));
        assert!(!store.is_hook_trusted(USER, &old[0]));
        assert!(store.is_hook_trusted(USER, &old[1]));
        assert!(!store.is_hook_trusted(USER, &now[1]));
        assert_eq!(
            store.review_hooks(USER, &edited)[1].trust,
            HookTrust::Changed {
                previous: "b".to_string()
            }
        );
    }

    #[test]
    fn moving_a_hook_or_spelling_out_its_default_timeout_keeps_its_trust() {
        let original = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[
                {"type":"command","command":"a"},
                {"type":"command","command":"b"}
            ]}]}}"#,
        );
        let edited = hooks(&format!(
            r#"{{"hooks":{{"Stop":[{{"hooks":[
                {{"type":"command","command":"b","timeout":{}}},
                {{"type":"command","command":"a","statusMessage":"Checking"}}
            ]}}]}}}}"#,
            crate::hooks::CommandHook::DEFAULT_TIMEOUT_SECS
        ));
        let text = recorded_hooks("", USER, &original, &runnable_hooks(&original), &[]);
        assert!(
            !text.contains("timeout_secs"),
            "default records stay backward compatible"
        );
        let store = TrustFile::parse(&text).unwrap();
        assert!(
            store
                .review_hooks(USER, &edited)
                .iter()
                .all(|review| review.trust == HookTrust::Trusted)
        );
    }

    #[test]
    fn a_hook_can_stop_being_trusted() {
        let text = recorded_hooks(
            "",
            USER,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let text = recorded_hooks(
            &text,
            USER,
            &hooks(STOP_FMT),
            &[],
            &[hook("Stop", "", "cargo fmt")],
        );
        let store = TrustFile::parse(&text).expect("recorded store parses");
        assert_eq!(
            store.review_hooks(USER, &hooks(STOP_FMT))[0].trust,
            HookTrust::New
        );
    }

    #[test]
    fn recording_one_file_leaves_every_other_record_alone() {
        let text = recorded_hooks(
            "",
            PROJECT,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let text = recorded_hooks(
            &text,
            USER,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let store = TrustFile::parse(&text).expect("recorded store parses");
        assert!(store.is_hook_trusted(PROJECT, &hook("Stop", "", "cargo fmt")));
        assert!(store.is_hook_trusted(USER, &hook("Stop", "", "cargo fmt")));
    }

    #[test]
    fn trusted_only_keeps_the_trusted_hooks_and_what_cannot_run_anyway() {
        let file = hooks(
            r#"{"hooks":{"Stop":[{"hooks":[
                {"type":"command","command":"./trusted.sh"},
                {"type":"command","command":"./untrusted.sh"},
                {"type":"prompt","command":"./prompt"}
            ]}],"FileChanged":[{"hooks":[{"type":"command","command":"./unmodelled.sh"}]}]}}"#,
        );
        let kept = trusted_only(&file, &|identity| identity.command == "./trusted.sh");
        let commands: Vec<Option<String>> = kept.hooks["Stop"][0]
            .hooks
            .iter()
            .map(|handler| handler.command.clone())
            .collect();
        assert_eq!(
            commands,
            vec![
                Some("./trusted.sh".to_string()),
                Some("./prompt".to_string())
            ]
        );
        assert_eq!(kept.hooks["FileChanged"][0].hooks.len(), 1);
        // What the runner selects is the trusted hook alone.
        let selection = kept.select(HookEvent::Stop, None);
        let running: Vec<&str> = selection
            .handlers
            .iter()
            .map(|hook| hook.command.as_str())
            .collect();
        assert_eq!(running, vec!["./trusted.sh"]);
    }

    #[test]
    fn an_empty_or_legacy_store_records_nothing() {
        for text in [
            "",
            "   \n",
            r#"{"projects":{"/repo":{"files":{"/repo/.mcp.json":"sha256:aa"}}}}"#,
        ] {
            let store = TrustFile::parse(text).expect("loads");
            assert_eq!(
                store.review_hooks(USER, &hooks(STOP_FMT))[0].trust,
                HookTrust::New,
                "{text:?}"
            );
        }
        // …and a write keeps the `/trust` era's records it has not carried
        // over yet, for the projects not opened since.
        let text = recorded_hooks(
            r#"{"projects":{"/repo":{"files":{"/repo/.mcp.json":"sha256:aa"}}}}"#,
            USER,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        assert!(text.contains(r#""/repo/.mcp.json": "sha256:aa""#), "{text}");
        let store = TrustFile::parse(&text).expect("parses");
        assert_eq!(
            TrustFile::parse(&store.serialize()).expect("round-trips"),
            store
        );
    }

    /// A legacy hook approval alongside obsolete file hashes. Only the
    /// current hooks file is eligible to migrate.
    fn legacy_store(hooks_fp: &str) -> TrustFile {
        TrustFile::parse(&format!(
            r#"{{"projects":{{
                "/repo":{{"files":{{"{PROJECT}":"{hooks_fp}","/repo/.mcp.json":"sha256:old"}}}},
                "/other":{{"files":{{"/other/.zeroth/hooks.json":"sha256:bb"}}}}
            }}}}"#
        ))
        .expect("legacy store parses")
    }

    #[test]
    fn a_trust_era_approval_carries_over_at_the_same_content() {
        let hooks_fp = fingerprint(STOP_FMT.as_bytes());
        let mut store = legacy_store(&hooks_fp);
        let parsed_hooks = hooks(STOP_FMT);
        assert!(store.migrate_legacy("/repo", Some((PROJECT, &hooks_fp, &parsed_hooks))));
        assert!(store.is_hook_trusted(PROJECT, &hook("Stop", "", "cargo fmt")));
        let text = store.serialize();
        assert!(!text.contains(&hooks_fp), "{text}");
        assert!(text.contains("/other/.zeroth/hooks.json"), "{text}");
        assert!(
            text.contains("/repo/.mcp.json"),
            "inert legacy hashes are left alone"
        );
        assert!(!store.migrate_legacy("/repo", Some((PROJECT, &hooks_fp, &parsed_hooks))));
    }

    #[test]
    fn a_file_edited_since_trust_approved_it_carries_nothing_over() {
        let mut store = legacy_store("sha256:old-hooks");
        let before = store.serialize();
        let parsed_hooks = hooks(STOP_FMT);
        assert!(!store.migrate_legacy("/repo", Some((PROJECT, "sha256:new-hooks", &parsed_hooks))));
        assert!(!store.is_hook_trusted(PROJECT, &hook("Stop", "", "cargo fmt")));
        assert_eq!(store.serialize(), before);
    }

    #[test]
    fn a_decision_is_never_written_over_a_malformed_store() {
        // Whatever the user decided before is still in a file with a typo,
        // waiting for it to be fixed: the write is refused, never a fresh
        // store that drops every other project's decisions.
        let err = update_store(r#"{"hooks": {},}"#, |store| {
            store.record_hooks(
                USER,
                &hooks(STOP_FMT),
                &[hook("Stop", "", "cargo fmt")],
                &[],
            );
        })
        .expect_err("a malformed store is not written over");
        assert!(err.contains("trailing comma"), "{err}");
        // An empty file is an empty store, recorded into.
        let text = recorded_hooks(
            "",
            USER,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let store = TrustFile::parse(&text).expect("fresh store parses");
        assert!(store.is_hook_trusted(USER, &hook("Stop", "", "cargo fmt")));
    }

    #[test]
    fn a_decision_is_recorded_over_the_file_as_it_is_now() {
        // Two sessions answer at once, each having read the store before
        // the other wrote. Each records over the file as it is when it
        // answers, so the second keeps the first's answer.
        let other = "/other/.zeroth/hooks.json";
        let first = recorded_hooks(
            "",
            PROJECT,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let second = recorded_hooks(
            &first,
            other,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        let store = TrustFile::parse(&second).expect("parses");
        assert!(store.is_hook_trusted(PROJECT, &hook("Stop", "", "cargo fmt")));
        assert!(store.is_hook_trusted(other, &hook("Stop", "", "cargo fmt")));
    }

    #[test]
    fn obsolete_mcp_records_are_ignored_without_losing_hook_trust() {
        let initial = recorded_hooks(
            "",
            USER,
            &hooks(STOP_FMT),
            &[hook("Stop", "", "cargo fmt")],
            &[],
        );
        for obsolete in [
            r#"{"/repo/.mcp.json":{"s":{"enabled":true,"fingerprint":"sha256:old"}}}"#,
            r#"{"/repo/.mcp.json":{"s":{"enabled":"yes","unknown":[1,2,3]}}}"#,
            "null",
        ] {
            let with_obsolete = format!("{{\"mcp\":{obsolete},{}", &initial[1..]);
            let store = TrustFile::parse(&with_obsolete).expect("obsolete records are ignored");
            assert!(store.is_hook_trusted(USER, &hook("Stop", "", "cargo fmt")));
            let text = recorded_hooks(
                &with_obsolete,
                PROJECT,
                &hooks(STOP_FMT),
                &[hook("Stop", "", "cargo fmt")],
                &[],
            );
            assert!(!text.contains("mcp"), "{text}");
            let store = TrustFile::parse(&text).unwrap();
            assert!(store.is_hook_trusted(USER, &hook("Stop", "", "cargo fmt")));
            assert!(store.is_hook_trusted(PROJECT, &hook("Stop", "", "cargo fmt")));
        }
        // Ignoring one obsolete key must not relax the active guard schema.
        assert!(
            TrustFile::parse(r#"{"mcp":{},"hooks":{"/f":[{"event":"Stop","command":42}]}}"#)
                .is_err()
        );
    }

    #[test]
    fn a_malformed_store_is_an_error_not_silent_trust() {
        assert!(TrustFile::parse("not json").is_err());
        // Unknown fields are malformed too — a guard file with keys we don't
        // model is a guard file we don't understand.
        assert!(TrustFile::parse(r#"{"hooks":{},"extra":1}"#).is_err());
        assert!(
            TrustFile::parse(r#"{"hooks":{"/f":[{"event":"Stop","command":"x","y":1}]}}"#).is_err()
        );
        assert!(TrustFile::parse(r#"{"projects":{"/r":{"files":{},"x":1}}}"#).is_err());
    }

    #[test]
    fn the_store_reads_back_as_written() {
        let text = recorded_hooks(
            "",
            USER,
            &hooks(
                r#"{"hooks":{"PreToolUse":[{"matcher":"bash","hooks":[{"type":"command","command":"./guard.sh"}]}],
                         "Stop":[{"hooks":[{"type":"command","command":"cargo fmt"}]}]}}"#,
            ),
            &[
                hook("PreToolUse", "bash", "./guard.sh"),
                hook("Stop", "", "cargo fmt"),
            ],
            &[],
        );
        // A match-everything hook writes no matcher at all.
        assert!(text.contains(r#""matcher": "bash""#), "{text}");
        assert_eq!(text.matches("matcher").count(), 1, "{text}");
        let store = TrustFile::parse(&text).expect("parses");
        assert_eq!(
            TrustFile::parse(&store.serialize()).expect("round-trips"),
            store
        );
        // A text file ends its last line.
        assert!(text.ends_with("}\n"), "{text:?}");
    }
}
