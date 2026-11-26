# Development

This guide covers building Manuscript Tracker from source, running it in development, testing it and releasing it. For how the app is put together, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Requirements

- [Node.js](https://nodejs.org/) 20 or later
- [Rust](https://rustup.rs/) (stable), with `rustfmt` and `clippy`
- The [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system

On Debian or Ubuntu, the system libraries are:

```sh
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf libdbus-1-dev libssl-dev pkg-config
```

## Build from source

```sh
git clone https://github.com/naimbh/manuscript-tracker-desktop.git
cd manuscript-tracker-desktop
npm ci
npm run build      # installers for this system, in src-tauri/target/release/bundle
```

Release builds also make signed update bundles, so they need the updater signing key in the environment (see “Releasing an update” below). To build installers without it, turn the update bundles off for that build:

```sh
npx tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

For a universal macOS build (Apple silicon and Intel), add both targets first:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run build:mac
```

## Development commands

| Command | What it does |
| --- | --- |
| `npm run dev` | Runs the app with hot reload (Vite on port 1420 plus the Rust core). |
| `npm run dev:web` | Runs only the Vite dev server for the interface. |
| `npm run build:web` | Builds the interface into `dist/`. |
| `npm run icons` | Rebuilds `src/flux/icons.js` with only the icons the app uses. |
| `npm run build` | Builds release installers for this system. |

Inside `src-tauri`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Debug hooks

These exist only in debug builds and are ignored by release builds.

| Variable | Effect |
| --- | --- |
| `MT_DATA_DIR=/some/folder` | Uses that folder for the database and files instead of the normal data folder, with its own web storage, no single-instance lock and no saved window state. The `--mcp` server honours it too. |
| `MT_E2E=1` | With `MT_DATA_DIR`: backups are exported to `<MT_DATA_DIR>/exports` without a save dialog, and the WebDriver chooses the web view’s profile folder. Used by the end-to-end tests. |
| `MT_E2E_CONFIRM=1` | With `MT_DATA_DIR` and `MT_E2E`: every confirmation dialog answers OK by itself, so flows behind one (deleting a paper, a file or a submission) can be tested. |
| `MT_QA_EVAL=/path/file.js` | Whenever the file appears, the app runs its JavaScript in the window and deletes the file (`menu:<id>` triggers a menu command instead). Useful for scripted visual checks. With `MT_DATA_DIR`, the script can call the `qa.write` action (`{ text }`, appended to `<MT_DATA_DIR>/qa-out.txt`) to report what it found, and `qa.rect` to get the window's frame for a screenshot. |

With `MT_DATA_DIR` set, `backup.import` also accepts `{ path }`, so a demo backup can be loaded without the file dialog.

## End-to-end tests

`e2e/run.mjs` drives a debug build through [tauri-driver](https://v2.tauri.app/develop/tests/webdriver/) and saves a screenshot of every step in `e2e/screenshots/`. It goes through onboarding, the Desk, adding and opening a paper, recording a submission, every main page and settings screen, a backup export and dark mode, all with fictional data in a temporary data folder.

```sh
npx tauri build --debug --no-bundle
cargo install tauri-driver --locked
cd e2e && npm ci && node run.mjs
```

It runs on Linux (with `webkit2gtk-driver`, under `xvfb-run` on a headless machine) and on Windows (with an `msedgedriver.exe` that matches the installed WebView2 version, passed as `NATIVE_DRIVER`). macOS has no WebDriver for WKWebView.

## Continuous integration

- **`.github/workflows/ci.yml`** runs on every push and pull request. On macOS, Windows and Ubuntu 22.04 it checks formatting, runs Clippy with warnings as errors, runs the unit tests, builds the installers and uploads them as artifacts. A second job runs the end-to-end tour on Ubuntu and Windows and uploads the screenshots.
- **`.github/workflows/release.yml`** runs when a tag such as `v1.0.0` is pushed. It builds a universal macOS `.dmg`, Windows `-setup.exe` and `.msi` installers, and Linux `.AppImage`, `.deb` and `.rpm` packages, signs the update bundles, and attaches everything, including `latest.json`, to a draft GitHub release.

## Automatic updates

The app uses the Tauri updater. It checks `https://github.com/naimbh/manuscript-tracker-desktop/releases/latest/download/latest.json` shortly after launch and once a day (unless “Check for updates automatically” is off in Settings → General), shows a notice in the sidebar and a notification once per version, and installs the update when the user clicks “Restart to update”. Settings → About and the “Check for Updates…” menu item check on demand.

Every download is verified against the public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`). The matching private key signs the update bundles in the release workflow, which reads it from two repository secrets: `TAURI_SIGNING_PRIVATE_KEY` (the key file’s contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Keep a backup of the private key: without it, installed copies cannot verify, and therefore cannot install, any future update.

To sign a local release build, export both variables first:

```sh
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/manuscript-tracker.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="…"
```

## Releasing an update

1. Bump the version in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` (then run `cargo check` so `Cargo.lock` follows).
2. Commit, then tag and push the tag: `git tag v1.0.1 && git push origin v1.0.1`.
3. The release workflow builds, signs and drafts the release with all installers and `latest.json`.
4. Review the draft on GitHub and publish it. Publishing makes `latest.json` live, and every installed copy then offers the update and installs it on restart.

## Project structure

```
src/                  Vue 3 interface
  flux/               Vue versions of the Flux UI components
  components/         Shared pieces and the app shell
  pages/              One file per screen
  lib/                API bridge, router, toasts, modals, shortcuts, formatting
src-tauri/            Rust core
  src/api/            One module per area, reached through the single `api` command
  src/models/         Domain rules
  src/research/       Research engine
  src/mcp/            Local MCP server
  icons/, installer/  App icons and installer artwork
e2e/                  End-to-end tour
docs/                 Architecture, development guide and screenshots
```

## Contributing

Issues and pull requests are welcome. Please run the formatting, Clippy and unit test checks before opening a pull request, and keep test data fictional. The code is free and open source under the [GNU AGPL v3.0](../LICENSE) (AGPL-3.0-or-later); contributions are accepted under the same licence.
