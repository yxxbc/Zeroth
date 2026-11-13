//! Env-gated timing lines for the boundary's measured freezes.
//!
//! The concurrency note asked for numbers before designs
//! (`docs/architecture/concurrency.md`): a turn-end checkpoint, a recorder
//! sync, a frame. Writing to stderr from the TUI would corrupt the inline
//! viewport, so the sink is a file:
//!
//! - `ZEROTH_TIMING=<path>` appends to that file;
//! - `ZEROTH_TIMING=1` appends to `<tmp>/zeroth-timing.log`;
//! - unset, empty, or `0` writes nothing.
//!
//! One line per measured event, flushed immediately, so a run can be timed
//! without a debugger. `TimingGuard` measures a scope even when it has early
//! returns.

use std::io::Write as _;
use std::time::{Duration, Instant};

/// Measure the scope it lives in; logs on drop.
pub(crate) struct TimingGuard {
    label: &'static str,
    began: Instant,
}

impl TimingGuard {
    pub(crate) fn new(label: &'static str) -> Self {
        Self {
            label,
            began: Instant::now(),
        }
    }
}

impl Drop for TimingGuard {
    fn drop(&mut self) {
        record(self.label, self.began.elapsed());
    }
}

/// Append one timing line, if `ZEROTH_TIMING` asks for one.
pub(crate) fn record(label: &str, elapsed: Duration) {
    let Ok(target) = std::env::var("ZEROTH_TIMING") else {
        return;
    };
    if target.is_empty() || target == "0" || target.eq_ignore_ascii_case("false") {
        return;
    }
    let path = if target == "1" || target.eq_ignore_ascii_case("true") {
        std::env::temp_dir().join("zeroth-timing.log")
    } else {
        std::path::PathBuf::from(target)
    };
    let line = format!(
        "{} {label}: {:.1} ms\n",
        super::host::utc_stamp(),
        elapsed.as_secs_f64() * 1_000.0
    );
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_disabled_guard_writes_nothing() {
        // The default is off; this mostly pins that the function is safe to
        // call in every environment (a missing variable is not a panic).
        super::record("test", std::time::Duration::from_millis(1));
    }
}
