<script setup>
import { onBeforeUnmount, onMounted, ref, useSlots } from 'vue';

// A light dropdown menu. Use <UiMenuItem> inside; an optional #trigger slot replaces the "…" button.
defineProps({ align: { type: String, default: 'right' }, label: { type: String, default: 'More actions' } });
const slots = useSlots();
const o = ref(false);
const root = ref(null);
const onDoc = (e) => { if (o.value && root.value && !root.value.contains(e.target)) o.value = false; };
const onKey = (e) => { if (e.key === 'Escape') o.value = false; };
onMounted(() => { document.addEventListener('click', onDoc); window.addEventListener('keydown', onKey); });
onBeforeUnmount(() => { document.removeEventListener('click', onDoc); window.removeEventListener('keydown', onKey); });
</script>

<template>
    <div ref="root" class="relative">
        <div v-if="slots.trigger" @click="o = !o"><slot name="trigger" /></div>
        <button v-else type="button" :aria-expanded="o ? 'true' : 'false'" :aria-label="label" :title="label" class="inline-flex size-8 items-center justify-center rounded-lg text-zinc-500 transition hover:bg-zinc-100 hover:text-zinc-800 dark:hover:bg-zinc-800 dark:hover:text-zinc-200" @click="o = !o">
            <svg viewBox="0 0 20 20" fill="currentColor" class="size-4"><path d="M3 10a1.5 1.5 0 1 1 3 0 1.5 1.5 0 0 1-3 0Zm5.5 0a1.5 1.5 0 1 1 3 0 1.5 1.5 0 0 1-3 0ZM15.5 8.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3Z" /></svg>
        </button>
        <Transition enter-active-class="transition-opacity duration-100" enter-from-class="opacity-0" leave-active-class="transition-opacity duration-100" leave-to-class="opacity-0">
            <div v-show="o" :class="['absolute top-full z-40 mt-1 min-w-48 rounded-xl border border-zinc-200 bg-white p-1 text-sm shadow-lg shadow-zinc-900/10', align === 'left' ? 'left-0' : 'right-0', 'dark:border-zinc-700 dark:bg-zinc-900']" @click="o = false">
                <slot />
            </div>
        </Transition>
    </div>
</template>
