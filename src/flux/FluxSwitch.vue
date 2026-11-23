<script setup>
import { computed, useAttrs } from 'vue';
import FluxWithField from './FluxWithField.vue';
import { errorProps, useFieldError } from './errors.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  ...errorProps,
  modelValue: { type: [Boolean, null], default: undefined },
  checked: { type: Boolean, default: undefined },
  align: { type: String, default: 'right' },
  disabled: { type: Boolean, default: false },
  label: { type: String, default: null },
  description: { type: String, default: null },
});
const emit = defineEmits(['update:modelValue', 'change']);
const attrs = useAttrs();
const message = useFieldError(props);
const isOn = computed(() => (props.modelValue !== undefined && props.modelValue !== null ? !!props.modelValue : !!props.checked));

function toggle() {
  if (props.disabled) return;
  emit('update:modelValue', !isOn.value);
  emit('change', !isOn.value);
}
function onKey(e) { if (e.key === ' ' || e.key === 'Enter') { e.preventDefault(); toggle(); } }
</script>

<template>
    <FluxWithField :mode="align === 'left' || align === 'start' ? 'inline' : 'reversed'" :label="label" :description="description" :error="message">
        <ui-switch
            v-bind="attrs"
            class="group h-5 w-8 min-w-8 relative inline-flex items-center outline-offset-2 rounded-full transition bg-zinc-800/15 [&[disabled]]:opacity-50 dark:bg-transparent dark:border dark:border-white/20 dark:[&[disabled]]:border-white/10 data-loading:opacity-50 data-loading:pointer-events-none dark:data-loading:border-white/10 [print-color-adjust:exact] data-checked:bg-(--color-accent) data-checked:border-0"
            :name="name"
            role="switch"
            :tabindex="disabled ? -1 : 0"
            :aria-checked="isOn ? 'true' : 'false'"
            :disabled="disabled ? '' : undefined"
            :checked="isOn ? '' : undefined"
            :data-checked="isOn ? '' : undefined"
            data-flux-control
            data-flux-switch
            @click="toggle"
            @keydown="onKey"
        >
            <span class="size-3.5 rounded-full transition translate-x-[0.1875rem] dark:translate-x-[0.125rem] rtl:-translate-x-[0.1875rem] dark:rtl:-translate-x-[0.125rem] bg-white group-data-checked:translate-x-[0.9375rem] rtl:group-data-checked:-translate-x-[0.9375rem] dark:group-data-checked:translate-x-[0.9375rem] dark:rtl:group-data-checked:-translate-x-[0.9375rem] group-data-checked:bg-(--color-accent-foreground)"></span>
        </ui-switch>
    </FluxWithField>
</template>
