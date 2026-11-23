<script setup>
import { computed, inject, useSlots } from 'vue';
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
  as: { type: String, default: null },
  type: { type: String, default: 'button' },
});
const slots = useSlots();
const navVariant = inject('fluxNavVariant', computed(() => null));
const isCurrent = useCurrent(props);
const square = computed(() => !hasContent(slots.default));
const iconClasses = computed(() => (square.value ? 'size-5!' : 'size-4!'));
const tag = computed(() => (props.as === 'div' && !props.href ? 'div' : props.as === 'a' || props.href ? 'a' : 'button'));

const classes = computed(() => cx(
  'h-10 lg:h-8 relative flex items-center gap-3 rounded-lg',
  square.value ? 'px-2.5!' : '',
  'py-0 text-start w-full px-3 my-px',
  'text-zinc-500 dark:text-white/80',
  navVariant.value === 'outline'
    ? (props.accent ? [
      'data-current:text-(--color-accent-content) hover:data-current:text-(--color-accent-content)',
      'data-current:bg-white dark:data-current:bg-white/[7%] data-current:border data-current:border-zinc-200 dark:data-current:border-transparent',
      'hover:text-zinc-800 dark:hover:text-white dark:hover:bg-white/[7%] hover:bg-zinc-800/5 ',
      'border border-transparent',
    ] : [
      'data-current:text-zinc-800 dark:data-current:text-zinc-100 data-current:border-zinc-200',
      'data-current:bg-white dark:data-current:bg-white/10 data-current:border data-current:border-zinc-200 dark:data-current:border-white/10 data-current:shadow-xs',
      'hover:text-zinc-800 dark:hover:text-white',
    ])
    : (props.accent ? [
      'data-current:text-(--color-accent-content) hover:data-current:text-(--color-accent-content)',
      'data-current:bg-zinc-800/[4%] dark:data-current:bg-white/[7%]',
      'hover:text-zinc-800 dark:hover:text-white hover:bg-zinc-800/[4%] dark:hover:bg-white/[7%]',
    ] : [
      'data-current:text-zinc-800 dark:data-current:text-zinc-100',
      'data-current:bg-zinc-800/[4%] dark:data-current:bg-white/10',
      'hover:text-zinc-800 dark:hover:text-white hover:bg-zinc-800/[4%] dark:hover:bg-white/10',
    ]),
));
</script>

<template>
    <component
        :is="tag"
        :href="tag === 'a' ? href : undefined"
        :type="tag === 'button' ? type : undefined"
        :data-current="isCurrent ? '' : undefined"
        :aria-current="isCurrent && tag === 'a' ? 'page' : undefined"
        :class="classes"
        data-flux-navlist-item
    >
        <div v-if="icon || slots.icon" class="relative">
            <FluxIcon v-if="icon" :icon="icon" :variant="iconVariant" :class="iconClasses" />
            <slot v-else name="icon" />
            <div v-if="iconDot" class="absolute top-[-2px] end-[-2px]"><div class="size-[6px] rounded-full bg-zinc-500 dark:bg-zinc-400"></div></div>
        </div>
        <div v-if="!square" class="flex-1 text-sm font-medium leading-none whitespace-nowrap [[data-nav-footer]_&]:hidden [[data-nav-sidebar]_[data-nav-footer]_&]:block" data-content><slot /></div>
        <div v-if="iconDot && !icon && iconTrailing" class="relative">
            <FluxIcon :icon="iconTrailing" :variant="iconVariant" class="size-4!" />
            <div class="absolute top-[-2px] end-[-2px]"><div class="size-[6px] rounded-full bg-zinc-500 dark:bg-zinc-400"></div></div>
        </div>
        <FluxIcon v-else-if="iconTrailing" :icon="iconTrailing" :variant="iconVariant" class="size-4!" />
        <FluxNavlistBadge v-if="badge !== null && badge !== undefined && badge !== ''" :color="badgeColor" data-flux-navlist-badge>{{ badge }}</FluxNavlistBadge>
    </component>
</template>
