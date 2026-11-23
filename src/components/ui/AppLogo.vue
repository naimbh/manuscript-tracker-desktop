<script setup>
import { computed } from 'vue';
import AppLogoIcon from './AppLogoIcon.vue';
import { site } from '@/lib/site.js';
import { afterLast, beforeLast } from '@/lib/format.js';

defineOptions({ inheritAttrs: false });
const name = computed(() => String(site.name || '').trim());
const first = computed(() => (name.value.includes(' ') ? beforeLast(name.value, ' ') : name.value));
const last = computed(() => (name.value.includes(' ') ? afterLast(name.value, ' ') : ''));
</script>

<template>
    <AppLogoIcon class="size-8 shrink-0 drop-shadow-sm" />
    <div class="ml-1 grid flex-1 text-left leading-tight">
        <span class="truncate text-sm font-semibold tracking-tight">{{ first }}{{ last ? ' ' : '' }}<span v-if="last" class="bg-gradient-to-r from-indigo-600 to-fuchsia-600 bg-clip-text text-transparent dark:from-indigo-400 dark:to-fuchsia-400">{{ last }}</span></span>
        <span v-if="site.tagline" class="text-[10px] leading-snug text-zinc-400 [text-wrap:balance]">{{ site.tagline }}</span>
    </div>
</template>
