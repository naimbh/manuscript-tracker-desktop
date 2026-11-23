<script setup>
import FluxButton from './FluxButton.vue';
import { sidebarAttrs, toggleSidebar } from './sidebar.js';
</script>

<template>
    <FluxButton class="shrink-0" variant="subtle" square aria-label="Toggle sidebar" data-flux-sidebar-toggle v-bind="sidebarAttrs()" @click="toggleSidebar" />
</template>
