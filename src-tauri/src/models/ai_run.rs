//! Port of app/Models/AiRun.php: one long AI job with live progress for the sidebar.
//! Every change emits the `ai-runs` event (payload: the run row) so the renderer can update without polling.
//!
//! ```ignore
//! let run = AiRun::begin(&ctx, "venues", "Refreshing venues", 12, Some("/venues"), Some("openai"))?;
//! run.step(&ctx, "Checking CHI 2027", None)?;
//! run.advance(&ctx, Some("Checked CHI 2027"))?;
//! run.finish(&ctx, None)?;           // or Some("error message")
//! ```

use crate::api::Ctx;
use crate::db::{Query, USER_ID};
use crate::error::Result;
use crate::util::now_ts;
use serde_json::{json, Value};

pub const STATUSES: &[&str] = &["running", "done", "failed"];

/// Runs still `running` and untouched for this long are closed as failed (machine slept, app quit).
pub const STALE_MINUTES: i64 = 20;

#[derive(Debug, Clone, Copy)]
pub struct AiRun {
    pub id: i64,
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn tail(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        s.chars().skip(n - max).collect()
    }
}

impl AiRun {
    pub fn begin(ctx: &Ctx, kind: &str, label: &str, total: i64, url: Option<&str>, provider: Option<&str>) -> Result<AiRun> {
        let id = ctx.db.insert(
            "ai_runs",
            &json!({ "user_id": USER_ID, "kind": kind, "label": label, "total": total, "url": url,
                     "step": "Starting…", "provider": provider }),
        )?;
        let run = AiRun { id };
        run.emit(ctx);
        Ok(run)
    }

    pub fn row(&self, ctx: &Ctx) -> Result<Option<Value>> {
        ctx.db.find("ai_runs", self.id)
    }

    fn emit(&self, ctx: &Ctx) {
        if let Ok(Some(row)) = self.row(ctx) {
            ctx.emit("ai-runs", row);
        }
    }

    /// Append a timestamped line to the job's log (keeps the last 20,000 characters).
    pub fn note(&self, ctx: &Ctx, line: &str) -> Result<&Self> {
        let time = chrono::Local::now().format("%H:%M:%S");
        ctx.db.with(|c| {
            let log: Option<String> = c.scalar("SELECT log FROM ai_runs WHERE id = ?1", [self.id])?;
            let log = format!("{}\n{time}  {line}", log.unwrap_or_default());
            let log = tail(log.trim_start(), 20000);
            c.exec("UPDATE ai_runs SET log = ?1, updated_at = ?2 WHERE id = ?3", rusqlite::params![log, now_ts(), self.id])?;
            Ok(())
        })?;
        Ok(self)
    }

    /// Show a new step (and optionally set `done`).
    pub fn step(&self, ctx: &Ctx, step: &str, done: Option<i64>) -> Result<&Self> {
        self.note(ctx, step)?;
        let mut data = json!({ "step": truncate(step, 250) });
        if let Some(d) = done {
            data["done"] = json!(d);
        }
        ctx.db.update("ai_runs", self.id, &data)?;
        self.emit(ctx);
        Ok(self)
    }

    /// `done += 1`, then optionally a new step.
    pub fn advance(&self, ctx: &Ctx, step: Option<&str>) -> Result<&Self> {
        ctx.db.exec("UPDATE ai_runs SET done = done + 1, updated_at = ?1 WHERE id = ?2", rusqlite::params![now_ts(), self.id])?;
        match step {
            Some(s) => self.step(ctx, s, None),
            None => {
                self.emit(ctx);
                Ok(self)
            }
        }
    }

    pub fn finish(&self, ctx: &Ctx, error: Option<&str>) -> Result<&Self> {
        self.note(
            ctx,
            &match error {
                Some(e) => format!("Failed: {e}"),
                None => "Finished".into(),
            },
        )?;
        ctx.db.with(|c| {
            let sql = if error.is_some() {
                "UPDATE ai_runs SET status = 'failed', error = ?1, finished_at = ?2, updated_at = ?2 WHERE id = ?3"
            } else {
                "UPDATE ai_runs SET status = 'done', error = ?1, finished_at = ?2, updated_at = ?2, done = MAX(done, total) WHERE id = ?3"
            };
            c.exec(sql, rusqlite::params![error.map(|e| truncate(e, 600)), now_ts(), self.id])?;
            Ok(())
        })?;
        self.emit(ctx);
        Ok(self)
    }
}

/// Percent complete, or `None` without a total.
pub fn percent(run: &Value) -> Option<i64> {
    let total = run.get("total").and_then(Value::as_i64).unwrap_or(0);
    let done = run.get("done").and_then(Value::as_i64).unwrap_or(0);
    (total > 0).then(|| (((done as f64 / total as f64) * 100.0).round() as i64).min(100))
}

/// Close runs that stopped responding (see [`STALE_MINUTES`]). Returns how many were closed.
pub fn close_stale(ctx: &Ctx) -> Result<usize> {
    let now = now_ts();
    ctx.db.exec(
        "UPDATE ai_runs SET status = 'failed', error = 'Stopped responding.', finished_at = ?1, updated_at = ?1
         WHERE status = 'running' AND updated_at < ?2",
        rusqlite::params![now, crate::util::ts_minutes_ago(STALE_MINUTES)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn run_lifecycle() {
        let ctx = Ctx::for_tests(test_db());
        let run = AiRun::begin(&ctx, "venues", "Refreshing", 3, None, None).unwrap();
        run.advance(&ctx, Some("One")).unwrap();
        run.step(&ctx, "Two", Some(2)).unwrap();
        let row = run.row(&ctx).unwrap().unwrap();
        assert_eq!(row["done"], 2);
        assert_eq!(percent(&row), Some(67));
        run.finish(&ctx, None).unwrap();
        let row = run.row(&ctx).unwrap().unwrap();
        assert_eq!(row["status"], "done");
        assert_eq!(row["done"], 3);
        assert!(row["log"].as_str().unwrap().contains("Finished"));
        assert_eq!(close_stale(&ctx).unwrap(), 0);
    }
}
