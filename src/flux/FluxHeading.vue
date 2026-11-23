<script setup>
import { computed } from 'vue';
import { cx } from './util.js';

const props = defineProps({
  size: { type: String, default: 'base' },
  accent: { type: Boolean, default: false },
  level: { type: [String, Number], default: null },
});
const tag = computed(() => ({ 1: 'h1', 2: 'h2', 3: 'h3', 4: 'h4' }[parseInt(props.level, 10)] || 'div'));
const classes = computed(() => cx(
  'font-medium',
  props.accent ? 'text-[var(--color-accent-content)]' : '[:where(&)]:text-zinc-800 [:where(&)]:dark:text-white',
  {
    '2xl': 'text-4xl [&:has(+[data-flux-subheading])]:mb-2 [[data-flux-subheading]+&]:mt-2',
    xl: 'text-2xl [&:has(+[data-flux-subheading])]:mb-2 [[data-flux-subheading]+&]:mt-2',
    lg: 'text-base [&:has(+[data-flux-subheading])]:mb-2 [[data-flux-subheading]+&]:mt-2',
  }[props.size] || 'text-sm [&:has(+[data-flux-subheading])]:mb-2 [[data-flux-subheading]+&]:mt-2',
));
</script>

<template>
    <component :is="tag" :class="classes" data-flux-heading><slot /></component>
</template>
