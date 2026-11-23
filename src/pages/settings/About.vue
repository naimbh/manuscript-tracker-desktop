<script setup>
import { computed, onMounted, ref } from 'vue';
import { api } from '@/lib/api.js';
import { TAGLINE, TAGLINE_2 } from '@/lib/site.js';
import { toast } from '@/lib/toast.js';
import { checkForUpdates, installUpdate, updatePercent, updates } from '@/lib/updates.js';

const WEBSITE = 'https://naimbh.com/dev';
const REPO = 'https://github.com/naimbh/manuscript-tracker-desktop';
const LICENCE_URL = 'https://www.gnu.org/licenses/agpl-3.0.html';

const info = ref(null);
const copied = ref(false);
const OS = { macos: 'macOS', windows: 'Windows', linux: 'Linux' };
const percent = computed(() => updatePercent());

onMounted(async () => {
    try { info.value = await api('app.info'); } catch {}
});

function humanBytes(n) {
    if (n == null) return '—';
    if (n >= 1048576) return `${(n / 1048576).toFixed(1)} MB`;
    return `${Math.max(1, Math.round(n / 1024))} KB`;
}

const rows = computed(() => {
    const i = info.value;
    if (!i) return [];
    return [
        ['Version', i.version],
        ['System', `${OS[i.platform] ?? i.platform} (${i.arch})`],
        ['Tauri', i.tauri],
        ['Database', `${humanBytes(i.database_size)} · schema ${i.schema_version}`],
        ['Data folder', i.data_dir],
    ];
});

function open(url) {
    api('app.openExternal', { url }).catch((e) => toast({ variant: 'danger', text: e.message }));
}

async function openFolder() {
    try { await api('app.openDataFolder'); } catch (e) { toast({ variant: 'danger', text: e.message }); }
}

async function copyInfo() {
    const text = rows.value.map(([k, v]) => `${k}: ${v}`).join('\n');
    try {
        await navigator.clipboard.writeText(`Manuscript Tracker\n${text}`);
        copied.value = true;
        setTimeout(() => (copied.value = false), 1500);
    } catch {}
}
</script>

<template>
    <section class="page-in w-full">
        <div class="relative mx-auto mb-6 w-full max-w-6xl">
            <FluxHeading size="xl" :level="1" class="max-sm:!text-xl">Settings</FluxHeading>
            <FluxSubheading size="lg" class="mb-4 max-sm:!text-sm sm:mb-6">Manage your profile and account settings</FluxSubheading>
            <FluxSeparator variant="subtle" />
        </div>

        <SettingsLayout heading="About" subheading="The app, its maker, and where your data lives.">
            <div class="space-y-8">
                <div class="flex items-center gap-4">
                    <AppLogoIcon class="size-16 shrink-0 drop-shadow-md" />
                    <div class="min-w-0">
                        <p class="text-lg font-semibold tracking-tight">Manuscript <span class="bg-gradient-to-r from-indigo-600 to-fuchsia-600 bg-clip-text text-transparent dark:from-indigo-400 dark:to-fuchsia-400">Tracker</span></p>
                        <p class="text-sm text-zinc-600 dark:text-zinc-300">{{ TAGLINE }}</p>
                        <p class="text-xs text-zinc-500 dark:text-zinc-400">{{ TAGLINE_2 }}</p>
                        <p v-if="info" class="mt-0.5 text-xs text-zinc-400 tabular-nums">Version {{ info.version }}</p>
                    </div>
                </div>

                <div class="space-y-3 rounded-xl border border-zinc-200 p-4 dark:border-zinc-700">
                    <div class="flex flex-wrap items-center justify-between gap-3">
                        <div class="min-w-0">
                            <FluxHeading>Updates</FluxHeading>
                            <FluxText class="mt-0.5">
                                <template v-if="updates.status === 'checking'">Checking for updates…</template>
                                <template v-else-if="updates.status === 'current'">You’re up to date<template v-if="info"> (version {{ info.version }})</template>.</template>
                                <template v-else-if="updates.status === 'available'">Version {{ updates.version }} is available.</template>
                                <template v-else-if="updates.status === 'downloading'">Downloading version {{ updates.version }}…</template>
                                <template v-else-if="updates.status === 'installing'">Installing. The app will restart by itself.</template>
                                <template v-else-if="updates.status === 'failed'">{{ updates.error }}</template>
                                <template v-else>You’re using version {{ info?.version ?? '…' }}. New versions install themselves when you restart.</template>
                            </FluxText>
                        </div>
                        <FluxButton v-if="updates.status === 'available'" size="sm" variant="primary" icon="arrow-path" @click="installUpdate">Restart to update</FluxButton>
                        <FluxButton v-else size="sm" icon="arrow-path" :loading="updates.status === 'checking'" :disabled="['checking', 'downloading', 'installing'].includes(updates.status)" @click="checkForUpdates">Check for updates</FluxButton>
                    </div>
                    <div v-if="['downloading', 'installing'].includes(updates.status)" class="h-1.5 overflow-hidden rounded-full bg-zinc-200 dark:bg-white/10">
                        <div class="h-full rounded-full bg-indigo-600 transition-[width] duration-200 dark:bg-indigo-400" :class="percent === null && updates.status === 'downloading' ? 'w-1/3 animate-pulse' : ''" :style="percent !== null || updates.status === 'installing' ? { width: (updates.status === 'installing' ? 100 : percent) + '%' } : {}" />
                    </div>
                    <div v-if="updates.notes && ['available', 'downloading'].includes(updates.status)" class="max-h-48 overflow-auto rounded-lg bg-zinc-50 p-3 text-sm whitespace-pre-line text-zinc-700 select-text dark:bg-white/5 dark:text-zinc-300">{{ updates.notes }}</div>
                </div>

                <div class="rounded-xl border border-emerald-200 bg-emerald-50/60 p-4 text-sm dark:border-emerald-500/20 dark:bg-emerald-500/10">
                    <p class="flex items-center gap-2 font-medium text-emerald-800 dark:text-emerald-300"><FluxIcon name="lock-closed" variant="mini" class="size-4" />Your data stays on this computer</p>
                    <p class="mt-1 text-emerald-900/80 dark:text-emerald-200/80">Papers, files, journals and conferences, and settings are stored only in the data folder below. There is no account, no cloud sync and no tracking. The app goes online only to fetch what its features need, such as AI updates, citation counts and venue pages. API keys are kept in your system’s keychain.</p>
                </div>

                <div class="space-y-3">
                    <FluxHeading>Made by</FluxHeading>
                    <FluxText>Manuscript Tracker is made by <a href="#" class="font-medium text-indigo-600 hover:underline dark:text-indigo-400" @click.prevent="open(WEBSITE)">Naim Bin Hasan</a>.</FluxText>
                    <div class="flex flex-wrap gap-2">
                        <FluxButton size="sm" icon="globe-alt" @click="open(WEBSITE)">Website</FluxButton>
                        <FluxButton size="sm" icon="code-bracket" @click="open(REPO)">Source on GitHub</FluxButton>
                        <FluxButton size="sm" icon="bug-ant" @click="open(`${REPO}/issues`)">Report a problem</FluxButton>
                    </div>
                </div>

                <div class="space-y-1">
                    <FluxHeading>Licence</FluxHeading>
                    <FluxText>Free and open source under the <a href="#" class="text-indigo-600 hover:underline dark:text-indigo-400" @click.prevent="open(LICENCE_URL)">GNU Affero General Public License v3.0</a>. You may use, study, share and change it; if you share a changed version, share its source too.</FluxText>
                </div>

                <FluxSeparator />

                <div class="space-y-3">
                    <div class="flex items-center justify-between gap-2">
                        <FluxHeading>System</FluxHeading>
                        <div class="flex gap-2">
                            <FluxButton size="sm" variant="ghost" icon="clipboard" :disabled="!info" @click="copyInfo">{{ copied ? 'Copied' : 'Copy' }}</FluxButton>
                            <FluxButton size="sm" icon="folder-open" @click="openFolder">Open data folder</FluxButton>
                        </div>
                    </div>
                    <dl class="divide-y divide-zinc-100 rounded-xl border border-zinc-200 text-sm dark:divide-zinc-700/60 dark:border-zinc-700">
                        <div v-for="[k, v] in rows" :key="k" class="grid grid-cols-[120px_1fr] gap-3 px-4 py-2">
                            <dt class="select-none text-zinc-500">{{ k }}</dt>
                            <dd class="min-w-0 break-all font-mono text-xs leading-5 text-zinc-700 dark:text-zinc-300">{{ v }}</dd>
                        </div>
                        <div v-if="!rows.length" class="px-4 py-2 text-zinc-400">Loading…</div>
                    </dl>
                </div>
            </div>
        </SettingsLayout>
    </section>
</template>
