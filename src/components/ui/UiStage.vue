<script setup>
import { computed } from 'vue';
import { STAGES } from '@/lib/constants.js';

const props = defineProps({ stage: { type: String, required: true } });
const DOT = { idea: 'bg-zinc-300', drafting: 'bg-zinc-400', ready: 'bg-violet-500', submitted: 'bg-indigo-500', under_review: 'bg-sky-500', revision: 'bg-amber-500', accepted: 'bg-emerald-500', published: 'bg-emerald-600', shelved: 'bg-zinc-300' };
const dot = computed(() => DOT[props.stage] ?? 'bg-zinc-300');
</script>

<template>
    <span class="inline-flex items-center gap-1.5 text-xs text-zinc-500 dark:text-zinc-400"><span :class="`size-1.5 rounded-full ${dot}`"></span>{{ STAGES[stage] ?? stage }}</span>
</template>
