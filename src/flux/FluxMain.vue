<script setup>
import { computed } from 'vue';
import { cx } from './util.js';

const props = defineProps({ container: { type: Boolean, default: false }, inset: { type: Boolean, default: false } });
const classes = computed(() => cx(
  '[grid-area:main]',
  props.inset ? 'p-6 lg:p-10' : 'p-6 lg:p-8',
  props.inset ? 'min-w-0 min-h-0 [:where(&)]:bg-white dark:[:where(&)]:bg-zinc-800 lg:m-2 lg:rounded-xl lg:border lg:[:where(&)]:border-zinc-200 lg:shadow-xs dark:lg:[:where(&)]:border-zinc-700' : '',
  props.inset ? 'lg:overflow-y-auto lg:overscroll-contain' : '',
  '[[data-flux-container]_&]:px-0',
  props.container && !props.inset ? 'mx-auto w-full [:where(&)]:max-w-7xl' : '',
));
</script>

<template>
    <div :class="classes" data-flux-main :data-flux-main-inset="inset ? '' : undefined">
        <div v-if="container && inset" class="mx-auto w-full [:where(&)]:max-w-7xl"><slot /></div>
        <slot v-else />
    </div>
</template>
