<script setup>
import { onBeforeUnmount, onMounted } from 'vue';
import { modal } from '@/lib/modal.js';

// shortcut: a key combo like 'cmd.k' / 'ctrl.k' / 'n' that also opens the modal (ignored while typing).
const props = defineProps({ name: { type: String, required: true }, shortcut: { type: String, default: null } });

function open(e) {
  if (e.currentTarget.querySelector('button[disabled]')) return;
  modal(props.name).show();
}
function onKey(e) {
  if (!props.shortcut) return;
  const parts = props.shortcut.toLowerCase().split('.');
  const key = parts.pop();
  const want = { meta: parts.includes('cmd') || parts.includes('meta'), ctrl: parts.includes('ctrl'), alt: parts.includes('alt'), shift: parts.includes('shift') };
  if (e.key.toLowerCase() !== key || e.metaKey !== want.meta || e.ctrlKey !== want.ctrl || e.altKey !== want.alt || e.shiftKey !== want.shift) return;
  e.preventDefault();
  modal(props.name).show();
}
onMounted(() => document.addEventListener('keydown', onKey));
onBeforeUnmount(() => document.removeEventListener('keydown', onKey));
</script>

<template>
    <div class="contents" data-flux-modal-trigger @click="open"><slot /></div>
</template>
