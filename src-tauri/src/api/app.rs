//! `app.*` handlers: boot, onboarding, theme, window readiness, preferences and system integration.

use crate::api::Ctx;
use crate::db::{Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::setting;
use crate::util::{now_ts, platform, random_token, start_of_week_ts};
use crate::validation::{is_url, Validator};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, Theme};

/// Launch state (managed).
#[derive(Default)]
pub struct LaunchState {
    /// Started at login with `--hidden`: stay in the tray until the user opens the window.
    pub hidden: AtomicBool,
    /// The main window has been shown at least once.
    pub shown: AtomicBool,
}

const DARK_BG: (u8, u8, u8) = (9, 9, 11); // zinc-950, `.dark .soft-bg`
const LIGHT_BG: (u8, u8, u8) = (247, 248, 252); // #f7f8fc, `.soft-bg`

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "app.boot" => boot(ctx),
        "app.ready" => ready(ctx),
        "app.onboard" => onboard(args, ctx),
        "app.setTheme" => set_theme(args, ctx),
        "app.openExternal" => open_external(args, ctx),
        "app.openDataFolder" => open_data_folder(ctx),
        "app.info" => info(ctx),
        "app.preferences" => preferences(ctx),
        "app.setPreferences" => set_preferences(args, ctx),
        "app.contextMenu" => crate::native::context_menu(args, ctx),
        "app.setBadge" => crate::native::set_badge(args, ctx),
        "app.confirm" => confirm(args, ctx),
        "app.checkUpdate" => crate::updater::check_update(ctx),
        "app.installUpdate" => crate::updater::install_update(ctx),
        "app.dismissNotice" => dismiss_notice(args, ctx),
        "app.restoreNotices" => restore_notices(ctx),
        // Debug builds only: visual QA prints page diagnostics (console errors) to the terminal.
        #[cfg(debug_assertions)]
        "app.qaLog" => {
            eprintln!("[qa] {}", args.get("text").and_then(Value::as_str).unwrap_or(""));
            Ok(Value::Null)
        }
        _ => return None,
    })
}

fn boot(ctx: &Ctx) -> Result<Value> {
    let new_opps = ctx.db.count(
        "SELECT COUNT(*) FROM opportunities WHERE user_id = ?1 AND status = 'new' AND created_at >= ?2",
        rusqlite::params![USER_ID, start_of_week_ts()],
    )?;
    Ok(json!({
        "user": ctx.user()?,
        "theme": ctx.setting("app.theme").unwrap_or_else(|| "system".into()),
        "unread_notifications": ctx.unread_notifications()?,
        "new_opportunities_this_week": new_opps,
        "platform": platform(),
        "material": crate::native::material(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// The renderer has painted: show the window (unless the app was started hidden at login).
fn ready(ctx: &Ctx) -> Result<Value> {
    if let Ok(app) = ctx.app() {
        let hidden = app.state::<LaunchState>().hidden.load(Ordering::SeqCst);
        if !hidden {
            show_main(app);
        }
    }
    Ok(json!(true))
}

/// `app.dismissNotice` `{ key }`: a warning the user closed stays closed (e.g. `anon:12`).
fn dismiss_notice(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = Validator::new(args).rule("key", "required|string|max:100").validate()?;
    setting::dismiss(&ctx.db, &v.string("key"))?;
    ctx.changed("desk");
    ctx.changed("papers");
    Ok(json!(true))
}

/// `app.restoreNotices`: show every dismissed warning again; returns how many there were.
fn restore_notices(ctx: &Ctx) -> Result<Value> {
    let n = setting::dismissed(&ctx.db).len();
    setting::put(&ctx.db, setting::DISMISSED_KEY, None)?;
    ctx.changed("desk");
    ctx.changed("papers");
    Ok(json!({ "restored": n }))
}

fn onboard(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = Validator::new(args)
        .rule("name", "required|string|max:255")
        .rule("email", "nullable|string|email|max:255")
        .rule("interests", "nullable|string|max:1000")
        .rule("scholar_url", "nullable|string|url|max:255")
        .rule("theme", "nullable|in:light,dark,system")
        .validate()?;
    let scholar = v.opt("scholar_url");
    if let Some(u) = &scholar {
        if !(u.starts_with("https://scholar.google.") && u.contains("/citations?")) {
            return Err(ApiError::field(
                "scholar_url",
                "Use your profile link, which starts with https://scholar.google.com/citations?user=…",
            ));
        }
    }
    let existed = ctx.db.find("users", USER_ID)?.is_some();
    let data = json!({
        "name": v.string("name"),
        "email": v.opt("email"),
        "interests": v.opt("interests"),
        "scholar_url": scholar,
        "is_admin": true,
    });
    ctx.db.tx(|tx| {
        if existed {
            tx.update("users", USER_ID, &data)?;
        } else {
            let mut data = data.clone();
            data["id"] = json!(USER_ID);
            data["calendar_token"] = json!(random_token(40));
            data["mcp_token"] = json!(format!("mt_{}", random_token(40)));
            tx.insert("users", &data)?;
        }
        Ok(())
    })?;
    if let Some(theme) = v.opt("theme") {
        setting::put(&ctx.db, "app.theme", Some(&theme))?;
        if let Ok(app) = ctx.app() {
            apply_theme(app, &theme);
        }
    }
    if !existed {
        crate::api::venues::on_user_created(ctx)?;
    }
    let user = ctx.require_user()?;
    ctx.emit("user", &user);
    Ok(user)
}

fn set_theme(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = Validator::new(args).rule("theme", "required|in:light,dark,system").validate()?;
    let theme = v.string("theme");
    setting::put(&ctx.db, "app.theme", Some(&theme))?;
    if let Ok(app) = ctx.app() {
        apply_theme(app, &theme);
    }
    Ok(json!({ "theme": theme }))
}

/// Only http(s) and mailto links leave the app.
pub fn safe_external_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("mailto:") {
        return url.len() > 7 && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    }
    (lower.starts_with("http://") || lower.starts_with("https://")) && is_url(url.trim())
}

fn open_external(args: &Value, ctx: &Ctx) -> Result<Value> {
    let url = args.get("url").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if !safe_external_url(&url) {
        return Err(ApiError::msg("Only web and email links can be opened."));
    }
    use tauri_plugin_opener::OpenerExt;
    ctx.app()?.opener().open_url(&url, None::<&str>).map_err(|e| ApiError::msg(format!("Could not open the link: {e}")))?;
    Ok(json!(true))
}

fn open_data_folder(ctx: &Ctx) -> Result<Value> {
    let dir = ctx.data_dir()?;
    use tauri_plugin_opener::OpenerExt;
    ctx.app()?
        .opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| ApiError::msg(format!("Could not open the folder: {e}")))?;
    Ok(json!(true))
}

fn info(ctx: &Ctx) -> Result<Value> {
    let dir = ctx.data_dir().ok();
    let db_size = dir.as_ref().and_then(|d| std::fs::metadata(d.join("tracker.sqlite")).ok()).map(|m| m.len());
    Ok(json!({
        "name": "Manuscript Tracker",
        "version": env!("CARGO_PKG_VERSION"),
        "tauri": tauri::VERSION,
        "platform": platform(),
        "arch": std::env::consts::ARCH,
        "data_dir": dir.as_ref().map(|d| d.to_string_lossy().to_string()),
        "database": dir.as_ref().map(|d| d.join("tracker.sqlite").to_string_lossy().to_string()),
        "database_size": db_size,
        "files_dir": dir.as_ref().map(|d| d.join("files").to_string_lossy().to_string()),
        "schema_version": ctx.db.scalar::<i64, _>("PRAGMA user_version", [])?,
        "now": now_ts(),
    }))
}

fn open_at_login(ctx: &Ctx) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    ctx.app().ok().and_then(|a| a.autolaunch().is_enabled().ok()).unwrap_or_else(|| setting::get_bool(&ctx.db, "app.open_at_login"))
}

fn preferences(ctx: &Ctx) -> Result<Value> {
    Ok(json!({
        "run_in_background": setting::get_bool(&ctx.db, "app.run_in_background"),
        "open_at_login": open_at_login(ctx),
        "native_notifications": setting::get_bool(&ctx.db, "app.native_notifications"),
    }))
}

fn set_preferences(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = Validator::new(args)
        .rule("run_in_background", "sometimes|boolean")
        .rule("open_at_login", "sometimes|boolean")
        .rule("native_notifications", "sometimes|boolean")
        .validate()?;
    for key in ["run_in_background", "native_notifications"] {
        if v.has(key) {
            setting::put(&ctx.db, &format!("app.{key}"), Some(if v.bool(key) { "1" } else { "0" }))?;
        }
    }
    if v.has("open_at_login") {
        let on = v.bool("open_at_login");
        setting::put(&ctx.db, "app.open_at_login", Some(if on { "1" } else { "0" }))?;
        if let Ok(app) = ctx.app() {
            use tauri_plugin_autostart::ManagerExt;
            let r = if on { app.autolaunch().enable() } else { app.autolaunch().disable() };
            r.map_err(|e| ApiError::msg(format!("Could not change the login item: {e}")))?;
        }
    }
    preferences(ctx)
}

/// Native confirmation dialog: `{ title?, message, ok?, danger? }` -> true when confirmed.
fn confirm(args: &Value, ctx: &Ctx) -> Result<Value> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let v = Validator::new(args)
        .rule("title", "nullable|string|max:120")
        .rule("message", "required|string|max:2000")
        .rule("ok", "nullable|string|max:40")
        .rule("danger", "nullable|boolean")
        .validate()?;
    let app = ctx.app()?;
    let mut dialog = app
        .dialog()
        .message(v.string("message"))
        .title(v.opt("title").unwrap_or_else(|| "Manuscript Tracker".into()))
        .kind(if v.bool("danger") { MessageDialogKind::Warning } else { MessageDialogKind::Info })
        .buttons(MessageDialogButtons::OkCancelCustom(v.opt("ok").unwrap_or_else(|| "OK".into()), "Cancel".into()));
    if let Some(w) = app.get_webview_window("main") {
        dialog = dialog.parent(&w);
    }
    Ok(json!(dialog.blocking_show()))
}

/// Show, restore and focus the main window.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    let state = app.state::<LaunchState>();
    state.shown.store(true, Ordering::SeqCst);
    state.hidden.store(false, Ordering::SeqCst);
}

/// Set the native window theme (title bar, vibrancy) and a matching background (no white flash in dark mode).
pub fn apply_theme(app: &AppHandle, theme: &str) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let native = match theme {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    };
    let _ = w.set_theme(native);
    let dark = match native {
        Some(t) => t == Theme::Dark,
        None => w.theme().map(|t| t == Theme::Dark).unwrap_or(false),
    };
    crate::native::match_material_theme(&w, dark);
    if crate::native::material().is_some() {
        // The OS material follows the window theme; an opaque background would hide it.
        return;
    }
    let (r, g, b) = if dark { DARK_BG } else { LIGHT_BG };
    let _ = w.set_background_color(Some(tauri::window::Color(r, g, b, 255)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn onboard_boot_and_links() {
        let ctx = Ctx::for_tests(Db::open_in_memory().unwrap());
        let boot = handle("app.boot", &json!({}), &ctx).unwrap().unwrap();
        assert!(boot["user"].is_null());
        assert_eq!(boot["theme"], "system");

        let err = handle("app.onboard", &json!({ "name": "", "email": "x" }), &ctx).unwrap().unwrap_err();
        let err = serde_json::to_value(err).unwrap();
        assert_eq!(err["errors"]["name"][0], "The name field is required.");
        assert_eq!(err["errors"]["email"][0], "The email field must be a valid email address.");
        let err = handle("app.onboard", &json!({ "name": "N", "scholar_url": "https://example.com" }), &ctx).unwrap().unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));

        let u = handle("app.onboard", &json!({ "name": "Alex", "email": "", "interests": "HCI", "theme": "dark" }), &ctx).unwrap().unwrap();
        assert_eq!(u["id"], 1);
        assert_eq!(u["is_admin"], true);
        assert!(u["email"].is_null());
        assert_eq!(u["calendar_token"].as_str().unwrap().len(), 40);
        assert!(u.get("password").is_none());
        let boot = handle("app.boot", &json!({}), &ctx).unwrap().unwrap();
        assert_eq!(boot["user"]["name"], "Alex");
        assert_eq!(boot["theme"], "dark");
        // Onboarding again updates the same user and keeps tokens.
        let u2 = handle("app.onboard", &json!({ "name": "Alex D" }), &ctx).unwrap().unwrap();
        assert_eq!(u2["calendar_token"], u["calendar_token"]);

        assert!(safe_external_url("https://example.com/a?b=c"));
        assert!(safe_external_url("mailto:a@b.co"));
        assert!(!safe_external_url("file:///etc/passwd"));
        assert!(!safe_external_url("javascript:alert(1)"));
        assert!(!safe_external_url("https://"));
    }
}
