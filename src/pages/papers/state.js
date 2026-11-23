import { reactive } from 'vue';
import { api } from '@/lib/api.js';
import { bus } from '@/lib/bus.js';

/**
 * State that outlives the papers page (the route component remounts when the selected paper changes):
 * the last list, cached details, the list's scroll position and actions waiting for a paper to open.
 */
export const cache = {
  list: null,
  details: new Map(),
  scroll: 0,
  animated: false,
  focusSearch: false,
  wide: typeof window !== 'undefined' ? window.innerWidth >= 1180 : true,
};

/** An action for the detail view to run once the paper is open: { id, action: 'new-submission' | 'attach', files? }. */
export const pending = reactive({ id: null, action: null, files: [] });

export function setPending(id, action, files = []) {
  pending.id = id;
  pending.action = action;
  pending.files = files;
}

export function takePending(id) {
  if (pending.id !== id || !pending.action) return null;
  const out = { action: pending.action, files: pending.files };
  pending.id = null;
  pending.action = null;
  pending.files = [];
  return out;
}

/** Tell the list, the Desk and the sidebar that papers changed. */
export function papersChanged(source = 'papers', id = null) {
  bus.emit('papers-changed', { source, id });
}

/** Open an external link in the default browser (the window never navigates away). */
export function openExternal(url) {
  if (url) api('app.openExternal', { url }).catch(() => {});
}

/** Copy text to the clipboard (falls back to execCommand where the async API is unavailable). */
export async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const t = document.createElement('textarea');
    t.value = text;
    t.style.position = 'fixed';
    t.style.opacity = '0';
    document.body.appendChild(t);
    t.select();
    const ok = document.execCommand('copy');
    t.remove();
    return ok;
  }
}

/** UTC timestamp from the database ('YYYY-MM-DD HH:MM:SS') as a Date. */
export function utc(ts) {
  return ts ? new Date(String(ts).replace(' ', 'T') + 'Z') : null;
}

/* Tone → classes, as in the Blade views. */
export const TINT = {
  blue: 'before:bg-sky-400/70 bg-sky-50/30 hover:bg-sky-50/60 dark:bg-sky-500/[.03] dark:hover:bg-sky-500/[.06]',
  amber: 'before:bg-amber-400/70 bg-amber-50/30 hover:bg-amber-50/60 dark:bg-amber-500/[.03] dark:hover:bg-amber-500/[.06]',
  red: 'before:bg-rose-400/70 bg-rose-50/25 hover:bg-rose-50/60 dark:bg-rose-500/[.03] dark:hover:bg-rose-500/[.06]',
  green: 'before:bg-emerald-400/70 bg-emerald-50/30 hover:bg-emerald-50/60 dark:bg-emerald-500/[.03] dark:hover:bg-emerald-500/[.06]',
  violet: 'before:bg-violet-400/70 bg-violet-50/30 hover:bg-violet-50/60 dark:bg-violet-500/[.03] dark:hover:bg-violet-500/[.06]',
  zinc: 'before:bg-zinc-300 bg-white/80 hover:bg-white dark:bg-zinc-900 dark:hover:bg-zinc-800/60',
  indigo: 'before:bg-indigo-400/70 bg-indigo-50/30 hover:bg-indigo-50/60 dark:bg-indigo-500/[.03] dark:hover:bg-indigo-500/[.06]',
  fuchsia: 'before:bg-fuchsia-400/70 bg-fuchsia-50/30 hover:bg-fuchsia-50/60 dark:bg-fuchsia-500/[.03] dark:hover:bg-fuchsia-500/[.06]',
  teal: 'before:bg-teal-400/70 bg-teal-50/30 hover:bg-teal-50/60 dark:bg-teal-500/[.03] dark:hover:bg-teal-500/[.06]',
  orange: 'before:bg-orange-400/70 bg-orange-50/30 hover:bg-orange-50/60 dark:bg-orange-500/[.03] dark:hover:bg-orange-500/[.06]',
};

export const DOT = {
  blue: 'bg-sky-500', amber: 'bg-amber-500', red: 'bg-rose-500', green: 'bg-emerald-500', zinc: 'bg-zinc-400',
  indigo: 'bg-indigo-500', fuchsia: 'bg-fuchsia-500', teal: 'bg-teal-500', orange: 'bg-orange-500',
};

export const RING_TONE = {
  blue: 'bg-sky-500 ring-sky-100 dark:ring-sky-500/20', amber: 'bg-amber-500 ring-amber-100 dark:ring-amber-500/20',
  red: 'bg-rose-500 ring-rose-100 dark:ring-rose-500/20', green: 'bg-emerald-500 ring-emerald-100 dark:ring-emerald-500/20',
  zinc: 'bg-zinc-400 ring-zinc-100 dark:ring-zinc-700', indigo: 'bg-indigo-500 ring-indigo-100 dark:ring-indigo-500/20',
  fuchsia: 'bg-fuchsia-500 ring-fuchsia-100 dark:ring-fuchsia-500/20', teal: 'bg-teal-500 ring-teal-100 dark:ring-teal-500/20',
  orange: 'bg-orange-500 ring-orange-100 dark:ring-orange-500/20',
};

export const ICON_OF = {
  blue: 'clock', amber: 'pencil', red: 'x-mark', green: 'check', zinc: 'minus', indigo: 'paper-airplane',
  fuchsia: 'user', teal: 'scale', orange: 'arrow-uturn-left',
};

export const PILL = {
  blue: 'bg-sky-50/60 text-sky-700 dark:bg-sky-500/[.06] dark:text-sky-300', amber: 'bg-amber-50/60 text-amber-700 dark:bg-amber-500/[.06] dark:text-amber-300',
  red: 'bg-rose-50/60 text-rose-600 dark:bg-rose-500/[.06] dark:text-rose-300', green: 'bg-emerald-50/60 text-emerald-700 dark:bg-emerald-500/[.06] dark:text-emerald-300',
  zinc: 'bg-zinc-100 text-zinc-500 dark:bg-zinc-800', indigo: 'bg-indigo-50/60 text-indigo-700 dark:bg-indigo-500/[.06] dark:text-indigo-300',
  fuchsia: 'bg-fuchsia-50/60 text-fuchsia-700 dark:bg-fuchsia-500/[.06] dark:text-fuchsia-300', teal: 'bg-teal-50/60 text-teal-700 dark:bg-teal-500/[.06] dark:text-teal-300',
  orange: 'bg-orange-50/60 text-orange-700 dark:bg-orange-500/[.06] dark:text-orange-300',
};

export const FILE_GRADIENT = {
  proof: 'from-rose-500 to-orange-400', manuscript: 'from-indigo-500 to-violet-500', anonymized: 'from-zinc-500 to-zinc-700',
  cover: 'from-sky-500 to-cyan-400', response: 'from-amber-500 to-yellow-400', supplementary: 'from-emerald-500 to-teal-400',
};
