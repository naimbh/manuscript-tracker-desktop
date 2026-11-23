<script setup>
import { computed, useSlots } from 'vue';
import FluxIcon from './FluxIcon.vue';
import { cx } from './util.js';

const props = defineProps({
  icon: { type: String, default: null },
  iconTrailing: { type: String, default: null },
  iconVariant: { type: String, default: 'mini' },
  variant: { type: String, default: 'default' },
  suffix: { type: String, default: null },
  kbd: { type: String, default: null },
  href: { type: String, default: null },
  as: { type: String, default: null },
  type: { type: String, default: 'button' },
});
const slots = useSlots();
const tag = computed(() => (props.as === 'div' && !props.href ? 'div' : props.as === 'a' || props.href ? 'a' : 'button'));
const suffix = computed(() => props.kbd || props.suffix);
const iconClasses = computed(() => cx('me-2', props.iconVariant === 'outline' ? 'size-5' : null));
const trailingClasses = computed(() => cx('ms-auto text-zinc-400 [[data-flux-menu-item-icon]:hover_&]:text-current', props.iconVariant === 'outline' ? 'size-5' : null));
const classes = computed(() => cx(
  'flex items-center px-2 py-1.5 w-full focus:outline-hidden select-none',
  'rounded-md',
  'text-start text-sm font-medium',
  '[&[disabled]]:opacity-50',
  props.variant === 'danger'
    ? ['text-zinc-800 data-active:text-red-600 data-active:bg-red-50 dark:text-white dark:data-active:bg-red-400/20 dark:data-active:text-red-400', '**:data-flux-menu-item-icon:text-zinc-400 dark:**:data-flux-menu-item-icon:text-white/60 [&[data-active]_[data-flux-menu-item-icon]]:text-current']
    : ['text-zinc-800 data-active:bg-zinc-50 dark:text-white dark:data-active:bg-zinc-600', '**:data-flux-menu-item-icon:text-zinc-400 dark:**:data-flux-menu-item-icon:text-white/60 [&[data-active]_[data-flux-menu-item-icon]]:text-current'],
));
</script>

<template>
    <component
        :is="tag"
        :href="tag === 'a' ? href : undefined"
        :type="tag === 'button' ? type : undefined"
        :class="classes"
        role="menuitem"
        data-flux-menu-item
        :data-flux-menu-item-has-icon="icon || slots.icon ? '' : undefined"
    >
        <FluxIcon v-if="icon" :icon="icon" :variant="iconVariant" :class="iconClasses" data-flux-menu-item-icon />
        <slot v-else-if="slots.icon" name="icon" />
        <div v-else class="w-7 hidden [[data-flux-menu]:has(>[data-flux-menu-item-has-icon])_&]:block"></div>
        <slot />
        <div v-if="suffix" class="ms-auto text-xs text-zinc-400">{{ suffix }}</div>
        <slot v-else name="suffix" />
        <FluxIcon v-if="iconTrailing" :icon="iconTrailing" :variant="iconVariant" :class="trailingClasses" data-flux-menu-item-icon />
        <slot name="submenu" />
    </component>
</template>
