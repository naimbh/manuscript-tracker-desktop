<script setup>
defineProps({ kbd: { type: String, default: null } });
</script>

<template>
    <div popover="manual" class="relative py-2 px-2.5 rounded-md text-xs text-white font-medium bg-zinc-800 dark:bg-zinc-700 dark:border dark:border-white/10 p-0 overflow-visible" data-flux-tooltip-content><slot /><span v-if="kbd" class="ps-1 text-zinc-300">{{ kbd }}</span></div>
</template>
