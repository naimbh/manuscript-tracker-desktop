<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useAttrs, watch } from 'vue';
import FluxButton from './FluxButton.vue';
import FluxModalClose from './FluxModalClose.vue';
import { modalState } from '@/lib/modal.js';
import { cx } from './util.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  name: { type: String, default: null },
  show: { type: Boolean, default: undefined }, // v-model:show for unnamed modals
  variant: { type: String, default: null }, // null | 'flyout' | 'bare' | 'floating' (with flyout)
  position: { type: String, default: null }, // flyout: right (default) | left | bottom
  flyout: { type: Boolean, default: false },
  dismissible: { type: Boolean, default: undefined },
  escapable: { type: Boolean, default: undefined },
  closable: { type: Boolean, default: undefined },
  scroll: { type: String, default: null },
});
const emit = defineEmits(['close', 'cancel', 'update:show']);
const attrs = useAttrs();
const dialog = ref(null);

const isFlyout = computed(() => props.flyout || props.variant === 'flyout');
const variant = computed(() => (props.variant === 'flyout' ? null : props.variant));
const isClosable = computed(() => props.closable ?? variant.value !== 'bare');
const overflow = computed(() => props.scroll === 'body' && !isFlyout.value);

const classes = computed(() => {
  const v = variant.value;
  if (isFlyout.value) {
    const pos = props.position;
    const shape = v === 'floating'
      ? (pos === 'bottom' ? 'fixed m-2 p-8 min-w-[calc(100%-1rem)] overflow-y-auto mt-auto [--flux-flyout-translate:translateY(50px)]'
        : pos === 'left' ? 'fixed m-2 p-8 max-h-[calc(100dvh-1rem)] min-h-[calc(100dvh-1rem)] md:[:where(&)]:min-w-[25rem] overflow-y-auto mr-auto [--flux-flyout-translate:translateX(-50px)] rtl:mr-0 rtl:ml-auto rtl:[--flux-flyout-translate:translateX(50px)]'
          : 'fixed m-2 p-8 max-h-[calc(100dvh-1rem)] min-h-[calc(100dvh-1rem)] md:[:where(&)]:min-w-[25rem] overflow-y-auto ml-auto [--flux-flyout-translate:translateX(50px)] rtl:ml-0 rtl:mr-auto rtl:[--flux-flyout-translate:translateX(-50px)]')
      : v === 'bare' ? ''
        : (pos === 'bottom' ? 'fixed m-0 p-8 min-w-[100%] overflow-y-auto mt-auto [--flux-flyout-translate:translateY(50px)] border-t'
          : pos === 'left' ? 'fixed m-0 p-8 max-h-dvh min-h-dvh md:[:where(&)]:min-w-[25rem] overflow-y-auto mr-auto [--flux-flyout-translate:translateX(-50px)] border-e rtl:mr-0 rtl:ml-auto rtl:[--flux-flyout-translate:translateX(50px)]'
            : 'fixed m-0 p-8 max-h-dvh min-h-dvh md:[:where(&)]:min-w-[25rem] overflow-y-auto ml-auto [--flux-flyout-translate:translateX(50px)] border-s rtl:ml-0 rtl:mr-auto rtl:[--flux-flyout-translate:translateX(-50px)]');
    const skin = v === 'floating' ? 'bg-white dark:bg-zinc-800 ring ring-black/5 dark:ring-zinc-700 shadow-lg rounded-xl' : v === 'bare' ? 'bg-transparent' : 'bg-white dark:bg-zinc-800 border-transparent dark:border-zinc-700';
    return cx(shape, skin);
  }
  if (overflow.value) return '';
  return cx(v === 'bare' ? '' : 'p-6 [:where(&)]:max-w-xl [:where(&)]:min-w-xs shadow-lg rounded-xl', v === 'bare' ? 'bg-transparent' : 'bg-white dark:bg-zinc-800 ring ring-black/5 dark:ring-zinc-700 shadow-lg rounded-xl');
});
const contentClasses = computed(() => cx('relative', variant.value === 'bare' ? '' : 'p-6 [:where(&)]:max-w-xl [:where(&)]:min-w-xs shadow-lg rounded-xl', variant.value === 'bare' ? 'bg-transparent' : 'bg-white dark:bg-zinc-800 ring ring-black/5 dark:ring-zinc-700 shadow-lg rounded-xl'));

// class/style/autofocus go on the dialog (or the content box with scroll="body"); the rest on <ui-modal>.
const contentAttrs = computed(() => ({ style: attrs.style, autofocus: attrs.autofocus }));
const outerAttrs = computed(() => {
  const r = {};
  for (const [k, v] of Object.entries(attrs)) if (!['class', 'style', 'autofocus'].includes(k)) r[k] = v;
  if (props.dismissible === false) r['disable-click-outside'] = '';
  if (props.escapable === false) r['disable-escape'] = '';
  return r;
});

const isOpen = computed(() => (props.name ? !!modalState[props.name] : !!props.show));
function setOpen(v) {
  if (props.name) modalState[props.name] = v;
  emit('update:show', v);
}

function sync(open) {
  const d = dialog.value;
  if (!d) return;
  if (open && !d.open) {
    d.showModal();
  } else if (!open && d.open) {
    d.close();
  }
}
watch(isOpen, (v) => nextTick(() => sync(v)));

function onClose() {
  if (isOpen.value) setOpen(false);
  emit('close');
}
function onCancel(e) {
  if (props.escapable === false) { e.preventDefault(); return; }
  emit('cancel');
}
let downInside = false;
function onPointerDown(e) {
  const d = dialog.value;
  const target = overflow.value ? d.querySelector('[data-flux-modal-content]') : d;
  const r = target.getBoundingClientRect();
  downInside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
}
function onClick(e) {
  if (props.dismissible === false || downInside) return;
  const d = dialog.value;
  const target = overflow.value ? d.querySelector('[data-flux-modal-content]') : d;
  const r = target.getBoundingClientRect();
  const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
  if (!inside && (e.target === d || overflow.value)) d.close();
}

onMounted(() => { if (props.name && !(props.name in modalState)) modalState[props.name] = false; sync(isOpen.value); });
onBeforeUnmount(() => { if (dialog.value?.open) dialog.value.close(); if (props.name) modalState[props.name] = false; });

defineExpose({ show: () => setOpen(true), close: () => setOpen(false), dialog });
</script>

<template>
    <ui-modal v-bind="outerAttrs" data-flux-modal>
        <slot name="trigger" />
        <dialog
            ref="dialog"
            v-bind="overflow ? {} : contentAttrs"
            :class="[classes, overflow ? '' : attrs.class]"
            :data-modal="name || undefined"
            :data-flux-flyout="isFlyout ? '' : undefined"
            :data-flux-modal-overflow="overflow ? '' : undefined"
            @close="onClose"
            @cancel="onCancel"
            @pointerdown="onPointerDown"
            @click="onClick"
        >
            <div v-if="overflow" class="flex min-h-full items-center justify-center p-4 sm:p-6">
                <div v-bind="contentAttrs" :class="[contentClasses, attrs.class]" data-flux-modal-content>
                    <slot />
                    <div v-if="isClosable" class="absolute top-0 end-0 mt-4 me-4">
                        <FluxModalClose><FluxButton variant="ghost" icon="x-mark" size="sm" aria-label="Close modal" class="text-zinc-400! hover:text-zinc-800! dark:text-zinc-500! dark:hover:text-white!" /></FluxModalClose>
                    </div>
                </div>
            </div>
            <template v-else>
                <slot />
                <div v-if="isClosable" class="absolute top-0 end-0 mt-4 me-4">
                    <FluxModalClose><FluxButton variant="ghost" icon="x-mark" size="sm" aria-label="Close modal" class="text-zinc-400! hover:text-zinc-800! dark:text-zinc-500! dark:hover:text-white!" /></FluxModalClose>
                </div>
            </template>
        </dialog>
    </ui-modal>
</template>
