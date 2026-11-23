//! `settings.*` handlers: Profile, General (scheduled AI jobs, desktop options) and "Erase all data".
//!
//! | Action | Args → result |
//! | --- | --- |
//! | `settings.profile` | → `{ name, email }` |
//! | `settings.saveProfile` | `{ name, email? }` → the user row (also emitted as `user`) |
//! | `settings.general` | → `{ jobs: [{ key, command, label, description, enabled, frequency, day, time, last_run, running }], desktop: { run_in_background, open_at_login, native_notifications, show_badge, auto_update }, now }` |
//! | `settings.saveGeneral` | `{ <key>_enabled, <key>_frequency, <key>_day, <key>_time }` for scan, venues, citations → like `settings.general` |
//! | `settings.setDesktop` | any of the `desktop` booleans → like `settings.general` (applied at once) |
//! | `settings.eraseAll` | Deletes every record, stored file, local backup and saved key on this computer (the page asks first). |
//! | `settings.avatar` | → `{ url }`: the profile photo as a `data:` URL, or `null`. |
//! | `settings.pickAvatar` | → native open dialog (PNG, JPEG, WebP, HEIC) → `{ name, mime, size, data }` (the source image as a `data:` URL) or `null`. Errors on `photo`. |
//! | `settings.readAvatar` | `{ token \| path }` of a file dropped on the window → like `settings.pickAvatar`. |
//! | `settings.saveAvatar` | `{ data }` (the cropped 512 × 512 PNG or WebP as a `data:` URL) → `{ user, url }` (also emits `user`). |
//! | `settings.removeAvatar` | → `{ user, url: null }` (also emits `user`). |

use crate::api::Ctx;
use crate::db::USER_ID;
use crate::error::{ApiError, Result};
use crate::models::setting;
use crate::validation::{ensure, Validator};
use chrono::Local;
use serde_json::{json, Value};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "settings.profile" => profile(ctx),
        "settings.saveProfile" => save_profile(args, ctx),
        "settings.general" => general(ctx),
        "settings.saveGeneral" => save_general(args, ctx),
        "settings.setDesktop" => set_desktop(args, ctx),
        "settings.eraseAll" => erase_all(ctx),
        "settings.avatar" => crate::files::avatar_url(ctx).map(|url| json!({ "url": url })),
        "settings.pickAvatar" => ctx.require_user().and_then(|_| crate::files::pick_avatar(ctx)),
        "settings.readAvatar" => ctx.require_user().and_then(|_| crate::files::read_dropped_avatar(args)),
        "settings.saveAvatar" => save_avatar(args, ctx),
        "settings.removeAvatar" => remove_avatar(ctx),
        _ => return None,
    })
}

fn profile(ctx: &Ctx) -> Result<Value> {
    let u = ctx.require_user()?;
    Ok(json!({ "name": u["name"], "email": u["email"] }))
}

fn save_profile(args: &Value, ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    let v = Validator::new(args).rule("name", "required|string|max:255").rule("email", "nullable|string|email|max:255").validate()?;
    let email = v.opt("email");
    if let Some(e) = &email {
        ensure(*e == e.to_lowercase(), "email", "The email field must be lowercase.")?;
    }
    ctx.db.update("users", USER_ID, &json!({ "name": v.string("name"), "email": email }))?;
    let user = ctx.require_user()?;
    ctx.emit("user", &user);
    Ok(user)
}

fn save_avatar(args: &Value, ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    let data = args.get("data").and_then(Value::as_str).filter(|s| !s.is_empty());
    let data = data.ok_or_else(|| ApiError::field("photo", "Choose a photo first."))?;
    crate::files::save_avatar(ctx, data)?;
    avatar_result(ctx)
}

fn remove_avatar(ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    crate::files::set_avatar(&ctx.db, &ctx.data_dir()?, None)?;
    avatar_result(ctx)
}

/// `{ user, url }` after a photo change; the shell also gets the user row as the `user` event.
fn avatar_result(ctx: &Ctx) -> Result<Value> {
    let user = ctx.require_user()?;
    ctx.emit("user", &user);
    Ok(json!({ "user": user, "url": crate::files::avatar_url(ctx)? }))
}

/// The three scheduled AI jobs: (settings prefix, scheduler command, label, description).
pub const JOBS: &[(&str, &str, &str, &str)] = &[
    ("scan", "opportunities:scan", "Calls for papers scan", "Looks for new calls for papers that fit your research."),
    ("venues", "venues:refresh", "Journal & conference updates", "Checks deadlines and details of your journals and conferences."),
    ("citations", "publications:refresh", "Citation updates", "Refreshes citation counts and your Google Scholar profile."),
];

fn general(ctx: &Ctx) -> Result<Value> {
    let g = |k: &str| setting::get(&ctx.db, k).unwrap_or_default();
    let b = |k: &str| setting::get_bool(&ctx.db, k);
    let prefs = crate::api::app::handle("app.preferences", &json!({}), ctx).unwrap_or_else(|| Ok(json!({})))?;
    let jobs: Vec<Value> = JOBS
        .iter()
        .map(|(p, command, label, description)| {
            json!({
                "key": p, "command": command, "label": label, "description": description,
                "enabled": b(&format!("schedule.{p}_enabled")),
                "frequency": if g(&format!("schedule.{p}_frequency")) == "weekly" { "weekly" } else { "daily" },
                "day": setting::get_i64(&ctx.db, &format!("schedule.{p}_day")).unwrap_or(1).rem_euclid(7),
                "time": g(&format!("schedule.{p}_time")),
                "last_run": crate::scheduler::last_started(ctx, command).map(|t| t.to_rfc3339()),
                "running": crate::scheduler::is_running(command),
            })
        })
        .collect();
    Ok(json!({
        "jobs": jobs,
        "desktop": {
            "run_in_background": prefs.get("run_in_background").cloned().unwrap_or(json!(b("app.run_in_background"))),
            "open_at_login": prefs.get("open_at_login").cloned().unwrap_or(json!(b("app.open_at_login"))),
            "native_notifications": prefs.get("native_notifications").cloned().unwrap_or(json!(b("app.native_notifications"))),
            "show_badge": b("app.show_badge"),
            "auto_update": b(crate::updater::AUTO_KEY),
            "always_on_top": crate::native::always_on_top(&ctx.db),
        },
        "now": Local::now().format("%H:%M, %-d %b %Y").to_string(),
    }))
}

fn is_time(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 5
        && b[2] == b':'
        && s[..2].parse::<u32>().is_ok_and(|h| h < 24)
        && s[3..].parse::<u32>().is_ok_and(|m| m < 60)
        && b[..2].iter().chain(&b[3..]).all(u8::is_ascii_digit)
}

/// Save the scheduled-job options: `{ scan_enabled, scan_frequency, scan_day, scan_time, venues_…, citations_… }`.
fn save_general(args: &Value, ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    let mut validator = Validator::new(args);
    for (p, ..) in JOBS {
        validator = validator
            .rule(&format!("{p}_enabled"), "required|boolean")
            .rule(&format!("{p}_frequency"), "required|in:daily,weekly")
            .rule(&format!("{p}_day"), "required|integer|in:0,1,2,3,4,5,6")
            .rule(&format!("{p}_time"), "required|string");
    }
    let v = validator.validate()?;
    let mut errors = crate::error::Errors::new();
    for (p, ..) in JOBS {
        let k = format!("{p}_time");
        if !is_time(&v.string(&k)) {
            errors.add(k.clone(), format!("The {} field must match the format H:i.", crate::validation::attribute_name(&k)));
        }
    }
    errors.into_result()?;
    for (p, ..) in JOBS {
        setting::put(&ctx.db, &format!("schedule.{p}_enabled"), Some(if v.bool(&format!("{p}_enabled")) { "1" } else { "0" }))?;
        setting::put(&ctx.db, &format!("schedule.{p}_frequency"), Some(&v.string(&format!("{p}_frequency"))))?;
        setting::put(&ctx.db, &format!("schedule.{p}_day"), Some(&v.i64(&format!("{p}_day")).unwrap_or(1).to_string()))?;
        setting::put(&ctx.db, &format!("schedule.{p}_time"), Some(&v.string(&format!("{p}_time"))))?;
    }
    ctx.changed("settings");
    general(ctx)
}

fn set_desktop(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = Validator::new(args)
        .rule("run_in_background", "sometimes|boolean")
        .rule("open_at_login", "sometimes|boolean")
        .rule("native_notifications", "sometimes|boolean")
        .rule("show_badge", "sometimes|boolean")
        .rule("auto_update", "sometimes|boolean")
        .rule("always_on_top", "sometimes|boolean")
        .validate()?;
    let mut prefs = serde_json::Map::new();
    for k in ["run_in_background", "open_at_login", "native_notifications"] {
        if v.has(k) {
            prefs.insert(k.into(), json!(v.bool(k)));
        }
    }
    if !prefs.is_empty() {
        crate::api::app::handle("app.setPreferences", &Value::Object(prefs), ctx).unwrap_or_else(|| Ok(Value::Null))?;
    }
    if v.has("show_badge") {
        let on = v.bool("show_badge");
        setting::put(&ctx.db, "app.show_badge", Some(if on { "1" } else { "0" }))?;
        if !on && ctx.app().is_ok() {
            let _ = crate::native::set_badge(&json!({ "count": 0 }), ctx);
        }
        ctx.changed("badge");
    }
    if v.has("always_on_top") {
        crate::native::set_always_on_top(ctx, v.bool("always_on_top"))?;
    }
    if v.has("auto_update") {
        setting::put(&ctx.db, crate::updater::AUTO_KEY, Some(if v.bool("auto_update") { "1" } else { "0" }))?;
    }
    general(ctx)
}

/// Delete every row of every table (keeps the schema), the stored files, local backups and keychain secrets.
fn erase_all(ctx: &Ctx) -> Result<Value> {
    crate::api::calendar::stop_server();
    for k in setting::SECRET {
        let _ = setting::put(&ctx.db, k, None);
    }
    ctx.db.with(|c| {
        c.execute_batch("PRAGMA foreign_keys = OFF;")?;
        let res = (|| -> Result<()> {
            let tables: Vec<String> = c
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            c.execute_batch("BEGIN IMMEDIATE;")?;
            for t in &tables {
                if t.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
                    if let Err(e) = c.execute(&format!("DELETE FROM {t}"), []) {
                        let _ = c.execute_batch("ROLLBACK;");
                        return Err(e.into());
                    }
                }
            }
            let _ = c.execute("DELETE FROM sqlite_sequence", []);
            c.execute_batch("COMMIT;")?;
            Ok(())
        })();
        let _ = c.execute_batch("PRAGMA foreign_keys = ON;");
        res?;
        let _ = c.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(())
    })?;
    if let Ok(dir) = ctx.data_dir() {
        for sub in ["files", "backups", crate::files::AVATAR_DIR] {
            let p = dir.join(sub);
            if p.exists() {
                std::fs::remove_dir_all(&p).map_err(|e| ApiError::msg(format!("Could not delete {}: {e}", p.display())))?;
            }
        }
        std::fs::create_dir_all(dir.join("files"))?;
    }
    if let Ok(app) = ctx.app() {
        use tauri_plugin_autostart::ManagerExt;
        let _ = app.autolaunch().disable();
        crate::api::app::apply_theme(app, "system");
        let _ = crate::native::set_badge(&json!({ "count": 0 }), ctx);
    }
    Ok(json!(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    #[test]
    fn profile_general_and_erase() {
        let ctx = Ctx::for_tests(test_db());
        let err =
            serde_json::to_value(handle("settings.saveProfile", &json!({ "name": "", "email": "A@B.co" }), &ctx).unwrap().unwrap_err())
                .unwrap();
        assert_eq!(err["errors"]["name"][0], "The name field is required.");
        let err =
            serde_json::to_value(handle("settings.saveProfile", &json!({ "name": "N", "email": "A@B.co" }), &ctx).unwrap().unwrap_err())
                .unwrap();
        assert_eq!(err["errors"]["email"][0], "The email field must be lowercase.");
        let u = handle("settings.saveProfile", &json!({ "name": "Alex", "email": "" }), &ctx).unwrap().unwrap();
        assert_eq!(u["name"], "Alex");
        assert!(u["email"].is_null());

        let g = handle("settings.general", &json!({}), &ctx).unwrap().unwrap();
        assert_eq!(g["jobs"][0]["time"], "06:00");
        assert_eq!(g["jobs"][0]["frequency"], "daily");
        assert_eq!(g["jobs"][1]["frequency"], "weekly");
        assert_eq!(g["desktop"]["show_badge"], json!(true));
        assert_eq!(g["desktop"]["always_on_top"], json!(false));
        let g = handle("settings.setDesktop", &json!({ "always_on_top": true }), &ctx).unwrap().unwrap();
        assert_eq!(g["desktop"]["always_on_top"], json!(true));
        assert_eq!(setting::get(&ctx.db, crate::native::ON_TOP_KEY).as_deref(), Some("1"));
        handle("settings.setDesktop", &json!({ "always_on_top": false }), &ctx).unwrap().unwrap();
        let mut form = json!({
            "scan_enabled": true, "scan_frequency": "daily", "scan_day": 1, "scan_time": "6:00",
            "venues_enabled": false, "venues_frequency": "weekly", "venues_day": 2, "venues_time": "07:30",
            "citations_enabled": true, "citations_frequency": "weekly", "citations_day": "4", "citations_time": "24:00",
        });
        let err = serde_json::to_value(handle("settings.saveGeneral", &form, &ctx).unwrap().unwrap_err()).unwrap();
        assert_eq!(err["errors"]["scan_time"][0], "The scan time field must match the format H:i.");
        assert_eq!(err["errors"]["citations_time"][0], "The citations time field must match the format H:i.");
        form["scan_time"] = json!("05:45");
        form["citations_time"] = json!("08:00");
        let g = handle("settings.saveGeneral", &form, &ctx).unwrap().unwrap();
        assert_eq!(g["jobs"][0]["time"], "05:45");
        assert_eq!(g["jobs"][1]["enabled"], json!(false));
        assert_eq!(g["jobs"][1]["day"], 2);
        assert_eq!(g["jobs"][2]["day"], 4);
        let g = handle("settings.setDesktop", &json!({ "show_badge": false }), &ctx).unwrap().unwrap();
        assert_eq!(g["desktop"]["show_badge"], json!(false));

        ctx.db.insert("papers", &json!({ "user_id": 1, "title": "P" })).unwrap();
        handle("settings.eraseAll", &json!({}), &ctx).unwrap().unwrap();
        assert!(ctx.user().unwrap().is_none());
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM papers", []).unwrap(), 0);
        assert_eq!(ctx.db.count("SELECT COUNT(*) FROM settings", []).unwrap(), 0);
    }
}
