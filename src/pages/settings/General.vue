<script setup>
import { onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { api } from '@/lib/api.js';
import { on } from '@/lib/events.js';
import { toast } from '@/lib/toast.js';
import { useResource } from '@/lib/useResource.js';
import { kbd } from '@/lib/platform.js';

// How the app behaves on this computer. The AI job schedules live in Settings → AI.
const desktop = reactive({ run_in_background: true, open_at_login: false, native_notifications: true, show_badge: true, auto_update: true, always_on_top: false });
const loaded = ref(false);

const res = useResource('settings.general', () => api('settings.general'), {
    onData: (g) => { Object.assign(desktop, g.desktop); loaded.value = true; },
    onError: (e) => toast({ variant: 'danger', text: e.message }),
});
const load = () => res.refresh();

async function setDesktop(key, value) {
    desktop[key] = value;
    try {
        Object.assign(desktop, (await api('settings.setDesktop', { [key]: value })).desktop);
    } catch (e) {
        desktop[key] = !value;
        toast({ variant: 'danger', text: e.message });
    }
}

async function restoreNotices() {
    try {
        const r = await api('app.restoreNotices');
        toast({ variant: 'success', text: r.restored ? `${r.restored} dismissed ${r.restored === 1 ? 'warning is' : 'warnings are'} shown again.` : 'No warnings were dismissed.' });
    } catch (e) {
        toast({ variant: 'danger', text: e.message });
    }
}

let offs = [];
onMounted(() => {
    load();
    offs.push(on('changed', (p) => { if (['settings', 'external'].includes(p?.area)) load(); }));
    // The Window menu, its shortcut and the tray toggle "Always on Top" too: keep the switch in step.
    offs.push(on('window-state', (p) => { if (typeof p?.always_on_top === 'boolean') desktop.always_on_top = p.always_on_top; }));
});
onBeforeUnmount(() => offs.forEach((f) => f?.()));
</script>

<template>
    <section class="page-in w-full">
        <div class="relative mx-auto mb-6 w-full max-w-6xl">
            <FluxHeading size="xl" :level="1" class="max-sm:!text-xl">Settings</FluxHeading>
            <FluxSubheading size="lg" class="mb-4 max-sm:!text-sm sm:mb-6">Manage your profile and account settings</FluxSubheading>
            <FluxSeparator variant="subtle" />
        </div>

        <SettingsLayout heading="General" subheading="How the app behaves on this computer.">
            <div v-if="!loaded" class="space-y-3">
                <div v-for="i in 3" :key="i" class="h-12 animate-pulse rounded-xl bg-zinc-100 dark:bg-zinc-800" />
            </div>

            <div v-if="loaded" class="space-y-6">
                <div class="space-y-1">
                    <FluxHeading>On this computer</FluxHeading>
                    <FluxText>These apply at once.</FluxText>
                </div>
                <FluxSwitch :model-value="desktop.run_in_background" label="Keep running when the window is closed" description="Scheduled jobs and notifications keep working. Quit from the menu bar icon or the app menu." @update:model-value="(v) => setDesktop('run_in_background', v)" />
                <FluxSwitch :model-value="desktop.open_at_login" label="Open at login" description="Starts quietly in the background when you log in." @update:model-value="(v) => setDesktop('open_at_login', v)" />
                <FluxSwitch :model-value="desktop.native_notifications" label="Notifications" description="Show a system notification when a scan, update or reminder finishes while the window is not in front." @update:model-value="(v) => setDesktop('native_notifications', v)" />
                <FluxSwitch :model-value="desktop.always_on_top" label="Always on top" :description="`Keeps the window above other windows. Also in the Window menu and the menu bar icon (${kbd('mod+shift+t')}).`" @update:model-value="(v) => setDesktop('always_on_top', v)" />
                <FluxSwitch :model-value="desktop.show_badge" label="Show badge" description="A count on the app icon of things that need your attention." @update:model-value="(v) => setDesktop('show_badge', v)" />
                <FluxSwitch :model-value="desktop.auto_update" label="Check for updates automatically" description="Looks for a new version when the app starts and once a day, and offers to install it. Nothing is installed without your click." @update:model-value="(v) => setDesktop('auto_update', v)" />
                <div class="space-y-2 pt-2">
                    <FluxHeading>Dismissed warnings</FluxHeading>
                    <FluxText>Warnings you closed, such as an anonymity risk, stay closed for that submission. Bring them all back here.</FluxText>
                    <FluxButton size="sm" icon="arrow-path" @click="restoreNotices">Show dismissed warnings again</FluxButton>
                </div>
            </div>
        </SettingsLayout>
    </section>
</template>
