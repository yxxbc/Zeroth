//! Temporary experiment: N headless sessions in one process under three
//! concurrency models. Not part of the product; deleted after the run.
#![allow(dead_code)]

use std::sync::mpsc;
use std::time::Instant;

use zeroth::cli::PrintOptions;

#[path = "../tui/mod.rs"]
mod tui;

use tui::startup::Startup;

fn startup() -> Startup {
    Startup {
        session: None,
        prompt: Some("hello".to_string()),
        permission_mode: None,
        print: Some(PrintOptions::default()),
    }
}

fn run_one() -> i32 {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(tui::headless::run(startup()))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("thread");
    let n: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(4);
    let workers: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(n);
    let began = Instant::now();
    match mode {
        "serial" => {
            for _ in 0..n {
                let _ = run_one();
            }
        }
        "thread" => {
            let handles: Vec<_> = (0..n).map(|_| std::thread::spawn(run_one)).collect();
            for handle in handles {
                let _ = handle.join();
            }
        }
        "pool" => {
            let (tx, rx) = mpsc::channel::<()>();
            let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
            let mut handles = Vec::new();
            for _ in 0..workers {
                let rx = rx.clone();
                handles.push(std::thread::spawn(move || {
                    while rx.lock().unwrap().recv().is_ok() {
                        let _ = run_one();
                    }
                }));
            }
            for _ in 0..n {
                tx.send(()).unwrap();
            }
            drop(tx);
            for handle in handles {
                let _ = handle.join();
            }
        }
        "async" => {
            // `Session` is !Send (the syntect/onig handle), so a session cannot
            // be a task on a multi-thread runtime. The remaining async shape is
            // tokio's blocking pool: one blocking task per session.
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(workers)
                .enable_all()
                .build()
                .expect("runtime");
            rt.block_on(async {
                let mut set = tokio::task::JoinSet::new();
                for _ in 0..n {
                    set.spawn_blocking(run_one);
                }
                while set.join_next().await.is_some() {}
            });
        }
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    }
    println!("elapsed {:.3}", began.elapsed().as_secs_f64());
}
