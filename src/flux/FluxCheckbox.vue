<script setup>
import { computed, useAttrs } from 'vue';
import FluxIcon from './FluxIcon.vue';
import FluxWithField from './FluxWithField.vue';
import { errorProps, useFieldError } from './errors.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  ...errorProps,
  // Boolean, or an array when several checkboxes share one v-model (each with its own `value`).
  modelValue: { type: [Boolean, Array, null], default: undefined },
  value: { type: [String, Number, Boolean], default: undefined },
  checked: { type: Boolean, default: undefined },
  indeterminate: { type: Boolean, default: false },
  disabled: { type: Boolean, default: false },
  label: { type: String, default: null },
  description: { type: String, default: null },
});
const emit = defineEmits(['update:modelValue', 'change']);
const attrs = useAttrs();
const message = useFieldError(props);

const isChecked = computed(() => {
  if (Array.isArray(props.modelValue)) return props.modelValue.some((v) => v == props.value);
  if (props.modelValue !== undefined && props.modelValue !== null) return !!props.modelValue;
  return !!props.checked;
});

function toggle() {
  if (props.disabled) return;
  const next = !isChecked.value;
  if (Array.isArray(props.modelValue)) {
    const list = props.modelValue.filter((v) => v != props.value);
    if (next) list.push(props.value);
    emit('update:modelValue', list);
  } else {
    emit('update:modelValue', next);
  }
  emit('change', next);
}
function onKey(e) {
  if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); toggle(); }
}
</script>

<template>
    <FluxWithField mode="inline" :label="label" :description="description" :error="message">
        <ui-checkbox
            v-bind="attrs"
            class="flex size-[1.125rem] rounded-[.3rem] mt-px outline-offset-2"
            :name="name"
            :value="value"
            role="checkbox"
            :tabindex="disabled ? -1 : 0"
            :aria-checked="indeterminate ? 'mixed' : isChecked ? 'true' : 'false'"
            :disabled="disabled ? '' : undefined"
            :data-checked="isChecked ? '' : undefined"
            :data-indeterminate="indeterminate ? '' : undefined"
            data-flux-control
            data-flux-checkbox
            @click="toggle"
            @keydown="onKey"
        >
            <div class="shrink-0 size-[1.125rem] rounded-[.3rem] flex justify-center items-center text-sm text-zinc-700 dark:text-zinc-800 shadow-xs [ui-checkbox[disabled]_&]:opacity-75 [ui-checkbox[data-checked][disabled]_&]:opacity-50 [ui-checkbox[disabled]_&]:shadow-none [ui-checkbox[data-checked]_&]:shadow-none [ui-checkbox[data-indeterminate]_&]:shadow-none [ui-checkbox[data-checked]:not([data-indeterminate])_&>svg:first-child]:block [ui-checkbox[data-indeterminate]_&>svg:last-child]:block border border-zinc-300 dark:border-white/10 [ui-checkbox[disabled]_&]:border-zinc-200 dark:[ui-checkbox[disabled]_&]:border-white/5 [ui-checkbox[data-checked]_&]:border-transparent [ui-checkbox[data-indeterminate]_&]:border-transparent [ui-checkbox[disabled][data-checked]_&]:border-transparent [ui-checkbox[disabled][data-indeterminate]_&]:border-transparent [print-color-adjust:exact] bg-white dark:bg-white/10 [ui-checkbox[data-checked]_&]:bg-[var(--color-accent)] hover:[ui-checkbox[data-checked]_&]:bg-(--color-accent) focus:[ui-checkbox[data-checked]_&]:bg-(--color-accent) [ui-checkbox[data-indeterminate]_&]:bg-[var(--color-accent)] hover:[ui-checkbox[data-indeterminate]_&]:bg-(--color-accent) focus:[ui-checkbox[data-indeterminate]_&]:bg-(--color-accent)" data-flux-checkbox-indicator>
                <FluxIcon name="check" variant="micro" class="hidden text-[var(--color-accent-foreground)]" />
                <FluxIcon name="minus" variant="micro" class="hidden text-[var(--color-accent-foreground)]" />
            </div>
        </ui-checkbox>
    </FluxWithField>
</template>
