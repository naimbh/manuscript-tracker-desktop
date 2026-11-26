//! Native desktop pieces: the main window (materials, release hardening), the menu bar, context menus,
//! the Dock/taskbar badge and file drops.

use crate::api::app::show_main;
use crate::api::Ctx;
use crate::error::{ApiError, Result};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::menu::{
    CheckMenuItem, CheckMenuItemBuilder, ContextMenu, Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder,
};
use tauri::{AppHandle, DragDropEvent, Manager, WebviewWindow, WebviewWindowBuilder, Window};

/// Commands in the menu bar. Each one emits `menu` `{ id }` to the renderer.
/// `reload` and `run_scans` are also handled natively (the renderer must not repeat them).
pub const MENU_IDS: &[&str] = &[
    "new_paper",
    "find",
    "settings",
    "run_scans",
    "backup_export",
    "backup_import",
    "toggle_sidebar",
    "reload",
    "go_desk",
    "go_papers",
    "go_publications",
    "go_venues",
    "go_opportunities",
    "go_deadlines",
    "help",
    "about",
    "check_updates",
];

/// Native window material in use: `"sidebar"` (macOS vibrancy) or `None` (solid: Windows and Linux).
pub fn material() -> Option<&'static str> {
    *MATERIAL.get().unwrap_or(&None)
}
static MATERIAL: OnceLock<Option<&'static str>> = OnceLock::new();

/// Release builds: no WebKit/Chromium default context menu ("Inspect", "Reload") except in text fields.
const HARDEN_SCRIPT: &str = r#"(function(){
  document.addEventListener('contextmenu', function (e) {
    if (e.defaultPrevented) return;
    var t = e.target;
    if (t && t.closest && t.closest('input, textarea, [contenteditable=""], [contenteditable="true"]')) return;
    e.preventDefault();
  });
})();"#;

/// Create the main window from tauri.conf.json (`create: false` there) with platform-specific transparency,
/// then apply the native material.
pub fn create_main_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let config = app.config().app.windows.iter().find(|w| w.label == "main").cloned().ok_or_else(|| tauri::Error::WindowNotFound)?;
    let mut builder = WebviewWindowBuilder::from_config(app, &config)?;
    // Transparent only where the OS draws a material behind the page (macOS). Windows stays solid: with a
    // transparent window and Mica behind it, the classic Win32 menu bar (File, View, Go, Window, Help) is drawn
    // half see-through and its words break up.
    #[cfg(target_os = "macos")]
    {
        builder = builder.transparent(true);
    }
    if !cfg!(debug_assertions) {
        builder = builder.initialization_script(HARDEN_SCRIPT);
    }
    // Windows debug builds: honour WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS (msedgedriver sets it to open the debugging
    // port for the end-to-end tests). The web view ignores the variable once the app passes its own arguments.
    #[cfg(all(target_os = "windows", debug_assertions))]
    if let Some(extra) = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").ok().filter(|a| !a.trim().is_empty()) {
        builder = builder.additional_browser_args(&format!("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection {extra}"));
    }
    // A separate data folder (debug builds) also gets its own web storage, apart from the installed app's.
    // Not under the end-to-end tests (`MT_E2E`): there the WebDriver chooses the web view's profile folder.
    if let Some(dir) = crate::mcp::data_dir_override().filter(|_| std::env::var_os("MT_E2E").is_none()) {
        #[cfg(target_os = "macos")]
        {
            let mut id = [0u8; 16];
            for (i, b) in dir.to_string_lossy().bytes().enumerate() {
                id[i % 16] = id[i % 16].wrapping_mul(31).wrapping_add(b);
            }
            builder = builder.data_store_identifier(id);
        }
        #[cfg(not(target_os = "macos"))]
        {
            builder = builder.data_directory(dir.join("webview"));
        }
    }
    let window = builder.build()?;
    let applied = apply_material(&window);
    let _ = MATERIAL.set(applied);
    Ok(window)
}

#[allow(unused_variables)]
fn apply_material(window: &WebviewWindow) -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        if apply_vibrancy(window, NSVisualEffectMaterial::Sidebar, Some(NSVisualEffectState::FollowsWindowActiveState), None).is_ok() {
            return Some("sidebar");
        }
    }
    // Windows: no Mica (see `create_main_window`), so the menu bar keeps a solid background.
    None
}

/// Windows: Mica follows the system theme unless told otherwise, so re-apply it in the app's own theme
/// (a dark app on a light system would otherwise get a light sidebar behind white text).
#[allow(unused_variables)]
pub fn match_material_theme(window: &WebviewWindow, dark: bool) {
    #[cfg(target_os = "windows")]
    if material() == Some("mica") {
        let _ = window_vibrancy::apply_mica(window, Some(dark));
    }
}

// ---- Always on Top ------------------------------------------------------------------------------------

/// Setting that keeps the main window above other windows (off by default; restored at launch).
pub const ON_TOP_KEY: &str = "window.always_on_top";
/// Menu-bar id of the "Always on Top" item (handled natively, not sent to the renderer as a menu command).
pub const ON_TOP_MENU_ID: &str = "always_on_top";
/// Tray id of the same toggle.
pub const ON_TOP_TRAY_ID: &str = "tray_on_top";
const ON_TOP_ACCEL: &str = "CmdOrCtrl+Shift+T";

/// Every "Always on Top" check item (menu bar and tray), so all of them show the same state.
fn on_top_items() -> &'static Mutex<Vec<CheckMenuItem<tauri::Wry>>> {
    static ITEMS: OnceLock<Mutex<Vec<CheckMenuItem<tauri::Wry>>>> = OnceLock::new();
    ITEMS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Whether the window is set to stay on top.
pub fn always_on_top(db: &crate::db::Db) -> bool {
    crate::models::setting::get_bool(db, ON_TOP_KEY)
}

/// A checkable "Always on Top" item (menu bar: with the shortcut; tray: without).
pub fn on_top_item(app: &AppHandle, id: &str, accel: bool) -> tauri::Result<CheckMenuItem<tauri::Wry>> {
    let on = always_on_top(&Ctx::new(app).db);
    let mut b = CheckMenuItemBuilder::with_id(id, "Always on Top").checked(on);
    if accel {
        b = b.accelerator(ON_TOP_ACCEL);
    }
    let item = b.build(app)?;
    on_top_items().lock().unwrap_or_else(|e| e.into_inner()).push(item.clone());
    Ok(item)
}

/// Keep the window above others (or not): applies it, saves it, updates every check mark and tells the
/// renderer (`window-state` `{ always_on_top }`, for Settings → General).
pub fn set_always_on_top(ctx: &Ctx, on: bool) -> Result<()> {
    crate::models::setting::put(&ctx.db, ON_TOP_KEY, Some(if on { "1" } else { "0" }))?;
    if let Ok(app) = ctx.app() {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_always_on_top(on);
        }
        for item in on_top_items().lock().unwrap_or_else(|e| e.into_inner()).iter() {
            let _ = item.set_checked(on);
        }
    }
    ctx.emit("window-state", json!({ "always_on_top": on }));
    Ok(())
}

/// Flip "Always on Top" (menu bar, shortcut and tray).
pub fn toggle_always_on_top(app: &AppHandle) {
    let ctx = Ctx::new(app);
    let on = !always_on_top(&ctx.db);
    let _ = set_always_on_top(&ctx, on);
}

/// At launch: put the window back on top if it was left that way.
pub fn restore_always_on_top(app: &AppHandle) {
    if always_on_top(&Ctx::new(app).db) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_always_on_top(true);
        }
    }
}

fn item(app: &AppHandle, id: &str, label: &str, accel: Option<&str>) -> tauri::Result<MenuItem<tauri::Wry>> {
    let mut b = MenuItemBuilder::with_id(id, label);
    if let Some(a) = accel {
        b = b.accelerator(a);
    }
    b.build(app)
}

/// The menu bar: macOS app menu, File, Edit, View, Go, Window, Help.
pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let name = "Manuscript Tracker";
    let settings = item(app, "settings", if cfg!(target_os = "macos") { "Settings…" } else { "Settings" }, Some("CmdOrCtrl+,"))?;

    let mut file = SubmenuBuilder::new(app, "File")
        .item(&item(app, "new_paper", "New Paper", Some("CmdOrCtrl+N"))?)
        .separator()
        .item(&item(app, "run_scans", "Run Scans Now", None)?)
        .separator()
        .item(&item(app, "backup_export", "Export Backup…", Some("CmdOrCtrl+Shift+E"))?)
        .item(&item(app, "backup_import", "Import Backup…", Some("CmdOrCtrl+Shift+I"))?)
        .separator();
    if !cfg!(target_os = "macos") {
        file = file.item(&settings).separator();
    }
    file = file.item(&PredefinedMenuItem::close_window(app, Some("Close Window"))?);
    if !cfg!(target_os = "macos") {
        file = file.item(&PredefinedMenuItem::quit(app, Some("Quit"))?);
    }

    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .separator()
        .item(&item(app, "find", "Find…", Some("CmdOrCtrl+K"))?)
        .build()?;

    let mut view = SubmenuBuilder::new(app, "View")
        .item(&item(app, "toggle_sidebar", "Toggle Sidebar", Some("CmdOrCtrl+Backslash"))?)
        .item(&item(app, "reload", "Reload", Some("CmdOrCtrl+R"))?);
    if cfg!(target_os = "macos") {
        view = view.separator().fullscreen();
    }

    let go = SubmenuBuilder::new(app, "Go")
        .item(&item(app, "go_desk", "Desk", Some("CmdOrCtrl+1"))?)
        .item(&item(app, "go_papers", "Papers", Some("CmdOrCtrl+2"))?)
        .item(&item(app, "go_publications", "Published", Some("CmdOrCtrl+3"))?)
        .item(&item(app, "go_venues", "Journals & Conferences", Some("CmdOrCtrl+4"))?)
        .item(&item(app, "go_opportunities", "Calls for Papers", Some("CmdOrCtrl+5"))?)
        .item(&item(app, "go_deadlines", "Deadlines", Some("CmdOrCtrl+6"))?)
        .build()?;

    let mut window = SubmenuBuilder::new(app, "Window").minimize().maximize().separator().item(&on_top_item(app, ON_TOP_MENU_ID, true)?);
    if cfg!(target_os = "macos") {
        window = window.separator().item(&PredefinedMenuItem::bring_all_to_front(app, None)?);
    }
    let window = window.build()?;

    let about = item(app, "about", &format!("About {name}"), None)?;
    let check_updates = item(app, "check_updates", "Check for Updates…", None)?;
    let mut help = SubmenuBuilder::new(app, "Help").item(&item(app, "help", &format!("{name} Help"), None)?);
    if !cfg!(target_os = "macos") {
        help = help.separator().item(&check_updates).item(&about);
    }
    let help = help.build()?;

    let mut menu = MenuBuilder::new(app);
    if cfg!(target_os = "macos") {
        let app_menu = SubmenuBuilder::new(app, name)
            .item(&about)
            .item(&check_updates)
            .separator()
            .item(&settings)
            .separator()
            .services()
            .separator()
            .hide()
            .hide_others()
            .show_all()
            .separator()
            .quit()
            .build()?;
        menu = menu.item(&app_menu);
    }
    menu.item(&file.build()?).item(&edit).item(&view.build()?).item(&go).item(&window).item(&help).build()
}

/// Global menu event handler (menu bar and context menus).
pub fn on_menu_event(app: &AppHandle, id: &str) {
    if let Some(rest) = id.strip_prefix("ctx:") {
        if let Some((seq, item)) = rest.split_once(':') {
            context_selected(seq.parse().unwrap_or(0), item.to_string());
        }
        return;
    }
    if id == ON_TOP_MENU_ID {
        toggle_always_on_top(app);
        return;
    }
    if !MENU_IDS.contains(&id) {
        return;
    }
    let ctx = Ctx::new(app);
    match id {
        "reload" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.reload();
            }
            return;
        }
        "run_scans" => {
            crate::scheduler::run_now(&ctx, None);
        }
        _ => {}
    }
    show_main(app);
    ctx.emit("menu", json!({ "id": id }));
}

// ---- Context menus ------------------------------------------------------------------------------------

static SEQ: AtomicU64 = AtomicU64::new(1);

/// The context menu waiting for a click: (sequence number, reply channel).
type Pending = Mutex<Option<(u64, Sender<Option<String>>)>>;

fn pending() -> &'static Pending {
    static P: OnceLock<Pending> = OnceLock::new();
    P.get_or_init(|| Mutex::new(None))
}

fn context_selected(seq: u64, id: String) {
    let mut p = pending().lock().unwrap_or_else(|e| e.into_inner());
    if p.as_ref().is_some_and(|(s, _)| *s == seq) {
        if let Some((_, tx)) = p.take() {
            let _ = tx.send(Some(id));
        }
    }
}

/// `app.contextMenu { items: [{ id, label, enabled?, separator?, danger? }] }` -> chosen id or `null`.
/// Pops up a native menu at the cursor. Blocks this worker thread (not the UI) until the menu closes.
pub fn context_menu(args: &Value, ctx: &Ctx) -> Result<Value> {
    let app = ctx.app()?;
    let window = app.get_webview_window("main").ok_or_else(|| ApiError::msg("No window."))?;
    let items = args.get("items").and_then(Value::as_array).ok_or_else(|| ApiError::field("items", "The items field is required."))?;
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let mut menu = MenuBuilder::new(app);
    for it in items {
        if it.get("separator").and_then(Value::as_bool).unwrap_or(false) {
            menu = menu.separator();
            continue;
        }
        let id = it.get("id").and_then(Value::as_str).unwrap_or("");
        let label = it.get("label").and_then(Value::as_str).unwrap_or(id);
        if id.is_empty() {
            continue;
        }
        let enabled = it.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        menu = menu.item(&MenuItemBuilder::with_id(format!("ctx:{seq}:{id}"), label).enabled(enabled).build(app)?);
    }
    let menu = menu.build()?;

    let (tx, rx) = channel::<Option<String>>();
    {
        let mut p = pending().lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, old)) = p.replace((seq, tx)) {
            let _ = old.send(None); // an older menu that never reported is superseded
        }
    }
    let w: Window = window.as_ref().window();
    menu.popup(w)?;
    // macOS and Windows: popup() returns after the menu closes; the click event may arrive just after.
    // Linux (GTK): popup() returns at once, so wait for the click (a new menu supersedes this one).
    let wait = if cfg!(target_os = "linux") { Duration::from_secs(120) } else { Duration::from_millis(400) };
    let chosen = rx.recv_timeout(wait).ok().flatten();
    let mut p = pending().lock().unwrap_or_else(|e| e.into_inner());
    if p.as_ref().is_some_and(|(s, _)| *s == seq) {
        *p = None;
    }
    Ok(chosen.map(Value::String).unwrap_or(Value::Null))
}

// ---- Badge --------------------------------------------------------------------------------------------

/// A small red dot used as the Windows taskbar overlay.
#[cfg(target_os = "windows")]
fn dot_icon() -> tauri::image::Image<'static> {
    let size = 16u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            let a = (c + 0.5 - d).clamp(0.0, 1.0);
            let i = ((y * size + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[225, 29, 72, (a * 255.0) as u8]);
        }
    }
    tauri::image::Image::new_owned(rgba, size, size)
}

/// `app.setBadge { count }`: Dock badge (macOS), taskbar overlay dot (Windows), launcher count (Linux, where supported).
pub fn set_badge(args: &Value, ctx: &Ctx) -> Result<Value> {
    let count = crate::validation::as_integer(args.get("count").unwrap_or(&Value::Null)).unwrap_or(0).max(0);
    // Settings → General → "Show badge" off: always clear it.
    let count = if crate::models::setting::get_bool(&ctx.db, "app.show_badge") { count } else { 0 };
    let window = ctx.app()?.get_webview_window("main").ok_or_else(|| ApiError::msg("No window."))?;
    #[cfg(target_os = "windows")]
    {
        let _ = window.set_overlay_icon(if count > 0 { Some(dot_icon()) } else { None });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window.set_badge_count(if count > 0 { Some(count) } else { None });
    }
    Ok(json!({ "count": count }))
}

// ---- File drops ---------------------------------------------------------------------------------------

/// Forward drag-and-drop on the main window: `file-drag` `{ type: "enter"|"over"|"leave", position }` while
/// dragging and `file-drop` `{ files: [{ token, path, name, size, mime }], position: { x, y } }` on drop
/// (logical pixels). Attach a dropped file with `files.attach { token | path, paper_id, kind, ... }`.
pub fn on_drag_drop(window: &Window, event: &DragDropEvent) {
    let app = window.app_handle();
    let ctx = Ctx::new(app);
    let scale = window.scale_factor().unwrap_or(1.0);
    let pos = |p: &tauri::PhysicalPosition<f64>| json!({ "x": p.x / scale, "y": p.y / scale });
    match event {
        DragDropEvent::Enter { position, .. } => ctx.emit("file-drag", json!({ "type": "enter", "position": pos(position) })),
        DragDropEvent::Over { position } => ctx.emit("file-drag", json!({ "type": "over", "position": pos(position) })),
        DragDropEvent::Leave => ctx.emit("file-drag", json!({ "type": "leave", "position": null })),
        DragDropEvent::Drop { paths, position } => {
            let files = crate::files::register_paths(paths);
            ctx.emit("file-drop", json!({ "files": files, "position": pos(position) }));
        }
        _ => {}
    }
}
