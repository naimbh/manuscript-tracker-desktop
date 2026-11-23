<script setup>
import { nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { toastState } from '@/lib/toast.js';
import { hidePopover, showPopover } from './anchor.js';

defineOptions({ inheritAttrs: false });

defineProps({
  position: { type: String, default: 'bottom end' },
  invert: { type: Boolean, default: false },
});

const root = ref(null);
const shown = ref(null); // the toast being displayed
const showing = ref(false);
let timer = null;
let removeTimer = null;
let remaining = 0;
let startedAt = 0;

function startTimer(ms) {
  clearTimeout(timer);
  if (!ms) return;
  remaining = ms;
  startedAt = Date.now();
  timer = setTimeout(hide, ms);
}
function pause() { if (!shown.value?.duration) return; clearTimeout(timer); remaining -= Date.now() - startedAt; }
function resume() { if (shown.value?.duration) startTimer(Math.max(remaining, 1000)); }

function hide() {
  clearTimeout(timer);
  showing.value = false;
  clearTimeout(removeTimer);
  removeTimer = setTimeout(() => {
    if (root.value) hidePopover(root.value);
    shown.value = null;
    if (toastState.current && !showing.value) toastState.current = null;
  }, 350);
}

watch(() => toastState.current, async (t) => {
  if (!t) { if (shown.value) hide(); return; }
  clearTimeout(removeTimer);
  showing.value = false;
  shown.value = t;
  await nextTick();
  if (root.value) {
    hidePopover(root.value);
    showPopover(root.value);
  }
  requestAnimationFrame(() => requestAnimationFrame(() => { showing.value = true; }));
  startTimer(t.duration);
});

onBeforeUnmount(() => { clearTimeout(timer); clearTimeout(removeTimer); });
</script>

<template>
    <ui-toast ref="root" popover="manual" :position="position" role="status">
        <div v-if="shown" :key="shown.id" :class="['group/toast max-w-sm in-[ui-toast-group]:max-w-auto in-[ui-toast-group]:w-xs sm:in-[ui-toast-group]:w-sm', $attrs.class, showing ? 'showing' : '']" :data-variant="shown.variant" :data-invert="invert ? '' : undefined" data-flux-toast-dialog @mouseenter="pause" @mouseleave="resume">
            <div class="p-2 flex rounded-xl shadow-lg bg-white border border-zinc-200 border-b-zinc-300/80 dark:bg-zinc-700 dark:border-zinc-600 group-data-invert/toast:bg-zinc-800 group-data-invert/toast:border-zinc-700 dark:group-data-invert/toast:bg-white dark:group-data-invert/toast:border-zinc-200 dark:group-data-invert/toast:border-b-zinc-300/80">
                <div class="flex-1 flex items-start gap-4 has-data-flux-toast-action:gap-2 overflow-hidden">
                    <div class="flex-1 py-1.5 ps-2.5 flex gap-2">
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="hidden [[data-flux-toast-dialog][data-variant=success]_&]:block shrink-0 mt-0.5 size-4 text-lime-600 dark:text-lime-400">
                            <path fill-rule="evenodd" d="M8 15A7 7 0 1 0 8 1a7 7 0 0 0 0 14Zm3.844-8.791a.75.75 0 0 0-1.188-.918l-3.7 4.79-1.649-1.833a.75.75 0 1 0-1.114 1.004l2.25 2.5a.75.75 0 0 0 1.15-.043l4.25-5.5Z" clip-rule="evenodd" />
                        </svg>
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="hidden [[data-flux-toast-dialog][data-variant=warning]_&]:block shrink-0 mt-0.5 size-4 text-amber-500 dark:text-amber-400">
                            <path fill-rule="evenodd" d="M6.701 2.25c.577-1 2.02-1 2.598 0l5.196 9a1.5 1.5 0 0 1-1.299 2.25H2.804a1.5 1.5 0 0 1-1.3-2.25l5.197-9ZM8 4a.75.75 0 0 1 .75.75v3a.75.75 0 1 1-1.5 0v-3A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z" clip-rule="evenodd" />
                        </svg>
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="hidden [[data-flux-toast-dialog][data-variant=info]_&]:block shrink-0 mt-0.5 size-4 text-cyan-500 dark:text-cyan-400">
                            <path fill-rule="evenodd" d="M15 8A7 7 0 1 1 1 8a7 7 0 0 1 14 0ZM9 5a1 1 0 1 1-2 0 1 1 0 0 1 2 0ZM6.75 8a.75.75 0 0 0 0 1.5h.75v1.75a.75.75 0 0 0 1.5 0v-2.5A.75.75 0 0 0 8.25 8h-1.5Z" clip-rule="evenodd" />
                        </svg>
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="currentColor" class="hidden [[data-flux-toast-dialog][data-variant=danger]_&]:block shrink-0 mt-0.5 size-4 text-rose-500 dark:text-rose-400">
                            <path fill-rule="evenodd" d="M8 15A7 7 0 1 0 8 1a7 7 0 0 0 0 14ZM8 4a.75.75 0 0 1 .75.75v3a.75.75 0 0 1-1.5 0v-3A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z" clip-rule="evenodd" />
                        </svg>
                        <div>
                            <div class="font-medium text-sm text-zinc-800 dark:text-white group-data-invert/toast:text-white dark:group-data-invert/toast:text-zinc-800 [&:not(:empty)+div]:font-normal [&:not(:empty)+div]:text-zinc-500 [&:not(:empty)+div]:dark:text-zinc-300 group-data-invert/toast:[&:not(:empty)+div]:text-zinc-300 dark:group-data-invert/toast:[&:not(:empty)+div]:text-zinc-500 [&:not(:empty)]:pb-2"><template v-if="shown.heading">{{ shown.heading }}</template></div>
                            <div class="font-medium text-sm text-zinc-800 dark:text-white group-data-invert/toast:text-white dark:group-data-invert/toast:text-zinc-800"><template v-if="shown.text">{{ shown.text }}</template></div>
                        </div>
                    </div>
                    <div class="flex items-center gap-1">
                        <ui-close class="flex items-center">
                            <button type="button" class="inline-flex items-center font-medium justify-center gap-2 truncate disabled:opacity-50 dark:disabled:opacity-75 disabled:cursor-default h-8 text-sm rounded-md w-8 bg-transparent hover:bg-zinc-800/5 dark:hover:bg-white/15 text-zinc-400 hover:text-zinc-800 dark:text-zinc-400 dark:hover:text-white group-data-invert/toast:hover:bg-white/15 group-data-invert/toast:hover:text-white dark:group-data-invert/toast:hover:bg-zinc-800/5 dark:group-data-invert/toast:hover:text-zinc-800" as="button" aria-label="Close" @click="hide">
                                <div>
                                    <svg class="[:where(&)]:size-5" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" aria-hidden="true" data-slot="icon">
                                        <path d="M6.28 5.22a.75.75 0 0 0-1.06 1.06L8.94 10l-3.72 3.72a.75.75 0 1 0 1.06 1.06L10 11.06l3.72 3.72a.75.75 0 1 0 1.06-1.06L11.06 10l3.72-3.72a.75.75 0 0 0-1.06-1.06L10 8.94 6.28 5.22Z"></path>
                                    </svg>
                                </div>
                            </button>
                        </ui-close>
                    </div>
                </div>
            </div>
        </div>
    </ui-toast>
</template>
