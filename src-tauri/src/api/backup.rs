//! `backup.*` handlers: port of app/Support/Backup.php and DataController::export, plus desktop extras.
//!
//! Same format as the web app ("backup v2"), so a backup made in either app can be imported into the other:
//! a `.zip` with `backup.json` and `files/<n>-<name>`, or a bare `.json` without files. `backup.json` holds `app`,
//! `version` (2), `exported_at`, `profile`, `venues`, `papers` (with submissions, revision items, queue, authors,
//! deadlines and file entries `{ kind, label, name, mime, size, path? }`), `deadlines`, `publications`,
//! `opportunities` and non-secret `settings`. Version 1 files (the web app's older JSON export) import too.
//! A `.zip` also holds the profile photo as `avatar.<png|webp|jpg>` at its root, named by `profile.avatar` (JSON-only
//! backups leave both out); import sets it only when the user has no photo yet.
//!
//! | Action | Args → result |
//! | --- | --- |
//! | `backup.export` | `{ include_files? }` → native save dialog; `{ saved, papers, venues, files }` or `null` (cancelled). `.json`, or `.zip` with the JSON and a `files/` folder. |
//! | `backup.import` | `{}` → native open dialog (`.json` or `.zip`); `{ papers, venues, files, publications, opportunities, deadlines, profile, settings, user_created, summary }` or `null`. Creates user 1 from the backup's profile first when the app is not set up yet (onboarding). |
//! | `backup.local` | → `{ auto, dir, backups: [{ name, created_at, size, human_size }] }` (newest first). |
//! | `backup.now` | → makes a copy of the database now; the new list entry. |
//! | `backup.restore` | `{ name }` → replaces the data with that copy (after saving a copy of the current data). |
//! | `backup.setAuto` | `{ auto }` → daily copies on or off. |
//! | `backup.reveal` | Opens the backups folder. |
//! | `backup.cloud` | → `{ dir, service, available, auto, files, folder, last, error, keep, backups, candidates: [{ service, path }] }`. |
//! | `backup.setCloud` | `{ dir?: string \| null, pick?, auto?, files? }` → choose the synced folder (iCloud Drive, Google Drive, Dropbox, OneDrive or any folder), turn daily cloud backups or attached files on or off, or stop; the new `backup.cloud`. |
//! | `backup.cloudNow` | → today's full `.zip` (data and files) in the cloud folder now; `{ name, path, files }`. |
//! | `backup.revealCloud` | Opens the cloud backups folder. |
//! | `backup.restoreCloud` | `{ name }` → imports that cloud backup (adds records, deletes nothing); `{ summary, user_created }`. |

use crate::api::Ctx;
use crate::db::{Db, Query, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::{paper, paper_file, setting};
use crate::util::{now_ts, random_token, TS_FORMAT};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

/// Largest JSON backup accepted, as in the web app (`max:20480` kilobytes).
pub const MAX_JSON_KB: u64 = 20480;
/// Daily database copies kept in `<app data>/backups`.
pub const KEEP_LOCAL: usize = 14;
/// Name of the JSON file inside a `.zip` backup.
pub const ZIP_JSON: &str = "backup.json";

const NOT_A_BACKUP: &str = "This is not a Manuscript Tracker backup.";
const STRIP: &[&str] = &["id", "user_id", "paper_id", "venue_id", "submission_id", "created_at", "updated_at"];

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "backup.export" => export_dialog(args, ctx),
        "backup.import" => import_dialog(args, ctx),
        "backup.local" => local(ctx),
        "backup.now" => backup_now(ctx),
        "backup.restore" => restore(args, ctx),
        "backup.setAuto" => set_auto(args, ctx),
        "backup.reveal" => reveal(ctx),
        "backup.cloud" => cloud(ctx),
        "backup.setCloud" => set_cloud(args, ctx),
        "backup.cloudNow" => cloud_now(ctx),
        "backup.revealCloud" => reveal_cloud(ctx),
        "backup.restoreCloud" => restore_cloud(args, ctx),
        _ => return None,
    })
}

// ---- Dates as Laravel serialises them ----------------------------------------------------------------

/// A `datetime` cast in `toArray()`: the stored UTC timestamp as ISO 8601 with microseconds.
fn iso_ts(v: &Value) -> Value {
    match v.as_str().and_then(crate::util::parse_ts) {
        Some(t) => json!(t.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()),
        None => v.clone(),
    }
}

/// Any date the web app or this app writes (`2026-11-07`, `2026-11-07 00:00:00`, `2026-11-07T05:00:00.000000Z`)
/// as a local `YYYY-MM-DD`.
pub fn norm_date(v: &Value) -> Value {
    let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
        return Value::Null;
    };
    if s.contains('T') {
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return json!(dt.with_timezone(&Local).format("%Y-%m-%d").to_string());
        }
    }
    match crate::util::parse_date(s) {
        Some(d) => json!(d.format("%Y-%m-%d").to_string()),
        None => Value::Null,
    }
}

/// A timestamp as stored here (UTC `YYYY-MM-DD HH:MM:SS`).
fn norm_ts(v: &Value) -> Value {
    let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
        return Value::Null;
    };
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return json!(dt.with_timezone(&Utc).format(TS_FORMAT).to_string());
    }
    if let Ok(t) = NaiveDateTime::parse_from_str(s, TS_FORMAT) {
        return json!(t.format(TS_FORMAT).to_string());
    }
    match NaiveDate::parse_from_str(s.get(..10).unwrap_or(s), "%Y-%m-%d") {
        Ok(d) => json!(format!("{} 00:00:00", d.format("%Y-%m-%d"))),
        Err(_) => Value::Null,
    }
}

fn except(row: &Value, keys: &[&str]) -> Map<String, Value> {
    row.as_object()
        .map(|o| o.iter().filter(|(k, _)| !keys.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

fn as_int_bool(v: &Value) -> Value {
    match v {
        Value::Bool(b) => json!(*b as i64),
        other => other.clone(),
    }
}

fn as_bool(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_i64().unwrap_or(0) != 0),
        Value::String(s) => json!(matches!(s.as_str(), "1" | "true")),
        Value::Bool(_) => v.clone(),
        _ => json!(false),
    }
}

// ---- Export ------------------------------------------------------------------------------------------

/// Profile fields carried by a backup (never the password or tokens).
const PROFILE: &[&str] = &["name", "email", "interests", "scholar_url", "scholar_stats", "scholar_checked_at"];

/// `safeName()`: a file name that is safe inside the zip.
fn safe_name(name: &str) -> String {
    let n: String = name.chars().map(|c| if matches!(c, '/' | '\\' | '\0') { '-' } else { c }).collect();
    let n = n.trim_matches(|c| c == ' ' || c == '.').to_string();
    if n.is_empty() {
        "file".into()
    } else {
        n
    }
}

/// The app data folder for a files folder (`<app data>/files`), where `avatars/` lives too.
fn data_root(files_root: &Path) -> &Path {
    files_root.parent().unwrap_or(files_root)
}

/// `Backup::export($user)` (version 2). With `files_root` (`<app data>/files`), each paper file that exists gets a
/// `path` inside the zip (`files/<n>-<name>`), the profile photo becomes `avatar.<ext>` (`profile.avatar`), and the
/// second value lists `(zip path, absolute source path)`.
pub fn export_full(conn: &Connection, files_root: Option<&Path>) -> Result<(Value, Vec<(String, PathBuf)>)> {
    let mut packed: Vec<(String, PathBuf)> = Vec::new();
    let venues = conn.rows("SELECT * FROM venues WHERE user_id = ?1 ORDER BY name", [USER_ID])?;
    let vmap: HashMap<i64, String> = venues.iter().filter_map(|v| Some((v["id"].as_i64()?, v["name"].as_str()?.to_string()))).collect();
    let venue_name = |id: &Value| id.as_i64().and_then(|i| vmap.get(&i).cloned());

    let venues_out: Vec<Value> = venues
        .iter()
        .map(|v| {
            let mut o = except(v, STRIP);
            if let Some(x) = o.get_mut("deadline_on") {
                *x = norm_date(x);
            }
            if let Some(x) = o.get_mut("checked_at") {
                *x = iso_ts(x);
            }
            Value::Object(o)
        })
        .collect();

    let mut titles: HashMap<i64, String> = HashMap::new();
    let mut papers_out = Vec::new();
    for p in conn.rows("SELECT * FROM papers WHERE user_id = ?1 ORDER BY id", [USER_ID])? {
        let pid = p["id"].as_i64().unwrap_or(0);
        titles.insert(pid, p["title"].as_str().unwrap_or("").to_string());
        let mut o = except(&p, STRIP);
        for k in ["started_on", "preprint_on"] {
            if let Some(x) = o.get_mut(k) {
                *x = norm_date(x);
            }
        }
        let mut subs = Vec::new();
        for s in conn.rows("SELECT * FROM submissions WHERE paper_id = ?1 ORDER BY submitted_on DESC, id DESC", [pid])? {
            let sid = s["id"].as_i64().unwrap_or(0);
            let mut so = except(&s, STRIP);
            if let Some(x) = so.get_mut("is_current") {
                *x = as_int_bool(x);
            }
            so.insert("venue".into(), json!(venue_name(&s["venue_id"])));
            let items: Vec<Value> = conn
                .rows("SELECT * FROM revision_items WHERE submission_id = ?1 ORDER BY position", [sid])?
                .iter()
                .map(|r| {
                    let mut ro = except(r, &[STRIP, &["position"]].concat());
                    if let Some(x) = ro.get_mut("done") {
                        *x = as_int_bool(x);
                    }
                    Value::Object(ro)
                })
                .collect();
            so.insert("revision_items".into(), Value::Array(items));
            let steps: Vec<Value> = conn
                .rows("SELECT status, on_date FROM submission_steps WHERE submission_id = ?1 ORDER BY id", [sid])?
                .into_iter()
                .map(|st| json!({ "status": st["status"], "on_date": norm_date(&st["on_date"]) }))
                .collect();
            so.insert("steps".into(), Value::Array(steps));
            subs.push(Value::Object(so));
        }
        let queue: Vec<Value> = conn
            .rows("SELECT venue_id FROM queue_items WHERE paper_id = ?1 ORDER BY position", [pid])?
            .iter()
            .filter_map(|q| venue_name(&q["venue_id"]).map(Value::String))
            .collect();
        let authors: Vec<Value> = conn
            .rows("SELECT * FROM authors WHERE paper_id = ?1 ORDER BY position", [pid])?
            .iter()
            .map(|a| Value::Object(except(a, &[STRIP, &["position"]].concat())))
            .collect();
        let deadlines: Vec<Value> = conn
            .rows("SELECT title, due_on, done FROM deadlines WHERE paper_id = ?1 ORDER BY due_on", [pid])?
            .into_iter()
            .map(|d| json!({ "title": d["title"], "due_on": norm_date(&d["due_on"]), "done": d["done"] }))
            .collect();
        let mut files = Vec::new();
        for f in conn.rows(
            "SELECT kind, label, original_name, mime, size, path FROM paper_files WHERE paper_id = ?1 ORDER BY created_at DESC, id DESC",
            [pid],
        )? {
            let mut e = json!({ "kind": f["kind"], "label": f["label"], "name": f["original_name"], "mime": f["mime"], "size": f["size"].as_i64().unwrap_or(0) });
            if let (Some(root), Some(rel)) = (files_root, f["path"].as_str()) {
                if let Some(src) = crate::files::resolve(root, rel).ok().filter(|p| p.is_file()) {
                    let zip_path = format!("files/{}-{}", packed.len() + 1, safe_name(f["original_name"].as_str().unwrap_or("")));
                    packed.push((zip_path.clone(), src));
                    e["path"] = json!(zip_path);
                }
            }
            files.push(e);
        }
        o.insert("submissions".into(), Value::Array(subs));
        o.insert("queue".into(), Value::Array(queue));
        o.insert("authors".into(), Value::Array(authors));
        o.insert("deadlines".into(), Value::Array(deadlines));
        o.insert("files".into(), Value::Array(files));
        papers_out.push(Value::Object(o));
    }

    let deadlines: Vec<Value> = conn
        .rows("SELECT title, due_on, done FROM deadlines WHERE user_id = ?1 AND paper_id IS NULL ORDER BY due_on", [USER_ID])?
        .into_iter()
        .map(|d| json!({ "title": d["title"], "due_on": norm_date(&d["due_on"]), "done": d["done"] }))
        .collect();

    let mut profile = Map::new();
    if let Some(u) = conn.first("SELECT * FROM users WHERE id = ?1", [USER_ID])? {
        for k in PROFILE {
            let v = &u[*k];
            profile.insert((*k).into(), if *k == "scholar_checked_at" { iso_ts(v) } else { v.clone() });
        }
        if let Some(root) = files_root {
            let photo =
                u["avatar_path"].as_str().and_then(|rel| crate::files::avatar_file(data_root(root), rel).ok()).filter(|p| p.is_file());
            let kind = photo.as_ref().and_then(|p| std::fs::File::open(p).ok()).and_then(|mut f| {
                let mut head = [0u8; 16];
                f.read_exact(&mut head).ok()?;
                crate::files::image_kind(&head)
            });
            if let (Some(src), Some(kind @ ("png" | "webp" | "jpeg"))) = (photo, kind) {
                let name = format!("avatar.{}", if kind == "jpeg" { "jpg" } else { kind });
                profile.insert("avatar".into(), json!(name));
                packed.push((name, src));
            }
        }
    }

    let publications: Vec<Value> = conn
        .rows("SELECT * FROM publications WHERE user_id = ?1 ORDER BY id", [USER_ID])?
        .into_iter()
        .map(|p| {
            let paper = p["paper_id"].as_i64().and_then(|id| titles.get(&id).cloned());
            let mut o = except(&p, &["id", "user_id", "paper_id"]);
            for k in ["checked_at", "created_at", "updated_at"] {
                if let Some(x) = o.get_mut(k) {
                    *x = iso_ts(x);
                }
            }
            o.insert("paper".into(), json!(paper));
            Value::Object(o)
        })
        .collect();

    let opportunities: Vec<Value> = conn
        .rows("SELECT * FROM opportunities WHERE user_id = ?1 ORDER BY id", [USER_ID])?
        .into_iter()
        .map(|o| {
            let mut m = except(&o, &["id", "user_id", "scan_id"]);
            for k in ["created_at", "updated_at"] {
                if let Some(x) = m.get_mut(k) {
                    *x = iso_ts(x);
                }
            }
            if let Some(x) = m.get_mut("deadline_on") {
                *x = norm_date(x);
            }
            Value::Object(m)
        })
        .collect();

    let mut settings = Map::new();
    for r in conn.rows("SELECT key, value, encrypted FROM settings ORDER BY key", [])? {
        let key = r["key"].as_str().unwrap_or("");
        if r["encrypted"] == json!(true) || setting::is_secret(key) || !setting::DEFAULTS.iter().any(|(k, _)| *k == key) {
            continue;
        }
        settings.insert(key.to_string(), r["value"].clone());
    }

    let out = json!({
        "app": "manuscript-tracker",
        "version": 2,
        "exported_at": Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        "profile": profile,
        "venues": venues_out,
        "papers": papers_out,
        "deadlines": deadlines,
        "publications": publications,
        "opportunities": opportunities,
        "settings": settings,
    });
    Ok((out, packed))
}

/// The backup JSON without file paths.
pub fn export_data(conn: &Connection) -> Result<Value> {
    Ok(export_full(conn, None)?.0)
}

/// JSON with 4-space indentation, unescaped slashes and Unicode (PHP's `JSON_PRETTY_PRINT | JSON_UNESCAPED_*`).
pub fn to_pretty(v: &Value) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    v.serialize(&mut ser)?;
    Ok(buf)
}

/// Import counts, as Backup::import returns them.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub papers: i64,
    pub venues: i64,
    pub files: i64,
    pub publications: i64,
    pub opportunities: i64,
    pub deadlines: i64,
    pub profile: i64,
    pub settings: i64,
}

/// `Backup::summary()`: “Imported 2 papers, 3 files and 1 new journal or conference.”
pub fn summary(n: &Counts) -> String {
    let parts: Vec<String> = [
        (n.papers, "paper", "papers"),
        (n.files, "file", "files"),
        (n.venues, "new journal or conference", "new journals or conferences"),
        (n.publications, "publication", "publications"),
        (n.opportunities, "new call", "new calls"),
        (n.deadlines, "deadline", "deadlines"),
        (n.profile, "profile field", "profile fields"),
        (n.settings, "setting", "settings"),
    ]
    .into_iter()
    .filter(|(c, ..)| *c > 0)
    .map(|(c, one, many)| format!("{c} {}", if c == 1 { one } else { many }))
    .collect();
    match parts.len() {
        0 => "Nothing new to import.".into(),
        1 => format!("Imported {}.", parts[0]),
        _ => format!("Imported {} and {}.", parts[..parts.len() - 1].join(", "), parts[parts.len() - 1]),
    }
}

/// Write a `.zip` backup: `backup.json` plus the listed files. Returns the number of paper files written.
pub fn write_zip<W: Write + Seek>(out: W, data: &Value, packed: &[(String, PathBuf)]) -> Result<i64> {
    let mut zip = zip::ZipWriter::new(out);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
    zip.start_file(ZIP_JSON, opts).map_err(zip_err)?;
    zip.write_all(&to_pretty(data)?)?;
    let mut n = 0;
    for (entry, src) in packed {
        let Ok(mut file) = std::fs::File::open(src) else {
            continue;
        };
        zip.start_file(entry.as_str(), opts).map_err(zip_err)?;
        std::io::copy(&mut file, &mut zip)?;
        if entry.starts_with("files/") {
            n += 1;
        }
    }
    zip.finish().map_err(zip_err)?;
    Ok(n)
}

fn zip_err(e: zip::result::ZipError) -> ApiError {
    ApiError::msg(format!("Could not read or write the zip file: {e}"))
}

fn export_dialog(args: &Value, ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    let with_files = args.get("include_files").is_some_and(|v| v.as_bool() == Some(true) || v == &json!(1));
    let ext = if with_files { "zip" } else { "json" };
    let name = format!("manuscript-tracker-{}.{ext}", Local::now().format("%Y-%m-%d"));
    // Debug builds run by the end-to-end tests (`MT_DATA_DIR` + `MT_E2E`) save to `<data dir>/exports` without a dialog.
    let e2e_dir = crate::mcp::data_dir_override().filter(|_| cfg!(debug_assertions) && std::env::var_os("MT_E2E").is_some());
    let dest = if let Some(dir) = e2e_dir {
        let dir = dir.join("exports");
        std::fs::create_dir_all(&dir)?;
        dir.join(&name)
    } else {
        use tauri_plugin_dialog::DialogExt;
        let app = ctx.app()?;
        let mut dialog = app.dialog().file().set_file_name(&name).add_filter(if with_files { "Zip archive" } else { "JSON" }, &[ext]);
        if let Some(w) = tauri::Manager::get_webview_window(app, "main") {
            dialog = dialog.set_parent(&w);
        }
        let Some(dest) = dialog.blocking_save_file() else {
            return Ok(Value::Null);
        };
        dest.into_path().map_err(|e| ApiError::msg(format!("Could not save there: {e}")))?
    };
    let root = crate::files::root(ctx)?;
    let (data, packed) = ctx.db.with(|c| export_full(c, with_files.then_some(root.as_path())))?;
    let mut files = 0;
    if with_files {
        let tmp = dest.with_extension("zip.part");
        let res = std::fs::File::create(&tmp).map_err(ApiError::from).and_then(|f| write_zip(f, &data, &packed));
        match res {
            Ok(n) => {
                files = n;
                std::fs::rename(&tmp, &dest)?;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    } else {
        std::fs::write(&dest, to_pretty(&data)?)?;
    }
    Ok(json!({
        "saved": true,
        "path": dest.to_string_lossy(),
        "papers": data["papers"].as_array().map_or(0, Vec::len),
        "venues": data["venues"].as_array().map_or(0, Vec::len),
        "files": files,
    }))
}

// ---- Import ------------------------------------------------------------------------------------------

/// Column info: (name, not null, has default).
fn table_cols(conn: &Connection, table: &str) -> Result<Vec<(String, bool, bool)>> {
    let mut stmt = conn.prepare_cached(&format!("PRAGMA table_info({table})"))?;
    let cols = stmt
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(3)? != 0, r.get::<_, Option<String>>(4)?.is_some())))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(cols)
}

/// Keep the keys that are columns of `table` (minus `skip`), converting dates and booleans to how they are
/// stored, and dropping nulls for NOT NULL columns that have a default.
fn clean(
    conn: &Connection,
    table: &str,
    data: &Value,
    skip: &[&str],
    dates: &[&str],
    stamps: &[&str],
    bools: &[&str],
) -> Result<Map<String, Value>> {
    let cols = table_cols(conn, table)?;
    let mut out = Map::new();
    let Some(obj) = data.as_object() else {
        return Ok(out);
    };
    for (k, v) in obj {
        let Some((_, not_null, has_default)) = cols.iter().find(|(c, _, _)| c == k) else {
            continue;
        };
        if skip.contains(&k.as_str()) || k == "position" {
            continue;
        }
        let v = if dates.contains(&k.as_str()) {
            norm_date(v)
        } else if stamps.contains(&k.as_str()) {
            norm_ts(v)
        } else if bools.contains(&k.as_str()) {
            if v.is_null() {
                Value::Null
            } else {
                as_bool(v)
            }
        } else {
            match v {
                Value::Object(_) | Value::Array(_) if !matches!(k.as_str(), "tags" | "roles" | "sources" | "scholar_stats") => {
                    json!(v.to_string())
                }
                _ => v.clone(),
            }
        };
        if v.is_null() && *not_null {
            if *has_default {
                continue;
            }
            return Err(ApiError::field("backup", NOT_A_BACKUP));
        }
        out.insert(k.clone(), v);
    }
    Ok(out)
}

fn str_of(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Where restored files come from (a `.zip` backup) and go to (`<app data>/files`; the photo goes to
/// `<app data>/avatars`).
pub struct FileSource<'a, R: Read + Seek> {
    pub zip: &'a mut zip::ZipArchive<R>,
    pub files_root: &'a Path,
}

/// `sha256(rtrim(strtolower(url), '/'))`, as the web app keys opportunities.
pub fn url_hash(url: &str) -> String {
    use sha2::Digest;
    let norm = url.to_lowercase();
    let norm = norm.trim_end_matches('/');
    sha2::Sha256::digest(norm.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// `Backup::import($user, $data, $zip)`: adds the backup's records (v1 or v2); nothing existing is deleted.
/// Venues are matched by name, publications by title, calls for papers by URL. Profile fields are filled only
/// where empty (never the email); settings only where not set yet (never secrets).
pub fn import_data<R: Read + Seek>(db: &Db, data: &Value, mut files: Option<FileSource<'_, R>>) -> Result<Counts> {
    if data.get("app").and_then(Value::as_str) != Some("manuscript-tracker") {
        return Err(ApiError::field("backup", NOT_A_BACKUP));
    }
    let mut written: Vec<PathBuf> = Vec::new();
    let res = db.tx(|tx| {
        let mut n = Counts::default();
        let mut venue_ids: HashMap<String, i64> = HashMap::new();
        for v in data["venues"].as_array().into_iter().flatten() {
            let Some(name) = str_of(&v["name"]) else { continue };
            let existing: Option<i64> =
                tx.scalar("SELECT id FROM venues WHERE user_id = ?1 AND name = ?2 ORDER BY id LIMIT 1", rusqlite::params![USER_ID, name])?;
            let id = match existing {
                Some(id) => id,
                None => {
                    let mut row = clean(tx, "venues", v, STRIP, &["deadline_on"], &["checked_at"], &["deadline_expected", "watch"])?;
                    row.insert("user_id".into(), json!(USER_ID));
                    row.insert("name".into(), json!(name));
                    n.venues += 1;
                    tx.insert("venues", &Value::Object(row))?
                }
            };
            venue_ids.insert(name, id);
        }

        for p in data["papers"].as_array().into_iter().flatten() {
            if str_of(&p["title"]).is_none() {
                continue;
            }
            let row = clean(tx, "papers", p, STRIP, &["started_on", "preprint_on"], &[], &[])?;
            let pid = paper::insert(tx, &Value::Object(row))?;
            n.papers += 1;
            for s in p["submissions"].as_array().into_iter().flatten() {
                let mut row = clean(
                    tx,
                    "submissions",
                    s,
                    STRIP,
                    &["submitted_on", "status_on", "decision_on", "revision_due_on"],
                    &[],
                    &["is_current"],
                )?;
                row.insert("paper_id".into(), json!(pid));
                let vid = s["venue"].as_str().and_then(|name| venue_ids.get(name.trim()).copied());
                row.insert("venue_id".into(), json!(vid));
                if !row.contains_key("venue_name") {
                    row.insert("venue_name".into(), json!(s["venue"].as_str().unwrap_or("")));
                }
                let sid = tx.insert("submissions", &Value::Object(row))?;
                // The status history from the file replaces the two steps the insert started with.
                let steps: Vec<&Value> = s["steps"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|st| st["status"].as_str().is_some_and(|k| crate::models::submission::DECISIONS.iter().any(|(d, _)| *d == k)))
                    .collect();
                if !steps.is_empty() {
                    tx.exec("DELETE FROM submission_steps WHERE submission_id = ?1", [sid])?;
                    for st in steps {
                        let on = str_of(&st["on_date"]).filter(|d| crate::util::parse_date(d).is_some());
                        tx.insert("submission_steps", &json!({ "submission_id": sid, "status": st["status"], "on_date": on }))?;
                    }
                }
                for (i, r) in s["revision_items"].as_array().into_iter().flatten().enumerate() {
                    let mut row = clean(tx, "revision_items", r, STRIP, &[], &[], &["done"])?;
                    if !row.contains_key("comment") {
                        continue;
                    }
                    row.insert("submission_id".into(), json!(sid));
                    row.insert("position".into(), json!(i));
                    tx.insert("revision_items", &Value::Object(row))?;
                }
            }
            for (i, name) in p["queue"].as_array().into_iter().flatten().enumerate() {
                if let Some(vid) = name.as_str().and_then(|n| venue_ids.get(n.trim())) {
                    tx.exec(
                        "INSERT OR IGNORE INTO queue_items (paper_id, venue_id, position, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                        rusqlite::params![pid, vid, i as i64, now_ts()],
                    )?;
                }
            }
            for (i, a) in p["authors"].as_array().into_iter().flatten().enumerate() {
                let mut row = clean(tx, "authors", a, STRIP, &[], &[], &["corresponding"])?;
                if !row.contains_key("name") {
                    continue;
                }
                row.insert("paper_id".into(), json!(pid));
                row.insert("position".into(), json!(i));
                tx.insert("authors", &Value::Object(row))?;
            }
            for d in p["deadlines"].as_array().into_iter().flatten() {
                insert_deadline(tx, d, Some(pid))?;
            }
            if let Some(src) = files.as_mut() {
                for f in p["files"].as_array().into_iter().flatten() {
                    if restore_file(tx, src, f, pid, &mut written)? {
                        n.files += 1;
                    }
                }
            }
        }
        for d in data["deadlines"].as_array().into_iter().flatten() {
            if insert_deadline(tx, d, None)? {
                n.deadlines += 1;
            }
        }

        // Publications: linked to a paper by title, skipped when one with the same title exists.
        let mut paper_ids: HashMap<String, i64> = HashMap::new();
        for r in tx.rows("SELECT id, title FROM papers WHERE user_id = ?1 ORDER BY id", [USER_ID])? {
            if let (Some(t), Some(id)) = (r["title"].as_str(), r["id"].as_i64()) {
                paper_ids.insert(t.to_string(), id);
            }
        }
        let mut existing: std::collections::HashSet<String> = tx
            .rows("SELECT title FROM publications WHERE user_id = ?1", [USER_ID])?
            .iter()
            .filter_map(|r| r["title"].as_str().map(|t| t.trim().to_lowercase()))
            .collect();
        for pubn in data["publications"].as_array().into_iter().flatten() {
            let Some(title) = str_of(&pubn["title"]) else { continue };
            let key = title.to_lowercase();
            if existing.contains(&key) {
                continue;
            }
            let mut row = clean(tx, "publications", pubn, &["id", "user_id", "paper_id"], &[], &["checked_at", "created_at", "updated_at"], &[])?;
            row.insert("user_id".into(), json!(USER_ID));
            row.insert("paper_id".into(), json!(pubn["paper"].as_str().and_then(|t| paper_ids.get(t).copied())));
            tx.insert("publications", &Value::Object(row))?;
            existing.insert(key);
            n.publications += 1;
        }

        // Calls for papers, matched by URL.
        for o in data["opportunities"].as_array().into_iter().flatten() {
            let Some(url) = str_of(&o["url"]) else { continue };
            let hash = str_of(&o["url_hash"]).unwrap_or_else(|| url_hash(&url));
            if tx.count("SELECT COUNT(*) FROM opportunities WHERE user_id = ?1 AND url_hash = ?2", rusqlite::params![USER_ID, hash])? > 0 {
                continue;
            }
            if str_of(&o["title"]).is_none() {
                continue;
            }
            let mut row = clean(tx, "opportunities", o, &["id", "user_id", "scan_id", "url_hash"], &["deadline_on"], &["created_at", "updated_at"], &[])?;
            row.insert("user_id".into(), json!(USER_ID));
            row.insert("url_hash".into(), json!(hash));
            tx.insert("opportunities", &Value::Object(row))?;
            n.opportunities += 1;
        }

        // Profile: only fields that are still empty; never the email.
        let profile = if data["profile"].is_object() { &data["profile"] } else { &data["user"] };
        if let (Some(obj), Some(user)) = (profile.as_object(), tx.find("users", USER_ID)?) {
            let mut fill = Map::new();
            for (k, v) in obj {
                if k == "email" || !PROFILE.contains(&k.as_str()) || crate::util::blank(v) || !crate::util::blank(&user[k.as_str()]) {
                    continue;
                }
                let v = match k.as_str() {
                    "scholar_checked_at" => norm_ts(v),
                    "scholar_stats" => v.clone(),
                    _ => match v.as_str() {
                        Some(s) => json!(s),
                        None => continue,
                    },
                };
                fill.insert(k.clone(), v);
            }
            if !fill.is_empty() {
                n.profile = fill.len() as i64;
                tx.update("users", USER_ID, &Value::Object(fill))?;
            }
            // The photo (zip backups only), when the user has none yet.
            if let (Some(src), Some(entry)) = (files.as_mut(), obj.get("avatar").and_then(Value::as_str)) {
                if crate::util::blank(&user["avatar_path"]) && restore_avatar(tx, src, entry, &mut written)? {
                    n.profile += 1;
                }
            }
        }
        Ok(n)
    });
    if res.is_err() {
        for p in written {
            let _ = std::fs::remove_file(p);
        }
    }
    let mut n = res?;
    // Settings (outside the transaction: secrets would go to the keychain, but they are never imported).
    if let Some(obj) = data["settings"].as_object() {
        for (k, v) in obj {
            let known = setting::DEFAULTS.iter().any(|(d, _)| d == k);
            let skip = setting::is_secret(k) || k.starts_with("calendar.") || k == "app.open_at_login" || k.starts_with("site.");
            let value = match v {
                Value::String(s) => s.clone(),
                Value::Number(x) => x.to_string(),
                Value::Bool(b) => (if *b { "1" } else { "0" }).to_string(),
                _ => continue,
            };
            if !known || skip || db.count("SELECT COUNT(*) FROM settings WHERE key = ?1", [k])? > 0 {
                continue;
            }
            setting::put(db, k, Some(&value))?;
            n.settings += 1;
        }
    }
    Ok(n)
}

fn insert_deadline(conn: &Connection, d: &Value, paper_id: Option<i64>) -> Result<bool> {
    let (Some(title), Value::String(due)) = (str_of(&d["title"]), norm_date(&d["due_on"])) else {
        return Ok(false);
    };
    conn.insert(
        "deadlines",
        &json!({ "user_id": USER_ID, "paper_id": paper_id, "title": title, "due_on": due, "done": as_bool(&d["done"]) }),
    )?;
    Ok(true)
}

/// Copy one file out of a zip backup into `<files>/papers/<paper>/<random>.<ext>` and add its row.
fn restore_file<R: Read + Seek>(
    conn: &Connection,
    src: &mut FileSource<'_, R>,
    f: &Value,
    paper_id: i64,
    written: &mut Vec<PathBuf>,
) -> Result<bool> {
    let Some(entry) = f["path"].as_str() else {
        return Ok(false);
    };
    let mut zf = match src.zip.by_name(entry) {
        Ok(z) => z,
        Err(_) => return Ok(false),
    };
    if !zf.is_file() || zf.size() > crate::files::MAX_KB * 1024 {
        return Ok(false);
    }
    let name = str_of(&f["name"]).unwrap_or_else(|| entry.rsplit('/').next().unwrap_or("file").to_string());
    let ext = Path::new(&name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 10 && e.chars().all(|c| c.is_ascii_alphanumeric()));
    let rel = format!("papers/{paper_id}/{}{}", random_token(40), ext.map(|e| format!(".{e}")).unwrap_or_default());
    let dest = src.files_root.join(&rel);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = std::fs::File::create(&dest)?;
    written.push(dest.clone());
    let size = std::io::copy(&mut (&mut zf).take(crate::files::MAX_KB * 1024 + 1), &mut out)?;
    let kind = f["kind"].as_str().filter(|k| paper_file::KINDS.iter().any(|(x, _)| x == k)).unwrap_or("manuscript");
    let label = str_of(&f["label"]).map(|l| l.chars().take(120).collect::<String>());
    let mime = str_of(&f["mime"]).unwrap_or_else(|| crate::files::mime_for(&name).to_string());
    conn.insert(
        "paper_files",
        &json!({ "paper_id": paper_id, "kind": kind, "label": label, "path": rel, "original_name": name, "mime": mime, "size": size }),
    )?;
    Ok(true)
}

/// Copy `avatar.<ext>` out of a zip backup into `<app data>/avatars/<random>.<ext>` and point user 1 at it.
fn restore_avatar<R: Read + Seek>(conn: &Connection, src: &mut FileSource<'_, R>, entry: &str, written: &mut Vec<PathBuf>) -> Result<bool> {
    if entry.contains(['/', '\\']) || !entry.starts_with("avatar.") {
        return Ok(false);
    }
    let Ok(zf) = src.zip.by_name(entry) else {
        return Ok(false);
    };
    if !zf.is_file() || zf.size() > crate::files::AVATAR_MAX_BYTES as u64 {
        return Ok(false);
    }
    let mut bytes = Vec::new();
    zf.take(crate::files::AVATAR_MAX_BYTES as u64).read_to_end(&mut bytes)?;
    let ext = match crate::files::image_kind(&bytes) {
        Some("png") => "png",
        Some("webp") => "webp",
        Some("jpeg") => "jpg",
        _ => return Ok(false),
    };
    let (rel, dest) = crate::files::write_avatar(data_root(src.files_root), &bytes, ext)?;
    written.push(dest);
    conn.update("users", USER_ID, &json!({ "avatar_path": rel }))?;
    Ok(true)
}

/// Create user 1 from the backup's profile (or a default) when the app is not set up yet.
pub fn ensure_user(ctx: &Ctx, data: &Value) -> Result<bool> {
    if ctx.db.find("users", USER_ID)?.is_some() {
        return Ok(false);
    }
    let u = if data["profile"].is_object() { &data["profile"] } else { &data["user"] };
    let email = str_of(&u["email"]).filter(|e| crate::validation::is_email(e)).map(|e| e.to_lowercase());
    ctx.db.insert(
        "users",
        &json!({
            "id": USER_ID,
            "name": str_of(&u["name"]).map(|n| n.chars().take(255).collect::<String>()).unwrap_or_else(|| "Researcher".into()),
            "email": email,
            "is_admin": true,
            "calendar_token": random_token(40),
            "mcp_token": format!("mt_{}", random_token(40)),
        }),
    )?;
    crate::api::venues::on_user_created(ctx)?;
    Ok(true)
}

/// Read a `.json` (v1 or v2) or `.zip` (v2) backup from disk and import it.
pub fn import_path(ctx: &Ctx, path: &Path, files_root: &Path) -> Result<(Counts, bool)> {
    let is_zip = {
        let mut magic = [0u8; 4];
        let mut f = std::fs::File::open(path)?;
        f.read(&mut magic)? == 4 && magic == *b"PK\x03\x04"
    };
    let too_big = || ApiError::field("backup", format!("The backup field must not be greater than {MAX_JSON_KB} kilobytes."));
    if is_zip {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|_| ApiError::field("backup", NOT_A_BACKUP))?;
        let data: Value = {
            let name = [ZIP_JSON, "manuscript-tracker.json"]
                .into_iter()
                .find(|n| zip.index_for_name(n).is_some())
                .ok_or_else(|| ApiError::field("backup", "This zip has no backup.json, so it is not a Manuscript Tracker backup."))?;
            let mut entry = zip.by_name(name).map_err(|_| ApiError::field("backup", NOT_A_BACKUP))?;
            if entry.size() > MAX_JSON_KB * 1024 {
                return Err(too_big());
            }
            let mut s = String::new();
            entry.read_to_string(&mut s).map_err(|_| ApiError::field("backup", NOT_A_BACKUP))?;
            serde_json::from_str(&s).map_err(|_| ApiError::field("backup", NOT_A_BACKUP))?
        };
        check(&data)?;
        let created = ensure_user(ctx, &data)?;
        let n = import_data(&ctx.db, &data, Some(FileSource { zip: &mut zip, files_root }))?;
        Ok((n, created))
    } else {
        if std::fs::metadata(path)?.len() > MAX_JSON_KB * 1024 {
            return Err(too_big());
        }
        let text = std::fs::read(path)?;
        let data: Value = serde_json::from_slice(&text).map_err(|_| ApiError::field("backup", NOT_A_BACKUP))?;
        check(&data)?;
        let created = ensure_user(ctx, &data)?;
        let n = import_data::<std::fs::File>(&ctx.db, &data, None)?;
        Ok((n, created))
    }
}

fn check(data: &Value) -> Result<()> {
    if !data.is_object() || data.get("app").and_then(Value::as_str) != Some("manuscript-tracker") {
        return Err(ApiError::field("backup", NOT_A_BACKUP));
    }
    Ok(())
}

/// `{}` → native open dialog. Debug builds with a separate data folder (`MT_DATA_DIR`) also take `{ path }`
/// so visual QA can load a demo backup without the dialog.
fn import_dialog(args: &Value, ctx: &Ctx) -> Result<Value> {
    use tauri_plugin_dialog::DialogExt;
    let given = args.get("path").and_then(Value::as_str).filter(|_| cfg!(debug_assertions) && crate::mcp::data_dir_override().is_some());
    let path = match given {
        Some(p) => PathBuf::from(p),
        None => {
            let app = ctx.app()?;
            let mut dialog = app.dialog().file().add_filter("Manuscript Tracker backup", &["json", "zip"]);
            if let Some(w) = tauri::Manager::get_webview_window(app, "main") {
                dialog = dialog.set_parent(&w);
            }
            let Some(picked) = dialog.blocking_pick_file() else {
                return Ok(Value::Null);
            };
            picked.into_path().map_err(|e| ApiError::msg(format!("Could not open the file: {e}")))?
        }
    };
    let root = crate::files::root(ctx)?;
    let (n, created) = import_path(ctx, &path, &root)?;
    for area in ["papers", "venues", "deadlines", "desk", "publications", "opportunities", "settings"] {
        ctx.changed(area);
    }
    if let Some(user) = ctx.user()? {
        ctx.emit("user", &user);
    }
    let mut out = serde_json::to_value(n)?;
    out["user_created"] = json!(created);
    out["summary"] = json!(summary(&n));
    Ok(out)
}

// ---- Local database copies -------------------------------------------------------------------------------

/// `<app data>/backups`.
pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

fn valid_copy_name(name: &str) -> bool {
    name.starts_with("tracker-")
        && name.ends_with(".sqlite")
        && name.len() < 64
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && !name.contains("..")
}

/// `tracker-2026-11-22_064501.sqlite` -> local `2026-11-22 06:45:01`.
fn copy_time(name: &str) -> Option<String> {
    let stem = name.strip_prefix("tracker-")?.strip_suffix(".sqlite")?;
    let t = NaiveDateTime::parse_from_str(stem.get(..17)?, "%Y-%m-%d_%H%M%S").ok()?;
    Some(t.format(TS_FORMAT).to_string())
}

/// Copies in the folder, newest first.
pub fn list_copies(dir: &Path) -> Vec<Value> {
    let mut out: Vec<(String, Value)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !valid_copy_name(&name) {
                return None;
            }
            let created = copy_time(&name)?;
            let size = e.metadata().ok()?.len() as i64;
            Some((name.clone(), json!({ "name": name, "created_at": created, "size": size, "human_size": paper_file::human_size(size) })))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, v)| v).collect()
}

/// Copy the live database with `VACUUM INTO` (consistent, compact) and keep the newest [`KEEP_LOCAL`].
pub fn make_copy(db: &Db, dir: &Path) -> Result<Value> {
    std::fs::create_dir_all(dir)?;
    let mut name = format!("tracker-{}.sqlite", Local::now().format("%Y-%m-%d_%H%M%S"));
    if dir.join(&name).exists() {
        name = format!("tracker-{}-{}.sqlite", Local::now().format("%Y-%m-%d_%H%M%S"), random_token(4));
    }
    let tmp = dir.join(format!("{name}.part"));
    let _ = std::fs::remove_file(&tmp);
    db.with(|c| {
        c.execute("VACUUM INTO ?1", [tmp.to_string_lossy().as_ref()])?;
        Ok(())
    })?;
    std::fs::rename(&tmp, dir.join(&name))?;
    prune(dir);
    Ok(list_copies(dir).into_iter().find(|v| v["name"] == name.as_str()).unwrap_or(json!({ "name": name })))
}

fn prune(dir: &Path) {
    for old in list_copies(dir).into_iter().skip(KEEP_LOCAL) {
        if let Some(n) = old["name"].as_str() {
            let _ = std::fs::remove_file(dir.join(n));
        }
    }
}

/// Make today's copy if there is none yet.
pub fn ensure_daily(db: &Db, dir: &Path) -> Result<bool> {
    let today = format!("tracker-{}_", Local::now().format("%Y-%m-%d"));
    if list_copies(dir).iter().any(|v| v["name"].as_str().is_some_and(|n| n.starts_with(&today))) {
        return Ok(false);
    }
    make_copy(db, dir)?;
    Ok(true)
}

/// Replace every table's rows with those of a copy. Columns are matched by name, so older copies still load.
pub fn restore_copy(db: &Db, file: &Path) -> Result<()> {
    let conn = db.conn();
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let res = (|| -> Result<()> {
        conn.execute("ATTACH DATABASE ?1 AS snap", [file.to_string_lossy().as_ref()])?;
        let inner = (|| -> Result<()> {
            let tables: Vec<String> = conn
                .prepare("SELECT name FROM main.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let snap_tables: Vec<String> = conn
                .prepare("SELECT name FROM snap.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            if !snap_tables.iter().any(|t| t == "users") {
                return Err(ApiError::msg("This copy is not a Manuscript Tracker database."));
            }
            conn.execute_batch("BEGIN IMMEDIATE;")?;
            let copy = (|| -> Result<()> {
                for t in &tables {
                    if !t.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
                        continue;
                    }
                    conn.execute(&format!("DELETE FROM main.{t}"), [])?;
                    if !snap_tables.contains(t) {
                        continue;
                    }
                    let main_cols = conn.columns(t)?;
                    let snap_cols: Vec<String> = conn
                        .prepare(&format!("PRAGMA snap.table_info({t})"))?
                        .query_map([], |r| r.get::<_, String>(1))?
                        .collect::<rusqlite::Result<_>>()?;
                    let cols: Vec<String> = main_cols.into_iter().filter(|c| snap_cols.contains(c)).map(|c| format!("\"{c}\"")).collect();
                    if cols.is_empty() {
                        continue;
                    }
                    let list = cols.join(", ");
                    conn.execute(&format!("INSERT INTO main.{t} ({list}) SELECT {list} FROM snap.{t}"), [])?;
                }
                // Copying submissions fired the status-history triggers: keep the copy's own history,
                // or start one for a copy made before the history existed.
                conn.execute("DELETE FROM main.submission_steps", [])?;
                if snap_tables.iter().any(|t| t == "submission_steps") {
                    conn.execute(
                        "INSERT INTO main.submission_steps (id, submission_id, status, on_date, created_at, updated_at)
                         SELECT id, submission_id, status, on_date, created_at, updated_at FROM snap.submission_steps",
                        [],
                    )?;
                } else {
                    conn.execute_batch(crate::db::STEPS_BACKFILL)?;
                }
                Ok(())
            })();
            match copy {
                Ok(()) => conn.execute_batch("COMMIT;")?,
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err(e);
                }
            }
            Ok(())
        })();
        let _ = conn.execute_batch("DETACH DATABASE snap;");
        inner
    })();
    let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");
    res
}

fn local(ctx: &Ctx) -> Result<Value> {
    let dir = backups_dir(&ctx.data_dir()?);
    Ok(json!({
        "auto": setting::get_bool(&ctx.db, "backup.auto"),
        "keep": KEEP_LOCAL,
        "dir": dir.to_string_lossy(),
        "backups": list_copies(&dir),
    }))
}

fn backup_now(ctx: &Ctx) -> Result<Value> {
    make_copy(&ctx.db, &backups_dir(&ctx.data_dir()?))
}

fn restore(args: &Value, ctx: &Ctx) -> Result<Value> {
    let name = args.get("name").and_then(Value::as_str).unwrap_or("");
    if !valid_copy_name(name) {
        return Err(ApiError::not_found());
    }
    let dir = backups_dir(&ctx.data_dir()?);
    let file = dir.join(name);
    if !file.is_file() {
        return Err(ApiError::not_found());
    }
    // Keep the current data too, so a restore can be undone. Copy the chosen file first: pruning may remove it.
    let staged = dir.join(format!("restore-{}.tmp", random_token(8)));
    std::fs::copy(&file, &staged)?;
    let res = make_copy(&ctx.db, &dir).and_then(|_| restore_copy(&ctx.db, &staged));
    let _ = std::fs::remove_file(&staged);
    res?;
    for area in ["papers", "venues", "deadlines", "desk", "publications", "opportunities", "notifications", "settings"] {
        ctx.changed(area);
    }
    Ok(json!(true))
}

fn set_auto(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = crate::validation::Validator::new(args).rule("auto", "required|boolean").validate()?;
    let on = v.bool("auto");
    setting::put(&ctx.db, "backup.auto", Some(if on { "1" } else { "0" }))?;
    if on {
        let _ = ensure_daily(&ctx.db, &backups_dir(&ctx.data_dir()?));
    }
    local(ctx)
}

fn reveal(ctx: &Ctx) -> Result<Value> {
    let dir = backups_dir(&ctx.data_dir()?);
    std::fs::create_dir_all(&dir)?;
    use tauri_plugin_opener::OpenerExt;
    ctx.app()?
        .opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| ApiError::msg(format!("Could not open the folder: {e}")))?;
    Ok(json!(true))
}

/// Start the daily copy thread (first check a minute after launch, then hourly).
pub fn start_auto(ctx: Ctx) {
    std::thread::Builder::new()
        .name("local-backups".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(60));
            loop {
                let has_user = ctx.db.find("users", USER_ID).ok().flatten().is_some();
                if setting::get_bool(&ctx.db, "backup.auto") && has_user {
                    if let Ok(dir) = ctx.data_dir() {
                        let _ = ensure_daily(&ctx.db, &backups_dir(&dir));
                    }
                }
                if has_user && cloud_auto(&ctx.db) {
                    let _ = ensure_cloud_daily(&ctx);
                }
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        })
        .ok();
}

// ---- Backups in a synced cloud folder ----------------------------------------------------------------
//
// Free cloud services (iCloud Drive, Google Drive, Dropbox, OneDrive) all sync a folder on the computer.
// The app writes a full `.zip` backup (data and attached files) into a "Manuscript Tracker backups" folder
// inside the one the user chooses, once a day, and the service uploads it. No account, sign-in or key is
// needed, and nothing goes anywhere the user's own sync client does not already send it. The folder is a
// per-computer setting, so it is not part of exported backups.

/// Zip backups kept in the cloud folder.
pub const KEEP_CLOUD: usize = 10;
/// Folder the backups go into, inside the chosen synced folder.
pub const CLOUD_SUBDIR: &str = "Manuscript Tracker backups";
const CLOUD_DIR_KEY: &str = "backup.cloud_dir";
const CLOUD_AUTO_KEY: &str = "backup.cloud_auto";
const CLOUD_FILES_KEY: &str = "backup.cloud_files";

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The service a folder belongs to, from its path.
pub fn service_of(path: &Path) -> &'static str {
    let p = path.to_string_lossy().to_lowercase().replace('\\', "/");
    if p.contains("mobile documents/com~apple~clouddocs") || p.contains("/iclouddrive") {
        "iCloud Drive"
    } else if p.contains("googledrive") || p.contains("google drive") || p.contains("/my drive") {
        "Google Drive"
    } else if p.contains("dropbox") {
        "Dropbox"
    } else if p.contains("onedrive") {
        "OneDrive"
    } else if p.contains("/box") || p.contains("box-box") {
        "Box"
    } else if p.contains("pcloud") {
        "pCloud"
    } else {
        "Folder"
    }
}

/// Synced folders of common cloud services found on this computer, as `(service, path)`.
pub fn cloud_candidates() -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    // Shortcuts such as ~/Google Drive point at the same folders: compare real paths, and use the account's
    // "My Drive" for Google Drive.
    let mut add = |p: PathBuf| {
        let Ok(real) = std::fs::canonicalize(&p) else {
            return;
        };
        let is_google_account = real.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().starts_with("googledrive"));
        let real = if is_google_account {
            std::fs::read_dir(&real)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .find(|q| q.is_dir() && q.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().contains("my drive")))
                .unwrap_or(real)
        } else {
            real
        };
        if real.is_dir() && !out.iter().any(|(_, q)| q == &real) {
            out.push((service_of(&real).to_string(), real));
        }
    };
    let Some(home) = home_dir() else {
        return out;
    };
    // macOS: iCloud Drive, and the File Provider folders of Google Drive, Dropbox, OneDrive and Box.
    add(home.join("Library/Mobile Documents/com~apple~CloudDocs"));
    if let Ok(rd) = std::fs::read_dir(home.join("Library/CloudStorage")) {
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        entries.sort();
        for p in entries {
            let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            // Google Drive for desktop mounts the account; add() picks its "My Drive".
            if name.starts_with("googledrive")
                || name.starts_with("dropbox")
                || name.starts_with("onedrive")
                || name.starts_with("box")
                || name.starts_with("pcloud")
            {
                add(p);
            }
        }
    }
    // Windows and Linux, and older macOS clients.
    add(home.join("iCloudDrive"));
    for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            add(PathBuf::from(v));
        }
    }
    for name in ["OneDrive", "Dropbox", "Google Drive", "My Drive", "pCloudDrive", "Box"] {
        add(home.join(name));
    }
    if cfg!(windows) {
        // Google Drive for desktop shows up as its own drive letter (G: by default).
        for letter in 'D'..='Z' {
            add(PathBuf::from(format!("{letter}:\\My Drive")));
        }
    }
    out
}

fn cloud_dir(db: &Db) -> Option<PathBuf> {
    setting::get(db, CLOUD_DIR_KEY).filter(|s| !s.trim().is_empty()).map(PathBuf::from)
}

fn cloud_auto(db: &Db) -> bool {
    cloud_dir(db).is_some() && setting::get(db, CLOUD_AUTO_KEY).is_none_or(|v| matches!(v.as_str(), "1" | "true" | "on" | "yes"))
}

/// Whether cloud backups include the attached files (on unless turned off).
fn cloud_files(db: &Db) -> bool {
    setting::get(db, CLOUD_FILES_KEY).is_none_or(|v| matches!(v.as_str(), "1" | "true" | "on" | "yes"))
}

fn valid_cloud_name(name: &str) -> bool {
    name.starts_with("manuscript-tracker-") && name.ends_with(".zip") && !name.contains("..") && !name.contains('/')
}

/// Zip backups in the cloud folder, newest first.
fn cloud_list(dir: &Path) -> Vec<Value> {
    let mut out: Vec<(String, Value)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !valid_cloud_name(&name) {
                return None;
            }
            let meta = e.metadata().ok()?;
            let modified: DateTime<Local> = meta.modified().ok()?.into();
            let size = meta.len() as i64;
            Some((
                modified.format("%Y%m%d%H%M%S").to_string(),
                json!({ "name": name, "created_at": modified.format(TS_FORMAT).to_string(), "size": size, "human_size": paper_file::human_size(size) }),
            ))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, v)| v).collect()
}

/// Write today's full backup (data and files) into `<chosen folder>/Manuscript Tracker backups`.
fn write_cloud_backup(ctx: &Ctx, base: &Path) -> Result<Value> {
    if !base.is_dir() {
        return Err(ApiError::msg(format!(
            "The folder {} is not available. Check that your cloud app is running, or choose the folder again.",
            base.to_string_lossy()
        )));
    }
    let dir = base.join(CLOUD_SUBDIR);
    std::fs::create_dir_all(&dir)?;
    let name = format!("manuscript-tracker-{}.zip", Local::now().format("%Y-%m-%d"));
    let dest = dir.join(&name);
    let root = crate::files::root(ctx)?;
    let with_files = cloud_files(&ctx.db);
    let (data, packed) = ctx.db.with(|c| export_full(c, with_files.then_some(root.as_path())))?;
    // Write beside the target and rename, so the sync client never uploads a half-written file.
    let tmp = dir.join(format!(".{name}.part"));
    let res = std::fs::File::create(&tmp).map_err(ApiError::from).and_then(|f| write_zip(f, &data, &packed));
    let files = match res {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    std::fs::rename(&tmp, &dest)?;
    for old in cloud_list(&dir).into_iter().skip(KEEP_CLOUD) {
        if let Some(n) = old["name"].as_str().filter(|n| valid_cloud_name(n)) {
            let _ = std::fs::remove_file(dir.join(n));
        }
    }
    setting::put(&ctx.db, "backup.cloud_last", Some(&now_ts()))?;
    setting::put(&ctx.db, "backup.cloud_error", None)?;
    Ok(json!({ "name": name, "path": dest.to_string_lossy(), "files": files }))
}

/// Make today's cloud backup if there is none yet (daily thread).
pub fn ensure_cloud_daily(ctx: &Ctx) -> Result<bool> {
    let Some(base) = cloud_dir(&ctx.db) else {
        return Ok(false);
    };
    let today = format!("manuscript-tracker-{}.zip", Local::now().format("%Y-%m-%d"));
    if base.join(CLOUD_SUBDIR).join(&today).is_file() {
        return Ok(false);
    }
    match write_cloud_backup(ctx, &base) {
        Ok(_) => Ok(true),
        Err(e) => {
            let _ = setting::put(&ctx.db, "backup.cloud_error", Some(&e.to_string()));
            Err(e)
        }
    }
}

/// `backup.cloud` → `{ dir, service, auto, folder, last, error, backups, candidates: [{ service, path }] }`.
fn cloud(ctx: &Ctx) -> Result<Value> {
    let dir = cloud_dir(&ctx.db);
    let folder = dir.as_ref().map(|d| d.join(CLOUD_SUBDIR));
    Ok(json!({
        "dir": dir.as_ref().map(|d| d.to_string_lossy().to_string()),
        "service": dir.as_ref().map(|d| service_of(d)),
        "available": dir.as_ref().is_some_and(|d| d.is_dir()),
        "auto": cloud_auto(&ctx.db),
        "files": cloud_files(&ctx.db),
        "folder": folder.as_ref().map(|f| f.to_string_lossy().to_string()),
        "last": setting::get(&ctx.db, "backup.cloud_last"),
        "error": setting::get(&ctx.db, "backup.cloud_error"),
        "keep": KEEP_CLOUD,
        "backups": folder.as_ref().map(|f| cloud_list(f)).unwrap_or_default(),
        "candidates": cloud_candidates().into_iter().map(|(s, p)| json!({ "service": s, "path": p.to_string_lossy() })).collect::<Vec<_>>(),
    }))
}

/// `backup.setCloud` `{ dir?: string|null, pick?: bool, auto?: bool }`: choose the synced folder (from the list,
/// or with the folder dialog when `pick`), turn daily backups on or off, or stop (`dir: null`). Choosing a
/// folder makes the first backup straight away.
fn set_cloud(args: &Value, ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    if let Some(auto) = args.get("auto").and_then(Value::as_bool) {
        setting::put(&ctx.db, CLOUD_AUTO_KEY, Some(if auto { "1" } else { "0" }))?;
    }
    if let Some(files) = args.get("files").and_then(Value::as_bool) {
        setting::put(&ctx.db, CLOUD_FILES_KEY, Some(if files { "1" } else { "0" }))?;
    }
    let chosen: Option<PathBuf> = if args.get("pick").and_then(Value::as_bool) == Some(true) {
        use tauri_plugin_dialog::DialogExt;
        let app = ctx.app()?;
        let mut dialog = app.dialog().file().set_title("Choose a folder that your cloud service syncs");
        if let Some(home) = home_dir() {
            dialog = dialog.set_directory(home);
        }
        if let Some(w) = tauri::Manager::get_webview_window(app, "main") {
            dialog = dialog.set_parent(&w);
        }
        match dialog.blocking_pick_folder() {
            Some(p) => Some(p.into_path().map_err(|e| ApiError::msg(format!("Could not use that folder: {e}")))?),
            None => return cloud(ctx),
        }
    } else {
        match args.get("dir") {
            Some(Value::Null) => {
                setting::put(&ctx.db, CLOUD_DIR_KEY, None)?;
                setting::put(&ctx.db, "backup.cloud_error", None)?;
                return cloud(ctx);
            }
            Some(Value::String(s)) if !s.trim().is_empty() => Some(PathBuf::from(s.trim())),
            Some(_) => return Err(ApiError::field("dir", "Choose a folder.")),
            None => None,
        }
    };
    if let Some(p) = chosen {
        if !p.is_dir() {
            return Err(ApiError::field("dir", "That folder does not exist."));
        }
        // The backups go into their own folder; choosing that folder itself means its parent.
        let p = if p.file_name().is_some_and(|n| n == CLOUD_SUBDIR) { p.parent().map(Path::to_path_buf).unwrap_or(p) } else { p };
        setting::put(&ctx.db, CLOUD_DIR_KEY, Some(&p.to_string_lossy()))?;
        if let Err(e) = write_cloud_backup(ctx, &p) {
            setting::put(&ctx.db, "backup.cloud_error", Some(&e.to_string()))?;
        }
    }
    cloud(ctx)
}

/// `backup.cloudNow` → today's backup in the cloud folder now; `{ name, path, files }`.
fn cloud_now(ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    let base = cloud_dir(&ctx.db).ok_or_else(|| ApiError::msg("Choose a cloud folder first."))?;
    write_cloud_backup(ctx, &base).inspect_err(|e| {
        let _ = setting::put(&ctx.db, "backup.cloud_error", Some(&e.to_string()));
    })
}

/// `backup.revealCloud` opens the cloud backups folder.
fn reveal_cloud(ctx: &Ctx) -> Result<Value> {
    let base = cloud_dir(&ctx.db).ok_or_else(|| ApiError::msg("Choose a cloud folder first."))?;
    let dir = base.join(CLOUD_SUBDIR);
    std::fs::create_dir_all(&dir)?;
    use tauri_plugin_opener::OpenerExt;
    ctx.app()?
        .opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| ApiError::msg(format!("Could not open the folder: {e}")))?;
    Ok(json!(true))
}

/// `backup.restoreCloud` `{ name }`: adds the records of one cloud backup (the same as importing it).
fn restore_cloud(args: &Value, ctx: &Ctx) -> Result<Value> {
    let name = args.get("name").and_then(Value::as_str).unwrap_or("");
    let base = cloud_dir(&ctx.db).ok_or_else(|| ApiError::msg("Choose a cloud folder first."))?;
    if !valid_cloud_name(name) {
        return Err(ApiError::not_found());
    }
    let file = base.join(CLOUD_SUBDIR).join(name);
    if !file.is_file() {
        return Err(ApiError::not_found());
    }
    let root = crate::files::root(ctx)?;
    let (n, created) = import_path(ctx, &file, &root)?;
    for area in ["papers", "venues", "deadlines", "desk", "publications", "opportunities", "settings"] {
        ctx.changed(area);
    }
    Ok(json!({ "summary": summary(&n), "user_created": created }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_db;

    /// A backup as the web app writes it (shape and value types from DataController::export).
    const WEB_FIXTURE: &str = r#"{
    "app": "manuscript-tracker",
    "version": 1,
    "exported_at": "2026-11-20T09:15:02-05:00",
    "venues": [
        {
            "name": "Journal of Testing",
            "kind": "journal",
            "field": "Information systems",
            "publisher": "Test Press",
            "rank": null,
            "quartile": "Q1",
            "acceptance_rate": "12%",
            "first_decision": "60 days",
            "proceedings": null,
            "next_edition": null,
            "deadline_on": "2027-01-15T05:00:00.000000Z",
            "deadline_label": null,
            "deadline_expected": false,
            "review_type": "double",
            "word_limit": "8000",
            "decision_days": 90,
            "fee": null,
            "preprint_policy": "allowed",
            "portal_url": "https://example.org/portal",
            "website_url": null,
            "watch": true,
            "checked_at": "2026-11-01T12:30:00.000000Z",
            "sources": [{"url": "https://example.org", "title": "Home"}],
            "notes": null
        },
        {
            "name": "Second Conference",
            "kind": "conference",
            "field": null, "publisher": null, "rank": "A", "quartile": null, "acceptance_rate": null, "first_decision": null,
            "proceedings": "ACM", "next_edition": "2027", "deadline_on": null, "deadline_label": null, "deadline_expected": true,
            "review_type": "double", "word_limit": null, "decision_days": null, "fee": null, "preprint_policy": "unknown",
            "portal_url": null, "website_url": null, "watch": false, "checked_at": null, "sources": null, "notes": null
        }
    ],
    "papers": [
        {
            "title": "A Test Paper on Testing",
            "short_title": "Testing",
            "stage": "under_review",
            "started_on": "2026-03-02T05:00:00.000000Z",
            "tags": ["Thesis"],
            "abstract": "An abstract with “curly quotes” and ünïcode.",
            "method": "Interviews",
            "dataset": null,
            "keywords": "testing; software",
            "overleaf_url": null,
            "drive_url": null,
            "preprint_server": null,
            "preprint_url": null,
            "preprint_on": null,
            "notes": null,
            "submissions": [
                {
                    "venue_name": "Journal of Testing",
                    "manuscript_id": "JT-2026-001",
                    "portal_url": "https://example.org/portal",
                    "submitted_on": "2026-10-01",
                    "status_on": "2026-10-05",
                    "decision": "pending",
                    "is_current": 1,
                    "decision_on": null,
                    "revision_due_on": null,
                    "note": null,
                    "venue": "Journal of Testing",
                    "revision_items": [
                        {"comment": "Clarify the sample.", "response": "Done.", "done": 1}
                    ]
                },
                {
                    "venue_name": "Old Venue",
                    "manuscript_id": null,
                    "portal_url": null,
                    "submitted_on": "2026-06-01",
                    "status_on": null,
                    "decision": "desk_reject",
                    "is_current": 0,
                    "decision_on": "2026-06-03",
                    "revision_due_on": null,
                    "note": "Scope.",
                    "venue": null,
                    "revision_items": []
                }
            ],
            "queue": ["Second Conference"],
            "authors": [
                {"name": "First Author", "affiliation": "Uni", "email": "first@example.org", "orcid": null, "corresponding": true, "roles": ["Conceptualization", "Writing – original draft"]},
                {"name": "Second Author", "affiliation": null, "email": null, "orcid": null, "corresponding": false, "roles": null}
            ],
            "deadlines": [{"title": "Camera ready", "due_on": "2027-02-01", "done": false}],
            "files": [{"kind": "manuscript", "label": "Submitted PDF", "name": "paper.pdf"}]
        }
    ],
    "deadlines": [{"title": "Grant report", "due_on": "2026-12-15", "done": true}]
}"#;

    #[test]
    fn web_v1_fixture_round_trip() {
        let db = test_db();
        let data: Value = serde_json::from_str(WEB_FIXTURE).unwrap();
        let n = import_data::<std::fs::File>(&db, &data, None).unwrap();
        assert_eq!((n.papers, n.venues, n.files, n.deadlines), (1, 2, 0, 1));
        assert_eq!(summary(&n), "Imported 1 paper, 2 new journals or conferences and 1 deadline.");

        let v = db.first("SELECT * FROM venues WHERE name = 'Journal of Testing'", []).unwrap().unwrap();
        assert_eq!(v["watch"], json!(true));
        assert_eq!(v["checked_at"], "2026-11-01 12:30:00");
        assert_eq!(v["sources"][0]["title"], "Home");
        assert_eq!(v["deadline_on"].as_str().unwrap().len(), 10);
        let p = db.first("SELECT * FROM papers", []).unwrap().unwrap();
        assert_eq!(p["tags"], json!(["Thesis"]));
        assert_eq!(p["abstract"], "An abstract with “curly quotes” and ünïcode.");
        let subs = db.rows("SELECT * FROM submissions ORDER BY id", []).unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0]["is_current"], json!(true));
        assert_eq!(subs[0]["venue_id"], v["id"]);
        assert!(subs[1]["venue_id"].is_null());
        assert_eq!(db.count("SELECT COUNT(*) FROM revision_items WHERE done = 1", []).unwrap(), 1);
        assert_eq!(db.count("SELECT COUNT(*) FROM queue_items", []).unwrap(), 1);
        let a = db.rows("SELECT * FROM authors ORDER BY position", []).unwrap();
        assert_eq!(a[0]["roles"][1], "Writing – original draft");
        assert_eq!(a[0]["corresponding"], json!(true));
        assert_eq!(db.count("SELECT COUNT(*) FROM deadlines WHERE paper_id IS NOT NULL", []).unwrap(), 1);
        assert_eq!(db.count("SELECT COUNT(*) FROM deadlines WHERE paper_id IS NULL AND done = 1", []).unwrap(), 1);

        // Export (v2): the v1 part has the same structure and values as the web app's file.
        let out = db.with(export_data).unwrap();
        let keys: Vec<&String> = out.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            ["app", "version", "exported_at", "profile", "venues", "papers", "deadlines", "publications", "opportunities", "settings"]
        );
        assert_eq!(out["version"], 2);
        assert_eq!(out["papers"][0]["files"], json!([]));
        let mut fixture = data.clone();
        let mut got = out.clone();
        for k in ["exported_at", "profile", "publications", "opportunities", "settings", "version"] {
            fixture.as_object_mut().unwrap().remove(k);
            got.as_object_mut().unwrap().remove(k);
        }
        // v1 wrote dates as Laravel ISO timestamps, v2 as plain dates: compare them as dates.
        assert_eq!(got["venues"][0]["deadline_on"], norm_date(&fixture["venues"][0]["deadline_on"]));
        assert_eq!(got["papers"][0]["started_on"], norm_date(&fixture["papers"][0]["started_on"]));
        // Web backups have no status history: each submission starts with "Submitted" and its current status.
        assert_eq!(
            got["papers"][0]["submissions"][0]["steps"],
            json!([{ "status": "submitted", "on_date": "2026-10-01" }, { "status": "pending", "on_date": "2026-10-05" }])
        );
        for sub in got["papers"][0]["submissions"].as_array_mut().unwrap() {
            sub.as_object_mut().unwrap().remove("steps");
        }
        for x in [&mut fixture, &mut got] {
            x["venues"][0].as_object_mut().unwrap().remove("deadline_on");
            x["papers"][0].as_object_mut().unwrap().remove("started_on");
            // A JSON backup without files lists no files to restore here, as on the web.
            x["papers"][0]["files"] = json!([]);
        }
        assert_eq!(got, fixture);

        // Importing again adds the papers but reuses venues by name.
        let n = import_data::<std::fs::File>(&db, &out, None).unwrap();
        assert_eq!((n.papers, n.venues), (1, 0));
        assert_eq!(db.count("SELECT COUNT(*) FROM venues", []).unwrap(), 2);

        let pretty = String::from_utf8(to_pretty(&out).unwrap()).unwrap();
        assert!(pretty.contains("\n    \"app\": \"manuscript-tracker\""));
        assert!(pretty.contains("https://example.org/portal"));
    }

    #[test]
    fn v2_publications_opportunities_profile_settings() {
        let db = test_db();
        db.update("users", 1, &json!({ "email": "me@example.org", "interests": "HCI", "scholar_stats": { "citations": 12 } })).unwrap();
        let pid = db.insert("papers", &json!({ "user_id": 1, "title": "Linked paper" })).unwrap();
        db.insert("publications", &json!({ "user_id": 1, "paper_id": pid, "title": "Published one", "year": 2025, "citations": 3, "sources": [{ "url": "https://x.org" }] })).unwrap();
        db.insert("opportunities", &json!({ "user_id": 1, "title": "CFP", "url": "https://cfp.example.org/", "url_hash": url_hash("https://cfp.example.org/"), "deadline_on": "2027-03-01", "fit": 4 })).unwrap();
        setting::put(&db, "schedule.scan_time", Some("05:00")).unwrap();
        setting::put(&db, "openai.key", Some("sk-secret-should-not-leave-1234")).unwrap();
        let out = db.with(export_data).unwrap();
        assert_eq!(out["profile"]["interests"], "HCI");
        assert_eq!(out["publications"][0]["paper"], "Linked paper");
        assert_eq!(out["opportunities"][0]["deadline_on"], "2027-03-01");
        assert_eq!(out["settings"]["schedule.scan_time"], "05:00");
        assert!(out["settings"].get("openai.key").is_none());
        assert!(!to_pretty(&out).map(|b| String::from_utf8(b).unwrap()).unwrap().contains("sk-secret"));
        assert_eq!(url_hash("HTTPS://CFP.example.org/"), url_hash("https://cfp.example.org"));

        // Fresh install: user from the profile, profile fields and settings filled in.
        let tmp = std::env::temp_dir().join(format!("mt-backup-v2-{}", random_token(8)));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("b.json");
        std::fs::write(&path, to_pretty(&out).unwrap()).unwrap();
        let ctx = Ctx::for_tests(Db::open_in_memory().unwrap());
        let (n, created) = import_path(&ctx, &path, &tmp).unwrap();
        assert!(created);
        assert_eq!((n.papers, n.publications, n.opportunities), (1, 1, 1));
        assert!(n.settings >= 1);
        let u = ctx.require_user().unwrap();
        assert_eq!(u["email"], "me@example.org");
        assert_eq!(u["interests"], "HCI");
        assert_eq!(u["scholar_stats"]["citations"], 12);
        let p = ctx.db.first("SELECT * FROM publications", []).unwrap().unwrap();
        assert!(p["paper_id"].is_i64());
        assert_eq!(p["sources"][0]["url"], "https://x.org");
        assert_eq!(setting::get(&ctx.db, "schedule.scan_time").as_deref(), Some("05:00"));
        // Again: publications and calls are not duplicated, settings already set are left alone.
        let (n, _) = import_path(&ctx, &path, &tmp).unwrap();
        assert_eq!((n.publications, n.opportunities, n.settings, n.profile), (0, 0, 0, 0));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_other_files() {
        let db = test_db();
        let err = import_data::<std::fs::File>(&db, &json!({ "app": "other" }), None).unwrap_err();
        let err = serde_json::to_value(err).unwrap();
        assert_eq!(err["errors"]["backup"][0], NOT_A_BACKUP);
    }

    #[test]
    fn status_history_survives_backups() {
        let tmp = std::env::temp_dir().join(format!("mt-steps-test-{}", random_token(8)));
        std::fs::create_dir_all(tmp.join("files")).unwrap();
        let journey = |db: &Db| -> Vec<(String, String)> {
            db.rows("SELECT status, on_date FROM submission_steps ORDER BY id", [])
                .unwrap()
                .iter()
                .map(|r| (r["status"].as_str().unwrap().to_string(), r["on_date"].as_str().unwrap_or("").to_string()))
                .collect()
        };
        let full = vec![
            ("submitted".to_string(), "2026-01-02".to_string()),
            ("with_editor".to_string(), "2026-01-05".to_string()),
            ("pending".to_string(), "2026-02-01".to_string()),
        ];
        let fill = |db: &Db| {
            let pid = db.insert("papers", &json!({ "user_id": 1, "title": "Steps" })).unwrap();
            let sid = db
                .insert(
                    "submissions",
                    &json!({ "paper_id": pid, "venue_name": "J", "submitted_on": "2026-01-02", "decision": "submitted" }),
                )
                .unwrap();
            db.exec("UPDATE submissions SET decision = 'with_editor', status_on = '2026-01-05' WHERE id = ?1", [sid]).unwrap();
            db.exec("UPDATE submissions SET decision = 'pending', status_on = '2026-02-01' WHERE id = ?1", [sid]).unwrap();
        };

        // Export and JSON import keep every step, not only the two a new submission starts with.
        let db = test_db();
        fill(&db);
        let data = db.with(export_data).unwrap();
        assert_eq!(data["papers"][0]["submissions"][0]["steps"].as_array().unwrap().len(), 3);
        let json_path = tmp.join("b.json");
        std::fs::write(&json_path, to_pretty(&data).unwrap()).unwrap();
        let db2 = test_db();
        import_path(&Ctx::for_tests(db2.clone()), &json_path, &tmp.join("files")).unwrap();
        assert_eq!(journey(&db2), full);

        // A local copy brings back its own history; a copy made before the history existed starts one.
        let file_db = Db::open(&tmp.join("tracker.sqlite")).unwrap();
        file_db.insert("users", &json!({ "id": 1, "name": "Copy", "is_admin": true })).unwrap();
        fill(&file_db);
        let dir = backups_dir(&tmp);
        let copy = dir.join(make_copy(&file_db, &dir).unwrap()["name"].as_str().unwrap());
        file_db.exec("UPDATE submissions SET decision = 'reject', decision_on = '2026-03-01'", []).unwrap();
        restore_copy(&file_db, &copy).unwrap();
        assert_eq!(journey(&file_db), full);
        rusqlite::Connection::open(&copy).unwrap().execute_batch("DROP TABLE submission_steps;").unwrap();
        restore_copy(&file_db, &copy).unwrap();
        assert_eq!(journey(&file_db), vec![full[0].clone(), full[2].clone()]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn zip_with_files_and_local_copies() {
        let tmp = std::env::temp_dir().join(format!("mt-backup-test-{}", random_token(8)));
        let files_root = tmp.join("files");
        std::fs::create_dir_all(files_root.join("papers/1")).unwrap();
        let db = test_db();
        let pid = db.insert("papers", &json!({ "user_id": 1, "title": "Zip paper" })).unwrap();
        std::fs::create_dir_all(files_root.join(format!("papers/{pid}"))).unwrap();
        std::fs::write(files_root.join(format!("papers/{pid}/abc.pdf")), b"%PDF-1.4 test").unwrap();
        db.insert(
            "paper_files",
            &json!({ "paper_id": pid, "kind": "cover", "label": "Letter", "path": format!("papers/{pid}/abc.pdf"), "original_name": "letter.pdf", "size": 13 }),
        )
        .unwrap();
        // A profile photo in <app data>/avatars.
        let photo = crate::files::tests::png_header(512, 512);
        let (rel, _) = crate::files::write_avatar(&tmp, &photo, "png").unwrap();
        crate::files::set_avatar(&db, &tmp, Some(&rel)).unwrap();
        assert!(db.with(export_data).unwrap()["profile"].get("avatar").is_none(), "JSON-only backups have no photo");
        let (data, packed) = db.with(|c| export_full(c, Some(&files_root))).unwrap();
        assert_eq!(data["papers"][0]["files"][0]["path"], "files/1-letter.pdf");
        assert_eq!(data["papers"][0]["files"][0]["size"], 13);
        assert_eq!(data["profile"]["avatar"], "avatar.png");
        let zip_path = tmp.join("b.zip");
        let n = write_zip(std::fs::File::create(&zip_path).unwrap(), &data, &packed).unwrap();
        assert_eq!(n, 1);
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let mut packed_photo = Vec::new();
        archive.by_name("avatar.png").unwrap().read_to_end(&mut packed_photo).unwrap();
        assert_eq!(packed_photo, photo);

        // Restore into a fresh database and files folder (another app data folder).
        let db2 = test_db();
        let root2 = tmp.join("other").join("files");
        std::fs::create_dir_all(&root2).unwrap();
        let ctx = Ctx::for_tests(db2.clone());
        let (counts, created) = import_path(&ctx, &zip_path, &root2).unwrap();
        assert!(!created);
        assert_eq!(counts.files, 1);
        let f = db2.first("SELECT * FROM paper_files", []).unwrap().unwrap();
        assert_eq!(f["original_name"], "letter.pdf");
        assert_eq!(f["kind"], "cover");
        assert_eq!(std::fs::read(root2.join(f["path"].as_str().unwrap())).unwrap(), b"%PDF-1.4 test");
        // The photo is set because the user had none, and counts as a profile field.
        assert_eq!(counts.profile, 1);
        let other = tmp.join("other");
        let set = crate::files::current_avatar(&db2, &other).unwrap().expect("photo restored");
        assert!(set.starts_with(other.join("avatars")));
        assert_eq!(std::fs::read(&set).unwrap(), photo);
        // A second import keeps the photo the user already has.
        let (counts, _) = import_path(&ctx, &zip_path, &root2).unwrap();
        assert_eq!(counts.profile, 0);
        assert_eq!(crate::files::current_avatar(&db2, &other).unwrap(), Some(set));
        assert_eq!(std::fs::read_dir(other.join("avatars")).unwrap().count(), 1);
        // Removing the photo deletes the file.
        crate::files::set_avatar(&db2, &other, None).unwrap();
        assert_eq!(std::fs::read_dir(other.join("avatars")).unwrap().count(), 0);
        assert!(crate::files::current_avatar(&db2, &other).unwrap().is_none());

        // JSON import on a fresh install creates the user from the backup.
        let json_path = tmp.join("b.json");
        std::fs::write(&json_path, to_pretty(&data).unwrap()).unwrap();
        let ctx3 = Ctx::for_tests(Db::open_in_memory().unwrap());
        let (counts, created) = import_path(&ctx3, &json_path, &root2).unwrap();
        assert!(created);
        assert_eq!(counts.papers, 1);
        assert_eq!(ctx3.require_user().unwrap()["name"], "Test");

        // Local copies: VACUUM INTO, list, restore.
        let file_db = Db::open(&tmp.join("tracker.sqlite")).unwrap();
        file_db.insert("users", &json!({ "id": 1, "name": "Copy", "is_admin": true })).unwrap();
        file_db.insert("papers", &json!({ "user_id": 1, "title": "Kept" })).unwrap();
        let dir = backups_dir(&tmp);
        let copy = make_copy(&file_db, &dir).unwrap();
        assert!(!ensure_daily(&file_db, &dir).unwrap());
        file_db.exec("DELETE FROM papers", []).unwrap();
        file_db.insert("papers", &json!({ "user_id": 1, "title": "New" })).unwrap();
        restore_copy(&file_db, &dir.join(copy["name"].as_str().unwrap())).unwrap();
        let titles: Vec<Value> = file_db.rows("SELECT title FROM papers", []).unwrap().into_iter().map(|r| r["title"].clone()).collect();
        assert_eq!(titles, vec![json!("Kept")]);
        assert_eq!(list_copies(&dir).len(), 1);
        assert!(!valid_copy_name("../tracker.sqlite"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
