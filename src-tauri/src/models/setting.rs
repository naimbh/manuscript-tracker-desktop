//! App settings (port of app/Models/Setting.php + config/tracker.php + config/services.php defaults).
//!
//! Plain values live in the `settings` table. Secrets (API keys, tokens) live in the OS keychain
//! (macOS Keychain / Windows Credential Manager) under service [`KEYRING_SERVICE`]; the table only holds
//! the marker [`SECRET_MARKER`] with `encrypted = 1`. Never send a secret to the renderer: use [`masked`].
//!
//! ```ignore
//! use crate::models::setting;
//! let model = setting::get(&ctx.db, "openai.model");            // Some("gpt-5") (default) or the saved value
//! setting::put(&ctx.db, "openai.key", Some("sk-..."))?;        // goes to the keychain
//! setting::put(&ctx.db, "schedule.scan_time", None)?;           // None removes it (back to the default)
//! let shown = setting::masked(&ctx.db, "openai.key");          // Some("sk-…abcd")
//! ```

use crate::db::Db;
use crate::error::Result;
use crate::util::now_ts;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const KEYRING_SERVICE: &str = "com.naimbh.manuscripttracker";
pub const SECRET_MARKER: &str = "keyring";

/// Every known key with its default (`None` = unset). Same keys as Setting::CONFIG in the web app, plus
/// the desktop-only `schedule.*` (config/tracker.php) and `app.*` preferences.
pub const DEFAULTS: &[(&str, Option<&str>)] = &[
    // AI providers (config/services.php)
    ("ai.driver", Some("claude-code")),
    ("ai.claude_bin", None),
    ("ai.claude_token", None),
    ("ai.claude_model", Some("opus")),
    ("anthropic.key", None),
    ("anthropic.model", Some("claude-opus-5-5")),
    ("openai.key", None),
    ("openai.model", Some("gpt-5")),
    ("gemini.key", None),
    ("gemini.model", Some("gemini-2.5-flash")),
    ("compatible.base_url", None),
    ("compatible.key", None),
    ("compatible.model", None),
    // Site identity: the name, tagline and credit are fixed in the app (src/lib/site.js), not settings.
    ("site.contact_email", None),
    ("site.registration", Some("1")),
    ("site.timezone", None),
    // Scheduled jobs (config/tracker.php). Days: 0 = Sunday ... 6 = Saturday. Times: HH:MM local.
    ("schedule.scan_time", Some("06:00")),
    // Per-job options (Settings -> General): on/off, daily or weekly, weekday (0 = Sunday) for weekly.
    ("schedule.scan_enabled", Some("1")),
    ("schedule.scan_frequency", Some("daily")),
    ("schedule.scan_day", Some("1")),
    ("schedule.venues_enabled", Some("1")),
    ("schedule.venues_frequency", Some("weekly")),
    ("schedule.citations_enabled", Some("1")),
    ("schedule.citations_frequency", Some("weekly")),
    ("schedule.venues_day", Some("1")),
    ("schedule.venues_time", Some("07:00")),
    ("schedule.citations_day", Some("3")),
    ("schedule.citations_time", Some("07:00")),
    // Desktop preferences
    ("app.theme", Some("system")),
    ("app.run_in_background", Some("1")),
    ("app.open_at_login", Some("0")),
    ("app.native_notifications", Some("1")),
    ("app.show_badge", Some("1")),
    ("app.auto_update", Some("1")),
    // Backup & calendar (desktop)
    ("backup.auto", Some("1")),
    ("calendar.serve", Some("0")),
    ("calendar.port", Some("47311")),
];

/// Keys stored in the OS keychain.
pub const SECRET: &[&str] = &["ai.claude_token", "anthropic.key", "openai.key", "gemini.key", "compatible.key"];

pub fn is_secret(key: &str) -> bool {
    SECRET.contains(&key)
}

pub fn default_for(key: &str) -> Option<&'static str> {
    DEFAULTS.iter().find(|(k, _)| *k == key).and_then(|(_, v)| *v)
}

/// Process-wide cache of secrets read from the keychain (avoids repeated keychain prompts).
fn secret_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(not(test))]
mod store {
    use super::KEYRING_SERVICE;
    pub fn get(key: &str) -> Option<String> {
        keyring::Entry::new(KEYRING_SERVICE, key).ok()?.get_password().ok()
    }
    pub fn set(key: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(KEYRING_SERVICE, key)
            .and_then(|e| e.set_password(value))
            .map_err(|e| {
                if cfg!(target_os = "linux") {
                    format!("Could not save to the system keyring: {e}. Make sure a Secret Service keyring (GNOME Keyring or KWallet) is installed and unlocked.")
                } else {
                    format!("Could not save to the system keychain: {e}")
                }
            })
    }
    pub fn delete(key: &str) {
        if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, key) {
            let _ = e.delete_credential();
        }
    }
}

#[cfg(test)]
mod store {
    // Tests never touch the real keychain.
    use std::collections::HashMap;
    use std::sync::Mutex;
    static MEM: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
    pub fn get(key: &str) -> Option<String> {
        MEM.lock().unwrap().as_ref().and_then(|m| m.get(key).cloned())
    }
    pub fn set(key: &str, value: &str) -> Result<(), String> {
        MEM.lock().unwrap().get_or_insert_with(HashMap::new).insert(key.into(), value.into());
        Ok(())
    }
    pub fn delete(key: &str) {
        if let Some(m) = MEM.lock().unwrap().as_mut() {
            m.remove(key);
        }
    }
}

fn read_secret(key: &str) -> Option<String> {
    let mut cache = secret_cache().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = cache.get(key) {
        return v.clone();
    }
    let v = store::get(key);
    cache.insert(key.to_string(), v.clone());
    v
}

/// The saved value, or the default. Secrets come from the keychain.
pub fn get(db: &Db, key: &str) -> Option<String> {
    let row: Option<(Option<String>, bool)> = db
        .with(|c| {
            Ok(c.prepare_cached("SELECT value, encrypted FROM settings WHERE key = ?1")?
                .query_row([key], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, bool>(1)?)))
                .ok())
        })
        .ok()
        .flatten();
    match row {
        Some((_, true)) => read_secret(key),
        Some((v, false)) => v,
        None => default_for(key).map(str::to_string),
    }
}

/// Value or `""`.
pub fn get_str(db: &Db, key: &str) -> String {
    get(db, key).unwrap_or_default()
}

/// `"1"`, `"true"`, `"on"` -> true.
pub fn get_bool(db: &Db, key: &str) -> bool {
    matches!(get(db, key).as_deref(), Some("1" | "true" | "on" | "yes"))
}

/// Integer value (e.g. schedule days), falling back to the default.
pub fn get_i64(db: &Db, key: &str) -> Option<i64> {
    get(db, key).and_then(|v| v.trim().parse().ok()).or_else(|| default_for(key).and_then(|v| v.parse().ok()))
}

/// Whether a value is saved (not just the default). For secrets: whether a key is in the keychain.
pub fn is_set(db: &Db, key: &str) -> bool {
    let saved = db.count("SELECT COUNT(*) FROM settings WHERE key = ?1", [key]).unwrap_or(0) > 0;
    saved && (!is_secret(key) || read_secret(key).is_some_and(|v| !v.is_empty()))
}

/// Save a setting. `None` removes it (falls back to the default); an empty string is stored.
pub fn put(db: &Db, key: &str, value: Option<&str>) -> Result<()> {
    let secret = is_secret(key);
    match value {
        None => {
            db.exec("DELETE FROM settings WHERE key = ?1", [key])?;
            if secret {
                store::delete(key);
                secret_cache().lock().unwrap_or_else(|e| e.into_inner()).insert(key.into(), None);
            }
        }
        Some(v) => {
            let stored = if secret {
                store::set(key, v)?;
                secret_cache().lock().unwrap_or_else(|e| e.into_inner()).insert(key.into(), Some(v.to_string()));
                SECRET_MARKER
            } else {
                v
            };
            let now = now_ts();
            db.exec(
                "INSERT INTO settings (key, value, encrypted, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, encrypted = excluded.encrypted, updated_at = excluded.updated_at",
                rusqlite::params![key, stored, secret, now],
            )?;
        }
    }
    Ok(())
}

/// Mask a secret for display: `sk-…abcd`.
pub fn mask(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 8 {
        return "••••".into();
    }
    let head: String = chars[..3].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// A secret, masked, or `None` when not set. Plain settings are returned as is.
pub fn masked(db: &Db, key: &str) -> Option<String> {
    let v = get(db, key)?;
    if v.is_empty() {
        return None;
    }
    Some(if is_secret(key) { mask(&v) } else { v })
}

/// Every known setting for the renderer: plain values, secrets masked.
pub fn all_public(db: &Db) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, _) in DEFAULTS {
        let v = if is_secret(k) { masked(db, k) } else { get(db, k) };
        map.insert((*k).to_string(), v.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null));
    }
    serde_json::Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_put_and_secrets() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(get(&db, "schedule.scan_time").as_deref(), Some("06:00"));
        assert_eq!(get_i64(&db, "schedule.venues_day"), Some(1));
        put(&db, "schedule.scan_time", Some("")).unwrap();
        assert_eq!(get(&db, "schedule.scan_time").as_deref(), Some(""));
        put(&db, "schedule.scan_time", None).unwrap();
        assert_eq!(get(&db, "schedule.scan_time").as_deref(), Some("06:00"));

        put(&db, "openai.key", Some("sk-test-1234567890abcd")).unwrap();
        let raw: Option<String> = db.scalar("SELECT value FROM settings WHERE key = 'openai.key'", []).unwrap();
        assert_eq!(raw.as_deref(), Some(SECRET_MARKER));
        assert_eq!(get(&db, "openai.key").as_deref(), Some("sk-test-1234567890abcd"));
        assert_eq!(masked(&db, "openai.key").as_deref(), Some("sk-…abcd"));
        assert!(is_set(&db, "openai.key"));
        assert_eq!(all_public(&db)["openai.key"], "sk-…abcd");
        put(&db, "openai.key", None).unwrap();
        assert!(get(&db, "openai.key").is_none());
    }
}
