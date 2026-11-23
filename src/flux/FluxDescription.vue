<script setup>
defineProps({ srOnly: { type: Boolean, default: false } });
</script>

<template>
    <ui-description :class="['text-sm text-zinc-500 dark:text-white/60', srOnly ? 'sr-only' : '']" data-flux-description><slot /></ui-description>
</template>
