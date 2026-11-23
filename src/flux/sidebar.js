import { reactive, watch } from 'vue';

/**
 * Shared state of the app sidebar, like Flux's ui-sidebar. The desktop window is at least 960px wide, so the
 * sidebar is in its desktop mode from 960px up (Flux uses 1024px); below that the stashable mobile mode applies.
 * collapsedDesktop hides the sidebar (menu "Toggle Sidebar" / Cmd+Ctrl+\) and is remembered.
 */
const media = typeof window !== 'undefined' ? window.matchMedia('(min-width: 960px)') : null;
const KEY = 'sidebar.collapsed';

function stored() {
  try { return window.localStorage.getItem(KEY) === '1'; } catch { return false; }
}

export const sidebarState = reactive({
  desktop: media ? media.matches : true,
  collapsedMobile: true,
  collapsedDesktop: stored(),
});

media?.addEventListener?.('change', (e) => {
  sidebarState.desktop = e.matches;
  sidebarState.collapsedMobile = true;
});
watch(() => sidebarState.collapsedDesktop, (v) => { try { window.localStorage.setItem(KEY, v ? '1' : '0'); } catch {} });

export function toggleSidebar() {
  if (sidebarState.desktop) sidebarState.collapsedDesktop = !sidebarState.collapsedDesktop;
  else sidebarState.collapsedMobile = !sidebarState.collapsedMobile;
}
export function collapseSidebar() { sidebarState.collapsedMobile = true; }

/** The data-* attributes Flux puts on the sidebar, its backdrop and its toggles. */
export function sidebarAttrs() {
  return sidebarState.desktop
    ? { 'data-flux-sidebar-on-desktop': '', 'data-flux-sidebar-collapsed-desktop': sidebarState.collapsedDesktop ? '' : undefined }
    : { 'data-flux-sidebar-on-mobile': '', 'data-flux-sidebar-collapsed-mobile': sidebarState.collapsedMobile ? '' : undefined };
}
