<script setup>
import { computed } from 'vue';
import { cx } from './util.js';

const props = defineProps({ variant: { type: String, default: 'block' } });

const classes = computed(() => cx(
  'min-w-0',
  '[&:not(:has([data-flux-field])):has([data-flux-control][disabled])>[data-flux-label]]:opacity-50',
  '[&:has(>[data-flux-radio-group][disabled])>[data-flux-label]]:opacity-50',
  '[&:has(>[data-flux-checkbox-group][disabled])>[data-flux-label]]:opacity-50',
  props.variant === 'bare' ? '[:where(&)]:block'
    : props.variant === 'inline' ? [
      'grid gap-x-3 gap-y-1.5',
      'has-[[data-flux-label]~[data-flux-control]]:grid-cols-[1fr_auto]',
      'has-[[data-flux-control]~[data-flux-label]]:grid-cols-[auto_1fr]',
      '[&>[data-flux-control]~[data-flux-description]]:row-start-2 [&>[data-flux-control]~[data-flux-description]]:col-start-2',
      '[&>[data-flux-control]~[data-flux-error]]:col-span-2 [&>[data-flux-control]~[data-flux-error]]:mt-1',
      '[&>[data-flux-label]~[data-flux-control]]:row-start-1 [&>[data-flux-label]~[data-flux-control]]:col-start-2',
    ] : 'block',
  props.variant === 'bare' || props.variant === 'inline' ? '' : '*:data-flux-label:mb-3 [&>[data-flux-label]:has(+[data-flux-description])]:mb-2',
  props.variant === 'bare' || props.variant === 'inline' ? '' : [
    '[&>[data-flux-label]+[data-flux-description]]:mt-0',
    '[&>[data-flux-label]+[data-flux-description]]:mb-3',
    '[&>[data-flux-description]:first-child:has(+*)]:mb-3',
    '[&>*:not([data-flux-label])+[data-flux-description]]:mt-3',
  ],
));
</script>

<template>
    <ui-field :class="classes" data-flux-field><slot /></ui-field>
</template>
