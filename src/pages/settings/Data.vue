<script setup>
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { api } from '@/lib/api.js';
import { confirmDialog } from '@/lib/native.js';
import { on } from '@/lib/events.js';
import { format } from '@/lib/format.js';
import { route } from '@/lib/router.js';
import { toast } from '@/lib/toast.js';
import { importBackup as shellImport } from '@/components/shell/shell.js';

const cal = ref(null);
const local = ref(null);
const includeFiles = ref(true);
const copied = ref(false);
const exporting = ref(false);
const importing = ref(false);
const backingUp = ref(false);
const restoring = ref(null);

async function load() {
    try {
        [cal.value, local.value] = await Promise.all([api('calendar.info'), api('backup.local')]);
    } catch (e) {
        toast({ variant: 'danger', text: e.message });
    }
}

function fail(e) {
    toast({ variant: 'danger', text: e.message });
}

async function copyFeed() {
    try {
        await navigator.clipboard.writeText(cal.value.url);
        copied.value = true;
        setTimeout(() => (copied.value = false), 1500);
    } catch {}
}

async function setServe(serve) {
    try {
        cal.value = await api('calendar.setServe', { serve });
    } catch (e) {
        fail(e);
        cal.value = await api('calendar.info').catch(() => cal.value);
    }
}

async function exportIcs() {
    try {
        const r = await api('calendar.export');
        if (r) toast({ variant: 'success', text: `Saved ${r.events} calendar ${r.events === 1 ? 'event' : 'events'}.` });
    } catch (e) {
        fail(e);
    }
}

async function resetToken() {
    if (!(await confirmDialog('Make a new link? The old one stops working.', { okLabel: 'New link' }))) return;
    try {
        cal.value = await api('calendar.resetToken');
        toast({ text: 'Calendar link changed. Update it in your calendar app.' });
    } catch (e) {
        fail(e);
    }
}

async function exportBackup() {
    exporting.value = true;
    try {
        const r = await api('backup.export', { include_files: includeFiles.value });
        if (r) toast({ variant: 'success', text: includeFiles.value ? `Backup saved with ${r.files} ${r.files === 1 ? 'file' : 'files'}.` : 'Backup saved.' });
    } catch (e) {
        fail(e);
    } finally {
        exporting.value = false;
    }
}

async function importBackup() {
    importing.value = true;
    try {
        // The shell's import also reloads the user, the open pages and the badge.
        if (await shellImport()) await load();
    } finally {
        importing.value = false;
    }
}

async function backupNow() {
    backingUp.value = true;
    try {
        await api('backup.now');
        local.value = await api('backup.local');
        toast({ variant: 'success', text: 'Copy of your data saved.' });
    } catch (e) {
        fail(e);
    } finally {
        backingUp.value = false;
    }
}

async function setAuto(auto) {
    try {
        local.value = await api('backup.setAuto', { auto });
    } catch (e) {
        fail(e);
    }
}

async function restore(b) {
    const when = format(b.created_at, 'j M Y, H:i');
    const ok = await confirmDialog(
        `Replace all your data with the copy from ${when}? A copy of your current data is saved first, so you can go back. Attached files are not changed.`,
        { title: 'Restore this copy?', okLabel: 'Restore', danger: true },
    );
    if (!ok) return;
    restoring.value = b.name;
    try {
        await api('backup.restore', { name: b.name });
        window.location.reload();
    } catch (e) {
        restoring.value = null;
        fail(e);
    }
}

async function reveal() {
    try { await api('backup.reveal'); } catch (e) { fail(e); }
}

let offs = [];
onMounted(() => {
    load();
    // Imports started from the menu bar or the command palette reload the copies list too.
    offs.push(on('changed', (p) => { if (['settings', 'external'].includes(p?.area)) load(); }));
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

        <SettingsLayout heading="Backup & calendar" subheading="Your data stays on this computer. Export it, bring it back, or show your deadlines in any calendar app.">
            <div class="space-y-8">
                <div class="space-y-3">
                    <FluxHeading>Calendar</FluxHeading>
                    <FluxText>Your deadlines, revision due dates and expected decisions, with a reminder one day before.</FluxText>
                    <div class="flex flex-wrap gap-2">
                        <FluxButton icon="arrow-down-tray" @click="exportIcs">Export .ics</FluxButton>
                    </div>
                    <template v-if="cal">
                        <FluxSwitch :model-value="cal.serve" label="Calendar feed on this computer" description="Subscribe once in Apple Calendar or Outlook on this computer, and new deadlines appear by themselves while the app is running. The feed is only reachable from this computer." @update:model-value="setServe" />
                        <FluxCallout v-if="cal.error" variant="danger" icon="exclamation-triangle" :text="cal.error" />
                        <template v-if="cal.serve && cal.url">
                            <div class="flex gap-2">
                                <FluxInput readonly :model-value="cal.url" class="font-mono text-xs" />
                                <FluxButton icon="clipboard" @click="copyFeed">{{ copied ? 'Copied' : 'Copy' }}</FluxButton>
                            </div>
                            <FluxText size="sm">Apple Calendar: File → New Calendar Subscription. Outlook: Add calendar → Subscribe from web.</FluxText>
                            <div class="flex gap-2">
                                <FluxButton size="sm" variant="ghost" @click="resetToken">New link</FluxButton>
                            </div>
                        </template>
                    </template>
                </div>

                <FluxSeparator />

                <div class="space-y-3">
                    <FluxHeading>Backup</FluxHeading>
                    <FluxText>Your papers, submissions, journals and conferences, authors, checklists, deadlines, publications, calls for papers and settings. API keys are never included. The same format as the web app, so a backup works in either.</FluxText>
                    <FluxCheckbox v-model="includeFiles" label="Include files" description="Saves a .zip with your attached manuscripts, letters and proofs. Without files, a .json is saved." />
                    <FluxButton icon="arrow-down-tray" :loading="exporting" :disabled="exporting" @click="exportBackup">{{ includeFiles ? 'Export .zip' : 'Export JSON' }}</FluxButton>
                </div>

                <div class="space-y-3">
                    <FluxHeading>Import</FluxHeading>
                    <FluxText>Adds the backup’s records to this computer. Nothing existing is deleted. Works with .json and .zip backups from this app or the web app.</FluxText>
                    <FluxButton variant="primary" icon="arrow-up-tray" :loading="importing" :disabled="importing" @click="importBackup">Import backup…</FluxButton>
                </div>

                <FluxSeparator />

                <div v-if="local" class="space-y-3">
                    <FluxHeading>Automatic copies</FluxHeading>
                    <FluxSwitch :model-value="local.auto" label="Keep a daily copy of the database" :description="`A copy is saved each day on this computer; the last ${local.keep} are kept. Attached files are not copied.`" @update:model-value="setAuto" />
                    <div class="flex flex-wrap gap-2">
                        <FluxButton size="sm" icon="archive-box" :loading="backingUp" :disabled="backingUp" @click="backupNow">Make a copy now</FluxButton>
                        <FluxButton size="sm" variant="ghost" icon="folder-open" @click="reveal">Show folder</FluxButton>
                    </div>
                    <div v-if="local.backups.length" class="divide-y divide-zinc-100 rounded-xl border border-zinc-200 dark:divide-zinc-700/60 dark:border-zinc-700">
                        <div v-for="b in local.backups" :key="b.name" class="flex items-center justify-between gap-3 px-4 py-2 text-sm">
                            <div class="min-w-0">
                                <p class="font-medium text-zinc-800 tabular-nums dark:text-zinc-200">{{ format(b.created_at, 'D j M Y, H:i') }}</p>
                                <p class="text-xs text-zinc-400">{{ b.human_size }}</p>
                            </div>
                            <FluxButton size="sm" variant="ghost" icon="arrow-path" :loading="restoring === b.name" :disabled="!!restoring" @click="restore(b)">Restore</FluxButton>
                        </div>
                    </div>
                    <FluxText v-else size="sm">No copies yet.</FluxText>
                </div>

                <FluxSeparator />

                <div class="space-y-1">
                    <FluxHeading>AI updates</FluxHeading>
                    <FluxText>Choose the AI provider and when the automatic jobs run under <a :href="route('settings.ai')" class="text-indigo-600 hover:underline">Settings → AI</a>.</FluxText>
                </div>
            </div>
        </SettingsLayout>
    </section>
</template>
