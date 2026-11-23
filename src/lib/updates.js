import { reactive } from 'vue';
import { api } from '@/lib/api.js';
import { on } from '@/lib/events.js';

/**
 * App update state shared by the sidebar notice and the About page.
 * status: idle · checking · current (up to date) · available · downloading · installing · failed
 */
export const updates = reactive({
  status: 'idle',
  version: null,
  notes: '',
  current: null,
  downloaded: 0,
  total: null,
  error: null,
});

function setAvailable(info) {
  if (!info || !info.available) return;
  if (['downloading', 'installing'].includes(updates.status)) return;
  Object.assign(updates, { status: 'available', version: info.version, notes: info.notes || '', current: info.current, error: null });
}

let started = false;

/** Listen to the backend's update events (call once from the shell). */
export function watchUpdates() {
  if (started) return;
  started = true;
  on('update-available', setAvailable);
  on('update', (e) => {
    if (e.status === 'downloading') Object.assign(updates, { status: 'downloading', downloaded: e.downloaded, total: e.total ?? null });
    else if (e.status === 'installing') updates.status = 'installing';
    else if (e.status === 'failed') Object.assign(updates, { status: 'failed', error: e.error });
  });
}

/** Ask the release server now. */
export async function checkForUpdates() {
  if (['checking', 'downloading', 'installing'].includes(updates.status)) return;
  updates.status = 'checking';
  updates.error = null;
  try {
    const info = await api('app.checkUpdate');
    updates.current = info.current;
    if (info.available) setAvailable(info);
    else updates.status = 'current';
  } catch (e) {
    Object.assign(updates, { status: 'failed', error: e.message });
  }
}

/** Download, install and restart. */
export async function installUpdate() {
  updates.status = 'downloading';
  updates.downloaded = 0;
  updates.total = null;
  try {
    await api('app.installUpdate');
  } catch (e) {
    Object.assign(updates, { status: 'failed', error: e.message });
  }
}

/** Download progress, 0–100, or null when the size is unknown. */
export function updatePercent() {
  return updates.total ? Math.min(100, Math.round((updates.downloaded / updates.total) * 100)) : null;
}
