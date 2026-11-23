<script setup>
import { computed } from 'vue';
import { installUpdate, updatePercent, updates } from '@/lib/updates.js';

// A small, non-blocking notice above the sidebar footer when a new version is ready.
const show = computed(() => ['available', 'downloading', 'installing'].includes(updates.status));
const percent = computed(() => updatePercent());
</script>

<template>
    <div v-if="show" class="mx-1 mt-2 rounded-xl border border-indigo-200/70 bg-indigo-50/80 p-2.5 text-xs text-indigo-900 dark:border-indigo-400/20 dark:bg-indigo-500/10 dark:text-indigo-100" role="status">
        <template v-if="updates.status === 'available'">
            <p class="font-medium">Version {{ updates.version }} is ready</p>
            <button type="button" class="mt-1.5 inline-flex items-center gap-1 rounded-md bg-indigo-600 px-2 py-1 font-medium text-white transition hover:bg-indigo-500" @click="installUpdate">
                <FluxIcon name="arrow-path" variant="micro" class="size-3.5" />Restart to update
            </button>
        </template>
        <template v-else>
            <p class="font-medium">{{ updates.status === 'installing' ? 'Installing the update…' : 'Downloading the update…' }}</p>
            <div class="mt-1.5 h-1 overflow-hidden rounded-full bg-indigo-200/70 dark:bg-white/10">
                <div class="h-full rounded-full bg-indigo-600 transition-[width] duration-200 dark:bg-indigo-400" :class="percent === null && updates.status === 'downloading' ? 'w-1/3 animate-pulse' : ''" :style="percent !== null || updates.status === 'installing' ? { width: (updates.status === 'installing' ? 100 : percent) + '%' } : {}" />
            </div>
        </template>
    </div>
</template>
