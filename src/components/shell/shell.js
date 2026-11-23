import { reactive } from 'vue';
import { api } from '@/lib/api.js';
import { applyPlatform } from '@/lib/platform.js';
import { setAppearance } from '@/lib/appearance.js';
import { user } from '@/lib/site.js';
import { toast } from '@/lib/toast.js';
import { bus } from '@/lib/bus.js';
import { refreshRoute } from '@/lib/router.js';

/** App shell state filled from `app.boot`. */
export const shell = reactive({
  booted: false,
  hasUser: false,
  newOpportunities: 0,
  unread: 0,
  platform: null,
  version: null,
});

/** Copy a user row into the shared `user` (UiAvatar, SettingsLayout, the sidebar). */
export function setUser(u) {
  shell.hasUser = !!u;
  if (u) Object.assign(user, { id: u.id, name: u.name ?? '', email: u.email ?? '', is_admin: u.is_admin !== false });
  loadAvatar(u?.avatar_path || null);
}

let avatarPath = null;

/** Show a photo the page already has (after saving or removing it), without asking the backend again. */
export function setAvatar(path, url) {
  avatarPath = path || null;
  user.avatar = (avatarPath && url) || null;
}

/** Load the profile photo (a `data:` URL) when the stored path changed. */
function loadAvatar(path) {
  if (path === avatarPath) return;
  avatarPath = path;
  if (!path) {
    user.avatar = null;
    return;
  }
  api('settings.avatar')
    .then((r) => { if (avatarPath === path) user.avatar = r?.url || null; })
    .catch(() => { if (avatarPath === path) user.avatar = null; });
}

/** Run `app.boot` and apply everything it returns. `first` also applies the theme and window material. */
export async function boot({ first = false } = {}) {
  const b = await api('app.boot');
  if (first) {
    applyPlatform(b);
    shell.platform = b.platform;
    shell.version = b.version;
    await setAppearance(b.theme || 'system', { persist: false });
  }
  setUser(b.user);
  shell.newOpportunities = b.new_opportunities_this_week || 0;
  shell.unread = b.unread_notifications || 0;
  shell.booted = true;
  return b;
}

let badgeTimer = null;
/** Update the Dock/taskbar badge with the Desk "Needs attention" count (debounced). Pass a count to skip the query. */
export function updateBadge(count) {
  clearTimeout(badgeTimer);
  badgeTimer = setTimeout(async () => {
    if (!shell.hasUser) return;
    try {
      const n = typeof count === 'number' ? count : await api('desk.attentionCount');
      const review = await api('proposals.count').catch(() => 0);
      await api('app.setBadge', { count: (n || 0) + (review || 0) });
    } catch {}
  }, typeof count === 'number' ? 0 : 300);
}

/** Save a full backup (.zip with the paper files) with the native save dialog (`backup.export`). */
export async function exportBackup() {
  try {
    const r = await api('backup.export', { include_files: true });
    if (r) toast({ variant: 'success', text: 'Backup saved.' });
  } catch (e) {
    toast({ variant: 'danger', heading: 'Could not save the backup', text: e.message });
  }
}

/**
 * Restore a backup (`backup.import` shows the native open dialog). Resolves with the result, or null when
 * cancelled or failed (the error is shown as a toast). Reloads the user, the open page and the badge.
 */
export async function importBackup() {
  try {
    const r = await api('backup.import');
    if (!r) return null;
    await boot();
    refreshRoute();
    bus.emit('papers-changed');
    updateBadge();
    toast({ variant: 'success', text: r.summary || 'Backup restored.' });
    return r;
  } catch (e) {
    toast({ variant: 'danger', heading: 'Could not import the backup', text: e.message });
    return null;
  }
}
