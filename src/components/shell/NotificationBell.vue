<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { api } from '@/lib/api.js';
import { on } from '@/lib/events.js';
import { toast } from '@/lib/toast.js';
import { diffForHumans } from '@/lib/format.js';
import { openUrl, utc } from './links.js';
import { usePager } from '@/lib/usePager.js';

const items = ref([]);
const unread = ref(0);
const running = ref([]);
const desktopOn = ref(null);
const scroller = ref(null);
const pager = usePager(items, { query: null, scrollTo: scroller }); // native notification preference (null until loaded)

const TONES = { success: 'bg-emerald-500', warning: 'bg-amber-500', danger: 'bg-rose-500' };
const tone = (n) => TONES[n.data?.status ?? 'success'] ?? 'bg-zinc-400';

function apply(r) {
  if (!r) return;
  items.value = r.items ?? [];
  unread.value = r.unread ?? 0;
  running.value = r.running ?? [];
}

async function load() {
  try { apply(await api('notifications.list')); } catch {}
}

async function openItem(n) {
  try {
    const r = await api('notifications.open', { id: n.id });
    apply(r);
    if (r?.url) openUrl(r.url);
  } catch {}
}

async function markAllRead() {
  try { apply(await api('notifications.markAllRead')); } catch {}
}

async function clearAll() {
  try { apply(await api('notifications.clear')); } catch {}
}

async function turnOnDesktop() {
  try {
    const p = await api('app.setPreferences', { native_notifications: true });
    desktopOn.value = !!p?.native_notifications;
  } catch {}
}

function percent(r) {
  return r.total > 0 ? Math.min(100, Math.round((r.done / r.total) * 100)) : null;
}

// An `ai-runs` event carries the run row: update the list in place, no query needed.
function onRun(row) {
  if (!row?.id) return;
  const list = running.value.filter((r) => r.id !== row.id);
  if (row.status === 'running') list.unshift({ ...row, percent: percent(row) });
  list.sort((a, b) => String(b.created_at).localeCompare(String(a.created_at)) || b.id - a.id);
  running.value = list;
}

const offs = [];
let timer = null;
onMounted(async () => {
  load();
  offs.push(on('notifications', (p) => {
    load();
    const d = p?.notification?.data;
    if (d?.title) {
      toast({ heading: d.title, text: d.body ?? '', variant: d.status === 'danger' ? 'danger' : d.status === 'warning' ? 'warning' : 'success', duration: 8000 });
    }
  }));
  offs.push(on('ai-runs', onRun));
  // Runs that stop responding are closed after 20 minutes; check now and then while something runs.
  timer = setInterval(() => { if (running.value.length) load(); }, 60000);
  try { desktopOn.value = !!(await api('app.preferences'))?.native_notifications; } catch {}
});
onBeforeUnmount(() => { offs.forEach((off) => off()); clearInterval(timer); });

const badge = computed(() => (unread.value > 9 ? '9+' : unread.value));
</script>

<template>
    <div>
        <UiCollapse>
        <div v-if="running.length" class="mb-2 space-y-2 rounded-xl border border-indigo-200/70 bg-gradient-to-br from-indigo-50 to-fuchsia-50/60 p-3 dark:border-indigo-500/20 dark:from-indigo-500/10 dark:to-fuchsia-500/5">
            <div class="flex items-center gap-1.5 text-[11px] font-semibold tracking-wide text-indigo-700 uppercase dark:text-indigo-300">
                <FluxIcon name="sparkles" variant="micro" class="size-3.5 animate-pulse" /> AI at work
            </div>
            <UiList class="space-y-2">
            <a v-for="r in running" :key="'run' + r.id" href="#" class="block" @click.prevent="openUrl(r.url)">
                <div class="flex items-baseline justify-between gap-2">
                    <span class="truncate text-xs font-medium text-zinc-800 dark:text-zinc-100">{{ r.label }}</span>
                    <span class="num shrink-0 text-[10px] text-zinc-500">{{ r.total > 1 ? r.done + '/' + r.total : '' }} {{ diffForHumans(utc(r.created_at), null, true, true) }}</span>
                </div>
                <div class="mt-1 h-1.5 overflow-hidden rounded-full bg-white/80 dark:bg-zinc-800">
                    <div
                        class="h-full rounded-full bg-gradient-to-r from-indigo-500 to-fuchsia-500 transition-[width] duration-700"
                        :class="r.percent === null ? 'w-1/3 animate-[scan_1.6s_ease-in-out_infinite]' : ''"
                        :style="r.percent !== null ? `width: ${Math.max(4, r.percent)}%` : undefined"
                    ></div>
                </div>
                <p class="mt-1 truncate text-[11px] text-zinc-500" :title="r.step">{{ r.step }}</p>
            </a>
            </UiList>
        </div>
        </UiCollapse>
        <FluxDropdown position="top" align="start">
            <button type="button" class="relative flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-zinc-600 transition hover:bg-zinc-800/5 hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-white/5 dark:hover:text-white">
                <span class="relative">
                    <FluxIcon name="bell" class="size-5" />
                    <span v-if="unread" class="absolute -top-1.5 -right-1.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-rose-500 px-1 text-[10px] font-bold text-white ring-2 ring-white dark:ring-zinc-900">{{ badge }}</span>
                </span>
                <span>Notifications</span>
            </button>

            <FluxMenu class="w-80 !p-0">
                <div class="flex items-center justify-between border-b border-zinc-100 px-4 py-3 dark:border-zinc-700">
                    <span class="text-sm font-semibold text-zinc-900 dark:text-white">Notifications</span>
                    <span class="flex items-center gap-3">
                        <button v-if="unread" type="button" class="text-xs text-zinc-500 hover:text-indigo-600 dark:text-zinc-300 dark:hover:text-indigo-300" @click="markAllRead">Mark all read</button>
                        <button v-if="items.length" type="button" class="text-xs text-zinc-500 hover:text-rose-600 dark:text-zinc-300 dark:hover:text-rose-300" @click="clearAll">Clear</button>
                    </span>
                </div>
                <div ref="scroller" class="max-h-96 overflow-y-auto">
                    <UiList>
                    <button
                        v-for="n in pager.items.value"
                        :key="'n' + n.id"
                        type="button"
                        class="flex w-full gap-3 px-4 py-3 text-left transition hover:bg-zinc-50 dark:hover:bg-white/5"
                        :class="n.read_at ? 'opacity-70' : ''"
                        @click="openItem(n)"
                    >
                        <span class="mt-1.5 size-2 shrink-0 rounded-full" :class="n.read_at ? 'bg-zinc-300 dark:bg-zinc-600' : tone(n)"></span>
                        <span class="min-w-0">
                            <span class="block text-sm font-medium text-zinc-900 dark:text-white">{{ n.data?.title }}</span>
                            <span class="block text-xs text-zinc-600 dark:text-zinc-300">{{ n.data?.body }}</span>
                            <span class="mt-0.5 block text-[11px] text-zinc-400 dark:text-zinc-400">{{ diffForHumans(utc(n.created_at)) }}</span>
                        </span>
                    </button>
                    </UiList>
                    <div v-if="pager.visible.value" class="border-t border-zinc-100 px-4 py-2 dark:border-zinc-700"><FluxPagination :pager="pager" :sizes="false" compact label="Notifications" /></div>
                    <div v-if="!items.length" class="px-4 py-8 text-center text-sm text-zinc-500 dark:text-zinc-300">
                        <FluxIcon name="bell-slash" class="mx-auto mb-2 size-6 text-zinc-300" />
                        Nothing yet. You will see finished scans and updates here.
                    </div>
                </div>
                <div class="border-t border-zinc-100 px-4 py-2.5 text-xs dark:border-zinc-700">
                    <button v-if="desktopOn === false" type="button" class="font-medium text-indigo-600 hover:underline" @click="turnOnDesktop">Turn on desktop notifications</button>
                    <span v-else-if="desktopOn" class="text-zinc-500 dark:text-zinc-400">Desktop notifications are on</span>
                </div>
            </FluxMenu>
        </FluxDropdown>
    </div>
</template>
