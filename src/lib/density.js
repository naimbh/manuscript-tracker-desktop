import { ref, watch } from 'vue';

const KEY = 'mt.listDensity';
let initial = 'comfortable';
try { if (localStorage.getItem(KEY) === 'compact') initial = 'compact'; } catch {}

/** Shared Compact/Comfortable choice for article lists (default Comfortable), remembered once chosen. */
const dense = ref(initial === 'compact');
watch(dense, (v) => { try { localStorage.setItem(KEY, v ? 'compact' : 'comfortable'); } catch {} });

export function useDensity() { return dense; }
