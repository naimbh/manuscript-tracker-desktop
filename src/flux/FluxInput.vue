<script setup>
import { computed, ref, useAttrs, useSlots } from 'vue';
import FluxIcon from './FluxIcon.vue';
import FluxButton from './FluxButton.vue';
import FluxWithField from './FluxWithField.vue';
import FluxDatePicker from './FluxDatePicker.vue';
import { inputClasses } from './inputClasses.js';
import { errorProps, useFieldError } from './errors.js';
import { cx } from './util.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  ...errorProps,
  modelValue: { type: [String, Number], default: undefined },
  modelModifiers: { type: Object, default: () => ({}) },
  type: { type: String, default: 'text' },
  label: { type: String, default: null },
  description: { type: String, default: null },
  descriptionTrailing: { type: String, default: null },
  badge: { type: String, default: null },
  variant: { type: String, default: 'outline' },
  size: { type: String, default: null },
  icon: { type: String, default: null },
  iconLeading: { type: String, default: null },
  iconTrailing: { type: String, default: null },
  iconVariant: { type: String, default: 'mini' },
  kbd: { type: String, default: null },
  clearable: { type: Boolean, default: false },
  viewable: { type: Boolean, default: false },
  copyable: { type: Boolean, default: false },
  invalid: { type: Boolean, default: false },
  loading: { type: Boolean, default: false },
  as: { type: String, default: null },
});
const emit = defineEmits(['update:modelValue']);
const attrs = useAttrs();
const slots = useSlots();
const input = ref(null);
const viewOpen = ref(false);
const copied = ref(false);

const message = useFieldError(props);
const leading = computed(() => props.iconLeading ?? props.icon);
const trailingCount = computed(() => [props.iconTrailing || slots.iconTrailing, props.kbd, props.clearable, props.copyable, props.viewable].filter(Boolean).length);
const iconClasses = computed(() => (props.iconVariant === 'outline' ? 'size-5' : ''));
const PE = ['pe-3', 'pe-10', 'pe-16', 'pe-23', 'pe-30', 'pe-37', 'pe-44'];
const PE_LOADING = ['pe-10', 'pe-16', 'pe-23', 'pe-30', 'pe-37', 'pe-44', 'pe-51'];

const classes = computed(() => cx(
  inputClasses({ size: props.size, variant: props.variant }),
  leading.value || slots.iconLeading ? 'ps-10' : 'ps-3',
  PE[trailingCount.value],
  attrs['class:input'],
  props.loading ? PE_LOADING[trailingCount.value] : '',
));

const isInvalid = computed(() => props.invalid || !!message.value);
const inputType = computed(() => (props.viewable && viewOpen.value ? 'text' : props.type));

const inputAttrs = computed(() => {
  const rest = {};
  for (const [k, v] of Object.entries(attrs)) if (k !== 'class' && !k.includes(':')) rest[k] = v;
  return rest;
});

function cast(v) {
  if (props.modelModifiers.trim) v = v.trim();
  if (props.modelModifiers.number || props.type === 'number') {
    if (props.modelModifiers.number) { const n = parseFloat(v); return isNaN(n) ? v : n; }
  }
  return v;
}
function onInput(e) { if (!props.modelModifiers.lazy) emit('update:modelValue', cast(e.target.value)); }
function onChange(e) { if (props.modelModifiers.lazy) emit('update:modelValue', cast(e.target.value)); }

function clear() {
  emit('update:modelValue', '');
  if (input.value) {
    input.value.value = '';
    input.value.dispatchEvent(new Event('input', { bubbles: true }));
    input.value.focus();
  }
}
async function copy() {
  try { await navigator.clipboard.writeText(input.value?.value ?? ''); copied.value = true; setTimeout(() => (copied.value = false), 2000); } catch {}
}
// type="date" renders FluxDatePicker (the native picker is tiny in WKWebView and differs per platform).
const picker = ref(null);
const pickerProps = computed(() => ({
  ...attrs,
  modelValue: props.modelValue ?? null,
  name: props.name, error: props.error, errors: props.errors,
  label: props.label, description: props.description, descriptionTrailing: props.descriptionTrailing, badge: props.badge,
  variant: props.variant, size: props.size, invalid: props.invalid,
}));
defineExpose({ focus: () => (picker.value ?? input.value)?.focus(), el: input });
</script>

<template>
    <button v-if="as === 'button'" type="button" v-bind="inputAttrs" :class="[classes, 'w-full relative flex', attrs.class]">
        <div v-if="leading" class="absolute top-0 bottom-0 flex items-center justify-center text-xs text-zinc-400/75 ps-3 start-0"><FluxIcon :icon="leading" :variant="iconVariant" :class="iconClasses" /></div>
        <div v-if="attrs.placeholder != null" class="block self-center text-start flex-1 font-medium text-zinc-400 dark:text-white/40">{{ attrs.placeholder }}</div>
        <div v-else class="text-start self-center flex-1 font-medium text-zinc-800 dark:text-white"><slot /></div>
        <div v-if="kbd" class="absolute top-0 bottom-0 flex items-center justify-center text-xs text-zinc-400/75 pe-4 end-0">{{ kbd }}</div>
        <div v-if="iconTrailing" class="absolute top-0 bottom-0 flex items-center justify-center text-xs text-zinc-400/75 pe-3 end-0"><FluxIcon :icon="iconTrailing" :variant="iconVariant" :class="iconClasses" /></div>
    </button>
    <FluxDatePicker v-else-if="type === 'date'" ref="picker" v-bind="pickerProps" @update:model-value="emit('update:modelValue', $event)" />
    <FluxWithField v-else :label="label" :description="description" :description-trailing="descriptionTrailing" :badge="badge" :error="message">
        <div :class="['w-full relative block group/input', attrs.class]" data-flux-input>
            <div v-if="leading" class="pointer-events-none absolute top-0 bottom-0 border-s border-transparent flex items-center justify-center text-xs text-zinc-400/75 dark:text-white/60 ps-3 start-0"><FluxIcon :icon="leading" :variant="iconVariant" :class="iconClasses" /></div>
            <div v-else-if="$slots.iconLeading" class="absolute top-0 bottom-0 border-s border-transparent flex items-center justify-center text-xs text-zinc-400/75 dark:text-white/60 ps-3 start-0"><slot name="iconLeading" /></div>
            <input
                ref="input"
                :type="inputType"
                :class="classes"
                v-bind="inputAttrs"
                :value="modelValue ?? (attrs.value ?? '')"
                :name="name"
                :aria-invalid="isInvalid ? 'true' : undefined"
                :data-invalid="isInvalid ? '' : undefined"
                data-flux-control
                data-flux-group-target
                @input="onInput"
                @change="onChange"
            >
            <div v-if="loading || trailingCount > 0" class="absolute top-0 bottom-0 flex items-center gap-x-1.5 pe-2 border-e border-transparent end-0 text-xs text-zinc-400">
                <FluxIcon v-if="loading" name="loading" :variant="iconVariant" :class="iconClasses" />
                <FluxButton v-if="clearable" variant="subtle" square inset="left right" :size="size === 'sm' || size === 'xs' ? 'xs' : 'sm'" class="-me-1 [[data-flux-input]:has(input:placeholder-shown)_&]:hidden [[data-flux-input]:has(input[disabled])_&]:hidden" tabindex="-1" aria-label="Clear input" data-flux-clear-button @click="clear"><FluxIcon name="x-mark" :variant="iconVariant" /></FluxButton>
                <span v-if="kbd" class="pointer-events-none last:pe-2">{{ kbd }}</span>
                <FluxButton v-if="copyable" variant="subtle" square inset="left right" :size="size === 'sm' || size === 'xs' ? 'xs' : 'sm'" class="-me-1" :data-copyable-copied="copied ? '' : undefined" aria-label="Copy to clipboard" @click="copy"><FluxIcon name="clipboard-document-check" :variant="iconVariant" class="hidden [[data-copyable-copied]>&]:block" /><FluxIcon name="clipboard-document" :variant="iconVariant" class="block [[data-copyable-copied]>&]:hidden" /></FluxButton>
                <FluxButton v-if="viewable" variant="subtle" square inset="left right" :size="size === 'sm' || size === 'xs' ? 'xs' : 'sm'" class="-me-1" :data-viewable-open="viewOpen ? '' : undefined" aria-label="Toggle password visibility" @click="viewOpen = !viewOpen"><FluxIcon name="eye-slash" :variant="iconVariant" class="hidden [[data-viewable-open]>&]:block" /><FluxIcon name="eye" :variant="iconVariant" class="block [[data-viewable-open]>&]:hidden" /></FluxButton>
                <FluxIcon v-if="iconTrailing" :icon="iconTrailing" :variant="iconVariant" :class="[iconClasses, 'text-zinc-400/75 dark:text-white/60 pointer-events-none']" />
                <slot v-else name="iconTrailing" />
            </div>
        </div>
    </FluxWithField>
</template>
