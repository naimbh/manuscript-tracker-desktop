// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--mcp`: run the local MCP server over stdin/stdout instead of the app (no window).
    if std::env::args().skip(1).any(|a| a == manuscript_tracker_lib::mcp::FLAG) {
        std::process::exit(manuscript_tracker_lib::mcp::main());
    }
    manuscript_tracker_lib::run();
}
