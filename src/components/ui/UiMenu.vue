<script setup>
import { nextTick, onBeforeUnmount, onMounted, ref, useSlots, watch } from 'vue';
import { anchor, hidePopover, showPopover } from '@/flux/anchor.js';
import { currentRoute } from '@/lib/router.js';

// A light dropdown menu. Use <UiMenuItem> inside; an optional #trigger slot replaces the "…" button.
// The menu is a popover in the top layer, placed next to its trigger with `anchor()`, so a scrolling or
// clipped panel never cuts it off and it always stays inside the window.
const props = defineProps({ align: { type: String, default: 'right' }, label: { type: String, default: 'More actions' } });
const slots = useSlots();
const o = ref(false);
const root = ref(null);
const trig = ref(null);
const menu = ref(null);

function place() {
  if (o.value && menu.value && trig.value) anchor(menu.value, trig.value, props.align === 'left' ? 'bottom start' : 'bottom end', 4);
}
async function open() {
  if (o.value) return;
  o.value = true;
  await nextTick();
  showPopover(menu.value);
  place();
}
function close() {
  if (!o.value) return;
  o.value = false;
  hidePopover(menu.value);
}
function toggle() { o.value ? close() : open(); }

const onDoc = (e) => { if (o.value && root.value && !root.value.contains(e.target) && !menu.value?.contains(e.target)) close(); };
const onKey = (e) => { if (e.key === 'Escape') close(); };
const onScroll = (e) => { if (o.value && !menu.value?.contains(e.target)) place(); };
onMounted(() => {
  document.addEventListener('pointerdown', onDoc, true);
  window.addEventListener('keydown', onKey);
  window.addEventListener('resize', place);
  window.addEventListener('scroll', onScroll, true);
});
onBeforeUnmount(() => {
  close();
  document.removeEventListener('pointerdown', onDoc, true);
  window.removeEventListener('keydown', onKey);
  window.removeEventListener('resize', place);
  window.removeEventListener('scroll', onScroll, true);
});
watch(() => currentRoute.fullPath, () => close());
</script>

<template>
    <div ref="root" class="relative inline-flex">
        <div v-if="slots.trigger" ref="trig" :aria-expanded="o ? 'true' : 'false'" @click="toggle"><slot name="trigger" /></div>
        <button v-else ref="trig" type="button" :aria-expanded="o ? 'true' : 'false'" :aria-label="label" :title="label" class="inline-flex size-8 items-center justify-center rounded-lg text-zinc-500 transition hover:bg-zinc-100 hover:text-zinc-800 dark:hover:bg-zinc-800 dark:hover:text-zinc-200" @click="toggle">
            <svg viewBox="0 0 20 20" fill="currentColor" class="size-4"><path d="M3 10a1.5 1.5 0 1 1 3 0 1.5 1.5 0 0 1-3 0Zm5.5 0a1.5 1.5 0 1 1 3 0 1.5 1.5 0 0 1-3 0ZM15.5 8.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3Z" /></svg>
        </button>
        <div ref="menu" popover="manual" role="menu" class="m-0 max-h-[calc(100vh-10px)] min-w-48 overflow-y-auto rounded-xl border border-zinc-200 bg-white p-1 text-sm text-zinc-700 shadow-lg shadow-zinc-900/10 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200" @click="close">
            <slot />
        </div>
    </div>
</template>
