<script setup>
import { onBeforeUnmount, onMounted, ref, useSlots } from 'vue';
import { anchor, hidePopover, showPopover } from './anchor.js';
import { flag } from './util.js';

const props = defineProps({
  content: { type: [String, Number], default: null },
  position: { type: String, default: 'top' },
  align: { type: String, default: 'center' },
  kbd: { type: String, default: null },
  interactive: { type: Boolean, default: false },
  toggleable: { type: Boolean, default: false },
  gap: { type: [String, Number], default: 5 },
});

const slots = useSlots();
const root = ref(null);
const tip = ref(null);
let trigger = null;
let hideTimer = null;
let showTimer = null;
const uid = 'flux-tooltip-' + Math.random().toString(36).slice(2, 9);

function overlay() {
  return tip.value || root.value?.querySelector(':scope > [data-flux-tooltip-content]');
}
function show() {
  clearTimeout(hideTimer);
  clearTimeout(showTimer);
  const o = overlay();
  if (!o || !trigger) return;
  showPopover(o);
  anchor(o, trigger, `${props.position} ${props.align}`, props.gap);
}
function hide(delay = 0) {
  clearTimeout(hideTimer);
  clearTimeout(showTimer);
  hideTimer = setTimeout(() => { const o = overlay(); if (o) hidePopover(o); }, delay);
}
function toggle() {
  const o = overlay();
  if (o?.matches(':popover-open')) hide(); else show();
}

const listeners = [];
function listen(el, ev, fn) { el.addEventListener(ev, fn); listeners.push([el, ev, fn]); }

onMounted(() => {
  trigger = [...root.value.children].find((c) => !c.hasAttribute('data-flux-tooltip-content'));
  if (!trigger) return;
  const o = overlay();
  if (o) { o.id = o.id || uid; o.setAttribute('role', 'tooltip'); trigger.setAttribute('aria-describedby', o.id); }
  if (props.toggleable) {
    listen(trigger, 'click', toggle);
    listen(document, 'pointerdown', (e) => { if (!root.value?.contains(e.target)) hide(); });
  } else {
    // Hover: a short delay like native tooltips, and ignore spurious leave events while the pointer is still
    // over the trigger (macOS sends these in the title-bar area, which made the tooltip flicker on and off).
    listen(trigger, 'mouseenter', () => {
      clearTimeout(hideTimer);
      if (overlay()?.matches(':popover-open')) return;
      clearTimeout(showTimer);
      showTimer = setTimeout(show, 350);
    });
    listen(trigger, 'mouseleave', (e) => {
      const r = trigger.getBoundingClientRect();
      if (e.clientX > r.left && e.clientX < r.right && e.clientY > r.top && e.clientY < r.bottom) return;
      hide(props.interactive ? 100 : 0);
    });
    listen(trigger, 'focusin', () => { if (trigger.matches(':focus-visible') || trigger.querySelector(':focus-visible')) show(); });
    listen(trigger, 'focusout', () => hide());
    listen(trigger, 'pointerdown', () => hide());
    if (props.interactive && o) {
      listen(o, 'mouseenter', () => clearTimeout(hideTimer));
      listen(o, 'mouseleave', () => hide());
    }
  }
  listen(document, 'keydown', (e) => { if (e.key === 'Escape') hide(); });
});

onBeforeUnmount(() => {
  clearTimeout(hideTimer);
  clearTimeout(showTimer);
  for (const [el, ev, fn] of listeners) el.removeEventListener(ev, fn);
});
</script>

<template>
    <ui-dropdown v-if="toggleable" ref="root" :position="`${position} ${align}`" data-flux-tooltip>
        <slot />
        <div v-if="content !== null && content !== undefined" ref="tip" popover="manual" class="relative py-2 px-2.5 rounded-md text-xs text-white font-medium bg-zinc-800 dark:bg-zinc-700 dark:border dark:border-white/10 p-0 overflow-visible" data-flux-tooltip-content>{{ content }}<kbd v-if="kbd" class="ms-2 rounded bg-white/15 px-1 py-px font-sans text-[10px] font-semibold text-zinc-200 ring-1 ring-white/20">{{ kbd }}</kbd></div>
        <slot name="content" />
    </ui-dropdown>
    <ui-tooltip v-else ref="root" :position="`${position} ${align}`" data-flux-tooltip :interactive="flag(interactive)">
        <slot />
        <div v-if="content !== null && content !== undefined" ref="tip" popover="manual" class="relative py-2 px-2.5 rounded-md text-xs text-white font-medium bg-zinc-800 dark:bg-zinc-700 dark:border dark:border-white/10 p-0 overflow-visible" data-flux-tooltip-content>{{ content }}<kbd v-if="kbd" class="ms-2 rounded bg-white/15 px-1 py-px font-sans text-[10px] font-semibold text-zinc-200 ring-1 ring-white/20">{{ kbd }}</kbd></div>
        <slot name="content" />
    </ui-tooltip>
</template>
