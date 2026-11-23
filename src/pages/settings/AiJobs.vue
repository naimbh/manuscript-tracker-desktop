<script setup>
import { onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { on } from '@/lib/events.js';
import { diffForHumans } from '@/lib/format.js';
import { toast } from '@/lib/toast.js';
import { provideErrors } from '@/flux/errors.js';
import { useResource } from '@/lib/useResource.js';

// Schedules for the automatic AI jobs (scans for calls for papers, journal and conference updates, citations).
const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];

const jobs = ref([]);
const form = reactive({});
const now = ref('');
const loaded = ref(false);
const busy = ref(false);
const errors = ref({});
provideErrors(errors);

function fill(g, { keepForm = false } = {}) {
    jobs.value = g.jobs;
    if (!keepForm) for (const j of g.jobs) {
        form[`${j.key}_enabled`] = j.enabled;
        form[`${j.key}_frequency`] = j.frequency;
        form[`${j.key}_day`] = j.day;
        form[`${j.key}_time`] = j.time;
    }
    now.value = g.now;
    loaded.value = true;
}

// Scheduler ticks refresh the job status only, so they never overwrite a time or day being edited.
let keepForm = false;
const res = useResource('settings.aiJobs', () => api('settings.general'), {
    onData: (g) => { fill(g, { keepForm }); keepForm = false; },
    onError: (e) => toast({ variant: 'danger', text: e.message }),
});
const load = () => res.refresh();
const refreshStatus = () => { keepForm = loaded.value; return res.refresh(); };

async function save() {
    busy.value = true;
    try {
        const g = await submit(errors, () => api('settings.saveGeneral', form));
        if (!g) return;
        fill(g);
        toast({ variant: 'success', text: 'Schedules saved.' });
    } catch (e) {
        toast({ variant: 'danger', text: e.message });
    } finally {
        busy.value = false;
    }
}

let offs = [];
onMounted(() => {
    load();
    offs.push(on('changed', (p) => { if (['settings', 'external'].includes(p?.area)) load(); }));
    offs.push(on('scheduler', refreshStatus));
});
onBeforeUnmount(() => offs.forEach((f) => f?.()));
</script>

<template>
    <div>
        <div v-if="!loaded" class="space-y-3">
            <div v-for="i in 3" :key="i" class="h-24 animate-pulse rounded-xl bg-zinc-100 dark:bg-zinc-800" />
        </div>
        <form v-else class="space-y-6" @submit.prevent="save">
            <div class="space-y-1">
                <FluxHeading>Automatic AI jobs</FluxHeading>
                <FluxText>They run while the app is open or in the background. If the computer was asleep or the app was closed at that time, they run as soon as possible afterwards. Time on this computer now: {{ now }}.</FluxText>
            </div>

            <div class="divide-y divide-zinc-100 rounded-xl border border-zinc-200 bg-white dark:divide-zinc-700/60 dark:border-zinc-700 dark:bg-zinc-900">
                <div v-for="j in jobs" :key="j.key" class="space-y-3 p-4">
                    <div class="flex items-start justify-between gap-4">
                        <div class="min-w-0">
                            <p class="text-sm font-medium text-zinc-800 dark:text-white">{{ j.label }}</p>
                            <p class="text-xs text-zinc-500">{{ j.description }}</p>
                            <p class="mt-1 text-[11px] text-zinc-400">
                                <template v-if="j.running">Running now…</template>
                                <template v-else-if="j.last_run">Last run {{ diffForHumans(j.last_run) }}</template>
                                <template v-else>Not run yet</template>
                            </p>
                        </div>
                        <FluxSwitch v-model="form[`${j.key}_enabled`]" :aria-label="`${j.label} on or off`" />
                    </div>
                    <div class="grid gap-3 sm:grid-cols-3" :class="form[`${j.key}_enabled`] ? '' : 'pointer-events-none opacity-50'">
                        <FluxSelect v-model="form[`${j.key}_frequency`]" :name="`${j.key}_frequency`" size="sm" aria-label="How often">
                            <FluxSelectOption value="daily">Every day</FluxSelectOption>
                            <FluxSelectOption value="weekly">Every week</FluxSelectOption>
                        </FluxSelect>
                        <FluxSelect v-if="form[`${j.key}_frequency`] === 'weekly'" v-model="form[`${j.key}_day`]" :name="`${j.key}_day`" size="sm" aria-label="Day">
                            <FluxSelectOption v-for="(d, i) in DAYS" :key="d" :value="i">on {{ d }}</FluxSelectOption>
                        </FluxSelect>
                        <FluxInput v-model="form[`${j.key}_time`]" :name="`${j.key}_time`" type="time" size="sm" aria-label="Time" />
                    </div>
                    <FluxError :name="`${j.key}_time`" />
                </div>
            </div>

            <FluxButton type="submit" variant="primary" :disabled="busy">Save</FluxButton>
        </form>

    </div>
</template>
