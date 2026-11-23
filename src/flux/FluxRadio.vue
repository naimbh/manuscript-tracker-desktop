<script setup>
import { computed, inject, useAttrs } from 'vue';
import FluxIcon from './FluxIcon.vue';
import FluxHeading from './FluxHeading.vue';
import FluxSubheading from './FluxSubheading.vue';
import FluxRadioIndicator from './FluxRadioIndicator.vue';
import FluxWithField from './FluxWithField.vue';
import { cx, hasContent } from './util.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  value: { type: [String, Number, Boolean], default: undefined },
  label: { type: String, default: null },
  description: { type: String, default: null },
  icon: { type: String, default: null },
  iconTrailing: { type: String, default: null },
  iconVariant: { type: String, default: null },
  checked: { type: Boolean, default: undefined },
  disabled: { type: Boolean, default: false },
  accent: { type: Boolean, default: true },
});
const attrs = useAttrs();
const group = inject('fluxRadioGroup', null);

const variant = computed(() => group?.variant.value ?? 'default');
const size = computed(() => group?.size.value ?? null);
const isChecked = computed(() => (group ? group.value.value == props.value && group.value.value !== undefined && group.value.value !== null : !!props.checked));
const isDisabled = computed(() => props.disabled || !!group?.disabled.value);
const indicatorPos = computed(() => {
  const i = group?.indicator.value ?? true;
  if (i === 'start' || i === 'left') return 'start';
  return i ? 'end' : false;
});

function pick() {
  if (isDisabled.value) return;
  group?.select(props.value);
}
function onKey(e) {
  if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); pick(); }
}

const common = computed(() => ({
  ...Object.fromEntries(Object.entries(attrs).filter(([k]) => k !== 'class')),
  role: 'radio',
  value: props.value,
  'aria-checked': isChecked.value ? 'true' : 'false',
  disabled: isDisabled.value ? '' : undefined,
  'data-checked': isChecked.value ? '' : undefined,
  tabindex: isChecked.value && !isDisabled.value ? 0 : -1,
  onClick: pick,
  onKeydown: onKey,
}));

const segmentedClasses = computed(() => cx(
  'flex whitespace-nowrap flex-1 justify-center items-center gap-2',
  'rounded-md data-checked:shadow-xs',
  'text-sm font-medium text-zinc-600 hover:text-zinc-800 dark:hover:text-white dark:text-white/70 data-checked:text-zinc-800 dark:data-checked:text-white',
  'data-checked:bg-white dark:data-checked:bg-white/20',
  '[&[disabled]]:opacity-50 dark:[&[disabled]]:opacity-75 [&[disabled]]:cursor-default [&[disabled]]:pointer-events-none',
  size.value === 'sm' ? 'px-3 text-sm' : 'px-4',
));
const segIconVariant = computed(() => props.iconVariant ?? 'mini');
const segIconClasses = computed(() => cx('text-zinc-500 dark:text-zinc-400 [ui-radio[data-checked]_&]:text-zinc-800 dark:[ui-radio[data-checked]_&]:text-white', segIconVariant.value === 'outline' ? 'size-5' : ''));

const cardIconVariant = computed(() => props.iconVariant ?? 'micro');
const cardIconClasses = computed(() => cx('inline-block mt-0.5 text-zinc-400 [ui-radio[data-checked]_&]:text-zinc-800 dark:[ui-radio[data-checked]_&]:text-white', cardIconVariant.value === 'outline' ? 'size-4' : ''));
const cardClasses = computed(() => cx(
  'relative flex gap-3 flex-1 p-4',
  indicatorPos.value === 'start' ? 'justify-start' : 'justify-between',
  'rounded-lg shadow-xs',
  'bg-white dark:bg-white/10 dark:hover:bg-white/15 dark:data-checked:bg-white/15',
  'after:absolute after:-inset-px after:rounded-lg',
  'border border-zinc-800/15 dark:border-white/10',
  [
    '[--haze:color-mix(in_oklab,_var(--color-accent-content),_transparent_97.5%)]',
    '[--haze-border:color-mix(in_oklab,_var(--color-accent-content),_transparent_80%)]',
    '[--haze-light:color-mix(in_oklab,_var(--color-accent),_transparent_98%)]',
    'dark:[--haze:color-mix(in_oklab,_var(--color-accent-content),_transparent_90%)]',
  ],
  props.accent ? [
    '[&:hover_[data-flux-radio-indicator]]:border-[var(--haze-border)] dark:[&:hover_[data-flux-radio-indicator]]:border-white/10',
    'hover:border-[var(--haze-border)] dark:hover:border-white/10',
    'data-checked:border-(--color-accent) hover:data-checked:border-(--color-accent) dark:data-checked:bg-white/15 ',
    'hover:after:bg-[var(--haze-light)] dark:hover:after:bg-white/[4%] data-checked:after:bg-(--haze) hover:data-checked:after:bg-(--haze)',
  ] : [
    'data-checked:bg-zinc-50 dark:data-checked:bg-white/15 data-checked:border-zinc-800 dark:data-checked:border-white',
    'hover:bg-zinc-50 dark:hover:bg-white/15',
  ],
  '[&[disabled]]:opacity-50 dark:[&[disabled]]:opacity-75 [&[disabled]]:cursor-default [&[disabled]]:pointer-events-none',
));
</script>

<template>
    <ui-radio v-if="variant === 'segmented'" v-bind="common" :class="[segmentedClasses, attrs.class]" data-flux-control data-flux-radio-segmented>
        <FluxIcon v-if="icon" :icon="icon" :variant="segIconVariant" :class="segIconClasses" />
        <slot v-if="hasContent($slots.default)" /><template v-else>{{ label }}</template>
        <FluxIcon v-if="iconTrailing" :icon="iconTrailing" variant="micro" />
    </ui-radio>
    <ui-radio v-else-if="variant === 'cards'" v-bind="common" :class="[cardClasses, attrs.class]" data-flux-control data-flux-radio-cards data-flux-field>
        <template v-if="label || hasContent($slots.default) && !$slots.custom">
            <FluxRadioIndicator v-if="indicatorPos === 'start'" class="mt-px" />
            <div class="flex-1 flex gap-2">
                <FluxIcon v-if="icon" :icon="icon" :variant="cardIconVariant" :class="cardIconClasses" />
                <div class="flex-1">
                    <FluxHeading><slot v-if="hasContent($slots.default)" /><template v-else>{{ label }}</template></FluxHeading>
                    <FluxSubheading v-if="description" size="sm">{{ description }}</FluxSubheading>
                </div>
            </div>
            <FluxRadioIndicator v-if="indicatorPos === 'end'" class="mt-px" />
        </template>
        <slot v-else name="custom" />
    </ui-radio>
    <FluxWithField v-else mode="inline" :label="label" :description="description">
        <ui-radio v-bind="common" :class="['flex size-[1.125rem] rounded-full mt-px outline-offset-2', attrs.class]" data-flux-control data-flux-radio>
            <FluxRadioIndicator />
        </ui-radio>
    </FluxWithField>
</template>
