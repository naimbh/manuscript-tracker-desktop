<script setup>
import FluxMenuSeparator from './FluxMenuSeparator.vue';
import FluxMenuHeading from './FluxMenuHeading.vue';

defineProps({ heading: { type: String, default: null } });
</script>

<template>
    <div class="-mx-[.3125rem] px-[.3125rem] [&+&>[data-flux-menu-separator-top]]:hidden [&:first-child>[data-flux-menu-separator-top]]:hidden [&:last-child>[data-flux-menu-separator-bottom]]:hidden" role="group" data-flux-menu-group>
        <FluxMenuSeparator data-flux-menu-separator-top />
        <FluxMenuHeading v-if="heading">{{ heading }}</FluxMenuHeading>
        <slot />
        <FluxMenuSeparator data-flux-menu-separator-bottom />
    </div>
</template>
