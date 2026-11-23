<script setup>
import { computed } from 'vue';
import { initials } from '@/lib/format.js';

const props = defineProps({ user: { type: Object, required: true }, size: { type: String, default: 'size-9' } });
const text = computed(() => initials(props.user?.name));
</script>

<template>
    <span :class="`relative inline-flex shrink-0 items-center justify-center rounded-full bg-gradient-to-br from-indigo-500 via-violet-500 to-fuchsia-500 p-[2px] shadow-sm shadow-indigo-500/30 ${size}`">
        <img v-if="user?.avatar" :src="user.avatar" alt="" draggable="false" class="size-full rounded-full object-cover ring-2 ring-white/70 dark:ring-zinc-900/70" />
        <span v-else class="flex size-full items-center justify-center rounded-full bg-gradient-to-br from-indigo-600 to-fuchsia-600 text-[11px] font-semibold tracking-wide text-white ring-2 ring-white/70 dark:ring-zinc-900/70">{{ text }}</span>
        <span class="absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full bg-emerald-500 ring-2 ring-white dark:ring-zinc-900"></span>
    </span>
</template>
