//! App updates (`app.checkUpdate`, `app.installUpdate`) and the silent background check.
//!
//! Releases on GitHub carry signed update bundles and a `latest.json` manifest; the updater plugin verifies each
//! download against the public key in `tauri.conf.json` before installing it.
//!
//! Events: `update-available` `{ version, notes, date, current }` when a check finds a newer version, and
//! `update` `{ status: "downloading" | "installing" | "failed", downloaded, total, error? }` while installing.

use crate::api::Ctx;
use crate::error::{ApiError, Result};
use crate::models::setting;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri_plugin_updater::UpdaterExt;

/// Setting: check for updates on launch and once a day (default on).
pub const AUTO_KEY: &str = "app.auto_update";
/// Cache key: the newest version a notification was already shown for.
const NOTIFIED_KEY: &str = "update:notified";

const FIRST_CHECK: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

static INSTALLING: AtomicBool = AtomicBool::new(false);

fn current_version(ctx: &Ctx) -> String {
    ctx.app().map(|a| a.package_info().version.to_string()).unwrap_or_default()
}

/// Ask the release server for a newer version. `Ok(None)` = up to date.
async fn check(ctx: &Ctx) -> Result<Option<tauri_plugin_updater::Update>> {
    let updater = ctx.app()?.updater().map_err(|e| ApiError::msg(format!("Updates are not available: {e}")))?;
    updater.check().await.map_err(|e| ApiError::msg(format!("Could not check for updates: {e}")))
}

fn describe(ctx: &Ctx, update: Option<&tauri_plugin_updater::Update>) -> Value {
    match update {
        Some(u) => json!({
            "available": true,
            "version": u.version,
            "notes": u.body.clone().unwrap_or_default(),
            "date": u.date.map(|d| d.to_string()),
            "current": current_version(ctx),
        }),
        None => json!({ "available": false, "version": null, "notes": null, "date": null, "current": current_version(ctx) }),
    }
}

/// `app.checkUpdate` → `{ available, version, notes, date, current }`.
pub fn check_update(ctx: &Ctx) -> Result<Value> {
    let found = ctx.block_on(check(ctx))?;
    let out = describe(ctx, found.as_ref());
    if found.is_some() {
        ctx.emit("update-available", &out);
    }
    Ok(out)
}

/// `app.installUpdate` → `{ started }`. Downloads in the background (progress on `update`), installs, relaunches.
pub fn install_update(ctx: &Ctx) -> Result<Value> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Ok(json!({ "started": false }));
    }
    ctx.spawn(|ctx| async move {
        let result = async {
            let update = check(&ctx).await?.ok_or_else(|| ApiError::msg("You’re already on the latest version."))?;
            let mut downloaded: u64 = 0;
            let progress_ctx = ctx.clone();
            let done_ctx = ctx.clone();
            update
                .download_and_install(
                    move |chunk, total| {
                        downloaded += chunk as u64;
                        progress_ctx.emit("update", json!({ "status": "downloading", "downloaded": downloaded, "total": total }));
                    },
                    move || done_ctx.emit("update", json!({ "status": "installing" })),
                )
                .await
                .map_err(|e| ApiError::msg(format!("The update could not be installed: {e}")))
        }
        .await;
        match result {
            Ok(()) => {
                if let Ok(app) = ctx.app() {
                    app.restart();
                }
            }
            Err(e) => {
                INSTALLING.store(false, Ordering::SeqCst);
                ctx.emit("update", json!({ "status": "failed", "error": e.to_string() }));
            }
        }
    });
    Ok(json!({ "started": true }))
}

/// One silent check: tells the window and, once per version, shows a notification.
async fn background_check(ctx: &Ctx) {
    if !setting::get_bool(&ctx.db, AUTO_KEY) || INSTALLING.load(Ordering::SeqCst) {
        return;
    }
    let Ok(Some(update)) = check(ctx).await else {
        return;
    };
    let info = describe(ctx, Some(&update));
    ctx.emit("update-available", &info);
    let notified = ctx.db.cache_get(NOTIFIED_KEY).ok().flatten();
    if notified.as_deref() != Some(update.version.as_str()) {
        let _ = ctx.db.cache_put(NOTIFIED_KEY, &update.version);
        let _ = ctx.notify(
            "update_available",
            json!({
                "title": format!("Manuscript Tracker {} is ready", update.version),
                "body": "Restart to update. Your data stays as it is.",
                "status": "success",
                "url": "#/settings/about",
            }),
        );
    }
}

/// Start the background checker (once): shortly after launch, then every 24 hours.
pub fn start(ctx: Ctx) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() || crate::mcp::data_dir_override().is_some() {
        return;
    }
    ctx.clone().spawn(|ctx| async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            background_check(&ctx).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}
