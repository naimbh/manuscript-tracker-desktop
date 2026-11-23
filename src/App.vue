<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, watchEffect } from 'vue';
import { api } from '@/lib/api.js';
import { on } from '@/lib/events.js';
import { onMenu } from '@/lib/native.js';
import { bus } from '@/lib/bus.js';
import { modal } from '@/lib/modal.js';
import { toast } from '@/lib/toast.js';
import { user } from '@/lib/site.js';
import { kbd } from '@/lib/platform.js';
import { setAppearance } from '@/lib/appearance.js';
import { shortcut } from '@/lib/shortcuts.js';
import { openPalette, registerCommands, registerSearch } from '@/lib/palette.js';
import { currentRoute, navigate, refreshRoute, route, routeIs, setGuard, startRouter } from '@/lib/router.js';
import { sidebarState, toggleSidebar } from '@/flux/sidebar.js';
import NotificationBell from '@/components/shell/NotificationBell.vue';
import NewPaper from '@/components/shell/NewPaper.vue';
import UpdateNotice from '@/components/shell/UpdateNotice.vue';
import { checkForUpdates, watchUpdates } from '@/lib/updates.js';
import { markEntering } from '@/lib/motion.js';
import { prefetchRoutes } from '@/lib/router.js';
import { boot, exportBackup, importBackup, setUser, shell, updateBadge } from '@/components/shell/shell.js';

const ready = ref(false);
const isWelcome = computed(() => currentRoute.name === 'welcome');
const collapsed = computed(() => sidebarState.desktop && sidebarState.collapsedDesktop);
const isMac = computed(() => shell.platform === 'macos');
const fullBleed = computed(() => !!currentRoute.meta?.fullBleed);

const SECTIONS = {
  go_desk: 'dashboard',
  go_papers: 'papers.index',
  go_publications: 'publications',
  go_venues: 'venues',
  go_opportunities: 'opportunities',
  go_deadlines: 'deadlines',
};

function goAbout() {
  try { navigate('settings.about'); } catch { navigate('settings.profile'); }
}

function newPaper() {
  if (shell.hasUser) modal('new-paper').show();
}

function focusSearch() {
  const s = document.querySelector('[data-search]');
  if (s) s.focus();
  else openPalette();
}

async function runScans() {
  try {
    const r = await api('desk.runScans');
    toast({ text: r?.started?.length ? 'Scans started. You will get a notification when they finish.' : 'The scans are already running.' });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

function onMenuCommand(id) {
  if (!shell.hasUser) {
    if (id === 'backup_import') importBackup();
    return;
  }
  if (SECTIONS[id]) return navigate(SECTIONS[id]);
  switch (id) {
    case 'new_paper': return newPaper();
    case 'settings': return navigate('settings.profile');
    case 'run_scans': return toast({ text: 'Scans started. You will get a notification when they finish.' });
    case 'backup_export': return exportBackup();
    case 'backup_import': return importBackup();
    case 'help': return modal('shortcuts').show();
    case 'about': return goAbout();
    case 'check_updates': goAbout(); return checkForUpdates();
  }
}

const SHORTCUTS = [
  ['New paper', 'N'],
  ['Search the page', '/'],
  ['Find anything', kbd('mod+k')],
  ['Desk … Deadlines', `${kbd('mod+1')} … ${kbd('mod+6')}`],
  ['Settings', kbd('mod+,')],
  ['Toggle sidebar', kbd('mod+backslash')],
  ['Always on top', kbd('mod+shift+t')],
  ['Close a dialog', 'Esc'],
  ['Keyboard shortcuts', '?'],
];

function registerPalette() {
  const page = (id, title, name, icon, k, keywords) => ({ id, title, group: 'Pages', icon, kbd: k ? kbd(k) : undefined, keywords, run: () => navigate(name) });
  const offs = [
    registerCommands('shell-pages', [
      page('go-desk', 'Desk', 'dashboard', 'home', 'mod+1', ['dashboard', 'home']),
      page('go-papers', 'Papers', 'papers.index', 'document-text', 'mod+2'),
      page('go-published', 'Published', 'publications', 'trophy', 'mod+3', ['publications', 'citations']),
      page('go-venues', 'Journals & Conferences', 'venues', 'building-library', 'mod+4', ['venues', 'journals', 'conferences']),
      page('go-opportunities', 'Calls for Papers', 'opportunities', 'sparkles', 'mod+5', ['opportunities', 'calls for papers', 'cfp']),
      page('go-deadlines', 'Deadlines', 'deadlines', 'calendar-days', 'mod+6'),
      page('go-data', 'Backup & calendar', 'settings.data', 'arrow-down-tray', null, ['export', 'import', 'ics']),
      page('go-settings', 'Settings', 'settings.profile', 'cog', 'mod+,', ['profile', 'preferences']),
      page('go-general', 'General settings', 'settings.general', 'adjustments-horizontal', null),
      page('go-research', 'Research profile', 'settings.research', 'academic-cap', null, ['scholar', 'interests']),
      page('go-ai', 'AI settings', 'settings.ai', 'sparkles', null, ['provider', 'model', 'key']),
      page('go-appearance', 'Appearance', 'settings.appearance', 'sun', null, ['theme', 'dark', 'light']),
      page('go-agents', 'Agents', 'settings.agents', 'command-line', null, ['mcp']),
    ]),
    registerCommands('shell-actions', [
      { id: 'new-paper', title: 'New paper', group: 'Actions', icon: 'plus', kbd: 'N', run: newPaper },
      { id: 'run-scans', title: 'Run scans now', group: 'Actions', icon: 'arrow-path', keywords: ['opportunities', 'refresh', 'venues', 'citations'], run: runScans },
      { id: 'export-backup', title: 'Export backup', group: 'Actions', icon: 'arrow-down-tray', run: exportBackup },
      { id: 'import-backup', title: 'Import backup', group: 'Actions', icon: 'arrow-up-tray', keywords: ['restore'], run: importBackup },
      { id: 'toggle-sidebar', title: 'Toggle sidebar', group: 'Actions', icon: 'view-columns', run: toggleSidebar },
      { id: 'theme-light', title: 'Use light appearance', group: 'Actions', icon: 'sun', keywords: ['theme'], run: () => setAppearance('light') },
      { id: 'theme-dark', title: 'Use dark appearance', group: 'Actions', icon: 'moon', keywords: ['theme'], run: () => setAppearance('dark') },
      { id: 'theme-system', title: 'Use system appearance', group: 'Actions', icon: 'computer-desktop', keywords: ['theme'], run: () => setAppearance('system') },
      { id: 'shortcuts', title: 'Keyboard shortcuts', group: 'Actions', icon: 'question-mark-circle', kbd: '?', run: () => modal('shortcuts').show() },
    ]),
    registerSearch('shell-search', async (q) => {
      if (!shell.hasUser) return [];
      const r = await api('desk.search', { q });
      return [
        ...(r.papers ?? []).map((p) => ({ id: 'paper-' + p.id, title: p.title, subtitle: p.stage, group: 'Papers', icon: 'document-text', run: () => navigate('papers.index', p.id) })),
        ...(r.venues ?? []).map((v) => ({ id: 'venue-' + v.id, title: v.name, subtitle: v.kind, group: 'Journals & Conferences', icon: 'building-library', run: () => navigate('venues', { q: v.name }) })),
      ];
    }),
  ];
  return () => offs.forEach((off) => off());
}

const cleanups = [];

// The shell is one screen tall: the sidebar stays put and only the main area scrolls (WKWebView and WebView2).
watchEffect(() => {
  const shellMode = ready.value && !isWelcome.value;
  for (const el of [document.documentElement, document.body]) {
    el.style.height = shellMode ? '100%' : '';
    el.style.overflow = shellMode ? 'hidden' : '';
  }
  document.body.style.minHeight = shellMode ? '0' : '';
});
watchEffect(() => document.documentElement.classList.toggle('sidebar-collapsed', ready.value && !isWelcome.value && collapsed.value));
/* Route changes. Each route renders inside its own page box (.route-page: absolutely fills the main area and owns
   its padding and scrolling), so the outgoing page keeps its own layout while it fades out over the incoming one:
   no padding snap between full-bleed (Papers) and padded pages, no gap frames, scroll position kept per route.
   Entrance animations play only while a page arrives (data-entering); refreshes never replay them. */
const routeView = ref(null);
const scrolls = new Map();
const pageEl = () => routeView.value?.querySelector(':scope > .route-page:not(.route-leave-active)') ?? null;
/** Classes of a route's page box (computed once per page; the leaving page is not re-rendered). */
const pageClass = computed(() => (fullBleed.value
  ? 'flex flex-col overflow-hidden'
  : ['overflow-y-auto p-6 xl:p-8', isMac.value ? 'pt-10 xl:pt-10' : collapsed.value ? 'pt-11 xl:pt-11' : '']));
watch(() => currentRoute.name, (to, from) => {
  // Before the DOM update: remember where the old page was scrolled.
  const el = pageEl();
  if (from && el) scrolls.set(from, el.scrollTop);
});

let stopEntering = null;
let enterFallback = null;
let offReady = null;
function startEntrance(el) {
  stopEntering?.();
  clearTimeout(enterFallback);
  offReady?.();
  if (!el) return;
  el.setAttribute('data-entering', '');
  // Entrance animations run while the first data arrives and a moment after; later refreshes never replay them.
  const settle = () => { clearTimeout(enterFallback); offReady?.(); stopEntering = markEntering(el); };
  offReady = bus.once('page-ready', settle);
  enterFallback = setTimeout(settle, 1500);
}
function onRouteEnter(el) {
  el.scrollTop = scrolls.get(currentRoute.name) ?? 0;
  startEntrance(el);
  // Safety net: a page whose fade never finished (window hidden mid-transition) must not linger.
  setTimeout(() => routeView.value?.querySelectorAll(':scope > .route-leave-active').forEach((x) => x.remove()), 600);
}
watch(routeView, (el) => { if (el) nextTick(() => startEntrance(pageEl())); });

/** Wait for the first page's code (and, if it says so, its data) before showing the window. */
async function firstPaint() {
  try { await currentRoute.component?.__asyncLoader?.(); } catch {}
  await nextTick();
  await new Promise((resolve) => {
    const t = setTimeout(resolve, 600);
    bus.once('page-ready', () => { clearTimeout(t); resolve(); });
  });
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
}

onMounted(async () => {
  watchUpdates();
  try {
    await boot({ first: true });
  } catch (e) {
    toast({ variant: 'danger', heading: 'Could not start', text: e.message, duration: 0 });
  }
  setGuard((r) => {
    if (!shell.hasUser) return r.name === 'welcome' ? null : 'welcome';
    return r.name === 'welcome' ? 'dashboard' : null;
  });
  startRouter();
  ready.value = true;

  cleanups.push(onMenu('*', onMenuCommand));
  cleanups.push(on('user', (u) => { setUser(u); refreshRoute(); }));
  cleanups.push(on('changed', ({ area } = {}) => {
    if (area === 'opportunities' || area === 'external') boot().catch(() => {});
    updateBadge();
  }));
  cleanups.push(on('notifications', (p) => { if (typeof p?.unread === 'number') shell.unread = p.unread; }));
  cleanups.push(bus.on('papers-changed', () => updateBadge()));
  cleanups.push(bus.on('desk-attention', (n) => updateBadge(n)));
  cleanups.push(bus.on('opportunities-changed', () => boot().catch(() => {})));
  const when = () => shell.hasUser && !isWelcome.value;
  cleanups.push(shortcut('n', newPaper, { when }));
  cleanups.push(shortcut('/', focusSearch, { when }));
  cleanups.push(shortcut('?', () => modal('shortcuts').show(), { when }));
  cleanups.push(registerPalette());
  updateBadge();

  await firstPaint();
  api('app.ready').catch(() => {});
  // Load every page's code while idle, so later route changes render at once (no blank frame while a chunk loads).
  setTimeout(prefetchRoutes, 400);
});
onBeforeUnmount(() => cleanups.forEach((off) => off?.()));

// Cmd/Ctrl+K also works from the page itself, in case the native menu accelerator does not fire.
if (typeof window !== 'undefined') {
    window.addEventListener('keydown', (e) => {
        if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === 'k') {
            e.preventDefault();
            openPalette();
        }
    });
}
</script>

<template>
    <template v-if="ready && isWelcome">
        <div data-entering>
            <component :is="currentRoute.component" v-if="currentRoute.component" :key="currentRoute.name" />
        </div>
    </template>

    <template v-else-if="ready">
        <FluxSidebar sticky stashable class="border-r border-zinc-200/70 bg-white/60 backdrop-blur dark:border-zinc-800 dark:bg-zinc-900/60">
            <div class="flex items-center gap-1">
                <a :href="route('dashboard')" class="mr-1 flex min-w-0 flex-1 items-center space-x-2">
                    <AppLogo />
                </a>
                <FluxButton v-if="!isMac" size="sm" variant="subtle" square icon="view-columns" icon-variant="micro" aria-label="Hide sidebar" :tooltip="'Hide sidebar'" :tooltip-kbd="kbd('mod+backslash')" tooltip-position="bottom" class="shrink-0 text-zinc-400" @click="toggleSidebar" />
            </div>

            <button type="button" class="group relative flex h-10 w-full items-center gap-2.5 overflow-hidden rounded-xl bg-gradient-to-r from-indigo-600 to-violet-600 pr-2 pl-2 text-sm font-medium text-white shadow-md shadow-indigo-600/25 transition duration-200 hover:shadow-lg hover:shadow-indigo-600/30" @click="newPaper">
                <span class="pointer-events-none absolute inset-0 bg-gradient-to-r from-white/0 via-white/15 to-white/0 opacity-0 transition duration-500 group-hover:opacity-100"></span>
                <span class="relative flex size-6 shrink-0 items-center justify-center rounded-lg bg-white/20 transition"><FluxIcon name="plus" variant="micro" class="size-4" /></span>
                <span class="relative flex-1 text-left leading-none">New paper</span>
                <kbd class="relative flex h-5 min-w-5 items-center justify-center rounded-md bg-white/15 px-1.5 font-sans text-[10px] leading-none font-semibold text-white/90 ring-1 ring-white/25">N</kbd>
            </button>

            <FluxNavlist variant="outline">
                <FluxNavlistItem icon="home" :href="route('dashboard')" :current="routeIs('dashboard')">Desk</FluxNavlistItem>
                <FluxNavlistItem icon="document-text" :href="route('papers.index')" :current="routeIs('papers.*')">Papers</FluxNavlistItem>
                <FluxNavlistItem icon="trophy" :href="route('publications')" :current="routeIs('publications')">Published</FluxNavlistItem>
                <FluxNavlistItem icon="building-library" :href="route('venues')" :current="routeIs('venues')">Journals &amp; Conferences</FluxNavlistItem>
                <FluxNavlistItem icon="sparkles" :href="route('opportunities')" :current="routeIs('opportunities')" :badge="shell.newOpportunities || null">Calls for Papers</FluxNavlistItem>
                <FluxNavlistItem icon="calendar-days" :href="route('deadlines')" :current="routeIs('deadlines')">Deadlines</FluxNavlistItem>
            </FluxNavlist>

            <FluxSpacer />

            <!-- Bottom group: notifications, backup and the profile sit together without gaps. -->
            <div class="-mb-1 flex flex-col gap-0.5">
                <NotificationBell />
                <FluxDropdown position="top" align="start">
                    <button type="button" class="flex w-full items-center gap-3 rounded-xl p-1.5 text-left transition hover:bg-zinc-800/5 dark:hover:bg-white/5">
                        <UiAvatar :user="user" />
                        <span class="min-w-0 flex-1 leading-tight">
                            <span class="block truncate text-sm font-semibold text-zinc-800 dark:text-zinc-100">{{ user.name }}</span>
                            <span class="block truncate text-[11px] text-zinc-500 dark:text-zinc-400">{{ user.email }}</span>
                        </span>
                        <FluxIcon name="chevrons-up-down" variant="micro" class="size-4 shrink-0 text-zinc-400" />
                    </button>
                    <FluxMenu class="w-[220px]">
                        <div class="flex items-center gap-3 px-2 py-2">
                            <UiAvatar :user="user" size="size-10" />
                            <div class="min-w-0 leading-tight"><div class="truncate text-sm font-semibold">{{ user.name }}</div><div class="truncate text-xs text-zinc-500">{{ user.email }}</div></div>
                        </div>
                        <FluxMenuSeparator />
                        <FluxMenuItem v-if="user.is_admin" :href="route('settings.general')" icon="adjustments-horizontal">General settings</FluxMenuItem>
                        <FluxMenuItem :href="route('settings.profile')" icon="cog" :kbd="kbd('mod+,')">Settings</FluxMenuItem>
                        <FluxMenuItem :href="route('settings.research')" icon="academic-cap">Research profile</FluxMenuItem>
                        <FluxMenuItem :href="route('settings.ai')" icon="sparkles">AI settings</FluxMenuItem>
                        <FluxMenuItem :href="route('settings.appearance')" icon="sun">Appearance</FluxMenuItem>
                    </FluxMenu>
                </FluxDropdown>
            </div>

            <UpdateNotice />

            <UiFooter />
        </FluxSidebar>

        <!-- macOS: the window is dragged by the strip under the traffic lights (the sidebar has its own). -->
        <div
            v-if="isMac"
            class="fixed top-0 right-0 z-40 h-7 select-none"
            :class="collapsed ? 'left-0 bg-[#f7f8fc]/85 backdrop-blur dark:bg-zinc-950/85' : 'left-64'"
            data-tauri-drag-region
        ></div>

        <!-- Sidebar toggle, always visible like a native title-bar button: right of the traffic lights on macOS,
             top-left on Windows/Linux (in the sidebar header while the sidebar is open there). -->
        <FluxTooltip v-if="isMac || collapsed" :content="collapsed ? 'Show sidebar' : 'Hide sidebar'" :kbd="kbd('mod+backslash')" position="bottom" class="!fixed z-50" :class="isMac ? ['top-[2px] transition-[left] duration-200', collapsed ? 'left-[88px]' : 'left-[222px]'] : 'top-2 left-2'">
            <FluxButton size="xs" variant="subtle" square icon="view-columns" icon-variant="micro" :aria-label="collapsed ? 'Show sidebar' : 'Hide sidebar'" class="text-zinc-500 dark:text-zinc-400" data-sidebar-toggle @click="toggleSidebar" />
        </FluxTooltip>

        <FluxMain class="flex h-dvh min-h-0 flex-col overflow-hidden !p-0">
            <div ref="routeView" class="relative min-h-0 flex-1" data-route-view>
                <Transition name="route" @enter="onRouteEnter">
                    <div
                        v-if="currentRoute.component"
                        :key="currentRoute.name"
                        class="route-page absolute inset-0 overscroll-contain"
                        :class="pageClass"
                        :data-shell-scroll="fullBleed ? null : ''"
                        :data-full-bleed="fullBleed ? '' : null"
                    >
                        <component :is="currentRoute.component" />
                    </div>
                </Transition>
            </div>
        </FluxMain>

        <NewPaper />

        <FluxModal name="shortcuts" class="w-full max-w-sm">
            <div class="space-y-4">
                <FluxHeading size="lg">Keyboard shortcuts</FluxHeading>
                <dl class="divide-y divide-zinc-100 text-sm dark:divide-zinc-700">
                    <div v-for="[label, keys] in SHORTCUTS" :key="label" class="flex items-center justify-between py-2">
                        <dt class="text-zinc-600 dark:text-zinc-300">{{ label }}</dt>
                        <dd><kbd class="rounded-md bg-zinc-100 px-1.5 py-0.5 font-sans text-xs font-semibold text-zinc-600 ring-1 ring-zinc-200 dark:bg-zinc-700 dark:text-zinc-200 dark:ring-zinc-600">{{ keys }}</kbd></dd>
                    </div>
                </dl>
            </div>
        </FluxModal>
    </template>

    <FluxToast position="bottom end" />
    <FluxCommandPalette />
</template>
