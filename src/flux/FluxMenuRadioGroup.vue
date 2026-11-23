<script setup>
import { computed, provide, ref } from 'vue';

const props = defineProps({ modelValue: { type: [String, Number, Boolean, null], default: undefined } });
const emit = defineEmits(['update:modelValue', 'change']);
const local = ref(props.modelValue);
const value = computed(() => (props.modelValue !== undefined ? props.modelValue : local.value));
provide('fluxMenuRadioGroup', {
  value,
  select(v) { local.value = v; emit('update:modelValue', v); emit('change', v); },
});
</script>

<template>
    <ui-menu-radio-group role="group" data-flux-menu-radio-group><slot /></ui-menu-radio-group>
</template>
