<script setup>
import FluxIcon from './FluxIcon.vue';

defineProps({
  icon: { type: String, default: null },
  iconVariant: { type: String, default: 'mini' },
});
</script>

<template>
    <div class="flex items-center gap-2 text-sm font-medium" data-slot="heading">
        <FluxIcon v-if="icon" :icon="icon" :variant="iconVariant" class="inline-block size-5 text-[var(--callout-icon)] dark:text-[var(--callout-icon)]" />
        <slot />
    </div>
</template>
