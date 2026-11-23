<script setup>
// Internal: Flux's with-field / with-inline-field / with-reversed-inline-field wrappers.
import { useSlots } from 'vue';
import FluxField from './FluxField.vue';
import FluxLabel from './FluxLabel.vue';
import FluxDescription from './FluxDescription.vue';
import FluxError from './FluxError.vue';

defineProps({
  mode: { type: String, default: 'block' }, // block | inline | reversed
  label: { type: String, default: null },
  description: { type: String, default: null },
  descriptionTrailing: { type: String, default: null },
  badge: { type: String, default: null },
  error: { type: String, default: null },
});
const slots = useSlots();
</script>

<template>
    <template v-if="mode === 'block'">
        <FluxField v-if="label != null || description != null || descriptionTrailing != null || slots.label">
            <FluxLabel v-if="label != null || slots.label" :badge="badge"><slot name="label">{{ label }}</slot></FluxLabel>
            <FluxDescription v-if="description != null">{{ description }}</FluxDescription>
            <slot />
            <FluxError :message="error || undefined" />
            <FluxDescription v-if="descriptionTrailing != null">{{ descriptionTrailing }}</FluxDescription>
        </FluxField>
        <slot v-else />
    </template>
    <template v-else-if="mode === 'inline'">
        <FluxField v-if="label || description || slots.label" variant="inline">
            <slot />
            <FluxLabel v-if="label || slots.label"><slot name="label">{{ label }}</slot></FluxLabel>
            <FluxDescription v-if="description">{{ description }}</FluxDescription>
            <FluxError :message="error || undefined" />
        </FluxField>
        <slot v-else />
    </template>
    <template v-else>
        <FluxField v-if="label || description || slots.label" variant="inline">
            <FluxLabel v-if="label || slots.label"><slot name="label">{{ label }}</slot></FluxLabel>
            <FluxDescription v-if="description">{{ description }}</FluxDescription>
            <slot />
            <FluxError :message="error || undefined" />
        </FluxField>
        <slot v-else />
    </template>
</template>
