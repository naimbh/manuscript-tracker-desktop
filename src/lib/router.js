import { reactive, markRaw, defineAsyncComponent } from 'vue';

/**
 * Tiny hash router. URLs look like #/papers/12?tab=files.
 * Route names and paths match the web app's routes/web.php.
 * `meta.fullBleed`: the page fills the shell's main area edge to edge (no padding, no shell scroll; it scrolls its own panes).
 */
const defs = [
  { name: 'welcome', path: '/welcome', component: () => import('@/pages/Welcome.vue') },
  { name: 'dashboard', path: '/dashboard', component: () => import('@/pages/Desk.vue') },
  { name: 'papers.index', path: '/papers/:paper?', component: () => import('@/pages/papers/Index.vue'), meta: { fullBleed: true } },
  { name: 'publications', path: '/published', component: () => import('@/pages/Publications.vue') },
  { name: 'venues', path: '/venues', component: () => import('@/pages/Venues.vue') },
  { name: 'deadlines', path: '/deadlines', component: () => import('@/pages/Deadlines.vue') },
  { name: 'opportunities', path: '/opportunities', component: () => import('@/pages/Opportunities.vue') },
  { name: 'settings.profile', path: '/settings/profile', component: () => import('@/pages/settings/Profile.vue') },
  { name: 'settings.appearance', path: '/settings/appearance', component: () => import('@/pages/settings/Appearance.vue') },
  { name: 'settings.data', path: '/settings/data', component: () => import('@/pages/settings/Data.vue') },
  { name: 'settings.ai', path: '/settings/ai', component: () => import('@/pages/settings/Ai.vue') },
  { name: 'settings.research', path: '/settings/research', component: () => import('@/pages/settings/Research.vue') },
  { name: 'settings.general', path: '/settings/general', component: () => import('@/pages/settings/General.vue') },
  { name: 'settings.agents', path: '/settings/agents', component: () => import('@/pages/settings/Agents.vue') },
  { name: 'settings.about', path: '/settings/about', component: () => import('@/pages/settings/About.vue') },
];

const redirects = { '/': 'dashboard', '/settings': 'settings.profile', '/home': 'dashboard' };

function compile(def) {
  const keys = [];
  const src = def.path.replace(/\/:(\w+)(\?)?/g, (_, key, opt) => {
    keys.push({ key, optional: !!opt });
    return opt ? '(?:/([^/]+))?' : '/([^/]+)';
  });
  return { ...def, keys, re: new RegExp('^' + src + '/?$') };
}

export const routes = defs.map(compile);
const byName = Object.fromEntries(routes.map((r) => [r.name, r]));

/** Reactive current route: { name, params, query, path, fullPath, component }. */
export const currentRoute = reactive({ name: null, params: {}, query: {}, path: '/', fullPath: '/', meta: {}, component: null });

let guard = null;
/** setGuard(route => 'routeName' | { name, params } | null) runs before every navigation (e.g. onboarding). */
export function setGuard(fn) { guard = fn; }

function parseQuery(qs) {
  const q = {};
  if (!qs) return q;
  for (const [k, v] of new URLSearchParams(qs)) q[k] = v;
  return q;
}

function normaliseParams(def, params) {
  if (params === undefined || params === null || params === '') return {};
  if (typeof params !== 'object') return def.keys.length ? { [def.keys[0].key]: params } : {};
  // A model-like object ({ id, title, ... }) given for a route with a parameter: use its id.
  if (def.keys.length && !(def.keys[0].key in params) && 'id' in params) return { [def.keys[0].key]: params.id };
  return params;
}

/** Path (without '#') for a named route. Extra params become the query string. */
export function path(name, params) {
  const def = byName[name];
  if (!def) throw new Error(`Route [${name}] not defined.`);
  const p = { ...normaliseParams(def, params) };
  let out = def.path.replace(/\/:(\w+)(\?)?/g, (_, key, opt) => {
    let v = p[key];
    delete p[key];
    if (v && typeof v === 'object') v = v.id;
    if (v === undefined || v === null || v === '') {
      if (opt) return '';
      throw new Error(`Missing parameter [${key}] for route [${name}].`);
    }
    return '/' + encodeURIComponent(v);
  });
  const qs = new URLSearchParams();
  for (const [k, v] of Object.entries(p)) if (v !== undefined && v !== null && v !== '') qs.append(k, v);
  const s = qs.toString();
  return out + (s ? '?' + s : '');
}

/** href for a named route: route('papers.index', 12) -> '#/papers/12'; route('papers.index', { paper: 12, tab: 'files' }). */
export function route(name, params) {
  return '#' + path(name, params);
}

export function resolve(fullPath) {
  const [p, qs] = fullPath.split('?');
  const pathOnly = p || '/';
  for (const def of routes) {
    const m = pathOnly.match(def.re);
    if (!m) continue;
    const params = {};
    def.keys.forEach(({ key }, i) => { if (m[i + 1] !== undefined) params[key] = decodeURIComponent(m[i + 1]); });
    return { name: def.name, params, query: parseQuery(qs), path: pathOnly, fullPath, meta: def.meta || {}, component: def.component };
  }
  return null;
}

/** Navigate to a named route (pushes history). navigate('papers.index', 12, { replace: true }). */
export function navigate(name, params, { replace = false } = {}) {
  const url = route(name, params);
  if (replace) {
    history.replaceState(null, '', url);
    sync();
  } else if (location.hash === url) {
    sync();
  } else {
    location.hash = url.slice(1);
  }
}

/** Replace just the query string of the current route (no scroll, no new history entry). */
export function setQuery(query) {
  const merged = { ...currentRoute.query, ...query };
  const qs = new URLSearchParams();
  for (const [k, v] of Object.entries(merged)) if (v !== undefined && v !== null && v !== '') qs.append(k, v);
  const s = qs.toString();
  history.replaceState(null, '', '#' + currentRoute.path + (s ? '?' + s : ''));
  sync();
}

/** routeIs('papers.*'), routeIs('dashboard', 'venues') — like request()->routeIs(). */
export function routeIs(...patterns) {
  const name = currentRoute.name || '';
  return patterns.flat().some((p) => new RegExp('^' + p.split('*').map((s) => s.replace(/[.+?^${}()|[\]\\]/g, '\\$&')).join('.*') + '$').test(name));
}

const loaded = new Map();
function component(loader) {
  if (!loaded.has(loader)) loaded.set(loader, markRaw(defineAsyncComponent(loader)));
  return loaded.get(loader);
}

function sync() {
  const raw = decodeURI(location.hash.replace(/^#/, '')) || '/';
  const [p] = raw.split('?');
  if (redirects[p]) {
    history.replaceState(null, '', route(redirects[p]));
    return sync();
  }
  let r = resolve(raw);
  if (!r) {
    history.replaceState(null, '', route('dashboard'));
    return sync();
  }
  if (guard) {
    const to = guard(r);
    if (to && (typeof to === 'string' ? to : to.name) !== r.name) {
      const t = typeof to === 'string' ? { name: to } : to;
      history.replaceState(null, '', route(t.name, t.params));
      return sync();
    }
  }
  const pathChanged = currentRoute.path !== r.path;
  Object.assign(currentRoute, { ...r, component: component(r.component) });
  if (pathChanged) window.scrollTo(0, 0);
}

/** Load every page's code in the background, so switching pages never waits for a chunk. */
export function prefetchRoutes() {
  for (const r of routes) {
    if (r.name === 'welcome') continue;
    try { component(r.component).__asyncLoader?.().catch(() => {}); } catch {}
  }
}

/** Start listening to hash changes. Call once from main.js (after setGuard if needed). */
export function startRouter() {
  window.addEventListener('hashchange', sync);
  sync();
}

/** Re-run the current route through the guard (e.g. after onboarding finished). */
export function refreshRoute() { sync(); }
