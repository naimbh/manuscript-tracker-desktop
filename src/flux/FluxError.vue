<script setup>
import { computed } from 'vue';
import FluxIcon from './FluxIcon.vue';
import { errorProps, useFieldError } from './errors.js';

const props = defineProps({
  ...errorProps,
  message: { type: [String, Array], default: undefined },
  icon: { type: String, default: 'exclamation-triangle' },
});
const fieldError = useFieldError(props);
const text = computed(() => {
  const m = Array.isArray(props.message) ? props.message[0] : props.message;
  return m || fieldError.value || null;
});
</script>

<template>
    <div role="alert" aria-live="polite" aria-atomic="true" :class="['mt-3 text-sm font-medium text-red-500 dark:text-red-400', text ? '' : 'hidden']" data-flux-error>
        <template v-if="text"><FluxIcon v-if="icon" :name="icon" variant="mini" class="inline" /> {{ text }}</template>
    </div>
</template>
