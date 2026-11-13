//! Host facts gathered at the I/O boundary — the impurities the pure library
//! never sees.
//!
//! `App` and `ui` are pure: they take a timestamp, a date, an OS string as
//! *values* (the `App::set_clock` pattern), so every read of the real world
//! funnels through here. Nothing in this module renders or decides anything;
//! each function answers one question about the machine we happen to be
//! running on.
//!
//! - the wall clock: [`local_timestamp`] (the Ctrl+O transcript's stamp,
//!   `docs/ui/timestamps.md`), [`utc_stamp`] (rollout write stamps,
//!   `docs/state/resume.md`), [`unix_secs`] (input-history `ts`,
//!   `docs/input/history-persistence.md`),
//! - the agent's environment context: [`local_date`], [`os_context`] and
//!   [`user_context`] (`docs/architecture/environment.md`),
//! - process identity: [`process_uid`] (the background tasks root,
//!   `docs/tools/background.md`, and the environment's `User` line) and
//!   [`session_id`],
//! - telemetry (`TELEMETRY.md`): [`utc_day`] (the once-a-day key) and
//!   [`random_bytes`] (the install id's entropy).

use std::collections::BTreeSet;
use std::path::PathBuf;

use zeroth::llm;

/// What one process resolves once and every session shares
/// (`docs/architecture/multithread-core.md`): the working directory and config
/// home, the first-run seed writes, the skill and agent walks, the settings
/// snapshot, the secrets and lifecycle-hook file, and the pieces the model
/// resolver needs (providers, `.env`, the saved selection).
///
/// A session still owns everything mutable — its `App`, rollout, permission
/// gate, MCP manager, background shells and subagent registry. The Host is
/// read-only after [`Host::resolve`], so a batch can hand `&Host` to every
/// worker; cloning a field out of it is how a session gets its own copy.
pub(crate) struct Host {
    pub(crate) cwd: PathBuf,
    pub(crate) home: Option<PathBuf>,
    pub(crate) project: String,
    pub(crate) config_home: Option<PathBuf>,
    /// The discovered skills, in precedence order, and the errors the startup
    /// toast reports (a broken `SKILL.md` costs its skill, not the session).
    pub(crate) skills: Vec<zeroth::skills::SkillMetadata>,
    pub(crate) skill_errors: Vec<zeroth::skills::SkillError>,
    /// This project's saved on/off choices from `skills.json`.
    pub(crate) skills_disabled: BTreeSet<String>,
    /// The discovered subagent definitions, in precedence order.
    pub(crate) agents: Vec<zeroth::subagents::AgentDefinition>,
    pub(crate) agent_errors: Vec<zeroth::subagents::AgentFileError>,
    /// This directory's `/settings` entry with the `ZEROTH_*` overrides
    /// already applied, and the tips cursor the session starts on.
    pub(crate) settings: zeroth::settings::SessionSettings,
    pub(crate) tips_next: usize,
    /// The loaded secrets file; each session wraps its own registry around a
    /// clone, because the registry's masked set is per conversation.
    pub(crate) secrets: zeroth::secrets::SecretStore,
    pub(crate) secrets_error: Option<String>,
    /// The user's `hooks.json` (path + parse) and the error to toast.
    pub(crate) hooks: Option<(PathBuf, zeroth::hooks::HooksFile)>,
    pub(crate) hooks_error: Option<String>,
    /// What `ModelSession::resolve` otherwise re-reads per session.
    pub(crate) providers: zeroth::llm::config::ProvidersFile,
    pub(crate) provider_issues: Vec<String>,
    pub(crate) env_file: zeroth::llm::keystore::EnvFile,
    pub(crate) saved_selection: Option<zeroth::llm::settings::ModelSelection>,
}

impl Host {
    /// Resolve the process-wide facts once, before any session bootstraps.
    ///
    /// Best-effort by the same rule as bootstrap: an unreadable file is an
    /// empty value and (where bootstrap toasts it) an error the session
    /// reports. The pre-fork home migration runs here first, so no session
    /// can race it on a fresh home.
    pub(crate) fn resolve() -> Self {
        let _ = super::config::migrate_from_alter_zero();
        let cwd = std::env::current_dir().unwrap_or_default();
        let project = cwd.display().to_string();
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let config_home = super::config::config_home();

        let mut skill_errors =
            match zeroth::llm::skill::resolved_builtin_skills_dir(config_home.as_deref()) {
                Some(root) => zeroth::llm::skill::seed_builtin_skills(&root),
                None => Vec::new(),
            };
        let (skills, walk_errors) = zeroth::llm::skill::load_skills(&cwd, config_home.as_deref());
        skill_errors.extend(walk_errors);
        let skills_disabled =
            super::config::load_skills_file(super::config::skills_json_path().as_deref())
                .disabled_for(&project);

        let mut agent_errors = match zeroth::llm::subagent::user_agents_dir(config_home.as_deref())
        {
            Some(dir) => zeroth::llm::subagent::seed_default_agents(&dir),
            None => Vec::new(),
        };
        let (agents, walk_errors) =
            zeroth::llm::subagent::load_agents(&cwd, config_home.as_deref(), home.as_deref());
        agent_errors.extend(walk_errors);

        let mut settings =
            super::config::load_settings_file(super::config::settings_json_path().as_deref())
                .settings_for(&project);
        settings.telemetry =
            super::config::load_telemetry_file(super::config::telemetry_json_path().as_deref())
                .enabled;
        settings.update_check =
            super::config::load_update_file(super::config::update_json_path().as_deref()).enabled;
        let tips = super::config::load_tips_file(super::config::tips_json_path().as_deref());
        settings.tips = tips.enabled;
        let settings = super::config::apply_setting_overrides(settings);

        let (secrets, secrets_error) = match super::config::secrets_json_path()
            .map(|path| zeroth::llm::keystore::load_secrets_file(&path))
        {
            Some(Ok(store)) => (store, None),
            Some(Err(error)) => (zeroth::secrets::SecretStore::new(), Some(error)),
            None => (zeroth::secrets::SecretStore::new(), None),
        };

        let hooks_path = super::config::user_hooks_path();
        let (hooks_file, hooks_error) = super::config::load_hooks(hooks_path.as_deref());
        let hooks = hooks_path.map(|path| (path, hooks_file));

        let (providers, provider_issues) = super::config::startup_providers(home.as_deref());
        let env_file = super::config::load_env_file(&super::config::env_file_path());
        let saved_selection = super::config::adopt_selection(
            super::config::settings_file_path().as_deref(),
            &project,
        );

        Self {
            cwd,
            home,
            project,
            config_home,
            skills,
            skill_errors,
            skills_disabled,
            agents,
            agent_errors,
            settings,
            tips_next: tips.next,
            secrets,
            secrets_error,
            hooks,
            hooks_error,
            providers,
            provider_issues,
            env_file,
            saved_selection,
        }
    }
}

/// Local wall-clock stamp for recorded items: 12-hour time, no seconds, e.g.
/// `03:20 AM`. Injected via [`App::set_clock`](zeroth::app::App::set_clock) and shown **only** under the
/// user's message in the Ctrl+O transcript — the one impurity kept out of the
/// pure library.
pub(crate) fn local_timestamp() -> String {
    chrono::Local::now().format("%I:%M %p").to_string()
}

/// Local date for the agent's environment context — weekday plus ISO date,
/// e.g. `Sunday 2026-07-19`. Gathered here at the boundary and folded into the
/// system prompt by [`augment_with_environment`] so the agent knows the day
/// (see `docs/architecture/environment.md`).
///
/// [`augment_with_environment`]: zeroth::llm::backend::augment_with_environment
pub(crate) fn local_date() -> String {
    chrono::Local::now().format("%A %Y-%m-%d").to_string()
}

/// The OS string for the agent's environment context: the platform
/// (`std::env::consts::OS`) enriched, on Linux, with the distro from
/// `/etc/os-release` — e.g. `linux (Ubuntu 24.04.4 LTS)`. Boundary code (reads
/// the file); the parse is the pure `backend::os_release_name`. Falls back to
/// the bare platform when the file is missing/unreadable or off Linux (see
/// `docs/architecture/environment.md`).
pub(crate) fn os_context() -> String {
    let os = std::env::consts::OS;
    if os == "linux" {
        let distro = std::fs::read_to_string("/etc/os-release")
            .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
            .ok()
            .and_then(|contents| llm::backend::os_release_name(&contents));
        if let Some(distro) = distro {
            return format!("{os} ({distro})");
        }
    }
    os.to_string()
}

/// The user for the agent's environment context — the account this process
/// runs as, e.g. `root` or `linuztx`, so the agent knows whether it is root
/// (`docs/architecture/environment.md`). The effective uid decides ([`process_uid`], as
/// `whoami` reports it); the name comes from `/etc/passwd`, else
/// `$USER`/`$LOGNAME`/`$USERNAME` (macOS and directory-service accounts live
/// outside the file). Boundary code (reads the uid, the file and the
/// environment); the label is the pure `backend::user_label`, the parse
/// `backend::passwd_name`.
pub(crate) fn user_context() -> String {
    // Off unix there is no uid to read, and `process_uid`'s path-shape 0
    // there would claim root.
    let uid = cfg!(unix).then(process_uid);
    let name = uid.and_then(passwd_lookup).or_else(|| {
        ["USER", "LOGNAME", "USERNAME"].into_iter().find_map(|key| {
            std::env::var(key)
                .ok()
                .filter(|name| !name.trim().is_empty())
        })
    });
    llm::backend::user_label(uid, name.as_deref())
}

/// `uid`'s name in `/etc/passwd`, streamed a line at a time: the file can be
/// large on a shared host, and a GECOS field in a legacy encoding must not
/// hide the entries after it (hence the lossy decode, never `read_to_string`).
fn passwd_lookup(uid: u32) -> Option<String> {
    use std::io::BufRead;
    let file = std::fs::File::open("/etc/passwd").ok()?;
    std::io::BufReader::new(file)
        .split(b'\n')
        .map_while(Result::ok)
        .find_map(|line| llm::backend::passwd_name(&String::from_utf8_lossy(&line), uid))
}

/// UTC write-time stamp for rollout lines — codex's
/// `YYYY-MM-DDTHH:MM:SS.mmmZ` shape.
pub(crate) fn utc_stamp() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// A unique-enough session id: nanos since the epoch, the pid, and a
/// process-local sequence, in hex. No uuid dependency — the id is never parsed
/// back (resume goes by path); it only has to keep concurrent instances off
/// each other's files. The sequence matters once several sessions share a
/// process (`docs/architecture/multithread-core.md`): two threads starting in
/// the same nanosecond used to land on one rollout path.
pub(crate) fn session_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{:x}-{seq:x}", std::process::id())
}

/// Today's UTC date, `YYYY-MM-DD` — the key the daily telemetry ping is
/// throttled on (`telemetry::should_ping`). UTC rather than local so the
/// client's day and the collector's agree except around midnight, where a
/// second ping is harmless (the collector dedups on the day too).
pub(crate) fn utc_day() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// `N` bytes from the OS entropy source — the telemetry install id's
/// material (`TelemetryFile::install_id_or_mint`). Falls back to the clock
/// and pid when the source fails, which should never happen on a real
/// system, so that a failure still yields an id unlikely to collide rather
/// than a shared all-zero one.
pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    if getrandom::fill(&mut bytes).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let seed = nanos ^ u128::from(std::process::id()).rotate_left(64);
        for (slot, byte) in bytes.iter_mut().zip(seed.to_le_bytes().iter().cycle()) {
            *slot = *byte;
        }
    }
    bytes
}

/// Seconds since the Unix epoch — the `ts` stamped into each history line.
/// Impurity kept at the boundary (the `utc_stamp`/`session_id` pattern).
pub(crate) fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// [`unix_secs`] as the signed seconds the `/usage` page counts in.
pub(crate) fn unix_secs_signed() -> i64 {
    i64::try_from(unix_secs()).unwrap_or(i64::MAX)
}

/// The local clock's offset from UTC, in seconds, at the instant `at` (Unix
/// seconds) — read per instant, so a time across a daylight-saving change
/// reads the right hour. Handed to `App` once (`App::set_local_offset`): the
/// `/usage` page's reset times (`docs/providers/usage.md`).
pub(crate) fn local_offset(at: i64) -> i32 {
    use chrono::{Offset, TimeZone};
    chrono::Local
        .timestamp_opt(at, 0)
        .single()
        .map_or(0, |local| local.offset().fix().local_minus_utc())
}

/// This process's effective uid — the one its permissions are checked
/// against: the stable per-user segment of the background tasks root (Claude
/// Code's `claude-{uid}` pattern, `background::tasks_dir`), and what the
/// environment's `User` line calls root or not ([`user_context`]). Read
/// through rustix's safe `geteuid` (this crate forbids `unsafe`, so no
/// `libc`), on macOS as on Linux: the `/proc/self` owner it replaced answered
/// 0 wherever `/proc` is absent, which on a Linux sandbox without it put every
/// user under one `/tmp/zeroth-0`.
#[cfg(unix)]
pub(crate) fn process_uid() -> u32 {
    rustix::process::geteuid().as_raw()
}

/// Non-unix fallback: no uid concept to read — 0 keeps the path shape.
#[cfg(not(unix))]
pub(crate) fn process_uid() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    #[test]
    fn session_ids_from_concurrent_threads_never_collide() {
        let ids = std::sync::Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    let mut local = Vec::new();
                    for _ in 0..64 {
                        local.push(super::session_id());
                    }
                    ids.lock().unwrap().extend(local);
                });
            }
        });
        let ids = ids.into_inner().unwrap();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "a session id was issued twice");
    }
}
