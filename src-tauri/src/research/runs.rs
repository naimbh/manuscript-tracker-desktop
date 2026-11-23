//! A research run: an `ai_runs` row (same as the web app) plus live `ai-run` events, token usage, duration
//! and cancellation.
//!
//! Event `ai-run` payload: `{ run_id, kind, status: "running"|"done"|"failed"|"cancelled", step, progress
//! (0–100), message, partial }`. `partial` carries results as they arrive (a venue checked, a call found).
//! The `ai-runs` event with the full row is still emitted by `AiRun` on every change.

use crate::api::Ctx;
use crate::error::{ApiError, Result};
use crate::models::ai_run::{self, AiRun};
use serde_json::{json, Value};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Error text used for a cancelled run.
pub const CANCELLED: &str = "Cancelled.";

type Registry = Mutex<Vec<(i64, Arc<AtomicBool>)>>;

fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Vec::new()))
}

/// Ask a running run to stop. Returns false when the run is not active in this process.
pub fn cancel(run_id: i64) -> bool {
    let list = registry().lock().unwrap_or_else(|e| e.into_inner());
    let mut hit = false;
    for (_, flag) in list.iter().filter(|(id, _)| *id == run_id) {
        flag.store(true, Ordering::SeqCst);
        hit = true;
    }
    hit
}

/// Whether a run is active in this process.
pub fn is_active(run_id: i64) -> bool {
    registry().lock().unwrap_or_else(|e| e.into_inner()).iter().any(|(id, _)| *id == run_id)
}

/// Run `fut` until it finishes or `flag` is set (checked every 200 ms). `None` when cancelled.
pub async fn until_cancelled<F: Future>(flag: &AtomicBool, fut: F) -> Option<F::Output> {
    tokio::select! {
        out = fut => Some(out),
        _ = async {
            loop {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        } => None,
    }
}

/// Kind and label of the “Update from Google Scholar” run (Published page, Settings → Research, the citations job).
pub const SCHOLAR_KIND: &str = "scholar";
pub const SCHOLAR_LABEL: &str = "Google Scholar update";

#[derive(Clone)]
pub struct Run {
    pub id: i64,
    pub kind: Arc<str>,
    ai: AiRun,
    cancel: Arc<AtomicBool>,
    started: Instant,
    /// Explicit progress (0–100) or -1 to use done/total.
    progress: Arc<AtomicI64>,
}

impl Run {
    /// Start a run (`kind` as in the web app: `scan`, `venues`, `citations`; plus `scholar` for “Update from Google Scholar”).
    pub fn begin(ctx: &Ctx, kind: &str, label: &str, total: i64, url: Option<&str>) -> Result<Run> {
        let provider = super::providers::status(&ctx.db);
        let ai = AiRun::begin(ctx, kind, label, total, url, Some(&provider))?;
        let cancel = Arc::new(AtomicBool::new(false));
        registry().lock().unwrap_or_else(|e| e.into_inner()).push((ai.id, cancel.clone()));
        let run = Run { id: ai.id, kind: kind.into(), ai, cancel, started: Instant::now(), progress: Arc::new(AtomicI64::new(-1)) };
        run.event(ctx, "running", None, None);
        Ok(run)
    }

    /// The shared cancel flag (pass to providers and fetches).
    pub fn flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// `Err(Cancelled.)` once the user cancelled.
    pub fn check(&self) -> Result<()> {
        if self.cancelled() {
            Err(ApiError::msg(CANCELLED))
        } else {
            Ok(())
        }
    }

    /// Run a future unless/until cancelled.
    pub async fn guard<F: Future>(&self, fut: F) -> Result<F::Output> {
        until_cancelled(&self.cancel, fut).await.ok_or_else(|| ApiError::msg(CANCELLED))
    }

    fn row(&self, ctx: &Ctx) -> Value {
        self.ai.row(ctx).ok().flatten().unwrap_or(Value::Null)
    }

    fn event(&self, ctx: &Ctx, status: &str, message: Option<&str>, partial: Option<Value>) {
        let row = self.row(ctx);
        let explicit = self.progress.load(Ordering::SeqCst);
        let progress = match status {
            "done" => 100,
            _ if explicit >= 0 => explicit,
            _ => ai_run::percent(&row).unwrap_or(0),
        };
        ctx.emit(
            "ai-run",
            json!({
                "run_id": self.id,
                "kind": &*self.kind,
                "status": status,
                "step": row.get("step").cloned().unwrap_or(Value::Null),
                "progress": progress.clamp(0, 100),
                "message": message,
                "partial": partial,
                "done": row.get("done").cloned().unwrap_or(json!(0)),
                "total": row.get("total").cloned().unwrap_or(json!(0)),
                "input_tokens": row.get("input_tokens").cloned().unwrap_or(json!(0)),
                "output_tokens": row.get("output_tokens").cloned().unwrap_or(json!(0)),
            }),
        );
    }

    /// Show a new step ("Reading 14 pages").
    pub fn step(&self, ctx: &Ctx, text: &str) {
        let _ = self.ai.step(ctx, text, None);
        self.event(ctx, "running", Some(text), None);
    }

    /// One unit done, optionally with a new step.
    pub fn advance(&self, ctx: &Ctx, text: Option<&str>) {
        let _ = self.ai.advance(ctx, text);
        self.event(ctx, "running", text, None);
    }

    /// A log line (shown in the history and as the event message) without changing the step.
    pub fn note(&self, ctx: &Ctx, text: &str) {
        let _ = self.ai.note(ctx, text);
        self.event(ctx, "running", Some(text), None);
    }

    /// Set progress explicitly (0–100) for runs with one unit of work.
    pub fn set_progress(&self, ctx: &Ctx, pct: i64) {
        self.progress.store(pct.clamp(0, 100), Ordering::SeqCst);
        self.event(ctx, "running", None, None);
    }

    /// Push a partial result to the renderer.
    pub fn partial(&self, ctx: &Ctx, value: Value) {
        self.event(ctx, "running", None, Some(value));
    }

    /// Add token usage.
    pub fn usage(&self, ctx: &Ctx, input: i64, output: i64) {
        if input == 0 && output == 0 {
            return;
        }
        let _ = ctx.db.exec(
            "UPDATE ai_runs SET input_tokens = input_tokens + ?1, output_tokens = output_tokens + ?2 WHERE id = ?3",
            rusqlite::params![input.max(0), output.max(0), self.id],
        );
    }

    /// Finish (error `None` = done). A cancelled run is stored as failed with "Cancelled.".
    pub fn finish(&self, ctx: &Ctx, error: Option<&str>, result: Option<Value>) {
        let error = if self.cancelled() && error.is_some() { Some(CANCELLED) } else { error };
        let _ = self.ai.finish(ctx, error);
        let ms = self.started.elapsed().as_millis() as i64;
        let _ = ctx.db.exec("UPDATE ai_runs SET duration_ms = ?1 WHERE id = ?2", rusqlite::params![ms, self.id]);
        registry().lock().unwrap_or_else(|e| e.into_inner()).retain(|(_, f)| !Arc::ptr_eq(f, &self.cancel));
        let status = match error {
            None => "done",
            Some(CANCELLED) => "cancelled",
            Some(_) => "failed",
        };
        let mut result = result.unwrap_or_else(|| json!({}));
        if let Some(o) = result.as_object_mut() {
            o.insert("pending_proposals".into(), json!(super::proposals::pending_count(&ctx.db)));
            o.insert("error".into(), json!(error));
        }
        self.event(ctx, status, error, Some(result));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_and_usage() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let run = Run::begin(&ctx, "venues", "Test", 2, None).unwrap();
        assert!(is_active(run.id));
        run.usage(&ctx, 100, 20);
        run.advance(&ctx, Some("One"));
        assert!(run.check().is_ok());
        assert!(cancel(run.id));
        assert!(run.check().is_err());
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        let out = rt.block_on(async { run.guard(tokio::time::sleep(Duration::from_secs(5))).await });
        assert!(out.is_err());
        run.finish(&ctx, Some("boom"), None);
        let row = ctx.db.find("ai_runs", run.id).unwrap().unwrap();
        assert_eq!(row["status"], "failed");
        assert_eq!(row["error"], CANCELLED);
        assert_eq!(row["input_tokens"], 100);
        assert!(row["duration_ms"].as_i64().is_some());
    }
}
