<script setup>
import { useResource } from '@/lib/useResource.js';
import { computed, onMounted, reactive, ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { confirmDialog } from '@/lib/native.js';
import { toast } from '@/lib/toast.js';
import { provideErrors } from '@/flux/errors.js';
import AiJobs from './AiJobs.vue';

const s = ref(null);
const form = reactive({ driver: 'claude-code', models: {}, secrets: {}, claudeBin: '', baseUrl: '', autofill_empty: true, web_fallback: true });
const errors = ref({});
provideErrors(errors);
const busy = ref(false);
const testing = ref(false);
const test = ref(null);
const detecting = ref(false);
const claude = ref(null);

const icons = { 'claude-code': 'command-line', api: 'sparkles', openai: 'bolt', gemini: 'globe-alt', compatible: 'server-stack' };

const provider = computed(() => s.value?.providers.find((p) => p.key === form.driver) ?? null);

function fill(data) {
    s.value = data;
    form.driver = data.driver;
    for (const p of data.providers) {
        form.models[p.key] = p.model ?? '';
        form.secrets[p.key] = '';
    }
    form.claudeBin = data.claude_bin ?? '';
    form.baseUrl = data.base_url ?? '';
    form.autofill_empty = data.autofill_empty;
    form.web_fallback = data.web_fallback;
    claude.value = data.claude;
}

// Revisits show the last settings at once; the background refresh fills the form only if it was not filled yet.
let filled = false;
const res = useResource('settings.ai', () => api('ai.settings'), {
    onData: (data) => { if (!filled) { filled = true; fill(data); } else { s.value = data; claude.value = data.claude; } },
    onError: (e) => toast({ variant: 'danger', text: e.message }),
});
onMounted(() => res.reload());

async function save() {
    busy.value = true;
    try {
        const data = await submit(errors, () => api('ai.save', form));
        if (!data) return;
        fill(data);
        res.mutate(data);
        test.value = null;
        toast({ variant: 'success', text: 'AI settings saved.' });
    } catch (e) {
        toast({ variant: 'danger', text: e.message });
    } finally {
        busy.value = false;
    }
}

async function forget(key) {
    const ok = await confirmDialog('The key is removed from your system keychain.', { title: 'Remove saved key?', okLabel: 'Remove', danger: true });
    if (!ok) return;
    try {
        const data = await api('ai.forget', { key });
        fill(data);
        res.mutate(data);
        toast({ text: 'Removed.' });
    } catch (e) {
        toast({ variant: 'danger', text: e.message });
    }
}

async function testConnection() {
    testing.value = true;
    test.value = null;
    try {
        test.value = await api('ai.test');
    } catch (e) {
        test.value = { ok: false, message: e.message };
    } finally {
        testing.value = false;
    }
}

async function detect() {
    detecting.value = true;
    try {
        claude.value = await api('ai.detectClaude');
    } catch (e) {
        toast({ variant: 'danger', text: e.message });
    } finally {
        detecting.value = false;
    }
}

function copy(text) {
    navigator.clipboard?.writeText(text).then(() => toast({ text: 'Copied.' }), () => {});
}
</script>

<template>
    <section class="w-full">
        <div class="relative mx-auto mb-6 w-full max-w-6xl">
            <FluxHeading size="xl" :level="1" class="max-sm:!text-xl">Settings</FluxHeading>
            <FluxSubheading size="lg" class="mb-4 max-sm:!text-sm sm:mb-6">Manage your profile and account settings</FluxSubheading>
            <FluxSeparator variant="subtle" />
        </div>

        <SettingsLayout heading="AI" subheading="Choose the AI that runs the scans for calls for papers, the journal and conference updates and the citation updates. Keys are stored in your system keychain.">
            <form v-if="s" class="space-y-7" @submit.prevent="save">
                <div class="flex items-center gap-3 rounded-xl border p-3 text-sm" :class="s.ready ? 'border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-500/20 dark:bg-emerald-500/10 dark:text-emerald-300' : 'border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-500/20 dark:bg-amber-500/10 dark:text-amber-300'">
                    <FluxIcon :name="s.ready ? 'check-circle' : 'exclamation-triangle'" variant="mini" />
                    <span class="flex-1">{{ s.status }}</span>
                    <FluxButton size="sm" :loading="testing" :disabled="testing" @click="testConnection">{{ testing ? 'Testing…' : 'Test connection' }}</FluxButton>
                </div>
                <p v-if="test" class="-mt-4 text-sm" :class="test.ok ? 'text-emerald-600' : 'text-rose-600'">{{ test.message }}</p>

                <FluxRadioGroup v-model="form.driver" label="Provider" variant="cards" class="grid! grid-cols-1 gap-3 sm:grid-cols-2">
                    <FluxRadio v-for="p in s.providers" :key="p.key" :value="p.key" :label="p.label" :icon="icons[p.key]"
                               :description="p.saved ? 'Key saved' : (p.key === 'claude-code' ? (claude?.bin ? 'Found on this computer' : 'Not found yet') : (p.key === 'compatible' ? 'Cloud or local models' : 'Needs an API key'))" />
                </FluxRadioGroup>

                <div v-if="provider" :key="form.driver" class="space-y-5 rounded-2xl border border-zinc-200 p-5 dark:border-zinc-700">
                    <p class="text-sm text-zinc-600 dark:text-zinc-400">{{ provider.about }}</p>

                    <template v-if="form.driver === 'claude-code'">
                        <div class="rounded-xl bg-zinc-50 p-4 text-sm dark:bg-zinc-800/60">
                            <div class="flex items-start gap-3">
                                <FluxIcon :name="claude?.bin ? 'check-circle' : 'magnifying-glass'" variant="mini" :class="claude?.bin ? 'text-emerald-600' : 'text-zinc-400'" />
                                <div class="min-w-0 flex-1">
                                    <p class="font-medium">{{ claude?.bin ? 'Claude Code found' : 'Claude Code was not found' }}</p>
                                    <p v-if="claude?.bin" class="mt-0.5 truncate font-mono text-xs text-zinc-500" :title="claude.bin">{{ claude.bin }}</p>
                                    <p v-if="claude?.bin" class="mt-0.5 text-xs text-zinc-500">{{ claude.found_in }}<template v-if="claude.version"> · {{ claude.version }}</template></p>
                                    <p v-else class="mt-0.5 text-xs text-zinc-500">Install Claude Code or the Claude desktop app, then detect again. You can also enter the CLI path below.</p>
                                </div>
                                <FluxButton size="xs" variant="ghost" icon="arrow-path" :loading="detecting" @click="detect">Detect again</FluxButton>
                            </div>
                            <p class="mt-3 text-xs text-zinc-500">{{ claude?.help }}</p>
                            <button type="button" class="mt-2 inline-flex items-center gap-1.5 rounded-md bg-white px-2 py-1 font-mono text-xs text-zinc-700 ring-1 ring-zinc-200 hover:ring-indigo-300 dark:bg-zinc-900 dark:text-zinc-300 dark:ring-zinc-700" @click="copy('claude setup-token')">
                                claude setup-token <FluxIcon name="clipboard" variant="micro" class="size-3.5 text-zinc-400" />
                            </button>
                        </div>
                    </template>

                    <FluxInput v-if="form.driver === 'compatible'" v-model="form.baseUrl" name="baseUrl" label="Base URL" placeholder="https://openrouter.ai/api/v1"
                               description-trailing="Examples: https://openrouter.ai/api/v1 · https://api.groq.com/openai/v1 · https://api.perplexity.ai · http://localhost:11434/v1 (Ollama) · http://localhost:1234/v1 (LM Studio)" />

                    <FluxField>
                        <FluxLabel>{{ form.driver === 'claude-code' ? 'Sign-in token (optional)' : 'API key' }}{{ form.driver === 'compatible' ? ' (if the provider needs one)' : '' }}</FluxLabel>
                        <FluxInput v-model="form.secrets[form.driver]" :name="`secrets.${form.driver}`" type="password" autocomplete="off" viewable
                                   :placeholder="provider.saved ? `Saved: ${provider.saved} (leave empty to keep)` : 'Paste here'" />
                        <FluxError :name="`secrets.${form.driver}`" />
                        <button v-if="provider.saved" type="button" class="justify-self-start text-xs text-zinc-500 hover:text-rose-600" @click="forget(provider.secret_key)">Remove saved key</button>
                    </FluxField>

                    <div class="grid gap-4 sm:grid-cols-2">
                        <template v-if="form.driver === 'claude-code'">
                            <FluxSelect v-model="form.models['claude-code']" label="Model">
                                <FluxSelectOption value="opus">Opus (best quality)</FluxSelectOption>
                                <FluxSelectOption value="sonnet">Sonnet (faster)</FluxSelectOption>
                                <FluxSelectOption value="haiku">Haiku (lightest)</FluxSelectOption>
                            </FluxSelect>
                            <FluxInput v-model="form.claudeBin" name="claudeBin" label="CLI path (optional)" :placeholder="claude?.bin ? 'Found automatically' : '/usr/local/bin/claude'" />
                        </template>
                        <FluxInput v-else v-model="form.models[form.driver]" :name="`models.${form.driver}`" label="Model" :placeholder="provider.placeholder" description-trailing="Use the model name exactly as the provider lists it." />
                    </div>
                </div>

                <div class="space-y-4">
                    <FluxHeading>How research results are used</FluxHeading>
                    <FluxSwitch v-model="form.autofill_empty" label="Fill in empty fields automatically"
                                description="Only values found on a source page, with a matching quote. Anything that would change what you entered waits for your review." />
                    <FluxSwitch v-model="form.web_fallback" label="Let the AI search the web when pages lack an answer"
                                description="Pages and free scholarly indexes are read first. Web search is slower and costs more; its answers are checked against the page they cite." />
                </div>

                <div class="flex items-center gap-3">
                    <FluxButton type="submit" variant="primary" :disabled="busy">Save</FluxButton>
                    <span class="text-xs text-zinc-500">Keys never leave this computer except to call the provider you choose.</span>
                </div>
            </form>
            <FluxSeparator class="my-8" />
            <AiJobs />
        </SettingsLayout>
    </section>
</template>
