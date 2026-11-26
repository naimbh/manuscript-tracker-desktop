<script setup>
import { computed, nextTick, reactive, ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { provideErrors } from '@/flux/errors.js';
import { appearance, setAppearance } from '@/lib/appearance.js';
import { navigate } from '@/lib/router.js';
import { site } from '@/lib/site.js';
import { toast } from '@/lib/toast.js';
import { importBackup, setUser, shell, updateBadge } from '@/components/shell/shell.js';

const step = ref(1);
const busy = ref(false);
const restoring = ref(false);
const form = reactive({ name: '', email: '', interests: '', scholar_url: '', theme: appearance.value });
const errors = ref({});
provideErrors(errors);
const year = new Date().getFullYear();
const isMac = computed(() => shell.platform === 'macos');
const root = ref(null);

const FEATURES = [
  ['clock', 'Review clocks that tell you when a decision is overdue'],
  ['building-library', 'Journals and conferences kept current with AI and real sources'],
  ['sparkles', 'Daily calls for papers that fit your research'],
  ['chart-bar', 'Your citations and h-index from Google Scholar'],
];
const JOURNEY = [['x-mark', 'bg-rose-400', 'Conference'], ['x-mark', 'bg-rose-400', 'Journal A'], ['clock', 'bg-sky-400', 'Journal B']];

async function focusFirst() {
  await nextTick();
  root.value?.querySelector('input, textarea')?.focus();
}

function next() {
  errors.value = {};
  if (!form.name.trim()) {
    errors.value = { name: ['The name field is required.'] };
    return;
  }
  step.value = 2;
  focusFirst();
}

function back() {
  step.value = 1;
  focusFirst();
}

function previewTheme(mode) {
  form.theme = mode;
  setAppearance(mode, { persist: false });
}

async function finish() {
  if (busy.value) return;
  busy.value = true;
  try {
    const u = await submit(errors, () => api('app.onboard', { ...form }));
    if (!u) {
      if (errors.value.name || errors.value.email) step.value = 1;
      return;
    }
    setUser(u);
    await setAppearance(form.theme, { persist: false });
    updateBadge();
    navigate('dashboard', undefined, { replace: true });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  } finally {
    busy.value = false;
  }
}

async function restore() {
  if (restoring.value) return;
  restoring.value = true;
  try {
    const r = await importBackup();
    if (r && shell.hasUser) navigate('dashboard', undefined, { replace: true });
  } finally {
    restoring.value = false;
  }
}
</script>

<template>
    <div class="flex min-h-svh bg-[#f7f8fc] select-none dark:bg-zinc-950">
        <div v-if="isMac" class="fixed inset-x-0 top-0 z-10 h-7" data-tauri-drag-region></div>

        <!-- Brand panel -->
        <aside class="relative flex w-[44%] max-w-2xl flex-col justify-between overflow-hidden bg-gradient-to-br from-indigo-600 via-violet-600 to-fuchsia-600 p-12 text-white" data-tauri-drag-region>
            <div class="pointer-events-none absolute -top-24 -right-24 size-96 rounded-full bg-white/10 blur-3xl"></div>
            <div class="pointer-events-none absolute -bottom-32 -left-20 size-96 rounded-full bg-fuchsia-300/20 blur-3xl"></div>
            <div class="hero-grid pointer-events-none absolute inset-0 text-white opacity-60"></div>

            <div class="relative flex items-center gap-3" :class="isMac ? 'mt-4' : ''">
                <span class="rounded-2xl bg-white/20 p-1 ring-1 ring-white/40 shadow-lg shadow-indigo-900/30"><AppLogoIcon class="size-10" /></span>
                <span class="leading-tight">
                    <span class="block text-lg font-semibold tracking-tight">{{ site.name }}</span>
                    <span class="block text-xs text-white/70">{{ site.tagline }}</span>
                </span>
            </div>

            <div class="relative">
                <h2 class="max-w-md text-4xl leading-tight font-semibold tracking-tight text-balance">Every paper, every journal, every decision, in one place.</h2>
                <ul class="mt-8 space-y-4 text-sm text-white/85">
                    <li v-for="[icon, text] in FEATURES" :key="icon" class="flex items-center gap-3">
                        <span class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-white/15 ring-1 ring-white/20"><FluxIcon :name="icon" variant="mini" class="size-4" /></span>
                        {{ text }}
                    </li>
                </ul>
            </div>

            <!-- Mini journey graphic -->
            <div class="relative rounded-2xl bg-white/10 p-4 ring-1 ring-white/20 backdrop-blur">
                <div class="flex items-center justify-between text-[11px] text-white/70"><span>Journey</span><span>3 journals</span></div>
                <div class="mt-3 flex items-center">
                    <template v-for="([ic, bg, label], i) in JOURNEY" :key="label">
                        <span v-if="i > 0" class="mx-2 h-px flex-1 bg-white/30"></span>
                        <span class="flex flex-col items-center gap-1.5">
                            <span class="flex size-8 items-center justify-center rounded-full text-white ring-4 ring-white/15" :class="[bg, i === 2 ? 'shadow-lg shadow-sky-500/40' : '']"><FluxIcon :name="ic" variant="micro" class="size-4" /></span>
                            <span class="text-[10px] text-white/80">{{ label }}</span>
                        </span>
                    </template>
                </div>
            </div>
        </aside>

        <!-- Form -->
        <main class="flex flex-1 flex-col items-center justify-center px-10 py-10">
            <div ref="root" class="page-in w-full max-w-md">
                <div class="rounded-3xl border border-zinc-200/80 bg-white p-10 shadow-2xl shadow-indigo-900/10 dark:border-zinc-800 dark:bg-zinc-900">
                    <form v-if="step === 1" class="flex flex-col gap-7" @submit.prevent="next">
                        <div class="flex w-full flex-col gap-1.5 text-center">
                            <p class="eyebrow">Step 1 of 2</p>
                            <h1 class="text-2xl font-semibold tracking-tight text-zinc-900 dark:text-zinc-100">Welcome to {{ site.name }}</h1>
                            <p class="text-center text-sm text-zinc-500 dark:text-zinc-400">Let’s set up your desk. Your papers stay on this computer.</p>
                        </div>

                        <div class="flex flex-col gap-6">
                            <FluxInput v-model="form.name" name="name" label="Name" type="text" autofocus autocomplete="name" placeholder="Full name" />
                            <FluxInput v-model="form.email" name="email" label="Email address" badge="Optional" type="email" autocomplete="email" placeholder="email@example.com" description="Used for author lists and your calendar feed." />
                            <FluxButton type="submit" variant="primary" class="w-full" icon-trailing="arrow-right">Continue</FluxButton>
                        </div>

                        <div class="flex flex-col items-center gap-2 border-t border-zinc-100 pt-5 text-center dark:border-zinc-800">
                            <p class="text-xs text-zinc-500">Moving from another computer or the web app?</p>
                            <FluxButton size="sm" variant="ghost" icon="arrow-up-tray" :loading="restoring" @click="restore">Restore from a backup</FluxButton>
                        </div>
                    </form>

                    <form v-else class="flex flex-col gap-7" @submit.prevent="finish">
                        <div class="flex w-full flex-col gap-1.5 text-center">
                            <p class="eyebrow">Step 2 of 2</p>
                            <h1 class="text-2xl font-semibold tracking-tight text-zinc-900 dark:text-zinc-100">Your research</h1>
                            <p class="text-center text-sm text-zinc-500 dark:text-zinc-400">This helps the daily scan find calls for papers that fit you. You can change it later in Settings.</p>
                        </div>

                        <div class="flex flex-col gap-6">
                            <FluxField>
                                <FluxLabel>Research interests</FluxLabel>
                                <FluxDescription>Topics separated by commas.</FluxDescription>
                                <FluxTextarea v-model="form.interests" name="interests" rows="3" placeholder="e.g. polymer chemistry, catalysis, green synthesis" />
                                <FluxError name="interests" />
                            </FluxField>
                            <FluxField>
                                <FluxLabel badge="Optional">Google Scholar profile</FluxLabel>
                                <FluxDescription>Open your profile on Google Scholar and copy the address.</FluxDescription>
                                <FluxInput v-model="form.scholar_url" name="scholar_url" placeholder="https://scholar.google.com/citations?user=…" />
                                <FluxError name="scholar_url" />
                            </FluxField>
                            <FluxField>
                                <FluxLabel>Appearance</FluxLabel>
                                <FluxRadioGroup variant="segmented" :model-value="form.theme" @update:model-value="previewTheme">
                                    <FluxRadio value="light" icon="sun">Light</FluxRadio>
                                    <FluxRadio value="dark" icon="moon">Dark</FluxRadio>
                                    <FluxRadio value="system" icon="computer-desktop">System</FluxRadio>
                                </FluxRadioGroup>
                            </FluxField>
                            <div class="flex items-center justify-between gap-2">
                                <FluxButton variant="ghost" icon="arrow-left" @click="back">Back</FluxButton>
                                <FluxButton type="submit" variant="primary" :disabled="busy" icon-trailing="arrow-right">Open my desk</FluxButton>
                            </div>
                        </div>
                    </form>
                </div>
                <p class="mt-6 text-center text-[11px] text-zinc-400">
                    © {{ year }} <template v-if="site.creditName">{{ site.creditName }} · </template>{{ site.name }}
                </p>
            </div>
        </main>
    </div>
</template>
