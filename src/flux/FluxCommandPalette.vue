<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import FluxIcon from './FluxIcon.vue';
import { closePalette, matchSearch, matchStatic, openPalette, palette } from '@/lib/palette.js';
import { onMenu } from '@/lib/native.js';

// Command palette, opened by the native menu's Edit > Find… (Cmd/Ctrl+K, menu id 'find') or openPalette().
// Mount once in the app shell; feed it with registerCommands/registerSearch.
defineProps({ placeholder: { type: String, default: 'Search papers, journals, conferences, pages and actions' } });

const dialog = ref(null);
const input = ref(null);
const list = ref(null);
const active = ref(0);
const remote = ref([]);
let seq = 0;
let debounce = null;

const results = computed(() => {
  const seen = new Set();
  return [...matchStatic(palette.query), ...remote.value].filter((c) => (seen.has(c.id) ? false : seen.add(c.id))).slice(0, 50);
});
const groups = computed(() => {
  const out = [];
  results.value.forEach((c, i) => {
    const g = c.group || 'Results';
    let grp = out.find((x) => x.name === g);
    if (!grp) out.push((grp = { name: g, items: [] }));
    grp.items.push({ ...c, index: i });
  });
  return out;
});

watch(() => palette.query, (q) => {
  active.value = 0;
  clearTimeout(debounce);
  const mine = ++seq;
  if (!q.trim()) { remote.value = []; return; }
  debounce = setTimeout(async () => { const r = await matchSearch(q.trim()); if (mine === seq) remote.value = r; }, 120);
});

watch(() => palette.open, async (open) => {
  await nextTick();
  const d = dialog.value;
  if (!d) return;
  if (open && !d.open) { d.showModal(); input.value?.focus(); input.value?.select(); }
  if (!open && d.open) d.close();
});

function run(c) {
  if (!c) return;
  closePalette();
  setTimeout(() => c.run?.(), 0);
}
function move(delta) {
  const n = results.value.length;
  if (!n) return;
  active.value = (active.value + delta + n) % n;
  nextTick(() => list.value?.querySelector('[data-active]')?.scrollIntoView({ block: 'nearest' }));
}
function onKey(e) {
  if (e.key === 'ArrowDown') { e.preventDefault(); move(1); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
  else if (e.key === 'Enter') { e.preventDefault(); run(results.value[active.value]); }
}
let offMenu = null;
onMounted(() => { offMenu = onMenu('find', () => openPalette()); });
onBeforeUnmount(() => offMenu?.());

function onClick(e) { if (e.target === dialog.value) closePalette(); }
</script>

<template>
    <ui-modal data-flux-modal>
        <dialog ref="dialog" class="mt-[12vh] w-full max-w-xl overflow-hidden p-0 shadow-lg rounded-xl bg-white dark:bg-zinc-800 ring ring-black/5 dark:ring-zinc-700" data-flux-command-palette @close="closePalette" @click="onClick">
            <div class="relative border-b border-zinc-200 dark:border-zinc-700">
                <FluxIcon name="magnifying-glass" variant="mini" class="pointer-events-none absolute top-1/2 left-4 -translate-y-1/2 text-zinc-400" />
                <input ref="input" v-model="palette.query" type="text" :placeholder="placeholder" class="h-12 w-full bg-transparent ps-11 pe-4 text-sm text-zinc-800 placeholder-zinc-400 outline-none dark:text-white" autocomplete="off" spellcheck="false" @keydown="onKey">
            </div>
            <div ref="list" class="max-h-[50vh] overflow-y-auto p-[.3125rem]" role="listbox">
                <template v-for="g in groups" :key="g.name">
                    <div class="p-2 pb-1 text-xs font-medium text-zinc-500 dark:text-zinc-300">{{ g.name }}</div>
                    <button
                        v-for="c in g.items"
                        :key="c.id"
                        type="button"
                        role="option"
                        :aria-selected="c.index === active ? 'true' : 'false'"
                        :data-active="c.index === active ? '' : undefined"
                        class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-start text-sm font-medium text-zinc-800 data-active:bg-zinc-100 dark:text-white dark:data-active:bg-zinc-700"
                        @mousemove="active = c.index"
                        @click="run(c)"
                    >
                        <FluxIcon v-if="c.icon" :name="c.icon" variant="mini" class="text-zinc-400 dark:text-white/60" />
                        <span class="min-w-0 flex-1 truncate">{{ c.title }}<span v-if="c.subtitle" class="ms-2 text-xs font-normal text-zinc-400">{{ c.subtitle }}</span></span>
                        <span v-if="c.kbd" class="text-xs text-zinc-400">{{ c.kbd }}</span>
                    </button>
                </template>
                <div v-if="!results.length" class="px-3 py-8 text-center text-sm text-zinc-500">{{ palette.query ? 'No matches.' : 'Type to search.' }}</div>
            </div>
        </dialog>
    </ui-modal>
</template>
