<script setup>
defineProps({ value: { type: [String, Number, Boolean, null], default: undefined } });
</script>

<template>
    <option :value="value"><slot /></option>
</template>
