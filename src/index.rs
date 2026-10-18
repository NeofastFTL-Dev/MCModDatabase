use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::api::{self, ModHit};
use crate::catalog;

static RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug)]
pub struct Progress {
    pub message: String,
    pub running: bool,
    pub mr: u64,
    pub cf: u64,
}

pub enum Event {
    Progress(Progress),
    Done(String),
}

struct RateLimit {
    min_gap: Duration,
    next: Mutex<Instant>,
}

impl RateLimit {
    fn new(per_sec: f64) -> Self {
        Self {
            min_gap: Duration::from_secs_f64(1.0 / per_sec.max(0.5)),
            next: Mutex::new(Instant::now()),
        }
    }

    fn wait(&self) {
        let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if now < *next {
            let sleep = *next - now;
            *next += self.min_gap;
            drop(next);
            thread::sleep(sleep);
        } else {
            *next = now + self.min_gap;
        }
    }
}

fn mr_limit() -> &'static RateLimit {
    static L: OnceLock<RateLimit> = OnceLock::new();
    L.get_or_init(|| RateLimit::new(5.0))
}

fn cf_limit() -> &'static RateLimit {
    static L: OnceLock<RateLimit> = OnceLock::new();
    L.get_or_init(|| RateLimit::new(8.0))
}

struct IndexBatch {
    source: String,
    class_or_type: String,
    category: String,
    offset: u32,
    hits: Vec<ModHit>,
    finished: bool,
    label: String,
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

pub fn start(tx: Sender<Event>, ctx: egui::Context, cf_key: String, force: bool) {
    if RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    thread::spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(|| run(&tx, &ctx, &cf_key, force)));
        RUNNING.store(false, Ordering::SeqCst);
        catalog::mark_idle();
        let stats = catalog::stats();
        let msg = match result {
            Ok(Ok(skipped)) if skipped => {
                "Catalog is fresh (under 20 hours). Force refresh to rebuild.".into()
            }
            Ok(Ok(_)) => format!(
                "Catalog ready: {} Modrinth · {} CurseForge",
                stats.modrinth, stats.curseforge
            ),
            Ok(Err(e)) => format!("Catalog index failed: {e}"),
            Err(_) => "Catalog index crashed. Check error.log.".into(),
        };
        let _ = tx.send(Event::Done(msg));
        ctx.request_repaint();
    });
}

fn push(out: &SyncSender<IndexBatch>, batch: IndexBatch) {
    let _ = out.send(batch);
}

fn emit(tx: &Sender<Event>, ctx: &egui::Context, message: String, mr: u64, cf: u64) {
    let _ = tx.send(Event::Progress(Progress {
        message,
        running: true,
        mr,
        cf,
    }));
    ctx.request_repaint();
}

fn retryable(err: &str) -> bool {
    err.contains("HTTP 429")
        || err.contains("HTTP 500")
        || err.contains("HTTP 502")
        || err.contains("HTTP 503")
        || err.contains("HTTP 504")
        || err.contains("network:")
}

fn retry<T>(mut f: impl FnMut() -> anyhow::Result<T>) -> anyhow::Result<T> {
    let mut delay = Duration::from_millis(400);
    for attempt in 0..8 {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                let msg = e.to_string();
                if !retryable(&msg) || attempt == 7 {
                    return Err(e);
                }
                thread::sleep(delay);
                delay = (delay * 2).min(Duration::from_secs(16));
            }
        }
    }
    unreachable!()
}

fn parallel_fetch<J: Send>(n: usize, jobs: Vec<J>, f: impl Fn(J) + Sync) {
    if jobs.is_empty() {
        return;
    }
    let n = n.min(jobs.len()).max(1);
    let jobs = Mutex::new(jobs.into_iter());
    thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| loop {
                let job = jobs.lock().unwrap_or_else(|e| e.into_inner()).next();
                match job {
                    Some(j) => {
                        let _ = catch_unwind(AssertUnwindSafe(|| f(j)));
                    }
                    None => break,
                }
            });
        }
    });
}

fn page_jobs(mut offset: u32, cap: u32, step: u32) -> Vec<u32> {
    let mut jobs = Vec::new();
    if step == 0 || offset >= cap {
        return jobs;
    }
    while offset < cap {
        jobs.push(offset);
        offset = offset.saturating_add(step);
        if jobs.len() >= 2_000 {
            break;
        }
    }
    jobs
}

fn mr_browse(ptype: &str, offset: u32) -> anyhow::Result<(Vec<ModHit>, u64)> {
    mr_limit().wait();
    retry(|| api::browse_modrinth(ptype, offset, 100))
}

fn cf_browse(
    key: &str,
    ptype: &str,
    cat: Option<i32>,
    offset: u32,
) -> anyhow::Result<(Vec<ModHit>, u64)> {
    cf_limit().wait();
    retry(|| api::browse_curseforge(key, ptype, cat, offset, 50))
}

fn run(
    tx: &Sender<Event>,
    ctx: &egui::Context,
    cf_key: &str,
    force: bool,
) -> anyhow::Result<bool> {
    if !catalog::can_refresh(force) {
        return Ok(true);
    }
    let conn = catalog::open()?;
    catalog::begin_bulk(&conn)?;
    if force {
        catalog::reset_index_state(&conn)?;
    }

    let seed = catalog::stats();
    let mut mr = if force { 0 } else { seed.modrinth };
    let mut cf = if force { 0 } else { seed.curseforge };
    emit(
        tx,
        ctx,
        "Indexing Modrinth + CurseForge…".into(),
        mr,
        cf,
    );

    let (batch_tx, batch_rx) = mpsc::sync_channel::<IndexBatch>(8);
    let mut seen: HashSet<String> = HashSet::new();
    let mut last_emit = Instant::now();
    let mut write_err = None;

    thread::scope(|s| {
        {
            let batch_tx = batch_tx.clone();
            s.spawn(move || {
                let _ = catch_unwind(AssertUnwindSafe(|| index_modrinth(batch_tx, force)));
            });
        }
        if cf_key.trim().is_empty() {
            emit(
                tx,
                ctx,
                "CurseForge skipped - add an API key in Settings.".into(),
                mr,
                cf,
            );
        } else {
            let batch_tx = batch_tx.clone();
            s.spawn(move || {
                let _ = catch_unwind(AssertUnwindSafe(|| index_curseforge(cf_key, batch_tx, force)));
            });
        }
        drop(batch_tx);

        while let Ok(batch) = batch_rx.recv() {
            if write_err.is_some() {
                continue;
            }
            let processed = catch_unwind(AssertUnwindSafe(|| {
                let mut new_hits = Vec::with_capacity(batch.hits.len());
                for h in batch.hits {
                    let k = format!("{}:{}", h.source, h.source_id);
                    if seen.insert(k) {
                        new_hits.push(h);
                    }
                }
                if !new_hits.is_empty() {
                    catalog::upsert_mods(&conn, &new_hits)?;
                    let n = new_hits.len() as u64;
                    if batch.source == "modrinth" {
                        mr += n;
                    } else {
                        cf += n;
                    }
                }
                if batch.finished {
                    let _ = catalog::set_state(
                        &conn,
                        &batch.source,
                        &batch.class_or_type,
                        &batch.category,
                        batch.offset,
                        true,
                    );
                }
                if last_emit.elapsed() >= Duration::from_millis(200) {
                    emit(tx, ctx, batch.label.clone(), mr, cf);
                    last_emit = Instant::now();
                }
                Ok::<(), anyhow::Error>(())
            }));
            match processed {
                Ok(Ok(())) => {}
                Ok(Err(e)) => write_err = Some(e),
                Err(_) => {
                    write_err = Some(anyhow::anyhow!("catalog writer panicked"));
                }
            }
        }
    });
    if let Some(e) = write_err {
        let _ = catalog::end_bulk(&conn);
        return Err(e);
    }

    emit(tx, ctx, "Rebuilding search index…".into(), mr, cf);
    catalog::rebuild_fts(&conn)?;
    catalog::end_bulk(&conn)?;
    catalog::mark_completed(&conn)?;
    Ok(false)
}

fn index_modrinth(out: SyncSender<IndexBatch>, force: bool) {
    let Ok(conn) = catalog::open_reader() else {
        return;
    };
    for ptype in api::PROJECT_TYPES {
        let (_, done) = catalog::get_state(&conn, "modrinth", ptype, "");
        if done && !force {
            continue;
        }
        let (hits, total) = match mr_browse(ptype, 0) {
            Ok(v) => v,
            Err(e) => {
                push(&out, IndexBatch {
                    source: "modrinth".into(),
                    class_or_type: ptype.to_string(),
                    category: String::new(),
                    offset: 0,
                    hits: Vec::new(),
                    finished: false,
                    label: format!("Modrinth {ptype}: {e}"),
                });
                continue;
            }
        };
        let n = hits.len() as u32;
        let cap = (total as u32).min(10_000);
        let finished = n < 100 || n >= cap || n == 0;
        push(&out, IndexBatch {
            source: "modrinth".into(),
            class_or_type: ptype.to_string(),
            category: String::new(),
            offset: n,
            hits,
            finished,
            label: format!("Modrinth {ptype}: {n} / {total}"),
        });
        if finished {
            continue;
        }
        let failed = AtomicBool::new(false);
        parallel_fetch(3, page_jobs(n, cap, 100), |offset| {
            match mr_browse(ptype, offset) {
                Ok((hits, total)) => {
                    let got = hits.len() as u32;
                    push(&out, IndexBatch {
                        source: "modrinth".into(),
                        class_or_type: ptype.to_string(),
                        category: String::new(),
                        offset: offset.saturating_add(got),
                        hits,
                        finished: false,
                        label: format!(
                            "Modrinth {ptype}: {} / {total}",
                            offset.saturating_add(got)
                        ),
                    });
                }
                Err(e) => {
                    failed.store(true, Ordering::SeqCst);
                    push(&out, IndexBatch {
                        source: "modrinth".into(),
                        class_or_type: ptype.to_string(),
                        category: String::new(),
                        offset,
                        hits: Vec::new(),
                        finished: false,
                        label: format!("Modrinth {ptype}: {e}"),
                    });
                }
            }
        });
        if failed.load(Ordering::SeqCst) {
            continue;
        }
        push(&out, IndexBatch {
            source: "modrinth".into(),
            class_or_type: ptype.to_string(),
            category: String::new(),
            offset: cap,
            hits: Vec::new(),
            finished: true,
            label: format!("Modrinth {ptype}: {cap} / {total}"),
        });
    }
}

fn index_curseforge(cf_key: &str, out: SyncSender<IndexBatch>, force: bool) {
    let Ok(conn) = catalog::open_reader() else {
        return;
    };
    for ptype in api::PROJECT_TYPES {
        let (_, done) = catalog::get_state(&conn, "curseforge", ptype, "");
        if done && !force {
            continue;
        }
        let (hits, total) = match cf_browse(cf_key, ptype, None, 0) {
            Ok(v) => v,
            Err(e) => {
                push(&out, IndexBatch {
                    source: "curseforge".into(),
                    class_or_type: ptype.to_string(),
                    category: String::new(),
                    offset: 0,
                    hits: Vec::new(),
                    finished: false,
                    label: format!("CurseForge {ptype}: {e}"),
                });
                continue;
            }
        };
        let n = hits.len() as u32;
        let cap = (total as u32).min(10_000);
        let finished = n < 50 || n >= cap || n == 0;
        push(&out, IndexBatch {
            source: "curseforge".into(),
            class_or_type: ptype.to_string(),
            category: String::new(),
            offset: n,
            hits,
            finished,
            label: format!("CurseForge {ptype}: {n} / {total}"),
        });
        if finished {
            continue;
        }
        let failed = AtomicBool::new(false);
        parallel_fetch(4, page_jobs(n, cap, 50), |offset| {
            match cf_browse(cf_key, ptype, None, offset) {
                Ok((hits, total)) => {
                    let got = hits.len() as u32;
                    push(&out, IndexBatch {
                        source: "curseforge".into(),
                        class_or_type: ptype.to_string(),
                        category: String::new(),
                        offset: offset.saturating_add(got),
                        hits,
                        finished: false,
                        label: format!(
                            "CurseForge {ptype}: {} / {total}",
                            offset.saturating_add(got)
                        ),
                    });
                }
                Err(e) => {
                    failed.store(true, Ordering::SeqCst);
                    push(&out, IndexBatch {
                        source: "curseforge".into(),
                        class_or_type: ptype.to_string(),
                        category: String::new(),
                        offset,
                        hits: Vec::new(),
                        finished: false,
                        label: format!("CurseForge {ptype}: {e}"),
                    });
                }
            }
        });
        if failed.load(Ordering::SeqCst) {
            continue;
        }
        push(&out, IndexBatch {
            source: "curseforge".into(),
            class_or_type: ptype.to_string(),
            category: String::new(),
            offset: cap,
            hits: Vec::new(),
            finished: true,
            label: format!("CurseForge {ptype}: {cap} / {total}"),
        });
    }
}
