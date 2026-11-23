<script setup>
import { appearance, setAppearance } from '@/lib/appearance.js';
</script>

<template>
    <div class="flex w-full flex-col">
        <div class="relative mx-auto mb-6 w-full max-w-6xl">
            <FluxHeading size="xl" :level="1" class="max-sm:!text-xl">Settings</FluxHeading>
            <FluxSubheading size="lg" class="mb-4 max-sm:!text-sm sm:mb-6">Manage your profile and account settings</FluxSubheading>
            <FluxSeparator variant="subtle" />
        </div>

        <SettingsLayout heading="Appearance" subheading="Update your account's appearance settings">
            <FluxRadioGroup variant="segmented" :model-value="appearance" @update:model-value="setAppearance">
                <FluxRadio value="light" icon="sun">Light</FluxRadio>
                <FluxRadio value="dark" icon="moon">Dark</FluxRadio>
                <FluxRadio value="system" icon="computer-desktop">System</FluxRadio>
            </FluxRadioGroup>
            <FluxText class="mt-4">System follows your computer’s light or dark mode, and the window follows the choice too.</FluxText>
        </SettingsLayout>
    </div>
</template>
