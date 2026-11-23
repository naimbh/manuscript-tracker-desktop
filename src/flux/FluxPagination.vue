<script setup>
import { computed } from 'vue';
import FluxIcon from './FluxIcon.vue';
import { PAGE_SIZES } from '@/lib/usePager.js';

/**
 * Flux-style pager: "Showing 21–40 of 87", Previous/Next and compact page numbers with ellipses.
 *   <FluxPagination :pager="pager" />   (from usePager) — hidden while everything fits on one page.
 * `sizes` shows the 20/50/100 selector; `compact` drops the numbers (narrow panes).
 */
const props = defineProps({
  pager: { type: Object, required: true },
  sizes: { type: Boolean, default: true },
  compact: { type: Boolean, default: false },
  label: { type: String, default: 'Pagination' },
});

const p = props.pager;
const numbers = computed(() => {
  const n = p.pages.value;
  const cur = p.page.value;
  if (n <= 7) return Array.from({ length: n }, (_, i) => i + 1);
  const set = new Set([1, n, cur - 1, cur, cur + 1]);
  if (cur <= 3) [2, 3, 4].forEach((x) => set.add(x));
  if (cur >= n - 2) [n - 3, n - 2, n - 1].forEach((x) => set.add(x));
  const list = [...set].filter((x) => x >= 1 && x <= n).sort((a, b) => a - b);
  const out = [];
  list.forEach((x, i) => { if (i && x - list[i - 1] > 1) out.push('…' + x); out.push(x); });
  return out;
});

const btn = 'inline-flex h-7 min-w-7 items-center justify-center gap-1 rounded-md px-2 text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-indigo-500 disabled:pointer-events-none disabled:opacity-40';
const idle = 'text-zinc-600 hover:bg-zinc-100 hover:text-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800 dark:hover:text-white';
const active = 'bg-white text-zinc-900 shadow-xs ring-1 ring-zinc-200 dark:bg-zinc-700 dark:text-white dark:ring-zinc-600';
</script>

<template>
    <nav v-if="p.visible.value" :aria-label="label" data-flux-pagination class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 text-xs text-zinc-500 select-none dark:text-zinc-400">
        <span class="num">Showing <span class="font-medium text-zinc-700 dark:text-zinc-200">{{ p.from.value }}–{{ p.to.value }}</span> of <span class="font-medium text-zinc-700 dark:text-zinc-200">{{ p.total.value }}</span></span>
        <div class="flex items-center gap-2">
            <label v-if="sizes" class="flex items-center gap-1.5">
                <span class="sr-only sm:not-sr-only">Per page</span>
                <select :value="p.perPage.value" class="h-7 rounded-md border border-zinc-200 bg-white px-1.5 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200" @change="p.setSize(Number($event.target.value))">
                    <option v-for="n in PAGE_SIZES" :key="n" :value="n">{{ n }}</option>
                </select>
            </label>
            <div class="flex items-center gap-0.5">
                <button type="button" :class="[btn, idle]" :disabled="p.page.value <= 1" aria-label="Previous page" @click="p.setPage(p.page.value - 1)">
                    <FluxIcon name="chevron-left" variant="micro" class="size-3.5" /><span v-if="!compact" class="hidden sm:inline">Previous</span>
                </button>
                <template v-if="!compact">
                    <template v-for="n in numbers" :key="n">
                        <span v-if="typeof n === 'string'" class="px-1 text-zinc-400" aria-hidden="true">…</span>
                        <button v-else type="button" :class="[btn, 'num', n === p.page.value ? active : idle]" :aria-current="n === p.page.value ? 'page' : undefined" :aria-label="`Page ${n}`" @click="p.setPage(n)">{{ n }}</button>
                    </template>
                </template>
                <span v-else class="num px-1.5">{{ p.page.value }} / {{ p.pages.value }}</span>
                <button type="button" :class="[btn, idle]" :disabled="p.page.value >= p.pages.value" aria-label="Next page" @click="p.setPage(p.page.value + 1)">
                    <span v-if="!compact" class="hidden sm:inline">Next</span><FluxIcon name="chevron-right" variant="micro" class="size-3.5" />
                </button>
            </div>
        </div>
    </nav>
</template>
