<script setup>
import { computed } from 'vue';
import { cx } from './util.js';

const props = defineProps({
  orientation: { type: String, default: null },
  vertical: { type: Boolean, default: false },
  variant: { type: String, default: null },
  faint: { type: Boolean, default: false },
  text: { type: String, default: null },
});
const orient = computed(() => props.orientation ?? (props.vertical ? 'vertical' : 'horizontal'));
const classes = computed(() => cx(
  'border-0 [print-color-adjust:exact]',
  props.variant === 'subtle' ? 'bg-zinc-800/5 dark:bg-white/10' : 'bg-zinc-800/15 dark:bg-white/20',
  orient.value === 'vertical' ? 'self-stretch self-center w-px' : 'h-px w-full',
));
</script>

<template>
    <div v-if="text" :data-orientation="orient" role="none" class="flex items-center w-full" data-flux-separator>
        <div :class="[classes, 'grow']"></div>
        <span class="shrink mx-6 font-medium text-sm text-zinc-500 dark:text-zinc-300 whitespace-nowrap">{{ text }}</span>
        <div :class="[classes, 'grow']"></div>
    </div>
    <div v-else :data-orientation="orient" role="none" :class="classes" data-flux-separator></div>
</template>
