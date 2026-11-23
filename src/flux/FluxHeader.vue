<script setup>
defineProps({ sticky: { type: Boolean, default: false }, container: { type: Boolean, default: false } });
</script>

<template>
    <header :class="['[grid-area:header]', 'z-10 min-h-14', container ? '' : 'flex items-center px-6 lg:px-8']" :style="sticky ? { position: 'sticky', top: '0px' } : undefined" data-flux-header>
        <div v-if="container" class="mx-auto w-full h-full [:where(&)]:max-w-7xl px-6 lg:px-8 flex items-center"><slot /></div>
        <slot v-else />
    </header>
</template>
