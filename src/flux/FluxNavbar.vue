<script setup>
import { computed, provide } from 'vue';

const props = defineProps({ scrollable: { type: Boolean, default: false }, variant: { type: String, default: null } });
provide('fluxNavVariant', computed(() => props.variant));
</script>

<template>
    <nav :class="['flex items-center gap-0.5 py-3', scrollable ? 'overflow-x-auto overflow-y-hidden' : '']" data-flux-navbar><slot /></nav>
</template>
