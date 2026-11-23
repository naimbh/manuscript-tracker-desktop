<script setup>
import { computed } from 'vue';

/**
 * Live progress of a research run (in a page hero): step text, done/total, bar, recent results.
 * Slot `result` renders one partial result: <template #result="{ r }">…</template>.
 */
const props = defineProps({
  run: { type: Object, required: true },
  text: { type: String, default: 'Working…' },
  showResults: { type: Number, default: 4 },
});

const pct = computed(() => {
  const r = props.run;
  if (r.total > 0 && r.done > 0) return Math.min(100, (r.done / r.total) * 100);
  return Math.min(100, r.progress || 0);
});
const step = computed(() => props.run.step || props.text);
const recent = computed(() => props.run.results.slice(0, props.showResults));
</script>

<template>
    <div class="relative mt-5 select-none">
        <div class="mb-1.5 flex justify-between gap-4 text-xs text-zinc-600 dark:text-zinc-400">
            <span class="flex min-w-0 items-center gap-2"><FluxIcon name="arrow-path" variant="micro" class="shrink-0 animate-spin" /> <span class="truncate">{{ step }}</span></span>
            <span v-if="run.total > 1" class="num shrink-0">{{ run.done }} / {{ run.total }}</span>
            <span v-else-if="pct > 0" class="num shrink-0">{{ Math.round(pct) }}%</span>
        </div>
        <div class="clock-track !h-2">
            <div v-if="pct > 0" class="clock-fill bg-gradient-to-r from-indigo-500 to-fuchsia-500 transition-[width] duration-300 ease-out" :style="`width: ${Math.max(2, pct)}%`"></div>
            <div v-else class="h-full w-1/3 animate-[scan_1.6s_ease-in-out_infinite] rounded-full bg-gradient-to-r from-indigo-500 to-fuchsia-500"></div>
        </div>
        <UiList v-if="recent.length && $slots.result" tag="ul" class="mt-2.5 space-y-1 overflow-hidden text-xs">
            <li v-for="r in recent" :key="r.__k" class="flex min-w-0 items-center gap-1.5 text-zinc-600 dark:text-zinc-400"><slot name="result" :r="r" /></li>
        </UiList>
    </div>
</template>
