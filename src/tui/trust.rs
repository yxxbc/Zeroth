//! The startup review's boundary half (`docs/state/config-review.md`): reading what
//! it decides on — the user's hooks and the project's hooks —
//! and `trust.json`; building its pages; applying each answer live; and the
//! `/hooks` toggles that change a decision later. Every decision
//! is applied to the session's own copy of the store first, so a session
//! with nowhere to write still remembers what it was told, then recorded
//! over `trust.json` **as it is on disk at that moment**
//! (`trust::update_store`) — never over the copy read at startup, which
//! would erase whatever another session recorded since — and never over a
//! file that will not parse.

use std::path::{Path, PathBuf};

use zeroth::app::{
    ItemChange, ReviewAnswer, ReviewItem, ReviewKey, ReviewKind, ReviewPage, ReviewSection,
    ToastKind,
};
use zeroth::hooks;
use zeroth::trust::{self, HookIdentity, HookTrust, TrustFile};

use super::Session;
use super::config;

/// A project config file as read at bootstrap: where, the fingerprint of
/// its bytes, and its parse.
#[derive(Debug, Clone)]
pub(crate) struct ProjectFile<T> {
    pub(crate) path: PathBuf,
    /// What the `/trust` era pinned an approval to (`trust::fingerprint` of
    /// the bytes) — what carrying one over compares against.
    pub(crate) fingerprint: String,
    pub(crate) parsed: Result<T, String>,
}

impl<T: Clone> ProjectFile<T> {
    /// `(path, fingerprint, parse)` — the file as the `/trust` era's
    /// approvals are carried over against — when it parsed.
    fn pinned(&self) -> Option<(String, String, T)> {
        let parsed = self.parsed.as_ref().ok()?.clone();
        Some((
            self.path.display().to_string(),
            self.fingerprint.clone(),
            parsed,
        ))
    }
}

/// The project's `.zeroth` layer, read once at bootstrap
/// (`docs/state/project-config.md`).
#[derive(Debug, Clone)]
pub(crate) struct ProjectLayer {
    /// The project root — the `/trust` era's key for its approvals.
    pub(crate) root: PathBuf,
    /// `{root}/.zeroth/hooks.json`, when present.
    pub(crate) hooks: Option<ProjectFile<hooks::HooksFile>>,
}

/// What the review decides over: `trust.json` and the files it is about.
#[derive(Debug, Default)]
pub(crate) struct TrustState {
    /// `trust.json` as the session knows it: what it read at startup — empty
    /// (nothing trusted) when the file would not parse, the guard failing
    /// closed — plus every decision made since.
    store: TrustFile,
    /// Where the store lives; `None` without a config home, when decisions
    /// hold for the session only.
    store_path: Option<PathBuf>,
    /// The user's hooks file: its path and parse (empty when absent or
    /// broken — the broken one's toast is `report_hooks_error`'s).
    user_hooks: Option<(PathBuf, hooks::HooksFile)>,
    /// The project layer; `None` when `ZEROTH_PROJECT_CONFIG` is off or
    /// the cwd resolves to the home directory.
    pub(crate) project: Option<ProjectLayer>,
}

impl TrustState {
    /// Read `trust.json` and hold the user's hooks file and the project
    /// layer beside it, carrying the project's `/trust`-era approvals over
    /// first. The second return is the error to toast, if any — a
    /// malformed store is loud.
    pub(crate) fn load(
        user_hooks: Option<(PathBuf, hooks::HooksFile)>,
        project: Option<ProjectLayer>,
    ) -> (Self, Option<String>) {
        let store_path = config::trust_json_path();
        let (store, error) = config::load_trust_file(store_path.as_deref());
        let mut state = Self {
            store,
            store_path,
            user_hooks,
            project,
        };
        let migrated = state.migrate_legacy().err();
        (state, error.or(migrated))
    }

    /// Carry over what `/trust` approved in this project at exactly the
    /// files' current bytes (`TrustFile::migrate_legacy`), so an upgrade
    /// does not ask about it again; written back only when something was.
    fn migrate_legacy(&mut self) -> Result<(), String> {
        let Some(project) = &self.project else {
            return Ok(());
        };
        let root = project.root.display().to_string();
        let hooks = project.hooks.as_ref().and_then(ProjectFile::pinned);
        let migrate = |store: &mut TrustFile| {
            let hooks = hooks
                .as_ref()
                .map(|(path, fingerprint, file)| (path.as_str(), fingerprint.as_str(), file));
            store.migrate_legacy(&root, hooks)
        };
        if !migrate(&mut self.store) {
            return Ok(());
        }
        self.persist(|store| {
            migrate(store);
        })
    }

    /// Every hooks file that parsed, with its absolute path and scope word —
    /// the user's first, the project's after it (the merge's order).
    fn hook_layers(&self) -> Vec<(String, &hooks::HooksFile, &'static str)> {
        let mut out = Vec::new();
        if let Some((path, file)) = &self.user_hooks {
            out.push((path.display().to_string(), file, "User"));
        }
        if let Some(file) = self.project.as_ref().and_then(|p| p.hooks.as_ref())
            && let Ok(parsed) = &file.parsed
        {
            out.push((file.path.display().to_string(), parsed, "Project"));
        }
        out
    }

    /// Every configured hook, trusted or not: the user's file with the
    /// project's unioned in after it (`HooksFile::merged`) — what `/hooks`
    /// browses and the `/settings` row's availability asks about.
    pub(crate) fn configured_hooks(&self) -> hooks::HooksFile {
        self.hook_layers()
            .into_iter()
            .fold(hooks::HooksFile::default(), |merged, (_, file, _)| {
                merged.merged(file)
            })
    }

    /// The hooks the session runs: the configured ones the user trusts
    /// (`trust::trusted_only`), each layer filtered against its own file's
    /// record.
    pub(crate) fn runner_hooks(&self) -> hooks::HooksFile {
        self.hook_layers().into_iter().fold(
            hooks::HooksFile::default(),
            |merged, (source, file, _)| {
                merged.merged(&trust::trusted_only(file, &|identity| {
                    self.store.is_hook_trusted(&source, identity)
                }))
            },
        )
    }

    /// Is `identity` trusted in the hooks file `source`?
    pub(crate) fn is_hook_trusted(&self, source: &str, identity: &HookIdentity) -> bool {
        self.store.is_hook_trusted(source, identity)
    }

    /// The hooks page: every new or changed hook, under its file.
    pub(crate) fn hooks_page(&self, home: Option<&Path>) -> Option<ReviewPage> {
        let mut sections = Vec::new();
        for (source, file, scope) in self.hook_layers() {
            let items: Vec<ReviewItem> = self
                .store
                .review_hooks(&source, file)
                .into_iter()
                .filter_map(|review| {
                    let change = match review.trust {
                        HookTrust::Trusted => return None,
                        HookTrust::New => None,
                        HookTrust::Changed { previous } => Some(ItemChange::Was(previous)),
                    };
                    Some(ReviewItem {
                        title: review.identity.label(),
                        detail: vec![review.identity.command.clone()],
                        change,
                        checked: true,
                        key: ReviewKey::Hook {
                            source: source.clone(),
                            identity: review.identity,
                        },
                    })
                })
                .collect();
            if !items.is_empty() {
                sections.push(ReviewSection {
                    scope: Some(scope.to_string()),
                    path: zeroth::ui::display_cwd(Path::new(&source), home),
                    items,
                });
            }
        }
        (!sections.is_empty()).then(|| ReviewPage::new(ReviewKind::Hooks, sections))
    }

    /// Record a hooks decision for the file `source`. `Err` names a write
    /// that did not happen: the decision still holds for the session.
    fn record_hooks(
        &mut self,
        source: &str,
        trusted: &[HookIdentity],
        distrusted: &[HookIdentity],
    ) -> Result<(), String> {
        let Some(file) = self
            .hook_layers()
            .into_iter()
            .find(|(path, _, _)| path == source)
            .map(|(_, file, _)| file.clone())
        else {
            return Ok(());
        };
        self.record(|store| store.record_hooks(source, &file, trusted, distrusted))
    }

    /// Apply `change` to the session's copy, then record it in `trust.json`.
    fn record(&mut self, change: impl Fn(&mut TrustFile)) -> Result<(), String> {
        change(&mut self.store);
        self.persist(change)
    }

    /// Record `change` over `trust.json` as it is on disk **now** — read
    /// afresh, so what another session recorded since startup survives — and
    /// never over a file that will not parse (`trust::update_store`): a typo
    /// costs this session's choices their persistence, not every earlier
    /// decision. `Ok` with nowhere to write (no config home).
    fn persist(&self, change: impl FnOnce(&mut TrustFile)) -> Result<(), String> {
        let Some(path) = &self.store_path else {
            return Ok(());
        };
        let not_saved = |err: &dyn std::fmt::Display| {
            format!("trust.json: {err} — not saved, so this choice lasts this session only")
        };
        let existing = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(not_saved(&err)),
        };
        let text = trust::update_store(&existing, change).map_err(|err| not_saved(&err))?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, text).map_err(|err| {
            format!("Could not save trust.json: {err} — this choice lasts this session only")
        })
    }
}

/// Gather the project layer at bootstrap: resolve the root and read each
/// project file. `None` when `ZEROTH_PROJECT_CONFIG` is off, or the
/// root is the home directory (never a project, `trust::is_project_root`).
pub(crate) fn load_project_layer(cwd: &Path) -> Option<ProjectLayer> {
    if !config::project_config_enabled() {
        return None;
    }
    let root = zeroth::project_doc::find_project_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if !trust::is_project_root(&root, home.as_deref()) {
        return None;
    }
    // A project path that IS the user layer's file (an
    // `ZEROTH_CONFIG_DIR` pointed inside the project) is already loaded
    // as the user's — never read twice.
    let user_file = config::user_hooks_path();
    let guard = |path: PathBuf| (user_file.as_ref() != Some(&path)).then_some(path);
    let read = |path: PathBuf| -> Option<(PathBuf, String, Result<String, String>)> {
        match std::fs::read_to_string(&path) {
            Ok(text) => Some((path, trust::fingerprint(text.as_bytes()), Ok(text))),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => Some((path, String::new(), Err(err.to_string()))),
        }
    };
    let hooks =
        guard(trust::project_hooks_file(&root))
            .and_then(read)
            .map(|(path, fingerprint, text)| ProjectFile {
                // Every handler remembers the project's file, so a verdict can
                // name it beside the user's own.
                parsed: text.and_then(|text| {
                    hooks::HooksFile::parse(&text).map(|file| file.with_source(&path))
                }),
                fingerprint,
                path,
            });
    Some(ProjectLayer { root, hooks })
}

impl Session<'_> {
    /// The startup toasts for what the review reads, least severe first so
    /// the one toast slot ends on what matters most: a project hooks file
    /// that would not parse (red), then a malformed `trust.json` (red — the guard
    /// file limits everything).
    pub(crate) fn report_trust_state(&mut self, trust_error: Option<String>) {
        let hooks_error = self
            .trust
            .project
            .as_ref()
            .and_then(|project| project.hooks.as_ref())
            .and_then(|file| {
                file.parsed.as_ref().err().map(|err| {
                    format!(
                        "{}: {err} — project hooks are off this session",
                        file.path.display()
                    )
                })
            });
        if let Some(message) = hooks_error {
            self.toast(message, ToastKind::Error);
        }
        if let Some(error) = trust_error {
            self.toast(error, ToastKind::Error);
        }
    }

    /// The review's hooks page while hooks are enabled and await trust.
    pub(crate) fn review_pages(&self, hooks: bool) -> Vec<ReviewPage> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut pages = Vec::new();
        if hooks
            && self.models.hooks_enabled()
            && let Some(page) = self.trust.hooks_page(home.as_deref())
        {
            pages.push(page);
        }
        pages
    }

    /// Open the startup review over whatever waits for a decision, holding
    /// `SessionStart` until it closes (`docs/state/config-review.md`). Whether it
    /// opened — the caller holds a `[PROMPT]` the same way.
    pub(crate) fn open_startup_review(&mut self) -> bool {
        let pages = self.review_pages(true);
        self.app.open_config_review(pages);
        let open = self.app.config_review.is_some();
        if open {
            self.models.hold_session_start();
        }
        open
    }

    /// Open the hooks page alone — the `/settings` **Hooks** row turned on
    /// over hooks that wait for trust.
    pub(crate) fn open_hooks_review(&mut self) {
        let pages = self.review_pages(true);
        self.app.open_requested_review(pages);
    }

    /// Apply one page's answer (`docs/state/config-review.md`), then — once the
    /// review has closed — let what it held go.
    pub(crate) fn apply_review_answer(&mut self, answer: ReviewAnswer) {
        let result = match answer.kind {
            ReviewKind::Hooks => self.apply_hooks_answer(&answer),
        };
        if let Err(error) = result {
            self.toast(error, ToastKind::Error);
        }
        if self.app.config_review.is_none() {
            self.finish_review();
        }
        self.frame.schedule_frame();
    }

    /// Hooks: a confirmed page records the selected hooks as trusted, and
    /// they run from now on; the rest stay untrusted, unrecorded, asked
    /// about again at the next launch — as is everything on a declined page.
    fn apply_hooks_answer(&mut self, answer: &ReviewAnswer) -> Result<(), String> {
        if !answer.confirmed {
            return Ok(());
        }
        let mut by_file: Vec<(String, Vec<HookIdentity>)> = Vec::new();
        for (key, checked) in &answer.choices {
            if let (ReviewKey::Hook { source, identity }, true) = (key, checked) {
                match by_file.iter_mut().find(|(file, _)| file == source) {
                    Some((_, list)) => list.push(identity.clone()),
                    None => by_file.push((source.clone(), vec![identity.clone()])),
                }
            }
        }
        let mut result = Ok(());
        for (source, trusted) in &by_file {
            if let Err(error) = self.trust.record_hooks(source, trusted, &[]) {
                result = Err(error);
            }
        }
        self.models.set_hooks_file(self.trust.runner_hooks());
        result
    }

    /// The review has closed: fire the held `SessionStart` with the hooks it
    /// trusted, and submit a held `[PROMPT]` with those hooks active.
    fn finish_review(&mut self) {
        self.models.release_session_start();
        if let Some(prompt) = self.held_prompt.take() {
            self.submit_startup_prompt(prompt);
        }
    }

    /// `/hooks`: trust a hook, or stop trusting it — recorded, and the hooks
    /// the session runs rebound at once.
    pub(crate) fn set_hook_trust(&mut self, source: &str, identity: HookIdentity, trusted: bool) {
        let (trust, distrust) = if trusted {
            (vec![identity], Vec::new())
        } else {
            (Vec::new(), vec![identity])
        };
        let result = self.trust.record_hooks(source, &trust, &distrust);
        self.models.set_hooks_file(self.trust.runner_hooks());
        match result {
            Err(error) => self.toast(error, ToastKind::Error),
            Ok(()) if trusted => self.toast("Hook trusted — it runs from now on", ToastKind::Info),
            Ok(()) => self.toast("Hook no longer trusted — it does not run", ToastKind::Info),
        }
        self.frame.schedule_frame();
    }
}
