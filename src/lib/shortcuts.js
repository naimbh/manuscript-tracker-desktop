import { onBeforeUnmount } from 'vue';
import { isMac } from './platform.js';

/**
 * Global keyboard shortcuts. One keydown listener on window; the most recently registered handler for a combo wins.
 *   const off = shortcut('mod+k', () => openPalette());       // Cmd+K on macOS, Ctrl+K elsewhere
 *   shortcut('n', fn)  shortcut('/', fn)  shortcut('mod+,', fn)  shortcut('mod+1', fn)  shortcut('mod+\\', fn)  shortcut('escape', fn)
 * Plain keys (no mod/alt) are ignored while typing in inputs/textareas/selects/contenteditable unless { inInputs: true }.
 * A handler may return false to let the event continue to the next (older) handler.
 */
const registry = new Map(); // combo -> array of { handler, opts }

function normalise(combo) {
  return combo.toLowerCase().split('+').map((p) => p.trim()).map((p) => (p === 'cmd' || p === 'meta' || p === 'ctrl' && !isMac ? 'mod' : p))
    .sort((a, b) => order(a) - order(b)).join('+');
}
function order(p) { return { mod: 0, ctrl: 1, alt: 2, shift: 3 }[p] ?? 9; }

function keyName(e) {
  const k = e.key;
  if (k === 'Escape' || k === 'Esc') return 'escape';
  if (k === ' ') return 'space';
  if (k === '\\' || e.code === 'Backslash') return '\\';
  if (/^Digit\d$/.test(e.code) && (e.metaKey || e.ctrlKey)) return e.code.slice(5);
  return k.length === 1 ? k.toLowerCase() : k.toLowerCase();
}

export function comboOf(e) {
  const parts = [];
  const mod = isMac ? e.metaKey : e.ctrlKey;
  if (mod) parts.push('mod');
  if (isMac && e.ctrlKey) parts.push('ctrl');
  if (e.altKey) parts.push('alt');
  const key = keyName(e);
  // Shift is implied for symbols such as '?', so only record it for letters/digits/named keys.
  if (e.shiftKey && (/^[a-z0-9]$/.test(key) || key.length > 1)) parts.push('shift');
  parts.push(key);
  return normalise(parts.join('+'));
}

export function isTyping(e) {
  const t = e.target;
  if (!t || !t.tagName) return false;
  return ['INPUT', 'TEXTAREA', 'SELECT'].includes(t.tagName) || t.isContentEditable;
}

function onKeydown(e) {
  if (e.defaultPrevented || e.isComposing) return;
  const combo = comboOf(e);
  const list = registry.get(combo);
  if (!list?.length) return;
  const plain = !combo.includes('mod') && !combo.includes('alt') && !combo.includes('ctrl') && combo !== 'escape';
  for (let i = list.length - 1; i >= 0; i--) {
    const { handler, opts } = list[i];
    if (plain && isTyping(e) && !opts.inInputs) continue;
    if (opts.when && !opts.when(e)) continue;
    const r = handler(e);
    if (r === false) continue;
    if (opts.preventDefault !== false) e.preventDefault();
    return;
  }
}

if (typeof window !== 'undefined') window.addEventListener('keydown', onKeydown);

/** Register a shortcut; returns an unregister function. opts: { inInputs, when(e), preventDefault } */
export function shortcut(combo, handler, opts = {}) {
  const key = normalise(combo);
  const entry = { handler, opts };
  if (!registry.has(key)) registry.set(key, []);
  registry.get(key).push(entry);
  return () => {
    const list = registry.get(key);
    const i = list ? list.indexOf(entry) : -1;
    if (i >= 0) list.splice(i, 1);
  };
}

/** Same as shortcut() but unregisters automatically when the calling component unmounts. */
export function useShortcut(combo, handler, opts) {
  const off = shortcut(combo, handler, opts);
  onBeforeUnmount(off);
  return off;
}
