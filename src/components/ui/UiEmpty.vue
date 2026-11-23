<script setup>
import { useSlots } from 'vue';
import { hasContent } from '@/flux/util.js';

defineProps({ title: { type: String, required: true }, text: { type: String, default: null }, art: { type: String, default: 'papers' } });
const slots = useSlots();
</script>

<template>
    <div class="flex flex-col items-center justify-center px-6 py-14 text-center">
        <svg viewBox="0 0 160 110" class="float mb-5 h-28 w-40" aria-hidden="true">
            <defs><linearGradient :id="`eg-${art}`" x1="0" x2="1" y1="0" y2="1"><stop offset="0" stop-color="#818cf8" /><stop offset="1" stop-color="#e879f9" /></linearGradient></defs>
            <ellipse cx="80" cy="102" rx="46" ry="5" fill="currentColor" class="text-zinc-200 dark:text-zinc-800" />
            <template v-if="art === 'calendar'">
                <rect x="38" y="18" width="84" height="74" rx="10" fill="none" :stroke="`url(#eg-${art})`" stroke-width="3" />
                <path d="M38 38h84" :stroke="`url(#eg-${art})`" stroke-width="3" /><path d="M58 12v14M102 12v14" :stroke="`url(#eg-${art})`" stroke-width="3" stroke-linecap="round" />
                <circle cx="62" cy="56" r="4" fill="#c7d2fe" /><circle cx="80" cy="56" r="4" fill="#c7d2fe" /><circle cx="98" cy="56" r="4" :fill="`url(#eg-${art})`" /><circle cx="62" cy="74" r="4" fill="#c7d2fe" /><circle cx="80" cy="74" r="4" fill="#c7d2fe" />
            </template>
            <template v-else-if="art === 'venue'">
                <path d="M30 44 80 18l50 26z" fill="none" :stroke="`url(#eg-${art})`" stroke-width="3" stroke-linejoin="round" />
                <path d="M44 50v32M64 50v32M96 50v32M116 50v32M32 90h96" :stroke="`url(#eg-${art})`" stroke-width="3" stroke-linecap="round" />
            </template>
            <template v-else>
                <rect x="52" y="14" width="62" height="80" rx="8" fill="#eef2ff" class="dark:fill-zinc-800" transform="rotate(8 83 54)" />
                <rect x="46" y="12" width="62" height="80" rx="8" fill="#fff" :stroke="`url(#eg-${art})`" stroke-width="3" class="dark:fill-zinc-900" />
                <path d="M58 32h38M58 44h38M58 56h24" stroke="#c7d2fe" stroke-width="4" stroke-linecap="round" />
                <circle cx="100" cy="76" r="12" :fill="`url(#eg-${art})`" /><path d="m94 76 4 4 8-8" stroke="#fff" stroke-width="3" fill="none" stroke-linecap="round" stroke-linejoin="round" />
            </template>
        </svg>
        <h3 class="text-base font-semibold">{{ title }}</h3>
        <p v-if="text" class="mt-1 max-w-sm text-sm text-zinc-500">{{ text }}</p>
        <div v-if="hasContent(slots.default)" class="mt-5"><slot /></div>
    </div>
</template>
