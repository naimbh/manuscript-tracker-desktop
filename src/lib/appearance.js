import { ref } from 'vue';
import { api } from './api.js';

/**
 * Flux appearance: 'light' | 'dark' | 'system'. Adds/removes the `dark` class on <html>.
 * 'system' follows prefers-color-scheme live. Stored like Flux (localStorage 'flux.appearance',
 * removed for 'system') and saved to the backend with api('app.setTheme', { theme }).
 */
const KEY = 'flux.appearance';
const media = typeof window !== 'undefined' && window.matchMedia ? window.matchMedia('(prefers-color-scheme: dark)') : null;

function read() {
  try {
    const v = window.localStorage.getItem(KEY);
    return v === 'light' || v === 'dark' ? v : 'system';
  } catch {
    return 'system';
  }
}

export const appearance = ref(read());

export function isDark(mode = appearance.value) {
  return mode === 'dark' || (mode === 'system' && !!media?.matches);
}

export function applyAppearance(mode = appearance.value) {
  document.documentElement.classList.toggle('dark', isDark(mode));
}

function store(mode) {
  try {
    if (mode === 'system') window.localStorage.removeItem(KEY);
    else window.localStorage.setItem(KEY, mode);
  } catch {}
}

/** Change the appearance. persist=false only updates the page (used when the backend tells us the value). */
export async function setAppearance(mode, { persist = true } = {}) {
  if (!['light', 'dark', 'system'].includes(mode)) mode = 'system';
  appearance.value = mode;
  store(mode);
  applyAppearance(mode);
  if (persist) {
    try { await api('app.setTheme', { theme: mode }); } catch {}
  }
}

export function initAppearance() {
  applyAppearance();
  const onChange = () => { if (appearance.value === 'system') applyAppearance(); };
  if (media?.addEventListener) media.addEventListener('change', onChange);
  else if (media?.addListener) media.addListener(onChange);
}
