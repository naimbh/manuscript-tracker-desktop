<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { useResource } from '@/lib/useResource.js';
import { usePager } from '@/lib/usePager.js';
import { openUrl } from '@/components/shell/links.js';
import { useDensity } from '@/lib/density.js';
import { api, submit } from '@/lib/api.js';
import { on } from '@/lib/events.js';
import { toast } from '@/lib/toast.js';
import { diffForHumans, numberFormat } from '@/lib/format.js';
import { provideErrors } from '@/flux/errors.js';
import { SCHOLAR_KIND, runText } from '@/pages/venues/runs.js';

const form = reactive({ scholar_url: '', interests: '' });
const errors = ref({});
provideErrors(errors);
const busy = ref(false);
const run = ref(null); // { id, step, progress }
const importing = ref(false);
const listEl = ref(null);
const dense = useDensity();

// Stale-while-revalidate: the profile and the Scholar list stay on screen while they refresh.
// The form is filled once (and after a save), so a refresh never overwrites what is being typed.
let filled = false;
const res = useResource('research', async () => {
    const [profile, list] = await Promise.all([api('research.profile'), api('research.scholarArticles')]);
    return { profile, articles: list.filter((a) => !a.in_library) };
}, {
    onData: ({ profile }) => {
        if (!filled) { filled = true; form.scholar_url = profile.scholar_url ?? ''; form.interests = profile.interests ?? ''; }
        if (profile.scholar_running && !run.value) run.value = { id: profile.scholar_running, step: runText({ kind: SCHOLAR_KIND }), progress: 0 };
    },
    onError: (e) => toast({ variant: 'danger', text: e.message }),
});
const p = computed(() => res.data.value?.profile ?? null);
const articles = computed(() => res.data.value?.articles ?? []);
const pager = usePager(articles, { query: 'articles', scrollTo: listEl });
const load = res.refresh;
const byline = (a) => [a.authors, a.venue, a.year].filter(Boolean).join(' · ');

const offs = [];
onMounted(() => {
    load();
    offs.push(on('ai-run', (e) => {
        // Also pick up a Scholar update started elsewhere (Published page, the scheduled citations job).
        if (!run.value && e.kind === SCHOLAR_KIND && e.status === 'running') run.value = { id: e.run_id, step: null, progress: 0 };
        if (!run.value || e.run_id !== run.value.id) return;
        if (e.status === 'running') {
            run.value = { id: e.run_id, step: e.step || runText({ kind: SCHOLAR_KIND }), progress: e.progress ?? 0 };
        } else {
            run.value = null;
            load();
        }
    }));
});
onBeforeUnmount(() => offs.forEach((off) => off()));

async function save() {
    busy.value = true;
    try {
        const data = await submit(errors, () => api('research.save', form));
        if (!data) return;
        res.mutate((cur) => ({ ...(cur ?? {}), profile: data, articles: cur?.articles ?? [] }));
        toast({ variant: 'success', text: 'Research profile saved.' });
    } catch (e) {
        toast({ variant: 'danger', text: e.message });
    } finally {
        busy.value = false;
    }
}

async function refresh() {
    try {
        const r = await api('research.refreshScholar');
        run.value = { id: r.run, step: runText({ kind: SCHOLAR_KIND }), progress: 0 };
    } catch (e) {
        toast({ variant: 'danger', heading: 'Update failed', text: e.message });
    }
}

async function cancel() {
    if (run.value) await api('ai.cancel', { id: run.value.id }).catch(() => {});
}

async function importAll() {
    importing.value = true;
    try {
        const r = await api('research.importPublications');
        toast({ variant: 'success', text: r.added === 1 ? 'Added 1 publication.' : `Added ${r.added} publications.` });
        await load();
    } catch (e) {
        toast({ variant: 'danger', text: e.message });
    } finally {
        importing.value = false;
    }
}

const statLabels = { citations: 'Citations', h: 'h-index', i10: 'i10-index' };
</script>

<template>
    <section class="w-full">
        <div class="relative mx-auto mb-6 w-full max-w-6xl">
            <FluxHeading size="xl" :level="1" class="max-sm:!text-xl">Settings</FluxHeading>
            <FluxSubheading size="lg" class="mb-4 max-sm:!text-sm sm:mb-6">Manage your profile and account settings</FluxSubheading>
            <FluxSeparator variant="subtle" />
        </div>

        <SettingsLayout heading="Research profile" subheading="Used by the Published page and the daily scan for calls for papers.">
            <form v-if="p" class="space-y-6" @submit.prevent="save">
                <FluxField>
                    <FluxLabel>Google Scholar profile</FluxLabel>
                    <FluxDescription>Open your profile on Google Scholar and copy the address. Citation counts, the h-index and the per-year chart are read from it.</FluxDescription>
                    <FluxInput v-model="form.scholar_url" name="scholar_url" placeholder="https://scholar.google.com/citations?user=…" />
                    <FluxError name="scholar_url" />
                </FluxField>
                <FluxField>
                    <FluxLabel>Research interests</FluxLabel>
                    <FluxDescription>Topics separated by commas. The daily scan uses them to find calls for papers. Leave empty to use your papers’ keywords: {{ p.suggested }}.</FluxDescription>
                    <FluxTextarea v-model="form.interests" name="interests" rows="3" placeholder="e.g. polymer chemistry, catalysis, green synthesis" />
                </FluxField>
                <FluxButton type="submit" variant="primary" :disabled="busy">Save</FluxButton>
            </form>

            <div v-if="p?.scholar_url" class="mt-10 space-y-4">
                <div class="flex flex-wrap items-center justify-between gap-3">
                    <div>
                        <FluxHeading>Google Scholar</FluxHeading>
                        <FluxText class="mt-0.5">{{ p.scholar_checked_at ? 'Read ' + diffForHumans(p.scholar_checked_at.replace(' ', 'T') + 'Z') : 'Not read from Scholar yet' }}</FluxText>
                    </div>
                    <div class="flex items-center gap-2">
                        <FluxButton v-if="run" size="sm" variant="ghost" icon="x-mark" @click="cancel">Cancel</FluxButton>
                        <FluxButton size="sm" icon="academic-cap" :loading="!!run" :disabled="!!run" @click="refresh">{{ run ? 'Updating from Google Scholar…' : 'Update from Scholar' }}</FluxButton>
                    </div>
                </div>

                <UiCollapse>
                <div v-if="run" class="rounded-xl border border-indigo-200/70 bg-gradient-to-br from-indigo-50 to-fuchsia-50/60 p-3 dark:border-indigo-500/20 dark:from-indigo-500/10 dark:to-fuchsia-500/5">
                    <div class="flex justify-between text-xs text-zinc-600 dark:text-zinc-400"><span class="truncate">{{ run.step }}</span><span class="num">{{ run.progress }}%</span></div>
                    <div class="mt-1.5 h-1.5 overflow-hidden rounded-full bg-white/80 dark:bg-zinc-800">
                        <div class="h-full rounded-full bg-gradient-to-r from-indigo-500 to-fuchsia-500 transition-[width] duration-300 ease-out" :style="{ width: Math.max(4, run.progress) + '%' }"></div>
                    </div>
                </div>
                </UiCollapse>

                <div v-if="p.scholar_stats?.all" class="grid grid-cols-3 gap-3">
                    <div v-for="(label, k) in statLabels" :key="k" class="panel p-3">
                        <p class="text-[11px] font-medium tracking-wide text-zinc-500 uppercase">{{ label }}</p>
                        <p class="num mt-1 text-xl font-semibold">{{ p.scholar_stats.all[k] != null ? numberFormat(p.scholar_stats.all[k]) : '—' }}</p>
                        <p v-if="p.scholar_stats.since && p.scholar_stats.recent?.[k] != null" class="text-xs text-zinc-500">{{ numberFormat(p.scholar_stats.recent[k]) }} since {{ p.scholar_stats.since }}</p>
                    </div>
                </div>

                <div v-if="articles.length" ref="listEl" class="panel scroll-mt-4 overflow-hidden">
                    <div class="flex flex-wrap items-center justify-between gap-2 border-b border-zinc-100 px-4 py-2.5 dark:border-zinc-800">
                        <p class="text-sm font-medium">{{ articles.length === 1 ? '1 article on Scholar is not in Published yet' : `${articles.length} articles on Scholar are not in Published yet` }}</p>
                        <FluxButton size="xs" icon="plus" :loading="importing" :disabled="importing" @click="importAll">Add to Published</FluxButton>
                    </div>
                    <!-- Rows follow the Published page's Comfortable/Compact choice: title (full title on hover), muted byline, citations on the right. -->
                    <UiList tag="ul" class="divide-y divide-zinc-100 dark:divide-zinc-800">
                        <li v-for="a in pager.items.value" :key="a.title" :class="dense ? 'min-h-12 py-1.5' : 'py-3'" class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 px-4 hover:bg-zinc-50/80 dark:hover:bg-zinc-800/30">
                            <div class="min-w-0">
                                <a v-if="a.scholar_url" :href="a.scholar_url" :title="a.title" :class="dense ? 'truncate' : 'line-clamp-2'" class="block text-[13px] font-medium text-indigo-700 hover:underline dark:text-indigo-300" @click.prevent="openUrl(a.scholar_url)">{{ a.title }}</a>
                                <p v-else :title="a.title" :class="dense ? 'truncate' : 'line-clamp-2'" class="text-[13px] font-medium text-zinc-800 dark:text-zinc-100">{{ a.title }}</p>
                                <p class="truncate text-xs text-zinc-500" :class="!dense && 'mt-0.5'" :title="byline(a)">{{ byline(a) }}</p>
                            </div>
                            <span class="num text-xs text-zinc-500" :title="a.cited_by ? `Cited by ${a.cited_by}` : null">{{ a.cited_by || '' }}</span>
                        </li>
                    </UiList>
                    <div v-if="pager.visible.value" class="border-t border-zinc-100 px-4 py-2 dark:border-zinc-800"><FluxPagination :pager="pager" :sizes="false" label="Scholar articles" /></div>
                </div>
            </div>
        </SettingsLayout>
    </section>
</template>
