<script setup>
import { computed, useSlots } from 'vue';
import FluxIcon from './FluxIcon.vue';
import FluxNavlistBadge from './FluxNavlistBadge.vue';
import { useCurrent } from './nav.js';
import { cx, hasContent } from './util.js';

const props = defineProps({
  href: { type: String, default: null },
  current: { type: Boolean, default: undefined },
  icon: { type: String, default: null },
  iconTrailing: { type: String, default: null },
  iconVariant: { type: String, default: 'outline' },
  iconDot: { type: Boolean, default: false },
  badge: { type: [String, Number], default: null },
  badgeColor: { type: String, default: null },
  accent: { type: Boolean, default: true },
  square: { type: Boolean, default: undefined },
  as: { type: String, default: null },
  type: { type: String, default: 'button' },
});
const slots = useSlots();
const isCurrent = useCurrent(props);
const isSquare = computed(() => props.square ?? !hasContent(slots.default));
const tag = computed(() => (props.as === 'div' && !props.href ? 'div' : props.as === 'a' || props.href ? 'a' : 'button'));
const classes = computed(() => cx(
  'px-3 h-8 flex items-center rounded-lg',
  'relative',
  isSquare.value ? 'px-2.5!' : '',
  'text-zinc-500 dark:text-white/80 ',
  'data-current:after:absolute data-current:after:-bottom-3 data-current:after:inset-x-0 data-current:after:h-[2px]',
  ['[--hover-fill:color-mix(in_oklab,_var(--color-accent-content),_transparent_90%)]'],
  props.accent ? [
    'hover:text-zinc-800 dark:hover:text-white',
    'data-current:text-(--color-accent-content) hover:data-current:text-(--color-accent-content) hover:bg-zinc-800/5 dark:hover:bg-white/10 hover:data-current:bg-(--hover-fill)',
    'data-current:after:bg-(--color-accent-content)',
  ] : [
    'hover:text-zinc-800 dark:hover:text-white',
    'data-current:text-zinc-800 dark:data-current:text-zinc-100 hover:bg-zinc-100 dark:hover:bg-white/10',
    'data-current:after:bg-zinc-800 dark:data-current:after:bg-white',
  ],
));
</script>

<template>
    <component
        :is="tag"
        :href="tag === 'a' ? href : undefined"
        :type="tag === 'button' ? type : undefined"
        :data-current="isCurrent ? '' : undefined"
        :class="classes"
        data-flux-navbar-items
    >
        <div v-if="icon" class="relative">
            <FluxIcon :icon="icon" :variant="iconVariant" class="size-5" />
            <div v-if="iconDot" class="absolute top-[-2px] end-[-2px]"><div class="size-[6px] rounded-full bg-zinc-500 dark:bg-zinc-400"></div></div>
        </div>
        <div v-if="hasContent(slots.default)" :class="[icon ? 'ms-3' : '', 'flex-1 text-sm font-medium leading-none whitespace-nowrap [[data-nav-footer]_&]:hidden [[data-nav-sidebar]_[data-nav-footer]_&]:block']" data-content><slot /></div>
        <div v-if="iconDot && !icon && iconTrailing" class="relative ms-1">
            <FluxIcon :icon="iconTrailing" :variant="iconVariant" class="size-5" />
            <div class="absolute top-[-2px] end-[-2px]"><div class="size-[6px] rounded-full bg-zinc-500 dark:bg-zinc-400"></div></div>
        </div>
        <FluxIcon v-else-if="iconTrailing" :icon="iconTrailing" :variant="iconVariant" class="size-5 ms-1" />
        <FluxNavlistBadge v-if="badge !== null && badge !== undefined && badge !== ''" :color="badgeColor" class="ms-2">{{ badge }}</FluxNavlistBadge>
    </component>
</template>
