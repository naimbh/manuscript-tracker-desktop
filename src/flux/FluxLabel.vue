<script setup>
import { computed, useSlots } from 'vue';
import { cx } from './util.js';

const props = defineProps({
  badge: { type: String, default: null },
  aside: { type: String, default: null },
  trailing: { type: String, default: null },
  srOnly: { type: Boolean, default: false },
});
const slots = useSlots();
const classes = computed(() => cx('inline-flex items-center', 'text-sm font-medium', props.srOnly ? 'sr-only' : '', '[:where(&)]:text-zinc-800 [:where(&)]:dark:text-white', '[&:has([data-flux-label-trailing])]:flex'));
</script>

<template>
    <ui-label :class="classes" data-flux-label>
        <slot />
        <span v-if="badge" class="ms-1.5 text-zinc-800/70 text-xs bg-zinc-800/5 px-1.5 py-1 -my-1 rounded-[4px] dark:bg-white/10 dark:text-zinc-300" aria-hidden="true">{{ badge }}</span>
        <span v-else-if="slots.badge" class="ms-1.5" aria-hidden="true"><slot name="badge" /></span>
        <span v-if="aside" class="ms-1.5 text-zinc-800/70 text-xs bg-zinc-800/5 px-1.5 py-1 -my-1 rounded-[4px] dark:bg-white/10 dark:text-zinc-300" aria-hidden="true">{{ aside }}</span>
        <div v-if="trailing || slots.trailing" class="ml-auto" data-flux-label-trailing><slot name="trailing">{{ trailing }}</slot></div>
    </ui-label>
</template>
