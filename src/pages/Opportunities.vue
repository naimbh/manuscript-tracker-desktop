<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { provideErrors } from '@/flux/errors.js';
import { on } from '@/lib/events.js';
import { bus } from '@/lib/bus.js';
import { modal } from '@/lib/modal.js';
import { toast } from '@/lib/toast.js';
import { contextMenu } from '@/lib/native.js';
import { currentRoute, setQuery } from '@/lib/router.js';
import { diffForHumans, diffInMinutes, diffInSeconds, format, plural, ucfirst } from '@/lib/format.js';
import { openUrl, utc } from '@/components/shell/links.js';
import RunProgress from './venues/RunProgress.vue';
import { useRuns } from './venues/runs.js';
import { useResource } from '@/lib/useResource.js';
import { usePager } from '@/lib/usePager.js';

const SHOWS = { open: 'Open calls', saved: 'Saved', dismissed: 'Dismissed', history: 'History' };
const show = ref(SHOWS[currentRoute.query.show] ? currentRoute.query.show : 'open');
const listEl = ref(null);
const interests = ref('');
const errors = ref({});
provideErrors(errors);
const opened = reactive({});
// Stale-while-revalidate per tab: refreshes (scan results, `changed` events) swap data in place.
const res = useResource(() => 'opportunities:' + show.value, () => api('opportunities.list', { show: show.value }), {
  pageReady: true,
  onData: (v) => { if (v?.run_id) queueMicrotask(() => runs.track(v.run_id, { kind: 'scan' })); },
  onError: (e) => toast({ variant: 'danger', text: e.message }),
});
const d = res.data;
const load = res.refresh;

// Calls are paged across the week groups: flatten, page, then group the page again.
const calls = computed(() => (d.value?.weeks ?? []).flatMap((w) => w.items.map((o) => ({ o, w }))));
const pager = usePager(calls, { resetOn: [show], scrollTo: listEl });
const pageWeeks = computed(() => {
  const out = [];
  for (const { o, w } of pager.items.value) {
    const last = out[out.length - 1];
    if (last && last.week === w.week) last.items.push(o);
    else out.push({ week: w.week, label: w.label, count: w.count, items: [o] });
  }
  return out;
});
const scansPager = usePager(computed(() => d.value?.scans ?? []), { query: 'scans' });
const runsPager = usePager(computed(() => d.value?.runs ?? []), { query: 'jobs' });

function setShow(k) {
  show.value = k;
  setQuery({ show: k === 'open' ? null : k });
  load();
}

// ---- Scans (live) ----
let partialTimer = null;
const found = ref(0);
const runs = useRuns(['scan'], {
  onPartial(p) {
    if (!p.opportunity) return;
    found.value++;
    clearTimeout(partialTimer);
    partialTimer = setTimeout(load, 300);
  },
  onFinish(r) {
    const res = r.result ?? {};
    if (r.status === 'cancelled') toast({ text: 'Scan stopped.' });
    else if (r.status === 'done') toast({ variant: 'success', text: `Scan finished: ${res.found ?? found.value} new ${plural('call', res.found ?? found.value)}.` });
    found.value = 0;
    runs.dismiss(r.run_id);
    load();
    bus.emit('opportunities-changed');
  },
});
const scan = computed(() => runs.active.value[0] ?? null);
const running = computed(() => !!scan.value || !!d.value?.running);

async function scanNow() {
  if (running.value) return;
  try {
    const r = await api('opportunities.scan');
    if (!r.started) return;
    found.value = 0;
    runs.track(r.run, { kind: 'scan' });
    toast({ text: 'Scanning. You can keep working; you will be notified when it finishes.' });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

// ---- Actions ----
async function saveInterests() {
  try {
    const r = await submit(errors, () => api('opportunities.saveInterests', { interests: interests.value }));
    if (!r) return;
    modal('interests').close();
    toast({ variant: 'success', text: 'Interests saved. The next scan will use them.' });
    await load();
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

function openInterests() {
  interests.value = d.value?.interests ?? '';
  errors.value = {};
  modal('interests').show();
}

async function setStatus(id, status) {
  try {
    await api('opportunities.setStatus', { id, status });
    bus.emit('opportunities-changed');
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
  await load();
}

async function addDeadline(id) {
  try {
    const r = await api('opportunities.addDeadline', { id });
    if (r.added) toast({ variant: 'success', text: 'Added to your deadlines.' });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
  await load();
}

async function addVenue(id) {
  try {
    const r = await api('opportunities.addVenue', { id });
    toast({ variant: 'success', text: r.created ? 'Added to Journals & Conferences.' : 'This journal or conference is already in Journals & Conferences.' });
    bus.emit('venues-changed');
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

async function rowMenu(o) {
  const id = await contextMenu([
    { id: 'call', label: 'Open call' },
    ...(o.deadline_on ? [{ id: 'deadline', label: 'Add deadline' }] : []),
    { id: 'venue', label: 'Add to Journals & Conferences' },
    { separator: true },
    ...(o.status !== 'saved' ? [{ id: 'save', label: 'Save' }] : []),
    o.status !== 'dismissed' ? { id: 'dismiss', label: 'Dismiss' } : { id: 'restore', label: 'Restore' },
  ]);
  if (id === 'call') openUrl(o.url);
  if (id === 'deadline') addDeadline(o.id);
  if (id === 'venue') addVenue(o.id);
  if (id === 'save') setStatus(o.id, 'saved');
  if (id === 'dismiss') setStatus(o.id, 'dismissed');
  if (id === 'restore') setStatus(o.id, 'new');
}

const BADGE = { done: 'bg-emerald-50 text-emerald-700 dark:bg-emerald-500/10 dark:text-emerald-300', failed: 'bg-rose-50 text-rose-700 dark:bg-rose-500/10 dark:text-rose-300', running: 'bg-sky-50 text-sky-700 dark:bg-sky-500/10 dark:text-sky-300' };
const badge = (st) => BADGE[st] ?? 'bg-zinc-100 text-zinc-600';
function took(sc) {
  const s = diffInSeconds(utc(sc.created_at), utc(sc.finished_at));
  return s < 90 ? Math.trunc(s) + 's' : Math.round(diffInMinutes(utc(sc.created_at), utc(sc.finished_at)) * 10) / 10 + ' min';
}
const meta = (o) => [o.venue, o.publisher, o.event].filter(Boolean).join(' · ');
const toggle = (k) => { opened[k] = !opened[k]; };

const offs = [];
onMounted(() => {
  load();
  offs.push(on('changed', ({ area } = {}) => { if (['opportunities', 'deadlines', 'backup', 'profile', 'external'].includes(area)) load(); }));
  offs.push(on('ai-runs', () => { if (show.value === 'history') load(); }));
});
onBeforeUnmount(() => { clearTimeout(partialTimer); offs.forEach((off) => off()); });
</script>

<template>
    <div class="page-in mx-auto w-full max-w-6xl space-y-6 2xl:max-w-[1440px]">
        <section class="hero relative overflow-hidden rounded-2xl border border-zinc-200/80 px-8 py-6 dark:border-zinc-800">
            <div class="hero-grid pointer-events-none absolute inset-0 text-zinc-900 dark:text-white"></div>
            <div class="relative flex flex-row flex-wrap items-end justify-between gap-4">
                <div class="max-w-xl">
                    <p class="eyebrow">Calls for Papers</p>
                    <h1 class="mt-1 text-2xl font-semibold tracking-tight">Calls that fit your work</h1>
                    <p class="mt-1 text-sm text-zinc-600 dark:text-zinc-400">Every day, the AI reads new calls from WikiCFP and the publishers’ call pages, keeps only those with proceedings or a journal behind them, and ranks them against your interests.</p>
                    <button type="button" class="mt-3 inline-flex max-w-full items-center gap-1.5 truncate rounded-full bg-white/70 px-3 py-1 text-xs text-zinc-600 ring-1 ring-zinc-200 select-none hover:ring-indigo-300 dark:bg-zinc-900/60 dark:text-zinc-300 dark:ring-zinc-700" @click="openInterests">
                        <FluxIcon name="adjustments-horizontal" variant="micro" class="size-3.5 shrink-0" /> <span class="truncate">{{ d?.interests }}</span>
                    </button>
                </div>
                <div class="flex flex-col items-end gap-2 select-none">
                    <div class="flex gap-2">
                        <FluxButton v-if="scan" variant="danger" icon="stop" @click="runs.cancel(scan.run_id)">Stop</FluxButton>
                        <FluxButton variant="primary" :icon="running ? 'arrow-path' : 'magnifying-glass-circle'" :disabled="running" :class="running && '[&_svg]:animate-spin'" @click="scanNow">
                            {{ running ? 'Scanning…' : 'Scan now' }}
                        </FluxButton>
                    </div>
                    <span v-if="d" class="text-[11px] text-zinc-500">
                        <template v-if="!d.aiReady">{{ d.aiStatus }}</template>
                        <template v-else-if="d.last">Last scan {{ diffForHumans(utc(d.last.created_at)) }} · {{ d.last.status === 'failed' ? 'failed' : (d.last.status === 'running' ? 'in progress' : d.last.found + ' new') }}</template>
                        <template v-else>Daily at {{ d.scan_time }}</template>
                    </span>
                </div>
            </div>
            <UiCollapse>
            <RunProgress v-if="scan" :run="scan" text="Scanning for calls…" :show-results="3">
                <template #result="{ r }">
                    <template v-if="r.opportunity">
                        <FluxIcon name="sparkles" variant="micro" class="size-3.5 shrink-0 text-indigo-500" />
                        <span class="truncate">{{ r.opportunity.title }}</span>
                        <span v-if="r.opportunity.fit" class="num shrink-0 text-zinc-400">· fit {{ r.opportunity.fit }}/5</span>
                    </template>
                </template>
            </RunProgress>
            <div v-else-if="running" key="busy" class="relative mt-5 overflow-hidden rounded-full"><div class="clock-track !h-1.5"><div class="h-full w-1/3 animate-[scan_1.6s_ease-in-out_infinite] rounded-full bg-gradient-to-r from-indigo-500 to-fuchsia-500"></div></div></div>
            </UiCollapse>
        </section>

        <UiCollapse><FluxCallout v-if="d?.last?.status === 'failed' && d.last.error !== 'Cancelled.'" variant="warning" icon="exclamation-triangle" heading="The last scan did not finish">
            <FluxCalloutText>{{ d.last.error }}</FluxCalloutText>
        </FluxCallout></UiCollapse>

        <div class="flex w-fit items-center gap-1 rounded-lg bg-zinc-100 p-0.5 text-sm font-medium select-none dark:bg-zinc-800">
            <button v-for="(l, k) in SHOWS" :key="k" type="button" class="rounded-md px-3 py-1.5 transition" :class="show === k ? 'bg-white text-zinc-900 shadow-xs dark:bg-zinc-700 dark:text-white' : 'text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200'" @click="setShow(k)">{{ l }}</button>
        </div>

        <template v-if="d && show === 'history'">
            <section class="space-y-3">
                <h2 class="font-semibold select-none">Scans</h2>
                <UiList class="panel divide-y divide-zinc-100 dark:divide-zinc-800">
                    <div v-for="sc in scansPager.items.value" :key="'sc' + sc.id" class="px-4 py-3">
                        <button type="button" class="flex w-full flex-wrap items-center gap-x-3 gap-y-1 text-left text-sm select-none" @click="toggle('sc' + sc.id)">
                            <span class="rounded-md px-1.5 py-0.5 text-[11px] font-semibold" :class="badge(sc.status)">{{ ucfirst(sc.status) }}</span>
                            <span class="font-medium">{{ format(utc(sc.created_at), 'j M Y, H:i') }}</span>
                            <span class="text-xs text-zinc-500">{{ sc.trigger === 'daily' ? 'Daily' : 'Manual' }}</span>
                            <span class="text-xs text-zinc-500">{{ sc.candidates }} candidates · {{ sc.found }} new</span>
                            <span v-if="sc.finished_at" class="num text-xs text-zinc-400">{{ took(sc) }}</span>
                            <span v-if="sc.provider" class="text-xs text-zinc-400">· {{ sc.provider }}</span>
                            <FluxIcon name="chevron-down" variant="micro" class="ml-auto size-4 text-zinc-400 transition" :class="opened['sc' + sc.id] && 'rotate-180'" />
                        </button>
                        <p v-if="sc.error" class="mt-2 rounded-lg bg-rose-50 px-3 py-2 text-xs text-rose-700 dark:bg-rose-500/10 dark:text-rose-300">{{ sc.error }}</p>
                        <pre v-show="opened['sc' + sc.id]" class="mt-2 max-h-64 overflow-auto rounded-lg bg-zinc-50 p-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-zinc-600 dark:bg-zinc-900 dark:text-zinc-400">{{ sc.log || 'No log recorded for this scan.' }}</pre>
                    </div>
                    <p v-if="!d.scans.length" key="none" class="px-4 py-6 text-center text-sm text-zinc-500">No scans yet.</p>
                </UiList>
                <FluxPagination :pager="scansPager" label="Scans" />

                <h2 class="pt-4 font-semibold select-none">All AI jobs</h2>
                <UiList class="panel divide-y divide-zinc-100 dark:divide-zinc-800">
                    <div v-for="r in runsPager.items.value" :key="'rn' + r.id" class="px-4 py-3">
                        <button type="button" class="flex w-full flex-wrap items-center gap-x-3 gap-y-1 text-left text-sm select-none" @click="toggle('rn' + r.id)">
                            <span class="rounded-md px-1.5 py-0.5 text-[11px] font-semibold" :class="badge(r.status)">{{ ucfirst(r.status) }}</span>
                            <span class="font-medium">{{ r.label }}</span>
                            <span class="text-xs text-zinc-500">{{ format(utc(r.created_at), 'j M Y, H:i') }}</span>
                            <span v-if="r.total > 1" class="num text-xs text-zinc-500">{{ r.done }}/{{ r.total }}</span>
                            <span v-if="r.provider" class="text-xs text-zinc-400">· {{ r.provider }}</span>
                            <span v-if="r.input_tokens || r.output_tokens" class="num text-xs text-zinc-400">· {{ (r.input_tokens + r.output_tokens).toLocaleString() }} tokens</span>
                            <span v-if="r.duration_ms" class="num text-xs text-zinc-400">· {{ r.duration_ms < 90000 ? Math.round(r.duration_ms / 1000) + 's' : Math.round(r.duration_ms / 6000) / 10 + ' min' }}</span>
                            <FluxIcon name="chevron-down" variant="micro" class="ml-auto size-4 text-zinc-400 transition" :class="opened['rn' + r.id] && 'rotate-180'" />
                        </button>
                        <p v-if="r.error" class="mt-2 rounded-lg bg-rose-50 px-3 py-2 text-xs text-rose-700 dark:bg-rose-500/10 dark:text-rose-300">{{ r.error }}</p>
                        <pre v-show="opened['rn' + r.id]" class="mt-2 max-h-64 overflow-auto rounded-lg bg-zinc-50 p-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-zinc-600 dark:bg-zinc-900 dark:text-zinc-400">{{ r.log || 'No log recorded.' }}</pre>
                    </div>
                    <p v-if="!d.runs.length" key="none" class="px-4 py-6 text-center text-sm text-zinc-500">No AI jobs yet.</p>
                </UiList>
                <FluxPagination :pager="runsPager" label="AI jobs" />
            </section>
        </template>

        <div v-else-if="!d" class="grid gap-3" aria-hidden="true"><div v-for="i in 3" :key="i" class="panel h-36 animate-pulse bg-zinc-100/60 dark:bg-zinc-800/40"></div></div>

        <div v-else ref="listEl" class="scroll-mt-4 space-y-6">
            <section v-for="w in pageWeeks" :key="'w' + w.week">
                <div class="mb-3 flex items-baseline gap-3 select-none">
                    <h2 class="font-semibold">{{ w.label }}</h2>
                    <span class="num text-xs text-zinc-400">{{ w.count }} {{ plural('call', w.count) }}</span>
                </div>
                <UiList class="stagger grid gap-3">
                    <article v-for="o in w.items" :key="'o' + o.id" class="panel lift flex gap-4 p-4" @contextmenu.prevent="rowMenu(o)">
                        <div class="flex w-14 shrink-0 flex-col items-center gap-1.5 pt-0.5 select-none">
                            <div class="flex gap-0.5" :aria-label="`Fit ${o.fit} of 5`">
                                <span v-for="i in 5" :key="i" class="h-4 w-1.5 rounded-full" :class="i <= o.fit ? 'bg-gradient-to-t from-indigo-500 to-fuchsia-500' : 'bg-zinc-200 dark:bg-zinc-700'"></span>
                            </div>
                            <span class="text-[10px] font-medium text-zinc-400 uppercase">fit</span>
                            <span v-if="o.days !== null" class="num mt-1 rounded-md px-1.5 py-0.5 text-[11px] font-semibold" :class="o.days <= 14 ? 'bg-rose-50 text-rose-600 dark:bg-rose-500/10' : 'bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300'">{{ o.days }}d</span>
                        </div>
                        <div class="min-w-0 flex-1">
                            <div class="flex flex-wrap items-center gap-2 select-none">
                                <span class="rounded-md bg-indigo-50 px-1.5 py-0.5 text-[11px] font-medium text-indigo-700 dark:bg-indigo-500/10 dark:text-indigo-300">{{ o.kind_label ?? o.kind }}</span>
                                <span v-if="o.is_new" class="rounded-md bg-emerald-500 px-1.5 py-0.5 text-[10px] font-bold tracking-wide text-white uppercase">new</span>
                                <span v-if="o.outlet" class="text-[11px] text-zinc-500">{{ o.outlet }}</span>
                            </div>
                            <h3 class="mt-1.5 leading-snug font-semibold"><a :href="o.url" class="hover:text-indigo-600">{{ o.title }}</a></h3>
                            <p class="text-xs text-zinc-500">{{ meta(o) }}</p>
                            <p v-if="o.summary" class="mt-2 text-sm text-zinc-600 dark:text-zinc-300">{{ o.summary }}</p>
                            <p v-if="o.why" class="mt-1.5 flex gap-1.5 text-sm text-indigo-700 dark:text-indigo-300"><FluxIcon name="sparkles" variant="micro" class="mt-0.5 size-3.5 shrink-0" />{{ o.why }}</p>
                            <div class="mt-3 flex flex-wrap items-center gap-2 text-xs select-none">
                                <span v-if="o.deadline_on" class="text-zinc-500">Deadline {{ format(o.deadline_on, 'j M Y') }}</span>
                                <span class="flex-1"></span>
                                <FluxButton size="xs" variant="ghost" icon="arrow-up-right" :href="o.url">Call</FluxButton>
                                <FluxButton v-if="o.deadline_on" size="xs" variant="ghost" icon="calendar-days" @click="addDeadline(o.id)">Add deadline</FluxButton>
                                <FluxButton size="xs" variant="ghost" icon="building-library" @click="addVenue(o.id)">Add journal or conference</FluxButton>
                                <FluxButton v-if="o.status !== 'saved'" size="xs" variant="ghost" icon="bookmark" @click="setStatus(o.id, 'saved')">Save</FluxButton>
                                <FluxButton v-if="o.status !== 'dismissed'" size="xs" variant="ghost" icon="x-mark" @click="setStatus(o.id, 'dismissed')">Dismiss</FluxButton>
                                <FluxButton v-else size="xs" variant="ghost" icon="arrow-uturn-left" @click="setStatus(o.id, 'new')">Restore</FluxButton>
                            </div>
                        </div>
                    </article>
                </UiList>
            </section>
            <FluxPagination :pager="pager" label="Calls" />
            <div v-if="!d.weeks.length" class="panel">
                <UiEmpty :title="show === 'open' ? 'No open calls yet' : 'Nothing here'" :text="show === 'open' ? 'Run a scan now, or wait for the daily scan at ' + d.scan_time + '.' : ''">
                    <FluxButton v-if="show === 'open' && !running" variant="primary" @click="scanNow">Scan now</FluxButton>
                </UiEmpty>
            </div>
        </div>

        <FluxModal name="interests" class="w-full max-w-lg">
            <form class="space-y-4" @submit.prevent="saveInterests">
                <FluxHeading size="lg">Your interests</FluxHeading>
                <FluxText>Separate topics with commas. The first six are also used to search WikiCFP.</FluxText>
                <FluxTextarea v-model="interests" name="interests" rows="4" placeholder="platform labor, gig economy, ICT4D, generative AI and work, CSCW" />
                <FluxError name="interests" />
                <div class="flex justify-end gap-2">
                    <FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose>
                    <FluxButton type="submit" variant="primary">Save</FluxButton>
                </div>
            </form>
        </FluxModal>
    </div>
</template>
