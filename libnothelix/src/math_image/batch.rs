use std::collections::HashMap;
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
    BadJobId,
    Expired,
    LockPoisoned,
}

impl PollReply {
    pub(crate) fn into_string(self) -> String {
        match self {
            Self::Pending => "PENDING".to_string(),
            Self::Ready(results) => results,
            Self::BadJobId => "ERROR:bad-job-id".to_string(),
            Self::Expired => "ERROR:expired".to_string(),
            Self::LockPoisoned => "ERROR:lock-poisoned".to_string(),
        }
    }
}

pub(crate) fn compile_in_parallel(
    blocks: String,
    font_size_pt: isize,
    text_color: String,
    render_block: RenderBlock,
) -> String {
    use rayon::prelude::*;

    blocks
        .split(BATCH_SEP)
        .collect::<Vec<_>>()
        .par_iter()
        .map(|block| render_block((*block).to_string(), font_size_pt, text_color.clone()))
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
        let joined = compile_in_parallel(blocks, font_size_pt, text_color, render_block);
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

pub(crate) fn poll(job_id: &str) -> PollReply {
    let Ok(id) = job_id.trim().parse::<u64>() else {
        return PollReply::BadJobId;
    };
    let Ok(mut registry) = jobs().lock() else {
        return PollReply::LockPoisoned;
    };
    match registry.remove(&id) {
        Some(BatchJob {
            results: Some(results),
            ..
        }) => PollReply::Ready(results),
        Some(pending) => {
            registry.insert(id, pending);
            PollReply::Pending
        }
        None => PollReply::Expired,
    }
}
