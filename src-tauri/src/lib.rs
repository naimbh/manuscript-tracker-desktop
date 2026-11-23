pub mod api;
pub mod db;
pub mod error;
pub mod files;
pub mod mcp;
pub mod models;
pub mod native;
pub mod research;
pub mod scheduler;
pub mod updater;
pub mod util;
pub mod validation;

use api::app::{apply_theme, show_main, LaunchState};
use api::Ctx;
use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, WindowEvent};

/// Argument added when the app is started at login: stay in the tray.
pub const HIDDEN_ARG: &str = "--hidden";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();
    // A separate data folder (`MT_DATA_DIR`, debug builds only) runs beside the installed app: no single-instance
    // lock and no window state written to the real app folder.
    if mcp::data_dir_override().is_none() {
        builder = builder
            // Must be first: a second launch focuses the running app instead.
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
            .plugin(
                tauri_plugin_window_state::Builder::default()
                    .with_state_flags(tauri_plugin_window_state::StateFlags::all() & !tauri_plugin_window_state::StateFlags::VISIBLE)
                    .build(),
            );
    }
    let app = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![HIDDEN_ARG])))
        .invoke_handler(tauri::generate_handler![api::api])
        .setup(|app| {
            setup(app)?;
            Ok(())
        })
        .on_menu_event(|app, event| native::on_menu_event(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if let WindowEvent::DragDrop(drop) = event {
                if window.label() == "main" {
                    native::on_drag_drop(window, drop);
                }
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let ctx = Ctx::new(window.app_handle());
                    if models::setting::get_bool(&ctx.db, "app.run_in_background") {
                        // macOS: keep running (Dock icon reopens). Windows/Linux: keep running in the tray.
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building the app");

    app.run(|_app, _event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = _event {
            show_main(_app);
        }
    });
}

fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let dir = match mcp::data_dir_override() {
        Some(d) => d,
        None => app.path().app_data_dir()?,
    };
    std::fs::create_dir_all(dir.join("files"))?;
    let db = db::Db::open(&dir.join("tracker.sqlite")).map_err(|e| e.to_string())?;
    app.manage(db);

    let hidden = std::env::args().any(|a| a == HIDDEN_ARG);
    let launch = LaunchState::default();
    launch.hidden.store(hidden, Ordering::SeqCst);
    app.manage(launch);

    native::create_main_window(&handle)?;
    app.set_menu(native::build_menu(&handle)?)?;
    native::restore_always_on_top(&handle);

    let ctx = Ctx::new(&handle);
    // Jobs from a previous session cannot still be running.
    let _ = ctx.db.exec(
        "UPDATE ai_runs SET status = 'failed', error = 'Stopped responding.', finished_at = ?1, updated_at = ?1 WHERE status = 'running'",
        [util::now_ts()],
    );
    let _ = ctx.db.exec(
        "UPDATE scans SET status = 'failed', error = 'Stopped responding.', finished_at = ?1, updated_at = ?1 WHERE status = 'running'",
        [util::now_ts()],
    );

    apply_theme(&handle, &ctx.setting("app.theme").unwrap_or_else(|| "system".into()));

    api::opportunities::register_jobs();
    api::venues::register_jobs();
    api::publications::register_jobs();
    research::register_jobs();
    scheduler::start(ctx.clone());
    updater::start(ctx.clone());
    api::backup::start_auto(ctx.clone());
    api::calendar::start_if_enabled(&ctx.db);

    build_tray(&handle)?;
    watch_external_writes(ctx.clone());
    #[cfg(debug_assertions)]
    qa_hook(&handle);

    // Fallback: show the window even if the page never calls app.ready.
    let h = handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        let state = h.state::<LaunchState>();
        if !state.shown.load(Ordering::SeqCst) && !state.hidden.load(Ordering::SeqCst) {
            let h2 = h.clone();
            let _ = h.run_on_main_thread(move || show_main(&h2));
        }
    });
    Ok(())
}

/// Another process (the `--mcp` server an assistant started) writes the same database: every 2 s, check
/// `PRAGMA data_version` and tell open pages to reload (`changed` `{ area: "external" }`).
fn watch_external_writes(ctx: Ctx) {
    std::thread::spawn(move || {
        let mut last = ctx.db.data_version().ok();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let Ok(v) = ctx.db.data_version() else {
                continue;
            };
            if last.is_some_and(|l| l != v) {
                ctx.changed("external");
            }
            last = Some(v);
        }
    });
}

/// Debug builds only: when `MT_QA_EVAL` names a file, its contents (JavaScript) are run in the main window
/// and the file is removed (`menu:<id>` runs a menu command instead). Visual QA drives the app with it.
#[cfg(debug_assertions)]
fn qa_hook(app: &AppHandle) {
    let Some(path) = std::env::var_os("MT_QA_EVAL").filter(|v| !v.is_empty()).map(std::path::PathBuf::from) else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let Ok(js) = std::fs::read_to_string(&path) else {
            continue;
        };
        let _ = std::fs::remove_file(&path);
        // `menu:<id>` runs a menu bar command as if it was chosen (e.g. `menu:find`).
        if let Some(id) = js.trim().strip_prefix("menu:") {
            let (h, id) = (app.clone(), id.to_string());
            let _ = app.run_on_main_thread(move || native::on_menu_event(&h, &id));
            continue;
        }
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.eval(&js);
        }
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "tray_open", "Open Manuscript Tracker", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "tray_scan", "Run scans now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "Quit", true, None::<&str>)?;
    let on_top = native::on_top_item(app, native::ON_TOP_TRAY_ID, false)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &scan, &sep, &on_top, &sep2, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Manuscript Tracker")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_open" => show_main(app),
            "tray_scan" => {
                let ctx = Ctx::new(app);
                let started = scheduler::run_now(&ctx, None);
                if started.is_empty() {
                    let _ = ctx.notify(
                        "ai_job_finished",
                        serde_json::json!({ "title": "Nothing to run", "body": "Scans are already running or none are set up.", "status": "warning", "url": null }),
                    );
                }
            }
            native::ON_TOP_TRAY_ID => native::toggle_always_on_top(app),
            "tray_quit" => app.exit(0),
            _ => {}
        });
    // macOS: a one-colour template glyph that the menu bar tints for light and dark mode.
    // Windows/Linux: the small colour app icon, as tray icons there usually are.
    #[cfg(target_os = "macos")]
    {
        tray = tray.icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    // Windows: left click opens the window, right click shows the menu. macOS/Linux: the menu on click.
    #[cfg(target_os = "windows")]
    {
        tray = tray.show_menu_on_left_click(false).on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    }
    tray.build(app)?;
    Ok(())
}
