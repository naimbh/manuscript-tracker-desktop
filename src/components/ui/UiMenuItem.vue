<script setup>
defineProps({ href: { type: String, default: null } });
</script>

<template>
    <a v-if="href" :href="href" class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-zinc-700 hover:bg-zinc-100 dark:text-zinc-200 dark:hover:bg-zinc-800"><slot /></a>
    <button v-else type="button" class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-zinc-700 hover:bg-zinc-100 dark:text-zinc-200 dark:hover:bg-zinc-800"><slot /></button>
</template>
