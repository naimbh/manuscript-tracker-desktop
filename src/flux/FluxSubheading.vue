<script setup>
defineProps({ size: { type: String, default: 'base' } });
</script>

<template>
    <div :class="[{ xl: 'text-lg', lg: 'text-base', sm: 'text-xs' }[size] || 'text-sm', '[:where(&)]:text-zinc-500 [:where(&)]:dark:text-white/70']" data-flux-subheading><slot /></div>
</template>
