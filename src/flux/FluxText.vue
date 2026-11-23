<script setup>
import { computed } from 'vue';
import { cx } from './util.js';

const props = defineProps({
  inline: { type: Boolean, default: false },
  variant: { type: String, default: null },
  color: { type: String, default: null },
  size: { type: String, default: null },
});
const COLORS = {
  red: 'text-red-600 dark:text-red-400', orange: 'text-orange-600 dark:text-orange-400', amber: 'text-amber-600 dark:text-amber-500',
  yellow: 'text-yellow-600 dark:text-yellow-500', lime: 'text-lime-600 dark:text-lime-500', green: 'text-green-600 dark:text-green-500',
  emerald: 'text-emerald-600 dark:text-emerald-400', teal: 'text-teal-600 dark:text-teal-400', cyan: 'text-cyan-600 dark:text-cyan-400',
  sky: 'text-sky-600 dark:text-sky-400', blue: 'text-blue-600 dark:text-blue-400', indigo: 'text-indigo-600 dark:text-indigo-400',
  violet: 'text-violet-600 dark:text-violet-400', purple: 'text-purple-600 dark:text-purple-400', fuchsia: 'text-fuchsia-600 dark:text-fuchsia-400',
  pink: 'text-pink-600 dark:text-pink-400', rose: 'text-rose-600 dark:text-rose-400',
};
const classes = computed(() => cx(
  '[:where(&)]:font-normal',
  { xl: 'text-lg', lg: 'text-base', sm: 'text-xs' }[props.size] || '[:where(&)]:text-sm',
  props.color ? COLORS[props.color]
    : props.variant === 'strong' ? '[:where(&)]:text-zinc-800 [:where(&)]:dark:text-white'
      : props.variant === 'subtle' ? '[:where(&)]:text-zinc-400 [:where(&)]:dark:text-white/50'
        : '[:where(&)]:text-zinc-500 [:where(&)]:dark:text-white/70',
));
</script>

<template>
    <span v-if="inline" :class="classes" data-flux-text :color="color || undefined"><slot /></span>
    <p v-else :class="classes" data-flux-text :data-color="color || undefined"><slot /></p>
</template>
