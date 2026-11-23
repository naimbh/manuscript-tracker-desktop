<script setup>
import { computed, provide, ref, useAttrs } from 'vue';
import FluxWithField from './FluxWithField.vue';
import { errorProps, useFieldError } from './errors.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  ...errorProps,
  modelValue: { type: [String, Number, Boolean, null], default: undefined },
  variant: { type: String, default: 'default' }, // default | segmented | cards
  size: { type: String, default: null },
  indicator: { type: [Boolean, String], default: true },
  disabled: { type: Boolean, default: false },
  label: { type: String, default: null },
  description: { type: String, default: null },
  descriptionTrailing: { type: String, default: null },
  badge: { type: String, default: null },
});
const emit = defineEmits(['update:modelValue', 'change']);
const attrs = useAttrs();
const message = useFieldError(props);
const local = ref(props.modelValue);
const el = ref(null);

const value = computed(() => (props.modelValue !== undefined ? props.modelValue : local.value));

function select(v) {
  if (props.disabled) return;
  local.value = v;
  emit('update:modelValue', v);
  emit('change', v);
}

provide('fluxRadioGroup', {
  value,
  select,
  variant: computed(() => props.variant),
  size: computed(() => props.size),
  indicator: computed(() => props.indicator),
  disabled: computed(() => props.disabled),
});

function onKeydown(e) {
  const keys = ['ArrowDown', 'ArrowRight', 'ArrowUp', 'ArrowLeft'];
  if (!keys.includes(e.key)) return;
  const radios = [...el.value.querySelectorAll('ui-radio:not([disabled])')];
  if (!radios.length) return;
  e.preventDefault();
  let i = radios.findIndex((r) => r.hasAttribute('data-checked'));
  i = e.key === 'ArrowDown' || e.key === 'ArrowRight' ? (i + 1) % radios.length : (i - 1 + radios.length) % radios.length;
  radios[i].click();
  radios[i].focus();
}
</script>

<template>
    <FluxWithField :label="label" :description="description" :description-trailing="descriptionTrailing" :badge="badge" :error="message">
        <ui-radio-group
            v-if="variant === 'segmented'"
            ref="el"
            v-bind="attrs"
            :class="['block flex p-1', 'rounded-lg bg-zinc-800/5 dark:bg-white/10', size === 'sm' ? 'h-8 py-[3px] px-[3px]' : 'h-10 p-1', size === 'sm' ? '-my-px h-[calc(2rem+2px)]' : '']"
            :name="name"
            role="radiogroup"
            :disabled="disabled ? '' : undefined"
            data-flux-radio-group-segmented
            @keydown="onKeydown"
        ><slot /></ui-radio-group>
        <ui-radio-group
            v-else-if="variant === 'cards'"
            ref="el"
            v-bind="attrs"
            class="flex gap-3"
            :name="name"
            role="radiogroup"
            :disabled="disabled ? '' : undefined"
            data-flux-radio-group-cards
            @keydown="onKeydown"
        ><slot /></ui-radio-group>
        <ui-radio-group
            v-else
            ref="el"
            v-bind="attrs"
            class="*:data-flux-field:mb-3 [&>[data-flux-field]:has(>[data-flux-description])]:mb-4 [&>[data-flux-field]:last-child]:mb-0!"
            :name="name"
            role="radiogroup"
            :disabled="disabled ? '' : undefined"
            data-flux-radio-group
            @keydown="onKeydown"
        ><slot /></ui-radio-group>
    </FluxWithField>
</template>
