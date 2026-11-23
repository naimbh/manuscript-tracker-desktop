<script setup>
import { computed, onBeforeUnmount, onMounted, ref, useAttrs, watch } from 'vue';
import { collapseSidebar, sidebarAttrs, sidebarState, toggleSidebar } from './sidebar.js';
import { onMenu } from '@/lib/native.js';
import { currentRoute } from '@/lib/router.js';
import { cx } from './util.js';

const props = defineProps({
  // Desktop: a drag strip under the macOS traffic lights (data-tauri-drag-region). Set false to omit.
  titlebar: { type: Boolean, default: true },
  sticky: { type: Boolean, default: false },
  stashable: { type: Boolean, default: false },
  collapsible: { type: [Boolean, String], default: null },
});
defineOptions({ inheritAttrs: false });
const attrs = useAttrs();
const rest = computed(() => Object.fromEntries(Object.entries(attrs).filter(([k]) => k !== 'class')));
const el = ref(null);
const cloak = ref(true);
const mobile = computed(() => props.stashable || props.collapsible === 'mobile' || props.collapsible === true);
const collapsibleAttr = computed(() => {
  const c = props.collapsible ?? (props.stashable ? 'mobile' : null);
  return c ? (c === 'mobile' ? 'mobile' : 'true') : undefined;
});

const classes = computed(() => cx(
  '[grid-area:sidebar]',
  'z-1 flex flex-col gap-4 [:where(&)]:w-64 p-4',
  'data-flux-sidebar-collapsed-desktop:w-14 data-flux-sidebar-collapsed-desktop:px-2',
  'data-flux-sidebar-collapsed-desktop:cursor-e-resize rtl:data-flux-sidebar-collapsed-desktop:cursor-w-resize',
  // Desktop: collapsing hides the sidebar completely; chrome is not selectable.
  'data-flux-sidebar-collapsed-desktop:hidden select-none cursor-default',
  mobile.value ? [
    'max-lg:data-flux-sidebar-cloak:hidden',
    'data-flux-sidebar-on-mobile:data-flux-sidebar-collapsed-mobile:-translate-x-full data-flux-sidebar-on-mobile:data-flux-sidebar-collapsed-mobile:rtl:translate-x-full',
    'z-20! data-flux-sidebar-on-mobile:start-0! data-flux-sidebar-on-mobile:fixed! data-flux-sidebar-on-mobile:top-0! data-flux-sidebar-on-mobile:min-h-dvh! data-flux-sidebar-on-mobile:max-h-dvh!',
  ] : '',
  props.sticky ? 'max-h-dvh overflow-y-auto overscroll-contain' : '',
));

let offMenu = null;
onMounted(() => {
  cloak.value = false;
  // View > Toggle Sidebar in the native menu bar.
  offMenu = onMenu('toggle_sidebar', toggleSidebar);
  // Like Flux's x-init: add the transition only after the first paint so it does not slide out on load.
  if (mobile.value) requestAnimationFrame(() => el.value?.classList.add('transition-transform'));
});
onBeforeUnmount(() => offMenu?.());
watch(() => currentRoute.fullPath, () => collapseSidebar());

function onKey(e) { if (e.key === 'Escape' && !sidebarState.desktop && !sidebarState.collapsedMobile) collapseSidebar(); }
</script>

<template>
    <ui-sidebar-toggle v-if="mobile" class="z-20 fixed inset-0 bg-black/10 hidden data-flux-sidebar-on-mobile:not-data-flux-sidebar-collapsed-mobile:block" v-bind="sidebarAttrs()" data-flux-sidebar-backdrop @click="collapseSidebar" />
    <ui-sidebar
        ref="el"
        v-bind="{ ...rest, ...sidebarAttrs() }"
        :class="[classes, attrs.class]"
        :collapsible="collapsibleAttr"
        :stashable="stashable ? '' : undefined"
        :sticky="sticky ? '' : undefined"
        :data-flux-sidebar-cloak="cloak ? '' : undefined"
        data-flux-sidebar
        @keydown="onKey"
    ><div v-if="titlebar" class="-mx-4 -mt-4 -mb-2 hidden h-9 shrink-0 [.platform-mac_&]:block" data-tauri-drag-region></div><slot /></ui-sidebar>
</template>
