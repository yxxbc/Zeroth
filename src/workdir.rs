//! The session's **working directory**, as the tools see it — the one fact
//! `/cd` moves.
//!
//! Most of a session's directory facts are the **project**: the permissions
//! key, the checkpoint store, the rollout file, the skills and agents that
//! were discovered, and the `AGENTS.md` in the context. Those stay where the
//! session started — moving them mid-conversation would strand rules, snapshots
//! and history under a directory nobody is working in
//! (`docs/state/change-directory.md`).
//!
//! What *does* move is where work happens: `bash`, `read`, `write` and `edit`,
//! the `!` shell, and the footer's path. Those all read the same handle, so
//! they cannot disagree: one `Arc<RwLock<PathBuf>>`, cloned into the executor,
//! the background registry and the shell runner by the boundary, and set by
//! `/cd` in the event loop. The pure core never reads it — the footer is fed
//! the display string the loop formats (`App::set_session_cwd`), the same seam
//! the clock and the model name ride.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// A shared, swappable working directory. Cheap to clone; every clone sees the
/// directory `/cd` last set.
#[derive(Debug, Clone)]
pub struct WorkDir(Arc<RwLock<PathBuf>>);

impl WorkDir {
    /// A handle on `path` (absolute, as the boundary read it).
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(Arc::new(RwLock::new(path.into())))
    }

    /// The directory commands run in now.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.0.read().map(|dir| dir.clone()).unwrap_or_default()
    }

    /// Move the session — `/cd`'s one write.
    pub fn set(&self, path: impl Into<PathBuf>) {
        if let Ok(mut dir) = self.0.write() {
            *dir = path.into();
        }
    }

    /// `given` as a real path under this session: an absolute path is itself,
    /// anything else is relative to the current directory. `~` and `~/…` are
    /// expanded against `home` when one is known, so `/cd ..` and `/cd
    /// ~/other-repo` both read the way a shell would.
    #[must_use]
    pub fn resolve(&self, given: &str, home: Option<&Path>) -> PathBuf {
        expand_home(given.trim(), home)
            .map_or_else(|relative| self.path().join(relative), PathBuf::from)
    }
}

/// `~` / `~/x` expanded against `home`; `Err` with the path untouched for
/// anything else, and for a `~` with no home to expand it with.
fn expand_home(given: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    let relative = match given {
        "~" => return home.map(Path::to_path_buf).ok_or_else(|| given.to_string()),
        _ => match given.strip_prefix("~/") {
            Some(rest) => rest,
            None => return Err(given.to_string()),
        },
    };
    home.map(|home| home.join(relative))
        .ok_or_else(|| given.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absolute_path_is_itself_and_a_relative_one_joins_the_current_dir() {
        let dir = WorkDir::new("/home/user/repo");
        assert_eq!(dir.path(), PathBuf::from("/home/user/repo"));
        assert_eq!(dir.resolve("/etc/hosts", None), PathBuf::from("/etc/hosts"));
        assert_eq!(
            dir.resolve("src/app.rs", None),
            PathBuf::from("/home/user/repo/src/app.rs")
        );
        assert_eq!(
            dir.resolve("../sibling", None),
            PathBuf::from("/home/user/repo/../sibling"),
            "lexical, like every other path the session touches"
        );
    }

    #[test]
    fn a_tilde_expands_against_home_and_without_one_stays_relative() {
        let dir = WorkDir::new("/home/user/repo");
        let home = Path::new("/home/user");
        assert_eq!(dir.resolve("~", Some(home)), PathBuf::from("/home/user"));
        assert_eq!(
            dir.resolve("~/other-repo", Some(home)),
            PathBuf::from("/home/user/other-repo")
        );
        // No home known: `~` is not silently a directory named `~` under the
        // cwd — the caller will fail its `is_dir` check and say so.
        assert_eq!(
            dir.resolve("~/other-repo", None),
            PathBuf::from("/home/user/repo/~/other-repo")
        );
    }

    #[test]
    fn every_clone_sees_the_move() {
        let one = WorkDir::new("/a");
        let two = one.clone();
        one.set("/b");
        assert_eq!(two.path(), PathBuf::from("/b"));
        two.set("/c");
        assert_eq!(one.path(), PathBuf::from("/c"));
    }
}
