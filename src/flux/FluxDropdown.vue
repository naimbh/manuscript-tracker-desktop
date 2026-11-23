<script setup>
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { anchor, hidePopover, showPopover } from './anchor.js';
import { currentRoute } from '@/lib/router.js';

const props = defineProps({
  position: { type: String, default: 'bottom' },
  align: { type: String, default: 'start' },
  gap: { type: [String, Number], default: 5 },
  offset: { type: [String, Number], default: 0 },
  disabled: { type: Boolean, default: false },
});
const emit = defineEmits(['open', 'close']);

const root = ref(null);
const isOpen = ref(false);
let trigger = null;
let overlay = null;

const ITEMS = '[data-flux-menu-item]:not([disabled]), ui-menu-radio:not([disabled]), ui-menu-checkbox:not([disabled])';

function items() { return overlay ? [...overlay.querySelectorAll(ITEMS)] : []; }
function activate(el) {
  for (const i of items()) i.removeAttribute('data-active');
  if (el) { el.setAttribute('data-active', ''); el.scrollIntoView?.({ block: 'nearest' }); }
}
function active() { return overlay?.querySelector('[data-active]'); }

function setAttrs(open) {
  for (const el of [root.value, trigger, overlay]) {
    if (!el) continue;
    if (open) el.setAttribute('data-open', ''); else el.removeAttribute('data-open');
  }
  trigger?.setAttribute('aria-expanded', open ? 'true' : 'false');
}
let sizeObserver = null;
function reposition() { if (isOpen.value) anchor(overlay, trigger, `${props.position} ${props.align}`, props.gap, props.offset); }

function open(focusFirst = false) {
  if (!overlay || !trigger || props.disabled || isOpen.value) return;
  showPopover(overlay);
  isOpen.value = true;
  setAttrs(true);
  reposition();
  overlay.focus({ preventScroll: true });
  if (focusFirst) activate(items()[0]);
  emit('open');
}
function close(restoreFocus = false) {
  if (!isOpen.value) return;
  activate(null);
  hidePopover(overlay);
  isOpen.value = false;
  setAttrs(false);
  if (restoreFocus) trigger?.focus({ preventScroll: true });
  emit('close');
}
function toggle() { isOpen.value ? close() : open(); }

function onTriggerKey(e) {
  if (['ArrowDown', 'ArrowUp'].includes(e.key) || ((e.key === 'Enter' || e.key === ' ') && !['BUTTON', 'A'].includes(trigger.tagName))) {
    e.preventDefault();
    open(true);
  }
}
function onOverlayKey(e) {
  const list = items();
  const cur = list.indexOf(active());
  if (e.key === 'ArrowDown') { e.preventDefault(); activate(list[(cur + 1) % list.length]); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); activate(list[cur <= 0 ? list.length - 1 : cur - 1]); }
  else if (e.key === 'Home') { e.preventDefault(); activate(list[0]); }
  else if (e.key === 'End') { e.preventDefault(); activate(list[list.length - 1]); }
  else if ((e.key === 'Enter' || e.key === ' ') && active() && !['INPUT', 'TEXTAREA'].includes(e.target.tagName)) { e.preventDefault(); active().click(); }
  else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(true); }
  else if (e.key === 'Tab') close();
}
function onOverlayOver(e) {
  const item = e.target.closest?.(ITEMS);
  if (item && overlay.contains(item)) activate(item);
}
function onOverlayLeave() { activate(null); }
function onOverlayClick(e) {
  const item = e.target.closest?.('[data-flux-menu-item], ui-menu-radio, ui-menu-checkbox');
  if (item && overlay.contains(item) && !item.hasAttribute('disabled') && !item.hasAttribute('keep-open')) close(true);
}
function onDocPointer(e) {
  if (!isOpen.value) return;
  if (root.value?.contains(e.target) || overlay?.contains(e.target)) return;
  close();
}
function onScroll(e) { if (isOpen.value && !overlay.contains(e.target)) reposition(); }

onMounted(() => {
  const kids = [...root.value.children];
  overlay = kids.find((k) => k.hasAttribute('popover'));
  trigger = kids.find((k) => k !== overlay);
  if (!trigger || !overlay) return;
  trigger.setAttribute('aria-haspopup', 'true');
  trigger.setAttribute('aria-expanded', 'false');
  if (!overlay.hasAttribute('tabindex')) overlay.setAttribute('tabindex', '-1');
  trigger.addEventListener('click', toggle);
  trigger.addEventListener('keydown', onTriggerKey);
  overlay.addEventListener('keydown', onOverlayKey);
  overlay.addEventListener('mouseover', onOverlayOver);
  overlay.addEventListener('mouseleave', onOverlayLeave);
  overlay.addEventListener('click', onOverlayClick);
  document.addEventListener('pointerdown', onDocPointer, true);
  window.addEventListener('resize', reposition);
  // Keep the menu attached to its trigger when its content grows or shrinks (e.g. clearing notifications).
  if (overlay && 'ResizeObserver' in window) { sizeObserver = new ResizeObserver(() => reposition()); sizeObserver.observe(overlay); }
  window.addEventListener('scroll', onScroll, true);
});
onBeforeUnmount(() => {
  close();
  trigger?.removeEventListener('click', toggle);
  trigger?.removeEventListener('keydown', onTriggerKey);
  document.removeEventListener('pointerdown', onDocPointer, true);
  window.removeEventListener('resize', reposition);
  sizeObserver?.disconnect();
  window.removeEventListener('scroll', onScroll, true);
});
watch(() => currentRoute.fullPath, () => close());

defineExpose({ open, close, toggle, isOpen });
</script>

<template>
    <ui-dropdown ref="root" :position="`${position} ${align}`" data-flux-dropdown><slot /></ui-dropdown>
</template>
