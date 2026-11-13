//! Manual concurrency probe for the tool executor (not run by `cargo test`).
//!
//! Runs `K` `bash` calls (default: `sleep 1`) in one of three shapes and prints
//! the wall time, so the ceiling of each can be compared on the same machine:
//!
//! ```bash
//! cargo run --release --example concurrency_probe -- seq 4 1
//! cargo run --release --example concurrency_probe -- threads 4 1
//! cargo run --release --example concurrency_probe -- pool 8 1 4
//! ```
//!
//! `seq` is the production shape (`llm/agent.rs`'s `for call in &rest`);
//! `threads` is one `std::thread` per call; `pool` is a bounded worker pool
//! (`pool` workers). The workload is the real [`RealToolExecutor`], a pty per
//! call — not a mock — so the numbers include process spawning and reaping.
//! Measure peak RSS and thread count from outside (`/usr/bin/time -l`,
//! `ps -M`), which is what the companion Python harness does.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use zeroth::llm::exec::{RealToolExecutor, ToolExecutor, ToolProgress};
use zeroth::llm::tools::ToolCallRequest;
use zeroth::stream::CancelToken;

fn call(i: usize, secs: f64) -> ToolCallRequest {
    let command = std::env::var("PROBE_CMD").unwrap_or_else(|_| format!("sleep {secs}"));
    ToolCallRequest {
        id: format!("probe-{i}"),
        name: "bash".to_string(),
        arguments: serde_json::json!({ "command": command }).to_string(),
    }
}

fn run_one(executor: &RealToolExecutor, cancel: &CancelToken, i: usize, secs: f64) -> bool {
    let request = call(i, secs);
    let mut sink = |_: ToolProgress<'_>| {};
    executor.execute(&request, cancel, &mut sink).ok
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("seq");
    let calls: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let secs: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let workers: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(4);

    let executor = RealToolExecutor::new();
    let cancel = CancelToken::new();
    // `PROBE_CANCEL_MS=N` trips the shared token after N ms, the shape an Esc
    // takes: every call must stop, whichever thread it runs on.
    if let Some(ms) = std::env::var("PROBE_CANCEL_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    {
        let token = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(ms));
            token.cancel();
        });
    }
    let failed = AtomicUsize::new(0);

    let started = Instant::now();

    match mode {
        "seq" => {
            for i in 0..calls {
                if !run_one(&executor, &cancel, i, secs) {
                    failed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        "threads" => {
            std::thread::scope(|scope| {
                for i in 0..calls {
                    let executor = &executor;
                    let cancel = &cancel;
                    let failed = &failed;
                    scope.spawn(move || {
                        if !run_one(executor, cancel, i, secs) {
                            failed.fetch_add(1, Ordering::Relaxed);
                        }
                    });
                }
            });
        }
        "pool" => {
            let next = AtomicUsize::new(0);
            std::thread::scope(|scope| {
                for _ in 0..workers.min(calls) {
                    let executor = &executor;
                    let cancel = &cancel;
                    let next = &next;
                    let failed = &failed;
                    scope.spawn(move || {
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            if i >= calls {
                                break;
                            }
                            if !run_one(executor, cancel, i, secs) {
                                failed.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    });
                }
            });
        }
        other => panic!("unknown mode {other:?} (seq|threads|pool)"),
    }

    println!(
        "mode={mode} calls={calls} sleep={secs}s workers={workers} failed={} wall={:.3}s",
        failed.load(Ordering::Relaxed),
        started.elapsed().as_secs_f64()
    );
}
