//! Small filesystem helpers shared by the boundary's writers.
//!
//! Today it exists for one reason: the first-run seeds (`skills/`, `agents/`)
//! are written by every session at bootstrap, and two sessions starting in one
//! process used to expose half-written files to each other
//! (`docs/architecture/multithread-core.md`). A seed is content the process
//! already owns; there is no reason a reader should ever see it partial.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Write `contents` to `path` through a unique temporary file in the same
/// directory, then rename it into place. A reader sees either the old file or
/// the complete new one, never a torn write — on every platform a rename
/// within one directory is atomic.
///
/// The temporary name carries the pid and a process-local sequence so two
/// sessions seeding the same directory at once cannot collide. A failed
/// rename removes the temporary; a successful one leaves no trace.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("seed");
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".{name}.{}.{seq:x}.tmp", std::process::id()));
    std::fs::write(&temporary, contents)?;
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&temporary);
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_lands_whole_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join(format!(
            "zeroth-atomic-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("SKILL.md");
        write_atomic(&path, "first").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
        write_atomic(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary files left: {leftovers:?}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
