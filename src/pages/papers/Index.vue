<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { api } from '@/lib/api.js';
import { bus } from '@/lib/bus.js';
import { on } from '@/lib/events.js';
import { STAGES, DECISIONS, isWaiting, tone } from '@/lib/constants.js';
import { before, format, limit } from '@/lib/format.js';
import { modal } from '@/lib/modal.js';
import { confirmDialog, contextMenu, dropTarget, onFileDrop } from '@/lib/native.js';
import { currentRoute, navigate, route, setQuery } from '@/lib/router.js';
import { useShortcut, isTyping } from '@/lib/shortcuts.js';
import { toast } from '@/lib/toast.js';
import { isMac } from '@/lib/platform.js';
import { cache, DOT, papersChanged, setPending, TINT } from './state.js';
import PaperDetail from './PaperDetail.vue';
import { useResource } from '@/lib/useResource.js';
import { usePager } from '@/lib/usePager.js';

const SORTS = {
  added: 'Recently started',
  updated: 'Recently updated',
  submitted: 'Recently submitted',
  ready: 'Ready to submit first',
  stage: 'By stage',
  title: 'Title (A–Z)',
};
const FILTERS = { active: 'Active', review: 'In review', done: 'Accepted', all: 'All' };
const GROUPS = {
  active: (p) => !['published', 'shelved'].includes(p.stage),
  review: (p) => ['submitted', 'under_review', 'revision'].includes(p.stage),
  done: (p) => ['accepted', 'published'].includes(p.stage),
  all: () => true,
};

// Stale-while-revalidate (shared with the Desk-style cache): the list stays on screen while it refreshes.
const res = useResource('papers', () => api('papers.list'), {
  onData: (v) => { cache.list = v.papers; },
  onError: (e) => toast({ variant: 'danger', text: e.message }),
});
if (!res.data.value && cache.list) res.mutate({ papers: cache.list });
const all = computed(() => res.data.value?.papers ?? []);
const loaded = computed(() => res.loaded.value || !!cache.list);
const q = ref(currentRoute.query.q ?? '');
const filter = ref(GROUPS[currentRoute.query.filter] ? currentRoute.query.filter : 'active');
const sort = ref(SORTS[currentRoute.query.sort] ? currentRoute.query.sort : 'added');
const selectedId = computed(() => Number(currentRoute.params.paper) || null);
const root = ref(null);
const nav = ref(null);
const search = ref(null);
const wide = ref(cache.wide);
const cursor = ref(-1);
const animate = !cache.animated;
const dropPaper = ref(null);

document.title = 'Papers';

const load = res.refresh;

/* ---------- filter, search, sort (as Livewire's with() and sorted()) ---------- */
const lower = (s) => String(s ?? '').toLowerCase();
const counts = computed(() => Object.fromEntries(Object.entries(GROUPS).map(([k, f]) => [k, all.value.filter(f).length])));
const tags = computed(() => {
  const seen = new Set();
  const out = [];
  for (const p of all.value) for (const t of p.tags ?? []) if (!seen.has(lower(t))) { seen.add(lower(t)); out.push(t); }
  return out.sort();
});
const tagQuery = computed(() => q.value.trim().startsWith('#'));
const list = computed(() => {
  let l = all.value.filter(GROUPS[filter.value] ?? GROUPS.active);
  const term = q.value.trim();
  if (term.startsWith('#')) {
    const tag = lower(term.replace(/^#+/, ''));
    l = l.filter((p) => (p.tags ?? []).some((t) => lower(t) === tag));
  } else if (term) {
    const needle = lower(term);
    l = l.filter((p) => lower([p.title, p.short_title, p.keywords, p.method, p.current?.venue_name].map((x) => x ?? '').join(' ')).includes(needle));
  }
  const stages = Object.keys(STAGES);
  const ts = (d) => (d ? new Date(String(d).replace(' ', 'T')).getTime() || 0 : 0);
  const by = (f, desc = false) => [...l].sort((a, b) => { const x = f(a), y = f(b); return (x < y ? -1 : x > y ? 1 : 0) * (desc ? -1 : 1); });
  switch (sort.value) {
    case 'updated': return by((p) => ts(p.updated_at), true);
    case 'submitted': return by((p) => Math.max(0, ...p.trail.map((s) => ts(s.submitted_on))), true);
    case 'ready': return by((p) => (p.stage === 'ready' ? 0 : 1));
    case 'stage': return by((p) => { const i = stages.indexOf(p.stage); return i < 0 ? 99 : i; });
    case 'title': return by((p) => lower(p.display_title));
    default: return l;
  }
});

const pager = usePager(list, { resetOn: [q, filter, sort], scrollTo: nav });
/** Keep the open paper on screen: jump to the page that holds it. */
const revealSelected = () => { if (selectedId.value) pager.reveal((p) => p.id === selectedId.value); };
watch(selectedId, revealSelected);

let qTimer = null;
watch(q, (v) => { clearTimeout(qTimer); qTimer = setTimeout(() => setQuery({ q: v || null }), 150); });
watch(filter, (v) => setQuery({ filter: v === 'active' ? null : v }));
watch(sort, (v) => setQuery({ sort: v === 'added' ? null : v }));
watch(() => currentRoute.query.q, (v) => { if ((v ?? '') !== q.value) q.value = v ?? ''; });

/** Link to a paper, keeping the list's search, filter and sort. */
function paperLink(id) {
  return { paper: id, q: q.value || null, filter: filter.value === 'active' ? null : filter.value, sort: sort.value === 'added' ? null : sort.value, page: pager.page.value > 1 ? pager.page.value : null };
}
function hrefFor(id) {
  return route('papers.index', paperLink(id));
}
function open(id, opts = {}) {
  if (!id) return;
  navigate('papers.index', paperLink(id), opts);
}

/* ---------- row helpers ---------- */
function rowTone(p) {
  return p.stage === 'ready' ? 'violet' : p.current ? tone(p.current) : 'zinc';
}
function trail(p) {
  return [...p.trail].sort((a, b) => ((a.submitted_on ?? '') < (b.submitted_on ?? '') ? -1 : (a.submitted_on ?? '') > (b.submitted_on ?? '') ? 1 : a.id - b.id));
}

/* ---------- keyboard: ↑/↓ move through the list, Enter opens ---------- */
const inSearch = (e) => !!e.target?.closest?.('[data-search]');
const canKey = (e) => !document.querySelector('dialog[open]') && !e.target?.closest?.('[data-paper-drop]') && (!isTyping(e) || inSearch(e));
function moveCursor(d) {
  const n = list.value.length;
  if (!n) return;
  const start = cursor.value >= 0 ? cursor.value : list.value.findIndex((p) => p.id === selectedId.value);
  const next = start < 0 ? (d > 0 ? 0 : n - 1) : Math.min(n - 1, Math.max(0, start + d));
  cursor.value = next;
  // ↑/↓ cross page boundaries.
  if (pager.pageOf(next) !== pager.page.value) pager.setPage(pager.pageOf(next), { scroll: false });
  nextTick(() => nav.value?.querySelector(`[data-paper-id="${list.value[next].id}"]`)?.scrollIntoView({ block: 'nearest' }));
  if (wide.value) {
    cache.focusSearch = document.activeElement?.closest?.('[data-search]') != null;
    open(list.value[next].id, { replace: true });
  }
}
useShortcut('arrowdown', () => moveCursor(1), { inInputs: true, when: canKey });
useShortcut('arrowup', () => moveCursor(-1), { inInputs: true, when: canKey });
useShortcut('enter', (e) => {
  const p = list.value[cursor.value] ?? (inSearch(e) && list.value.length === 1 ? list.value[0] : null);
  if (!p || (!inSearch(e) && cursor.value < 0)) return false;
  open(p.id);
}, { inInputs: true, when: canKey });

/* ---------- native context menu on rows ---------- */
async function rowMenu(p) {
  const id = await contextMenu([
    { id: 'open', label: 'Open' },
    { id: 'submit', label: 'Mark submitted…' },
    { id: 'duplicate', label: 'Duplicate' },
    { separator: true },
    { id: 'delete', label: 'Delete paper…', danger: true },
  ]);
  if (id === 'open') open(p.id);
  else if (id === 'submit') { setPending(p.id, 'new-submission'); open(p.id); }
  else if (id === 'duplicate') {
    try {
      const copy = await api('papers.duplicate', { id: p.id });
      toast({ variant: 'success', text: 'Paper duplicated.' });
      papersChanged('papers-list');
      await load();
      open(copy.id);
    } catch (e) { toast({ variant: 'danger', text: e.message }); }
  } else if (id === 'delete') await deletePaper(p.id);
}

async function deletePaper(id) {
  const ok = await confirmDialog('Delete this paper, its submissions and its files? This cannot be undone.', { title: 'Delete paper', okLabel: 'Delete', danger: true });
  if (!ok) return;
  try {
    await api('papers.delete', { id });
    cache.details.delete(id);
    toast({ variant: 'success', text: 'Paper deleted.' });
    papersChanged('papers-list');
    if (selectedId.value === id) navigate('papers.index', paperLink(null));
    else await load();
  } catch (e) { toast({ variant: 'danger', text: e.message }); }
}

function onDeleted() {
  cache.details.delete(selectedId.value);
  navigate('papers.index', paperLink(null));
}

/* ---------- files dragged from Finder/Explorer ---------- */
function paperAt(position) {
  const el = dropTarget(position);
  if (!el || !root.value?.contains(el)) return null;
  const row = el.closest('[data-paper-id]');
  if (row) return Number(row.dataset.paperId);
  return el.closest('[data-paper-drop]') ? selectedId.value : null;
}

/* ---------- layout: master–detail when wide (the route is full-bleed: the list and the detail scroll on their own) ---------- */
let ro = null;
let offs = [];
onMounted(async () => {
  ro = new ResizeObserver(([e]) => { wide.value = e.contentRect.width >= 860; cache.wide = wide.value; });
  if (root.value) ro.observe(root.value);
  offs.push(bus.on('papers-changed', (e) => { if (e?.source !== 'papers-list') load(); }));
  offs.push(on('changed', (p) => { if (['papers', 'external'].includes(p?.area)) load(); }));
  offs.push(onFileDrop(({ type, position, files }) => {
    if (type === 'leave') { dropPaper.value = null; return; }
    const id = paperAt(position);
    if (type !== 'drop') { dropPaper.value = id; return; }
    dropPaper.value = null;
    if (!id || !files.length) return;
    setPending(id, 'attach', files);
    if (id !== selectedId.value) open(id);
  }));
  cache.animated = true;
  await nextTick();
  if (nav.value) nav.value.scrollTop = cache.scroll;
  if (cache.focusSearch) { cache.focusSearch = false; search.value?.focus?.(); }
  await load();
  revealSelected();
  await nextTick();
  if (selectedId.value) nav.value?.querySelector(`[data-paper-id="${selectedId.value}"]`)?.scrollIntoView({ block: 'nearest' });
});
onBeforeUnmount(() => {
  cache.scroll = nav.value?.scrollTop ?? 0;
  ro?.disconnect();
  offs.forEach((f) => f());
  clearTimeout(qTimer);
});

watch(selectedId, () => { cursor.value = -1; });
watch(list, () => { if (cursor.value >= list.value.length) cursor.value = list.value.length - 1; });
</script>

<template>
    <div ref="root" class="flex min-h-0 flex-1 overflow-hidden">
        <!-- List -->
        <aside :class="[(selectedId && !wide) ? 'hidden' : 'flex', wide ? 'w-[22rem] xl:w-[26rem] border-r' : 'w-full', 'min-h-0 shrink-0 flex-col border-zinc-200/70 bg-white/40 dark:border-zinc-800 dark:bg-zinc-900/30']">
            <div :class="['sticky top-0 z-10 space-y-3 border-b border-zinc-200/80 bg-[#f7f8fc]/90 p-4 backdrop-blur', isMac ? 'pt-9' : '[.sidebar-collapsed_&]:pl-12']" class=" select-none dark:border-zinc-800 dark:bg-zinc-900/80" data-tauri-drag-region>
                <div class="flex items-center justify-between" data-tauri-drag-region>
                    <h1 class="text-lg font-semibold tracking-tight" data-tauri-drag-region>Papers</h1>
                    <FluxModalTrigger name="new-paper">
                        <FluxButton size="sm" variant="ghost" square icon="plus" aria-label="New paper" tooltip="New paper" tooltip-kbd="N" tooltip-position="bottom" />
                    </FluxModalTrigger>
                </div>
                <FluxInput ref="search" v-model="q" data-search icon="magnifying-glass" placeholder="Search titles, venues, keywords" size="sm" clearable>
                    <template #iconTrailing><span class="kbd">/</span></template>
                </FluxInput>
                <div class="flex items-stretch gap-2">
                    <div class="flex min-w-0 flex-1 gap-0.5 rounded-lg bg-zinc-100 p-0.5 text-[11px] font-medium sm:text-xs dark:bg-zinc-800">
                        <button v-for="(label, k) in FILTERS" :key="k" type="button" @click="filter = k"
                                :class="['flex-1 rounded-md px-1 py-1.5 whitespace-nowrap transition sm:px-1.5', filter === k ? 'bg-white text-zinc-900 shadow-xs dark:bg-zinc-700 dark:text-white' : 'text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200']">
                            {{ label }} <span class="num opacity-60">{{ counts[k] }}</span>
                        </button>
                    </div>
                    <FluxDropdown align="end">
                        <button type="button" :title="`Sort: ${SORTS[sort] ?? ''}`" :class="['flex h-full items-center gap-1 rounded-lg px-2.5 text-xs font-medium transition', sort !== 'added' ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-500/10 dark:text-indigo-300' : 'text-zinc-500 hover:bg-zinc-100 hover:text-zinc-800 dark:hover:bg-zinc-800']">
                            <FluxIcon name="arrows-up-down" variant="micro" class="size-3.5" /><span class="sr-only">Sort</span>
                        </button>
                        <FluxMenu>
                            <FluxMenuHeading>Sort by</FluxMenuHeading>
                            <FluxMenuItem v-for="(label, k) in SORTS" :key="k" :icon="sort === k ? 'check' : null" :class="sort === k ? '' : 'ps-9'" @click="sort = k">{{ label }}</FluxMenuItem>
                        </FluxMenu>
                    </FluxDropdown>
                    <FluxDropdown v-if="tags.length" align="end">
                        <button type="button" :class="['flex h-full items-center gap-1 rounded-lg px-2.5 text-xs font-medium transition', tagQuery ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-500/10 dark:text-indigo-300' : 'text-zinc-500 hover:bg-zinc-100 hover:text-zinc-800 dark:hover:bg-zinc-800']">
                            <FluxIcon name="tag" variant="micro" class="size-3.5" /><span class="max-[359px]:sr-only">{{ tagQuery ? q.trim().replace(/^#+/, '') : 'Tags' }}</span>
                        </button>
                        <FluxMenu>
                            <template v-if="tagQuery"><FluxMenuItem icon="x-mark" @click="q = ''">Clear tag</FluxMenuItem><FluxMenuSeparator /></template>
                            <FluxMenuItem v-for="t in tags" :key="t" @click="q = '#' + t">{{ t }}</FluxMenuItem>
                        </FluxMenu>
                    </FluxDropdown>
                </div>
            </div>

            <nav ref="nav" class="flex-1 overflow-y-auto p-3" @scroll.passive="cache.scroll = nav?.scrollTop ?? 0">
              <UiList :class="['space-y-2', animate ? 'stagger' : '']">
                <a v-for="(p, i) in pager.items.value" :key="'p' + p.id" :href="hrefFor(p.id)" :data-paper-id="p.id"
                   @contextmenu.prevent="rowMenu(p)" draggable="false"
                   :class="['group relative block overflow-hidden rounded-xl py-3 pr-3 pl-4 transition before:absolute before:inset-y-0 before:left-0 before:w-0.5 select-none', TINT[rowTone(p)],
                            dropPaper === p.id ? 'ring-2 ring-indigo-500 ring-offset-2 ring-offset-[#f7f8fc] dark:ring-offset-zinc-950'
                            : selectedId === p.id ? 'shadow-md ring-2 shadow-zinc-900/5 ring-indigo-500/70 dark:ring-indigo-400/60'
                            : cursor === pager.offset.value + i ? 'ring-2 ring-indigo-300 dark:ring-indigo-500/40'
                            : p.stage === 'ready' ? 'shadow-sm shadow-violet-500/10 ring-1 ring-violet-300 dark:ring-violet-500/40' : 'ring-1 ring-zinc-200/70 dark:ring-zinc-800']">
                    <div class="flex items-center justify-between gap-2">
                        <span class="truncate text-[10px] font-bold tracking-[.08em] text-zinc-500 uppercase dark:text-zinc-400">{{ p.short_title || 'Paper' }}</span>
                        <span v-if="p.stage === 'ready'" class="inline-flex items-center gap-1 rounded-full bg-gradient-to-r from-violet-600 to-fuchsia-600 px-2 py-0.5 text-[11px] font-semibold text-white shadow-sm shadow-violet-500/30"><FluxIcon name="paper-airplane" variant="micro" class="size-3" />Ready to submit</span>
                        <UiDecision v-else-if="p.current" :s="p.current" class="!py-0 !text-[11px]" />
                        <UiStage v-else :stage="p.stage" />
                    </div>
                    <div class="mt-1 line-clamp-3 text-[13.5px] leading-snug font-semibold text-zinc-900 dark:text-zinc-50">{{ p.title }}</div>
                    <div v-if="p.tags?.length" class="mt-1.5 flex flex-wrap gap-1"><UiTag v-for="t in p.tags" :key="t" :tag="t" /></div>

                    <div v-if="p.trail.length" class="mt-2 flex flex-wrap items-center gap-1">
                        <template v-for="(t, i) in trail(p)" :key="t.id">
                            <FluxIcon v-if="i > 0" name="chevron-right" variant="micro" class="size-3 text-zinc-300 dark:text-zinc-600" />
                            <span :class="['inline-flex max-w-[15rem] items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px]', t.id === p.current?.id ? 'bg-white font-semibold text-zinc-800 shadow-xs ring-1 ring-zinc-200 dark:bg-zinc-800 dark:text-zinc-100 dark:ring-zinc-700' : 'text-zinc-500 line-through decoration-zinc-300 dark:decoration-zinc-600']" :title="`${t.venue_name} · ${DECISIONS[t.decision] ?? t.decision}`">
                                <span :class="['size-1.5 shrink-0 rounded-full', DOT[tone(t)]]"></span>
                                <span class="truncate">{{ limit(before(t.venue_name, ' ('), 34) }}</span>
                            </span>
                        </template>
                    </div>
                    <div v-else-if="p.next_venue" class="mt-2 flex items-center gap-1.5 text-[11px] text-zinc-500"><FluxIcon name="arrow-right" variant="micro" class="size-3 text-violet-500" />Next: <span class="truncate font-medium text-zinc-700 dark:text-zinc-300">{{ p.next_venue }}</span></div>

                    <div v-if="p.current" class="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-zinc-500">
                        <span v-if="p.current.manuscript_id" class="font-mono text-zinc-600 dark:text-zinc-300">{{ p.current.manuscript_id }}</span>
                        <span v-if="p.current.submitted_on">Sent {{ format(p.current.submitted_on, 'j M') }}</span>
                        <span v-if="p.current.status_on">· {{ DECISIONS[p.current.decision] }} {{ format(p.current.status_on, 'j M') }}</span>
                        <span v-if="isWaiting(p.current) && p.current.days !== null" :class="['num ml-auto font-semibold', p.current.clock?.overdue ? 'text-rose-500' : 'text-zinc-600 dark:text-zinc-300']">{{ p.current.days }}d</span>
                    </div>
                    <div v-if="p.current?.clock" class="clock-track mt-2 !h-1 !bg-white/70 dark:!bg-zinc-800"><div :class="['clock-fill grow-x', p.current.clock.overdue ? 'bg-rose-500' : 'bg-indigo-400/80']" :style="`width: ${p.current.clock.pct}%`"></div></div>
                </a>
              </UiList>
                <div v-if="loaded && !list.length" class="px-4 py-10 text-center text-sm text-zinc-500">
                    {{ q ? 'No papers match “' + q + '”.' : 'No papers here yet.' }}
                </div>
                <FluxPagination :pager="pager" :sizes="false" compact label="Papers" class="mt-3 px-1" />
            </nav>
        </aside>

        <!-- Record -->
        <section v-if="selectedId || wide" class="flex min-h-0 min-w-0 flex-1 flex-col">
            <PaperDetail v-if="selectedId" :key="'detail-' + selectedId" :id="selectedId" :wide="wide" :back-href="hrefFor(null)" :dropping="dropPaper === selectedId" @deleted="onDeleted" @delete="deletePaper(selectedId)" />
            <div v-else class="flex flex-1 items-center justify-center" data-tauri-drag-region>
                <UiEmpty title="Pick a paper" text="Choose a paper on the left to see its submissions, files and next steps. Press N to add a new one.">
                    <FluxButton icon="plus" @click="modal('new-paper').show()">New paper</FluxButton>
                </UiEmpty>
            </div>
        </section>
    </div>
</template>
