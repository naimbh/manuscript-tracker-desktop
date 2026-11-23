import { api } from './api.js';
import { on } from './events.js';
import { openUrl } from '@/components/shell/links.js';

/**
 * Native OS pieces, provided by the Rust backend.
 *
 * contextMenu(items) shows a native context menu at the pointer and resolves with the chosen item's id (or null).
 *   items: [{ id: 'open', label: 'Open' }, { separator: true }, { id: 'delete', label: 'Delete…', enabled: true, checked?: bool }]
 *   Usage on a row: @contextmenu.prevent="row(p, $event)" with
 *     async function row(p) { const id = await contextMenu([...]); if (id === 'open') ... }
 */
export async function contextMenu(items) {
  try {
    return (await api('app.contextMenu', { items })) ?? null;
  } catch {
    return null;
  }
}

/**
 * Native confirm dialog (replaces wire:confirm / window.confirm). Resolves true when the user confirms.
 *   if (await confirmDialog('Delete this paper? This cannot be undone.', { title: 'Delete paper', okLabel: 'Delete', danger: true })) ...
 * Backend: app.confirm { title?, message, ok?, danger? }. Falls back to window.confirm if the call fails.
 */
export async function confirmDialog(message, { title = null, okLabel = null, danger = false, kind = null } = {}) {
  try {
    return !!(await api('app.confirm', { message, title, ok: okLabel, danger: danger || kind === 'warning' || kind === 'error' }));
  } catch {
    return window.confirm(message);
  }
}

/**
 * Listen to native menu-bar / tray commands. The backend emits event 'menu' with payload { id }.
 * Ids: new_paper, find, settings, run_scans, backup_export, backup_import, toggle_sidebar, go_desk, go_papers,
 * go_publications, go_venues, go_opportunities, go_deadlines, help.
 *   onMenu('new_paper', () => modal('new-paper').show())   onMenu('*', (id) => ...) for every id.
 * Returns an unlisten function (call it in onBeforeUnmount for page-level listeners).
 */
export function onMenu(id, cb) {
  return on('menu', (payload) => {
    const got = typeof payload === 'string' ? payload : payload?.id;
    if (id === '*' || got === id) cb(got, payload);
  });
}

/**
 * Files dragged from Finder/Explorer onto the window (backend events 'file-drag' and 'file-drop').
 *   cb({ type: 'enter' | 'over' | 'leave', position })                       while dragging
 *   cb({ type: 'drop', files: [{ token, path, name, size, mime }], position }) on drop
 * position is { x, y } in CSS (logical) pixels or null; dropTarget(position) gives the element under it.
 * Attach a dropped file with api('files.attach', { path, paper_id, kind }). Returns an unlisten function.
 */
export function onFileDrop(cb) {
  const offDrag = on('file-drag', (p) => cb({ type: p?.type, position: p?.position ?? null, files: [] }));
  const offDrop = on('file-drop', (p) => cb({ type: 'drop', position: p?.position ?? null, files: p?.files ?? [] }));
  return () => { offDrag(); offDrop(); };
}

/** Element under a drag/drop position from onFileDrop, e.g. to find the paper row a file was dropped on. */
export function dropTarget(position) {
  return position ? document.elementFromPoint(position.x, position.y) : null;
}

/**
 * Browser behaviours a native app should not have: the page context menu (kept in text fields),
 * zoom and reload shortcuts, dragging links/images out of the window. Called once from main.js.
 */
export function installNativeBehaviour() {
  window.addEventListener('contextmenu', (e) => {
    const t = e.target;
    const editable = t?.closest?.('input, textarea, [contenteditable=""], [contenteditable="true"]');
    if (!editable && !e.defaultPrevented) e.preventDefault();
  });
  window.addEventListener('keydown', (e) => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && ['+', '=', '-', '_', '0'].includes(e.key)) e.preventDefault(); // browser zoom
    if ((mod && e.key.toLowerCase() === 'r') || e.key === 'F5') e.preventDefault(); // reload (the menu has its own)
    if (mod && e.key.toLowerCase() === 'p') e.preventDefault(); // print
  }, true);
  window.addEventListener('dragstart', (e) => {
    const t = e.target;
    if (t?.tagName === 'A' || t?.tagName === 'IMG') e.preventDefault();
  });
  // Ctrl+wheel zoom (trackpad pinch on Windows).
  window.addEventListener('wheel', (e) => { if (e.ctrlKey) e.preventDefault(); }, { passive: false });
  // Web and email links anywhere open in the default browser/mail app; the app window never navigates away.
  const external = (e) => {
    if (e.defaultPrevented) return;
    const a = e.target?.closest?.('a[href]');
    const href = (a?.getAttribute('href') || '').trim();
    if (!/^(https?:|mailto:)/i.test(href)) return;
    e.preventDefault();
    if (e.button === 0 || e.button === 1) openUrl(href);
  };
  window.addEventListener('click', external);
  window.addEventListener('auxclick', external);
}
