<script setup>
import { onMounted, ref } from 'vue';
import { api } from '@/lib/api.js';
import { route } from '@/lib/router.js';
import { site } from '@/lib/site.js';

// One small line: the logo mark, the version (links to About) and the maker's credit.
let cached = null;
const version = ref(cached);
onMounted(async () => {
    if (cached) return;
    try { cached = (await api('app.info')).version; version.value = cached; } catch {}
});

function openCredit() {
    api('app.openExternal', { url: site.creditUrl || 'https://naimbh.com/dev' }).catch(() => {});
}
</script>

<template>
    <footer class="select-none border-t border-zinc-800/5 pt-3 pb-0.5 text-center dark:border-white/5">
        <p class="text-[11px] text-zinc-500 dark:text-zinc-400">
            <button type="button" class="group cursor-pointer transition hover:text-zinc-800 dark:hover:text-zinc-100" @click="openCredit">
                Developed by <span class="font-medium text-zinc-700 group-hover:text-indigo-600 dark:text-zinc-200 dark:group-hover:text-indigo-400">{{ site.creditName || 'Naim Bin Hasan' }}</span>
            </button>
            <a v-if="version" :href="route('settings.about')" class="ms-0.5 text-[10px] tabular-nums text-zinc-400 transition hover:text-zinc-700 dark:text-zinc-500 dark:hover:text-zinc-200" title="About Manuscript Tracker">(v{{ version }})</a>
        </p>
    </footer>
</template>
