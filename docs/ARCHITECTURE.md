# Architecture

Manuscript Tracker is a Tauri 2 desktop app. A Rust core owns the data, the network and everything the operating system offers; a Vue 3 interface draws the screens inside the system’s own web view (WebKit on macOS and Linux, WebView2 on Windows). This document explains how the pieces fit together.

## Overview

```
src/            Vue 3 interface (Vite, Tailwind CSS 4, plain JavaScript)
  flux/         Vue versions of the Flux UI components the web app uses
  components/   Shared pieces (logo, avatars, rings, empty states) and the app shell
  pages/        One file per screen: Desk, Papers, Published, Journals & Conferences (venues), Calls for Papers (opportunities), Deadlines, Settings
  lib/          The API bridge, a small hash router, toasts, modals, shortcuts and formatting helpers
src-tauri/      Rust core
  src/api/      One module per area, all reached through a single command
  src/models/   Domain rules: stages, decisions, review clocks, venues, settings
  src/research/ The research engine (fetching, extraction, providers, review)
  src/mcp/      A local MCP server for AI assistants
e2e/            End-to-end tour run in CI through tauri-driver
```

## One command: `api`

The interface never touches the file system, the network or the operating system directly. It calls a single Tauri command, `api(name, args)`, where `name` has the form `area.action` (for example `papers.saveSubmission` or `venues.research`). The dispatcher in `src-tauri/src/api/mod.rs` routes the call to the module for that area (`desk`, `papers`, `venues`, `publications`, `deadlines`, `opportunities`, `notifications`, `ai`, `research`, `settings`, `backup`, `calendar`, `agents`, `app`). Unknown names are rejected.

Handlers are synchronous and run on a worker thread, never on the UI thread. Inputs are checked by a small validator modelled on Laravel’s (`required|string|max:255`, `nullable|date` and so on), so the interface receives the same field-level error messages as the web app. Errors come back in two shapes: `{ kind: "validation", errors }` for form fields and `{ kind: "error", message }` for everything else.

The backend pushes changes to the interface as events: `changed` (reload pages that show an area), `ai-run` (live progress of research jobs), `notifications`, `menu` (commands from the native menu bar), `scheduler`, and file drag-and-drop events.

## Data

All data lives in one SQLite database (`tracker.sqlite`, WAL mode) in the app’s data folder:

- macOS: `~/Library/Application Support/com.naimbh.manuscripttracker/`
- Windows: `%APPDATA%\com.naimbh.manuscripttracker\`
- Linux: `~/.local/share/com.naimbh.manuscripttracker/`

The schema follows the web app’s migrations (users, papers, submissions, revision items, queue, authors, paper files, deadlines, venues, publications, opportunities, scans, AI runs, notifications, settings), so backups move freely between the two apps. There is a single local user; there is no sign-in. Attached files are stored under `files/` in the same folder, and daily database copies under `backups/`.

The domain rules (paper stages, decision labels, review clocks against a venue’s usual decision time, anonymity risks, deadline maths) live in `src-tauri/src/models/` as plain functions over database rows, with unit tests.

## Scheduler

Three background jobs keep the library current: the opportunity scan, venue updates and citation updates. Each can be switched on or off and set to run daily or weekly at a chosen time (Settings → AI). The scheduler checks every few minutes and catches up after the computer was asleep or the app was closed, without ever running the same job twice at once.

## Research engine

The research engine (`src-tauri/src/research/`) follows one rule: fetch first, use AI second.

1. **Fetching.** The app downloads the sources itself: WikiCFP feeds and call pages, venue home pages and author guidelines, the free OpenAlex, Crossref and Semantic Scholar APIs, and the public Google Scholar profile page. Requests run a few at a time, respect `robots.txt` and per-host rate limits, retry politely and are cached on disk for a day. Pages are reduced to readable text (headings, dates and links kept; navigation and scripts removed).
2. **Extraction.** The AI provider only reads the fetched text and returns structured JSON. Every value carries its source URL and a short supporting quote. A value counts as verified only when its quote is found on the source page. This works with any provider, including local models, and the provider’s own web search is used only as a fallback.
3. **Review before apply.** Findings become proposals. Verified values for empty fields can be filled in automatically; everything else waits on a review screen that shows the current value, the value found, the source and the quote. Unverified values are never applied on their own.
4. **Progress.** Every job is recorded as an AI run with live steps, partial results, token counts and a cancel button.

Citation counts and Scholar statistics come straight from the scholarly indexes and the Scholar profile page, so they need no AI at all. Supported providers are the Claude Code CLI (detected automatically), the Anthropic API, OpenAI, Gemini and any OpenAI-compatible endpoint (for example Ollama or LM Studio).

## Interface

The interface is a port of the web app’s Blade and Livewire views to Vue single-file components, keeping the same markup, Tailwind classes and copy. The `src/flux/` folder contains Vue versions of the Flux components (buttons, inputs, selects, modals, menus, tabs, tooltips, toasts and more) that render the same DOM and `data-flux-*` attributes as the originals, so the web app’s CSS applies unchanged. Pages call `api(...)` and reload their data after each change, much as a Livewire component re-renders.

Desktop additions sit on top of that design: a master–detail Papers screen, a command palette (Ctrl/⌘+K), keyboard shortcuts for every main section, right-click menus on list rows, a collapsible sidebar and reduced motion when the system asks for it.

## Native shell

`src-tauri/src/native.rs` and `lib.rs` connect the app to the operating system:

- A native menu bar with every main command and its shortcut, plus native context menus.
- Window materials: sidebar vibrancy on macOS and Mica on Windows 11; a solid background on Linux.
- Native open, save and confirm dialogs; native notifications; a Dock or taskbar badge; a tray icon with quick actions.
- Files dragged from Finder or Explorer onto a paper are attached to it.
- Window size and position are remembered; only one instance runs at a time; the app can start at login.

## Security model

- **Narrow bridge.** The page may only call the `api` command, listen to events and drag the window. It has no file system, shell, HTTP, dialog or opener access of its own.
- **Strict content security policy.** Scripts load only from the app itself; the page makes no network requests. All network traffic happens in Rust.
- **Secrets in the keychain.** API keys and tokens are stored in the operating system’s credential store (macOS Keychain, Windows Credential Manager, Linux Secret Service), are never returned to the interface and are never logged or exported.
- **Checked inputs.** Every action validates its arguments. External links open in the default browser and only `http`, `https` and `mailto` addresses are allowed.
- **No personal data in research requests.** The research engine sends venue names, titles and DOIs, never the user’s profile.
- **Release hardening.** Developer tools and the web view’s default context menu are disabled in release builds. Test hooks (a separate data folder, file dialogs skipped) exist only in debug builds.

## MCP server for AI assistants

Running the app’s executable with `--mcp` starts a local MCP server over standard input and output, with the same thirteen tools as the web app (list and update papers, record submissions and decisions, add deadlines, search venues and more). It uses the same database as the app, runs without a window, and needs no token because nothing listens on the network. Settings → Agents shows the exact configuration to paste into an assistant.

## Backups

Backups use the web app’s format (version 2): a `.zip` file with `backup.json` and the attached files, or a bare `.json` file. Import adds records and never deletes; venues are matched by name. A backup made in either app can be imported into the other.

## Tests

- `cargo test` covers the models, the validator, the scheduler, backups, research parsing and the MCP server.
- `e2e/run.mjs` drives a debug build through tauri-driver on Linux and Windows: onboarding, the Desk, adding a paper and a submission, every main page and settings screen, a backup export and dark mode. It saves a screenshot of each step.
