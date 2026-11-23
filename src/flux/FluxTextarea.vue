<script setup>
import { computed, ref, useAttrs } from 'vue';
import FluxWithField from './FluxWithField.vue';
import { errorProps, useFieldError } from './errors.js';
import { cx } from './util.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  ...errorProps,
  modelValue: { type: [String, Number, null], default: undefined },
  modelModifiers: { type: Object, default: () => ({}) },
  label: { type: String, default: null },
  description: { type: String, default: null },
  descriptionTrailing: { type: String, default: null },
  badge: { type: String, default: null },
  resize: { type: String, default: 'vertical' },
  rows: { type: [String, Number], default: 4 },
  invalid: { type: Boolean, default: false },
});
const emit = defineEmits(['update:modelValue']);
const attrs = useAttrs();
const rest = computed(() => { const r = {}; for (const [k, v] of Object.entries(attrs)) if (k !== 'class' && k !== 'value' && !k.includes(':')) r[k] = v; return r; });
const el = ref(null);
const message = useFieldError(props);
const isInvalid = computed(() => props.invalid || !!message.value);

const classes = computed(() => cx(
  'block p-3 w-full',
  'shadow-xs disabled:shadow-none border rounded-lg',
  'bg-white dark:bg-white/10 dark:disabled:bg-white/[7%]',
  props.resize ? ({ none: 'resize-none', both: 'resize', horizontal: 'resize-x', vertical: 'resize-y' }[props.resize] || 'resize-y') : 'resize-none',
  props.rows === 'auto' ? 'field-sizing-content' : '',
  'text-base sm:text-sm text-zinc-700 disabled:text-zinc-500 placeholder-zinc-400 disabled:placeholder-zinc-400/70 dark:text-zinc-300 dark:disabled:text-zinc-400 dark:placeholder-zinc-400 dark:disabled:placeholder-zinc-500',
  'border-zinc-200 border-b-zinc-300/80 dark:border-white/10',
  'data-invalid:shadow-none data-invalid:border-red-500 dark:data-invalid:border-red-500',
));

function value(e) { let v = e.target.value; if (props.modelModifiers.trim) v = v.trim(); return v; }
defineExpose({ focus: () => el.value?.focus(), el });
</script>

<template>
    <FluxWithField :label="label" :description="description" :description-trailing="descriptionTrailing" :badge="badge" :error="message">
        <textarea
            ref="el"
            v-bind="rest"
            :class="[classes, attrs.class]"
            :rows="rows"
            :name="name"
            :value="modelValue ?? attrs.value ?? ''"
            :aria-invalid="isInvalid ? 'true' : undefined"
            :data-invalid="isInvalid ? '' : undefined"
            data-flux-control
            data-flux-textarea
            @input="!modelModifiers.lazy && emit('update:modelValue', value($event))"
            @change="modelModifiers.lazy && emit('update:modelValue', value($event))"
        />
    </FluxWithField>
</template>
