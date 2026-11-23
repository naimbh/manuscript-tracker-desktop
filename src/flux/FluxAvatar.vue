<script setup>
import { computed } from 'vue';
import FluxIcon from './FluxIcon.vue';
import { crc32, cx } from './util.js';

const props = defineProps({
  src: { type: String, default: null },
  name: { type: String, default: null },
  initials: { type: String, default: null },
  icon: { type: String, default: null },
  iconVariant: { type: String, default: 'solid' },
  size: { type: String, default: 'md' },
  circle: { type: Boolean, default: false },
  color: { type: String, default: null },
  alt: { type: String, default: null },
});

const COLORS = ['red', 'orange', 'amber', 'yellow', 'lime', 'green', 'emerald', 'teal', 'cyan', 'sky', 'blue', 'indigo', 'violet', 'purple', 'fuchsia', 'pink', 'rose'];
const BG = {
  red: 'bg-red-200 text-red-800', orange: 'bg-orange-200 text-orange-800', amber: 'bg-amber-200 text-amber-800', yellow: 'bg-yellow-200 text-yellow-800',
  lime: 'bg-lime-200 text-lime-800', green: 'bg-green-200 text-green-800', emerald: 'bg-emerald-200 text-emerald-800', teal: 'bg-teal-200 text-teal-800',
  cyan: 'bg-cyan-200 text-cyan-800', sky: 'bg-sky-200 text-sky-800', blue: 'bg-blue-200 text-blue-800', indigo: 'bg-indigo-200 text-indigo-800',
  violet: 'bg-violet-200 text-violet-800', purple: 'bg-purple-200 text-purple-800', fuchsia: 'bg-fuchsia-200 text-fuchsia-800', pink: 'bg-pink-200 text-pink-800',
  rose: 'bg-rose-200 text-rose-800',
};
const initialsText = computed(() => {
  if (props.initials || !props.name) return props.initials;
  const parts = props.name.trim().split(' ').filter(Boolean);
  if (parts.length > 1) return ([...parts[0]][0] + [...parts[1]][0]).toUpperCase();
  if (parts.length === 1) return ([...parts[0]][0] || '').toUpperCase() + ([...parts[0]][1] || '').toLowerCase();
  return null;
});
const iconName = computed(() => props.icon ?? (initialsText.value ? null : 'user'));
const color = computed(() => (props.color === 'auto' ? COLORS[crc32(String(props.name ?? props.icon ?? initialsText.value ?? '')) % COLORS.length] : props.color));
const classes = computed(() => cx(
  { xl: '[:where(&)]:size-16 [:where(&)]:text-base', lg: '[:where(&)]:size-12 [:where(&)]:text-base', sm: '[:where(&)]:size-8 [:where(&)]:text-sm', xs: '[:where(&)]:size-6 [:where(&)]:text-xs' }[props.size] || '[:where(&)]:size-10 [:where(&)]:text-sm',
  props.circle ? '[--avatar-radius:calc(infinity*1px)]' : { xl: '[--avatar-radius:var(--radius-xl)]', lg: '[--avatar-radius:var(--radius-lg)]', sm: '[--avatar-radius:var(--radius-md)]', xs: '[--avatar-radius:var(--radius-sm)]' }[props.size] || '[--avatar-radius:var(--radius-lg)]',
  'relative flex-none isolate flex items-center justify-center',
  '[:where(&)]:font-medium',
  'rounded-[var(--avatar-radius)]',
  '[:where(&)]:bg-zinc-200 [:where(&)]:dark:bg-zinc-600 [:where(&)]:text-zinc-800 [:where(&)]:dark:text-white',
  BG[color.value] || '',
  ['after:absolute after:inset-0 after:inset-ring-[1px] after:inset-ring-black/7 dark:after:inset-ring-white/10', props.circle ? 'after:rounded-full' : { xl: 'after:rounded-xl', lg: 'after:rounded-lg', sm: 'after:rounded-md', xs: 'after:rounded-sm' }[props.size] || 'after:rounded-lg'],
));
const iconClasses = computed(() => cx('opacity-75', { lg: 'size-8', sm: 'size-5', xs: 'size-4' }[props.size] || 'size-6'));
</script>

<template>
    <div :class="classes" :data-circle="circle ? 'true' : undefined" data-flux-avatar data-slot="avatar" :data-size="size">
        <img v-if="src" :src="src" :alt="alt ?? name ?? ''" class="rounded-[var(--avatar-radius)] size-full object-cover">
        <FluxIcon v-else-if="iconName" :name="iconName" :variant="iconVariant" :class="iconClasses" />
        <span v-else class="select-none">{{ initialsText }}<slot /></span>
    </div>
</template>
