<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { useResource } from '@/lib/useResource.js';
import { usePager } from '@/lib/usePager.js';
import { api, submit } from '@/lib/api.js';
import { provideErrors } from '@/flux/errors.js';
import { on } from '@/lib/events.js';
import { bus } from '@/lib/bus.js';
import { toast } from '@/lib/toast.js';
import { contextMenu } from '@/lib/native.js';
import { navigate } from '@/lib/router.js';
import { format } from '@/lib/format.js';

const res = useResource('deadlines', () => api('deadlines.list'), {
  pageReady: true,
  onError: (e) => toast({ variant: 'danger', text: e.message }),
});
const upcoming = computed(() => res.data.value?.upcoming ?? []);
const done = computed(() => res.data.value?.done ?? []);
const papers = computed(() => res.data.value?.papers ?? []);
const loaded = res.loaded;
const listEl = ref(null);
const pager = usePager(upcoming, { scrollTo: listEl });
const blank = () => ({ title: '', due_on: '', paper_id: '' });
const form = reactive(blank());
const errors = ref({});
provideErrors(errors);
const busy = ref(false);
const load = res.refresh;

async function add() {
  if (busy.value) return;
  busy.value = true;
  try {
    const row = await submit(errors, () => api('deadlines.create', form));
    if (!row) return;
    Object.assign(form, blank());
    toast({ variant: 'success', text: 'Deadline added.' });
    await load();
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  } finally {
    busy.value = false;
  }
}

async function toggle(id) {
  try { await api('deadlines.toggle', { id }); } catch (e) { toast({ variant: 'danger', text: e.message }); }
  await load();
}

async function remove(id) {
  try { await api('deadlines.delete', { id }); } catch (e) { toast({ variant: 'danger', text: e.message }); }
  await load();
}

async function rowMenu(d) {
  const id = await contextMenu([
    { id: 'toggle', label: d.done ? 'Mark as not done' : 'Mark as done' },
    ...(d.paper_id ? [{ id: 'open', label: 'Open paper' }] : []),
    { separator: true },
    { id: 'delete', label: 'Delete' },
  ]);
  if (id === 'toggle') toggle(d.id);
  if (id === 'open') navigate('papers.index', d.paper_id);
  if (id === 'delete') remove(d.id);
}

const pct = (left) => Math.max(4, Math.min(100, 100 - (left * 100) / 60));
const tone = (left, late, soon, later) => (left < 0 ? late : left <= 7 ? soon : later);
const leftText = (left) => (left < 0 ? `${Math.abs(left)}d late` : left === 0 ? 'Today' : `${left}d`);

const offs = [];
onMounted(() => {
  load();
  offs.push(on('changed', ({ area } = {}) => { if (['deadlines', 'papers', 'backup', 'external'].includes(area)) load(); }));
  offs.push(bus.on('papers-changed', load));
});
onBeforeUnmount(() => offs.forEach((off) => off()));
</script>

<template>
    <div class="page-in mx-auto w-full max-w-6xl space-y-6 2xl:max-w-[1440px]">
        <div class="flex flex-wrap items-end justify-between gap-3">
            <div><p class="eyebrow">Deadlines</p><h1 class="mt-1 text-2xl font-semibold tracking-tight">What is due, and when</h1></div>
            <FluxButton size="sm" icon="calendar-days" :href="route('settings.data')">Add to my calendar</FluxButton>
        </div>

        <form class="panel grid grid-cols-[minmax(0,2fr)_11rem_minmax(0,1fr)_auto] items-end gap-3 p-4 [&>*]:min-w-0" @submit.prevent="add">
            <FluxInput v-model="form.title" name="title" label="Deadline" placeholder="CHI late-breaking work" />
            <FluxInput v-model="form.due_on" name="due_on" type="date" label="Due" />
            <FluxSelect v-model="form.paper_id" name="paper_id" label="Paper">
                <FluxSelectOption value="">General</FluxSelectOption>
                <FluxSelectOption v-for="p in papers" :key="p.id" :value="String(p.id)">{{ p.title }}</FluxSelectOption>
            </FluxSelect>
            <FluxButton type="submit" variant="primary" icon="plus" :disabled="busy">Add</FluxButton>
        </form>

        <div v-if="!loaded" class="space-y-2" aria-hidden="true"><div v-for="i in 3" :key="i" class="panel h-[86px] animate-pulse bg-zinc-100/60 dark:bg-zinc-800/40"></div></div>
        <div v-else ref="listEl" class="scroll-mt-4 space-y-3">
          <UiList class="stagger space-y-2">
            <div v-for="d in pager.items.value" :key="'d' + d.id" class="panel lift group flex items-center gap-4 p-4" @contextmenu.prevent="rowMenu(d)">
                <button type="button" class="flex size-5 shrink-0 items-center justify-center rounded-full border-2 border-zinc-300 transition hover:border-emerald-500 dark:border-zinc-600" aria-label="Mark as done" @click="toggle(d.id)"></button>
                <div class="flex w-12 shrink-0 flex-col items-center rounded-lg border border-zinc-200 py-1 leading-none dark:border-zinc-700">
                    <span class="text-[10px] font-semibold text-rose-500 uppercase">{{ format(d.due_on, 'M') }}</span>
                    <span class="num text-lg font-semibold">{{ format(d.due_on, 'j') }}</span>
                </div>
                <div class="min-w-0 flex-1">
                    <p class="truncate text-base font-medium">{{ d.title }}</p>
                    <p class="truncate text-xs text-zinc-500">{{ d.paper ?? 'General' }}<span> · {{ format(d.due_on, 'l, j F Y') }}</span></p>
                    <div class="clock-track mt-2 !h-1"><div class="clock-fill grow-x" :class="tone(d.days_left, 'bg-rose-500', 'bg-amber-500', 'bg-indigo-500')" :style="`width: ${pct(d.days_left)}%`"></div></div>
                </div>
                <span class="num w-16 text-right text-sm font-semibold" :class="tone(d.days_left, 'text-rose-500', 'text-amber-600', 'text-zinc-500')">{{ leftText(d.days_left) }}</span>
                <button type="button" class="text-zinc-300 opacity-0 transition group-hover:opacity-100 hover:text-rose-500" aria-label="Delete" @click="remove(d.id)"><FluxIcon name="trash" variant="micro" /></button>
            </div>
          </UiList>
            <div v-if="!upcoming.length" class="panel"><UiEmpty art="calendar" title="No deadlines" text="Add conference dates, revision due dates or anything else you need to remember." /></div>
            <FluxPagination :pager="pager" label="Deadlines" />
        </div>

        <div v-if="done.length">
            <p class="eyebrow mb-2">Done</p>
            <UiList class="space-y-1">
                <div v-for="d in done" :key="'dd' + d.id" class="flex items-center gap-3 px-4 py-2 text-sm text-zinc-400" @contextmenu.prevent="rowMenu(d)">
                    <button type="button" class="flex size-5 items-center justify-center rounded-full bg-emerald-500 text-white" aria-label="Mark as not done" @click="toggle(d.id)"><FluxIcon name="check" variant="micro" class="size-3" /></button>
                    <span class="flex-1 truncate line-through">{{ d.title }}</span><span class="num">{{ format(d.due_on, 'j M') }}</span>
                </div>
            </UiList>
        </div>
    </div>
</template>
