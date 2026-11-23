import { markRaw, reactive } from 'vue';

/**
 * Command palette registry (rendered by <FluxCommandPalette>, opened with Cmd/Ctrl+K).
 * A command: { id, title, subtitle?, group?, icon?, keywords?, kbd?, run() }.
 *   registerCommands('pages', [ { id: 'go-desk', title: 'Desk', group: 'Pages', icon: 'home', run: () => navigate('dashboard') } ])
 *   registerSearch('papers', async (q) => (await api('papers.search', { q })).map(p => ({ id: 'paper-'+p.id, title: p.title, group: 'Papers', run: ... })))
 * Both return an unregister function.
 */
export const palette = reactive({ open: false, query: '' });
// Reactive, so an open palette (and its cached results) updates when pages register or remove commands.
const statics = reactive(new Map());
const searches = new Map();

export function registerCommands(source, commands) {
  statics.set(source, markRaw(commands));
  return () => statics.delete(source);
}
export function registerSearch(source, fn) {
  searches.set(source, fn);
  return () => searches.delete(source);
}
export function openPalette(query = '') { palette.query = query; palette.open = true; }
export function closePalette() { palette.open = false; }
export function togglePalette() { palette.open ? closePalette() : openPalette(); }

function score(c, q) {
  if (!q) return 1;
  const hay = `${c.title} ${c.subtitle ?? ''} ${(c.keywords ?? []).join(' ')} ${c.group ?? ''}`.toLowerCase();
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.every((w) => hay.includes(w))) return 0;
  const t = c.title.toLowerCase();
  return t.startsWith(q.toLowerCase()) ? 3 : t.includes(q.toLowerCase()) ? 2 : 1;
}

/** Static matches, best first. */
export function matchStatic(q) {
  const all = [...statics.values()].flat();
  return all.map((c) => [c, score(c, q)]).filter(([, s]) => s > 0).sort((a, b) => b[1] - a[1]).map(([c]) => c);
}
/** Results of every registered search for q (errors ignored). */
export async function matchSearch(q) {
  if (!q) return [];
  const results = await Promise.all([...searches.values()].map((fn) => Promise.resolve().then(() => fn(q)).catch(() => [])));
  return results.flat().filter(Boolean);
}
