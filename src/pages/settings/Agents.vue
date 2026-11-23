<script setup>
import { onMounted, ref } from 'vue';
import { api } from '@/lib/api.js';
import { toast } from '@/lib/toast.js';
import { useResource } from '@/lib/useResource.js';

const res = useResource('settings.agents', () => api('agents.info'), { onError: (e) => toast({ variant: 'danger', text: e.message }) });
const info = res.data;
const copied = ref(null);
const testing = ref(false);
const result = ref(null);

onMounted(() => res.reload());

async function copy(text, key) {
    try {
        await navigator.clipboard.writeText(text);
        copied.value = key;
        setTimeout(() => (copied.value = null), 1500);
    } catch {}
}

async function test() {
    testing.value = true;
    result.value = null;
    try {
        const r = await api('agents.test');
        result.value = { ok: true, text: `The server answered with ${r.tools} tools.` };
    } catch (e) {
        result.value = { ok: false, text: e.message };
    } finally {
        testing.value = false;
    }
}
</script>

<template>
    <section class="page-in w-full">
        <div class="relative mx-auto mb-6 w-full max-w-6xl">
            <FluxHeading size="xl" :level="1" class="max-sm:!text-xl">Settings</FluxHeading>
            <FluxSubheading size="lg" class="mb-4 max-sm:!text-sm sm:mb-6">Manage your profile and account settings</FluxSubheading>
            <FluxSeparator variant="subtle" />
        </div>

        <SettingsLayout heading="Agents (MCP)" subheading="Let AI assistants such as Claude Desktop, Claude Code or Cursor read your papers and record submissions, decisions and deadlines.">
            <div class="space-y-8">
                <div class="rounded-xl border border-zinc-200 p-4 text-sm dark:border-zinc-700">
                    <p class="font-medium">What agents can do</p>
                    <p class="mt-1 text-zinc-500">List and read papers, record a submission, record an editor’s decision, list and add deadlines, search your journals and conferences, list calls for papers and list published work with citations.</p>
                </div>

                <div v-if="!info" class="h-40 animate-pulse rounded-xl bg-zinc-100 dark:bg-zinc-800" />

                <div v-else class="space-y-3">
                    <FluxHeading>On this computer</FluxHeading>
                    <FluxText>Add this server to your assistant’s MCP settings. For Claude Desktop, open Settings → Developer → Edit Config. For Cursor, add it to <code class="rounded bg-zinc-100 px-1 dark:bg-zinc-800">~/.cursor/mcp.json</code>. The assistant starts this app in the background, without a window, and works on the data on this computer. It needs no token, and nothing is reachable from other computers.</FluxText>
                    <div class="relative">
                        <pre class="overflow-x-auto rounded-xl bg-zinc-950 p-4 font-mono text-[12px] leading-relaxed text-zinc-200 select-text">{{ info.config }}</pre>
                        <button type="button" class="absolute top-2 right-2 rounded-md bg-white/10 px-2 py-1 text-[11px] text-white hover:bg-white/20" @click="copy(info.config, 'local')">{{ copied === 'local' ? 'Copied' : 'Copy' }}</button>
                    </div>
                    <FluxText>Or, with Claude Code:</FluxText>
                    <div class="relative">
                        <pre class="overflow-x-auto rounded-xl bg-zinc-950 p-4 pr-16 font-mono text-[12px] text-zinc-200 select-text">{{ info.cli }}</pre>
                        <button type="button" class="absolute top-2 right-2 rounded-md bg-white/10 px-2 py-1 text-[11px] text-white hover:bg-white/20" @click="copy(info.cli, 'cli')">{{ copied === 'cli' ? 'Copied' : 'Copy' }}</button>
                    </div>
                    <FluxText size="sm">If you move the app to another folder, copy the settings again.</FluxText>
                    <div class="flex items-center gap-3">
                        <FluxButton size="sm" icon="play" :loading="testing" :disabled="testing" @click="test">Test the server</FluxButton>
                        <span v-if="result" class="inline-flex items-center gap-1 text-sm" :class="result.ok ? 'text-emerald-600' : 'text-rose-600'">
                            <FluxIcon :name="result.ok ? 'check-circle' : 'exclamation-triangle'" variant="mini" class="size-4" />{{ result.text }}
                        </span>
                    </div>
                </div>

                <template v-if="info">
                    <FluxSeparator />

                    <div class="space-y-3">
                        <FluxHeading>Tools</FluxHeading>
                        <div class="divide-y divide-zinc-100 rounded-xl border border-zinc-200 dark:divide-zinc-700/60 dark:border-zinc-700">
                            <div v-for="t in info.tools" :key="t.name" class="px-4 py-2.5 text-sm">
                                <p class="flex items-center gap-2">
                                    <code class="font-mono text-[12px] font-medium text-zinc-800 dark:text-zinc-200">{{ t.name }}</code>
                                    <span class="rounded-full px-1.5 py-px text-[10px] font-medium" :class="t.read_only ? 'bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400' : 'bg-amber-50 text-amber-700 dark:bg-amber-500/10 dark:text-amber-300'">{{ t.read_only ? 'reads' : 'writes' }}</span>
                                </p>
                                <p class="mt-0.5 text-xs text-zinc-500">{{ t.description }}</p>
                            </div>
                        </div>
                    </div>
                </template>
            </div>
        </SettingsLayout>
    </section>
</template>
