<script setup>
import { nextTick, onMounted, ref } from 'vue';
import { route } from '@/lib/router.js';
import { user } from '@/lib/site.js';

defineProps({ heading: { type: String, default: '' }, subheading: { type: String, default: '' } });
const nav = ref(null);
onMounted(() => nextTick(() => nav.value?.querySelector('[data-current]')?.scrollIntoView({ inline: 'center', block: 'nearest' })));
</script>

<template>
    <!-- The page scrolls with the window (no inner scroll box); the nav stays in view while the content scrolls. -->
    <div class="mx-auto flex w-full max-w-6xl shrink-0 items-start self-stretch max-md:flex-col">
        <div ref="nav" class="w-full select-none pb-4 md:sticky md:top-10 md:mr-10 md:w-[220px] md:shrink-0">
            <FluxNavlist class="max-md:-mx-4 max-md:flex max-md:w-[calc(100%+2rem)] max-md:flex-row max-md:gap-1 max-md:overflow-x-auto max-md:px-4 max-md:[&_[data-flux-navlist-item]]:shrink-0 max-md:[&_[data-flux-navlist-item]]:w-auto">
                <FluxNavlistItem v-if="user.is_admin" :href="route('settings.general')">General</FluxNavlistItem>
                <FluxNavlistItem :href="route('settings.profile')">Profile</FluxNavlistItem>
                <FluxNavlistItem :href="route('settings.appearance')">Appearance</FluxNavlistItem>
                <FluxNavlistItem :href="route('settings.research')">Research profile</FluxNavlistItem>
                <FluxNavlistItem :href="route('settings.ai')">AI</FluxNavlistItem>
                <FluxNavlistItem :href="route('settings.agents')">Agents (MCP)</FluxNavlistItem>
                <FluxNavlistItem :href="route('settings.data')">Backup &amp; calendar</FluxNavlistItem>
                <FluxNavlistItem :href="route('settings.about')">About</FluxNavlistItem>
            </FluxNavlist>
        </div>

        <FluxSeparator class="md:hidden" />

        <!-- Fixed column: its position and width never depend on the page's content (long paths wrap, wide boxes scroll inside). -->
        <div class="min-w-0 flex-1 basis-0 self-stretch max-md:w-full max-md:pt-6">
            <FluxHeading>{{ heading }}</FluxHeading>
            <FluxSubheading>{{ subheading }}</FluxSubheading>

            <div class="mt-5 w-full max-w-2xl min-w-0 pb-10 [overflow-wrap:anywhere]">
                <slot />
            </div>
        </div>
    </div>
</template>
