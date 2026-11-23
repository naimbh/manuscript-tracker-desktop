<script setup>
import { computed, provide } from 'vue';

const props = defineProps({ variant: { type: String, default: null } });
provide('fluxNavVariant', computed(() => props.variant));
</script>

<template>
    <nav class="flex flex-col overflow-visible min-h-auto" data-flux-navlist><slot /></nav>
</template>
