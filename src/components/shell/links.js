import { api } from '@/lib/api.js';

/** A stored UTC timestamp ('YYYY-MM-DD HH:MM:SS') as an ISO string the date helpers read as UTC. */
export function utc(ts) {
  return ts ? String(ts).trim().replace(' ', 'T') + 'Z' : null;
}

/**
 * Follow a link stored in the data (notifications, AI runs): '#/venues', '/papers/12' or a web app URL
 * such as 'https://manuscripttracker.test/venues' (from an imported backup) open inside the app;
 * other http(s)/mailto links open in the default browser.
 */
export function openUrl(url) {
  if (!url || url === '#') return;
  const s = String(url).trim();
  if (s.startsWith('#')) { location.hash = s.slice(1) || '/'; return; }
  if (s.startsWith('/')) { location.hash = s; return; }
  try {
    const u = new URL(s);
    if (u.hash.startsWith('#/')) { location.hash = u.hash.slice(1); return; }
    const local = /^(localhost|127\.0\.0\.1)$/.test(u.hostname) || /\.test$/.test(u.hostname) || /manuscripttracker/i.test(u.hostname);
    if (local && /^https?:$/.test(u.protocol)) { location.hash = (u.pathname || '/') + u.search; return; }
  } catch {
    return;
  }
  api('app.openExternal', { url: s }).catch(() => {});
}
