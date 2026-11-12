//! `zeroth -p --jobs FILE`: several headless sessions in one process
//! (`docs/cli/batch.md`).
//!
//! The first client of the multi-threaded core
//! (`docs/architecture/multithread-core.md`). Each job is the same
//! [`headless::run`] a `-p` invocation runs, on its own OS thread with its own
//! current-thread runtime. What the jobs share is the config home — read-mostly,
//! with the first-run seeds written atomically — and stdout, which each job
//! reaches through its own thread-local reporter sink, so the process never
//! interleaves two sessions' output.
//!
//! Admission is the explicit `--concurrency` budget, not the job count: a fixed
//! number of workers pull from one queue. That is the knob the measurements in
//! the design note recommend, and it keeps the file-descriptor ceiling
//! predictable under the default 256-fd soft limit.

use std::collections::{HashSet, VecDeque};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use zeroth::cli::PrintOptions;
use zeroth::headless::Emit;
use zeroth::permission::PermissionMode;

use super::headless::{self, JobOutcome};
use super::startup::Startup;

/// How many jobs run at once when `--concurrency` is absent. Four keeps the
/// fd use well under the default soft limit (each session's runtime owns a
/// kqueue/epoll descriptor) while still overlapping network-bound turns.
const DEFAULT_CONCURRENCY: usize = 4;

/// One line of the jobs file, after validation.
#[derive(Debug)]
struct Job {
    /// Its position in the file, 1-based — the log file's prefix and the
    /// fallback id.
    index: usize,
    id: String,
    prompt: String,
}

#[derive(serde::Deserialize)]
struct JobLine {
    id: Option<String>,
    prompt: String,
}

/// What the batch writes on stdout per finished job. `exit_code` is the
/// job's own status; a result object with a non-zero code is a completed
/// run that failed, not a missing one.
#[derive(Debug, serde::Serialize)]
struct JobReport {
    id: String,
    exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    num_turns: usize,
    usage: zeroth::headless::RunUsage,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    log: Option<String>,
}

impl JobReport {
    fn new(job: &Job, outcome: JobOutcome, log: Option<PathBuf>) -> Self {
        Self {
            id: job.id.clone(),
            exit_code: outcome.exit_code,
            session_id: outcome.session_id,
            result: outcome.result,
            num_turns: outcome.num_turns,
            usage: outcome.usage,
            duration_ms: outcome.duration_ms,
            log: log.map(|path| path.display().to_string()),
        }
    }
}

/// Run the jobs named by `--jobs` and return the process's exit status: 0
/// when every job exited 0, the first failing job's status otherwise, or the
/// signal's status when a Ctrl+C stopped the batch.
pub(crate) fn run(startup: Startup) -> i32 {
    let options = startup.print.clone().unwrap_or_default();
    let Some(jobs_file) = options.jobs.clone() else {
        eprintln!("zeroth: --jobs was not set");
        return 2;
    };
    let jobs = match read_jobs(&jobs_file) {
        Ok(jobs) => jobs,
        Err(err) => {
            eprintln!("zeroth: {err}");
            return 2;
        }
    };
    // The one-time pre-fork home migration is a shared write. Run it once,
    // before any session bootstraps; every session's own call then sees the
    // destination and no-ops. A Host will own this permanently.
    let _ = super::config::migrate_from_alter_zero();
    let out_dir = match options.out_dir.as_deref().map(ensure_dir) {
        Some(Ok(dir)) => Some(dir),
        Some(Err(err)) => {
            eprintln!("zeroth: {err}");
            return 2;
        }
        None => None,
    };
    let workers = options
        .concurrency
        .map_or(DEFAULT_CONCURRENCY, std::num::NonZeroUsize::get)
        .max(1)
        .min(jobs.len());
    let queue = Mutex::new(VecDeque::from(jobs));
    let (tx, rx) = std::sync::mpsc::channel::<JobReport>();
    let mut exit = 0;
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let tx = tx.clone();
            let queue = &queue;
            let options = &options;
            let out_dir = out_dir.as_deref();
            let permission_mode = startup.permission_mode;
            scope.spawn(move || {
                loop {
                    let job = {
                        let mut queue = queue.lock().expect("the job queue");
                        queue.pop_front()
                    };
                    let Some(job) = job else {
                        break;
                    };
                    let report = run_job(job, options, out_dir, permission_mode);
                    let interrupted = matches!(report.exit_code, 130 | 143);
                    let _ = tx.send(report);
                    // The batch shares one process-level signal with every
                    // session; once one run has stopped on SIGINT/SIGTERM,
                    // starting another job would just be interrupted too.
                    if interrupted {
                        queue.lock().expect("the job queue").clear();
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut stdout = std::io::stdout().lock();
        for report in rx {
            if report.exit_code != 0 && exit == 0 {
                exit = report.exit_code;
            }
            match serde_json::to_string(&report) {
                Ok(line) => {
                    let _ = writeln!(stdout, "{line}");
                    let _ = stdout.flush();
                }
                Err(err) => eprintln!("zeroth: could not serialize a job result: {err}"),
            }
        }
    });
    exit
}

/// Run one job on this worker's thread: its own runtime and session, its
/// reporter output captured instead of written to the process's streams.
fn run_job(
    job: Job,
    options: &PrintOptions,
    out_dir: Option<&Path>,
    permission_mode: Option<PermissionMode>,
) -> JobReport {
    let captured = std::sync::Arc::new(Mutex::new(String::new()));
    let sink_capture = captured.clone();
    let sink: headless::OutputSink = Box::new(move |emits: &[Emit]| {
        let mut text = sink_capture.lock().expect("the job's output buffer");
        for emit in emits {
            let line = match emit {
                Emit::Stdout(line) | Emit::Stderr(line) => line,
            };
            text.push_str(line);
            text.push('\n');
        }
    });
    let guard = headless::capture_output(sink);
    let job_options = PrintOptions {
        jobs: None,
        concurrency: None,
        out_dir: None,
        ..options.clone()
    };
    let startup = Startup {
        session: None,
        prompt: Some(job.prompt.clone()),
        permission_mode,
        print: Some(job_options),
    };
    let began = Instant::now();
    let outcome = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(headless::run_job(startup)),
        Err(err) => {
            eprintln!("zeroth: could not start a job runtime: {err}");
            JobOutcome::failed(began)
        }
    };
    drop(guard);
    let log = out_dir.and_then(|dir| {
        let path = dir.join(log_name(job.index, &job.id));
        match std::fs::write(&path, captured.lock().expect("the job's output").as_str()) {
            Ok(()) => Some(path),
            Err(err) => {
                eprintln!("zeroth: could not write {}: {err}", path.display());
                None
            }
        }
    });
    JobReport::new(&job, outcome, log)
}

/// Read and validate the JSONL jobs file: one object per line, `prompt`
/// required, `id` optional. Blank lines are skipped; duplicate ids are
/// refused because the log file and the result stream both name jobs by id.
fn read_jobs(path: &Path) -> Result<Vec<Job>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("could not read {}: {err}", path.display()))?;
    let mut jobs = Vec::new();
    let mut seen = HashSet::new();
    for (offset, line) in text.lines().enumerate() {
        let number = offset + 1;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: JobLine = serde_json::from_str(line)
            .map_err(|err| format!("{}:{number}: {err}", path.display()))?;
        if parsed.prompt.trim().is_empty() {
            return Err(format!(
                "{}:{number}: prompt must not be empty",
                path.display()
            ));
        }
        let id = parsed
            .id
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| format!("job-{number}"));
        if !seen.insert(id.clone()) {
            return Err(format!(
                "{}:{number}: duplicate job id \"{id}\"",
                path.display()
            ));
        }
        jobs.push(Job {
            index: jobs.len() + 1,
            id,
            prompt: parsed.prompt,
        });
    }
    if jobs.is_empty() {
        return Err(format!("{} has no jobs", path.display()));
    }
    Ok(jobs)
}

fn ensure_dir(path: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(path)
        .map_err(|err| format!("could not create {}: {err}", path.display()))?;
    Ok(path.to_path_buf())
}

/// A log file name that is safe on every filesystem: the job's file position
/// keeps names unique, and anything but a conservative identifier becomes
/// `_`.
fn log_name(index: usize, id: &str) -> String {
    let mut safe: String = id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        safe.push_str("job");
    }
    format!("{index:03}-{safe}.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jobs_file(contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zeroth-jobs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("jobs.jsonl");
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn a_jobs_file_needs_a_prompt_and_unique_ids() {
        let path = jobs_file("{\"prompt\":\"one\"}\n\n{\"id\":\"b\",\"prompt\":\"two\"}\n");
        let jobs = read_jobs(&path).unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, "job-1", "a missing id falls back to its line");
        assert_eq!(jobs[1].id, "b");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();

        let path = jobs_file("{\"prompt\":\"\"}\n");
        assert!(read_jobs(&path).unwrap_err().contains("must not be empty"));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();

        let path = jobs_file("{\"id\":\"a\",\"prompt\":\"x\"}\n{\"id\":\"a\",\"prompt\":\"y\"}\n");
        assert!(read_jobs(&path).unwrap_err().contains("duplicate job id"));
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_log_name_keeps_jobs_apart() {
        assert_eq!(log_name(3, "a/b"), "003-a_b.log");
        assert_eq!(log_name(1, ""), "001-job.log");
    }
}
