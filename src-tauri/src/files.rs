//! Paper file storage (port of FileController + PaperFile): files live in `<app data>/files/papers/<paper_id>/<random>.<ext>`;
//! `paper_files.path` stores the path relative to `<app data>/files` (like Laravel's `papers/<id>/<random>`).
//!
//! The renderer cannot read arbitrary paths: only files the user picked or dropped can be attached. Picking returns a one-time token:
//!   `files.pick {}` -> `{ token, name, size, mime }` or `null` (cancelled)
//!   `files.attach { token | path, paper_id, kind, label?, submission_id? }` -> the new `paper_files` row
//!   (`path` only for files the user dropped on the window: see the `file-drop` event)
//!   `files.list { paper_id }` -> rows with `kind_label`, `human_size`, `is_pdf`
//!   `files.open { id }` (system app), `files.saveAs { id }` -> `{ saved }`, `files.reveal { id }`,
//!   `files.preview { id }` -> `{ id, name, mime, size, kind, too_large, limit, chunk, quick_look }` (no bytes; `kind` is
//!   `pdf` | `image` | `text` | `other`), `files.chunk { id, offset, length? }` -> `{ data (base64), offset, length, eof }`
//!   (the bytes, read from disk in pieces of at most `CHUNK_BYTES`), `files.quickLook { id }` (macOS Quick Look panel),
//!   `files.delete { id }`

use crate::api::Ctx;
use crate::db::{Db, USER_ID};
use crate::error::{ApiError, Result};
use crate::models::{label, paper_file};
use crate::util::random_token;
use crate::validation::Validator;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Largest accepted upload, as in the web app (`max:51200` kilobytes).
pub const MAX_KB: u64 = 51200;
/// Largest PDF or image shown inline (as large as an upload may be).
pub const MAX_PREVIEW_BYTES: u64 = MAX_KB * 1024;
/// Text files show only their first megabyte.
pub const MAX_TEXT_BYTES: u64 = 1024 * 1024;
/// Largest piece of a file sent to the renderer in one `files.chunk` call.
pub const CHUNK_BYTES: u64 = 4 * 1024 * 1024;

fn picks() -> &'static Mutex<HashMap<String, PathBuf>> {
    static PICKS: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    PICKS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "files.pick" => pick(ctx),
        "files.attach" => attach(args, ctx),
        "files.list" => id_arg(args, "paper_id").and_then(|id| Ok(Value::Array(list_for(&ctx.db, id)?))),
        "files.open" => id_arg(args, "id").and_then(|id| open(ctx, id)).map(|_| json!(true)),
        "files.reveal" => id_arg(args, "id").and_then(|id| reveal(ctx, id)).map(|_| json!(true)),
        "files.saveAs" => id_arg(args, "id").and_then(|id| save_as(ctx, id)).map(|saved| json!({ "saved": saved })),
        "files.preview" => id_arg(args, "id").and_then(|id| preview(ctx, id)),
        "files.chunk" => id_arg(args, "id").and_then(|id| chunk(ctx, id, args)),
        "files.quickLook" => id_arg(args, "id").and_then(|id| quick_look(ctx, id)).map(|_| json!(true)),
        "files.delete" => id_arg(args, "id").and_then(|id| delete(ctx, id)).map(|_| json!(true)),
        "files.update" => update(args, ctx),
        _ => return None,
    })
}

fn id_arg(args: &Value, key: &str) -> Result<i64> {
    crate::validation::as_integer(args.get(key).unwrap_or(&Value::Null)).ok_or_else(ApiError::not_found)
}

/// `<app data>/files`, created on demand.
pub fn root(ctx: &Ctx) -> Result<PathBuf> {
    let dir = ctx.data_dir()?.join("files");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Absolute path for a stored relative path; rejects anything that escapes the files folder.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    if rel.is_empty() || !rel_path.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(ApiError::msg("Invalid file path."));
    }
    Ok(root.join(rel_path))
}

/// Mime type from the file extension.
pub fn mime_for(name: &str) -> &'static str {
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "odt" => "application/vnd.oasis.opendocument.text",
        "rtf" => "application/rtf",
        "tex" => "application/x-tex",
        "bib" => "application/x-bibtex",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "zip" => "application/zip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn safe_ext(name: &str) -> Option<String> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    (!ext.is_empty() && ext.len() <= 10 && ext.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(ext)
}

fn main_window(ctx: &Ctx) -> Option<tauri::WebviewWindow> {
    use tauri::Manager;
    ctx.app().ok()?.get_webview_window("main")
}

/// Show the native open dialog; remember the picked path behind a one-time token.
fn pick(ctx: &Ctx) -> Result<Value> {
    // Debug builds run by the end-to-end tests (`MT_DATA_DIR` + `MT_E2E`): `MT_E2E_PICK` names the file to "pick".
    if cfg!(debug_assertions) && crate::mcp::data_dir_override().is_some() && std::env::var_os("MT_E2E").is_some() {
        if let Some(path) = std::env::var_os("MT_E2E_PICK").map(PathBuf::from) {
            let size = std::fs::metadata(&path)?.len();
            let mut info = remember(path, size);
            if let Some(o) = info.as_object_mut() {
                o.remove("path");
            }
            return Ok(info);
        }
    }
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = ctx.app()?.dialog().file().set_title("Choose a file");
    if let Some(w) = main_window(ctx) {
        dialog = dialog.set_parent(&w);
    }
    let Some(picked) = dialog.blocking_pick_file() else {
        return Ok(Value::Null);
    };
    let path = picked.into_path().map_err(|e| ApiError::msg(format!("Could not read that file: {e}")))?;
    let meta = std::fs::metadata(&path)?;
    if !meta.is_file() {
        return Err(ApiError::msg("Choose a file, not a folder."));
    }
    let mut info = remember(path, meta.len());
    if let Some(o) = info.as_object_mut() {
        o.remove("path");
    }
    Ok(info)
}

/// Remember a user-chosen path behind a one-time token; returns `{ token, path, name, size, mime }`.
fn remember(path: PathBuf, size: u64) -> Value {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let token = random_token(32);
    let shown = path.to_string_lossy().to_string();
    let mut map = picks().lock().unwrap_or_else(|e| e.into_inner());
    if map.len() > 200 {
        map.clear();
    }
    map.insert(token.clone(), path);
    json!({ "token": token, "path": shown, "name": name, "size": size, "mime": mime_for(&name) })
}

/// Register files dropped on the window (folders are skipped) so they can be attached by token or path.
pub fn register_paths(paths: &[PathBuf]) -> Vec<Value> {
    paths
        .iter()
        .filter_map(|p| {
            let meta = std::fs::metadata(p).ok()?;
            meta.is_file().then(|| remember(p.clone(), meta.len()))
        })
        .collect()
}

/// Validate and store a picked file for a paper (`$paper->files()->create([...])`).
fn attach(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = Validator::new(args)
        .rule("token", "nullable|string")
        .rule("path", "nullable|string")
        .rule("paper_id", "required|integer")
        .rule("kind", &format!("required|in:{}", crate::models::keys(paper_file::KINDS)))
        .rule("label", "nullable|string|max:120")
        .rule("submission_id", "nullable|integer")
        .attribute("token", "upload")
        .attribute("kind", "file kind")
        .validate()?;
    // Only paths the user picked or dropped are accepted (never an arbitrary path from the page).
    let src = {
        let map = picks().lock().unwrap_or_else(|e| e.into_inner());
        match (v.opt("token"), v.opt("path")) {
            (Some(t), _) => map.get(&t).cloned(),
            (None, Some(p)) => map.values().find(|x| x.to_string_lossy() == p).cloned(),
            _ => None,
        }
    };
    let Some(src) = src else {
        return Err(ApiError::field("upload", "The upload field is required."));
    };
    let size = std::fs::metadata(&src).map_err(|_| ApiError::field("upload", "The upload failed to upload."))?.len();
    if size > MAX_KB * 1024 {
        return Err(ApiError::field("upload", format!("The upload field must not be greater than {MAX_KB} kilobytes.")));
    }
    let paper_id = v.i64("paper_id").unwrap_or(0);
    ctx.db.first("SELECT id FROM papers WHERE id = ?1 AND user_id = ?2", [paper_id, USER_ID])?.ok_or_else(ApiError::not_found)?;
    let submission_id = match v.i64("submission_id") {
        Some(s) => ctx.db.scalar::<i64, _>("SELECT id FROM submissions WHERE id = ?1 AND paper_id = ?2", [s, paper_id])?,
        None => None,
    };
    let original = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let rel = format!("papers/{paper_id}/{}{}", random_token(40), safe_ext(&original).map(|e| format!(".{e}")).unwrap_or_default());
    let dest = resolve(&root(ctx)?, &rel)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&src, &dest)?;
    let inserted = ctx.db.insert(
        "paper_files",
        &json!({
            "paper_id": paper_id,
            "submission_id": submission_id,
            "kind": v.string("kind"),
            "label": v.opt("label"),
            "path": rel,
            "original_name": original,
            "mime": mime_for(&original),
            "size": size,
        }),
    );
    let id = match inserted {
        Ok(id) => id,
        Err(e) => {
            let _ = std::fs::remove_file(&dest);
            return Err(e);
        }
    };
    picks().lock().unwrap_or_else(|e| e.into_inner()).retain(|_, p| *p != src);
    Ok(decorate(ctx.db.find("paper_files", id)?.ok_or_else(ApiError::not_found)?))
}

/// Add `kind_label`, `human_size` and `is_pdf` to a file row.
pub fn decorate(mut f: Value) -> Value {
    let kind = f.get("kind").and_then(Value::as_str).unwrap_or("").to_string();
    let size = f.get("size").and_then(Value::as_i64).unwrap_or(0);
    let pdf = paper_file::is_pdf(&f);
    if let Some(o) = f.as_object_mut() {
        o.insert("kind_label".into(), json!(label(paper_file::KINDS, &kind)));
        o.insert("human_size".into(), json!(paper_file::human_size(size)));
        o.insert("is_pdf".into(), json!(pdf));
    }
    f
}

/// Change a file's kind, label or submission: `{ id, kind, label?, submission_id? }` → the decorated row.
fn update(args: &Value, ctx: &Ctx) -> Result<Value> {
    let v = Validator::new(args)
        .rule("id", "required|integer")
        .rule("kind", &format!("required|in:{}", crate::models::keys(paper_file::KINDS)))
        .rule("label", "nullable|string|max:120")
        .rule("submission_id", "nullable|integer")
        .attribute("kind", "file kind")
        .validate()?;
    let id = v.i64("id").unwrap_or(0);
    let f = ctx
        .db
        .first("SELECT f.* FROM paper_files f JOIN papers p ON p.id = f.paper_id WHERE f.id = ?1 AND p.user_id = ?2", [id, USER_ID])?
        .ok_or_else(ApiError::not_found)?;
    let paper_id = f["paper_id"].as_i64().unwrap_or(0);
    let submission_id = match v.i64("submission_id") {
        Some(s) => Some(
            ctx.db
                .scalar::<i64, _>("SELECT id FROM submissions WHERE id = ?1 AND paper_id = ?2", [s, paper_id])?
                .ok_or_else(|| ApiError::field("submission_id", "Choose a submission of this paper."))?,
        ),
        None => None,
    };
    ctx.db.update("paper_files", id, &json!({ "kind": v.string("kind"), "label": v.opt("label"), "submission_id": submission_id }))?;
    list_for(&ctx.db, paper_id)?.into_iter().find(|r| r["id"] == json!(id)).ok_or_else(ApiError::not_found)
}

/// A paper's files, newest first (`$paper->files()`), decorated, with the venue of the submission each one
/// belongs to (`submission_name`).
pub fn list_for(db: &Db, paper_id: i64) -> Result<Vec<Value>> {
    Ok(db
        .rows(
            "SELECT f.*, s.venue_name AS submission_name FROM paper_files f JOIN papers p ON p.id = f.paper_id
             LEFT JOIN submissions s ON s.id = f.submission_id
             WHERE f.paper_id = ?1 AND p.user_id = ?2 ORDER BY f.created_at DESC, f.id DESC",
            [paper_id, USER_ID],
        )?
        .into_iter()
        .map(decorate)
        .collect())
}

/// The file row and its absolute path (only files of the local user's papers).
pub fn locate(ctx: &Ctx, id: i64) -> Result<(Value, PathBuf)> {
    let f = ctx
        .db
        .first("SELECT f.* FROM paper_files f JOIN papers p ON p.id = f.paper_id WHERE f.id = ?1 AND p.user_id = ?2", [id, USER_ID])?
        .ok_or_else(ApiError::not_found)?;
    let path = resolve(&root(ctx)?, f["path"].as_str().unwrap_or(""))?;
    if !path.is_file() {
        return Err(ApiError::msg("The file is missing from the app’s data folder."));
    }
    Ok((f, path))
}

/// Open with the default system app (FileController::show).
pub fn open(ctx: &Ctx, id: i64) -> Result<()> {
    let (_, path) = locate(ctx, id)?;
    use tauri_plugin_opener::OpenerExt;
    ctx.app()?.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| ApiError::msg(format!("Could not open the file: {e}")))
}

/// Show the stored file in Finder / Explorer / the file manager.
pub fn reveal(ctx: &Ctx, id: i64) -> Result<()> {
    let (_, path) = locate(ctx, id)?;
    use tauri_plugin_opener::OpenerExt;
    ctx.app()?.opener().reveal_item_in_dir(&path).map_err(|e| ApiError::msg(format!("Could not show the file: {e}")))
}

/// Save a copy under its original name (FileController::download). Returns false when cancelled.
pub fn save_as(ctx: &Ctx, id: i64) -> Result<bool> {
    let (f, path) = locate(ctx, id)?;
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = ctx.app()?.dialog().file().set_file_name(f["original_name"].as_str().unwrap_or("file"));
    if let Some(w) = main_window(ctx) {
        dialog = dialog.set_parent(&w);
    }
    let Some(dest) = dialog.blocking_save_file() else {
        return Ok(false);
    };
    let dest = dest.into_path().map_err(|e| ApiError::msg(format!("Could not save there: {e}")))?;
    std::fs::copy(&path, &dest)?;
    Ok(true)
}

/// How the renderer previews a file: `pdf` (pdf.js), `image` (`<img>`), `text` (escaped, monospace) or `other`.
pub fn preview_kind(name: &str, mime: &str) -> &'static str {
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => "pdf",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "avif" => "image",
        "txt" | "md" | "markdown" | "csv" | "tsv" | "tex" | "bib" | "json" | "log" | "xml" | "yml" | "yaml" | "r" | "py" | "do" | "sps"
        | "sas" | "ris" | "nbib" => "text",
        _ if mime == "application/pdf" => "pdf",
        _ if mime.starts_with("image/") => "image",
        _ if mime.starts_with("text/") => "text",
        _ => "other",
    }
}

/// What the renderer needs to preview a file; the bytes follow through `files.chunk`.
pub fn preview(ctx: &Ctx, id: i64) -> Result<Value> {
    let (f, path) = locate(ctx, id)?;
    let size = std::fs::metadata(&path)?.len();
    let name = f["original_name"].as_str().unwrap_or("file").to_string();
    let mime = f["mime"].as_str().map(str::to_string).unwrap_or_else(|| mime_for(&name).to_string());
    let kind = preview_kind(&name, &mime);
    let limit = if kind == "text" { MAX_TEXT_BYTES } else { MAX_PREVIEW_BYTES };
    Ok(json!({
        "id": id,
        "name": name,
        "mime": mime,
        "size": size,
        "kind": kind,
        // Text is cut at `limit` instead (`truncated` in the renderer).
        "too_large": kind != "text" && size > limit,
        "limit": limit,
        "chunk": CHUNK_BYTES,
        "quick_look": cfg!(target_os = "macos"),
    }))
}

/// A piece of a stored file, read from disk: `{ data (base64), offset, length, eof }`.
fn chunk(ctx: &Ctx, id: i64, args: &Value) -> Result<Value> {
    let (_, path) = locate(ctx, id)?;
    let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let want = args.get("length").and_then(Value::as_u64).unwrap_or(CHUNK_BYTES);
    read_chunk(&path, offset, want)
}

/// Read at most `want` (capped at `CHUNK_BYTES`) bytes from `offset`.
fn read_chunk(path: &Path, offset: u64, want: u64) -> Result<Value> {
    use std::io::{Read, Seek, SeekFrom};
    let want = want.clamp(1, CHUNK_BYTES);
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if offset > size {
        return Err(ApiError::msg("Invalid offset."));
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::with_capacity(want.min(size - offset) as usize);
    file.take(want).read_to_end(&mut buf)?;
    let length = buf.len() as u64;
    Ok(json!({ "data": base64(&buf), "offset": offset, "length": length, "eof": offset + length >= size }))
}

/// The stored file's path, checked (after resolving symlinks) to be inside the app's files folder.
fn contained_path(ctx: &Ctx, id: i64) -> Result<PathBuf> {
    let (_, path) = locate(ctx, id)?;
    let root = std::fs::canonicalize(root(ctx)?)?;
    let real = std::fs::canonicalize(&path)?;
    if !real.starts_with(&root) || !real.is_file() {
        return Err(ApiError::msg("Invalid file path."));
    }
    Ok(real)
}

/// Show the file in the native Quick Look panel (macOS): `qlmanage -p <path>`, started directly (no shell).
pub fn quick_look(ctx: &Ctx, id: i64) -> Result<()> {
    let path = contained_path(ctx, id)?;
    #[cfg(target_os = "macos")]
    {
        use std::process::{Command, Stdio};
        let mut child = Command::new("/usr/bin/qlmanage")
            .arg("-p")
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ApiError::msg(format!("Could not start Quick Look: {e}")))?;
        // Reap the process when the panel closes, without blocking the handler.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(ApiError::msg("Quick Look is only available on macOS."))
    }
}

/// Delete the stored file and its row.
pub fn delete(ctx: &Ctx, id: i64) -> Result<()> {
    let f = ctx
        .db
        .first("SELECT f.* FROM paper_files f JOIN papers p ON p.id = f.paper_id WHERE f.id = ?1 AND p.user_id = ?2", [id, USER_ID])?
        .ok_or_else(ApiError::not_found)?;
    if let Ok(path) = resolve(&root(ctx)?, f["path"].as_str().unwrap_or("")) {
        let _ = std::fs::remove_file(path);
    }
    ctx.db.delete("paper_files", id)?;
    Ok(())
}

/// Remove every stored file of a paper (call before/after deleting the paper; rows cascade on their own).
pub fn delete_paper_dir(ctx: &Ctx, paper_id: i64) -> Result<()> {
    let dir = resolve(&root(ctx)?, &format!("papers/{paper_id}"))?;
    if dir.is_dir() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

// ---- Profile photo ------------------------------------------------------------------------------------
//
// The renderer crops the photo itself (canvas) and sends a 512 × 512 PNG or WebP. The file is stored at
// `<app data>/avatars/<40 random chars>.<png|webp>`; `users.avatar_path` holds `avatars/<name>` (relative to the app
// data folder). The page never reads a path: it gets the picked or dropped source image, and the saved photo, as
// `data:` URLs (allowed by the CSP's `img-src`).

/// Largest source image the user may pick or drop.
pub const AVATAR_MAX_SOURCE: u64 = 10 * 1024 * 1024;
/// Largest cropped photo accepted from the renderer (it aims for about 200 KB).
pub const AVATAR_MAX_BYTES: usize = 2 * 1024 * 1024;
/// Side of the cropped photo in pixels.
pub const AVATAR_SIZE: u32 = 512;
/// Folder under the app data folder.
pub const AVATAR_DIR: &str = "avatars";
pub const AVATAR_TYPE_MESSAGE: &str = "Choose a PNG, JPEG or WebP image.";
pub const AVATAR_SIZE_MESSAGE: &str = "That image is too large (max 10 MB).";

/// The image format from the first bytes: `png`, `jpeg`, `webp` or `heic` (HEIC/HEIF, shown only where the web view
/// can decode it).
pub fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpeg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" | b"mif1" | b"msf1")
    {
        Some("heic")
    } else {
        None
    }
}

/// Mime type for an `image_kind`.
pub fn image_mime(kind: &str) -> &'static str {
    match kind {
        "png" => "image/png",
        "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "heic" => "image/heic",
        _ => "application/octet-stream",
    }
}

/// Width and height of a PNG or WebP image, read from its header.
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let le24 = |b: &[u8]| b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16;
    match image_kind(bytes)? {
        "png" if bytes.len() >= 24 && &bytes[12..16] == b"IHDR" => {
            Some((u32::from_be_bytes(bytes[16..20].try_into().ok()?), u32::from_be_bytes(bytes[20..24].try_into().ok()?)))
        }
        "webp" if bytes.len() >= 30 => match &bytes[12..16] {
            b"VP8X" => Some((le24(&bytes[24..27]) + 1, le24(&bytes[27..30]) + 1)),
            b"VP8L" if bytes[20] == 0x2F => {
                let b = u32::from_le_bytes(bytes[21..25].try_into().ok()?);
                Some(((b & 0x3FFF) + 1, ((b >> 14) & 0x3FFF) + 1))
            }
            b"VP8 " if bytes[23..26] == [0x9D, 0x01, 0x2A] => Some((
                (u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF) as u32,
                (u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF) as u32,
            )),
            _ => None,
        },
        _ => None,
    }
}

/// Decode standard base64 (padding optional, whitespace ignored). `None` when it is not base64.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut ended = false;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                ended = true;
                continue;
            }
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        if ended {
            return None;
        }
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// A `data:` URL for the bytes.
pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64(bytes))
}

/// Path of a stored photo (`avatars/<name>`) inside the app data folder; rejects anything else.
pub fn avatar_file(data_dir: &Path, rel: &str) -> Result<PathBuf> {
    if !rel.starts_with("avatars/") || rel.matches('/').count() != 1 {
        return Err(ApiError::msg("Invalid file path."));
    }
    resolve(data_dir, rel)
}

/// The saved photo of user 1 (its stored path), if the file exists.
pub fn current_avatar(db: &Db, data_dir: &Path) -> Result<Option<PathBuf>> {
    let rel: Option<String> = db.scalar("SELECT avatar_path FROM users WHERE id = ?1", [USER_ID])?;
    Ok(rel.filter(|r| !r.is_empty()).and_then(|r| avatar_file(data_dir, &r).ok()).filter(|p| p.is_file()))
}

/// The saved photo as a `data:` URL, or `None`.
pub fn avatar_url(ctx: &Ctx) -> Result<Option<String>> {
    let Some(path) = current_avatar(&ctx.db, &ctx.data_dir()?)? else {
        return Ok(None);
    };
    let bytes = std::fs::read(path)?;
    Ok(image_kind(&bytes).map(|k| data_url(image_mime(k), &bytes)))
}

/// Write photo bytes to `<data dir>/avatars/<random>.<ext>`; returns the relative path to store.
pub fn write_avatar(data_dir: &Path, bytes: &[u8], ext: &str) -> Result<(String, PathBuf)> {
    let rel = format!("{AVATAR_DIR}/{}.{ext}", random_token(40));
    let dest = avatar_file(data_dir, &rel)?;
    std::fs::create_dir_all(data_dir.join(AVATAR_DIR))?;
    std::fs::write(&dest, bytes)?;
    Ok((rel, dest))
}

/// Point user 1 at a new photo (or none) and delete the previous file.
pub fn set_avatar(db: &Db, data_dir: &Path, rel: Option<&str>) -> Result<()> {
    let old = current_avatar(db, data_dir)?;
    db.update("users", USER_ID, &json!({ "avatar_path": rel }))?;
    if let Some(old) = old {
        if rel.is_none_or(|r| !old.ends_with(r)) {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(())
}

/// Read a picked or dropped source image for the crop dialog: `{ name, mime, size, data }` (a `data:` URL).
pub fn read_avatar_source(path: &Path) -> Result<Value> {
    let meta = std::fs::metadata(path).map_err(|_| ApiError::field("photo", "That file could not be read."))?;
    if !meta.is_file() {
        return Err(ApiError::field("photo", AVATAR_TYPE_MESSAGE));
    }
    if meta.len() > AVATAR_MAX_SOURCE {
        return Err(ApiError::field("photo", AVATAR_SIZE_MESSAGE));
    }
    let bytes = std::fs::read(path)?;
    let kind = image_kind(&bytes).ok_or_else(|| ApiError::field("photo", AVATAR_TYPE_MESSAGE))?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "photo".into());
    Ok(json!({ "name": name, "mime": image_mime(kind), "size": bytes.len(), "data": data_url(image_mime(kind), &bytes) }))
}

/// Native open dialog for a photo; `null` when cancelled.
pub fn pick_avatar(ctx: &Ctx) -> Result<Value> {
    use tauri_plugin_dialog::DialogExt;
    // Visual QA (debug builds with a separate data folder): `MT_QA_PICK=<file>` stands in for the dialog.
    if cfg!(debug_assertions) && crate::mcp::data_dir_override().is_some() {
        if let Some(p) = std::env::var_os("MT_QA_PICK").filter(|p| !p.is_empty()) {
            return read_avatar_source(Path::new(&p));
        }
    }
    let mut dialog =
        ctx.app()?.dialog().file().set_title("Choose a photo").add_filter("Images", &["png", "jpg", "jpeg", "webp", "heic", "heif"]);
    if let Some(w) = main_window(ctx) {
        dialog = dialog.set_parent(&w);
    }
    let Some(picked) = dialog.blocking_pick_file() else {
        return Ok(Value::Null);
    };
    let path = picked.into_path().map_err(|e| ApiError::msg(format!("Could not read that file: {e}")))?;
    read_avatar_source(&path)
}

/// Read a file the user dropped on the window (by its `file-drop` token or path) for the crop dialog.
pub fn read_dropped_avatar(args: &Value) -> Result<Value> {
    let token = args.get("token").and_then(Value::as_str).filter(|s| !s.is_empty());
    let path = args.get("path").and_then(Value::as_str).filter(|s| !s.is_empty());
    let src = {
        let map = picks().lock().unwrap_or_else(|e| e.into_inner());
        match (token, path) {
            (Some(t), _) => map.get(t).cloned(),
            (None, Some(p)) => map.values().find(|x| x.to_string_lossy() == p).cloned(),
            _ => None,
        }
    };
    let src = src.ok_or_else(|| ApiError::field("photo", "That file could not be read."))?;
    read_avatar_source(&src)
}

/// Save the cropped photo sent by the renderer (`data:image/png|webp;base64,...`).
pub fn save_avatar(ctx: &Ctx, data: &str) -> Result<()> {
    let b64 = data.split_once(";base64,").map_or(data, |(_, b)| b);
    let bytes = base64_decode(b64).ok_or_else(|| ApiError::field("photo", AVATAR_TYPE_MESSAGE))?;
    validate_cropped(&bytes)?;
    let ext = if image_kind(&bytes) == Some("webp") { "webp" } else { "png" };
    let dir = ctx.data_dir()?;
    let (rel, dest) = write_avatar(&dir, &bytes, ext)?;
    if let Err(e) = set_avatar(&ctx.db, &dir, Some(&rel)) {
        let _ = std::fs::remove_file(dest);
        return Err(e);
    }
    Ok(())
}

/// A cropped photo must be a 512 × 512 PNG or WebP of at most `AVATAR_MAX_BYTES`.
pub fn validate_cropped(bytes: &[u8]) -> Result<()> {
    if bytes.len() > AVATAR_MAX_BYTES {
        return Err(ApiError::field("photo", "That photo is too large (max 2 MB)."));
    }
    if !matches!(image_kind(bytes), Some("png" | "webp")) {
        return Err(ApiError::field("photo", AVATAR_TYPE_MESSAGE));
    }
    if image_size(bytes) != Some((AVATAR_SIZE, AVATAR_SIZE)) {
        return Err(ApiError::field("photo", "The photo must be 512 × 512 pixels."));
    }
    Ok(())
}

/// Standard base64 (with padding).
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn paths_and_helpers() {
        let root = Path::new("/data/files");
        assert!(resolve(root, "papers/1/abc.pdf").is_ok());
        assert!(resolve(root, "../etc/passwd").is_err());
        assert!(resolve(root, "/etc/passwd").is_err());
        assert!(resolve(root, "papers/../../x").is_err());
        assert_eq!(mime_for("Paper.PDF"), "application/pdf");
        assert_eq!(safe_ext("a.tar.GZ").as_deref(), Some("gz"));
        assert_eq!(safe_ext("noext"), None);
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        let f = decorate(json!({ "kind": "cover", "size": 2048, "original_name": "x.pdf" }));
        assert_eq!(f["kind_label"], "Cover letter");
        assert_eq!(f["human_size"], "2 KB");
        assert_eq!(f["is_pdf"], true);
        assert_eq!(preview_kind("Paper.PDF", "application/octet-stream"), "pdf");
        assert_eq!(preview_kind("fig.JPEG", ""), "image");
        assert_eq!(preview_kind("notes.md", "text/markdown"), "text");
        assert_eq!(preview_kind("draft.docx", mime_for("draft.docx")), "other");
        assert_eq!(preview_kind("noext", "application/pdf"), "pdf");
    }

    /// Header bytes of a PNG of the given size (enough for `image_kind` and `image_size`).
    pub fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, 6, 0, 0, 0]);
        b
    }

    fn webp(chunk: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WEBP".to_vec();
        b.extend(chunk);
        b.extend((body.len() as u32).to_le_bytes());
        b.extend(body);
        b
    }

    #[test]
    fn avatar_helpers() {
        assert_eq!(image_kind(&png_header(1, 1)), Some("png"));
        assert_eq!(image_kind(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpeg"));
        assert_eq!(image_kind(b"\0\0\0\x18ftypheic\0\0\0\0"), Some("heic"));
        assert_eq!(image_kind(b"GIF89a..."), None);
        assert_eq!(image_kind(b"%PDF-1.7"), None);
        assert_eq!(image_size(&png_header(512, 512)), Some((512, 512)));
        // VP8X: 24-bit (width - 1) and (height - 1).
        let x = webp(b"VP8X", &[0, 0, 0, 0, 0xFF, 0x01, 0, 0xFF, 0x01, 0]);
        assert_eq!(image_kind(&x), Some("webp"));
        assert_eq!(image_size(&x), Some((512, 512)));
        // VP8L: 14-bit (width - 1) and (height - 1) after the 0x2F signature.
        let bits: u32 = 511 | (255 << 14);
        let mut body = vec![0x2F];
        body.extend(bits.to_le_bytes());
        body.extend([0; 8]);
        assert_eq!(image_size(&webp(b"VP8L", &body)), Some((512, 256)));
        // VP8 (lossy): frame tag, start code, 14-bit sizes.
        let mut body = vec![0, 0, 0, 0x9D, 0x01, 0x2A];
        body.extend(512u16.to_le_bytes());
        body.extend(512u16.to_le_bytes());
        assert_eq!(image_size(&webp(b"VP8 ", &body)), Some((512, 512)));

        assert_eq!(base64_decode("TWFu").unwrap(), b"Man");
        assert_eq!(base64_decode("TWE=").unwrap(), b"Ma");
        assert_eq!(base64_decode("TQ==").unwrap(), b"M");
        assert_eq!(base64_decode("TQ").unwrap(), b"M");
        assert!(base64_decode("T@==").is_none());
        let bytes: Vec<u8> = (0..=255u8).collect();
        assert_eq!(base64_decode(&base64(&bytes)).unwrap(), bytes);

        let dir = Path::new("/data");
        assert!(avatar_file(dir, "avatars/abc.webp").is_ok());
        assert!(avatar_file(dir, "files/abc.webp").is_err());
        assert!(avatar_file(dir, "avatars/../x").is_err());
        assert!(avatar_file(dir, "avatars/a/b.png").is_err());

        assert!(validate_cropped(&png_header(512, 512)).is_ok());
        let err = serde_json::to_value(validate_cropped(&png_header(600, 512)).unwrap_err()).unwrap();
        assert_eq!(err["errors"]["photo"][0], "The photo must be 512 × 512 pixels.");
        let err = serde_json::to_value(validate_cropped(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap_err()).unwrap();
        assert_eq!(err["errors"]["photo"][0], AVATAR_TYPE_MESSAGE);
    }

    #[test]
    fn avatar_sources() {
        let dir = std::env::temp_dir().join(format!("mt-avatar-src-{}", random_token(12)));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("me.png");
        std::fs::write(&png, png_header(40, 30)).unwrap();
        let v = read_avatar_source(&png).unwrap();
        assert_eq!(v["mime"], "image/png");
        assert!(v["data"].as_str().unwrap().starts_with("data:image/png;base64,iVBORw0KGgo"));
        let gif = dir.join("x.gif");
        std::fs::write(&gif, b"GIF89a\x01\0\x01\0").unwrap();
        let err = serde_json::to_value(read_avatar_source(&gif).unwrap_err()).unwrap();
        assert_eq!(err["errors"]["photo"][0], "Choose a PNG, JPEG or WebP image.");
        let big = dir.join("big.jpg");
        let f = std::fs::File::create(&big).unwrap();
        f.set_len(AVATAR_MAX_SOURCE + 1).unwrap();
        let err = serde_json::to_value(read_avatar_source(&big).unwrap_err()).unwrap();
        assert_eq!(err["errors"]["photo"][0], "That image is too large (max 10 MB).");
        // Dropped files are read only through their token.
        let token = register_paths(std::slice::from_ref(&png))[0]["token"].as_str().unwrap().to_string();
        assert_eq!(read_dropped_avatar(&json!({ "token": token })).unwrap()["mime"], "image/png");
        assert!(read_dropped_avatar(&json!({ "path": "/etc/hosts" })).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chunks() {
        let path = std::env::temp_dir().join(format!("mt-chunk-test-{}.pdf", random_token(12)));
        let bytes: Vec<u8> = (0..(CHUNK_BYTES as usize + 10)).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &bytes).unwrap();
        let a = read_chunk(&path, 0, u64::MAX).unwrap();
        assert_eq!(a["length"], CHUNK_BYTES);
        assert_eq!(a["eof"], false);
        let b = read_chunk(&path, CHUNK_BYTES, CHUNK_BYTES).unwrap();
        assert_eq!(b["length"], 10);
        assert_eq!(b["eof"], true);
        assert_eq!(b["data"], base64(&bytes[CHUNK_BYTES as usize..]));
        assert_eq!(read_chunk(&path, 0, 3).unwrap()["data"], base64(&bytes[..3]));
        assert_eq!(read_chunk(&path, bytes.len() as u64, 5).unwrap()["eof"], true);
        assert!(read_chunk(&path, bytes.len() as u64 + 1, 5).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
