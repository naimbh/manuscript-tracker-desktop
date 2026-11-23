<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { api } from '@/lib/api.js';
import { on } from '@/lib/events.js';
import { bus } from '@/lib/bus.js';
import { toast } from '@/lib/toast.js';
import { contextMenu } from '@/lib/native.js';
import { currentRoute, setQuery } from '@/lib/router.js';
import { REVIEW_TYPES } from '@/lib/constants.js';
import { before, diffForHumans, format, plural } from '@/lib/format.js';
import { openUrl, utc } from '@/components/shell/links.js';
import VenueForm from './venues/VenueForm.vue';
import ReviewPanel from './venues/ReviewPanel.vue';
import RunProgress from './venues/RunProgress.vue';
import { reviewText, useRuns } from './venues/runs.js';
import { useResource } from '@/lib/useResource.js';
import { usePager } from '@/lib/usePager.js';

// Stale-while-revalidate: the library stays on screen while AI runs and `changed` events refresh it.
const res = useResource('venues', () => api('venues.list'), {
  pageReady: true,
  onError: (e) => toast({ variant: 'danger', text: e.message }),
});
const venues = computed(() => res.data.value?.venues ?? []);
const pending = computed(() => res.data.value?.pending ?? 0);
const aiReady = computed(() => !!res.data.value?.aiReady);
const aiStatus = computed(() => res.data.value?.aiStatus ?? '');
const loaded = res.loaded;
const listEl = ref(null);
const tab = ref(currentRoute.query.tab === 'journal' ? 'journal' : 'conference');
const q = ref(currentRoute.query.q ?? '');
const field = ref(currentRoute.query.field ?? '');
const errors = ref([]);
const form = ref(null);
const review = ref(null);
const load = res.refresh;
let fitted = false;
watch(loaded, (v) => { if (v && !fitted) { fitted = true; fitTabToQuery(); } }, { immediate: true });

// ---- Filters (instant; the web app did the same in `with()`) ----
const counts = computed(() => ({
  conference: venues.value.filter((v) => v.kind === 'conference').length,
  journal: venues.value.filter((v) => v.kind === 'journal').length,
}));
const inTab = computed(() => venues.value.filter((v) => v.kind === tab.value));
const fields = computed(() => [...new Set(inTab.value.map((v) => v.field).filter(Boolean))].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)));
const matches = (v, needle) => `${v.name} ${v.publisher ?? ''} ${v.field ?? ''} ${v.next_edition ?? ''}`.toLowerCase().includes(needle);
const cmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const list = computed(() => {
  let l = inTab.value;
  if (field.value) l = l.filter((v) => v.field === field.value);
  const needle = q.value.trim().toLowerCase();
  if (needle) l = l.filter((v) => matches(v, needle));
  const key = tab.value === 'conference'
    ? (v) => (v.deadline_on && v.deadline_days >= 0 ? '0' + v.deadline_on.replaceAll('-', '') : v.deadline_label !== null ? '1' + v.name : '2' + v.name)
    : (v) => v.name.toLowerCase();
  return [...l].sort((a, b) => cmp(key(a), key(b)));
});
const pager = usePager(list, { resetOn: [tab, field, q], scrollTo: listEl });
const soon = computed(() => venues.value
  .filter((v) => v.kind === 'conference' && v.deadline_on && v.deadline_days >= 0)
  .sort((a, b) => cmp(a.deadline_on, b.deadline_on))
  .slice(0, 4));

function setTab(k) {
  tab.value = k;
  field.value = '';
}
watch([tab, field], () => setQuery({ tab: tab.value === 'conference' ? null : tab.value, field: field.value || null }));
let qTimer = null;
watch(q, (val) => {
  clearTimeout(qTimer);
  qTimer = setTimeout(() => { if ((currentRoute.query.q ?? '') !== val) setQuery({ q: val || null }); }, 150);
});
// The command palette links here with ?q=<venue name>: follow it, and open the tab that has the venue.
watch(() => currentRoute.query.q, (val) => {
  if ((val ?? '') === q.value) return;
  q.value = val ?? '';
  field.value = '';
  fitTabToQuery();
});
function fitTabToQuery() {
  const needle = q.value.trim().toLowerCase();
  if (!needle) return;
  const other = tab.value === 'journal' ? 'conference' : 'journal';
  const here = venues.value.some((v) => v.kind === tab.value && matches(v, needle));
  if (!here && venues.value.some((v) => v.kind === other && matches(v, needle))) setTab(other);
}

// ---- AI research runs ----
const runs = useRuns(['venues'], {
  onFinish(r) {
    const res = r.result ?? {};
    if (r.keep) {
      if (r.status === 'cancelled') toast({ text: 'Update stopped.' });
      else if (r.status === 'failed') toast({ variant: 'danger', heading: 'Update failed', text: res.error });
      else {
        const failed = res.failed ?? 0;
        const more = res.pending ? ` ${reviewText(res.pending)}.` : '';
        toast({ variant: failed ? 'warning' : 'success', text: 'Checked ' + (res.checked ?? r.done) + (Number(res.checked ?? r.done) === 1 ? ' journal or conference' : ' journals and conferences') + (failed ? ', ' + failed + ' failed.' : '.') + more });
      }
      errors.value = r.results.filter((p) => p.venue && !p.venue.ok).map((p) => p.venue.name + ': ' + p.venue.error);
    } else if (r.status === 'cancelled') {
      toast({ text: 'Update stopped.' });
    } else if (res.ok) {
      const more = res.pending?.length ? ` ${reviewText(res.pending.length)}.` : '';
      toast({ variant: 'success', text: 'Updated ' + res.name + '.' + more });
    } else if (res.venue_id || res.error) {
      toast({ variant: 'danger', heading: 'Update failed', text: res.error || 'The journal or conference could not be checked.' });
    }
    runs.dismiss(r.run_id);
    load();
  },
});
const batch = computed(() => runs.list.value.find((r) => r.status === 'running' && (r.keep || r.total > 1)) ?? null);
const busy = computed(() => Object.fromEntries(runs.active.value.filter((r) => r.venue_id).map((r) => [r.venue_id, true])));

async function single(id) {
  if (busy.value[id]) return;
  errors.value = [];
  try {
    const { run } = await api('venues.research', { id });
    runs.track(run, { venue_id: id });
  } catch (e) {
    toast({ variant: 'danger', heading: 'Update failed', text: e.message });
  }
}

async function all() {
  errors.value = [];
  try {
    const r = await api('venues.refreshAll', { ids: list.value.map((v) => v.id), tab: tab.value });
    runs.track(r.run, { keep: true, total: r.total });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

function stop() {
  if (batch.value) runs.cancel(batch.value.run_id);
}

function resultText(p) {
  const v = p.venue;
  if (!v) return '';
  if (!v.ok) return `${before(v.name, ' (')}: ${v.error}`;
  const bits = [];
  if (v.applied?.length) bits.push(`${v.applied.length} filled in`);
  if (v.pending?.length) bits.push(reviewText(v.pending.length));
  return `${before(v.name, ' (')}: ${bits.length ? bits.join(', ') : 'no changes'}`;
}

// ---- Actions ----
async function toggleWatch(v) {
  try {
    const r = await api('venues.toggleWatch', { id: v.id });
    res.mutate((cur) => cur && { ...cur, venues: cur.venues.map((x) => (x.id === v.id ? { ...x, watch: r.watch } : x)) });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

async function restoreCatalog() {
  try {
    const { added: n } = await api('venues.restoreCatalog');
    toast({ variant: 'success', text: n ? `Added ${n} journals and conferences from the starter list.` : 'You already have everything in the starter list.' });
    await load();
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

function add() { form.value?.open(null, tab.value); }
function edit(id) { form.value?.open(id); }

function saved(r) {
  tab.value = r.tab;
  bus.emit('venues-changed');
  load();
}

async function rowMenu(v) {
  const id = await contextMenu([
    ...(v.website_url ? [{ id: 'website', label: 'Open website' }] : []),
    { id: 'update', label: 'Update with AI', enabled: !busy.value[v.id] && !batch.value },
    ...(v.pending ? [{ id: 'review', label: `Review ${reviewText(v.pending)}` }] : []),
    { id: 'watch', label: v.watch ? 'Stop watching' : 'Watch weekly' },
    { separator: true },
    { id: 'edit', label: 'Edit' },
    { id: 'delete', label: 'Remove…' },
  ]);
  if (id === 'website') openUrl(v.website_url);
  if (id === 'update') single(v.id);
  if (id === 'review') review.value?.open(v.id);
  if (id === 'watch') toggleWatch(v);
  if (id === 'edit') edit(v.id);
  if (id === 'delete') form.value?.remove(v.id);
}

const meta = (v) => [v.publisher, v.field].filter(Boolean).join(' · ');
const checked = (v) => diffForHumans(utc(v.checked_at));
const sources = (v) => (Array.isArray(v.sources) ? v.sources : []);
const iconBtn = 'inline-flex size-7 items-center justify-center rounded-md text-zinc-500 transition hover:bg-zinc-100 hover:text-zinc-800 disabled:opacity-40 dark:hover:bg-zinc-800 dark:hover:text-zinc-200';
const chip = (on) => (on ? 'bg-zinc-900 text-white dark:bg-white dark:text-zinc-900' : 'bg-zinc-100 text-zinc-600 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-zinc-300');

const offs = [];
onMounted(() => {
  load();
  offs.push(on('changed', ({ area } = {}) => { if (['venues', 'proposals', 'backup', 'papers', 'external'].includes(area)) load(); }));
  offs.push(bus.on('papers-changed', load));
});
onBeforeUnmount(() => { clearTimeout(qTimer); offs.forEach((off) => off()); });
</script>

<template>
    <div class="page-in mx-auto w-full max-w-6xl space-y-6 2xl:max-w-[1440px]">
        <!-- Header -->
        <section class="hero relative overflow-hidden rounded-2xl border border-zinc-200/80 px-8 py-6 dark:border-zinc-800">
            <div class="hero-grid pointer-events-none absolute inset-0 text-zinc-900 dark:text-white"></div>
            <div class="relative flex flex-row flex-wrap items-end justify-between gap-4">
                <div>
                    <p class="eyebrow">Journals &amp; Conferences</p>
                    <h1 class="mt-1 text-2xl font-semibold tracking-tight">Where your papers can go</h1>
                    <p class="mt-1 max-w-xl text-sm text-zinc-600 dark:text-zinc-400">Conferences that publish proceedings, and journals with their speed and acceptance rate. Keep them current with one click.</p>
                </div>
                <div class="flex flex-wrap items-center gap-2 select-none">
                    <FluxTooltip :content="aiStatus">
                        <span class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium" :class="aiReady ? 'border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-500/20 dark:bg-emerald-500/10 dark:text-emerald-300' : 'border-zinc-200 bg-white/70 text-zinc-500 dark:border-zinc-700 dark:bg-zinc-900/60'">
                            <FluxIcon name="sparkles" variant="micro" /> {{ aiReady ? 'AI updates on' : 'AI updates off' }}
                        </span>
                    </FluxTooltip>
                    <FluxButton icon="plus" @click="add">Add journal or conference</FluxButton>
                    <FluxButton v-if="!batch" variant="primary" icon="sparkles" :disabled="!list.length" @click="all">Update {{ tab === 'journal' ? 'journals' : 'conferences' }}</FluxButton>
                    <FluxButton v-else variant="danger" icon="stop" @click="stop">Stop</FluxButton>
                </div>
            </div>

            <!-- Progress -->
            <UiCollapse>
            <RunProgress v-if="batch" :run="batch" text="Searching the web for the latest calls and journal data…">
                <template #result="{ r }">
                    <FluxIcon :name="r.venue?.ok ? 'check-circle' : 'exclamation-triangle'" variant="micro" class="size-3.5 shrink-0" :class="r.venue?.ok ? 'text-emerald-500' : 'text-amber-500'" />
                    <span class="truncate">{{ resultText(r) }}</span>
                </template>
            </RunProgress>
            </UiCollapse>

            <UiList v-if="tab === 'conference' && soon.length" :pin="false" class="stagger relative mt-6 grid grid-cols-4 gap-3">
                <div v-for="v in soon" :key="'s' + v.id" class="lift rounded-xl border border-white/60 bg-white/70 p-3.5 backdrop-blur dark:border-white/5 dark:bg-zinc-900/60">
                    <div class="flex items-center justify-between">
                        <span class="num text-2xl font-semibold tracking-tight">{{ v.deadline_days }}<span class="ml-0.5 text-sm font-medium text-zinc-400">days</span></span>
                        <span v-if="v.deadline_expected" class="rounded-full border border-dashed border-amber-400 px-1.5 text-[10px] font-medium text-amber-600">expected</span>
                    </div>
                    <div class="mt-1 truncate text-sm font-medium">{{ before(v.name, ' (') }}</div>
                    <div class="truncate text-xs text-zinc-500">{{ format(v.deadline_on, 'j M Y') }} · {{ v.deadline_label }}</div>
                </div>
            </UiList>
        </section>

        <!-- Changes waiting for review -->
        <UiCollapse><FluxCallout v-if="pending" icon="sparkles" color="indigo">
            <FluxCalloutHeading>{{ reviewText(pending) }}</FluxCalloutHeading>
            <FluxCalloutText>AI updates that would replace what you entered, or that it could not verify, wait for your OK.</FluxCalloutText>
            <template #actions><FluxButton size="sm" variant="primary" @click="review?.open()">Review</FluxButton></template>
        </FluxCallout></UiCollapse>

        <!-- Errors from AI runs -->
        <UiCollapse><FluxCallout v-if="errors.length" variant="warning" icon="exclamation-triangle" heading="Some updates did not finish">
            <FluxCalloutText><ul class="list-disc pl-4"><li v-for="(e, i) in errors" :key="i">{{ e }}</li></ul></FluxCalloutText>
        </FluxCallout></UiCollapse>

        <!-- Toolbar -->
        <div class="flex flex-wrap items-center gap-3 select-none">
            <div class="relative flex rounded-lg bg-zinc-100 p-0.5 text-sm font-medium dark:bg-zinc-800">
                <button v-for="(label, k) in { conference: 'Conferences', journal: 'Journals' }" :key="k" type="button" class="relative rounded-md px-3.5 py-1.5 transition" :class="tab === k ? 'bg-white text-zinc-900 shadow-xs dark:bg-zinc-700 dark:text-white' : 'text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200'" @click="setTab(k)">
                    {{ label }} <span class="num ml-1 text-xs opacity-60">{{ counts[k] }}</span>
                </button>
            </div>
            <div class="w-64"><FluxInput v-model="q" data-search icon="magnifying-glass" size="sm" placeholder="Search venues" clearable /></div>
            <div class="flex flex-wrap gap-1.5">
                <button type="button" class="rounded-full px-2.5 py-1 text-xs font-medium transition" :class="chip(field === '')" @click="field = ''">All fields</button>
                <button v-for="f in fields" :key="f" type="button" class="rounded-full px-2.5 py-1 text-xs font-medium transition" :class="chip(field === f)" @click="field = f">{{ f }}</button>
            </div>
        </div>

        <div v-if="!loaded" class="grid grid-cols-2 gap-3 2xl:grid-cols-3" aria-hidden="true"><div v-for="i in 4" :key="i" class="panel h-40 animate-pulse bg-zinc-100/60 dark:bg-zinc-800/40"></div></div>
        <div v-else ref="listEl" class="scroll-mt-4 space-y-4">
            <div v-if="!list.length" class="panel">
                <UiEmpty art="venue" title="No journals or conferences here" text="Add one yourself, or bring back the starter list of IS, iSchool and HCI journals and conferences.">
                    <div class="flex gap-2"><FluxButton icon="plus" @click="add">Add journal or conference</FluxButton><FluxButton variant="ghost" @click="restoreCatalog">Restore starter list</FluxButton></div>
                </UiEmpty>
            </div>

            <!-- Conference cards -->
            <UiList v-else-if="tab === 'conference'" class="stagger grid grid-cols-2 gap-3 2xl:grid-cols-3">
                <article v-for="v in pager.items.value" :key="'v' + v.id" class="panel lift group relative flex gap-4 p-4" :class="busy[v.id] && 'ring-2 ring-indigo-400/60'" @contextmenu.prevent="rowMenu(v)">
                    <!-- Date block -->
                    <div class="flex w-16 shrink-0 flex-col items-center justify-center rounded-xl border py-2 text-center leading-none select-none"
                         :class="v.deadline_on && v.deadline_days >= 0 ? (v.deadline_expected ? 'border-dashed border-amber-300 bg-amber-50/60 dark:border-amber-500/40 dark:bg-amber-500/5' : 'border-indigo-200 bg-indigo-50/70 dark:border-indigo-500/30 dark:bg-indigo-500/10') : 'border-zinc-200 bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-800/50'">
                        <template v-if="v.deadline_on">
                            <span class="text-[10px] font-semibold tracking-wide uppercase" :class="v.deadline_days < 0 ? 'text-zinc-400' : 'text-rose-500'">{{ format(v.deadline_on, 'M') }}</span>
                            <span class="num mt-0.5 text-xl font-semibold" :class="v.deadline_days < 0 && 'text-zinc-400 line-through'">{{ format(v.deadline_on, 'j') }}</span>
                            <span class="num mt-1 text-[10px] text-zinc-500">{{ v.deadline_days < 0 ? 'passed' : (v.deadline_days === 0 ? 'today' : v.deadline_days + 'd left') }}</span>
                        </template>
                        <template v-else>
                            <FluxIcon name="calendar" class="size-5 text-zinc-300 dark:text-zinc-600" />
                            <span class="mt-1 text-[10px] text-zinc-400">{{ v.deadline_label ? 'see note' : 'no date' }}</span>
                        </template>
                    </div>

                    <div class="min-w-0 flex-1">
                        <div class="flex items-start justify-between gap-2">
                            <div class="min-w-0">
                                <h3 class="text-sm leading-snug font-semibold">{{ v.name }}</h3>
                                <p class="mt-0.5 text-xs text-zinc-500">{{ meta(v) }}</p>
                            </div>
                            <div class="flex shrink-0 items-center opacity-60 transition select-none group-hover:opacity-100">
                                <button type="button" :title="v.watch ? 'Watched: updated weekly' : 'Watch for weekly updates'" :class="[iconBtn, v.watch && '!text-indigo-600 dark:!text-indigo-400']" @click="toggleWatch(v)"><FluxIcon :name="v.watch ? 'bell-alert' : 'bell'" variant="micro" /></button>
                                <button type="button" :disabled="busy[v.id] || !!batch" title="Update with AI" :class="iconBtn" @click="single(v.id)"><FluxIcon name="sparkles" variant="micro" /></button>
                                <button type="button" title="Edit" :class="iconBtn" @click="edit(v.id)"><FluxIcon name="pencil-square" variant="micro" /></button>
                            </div>
                        </div>

                        <div class="mt-2.5 flex flex-wrap gap-1.5">
                            <span v-if="v.acceptance_rate" class="rounded-md bg-sky-50 px-1.5 py-0.5 text-[11px] font-medium text-sky-700 dark:bg-sky-500/10 dark:text-sky-300">{{ v.acceptance_rate }} accepted</span>
                            <span v-if="v.proceedings" class="inline-flex items-center gap-1 rounded-md bg-zinc-100 px-1.5 py-0.5 text-[11px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"><FluxIcon name="book-open" variant="micro" class="size-3" />{{ v.proceedings }}</span>
                            <span v-if="v.submissions_count" class="rounded-md bg-emerald-50 px-1.5 py-0.5 text-[11px] font-medium text-emerald-700 dark:bg-emerald-500/10 dark:text-emerald-300">{{ v.submissions_count }} of your submissions</span>
                            <button v-if="v.pending" type="button" class="inline-flex items-center gap-1 rounded-md bg-amber-50 px-1.5 py-0.5 text-[11px] font-medium text-amber-700 hover:bg-amber-100 dark:bg-amber-500/10 dark:text-amber-300" @click="review?.open(v.id)"><FluxIcon name="sparkles" variant="micro" class="size-3" />{{ reviewText(v.pending) }}</button>
                        </div>

                        <dl class="mt-2.5 space-y-1 text-xs">
                            <div v-if="v.next_edition" class="flex gap-1.5 text-zinc-600 dark:text-zinc-300"><FluxIcon name="map-pin" variant="micro" class="mt-px size-3.5 shrink-0 text-zinc-400" />{{ v.next_edition }}</div>
                            <div v-if="v.deadline_label" class="flex gap-1.5 text-zinc-600 dark:text-zinc-300"><FluxIcon name="clock" variant="micro" class="mt-px size-3.5 shrink-0 text-zinc-400" />{{ v.deadline_label }}<span v-if="v.deadline_expected" class="text-amber-600">· expected</span></div>
                        </dl>

                        <div class="mt-3 flex items-center justify-between border-t border-zinc-100 pt-2.5 text-[11px] text-zinc-400 dark:border-zinc-800">
                            <span v-if="!busy[v.id]">
                                <template v-if="v.checked_at">Checked {{ checked(v) }}<template v-if="sources(v).length"> · <a :href="sources(v)[0]" class="hover:text-indigo-600">{{ sources(v).length }} {{ plural('source', sources(v).length) }}</a></template></template>
                                <template v-else>Not checked yet</template>
                            </span>
                            <span v-else class="flex items-center gap-1.5 text-indigo-600"><FluxIcon name="arrow-path" variant="micro" class="size-3 animate-spin" /> Searching…</span>
                            <a v-if="v.website_url" :href="v.website_url" class="inline-flex items-center gap-1 font-medium text-zinc-500 hover:text-indigo-600">Website <FluxIcon name="arrow-up-right" variant="micro" class="size-3" /></a>
                        </div>
                    </div>
                </article>
            </UiList>

            <!-- Journal table -->
            <div v-else class="panel overflow-hidden">
                <div class="overflow-x-auto">
                    <table class="w-full min-w-[860px] text-sm">
                        <thead>
                            <tr class="border-b border-zinc-200 bg-zinc-50/80 text-left text-[11px] font-semibold tracking-wide text-zinc-500 uppercase select-none dark:border-zinc-800 dark:bg-zinc-900">
                                <th class="px-4 py-2.5">Journal</th>
                                <th class="px-3 py-2.5">First decision</th>
                                <th class="px-3 py-2.5">Acceptance</th>
                                <th class="px-3 py-2.5">Review</th>
                                <th class="px-3 py-2.5">Fee</th>
                                <th class="px-3 py-2.5">Words</th>
                                <th class="px-3 py-2.5"></th>
                            </tr>
                        </thead>
                        <UiList tag="tbody" :pin="false" class="stagger">
                            <tr v-for="v in pager.items.value" :key="'j' + v.id" class="group border-b border-zinc-100 transition-colors last:border-0 hover:bg-zinc-50/70 dark:border-zinc-800/70 dark:hover:bg-zinc-800/30" :class="busy[v.id] && 'bg-indigo-50/60 dark:bg-indigo-500/5'" @contextmenu.prevent="rowMenu(v)">
                                <td class="px-4 py-3">
                                    <div class="font-medium">{{ v.name }}</div>
                                    <div class="mt-0.5 flex flex-wrap items-center gap-x-2 text-xs text-zinc-500">
                                        <span>{{ meta(v) }}</span>
                                        <span v-if="v.submissions_count" class="text-emerald-600">· {{ v.submissions_count }} of yours</span>
                                        <button v-if="v.pending" type="button" class="text-amber-600 hover:underline" @click="review?.open(v.id)">· {{ reviewText(v.pending) }}</button>
                                        <span v-if="busy[v.id]" class="flex items-center gap-1 text-indigo-600"><FluxIcon name="arrow-path" variant="micro" class="size-3 animate-spin" />Searching…</span>
                                    </div>
                                </td>
                                <td class="px-3 py-3 text-zinc-600 dark:text-zinc-300">{{ v.first_decision ?? '–' }}</td>
                                <td class="px-3 py-3 text-zinc-600 dark:text-zinc-300">{{ v.acceptance_rate ?? '–' }}</td>
                                <td class="px-3 py-3 text-xs text-zinc-500">{{ REVIEW_TYPES[v.review_type] ?? '–' }}</td>
                                <td class="px-3 py-3 text-xs text-zinc-500">{{ v.fee ?? '–' }}</td>
                                <td class="px-3 py-3 text-xs text-zinc-500">{{ v.word_limit ?? '–' }}</td>
                                <td class="px-3 py-3">
                                    <div class="flex items-center justify-end gap-0.5 opacity-50 transition select-none group-hover:opacity-100">
                                        <a v-if="v.website_url" :href="v.website_url" title="Website" :class="iconBtn"><FluxIcon name="arrow-up-right" variant="micro" /></a>
                                        <button type="button" :disabled="busy[v.id] || !!batch" :title="v.checked_at ? 'Checked ' + checked(v) : 'Update with AI'" :class="iconBtn" @click="single(v.id)"><FluxIcon name="sparkles" variant="micro" /></button>
                                        <button type="button" title="Edit" :class="iconBtn" @click="edit(v.id)"><FluxIcon name="pencil-square" variant="micro" /></button>
                                    </div>
                                </td>
                            </tr>
                        </UiList>
                    </table>
                </div>
                <div class="flex flex-wrap items-center justify-between gap-2 border-t border-zinc-100 px-4 py-2.5 text-[11px] text-zinc-400 select-none dark:border-zinc-800">
                    <span>Run an AI update to fill gaps from the publisher’s pages.</span>
                    <button type="button" class="hover:text-indigo-600" @click="restoreCatalog">Restore starter list</button>
                </div>
            </div>
            <FluxPagination :pager="pager" label="Journals and conferences" />
        </div>

        <VenueForm ref="form" @saved="saved" @deleted="() => { bus.emit('venues-changed'); load(); }" />
        <ReviewPanel ref="review" name="review-venues" :kinds="['venue']" @changed="load" />
    </div>
</template>
