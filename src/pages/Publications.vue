<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { provideErrors } from '@/flux/errors.js';
import { on } from '@/lib/events.js';
import { modal } from '@/lib/modal.js';
import { toast } from '@/lib/toast.js';
import { contextMenu } from '@/lib/native.js';
import { diffForHumans, limit } from '@/lib/format.js';
import { openUrl, utc } from '@/components/shell/links.js';
import { useResource } from '@/lib/useResource.js';
import { usePager } from '@/lib/usePager.js';
import { useDensity } from '@/lib/density.js';
import PubForm from './publications/PubForm.vue';
import ReviewPanel from './venues/ReviewPanel.vue';
import RunProgress from './venues/RunProgress.vue';
import { SCHOLAR_KIND, reviewText, runText, useRuns } from './venues/runs.js';

const sort = ref('year');
const form = ref(null);
const review = ref(null);
const newOnScholar = ref([]);
const listEl = ref(null);
const dense = useDensity();

// Stale-while-revalidate: the list stays on screen while it refreshes; bursts of `changed` events become one fetch.
const res = useResource('publications', () => api('publications.list', { sort: sort.value }), {
  pageReady: true,
  onError: (e) => toast({ variant: 'danger', text: e.message }),
});
const d = res.data;
const load = res.refresh;

function setSort(s) {
  if (sort.value === s) return;
  sort.value = s;
  load();
}

const pubs = computed(() => d.value?.pubs ?? []);
const stats = computed(() => d.value?.stats ?? {});
const all = computed(() => stats.value.all ?? { citations: d.value?.total, h: d.value?.h, i10: d.value?.i10 });
const recent = computed(() => stats.value.recent ?? null);
const years = computed(() => Object.entries(stats.value.by_year ?? {}));
const ymax = computed(() => Math.max(1, ...years.value.map(([, c]) => Number(c) || 0)));
const top = computed(() => {
  const step = ymax.value > 50 ? 30 : ymax.value > 20 ? 10 : 5;
  return Math.ceil(ymax.value / step) * step;
});
const pager = usePager(pubs, { resetOn: [sort], scrollTo: listEl });
/** "Authors · Venue · Year" for the compact row. */
const byline = (p) => [p.authors, p.citation || p.publisher, p.year].filter(Boolean).join(' · ');

// ---- Scholar profile ----
const scholarUrl = ref('');
const errors = ref({});
provideErrors(errors);
function openScholar() {
  scholarUrl.value = d.value?.scholar ?? '';
  errors.value = {};
  modal('scholar').show();
}
async function saveScholar() {
  try {
    const r = await submit(errors, () => api('research.saveScholar', { scholarUrl: scholarUrl.value }));
    if (!r) return;
    modal('scholar').close();
    toast({ variant: 'success', text: 'Scholar profile saved.' });
    await load();
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

// ---- Research runs ----
const runs = useRuns(['citations', SCHOLAR_KIND], {
  onFinish(r) {
    const res = r.result ?? {};
    if (r.status === 'cancelled') toast({ text: 'Update stopped.' });
    else if (r.status === 'failed') toast({ variant: 'danger', heading: 'Update failed', text: res.error || 'The update did not finish.' });
    else if ('source' in res) {
      const extra = res.pending ? ` ${reviewText(res.pending)}.` : '';
      if (res.source === 'scholar') {
        const news = res.new ?? [];
        toast({ variant: 'success', heading: 'Citations updated', text: `${res.updated} articles updated from Google Scholar.` + (news.length ? ' Not in your list yet: ' + news.slice(0, 3).join('; ') + '.' : '') + extra });
        newOnScholar.value = news;
      } else if (res.source === 'ai') {
        toast({ variant: 'warning', heading: 'Citations updated', text: `Google Scholar blocked the app, so the AI read it instead. ${res.pending} figures to review.` });
      } else {
        toast({ variant: 'warning', heading: 'Citations updated', text: 'Google Scholar blocked the app, so statistics from OpenAlex are ready to review.' });
      }
    } else if ('changed' in res) {
      const failed = res.failed ?? 0;
      toast({ variant: failed ? 'warning' : 'success', text: res.changed + ' counts changed' + (failed ? ', ' + failed + ' failed.' : '.') + (res.pending_proposals ? ` ${reviewText(res.pending_proposals)}.` : '') });
    } else if (res.ok) {
      const more = res.pending?.length ? ` ${reviewText(res.pending.length)}.` : '';
      toast({ variant: 'success', text: (res.after === null || res.after === undefined ? 'Details updated; Scholar count not readable.' : 'Cited by ' + res.after + (res.before !== null && res.after !== res.before ? ' (was ' + res.before + ')' : '')) + more });
    } else if (res.publication_id) {
      toast({ variant: 'danger', heading: 'Update failed', text: res.error || 'The publication could not be checked.' });
    }
    runs.dismiss(r.run_id);
    load();
  },
});
const batch = computed(() => runs.active.value.find((r) => r.keep || r.total > 1 || !r.publication_id) ?? null);
const isScholar = (r) => r?.kind === SCHOLAR_KIND;
const busy = computed(() => Object.fromEntries(runs.active.value.filter((r) => r.publication_id).map((r) => [r.publication_id, true])));

async function single(id) {
  if (busy.value[id]) return;
  try {
    const { run } = await api('publications.refreshCitations', { id });
    runs.track(run, { publication_id: id });
  } catch (e) {
    toast({ variant: 'danger', heading: 'Update failed', text: e.message });
  }
}

async function refreshProfile() {
  if (!d.value?.scholar) { openScholar(); return; }
  try {
    const r = await api('research.refreshScholar');
    runs.track(r.run, { kind: SCHOLAR_KIND, keep: true });
  } catch (e) {
    toast({ variant: 'danger', heading: 'Update failed', text: e.message });
  }
}

async function refreshAll() {
  try {
    const r = await api('publications.refreshAll');
    runs.track(r.run, { keep: true, total: r.total });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

async function importNew() {
  try {
    const { added } = await api('research.importPublications', { titles: newOnScholar.value });
    newOnScholar.value = [];
    toast({ variant: 'success', text: `Added ${added} ${added === 1 ? 'publication' : 'publications'} from Google Scholar.` });
    await load();
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

function resultText(p) {
  const x = p.publication;
  if (!x) return '';
  if (!x.ok) return `${limit(x.title ?? '', 60)}: ${x.error}`;
  const change = x.after !== null && x.after !== undefined && x.after !== x.before ? `cited by ${x.after}` + (x.before !== null && x.before !== undefined ? ` (was ${x.before})` : '') : x.found ? 'no change' : 'not found in the indexes';
  return `${limit(x.title ?? '', 60)}: ${change}` + (x.pending?.length ? `, ${reviewText(x.pending.length)}` : '');
}

async function rowMenu(p) {
  const link = p.scholar_url || p.url;
  const id = await contextMenu([
    ...(link ? [{ id: 'open', label: 'Open' }] : []),
    ...(p.doi ? [{ id: 'doi', label: 'Open DOI' }] : []),
    { id: 'update', label: 'Update', enabled: !busy.value[p.id] && !batch.value },
    ...(p.pending ? [{ id: 'review', label: `Review ${reviewText(p.pending)}` }] : []),
    { separator: true },
    { id: 'edit', label: 'Edit' },
    { id: 'delete', label: 'Remove…' },
  ]);
  if (id === 'open') openUrl(link);
  if (id === 'doi') openUrl('https://doi.org/' + p.doi);
  if (id === 'update') single(p.id);
  if (id === 'review') review.value?.open(p.id);
  if (id === 'edit') form.value?.open(p);
  if (id === 'delete') form.value?.remove(p.id);
}

const offs = [];
onMounted(() => {
  load();
  offs.push(on('changed', ({ area } = {}) => { if (['publications', 'profile', 'proposals', 'backup', 'external'].includes(area)) load(); }));
  offs.push(on('user', load));
});
onBeforeUnmount(() => offs.forEach((off) => off()));
</script>

<template>
    <div class="page-in @container mx-auto w-full max-w-6xl space-y-6 2xl:max-w-[1440px]">
        <section class="hero relative overflow-hidden rounded-2xl border border-zinc-200/80 px-8 py-6 dark:border-zinc-800">
            <div class="hero-grid pointer-events-none absolute inset-0 text-zinc-900 dark:text-white"></div>
            <div class="relative flex flex-row flex-wrap items-end justify-between gap-4">
                <div>
                    <p class="eyebrow">Published</p>
                    <h1 class="mt-1 text-2xl font-semibold tracking-tight">Your published work</h1>
                    <p class="mt-1 text-sm text-zinc-600 dark:text-zinc-400">
                        Citation counts from Google Scholar.
                        <template v-if="d?.lastCheck">Last checked {{ diffForHumans(utc(d.lastCheck)) }}. </template>
                        {{ d?.schedule }}
                        <a v-if="d?.scholar" :href="d.scholar" class="font-medium text-indigo-600 hover:underline">Open Scholar profile ↗</a>
                    </p>
                </div>
                <div class="flex flex-wrap gap-2 select-none">
                    <FluxButton v-if="d && !d.scholar" icon="academic-cap" @click="openScholar">Connect Google Scholar</FluxButton>
                    <FluxButton icon="plus" @click="form?.open(null, d?.userName ?? '')">Add</FluxButton>
                    <template v-if="!batch">
                        <FluxButton v-if="pubs.length" icon="arrow-path" :title="'Check every publication in OpenAlex, Crossref and Semantic Scholar'" @click="refreshAll">Update all</FluxButton>
                        <FluxButton variant="primary" icon="sparkles" :title="d?.aiStatus" @click="refreshProfile">Update from Scholar</FluxButton>
                    </template>
                    <FluxButton v-else variant="danger" icon="stop" @click="runs.cancel(batch.run_id)">Stop</FluxButton>
                </div>
            </div>
            <UiCollapse>
                <RunProgress v-if="batch" :run="batch" :text="isScholar(batch) ? runText(batch) : 'Checking citations…'">
                    <template #result="{ r }">
                        <FluxIcon :name="r.publication?.ok ? 'check-circle' : 'exclamation-triangle'" variant="micro" class="size-3.5 shrink-0" :class="r.publication?.ok ? 'text-emerald-500' : 'text-amber-500'" />
                        <span class="truncate">{{ resultText(r) }}</span>
                    </template>
                </RunProgress>
            </UiCollapse>
        </section>

        <UiCollapse><FluxCallout v-if="d?.pending" icon="sparkles" color="indigo">
            <FluxCalloutHeading>{{ reviewText(d.pending) }}</FluxCalloutHeading>
            <FluxCalloutText>Updates that would replace what you entered, or that could not be verified, wait for your OK.</FluxCalloutText>
            <template #actions><FluxButton size="sm" variant="primary" @click="review?.open()">Review</FluxButton></template>
        </FluxCallout></UiCollapse>

        <UiCollapse><FluxCallout v-if="newOnScholar.length" icon="academic-cap">
            <FluxCalloutHeading>Not in your list yet</FluxCalloutHeading>
            <FluxCalloutText>{{ newOnScholar.slice(0, 5).join('; ') }}{{ newOnScholar.length > 5 ? ` and ${newOnScholar.length - 5} more` : '' }}.</FluxCalloutText>
            <template #actions>
                <FluxButton size="sm" variant="ghost" @click="newOnScholar = []">Not now</FluxButton>
                <FluxButton size="sm" variant="primary" @click="importNew">Add to Published</FluxButton>
            </template>
        </FluxCallout></UiCollapse>

        <div v-if="!d" class="grid gap-6 @3xl:grid-cols-[minmax(0,1fr)_18rem]" aria-hidden="true">
            <div class="panel h-96 animate-pulse bg-zinc-100/60 dark:bg-zinc-800/40"></div>
            <div class="panel h-72 animate-pulse bg-zinc-100/60 dark:bg-zinc-800/40"></div>
        </div>
        <div v-else class="grid gap-6 @3xl:grid-cols-[minmax(0,1fr)_18rem]">
            <!-- Article list, Scholar style: Comfortable by default, Compact on request (one-line title, muted byline, citations on the right) -->
            <div ref="listEl" class="min-w-0 scroll-mt-4">
                <div class="panel overflow-hidden">
                    <div class="grid grid-cols-[minmax(0,1fr)_4.5rem_3.5rem] items-center gap-x-2 border-b border-zinc-200 bg-zinc-50 px-4 py-2 text-[11px] font-semibold tracking-wide text-zinc-500 uppercase select-none dark:border-zinc-800 dark:bg-zinc-900">
                        <span class="flex items-center gap-3">
                            <button type="button" class="text-left uppercase" :class="sort === 'year' && 'text-indigo-600 dark:text-indigo-400'" @click="setSort('year')">Title</button>
                            <span class="ml-auto flex rounded-md bg-zinc-200/60 p-0.5 font-medium tracking-normal normal-case dark:bg-zinc-800">
                                <button v-for="(l, k) in { comfortable: 'Comfortable', compact: 'Compact' }" :key="k" type="button" class="rounded px-1.5 py-0.5 text-[10px] transition" :class="(k === 'compact') === dense ? 'bg-white text-zinc-800 shadow-xs dark:bg-zinc-700 dark:text-white' : 'text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200'" :aria-pressed="(k === 'compact') === dense" @click="dense = k === 'compact'">{{ l }}</button>
                            </span>
                        </span>
                        <button type="button" class="text-right uppercase" :class="sort === 'cites' && 'text-indigo-600 dark:text-indigo-400'" @click="setSort('cites')">Cited by</button>
                        <button type="button" class="text-right uppercase" :class="sort === 'year' && 'text-indigo-600 dark:text-indigo-400'" @click="setSort('year')">Year</button>
                    </div>
                    <UiList class="stagger divide-y divide-zinc-100 dark:divide-zinc-800">
                        <div v-for="p in pager.items.value" :key="'pub' + p.id" class="group grid grid-cols-[minmax(0,1fr)_4.5rem_3.5rem] items-center gap-x-2 px-4 transition-colors hover:bg-zinc-50/80 dark:hover:bg-zinc-800/30" :class="[dense ? 'min-h-12 py-1.5' : 'py-3', busy[p.id] && 'bg-indigo-50/60 dark:bg-indigo-500/5']" @contextmenu.prevent="rowMenu(p)">
                            <div class="min-w-0">
                                <div class="flex min-w-0 items-baseline gap-2">
                                    <a :href="p.scholar_url || p.url || '#'" :title="p.title" class="text-[13.5px] leading-snug font-medium text-indigo-700 hover:underline dark:text-indigo-300" :class="dense ? 'truncate' : 'line-clamp-2'" @click="!(p.scholar_url || p.url) && $event.preventDefault()">{{ p.title }}</a>
                                    <span v-if="busy[p.id]" class="flex shrink-0 items-center gap-1 text-[11px] text-indigo-600"><FluxIcon name="arrow-path" variant="micro" class="size-3 animate-spin" />Checking…</span>
                                    <button v-if="p.pending" type="button" class="shrink-0 text-[11px] font-medium text-amber-600 hover:underline" @click="review?.open(p.id)">{{ reviewText(p.pending) }}</button>
                                </div>
                                <p class="truncate text-xs text-zinc-500" :class="!dense && 'mt-0.5'" :title="byline(p)">{{ byline(p) }}</p>
                                <!-- Comfortable: the row's links, quiet until hovered. -->
                                <div v-if="!dense" class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] opacity-0 transition-opacity select-none group-hover:opacity-100 focus-within:opacity-100">
                                    <span class="text-zinc-400">{{ p.kind_label }}</span>
                                    <a v-if="p.cites_url" :href="p.cites_url" class="text-zinc-500 hover:text-indigo-600">Citing papers</a>
                                    <a v-if="p.doi" :href="'https://doi.org/' + p.doi" class="text-zinc-500 hover:text-indigo-600">DOI</a>
                                    <a v-if="p.url" :href="p.url" class="text-zinc-500 hover:text-indigo-600">Publisher</a>
                                    <button type="button" class="text-zinc-500 hover:text-indigo-600 disabled:opacity-40" :disabled="busy[p.id] || !!batch" @click="single(p.id)">Update</button>
                                    <button type="button" class="text-zinc-500 hover:text-indigo-600" @click="form?.open(p)">Edit</button>
                                </div>
                            </div>
                            <div class="num text-right">
                                <a v-if="p.citations" :href="p.cites_url || '#'" class="text-[13px] font-medium text-indigo-700 hover:underline dark:text-indigo-300" @click="!p.cites_url && $event.preventDefault()">{{ p.citations }}</a>
                            </div>
                            <div class="num text-right text-[13px] text-zinc-600 dark:text-zinc-300">{{ p.year }}</div>
                        </div>
                    </UiList>
                    <UiEmpty v-if="!pubs.length" title="Nothing published yet" text="Add your published work, and its citations will be kept up to date."><FluxButton @click="form?.open(null, d?.userName ?? '')">Add publication</FluxButton></UiEmpty>
                    <div v-if="pubs.length" class="border-t border-zinc-100 px-4 py-2.5 text-xs text-zinc-500 select-none dark:border-zinc-800">
                        <FluxPagination v-if="pager.visible.value" :pager="pager" label="Articles" />
                        <p v-else class="text-center">Articles 1–{{ pubs.length }}</p>
                    </div>
                </div>
            </div>

            <!-- Cited by box -->
            <aside class="space-y-4">
                <div class="panel p-4">
                    <div class="flex items-center justify-between select-none">
                        <h2 class="font-semibold">Cited by</h2>
                        <button type="button" class="text-xs text-zinc-500 hover:text-indigo-600" @click="openScholar">{{ d.scholar ? 'Profile' : 'Add profile' }}</button>
                    </div>
                    <table class="num mt-3 w-full text-sm">
                        <thead><tr class="border-b border-zinc-200 text-xs text-zinc-500 dark:border-zinc-700"><th class="pb-1.5 text-left font-medium"></th><th class="pb-1.5 text-right font-medium">All</th><th v-if="recent" class="pb-1.5 text-right font-medium">Since {{ stats.since }}</th></tr></thead>
                        <tbody>
                            <tr v-for="[k, l] in [['citations', 'Citations'], ['h', 'h-index'], ['i10', 'i10-index']]" :key="k"><td class="py-1 text-zinc-600 dark:text-zinc-300">{{ l }}</td><td class="py-1 text-right font-medium">{{ all[k] ?? '–' }}</td><td v-if="recent" class="py-1 text-right font-medium">{{ recent[k] ?? '–' }}</td></tr>
                        </tbody>
                    </table>

                    <template v-if="years.length">
                        <div class="relative mt-5 h-40 pr-7 select-none">
                            <div v-for="g in [0, 1, 2]" :key="g" class="absolute right-0 left-0 border-t border-zinc-100 dark:border-zinc-800" :style="`top: ${g * 50}%`"><span class="num absolute -top-2 right-0 bg-white pl-1 text-[10px] text-zinc-400 dark:bg-zinc-900">{{ Math.trunc(top - g * (top / 2)) }}</span></div>
                            <div class="absolute inset-0 right-7 flex items-end justify-end gap-2 border-b border-zinc-200 dark:border-zinc-700">
                                <div v-for="([y, c], i) in years" :key="y" class="group relative flex w-7 flex-col items-center justify-end" style="height: 100%">
                                    <span class="num pointer-events-none absolute -top-5 hidden rounded bg-zinc-900 px-1 text-[10px] text-white group-hover:block dark:bg-white dark:text-zinc-900">{{ c }}</span>
                                    <div class="grow-y w-full origin-bottom rounded-t bg-gradient-to-t from-zinc-500 to-zinc-400 transition-[height,background-color] duration-300 ease-out group-hover:from-indigo-600 group-hover:to-fuchsia-500 dark:from-zinc-500 dark:to-zinc-400" :style="`height: ${Math.max(1, (c / top) * 100)}%; --d: ${i * 0.08}s`"></div>
                                </div>
                            </div>
                        </div>
                        <div class="flex justify-end gap-2 pt-1 pr-7 select-none"><span v-for="[y] in years" :key="y" class="num w-7 text-center text-[10px] text-zinc-500">{{ y }}</span></div>
                    </template>

                    <div class="mt-4 flex items-center justify-between border-t border-zinc-100 pt-3 text-[11px] text-zinc-400 select-none dark:border-zinc-800">
                        <span>{{ d.statsChecked ? 'From Scholar ' + diffForHumans(utc(d.statsChecked)) : (stats.note ?? 'Not read from Scholar yet') }}</span>
                        <button type="button" class="font-medium text-indigo-600 hover:underline disabled:text-zinc-400 disabled:no-underline" :disabled="!d.scholar || !!batch" @click="refreshProfile">{{ isScholar(batch) ? 'Reading…' : 'Refresh' }}</button>
                    </div>
                </div>
            </aside>
        </div>

        <FluxModal name="scholar" class="w-full max-w-lg">
            <form class="space-y-4" @submit.prevent="saveScholar">
                <FluxHeading size="lg">Google Scholar profile</FluxHeading>
                <FluxText>The weekly update reads this page for your citation table, the per-year chart and each article’s count.</FluxText>
                <FluxInput v-model="scholarUrl" name="scholarUrl" placeholder="https://scholar.google.com/citations?user=…" />
                <FluxError name="scholarUrl" />
                <div class="flex justify-end gap-2"><FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose><FluxButton type="submit" variant="primary">Save</FluxButton></div>
            </form>
        </FluxModal>

        <PubForm ref="form" @saved="load" @deleted="load" />
        <ReviewPanel ref="review" name="review-publications" :kinds="['publication', 'profile']" @changed="load" />
    </div>
</template>
