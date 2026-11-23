<script setup>
import { computed, useAttrs } from 'vue';
import FluxWithField from './FluxWithField.vue';
import { errorProps, useFieldError } from './errors.js';
import { cx } from './util.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  ...errorProps,
  modelValue: { type: [String, Number, Boolean, null], default: undefined },
  variant: { type: String, default: 'default' },
  label: { type: String, default: null },
  description: { type: String, default: null },
  descriptionTrailing: { type: String, default: null },
  badge: { type: String, default: null },
  placeholder: { type: String, default: null },
  size: { type: String, default: null },
  invalid: { type: Boolean, default: undefined },
});
const emit = defineEmits(['update:modelValue']);
const attrs = useAttrs();
const rest = computed(() => { const r = {}; for (const [k, v] of Object.entries(attrs)) if (k !== 'class' && k !== 'value' && !k.includes(':')) r[k] = v; return r; });
const message = useFieldError(props);
const isInvalid = computed(() => props.invalid ?? !!message.value);

const model = computed({
  get: () => (props.modelValue === null || props.modelValue === undefined ? '' : props.modelValue),
  set: (v) => emit('update:modelValue', v),
});

const classes = computed(() => cx(
  'appearance-none',
  '[:where(&)]:w-full ps-3 pe-10 block',
  props.size === 'sm' ? 'h-8 py-1.5 text-sm leading-[1.125rem] rounded-md'
    : props.size === 'xs' ? 'h-6 text-xs leading-[1.125rem] rounded-md'
      : 'h-10 py-2 text-base sm:text-sm leading-[1.375rem] rounded-lg',
  'shadow-xs border',
  'bg-white dark:bg-white/10 dark:disabled:bg-white/[7%]',
  'text-zinc-700 dark:text-zinc-300 disabled:text-zinc-500 dark:disabled:text-zinc-400',
  'has-[option.placeholder:checked]:text-zinc-400 dark:has-[option.placeholder:checked]:text-zinc-400',
  'dark:[&_option]:bg-zinc-700 dark:[&_option]:text-white',
  'disabled:shadow-none',
  isInvalid.value ? 'border border-red-500' : 'border border-zinc-200 border-b-zinc-300/80 dark:border-white/10',
));
</script>

<template>
    <FluxWithField :label="label" :description="description" :description-trailing="descriptionTrailing" :badge="badge" :error="message">
        <select
            v-model="model"
            v-bind="rest"
            :class="[classes, attrs.class]"
            :aria-invalid="isInvalid ? 'true' : undefined"
            :data-invalid="isInvalid ? '' : undefined"
            :name="name"
            data-flux-control
            data-flux-select-native
            data-flux-group-target
        >
            <option v-if="placeholder" value="" disabled selected class="placeholder">{{ placeholder }}</option>
            <slot />
        </select>
    </FluxWithField>
</template>
