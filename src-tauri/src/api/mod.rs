//! The one IPC entry point: `api(name, args)`. Calls are dispatched by area prefix (`papers.list` -> `papers.rs`).
//!
//! Every area module exposes
//! ```ignore
//! pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
//!     Some(match name {
//!         "papers.list" => list(args, ctx),
//!         "papers.create" => create(args, ctx),
//!         _ => return None,          // not ours -> "Unknown action"
//!     })
//! }
//! ```
//! Handlers are synchronous and run on a blocking worker thread (never the UI thread), so plain SQLite calls
//! are fine. Long network jobs must not run inside a handler: start them with `ctx.spawn(...)` /
//! `ctx.spawn_blocking(...)`, return at once, and report progress with `AiRun` + `ctx.emit(...)`.

pub mod agents;
pub mod ai;
pub mod app;
pub mod backup;
pub mod calendar;
pub mod deadlines;
pub mod desk;
pub mod notifications;
pub mod opportunities;
pub mod papers;
pub mod proposals;
pub mod publications;
pub mod research;
pub mod settings;
pub mod venues;

use crate::db::{Db, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::setting;
use crate::util::uuid_v4;
use serde::Serialize;
use serde_json::{json, Value};
use std::future::Future;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager};

/// Context handed to every handler, job and background task. Cheap to clone; `Send + Sync`.
#[derive(Clone)]
pub struct Ctx {
    /// The database (see `db.rs` for the helpers).
    pub db: Db,
    app: Option<AppHandle>,
}

impl Ctx {
    /// Build from the running app (the `Db` is managed state).
    pub fn new(app: &AppHandle) -> Ctx {
        Ctx { db: app.state::<Db>().inner().clone(), app: Some(app.clone()) }
    }

    /// A context without an app (unit tests): `emit` and native notifications do nothing.
    pub fn for_tests(db: Db) -> Ctx {
        Ctx { db, app: None }
    }

    /// The Tauri app handle (windows, plugins, paths).
    pub fn app(&self) -> Result<&AppHandle> {
        self.app.as_ref().ok_or_else(|| ApiError::msg("The app is not running."))
    }

    /// The local user's id (always 1).
    pub fn user_id(&self) -> i64 {
        USER_ID
    }

    /// The local user row (password and remember_token removed), or `None` before onboarding.
    pub fn user(&self) -> Result<Option<Value>> {
        let mut u = self.db.find("users", USER_ID)?;
        if let Some(o) = u.as_mut().and_then(Value::as_object_mut) {
            for k in crate::models::user::HIDDEN {
                o.remove(*k);
            }
        }
        Ok(u)
    }

    /// The local user row, or an error before onboarding.
    pub fn require_user(&self) -> Result<Value> {
        self.user()?.ok_or_else(|| ApiError::msg("Finish setting up the app first."))
    }

    /// A setting value or its default (secrets are read from the keychain; never return them to the renderer).
    pub fn setting(&self, key: &str) -> Option<String> {
        setting::get(&self.db, key)
    }

    /// Push an event to the renderer (`on(channel, cb)` in src/lib/events.js). No-op in tests.
    pub fn emit<S: Serialize + Clone>(&self, channel: &str, payload: S) {
        if let Some(app) = &self.app {
            let _ = app.emit(channel, payload);
        }
    }

    /// Tell open pages that data in an area changed (e.g. after a background job): emits `changed` `{ area }`.
    pub fn changed(&self, area: &str) {
        self.emit("changed", json!({ "area": area }));
    }

    /// Store a Laravel-style database notification for the user, show a native notification (when the window
    /// is not focused and the preference is on) and emit `notifications` `{ unread, notification }`.
    ///
    /// `kind` is a short name (`"ai_job_finished"` -> `App\Notifications\AiJobFinished`) or a full class name.
    /// `data` is usually `{ "title", "body", "status": "success"|"warning"|"danger", "url": "#/venues" | null }`.
    pub fn notify(&self, kind: &str, data: Value) -> Result<Value> {
        let id = uuid_v4();
        self.db.insert(
            "notifications",
            &json!({
                "id": id,
                "type": notification_type(kind),
                "notifiable_type": "App\\Models\\User",
                "notifiable_id": USER_ID,
                "data": data,
                "read_at": null,
            }),
        )?;
        let row = self.db.first("SELECT * FROM notifications WHERE id = ?1", [&id])?.unwrap_or(Value::Null);
        if let Some(app) = &self.app {
            let focused =
                app.get_webview_window("main").is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
            if !focused && setting::get_bool(&self.db, "app.native_notifications") {
                use tauri_plugin_notification::NotificationExt;
                let title = data.get("title").and_then(Value::as_str).unwrap_or("Manuscript Tracker");
                let body = data.get("body").and_then(Value::as_str).unwrap_or("");
                let _ = app.notification().builder().title(title).body(body).show();
            }
        }
        let unread = self.unread_notifications()?;
        self.emit("notifications", json!({ "unread": unread, "notification": row }));
        Ok(row)
    }

    /// Number of unread notifications.
    pub fn unread_notifications(&self) -> Result<i64> {
        self.db.count(
            "SELECT COUNT(*) FROM notifications WHERE notifiable_id = ?1 AND notifiable_type = 'App\\Models\\User' AND read_at IS NULL",
            [USER_ID],
        )
    }

    /// The app data directory (`.../com.naimbh.manuscripttracker`).
    pub fn data_dir(&self) -> Result<PathBuf> {
        if let Some(d) = crate::mcp::data_dir_override() {
            return Ok(d);
        }
        Ok(self.app()?.path().app_data_dir()?)
    }

    /// Run an async task (network work) on the async runtime with its own context clone. Returns at once.
    pub fn spawn<F, Fut>(&self, f: F)
    where
        F: FnOnce(Ctx) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let ctx = self.clone();
        tauri::async_runtime::spawn(async move { f(ctx).await });
    }

    /// Run blocking work (e.g. a long loop of SQLite + blocking calls) on its own thread. Returns at once.
    pub fn spawn_blocking<F>(&self, f: F)
    where
        F: FnOnce(Ctx) + Send + 'static,
    {
        let ctx = self.clone();
        std::thread::spawn(move || f(ctx));
    }

    /// Run a future to completion from blocking code (a handler, a job or a `spawn_blocking` thread).
    pub fn block_on<F: Future>(&self, fut: F) -> F::Output {
        tauri::async_runtime::block_on(fut)
    }

    /// Shared HTTP client (rustls, connection pooling, 60 s timeout). Use from async code.
    pub fn http(&self) -> &'static reqwest::Client {
        http()
    }
}

/// Shared HTTP client for services.
pub fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("ManuscriptTracker/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default()
    })
}

/// `ai_job_finished` -> `App\Notifications\AiJobFinished`.
pub fn notification_type(kind: &str) -> String {
    if kind.contains('\\') {
        return kind.to_string();
    }
    let studly: String = kind
        .split(['_', '-', '.'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect();
    format!("App\\Notifications\\{studly}")
}

fn valid_name(name: &str) -> bool {
    name.len() <= 80
        && name.contains('.')
        && name.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
}

/// Route a call to its area module.
pub fn dispatch(name: &str, args: &Value, ctx: &Ctx) -> Result<Value> {
    if !valid_name(name) {
        return Err(ApiError::msg("Unknown action."));
    }
    let empty = Value::Object(Default::default());
    let args = if args.is_null() { &empty } else { args };
    if !args.is_object() {
        return Err(ApiError::msg("Invalid arguments."));
    }
    // Research entry points (`proposals.*`, `venues.research`, `venues.refreshAll`, `venues.restoreCatalog`,
    // `publications.refreshCitations`, `publications.refreshAll`) are claimed before the area modules.
    if let Some(out) = proposals::handle(name, args, ctx) {
        return out;
    }
    let area = name.split('.').next().unwrap_or("");
    let out = match area {
        "app" => app::handle(name, args, ctx),
        "desk" => desk::handle(name, args, ctx),
        "papers" => papers::handle(name, args, ctx),
        "venues" => venues::handle(name, args, ctx),
        "publications" => publications::handle(name, args, ctx),
        "deadlines" => deadlines::handle(name, args, ctx),
        "opportunities" => opportunities::handle(name, args, ctx),
        "notifications" => notifications::handle(name, args, ctx),
        "ai" => ai::handle(name, args, ctx),
        "research" => research::handle(name, args, ctx),
        "settings" => settings::handle(name, args, ctx),
        "backup" => backup::handle(name, args, ctx),
        "calendar" => calendar::handle(name, args, ctx),
        "agents" => agents::handle(name, args, ctx),
        "files" => crate::files::handle(name, args, ctx),
        _ => None,
    };
    out.unwrap_or_else(|| Err(ApiError::msg(format!("Unknown action: {name}"))))
}

/// The Tauri command behind `invoke('api', { name, args })`. Runs the handler on a blocking worker thread.
#[tauri::command]
pub async fn api(app: AppHandle, name: String, args: Value) -> Result<Value> {
    let ctx = Ctx::new(&app);
    let label = name.clone();
    let res = tauri::async_runtime::spawn_blocking(move || dispatch(&name, &args, &ctx))
        .await
        .unwrap_or_else(|_| Err(ApiError::msg("Something went wrong. Please try again.")));
    if cfg!(debug_assertions) {
        if let Err(ApiError::Message(m)) = &res {
            eprintln!("api {label}: {m}");
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn dispatch_and_notify() {
        let ctx = Ctx::for_tests(test_db());
        assert!(dispatch("nope", &json!({}), &ctx).is_err());
        assert!(dispatch("papers.zzz", &json!({}), &ctx).is_err());
        assert!(dispatch("app.boot", &json!([]), &ctx).is_err());
        assert_eq!(notification_type("ai_job_finished"), "App\\Notifications\\AiJobFinished");
        let n = ctx.notify("ai_job_finished", json!({ "title": "Done", "body": "B", "status": "success", "url": null })).unwrap();
        assert_eq!(n["data"]["title"], "Done");
        assert_eq!(n["type"], "App\\Notifications\\AiJobFinished");
        assert_eq!(ctx.unread_notifications().unwrap(), 1);
        let err = serde_json::to_value(dispatch("x.y", &json!({}), &ctx).unwrap_err()).unwrap();
        assert_eq!(err["kind"], "error");
    }
}
