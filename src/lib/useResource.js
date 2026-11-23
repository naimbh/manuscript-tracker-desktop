import { onBeforeUnmount, ref, shallowRef } from 'vue';
import { bus } from '@/lib/bus.js';

/** Last data per key, so a page that is opened again shows its content at once and refreshes it in the background. */
const store = new Map();

/**
 * Stale-while-revalidate data for a page or panel.
 *
 *   const pubs = useResource('publications', () => api('publications.list'), { pageReady: true, onData: (d) => … });
 *   pubs.data      // ref: null only before the very first load (show a skeleton then)
 *   pubs.loaded    // ref: true once data has arrived (or came from the cache)
 *   pubs.reload()  // refetch; the current content stays on screen and is swapped in place
 *
 * Reloads are coalesced: a burst of `changed` events (several in the same tick, or while a fetch is running)
 * becomes at most one more fetch, and an older response never overwrites a newer one.
 * `pageReady` emits `page-ready` on the bus after the first load (the shell waits for it before showing the window).
 * `onError(e)` handles failures (default: rethrown to the console; pages pass a toast).
 */
export function useResource(key, fetcher, { pageReady = false, onData = null, onError = null, cache = true } = {}) {
  // `key` may be a function (e.g. a tab): switching to another key shows that key's cached data, or the skeleton.
  const keyOf = () => (typeof key === 'function' ? key() : key);
  let dataKey = keyOf();
  const cached = cache && dataKey ? store.get(dataKey) : undefined;
  const data = shallowRef(cached ?? null);
  const loaded = ref(cached !== undefined);
  const refreshing = ref(false);
  let running = null;
  let again = false;
  let scheduled = null;
  let seq = 0;
  let alive = true;
  let readySent = false;

  function ready() {
    if (pageReady && !readySent) { readySent = true; bus.emit('page-ready'); }
  }

  async function run() {
    const mine = ++seq;
    const k = keyOf();
    if (k !== dataKey) {
      dataKey = k;
      const hit = cache && k ? store.get(k) : undefined;
      data.value = hit ?? null;
      loaded.value = hit !== undefined;
    }
    refreshing.value = true;
    try {
      const value = await fetcher();
      if (!alive || mine !== seq) return;
      data.value = value;
      if (cache && k) store.set(k, value);
      loaded.value = true;
      onData?.(value);
    } catch (e) {
      if (alive) (onError ?? ((err) => console.error(err)))(e);
    } finally {
      if (mine === seq) refreshing.value = false;
      ready();
    }
  }

  /** Refetch. Resolves when the data on screen is current. */
  function reload() {
    if (running) { again = true; return running; }
    running = (async () => {
      do { again = false; await run(); } while (again && alive);
      running = null;
    })();
    return running;
  }

  /** Reload on the next tick, merging every request made in this tick (use for event handlers). */
  function refresh() {
    if (scheduled) return scheduled;
    scheduled = Promise.resolve().then(() => { scheduled = null; return reload(); });
    return scheduled;
  }

  /** Change the data locally (optimistic updates); also updates the cache. */
  function mutate(fn) {
    const next = typeof fn === 'function' ? fn(data.value) : fn;
    data.value = next;
    if (cache && dataKey) store.set(dataKey, next);
  }

  if (cached !== undefined) { onData?.(cached); ready(); }
  onBeforeUnmount(() => { alive = false; });

  return { data, loaded, refreshing, reload, refresh, mutate };
}

/** Forget cached data (e.g. after a backup import or erasing everything). */
export function clearResources() { store.clear(); }
