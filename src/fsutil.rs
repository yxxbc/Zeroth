//! Small filesystem helpers shared by the boundary's writers.
//!
//! Two sessions in one process write the same bootstrap files: the first-run
//! seeds (`skills/`, `agents/`) and the config-home migration. A reader in
//! another session used to see half-written files (`docs/architecture/
//! multithread-core.md`); these helpers publish a complete file with one
//! rename, so it sees either the old content or the whole new content.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A unique temporary path next to `path`: same directory, so the final
/// rename stays atomic, with the pid and a process-local sequence keeping two
/// writers apart.
fn temporary(path: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".{name}.{}.{seq:x}.tmp", std::process::id()))
}

/// Rename `temporary` onto `path`, removing it if the rename fails so a
/// failed write leaves no debris.
fn publish(temporary: &Path, path: &Path) -> std::io::Result<()> {
    match std::fs::rename(temporary, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(temporary);
            Err(err)
        }
    }
}

/// Write `contents` to `path` through a unique temporary file in the same
/// directory, then rename it into place. A reader sees either the old file or
/// the complete new one, never a torn write.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let temporary = temporary(path);
    std::fs::write(&temporary, contents)?;
    publish(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zeroth-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn a_write_lands_whole_and_leaves_no_temporary() {
        let dir = scratch("atomic");
        let path = dir.join("SKILL.md");
        write_atomic(&path, "first").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
        write_atomic(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        assert!(
            leftovers(&dir).is_empty(),
            "temporaries: {:?}",
            leftovers(&dir)
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
