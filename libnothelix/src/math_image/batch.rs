use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::BATCH_SEP;
use crate::error::{Error, Result};

const SUBJECT: &str = "math render jobs";
const ABANDONED_AFTER: Duration = Duration::from_secs(60);

pub(crate) type RenderBlock = fn(String, isize, String) -> String;

struct BatchJob {
    started: Instant,
    results: Option<String>,
}

fn jobs() -> &'static Mutex<HashMap<u64, BatchJob>> {
    static JOBS: OnceLock<Mutex<HashMap<u64, BatchJob>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_job_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub(crate) enum PollReply {
    Pending,
    Ready(String),
}

impl fmt::Display for PollReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => f.write_str("PENDING"),
            Self::Ready(results) => f.write_str(results),
        }
    }
}

pub(crate) fn compile_in_parallel(
    blocks: &str,
    font_size_pt: isize,
    text_color: &str,
    render_block: RenderBlock,
) -> String {
    use rayon::prelude::*;

    blocks
        .split(BATCH_SEP)
        .collect::<Vec<_>>()
        .par_iter()
        .map(|block| render_block((*block).to_string(), font_size_pt, text_color.to_string()))
        .collect::<Vec<String>>()
        .join(&BATCH_SEP.to_string())
}

pub(crate) fn spawn(
    blocks: String,
    font_size_pt: isize,
    text_color: String,
    render_block: RenderBlock,
) -> Result<String> {
    let job_id = next_job_id();
    let started = Instant::now();

    let mut registry = jobs()
        .lock()
        .map_err(|_| Error::LockPoisoned { subject: SUBJECT })?;
    registry.retain(|_, job| started.duration_since(job.started) < ABANDONED_AFTER);
    registry.insert(
        job_id,
        BatchJob {
            started,
            results: None,
        },
    );
    drop(registry);

    std::thread::spawn(move || {
        let joined = compile_in_parallel(&blocks, font_size_pt, &text_color, render_block);
        if let Ok(mut registry) = jobs().lock() {
            registry.insert(
                job_id,
                BatchJob {
                    started,
                    results: Some(joined),
                },
            );
        }
    });

    Ok(job_id.to_string())
}

pub(crate) fn poll(job_id: &str) -> Result<PollReply> {
    let trimmed = job_id.trim();
    let id = trimmed.parse::<u64>().map_err(|_| Error::Malformed {
        subject: SUBJECT,
        detail: format!("`{trimmed}` is not a job id"),
    })?;
    let mut registry = jobs()
        .lock()
        .map_err(|_| Error::LockPoisoned { subject: SUBJECT })?;
    match registry.remove(&id) {
        Some(BatchJob {
            results: Some(results),
            ..
        }) => Ok(PollReply::Ready(results)),
        Some(pending) => {
            registry.insert(id, pending);
            Ok(PollReply::Pending)
        }
        None => Err(Error::Malformed {
            subject: SUBJECT,
            detail: format!("job {id} expired or was already collected"),
        }),
    }
}
