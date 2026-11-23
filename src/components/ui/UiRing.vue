<script setup>
import { onMounted, ref, watch } from 'vue';

const props = defineProps({
  pct: { type: [Number, String], default: 0 },
  size: { type: [Number, String], default: 56 },
  tone: { type: String, default: 'indigo' },
  label: { type: String, default: null },
  sub: { type: String, default: null },
});
const r = 22;
const c = 2 * Math.PI * r;
const COLORS = { indigo: '#6366f1', red: '#ef4444', amber: '#f59e0b', green: '#10b981', zinc: '#a1a1aa' };
const p = ref(0);
onMounted(() => requestAnimationFrame(() => (p.value = parseInt(props.pct, 10) || 0)));
watch(() => props.pct, (v) => (p.value = parseInt(v, 10) || 0));
</script>

<template>
    <div class="relative shrink-0" :style="`width: ${size}px; height: ${size}px`">
        <svg viewBox="0 0 52 52" class="size-full -rotate-90">
            <circle cx="26" cy="26" :r="r" fill="none" stroke-width="5" class="ring-track" />
            <circle cx="26" cy="26" :r="r" fill="none" stroke-width="5" stroke-linecap="round" class="ring-fill" :stroke="COLORS[tone] ?? COLORS.indigo" :stroke-dasharray="c" :stroke-dashoffset="c * (1 - p / 100)" />
        </svg>
        <div class="absolute inset-0 flex flex-col items-center justify-center leading-none">
            <span class="num text-[13px] font-semibold">{{ label ?? (parseInt(pct, 10) || 0) + '%' }}</span>
            <span v-if="sub" class="mt-0.5 text-[9px] uppercase tracking-wide text-zinc-400">{{ sub }}</span>
        </div>
    </div>
</template>
