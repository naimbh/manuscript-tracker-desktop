<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { api } from '@/lib/api.js';
import { on } from '@/lib/events.js';
import { bus } from '@/lib/bus.js';
import { toast } from '@/lib/toast.js';
import { contextMenu } from '@/lib/native.js';
import { navigate } from '@/lib/router.js';
import { user } from '@/lib/site.js';
import { before, diffForHumans, format, now, plural } from '@/lib/format.js';
import { useResource } from '@/lib/useResource.js';

const busy = ref(null);

// Stale-while-revalidate: revisits show the last desk at once; refreshes swap data in place.
const res = useResource('desk', () => api('desk.data'), {
  pageReady: true,
  onData: (v) => bus.emit('desk-attention', v?.attention?.length ?? 0),
  onError: (e) => toast({ variant: 'danger', text: e.message }),
});
const d = res.data;
const load = res.refresh;
async function dismiss(a) {
  try {
    await api('app.dismissNotice', { key: a.key });
    await load();
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

const today = now();
const greeting = computed(() => (today.getHours() < 12 ? 'morning' : today.getHours() < 18 ? 'afternoon' : 'evening'));
const firstName = computed(() => before(user.name || '', ' '));
const attention = computed(() => d.value?.attention ?? []);

const TONES = {
  red: 'bg-rose-500/10 text-rose-600 dark:text-rose-400',
  amber: 'bg-amber-500/10 text-amber-600 dark:text-amber-400',
  indigo: 'bg-indigo-500/10 text-indigo-600 dark:text-indigo-400',
  violet: 'bg-gradient-to-br from-violet-600 to-fuchsia-600 text-white shadow-sm shadow-violet-500/30',
};
const BARS = {
  blue: 'bg-gradient-to-r from-sky-400 to-sky-500',
  amber: 'bg-gradient-to-r from-amber-400 to-amber-500',
  red: 'bg-gradient-to-r from-rose-400 to-rose-500',
  green: 'bg-gradient-to-r from-emerald-400 to-emerald-500',
  zinc: 'bg-zinc-300 dark:bg-zinc-600',
  indigo: 'bg-gradient-to-r from-indigo-400 to-indigo-500',
  fuchsia: 'bg-gradient-to-r from-fuchsia-400 to-fuchsia-500',
  teal: 'bg-gradient-to-r from-teal-400 to-teal-500',
  orange: 'bg-gradient-to-r from-orange-400 to-orange-500',
};

const text = (a) => (a.revision_due_on ? `Revision due ${diffForHumans(a.revision_due_on)}.` : a.text);
const left = (dl) => dl.days_left;
const leftText = (n) => (n < 0 ? `${Math.abs(n)}d late` : n === 0 ? 'today' : `${n}d`);
const ringTone = (c) => (c.overdue ? 'red' : c.pct > 75 ? 'amber' : 'indigo');

async function submitNext(paperId) {
  if (busy.value) return;
  busy.value = paperId;
  try {
    const r = await api('desk.submitNext', { paper_id: paperId });
    if (r?.submission) {
      toast({ variant: 'success', heading: 'Submitted', text: `${r.paper.title} is now with ${r.submission.venue_name}.` });
      bus.emit('papers-changed');
    }
    await load();
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  } finally {
    busy.value = null;
  }
}

async function rowMenu(a) {
  const id = await contextMenu([
    { id: 'open', label: 'Open paper' },
    ...(a.next ? [{ separator: true }, { id: 'submit', label: 'Submit to the next journal or conference' }] : []),
  ]);
  if (id === 'open') navigate('papers.index', a.paper.id);
  if (id === 'submit') submitNext(a.paper.id);
}

const offs = [];
onMounted(() => {
  load();
  offs.push(on('changed', ({ area } = {}) => { if (['papers', 'deadlines', 'venues', 'backup', 'external'].includes(area)) load(); }));
  offs.push(bus.on('papers-changed', load));
});
onBeforeUnmount(() => offs.forEach((off) => off()));
</script>

<template>
    <div class="page-in mx-auto w-full max-w-6xl space-y-6 2xl:max-w-[1440px]">
        <!-- Hero -->
        <section class="hero relative overflow-hidden rounded-2xl border border-zinc-200/80 px-8 py-7 dark:border-zinc-800">
            <div class="hero-grid pointer-events-none absolute inset-0 text-zinc-900 dark:text-white"></div>
            <div class="relative">
                <p class="eyebrow">{{ format(today, 'l, j F Y') }}</p>
                <h1 class="mt-1 text-3xl font-semibold tracking-tight">Good {{ greeting }}, {{ firstName }}.</h1>
                <p v-if="d" class="mt-1 text-sm text-zinc-600 dark:text-zinc-400">
                    <template v-if="!attention.length">Nothing needs you today. Your papers are where they should be.</template>
                    <template v-else>{{ attention.length }} {{ plural('thing', attention.length) }} could use your attention.</template>
                </p>
                <p v-else class="mt-1 text-sm text-transparent">&nbsp;</p>
                <div v-if="d" class="stagger mt-6 grid grid-cols-4 gap-3">
                    <div v-for="s in d.stats" :key="s.label" class="lift rounded-xl border border-white/60 bg-white/70 p-4 backdrop-blur dark:border-white/5 dark:bg-zinc-900/60">
                        <div class="flex items-center justify-between">
                            <span class="text-xs font-medium text-zinc-500">{{ s.label }}</span>
                            <span class="flex size-7 items-center justify-center rounded-lg bg-gradient-to-br text-white shadow-sm" :class="s.grad">
                                <FluxIcon :name="s.icon" variant="micro" />
                            </span>
                        </div>
                        <UiCount :value="s.value" class="mt-2 block text-3xl font-semibold tracking-tight" />
                    </div>
                </div>
                <div v-else class="mt-6 h-[94px]"></div>
            </div>
        </section>

        <div v-if="!d" class="grid h-96 grid-cols-5 gap-6" aria-hidden="true"><div class="panel col-span-3 animate-pulse bg-zinc-100/60 dark:bg-zinc-800/40"></div><div class="panel col-span-2 animate-pulse bg-zinc-100/60 dark:bg-zinc-800/40"></div></div>
        <template v-else>
            <div v-if="d.empty" class="panel">
                <UiEmpty title="Your desk is clear" text="Add your first paper and its submissions. The desk will then show review clocks, deadlines and what to do next.">
                    <FluxModalTrigger name="new-paper"><FluxButton variant="primary" icon="plus">Add a paper</FluxButton></FluxModalTrigger>
                </UiEmpty>
            </div>

            <template v-else>
                <div class="grid grid-cols-5 gap-6">
                    <!-- Attention -->
                    <section class="panel col-span-3 p-5">
                        <div class="mb-3 flex items-center justify-between">
                            <h2 class="font-semibold">Needs attention</h2>
                            <span class="num text-xs text-zinc-400">{{ attention.length }}</span>
                        </div>
                        <UiList>
                        <div v-for="a in attention" :key="a.paper.id + '-' + a.icon" class="stagger">
                            <div
                                class="flex items-start gap-3 border-t border-zinc-100 py-3 first:border-0 dark:border-zinc-800"
                                :class="[a.tone === 'violet' ? '-mx-2 rounded-xl border-transparent bg-violet-50/70 px-2 ring-1 ring-violet-200 dark:bg-violet-500/10 dark:ring-violet-500/30' : '']"
                                @contextmenu.prevent="rowMenu(a)"
                            >
                                <span class="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg" :class="TONES[a.tone]">
                                    <FluxIcon :name="a.icon" variant="mini" />
                                </span>
                                <div class="min-w-0 flex-1">
                                    <a :href="route('papers.index', a.paper.id)" class="text-sm font-medium hover:text-indigo-600">{{ a.paper.title }}</a>
                                    <p class="text-sm break-words text-zinc-500">{{ text(a) }}</p>
                                </div>
                                <FluxButton v-if="a.next" size="sm" variant="primary" icon-trailing="arrow-right" :loading="busy === a.paper.id" @click="submitNext(a.paper.id)">Submit</FluxButton>
                                <FluxButton v-if="a.key" size="sm" variant="subtle" icon="x-mark" aria-label="Dismiss this warning" title="Dismiss (you can bring it back in Settings → General)" @click="dismiss(a)" />
                            </div>
                        </div>
                        </UiList>
                        <div v-if="!attention.length" class="flex items-center gap-3 rounded-lg bg-emerald-500/5 p-4 text-sm text-emerald-700 dark:text-emerald-400">
                            <FluxIcon name="check-circle" variant="mini" /> All clear. No overdue reviews, risky preprints or stalled papers.
                        </div>
                    </section>

                    <!-- Deadlines -->
                    <section class="panel col-span-2 p-5">
                        <div class="mb-3 flex items-center justify-between">
                            <h2 class="font-semibold">Coming up</h2>
                            <a :href="route('deadlines')" class="text-xs text-zinc-500 hover:text-indigo-600">All deadlines</a>
                        </div>
                        <UiList class="stagger space-y-1">
                            <div v-for="dl in d.deadlines" :key="dl.id" class="flex items-center gap-3 rounded-lg px-2 py-2 hover:bg-zinc-50 dark:hover:bg-zinc-800/50">
                                <div class="flex w-11 shrink-0 flex-col items-center rounded-lg border border-zinc-200 py-1 leading-none dark:border-zinc-700">
                                    <span class="text-[10px] font-semibold uppercase text-rose-500">{{ format(dl.due_on, 'M') }}</span>
                                    <span class="num text-base font-semibold">{{ format(dl.due_on, 'j') }}</span>
                                </div>
                                <div class="min-w-0 flex-1">
                                    <div class="truncate text-sm font-medium">{{ dl.title }}</div>
                                    <div class="truncate text-xs text-zinc-500">{{ dl.paper ?? 'General' }}</div>
                                </div>
                                <span class="num text-xs font-medium" :class="left(dl) < 0 ? 'text-rose-500' : left(dl) <= 7 ? 'text-amber-600' : 'text-zinc-400'">{{ leftText(left(dl)) }}</span>
                            </div>
                        </UiList>
                        <UiEmpty v-if="!d.deadlines.length" art="calendar" title="No deadlines" text="Add conference dates or revision due dates on the Deadlines page." class="!py-6" />
                    </section>
                </div>

                <!-- Review clocks -->
                <section v-if="d.clocks.length">
                    <div class="mb-3 flex items-baseline justify-between gap-2">
                        <h2 class="font-semibold">Review clocks</h2>
                        <span class="text-xs text-zinc-400">Days in review against each journal’s or conference’s usual decision time</span>
                    </div>
                    <UiList :pin="false" class="stagger grid grid-cols-2 gap-3 xl:grid-cols-3">
                        <a v-for="s in d.clocks" :key="s.id" :href="route('papers.index', s.paper_id)" class="panel lift flex items-center gap-4 p-4">
                            <UiRing v-if="s.clock" :pct="s.clock.pct" :tone="ringTone(s.clock)" :label="String(s.days)" sub="days" />
                            <UiRing v-else :pct="0" tone="zinc" :label="s.days === null || s.days === undefined ? '–' : String(s.days)" sub="days" />
                            <div class="min-w-0">
                                <div class="truncate text-sm font-semibold">{{ s.paper_title }}</div>
                                <div class="truncate text-sm text-zinc-500">{{ s.venue_name }}</div>
                                <div class="mt-1 text-xs" :class="s.clock && s.clock.overdue ? 'text-rose-500' : 'text-zinc-400'">
                                    <template v-if="s.clock">{{ s.clock.overdue ? 'Past the usual ' + s.clock.typical + ' days' : 'Decision expected around ' + format(s.clock.expected_on, 'j M') }}</template>
                                    <template v-else>Add the journal’s or conference’s decision time to start the clock</template>
                                </div>
                            </div>
                        </a>
                    </UiList>
                </section>

                <!-- Journey map -->
                <section v-if="d.journey.lanes.length" class="panel overflow-hidden p-5">
                    <div class="mb-4 flex flex-wrap items-baseline justify-between gap-2">
                        <h2 class="font-semibold">The last twelve months</h2>
                        <div class="flex flex-wrap gap-3 text-xs text-zinc-500">
                            <span class="flex items-center gap-1.5"><i class="size-2 rounded-full bg-sky-500"></i>In review</span>
                            <span class="flex items-center gap-1.5"><i class="size-2 rounded-full bg-amber-500"></i>Revision</span>
                            <span class="flex items-center gap-1.5"><i class="size-2 rounded-full bg-rose-500"></i>Rejected</span>
                            <span class="flex items-center gap-1.5"><i class="size-2 rounded-full bg-emerald-500"></i>Accepted</span>
                        </div>
                    </div>
                    <div class="relative">
                        <div class="ml-36 grid grid-cols-12 border-b border-zinc-100 pb-1 text-[10px] tracking-wide text-zinc-400 uppercase dark:border-zinc-800">
                            <span v-for="m in d.journey.months" :key="m">{{ format(m, 'M') }}</span>
                        </div>
                        <div class="space-y-2 pt-3">
                            <div v-for="(lane, i) in d.journey.lanes" :key="lane.paper.id" class="flex items-center">
                                <a :href="route('papers.index', lane.paper.id)" class="w-36 shrink-0 truncate pr-3 text-xs font-medium hover:text-indigo-600">{{ lane.paper.title }}</a>
                                <div class="relative h-6 flex-1 rounded-md bg-zinc-50 dark:bg-zinc-800/40">
                                    <div
                                        v-for="b in lane.bars"
                                        :key="b.id"
                                        class="grow-x group absolute top-1 h-4 rounded shadow-sm"
                                        :class="BARS[b.tone]"
                                        :style="`left: ${b.x}%; width: ${b.w}%; animation-delay: ${0.1 + i * 0.08}s`"
                                    >
                                        <span class="pointer-events-none absolute bottom-full left-0 z-10 mb-1 hidden rounded-md bg-zinc-900 px-2 py-1 text-[11px] whitespace-nowrap text-white shadow-lg group-hover:block dark:bg-white dark:text-zinc-900">
                                            {{ b.venue_name }} · {{ b.decision_label }} · {{ b.days }} days
                                        </span>
                                    </div>
                                </div>
                            </div>
                        </div>
                        <div class="pointer-events-none absolute top-0 bottom-0 ml-36" style="left: 0; right: 0">
                            <div class="absolute top-0 bottom-0 w-px bg-indigo-500/70" :style="`left: ${d.journey.today_x}%`">
                                <span class="absolute -top-0.5 -translate-x-full rounded bg-indigo-600 px-1 text-[9px] font-semibold text-white">today</span>
                            </div>
                        </div>
                    </div>
                </section>
            </template>
        </template>
    </div>
</template>
