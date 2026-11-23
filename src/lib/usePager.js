import { computed, nextTick, ref, unref, watch } from 'vue';
import { currentRoute, setQuery } from '@/lib/router.js';

export const PAGE_SIZES = [20, 50, 100];
const SIZE_KEY = 'mt.pageSize';

function storedSize(fallback) {
  try {
    const n = Number(localStorage.getItem(SIZE_KEY));
    return PAGE_SIZES.includes(n) ? n : fallback;
  } catch { return fallback; }
}

/**
 * Pages a client-side array (20 per page by default; the 20/50/100 choice is shared and remembered).
 *
 *   const pager = usePager(list, { query: 'page', resetOn: [q, filter, sort], scrollTo: listEl });
 *   pager.items      // the rows on this page
 *   <FluxPagination :pager="pager" />
 *
 * The page lives in the URL query (`?page=2`, name set by `query`; null keeps it local), resets to 1 when any
 * `resetOn` source changes, is clamped when the list shrinks, and the list scrolls (smoothly) to its top on change.
 */
export function usePager(source, { query = 'page', size = null, resetOn = [], scrollTo = null } = {}) {
  const fromUrl = () => Math.max(1, parseInt(query ? currentRoute.query[query] : '', 10) || 1);
  const local = ref(fromUrl());
  const perPage = ref(size ?? storedSize(20));

  const all = computed(() => unref(source) ?? []);
  const total = computed(() => all.value.length);
  const pages = computed(() => Math.max(1, Math.ceil(total.value / perPage.value)));
  const page = computed(() => Math.min(local.value, pages.value));
  const offset = computed(() => (page.value - 1) * perPage.value);
  const items = computed(() => all.value.slice(offset.value, offset.value + perPage.value));
  const from = computed(() => (total.value ? offset.value + 1 : 0));
  const to = computed(() => Math.min(total.value, offset.value + perPage.value));
  const visible = computed(() => total.value > Math.min(...PAGE_SIZES));

  function writeUrl(p) {
    if (query && (Number(currentRoute.query[query]) || 1) !== p) setQuery({ [query]: p > 1 ? p : null });
  }

  function scrollTop() {
    const el = unref(scrollTo)?.$el ?? unref(scrollTo);
    if (!el) return;
    nextTick(() => {
      // A scrollable list scrolls itself; otherwise bring the list's top into view if it is above the window.
      if (el.scrollHeight > el.clientHeight + 1 && getComputedStyle(el).overflowY !== 'visible') {
        el.scrollTo({ top: 0, behavior: 'smooth' });
      } else if (el.getBoundingClientRect().top < 0) {
        el.scrollIntoView({ block: 'start', behavior: 'smooth' });
      }
    });
  }

  function setPage(p, { scroll = true } = {}) {
    const next = Math.min(Math.max(1, p), pages.value);
    if (next === page.value && next === local.value) return;
    local.value = next;
    writeUrl(next);
    if (scroll) scrollTop();
  }

  function setSize(n) {
    if (!PAGE_SIZES.includes(n)) return;
    const first = offset.value;
    perPage.value = n;
    try { localStorage.setItem(SIZE_KEY, String(n)); } catch {}
    setPage(Math.floor(first / n) + 1, { scroll: false });
  }

  /** Page that holds the item at `index` of the full list. */
  const pageOf = (index) => Math.floor(index / perPage.value) + 1;

  /** Jump to the page that holds the first item matching `fn` (e.g. the selected paper). */
  function reveal(fn) {
    const i = all.value.findIndex(fn);
    if (i >= 0 && pageOf(i) !== page.value) setPage(pageOf(i), { scroll: false });
    return i;
  }

  if (resetOn.length) watch(resetOn, () => setPage(1, { scroll: false }));
  if (query) watch(() => currentRoute.query[query], () => { const p = fromUrl(); if (p !== local.value) local.value = p; });
  // Clamp the URL when the list shrinks below the current page.
  watch(pages, (n) => { if (local.value > n) { local.value = n; writeUrl(n); } });

  return { page, pages, perPage, total, items, from, to, offset, visible, setPage, setSize, pageOf, reveal };
}
