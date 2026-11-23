//! Scheduled jobs with catch-up (port of routes/console.php).
//!
//! Each job runs at its set time, or as soon as possible afterwards if the computer was asleep or the app was
//! closed then. A background thread checks every 5 minutes. The last run is stamped in the `cache` table under
//! `schedule:last-run:<command>`; the first check only stamps (no burst of jobs on first launch). A job never
//! overlaps itself (the lock expires after 120 minutes, like `withoutOverlapping(120)`).
//!
//! ```ignore
//! // In your area module's register_jobs() (called once at startup):
//! crate::scheduler::register_job("venues:refresh", |ctx| {
//!     // Blocking is fine here: each job runs on its own thread.
//!     refresh_all(ctx)?;
//!     Ok(())
//! });
//! ```
//! Known commands take their schedule from settings (`schedule.*`, defaults from config/tracker.php):
//! `opportunities:scan` daily at `schedule.scan_time`; `venues:refresh` weekly on `schedule.venues_day` at
//! `schedule.venues_time`; `publications:refresh` weekly on `schedule.citations_day` at `schedule.citations_time`.
//! Use [`register_job_at`] for anything else.

use crate::api::Ctx;
use crate::error::Result;
use crate::models::setting;
use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, NaiveTime, TimeZone};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub type JobFn = Arc<dyn Fn(&Ctx) -> Result<()> + Send + Sync>;

#[derive(Clone)]
struct Job {
    command: String,
    time_key: String,
    day_key: Option<String>,
    run: JobFn,
}

/// How often the scheduler checks for due jobs.
pub const CHECK_EVERY_SECS: u64 = 300;
/// Delay before the first check after launch (let the window load first).
pub const FIRST_CHECK_SECS: u64 = 45;
/// Overlap lock expiry.
pub const LOCK_MINUTES: u64 = 120;

fn jobs() -> &'static Mutex<Vec<Job>> {
    static JOBS: OnceLock<Mutex<Vec<Job>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(Vec::new()))
}

fn running() -> &'static Mutex<HashMap<String, Instant>> {
    static RUNNING: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    RUNNING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Settings keys for the known commands: (time key, optional weekday key).
/// Settings prefix of the per-job options (`schedule.<p>_enabled`, `schedule.<p>_frequency` = daily | weekly,
/// `schedule.<p>_day`, `schedule.<p>_time`) for the three AI jobs.
pub fn job_prefix(command: &str) -> Option<&'static str> {
    match command {
        "opportunities:scan" => Some("scan"),
        "venues:refresh" => Some("venues"),
        "publications:refresh" => Some("citations"),
        _ => None,
    }
}

fn schedule_keys(command: &str) -> (&'static str, Option<&'static str>) {
    match command {
        "opportunities:scan" => ("schedule.scan_time", Some("schedule.scan_day")),
        "venues:refresh" => ("schedule.venues_time", Some("schedule.venues_day")),
        "publications:refresh" => ("schedule.citations_time", Some("schedule.citations_day")),
        _ => ("schedule.scan_time", None),
    }
}

/// Register a job for a known command (schedule from settings). Re-registering replaces it.
pub fn register_job(command: &str, f: impl Fn(&Ctx) -> Result<()> + Send + Sync + 'static) {
    let (time_key, day_key) = schedule_keys(command);
    register_job_at(command, time_key, day_key, f);
}

/// Register a job whose time (`HH:MM`) and optional weekday (0 = Sunday) come from the given setting keys.
pub fn register_job_at(command: &str, time_key: &str, day_key: Option<&str>, f: impl Fn(&Ctx) -> Result<()> + Send + Sync + 'static) {
    let mut list = jobs().lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|j| j.command != command);
    list.push(Job { command: command.into(), time_key: time_key.into(), day_key: day_key.map(Into::into), run: Arc::new(f) });
}

/// Registered command names.
pub fn commands() -> Vec<String> {
    jobs().lock().unwrap_or_else(|e| e.into_inner()).iter().map(|j| j.command.clone()).collect()
}

pub fn is_running(command: &str) -> bool {
    running().lock().unwrap_or_else(|e| e.into_inner()).get(command).is_some_and(|t| t.elapsed().as_secs() < LOCK_MINUTES * 60)
}

fn stamp_key(command: &str) -> String {
    format!("schedule:last-run:{command}")
}

fn ran_key(command: &str) -> String {
    format!("schedule:last-started:{command}")
}

/// When a job last actually started (local time). Unlike `last_run`, the first-check bookkeeping does not set it.
pub fn last_started(ctx: &Ctx, command: &str) -> Option<DateTime<FixedOffset>> {
    ctx.db.cache_get(&ran_key(command)).ok().flatten().and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
}

/// When a job last started (local time), if ever.
pub fn last_run(ctx: &Ctx, command: &str) -> Option<DateTime<FixedOffset>> {
    ctx.db.cache_get(&stamp_key(command)).ok().flatten().and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
}

/// The most recent scheduled slot at or before `now`.
pub fn last_slot(now: DateTime<Local>, time: &str, weekday: Option<u32>) -> DateTime<Local> {
    let mut parts = time.split(':').map(|p| p.trim().parse::<u32>().unwrap_or(0));
    let (h, m) = (parts.next().unwrap_or(0).min(23), parts.next().unwrap_or(0).min(59));
    let t = NaiveTime::from_hms_opt(h, m, 0).unwrap_or_default();
    let at = |date: chrono::NaiveDate| Local.from_local_datetime(&date.and_time(t)).earliest().unwrap_or(now);
    let mut date = now.date_naive();
    if let Some(wd) = weekday {
        let back = (now.weekday().num_days_from_sunday() as i64 - (wd % 7) as i64 + 7) % 7;
        date -= Duration::days(back);
    }
    let mut slot = at(date);
    if slot > now {
        date -= Duration::days(if weekday.is_some() { 7 } else { 1 });
        slot = at(date);
    }
    slot
}

/// Catch-up rule: due when the last run started before the most recent slot. `None` (never run) is not due.
pub fn is_due(now: DateTime<Local>, time: &str, weekday: Option<u32>, last: Option<DateTime<FixedOffset>>) -> bool {
    match last {
        Some(last) => last < last_slot(now, time, weekday),
        None => false,
    }
}

fn start_job(ctx: &Ctx, job: Job) -> bool {
    {
        let mut map = running().lock().unwrap_or_else(|e| e.into_inner());
        if map.get(&job.command).is_some_and(|t| t.elapsed().as_secs() < LOCK_MINUTES * 60) {
            return false;
        }
        map.insert(job.command.clone(), Instant::now());
    }
    let started_at = Local::now().to_rfc3339();
    let _ = ctx.db.cache_put(&stamp_key(&job.command), &started_at);
    let _ = ctx.db.cache_put(&ran_key(&job.command), &started_at);
    ctx.emit("scheduler", json!({ "command": job.command, "status": "started" }));
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (job.run)(&ctx)));
        running().lock().unwrap_or_else(|e| e.into_inner()).remove(&job.command);
        let (status, error) = match result {
            Ok(Ok(())) => ("finished", None),
            Ok(Err(e)) => ("failed", Some(e.to_string())),
            Err(_) => ("failed", Some("The job stopped unexpectedly.".to_string())),
        };
        ctx.emit("scheduler", json!({ "command": job.command, "status": status, "error": error }));
    });
    true
}

/// Check every job once; start the due ones. Returns the commands started.
pub fn tick(ctx: &Ctx) -> Vec<String> {
    let list = jobs().lock().unwrap_or_else(|e| e.into_inner()).clone();
    let now = Local::now();
    let mut started = Vec::new();
    for job in list {
        let time = setting::get_str(&ctx.db, &job.time_key);
        // Settings -> General: each AI job can be switched off, and run daily or weekly.
        let (enabled, weekly) = match job_prefix(&job.command) {
            Some(p) => (
                setting::get_bool(&ctx.db, &format!("schedule.{p}_enabled")),
                setting::get_str(&ctx.db, &format!("schedule.{p}_frequency")) == "weekly",
            ),
            None => (true, job.day_key.is_some()),
        };
        let day = job.day_key.as_deref().filter(|_| weekly).and_then(|k| setting::get_i64(&ctx.db, k)).map(|d| d.rem_euclid(7) as u32);
        let last = last_run(ctx, &job.command);
        if last.is_none() {
            // First run: start counting from now instead of running everything at once.
            let _ = ctx.db.cache_put(&stamp_key(&job.command), &now.to_rfc3339());
            continue;
        }
        if !enabled {
            continue;
        }
        if is_due(now, &time, day, last) && start_job(ctx, job.clone()) {
            started.push(job.command);
        }
    }
    started
}

/// Run jobs now regardless of their schedule (tray "Run scans now"). `None` = every registered job.
pub fn run_now(ctx: &Ctx, command: Option<&str>) -> Vec<String> {
    let list = jobs().lock().unwrap_or_else(|e| e.into_inner()).clone();
    list.into_iter()
        .filter(|j| command.is_none_or(|c| c == j.command))
        .filter_map(|j| {
            let name = j.command.clone();
            start_job(ctx, j).then_some(name)
        })
        .collect()
}

/// Start the background checker (once).
pub fn start(ctx: Ctx) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("scheduler".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(FIRST_CHECK_SECS));
            loop {
                tick(&ctx);
                std::thread::sleep(std::time::Duration::from_secs(CHECK_EVERY_SECS));
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(s: &str) -> DateTime<Local> {
        Local.from_local_datetime(&chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()).earliest().unwrap()
    }

    fn fixed(s: &str) -> Option<DateTime<FixedOffset>> {
        Some(local(s).fixed_offset())
    }

    #[test]
    fn daily_catch_up() {
        // 2026-11-22 is a Sunday.
        let now = local("2026-11-22 09:00");
        assert_eq!(last_slot(now, "06:00", None), local("2026-11-22 06:00"));
        assert_eq!(last_slot(local("2026-11-22 05:00"), "06:00", None), local("2026-11-21 06:00"));
        assert!(is_due(now, "06:00", None, fixed("2026-11-21 06:01")));
        assert!(!is_due(now, "06:00", None, fixed("2026-11-22 06:01")));
        // Asleep for days: one catch-up run.
        assert!(is_due(now, "06:00", None, fixed("2026-11-18 06:00")));
        assert!(!is_due(now, "06:00", None, None));
    }

    #[test]
    fn weekly_catch_up() {
        let now = local("2026-11-22 09:00"); // Sunday
                                             // Monday 07:00 -> last slot is Monday 16 Nov.
        assert_eq!(last_slot(now, "07:00", Some(1)), local("2026-11-16 07:00"));
        // Sunday 07:00 -> today.
        assert_eq!(last_slot(now, "07:00", Some(0)), local("2026-11-22 07:00"));
        // Sunday 10:00 -> not yet, so last week.
        assert_eq!(last_slot(now, "10:00", Some(0)), local("2026-11-15 10:00"));
        assert!(is_due(now, "07:00", Some(3), fixed("2026-11-17 12:00"))); // Wednesday 18 Nov passed
        assert!(!is_due(now, "07:00", Some(3), fixed("2026-11-18 07:30")));
    }

    #[test]
    fn registry_and_overlap() {
        let ctx = Ctx::for_tests(crate::db::Db::open_in_memory().unwrap());
        register_job("test:job", |_| {
            std::thread::sleep(std::time::Duration::from_millis(200));
            Ok(())
        });
        assert!(commands().contains(&"test:job".to_string()));
        assert_eq!(run_now(&ctx, Some("test:job")), vec!["test:job".to_string()]);
        assert!(is_running("test:job"));
        assert!(run_now(&ctx, Some("test:job")).is_empty());
        assert!(last_run(&ctx, "test:job").is_some());
    }
}
