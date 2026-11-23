<script setup>
import { nextTick, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue';
// The worker is bundled with the app (no CDN); this component and pdf.js load only when a PDF is previewed.
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';

// PDF preview with pdf.js: one canvas per page, rendered as it scrolls into view, sharp on Retina screens.
const props = defineProps({ data: { type: Uint8Array, required: true } });
const emit = defineEmits(['error']);

const MIN = 0.25;
const MAX = 5;
const STEP = 1.2;
// WebKit refuses canvases above ~16.7 million pixels.
const MAX_CANVAS_PIXELS = 16_000_000;

const scroller = ref(null);
const loading = ref(true);
const pages = shallowRef([]); // [{ n, width, height }] at scale 1 (CSS px)
const scale = ref(1);
const fit = ref(true);
const current = ref(1);
const pageInput = ref('1');

let doc = null;
let task = null;
let observer = null;
let resizer = null;
let destroyed = false;
const els = new Map(); // page number -> wrapper element
const state = new Map(); // page number -> { scale, task, visible }

function asset(dir) {
  return new URL(`pdfjs/${dir}/`, document.baseURI).href;
}

onMounted(async () => {
  try {
    const pdfjs = await import('pdfjs-dist/legacy/build/pdf.min.mjs');
    pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
    task = pdfjs.getDocument({
      data: props.data.slice(),
      cMapUrl: asset('cmaps'),
      cMapPacked: true,
      standardFontDataUrl: asset('standard_fonts'),
      wasmUrl: asset('wasm'),
      iccUrl: asset('iccs'),
      enableXfa: false,
    });
    doc = await task.promise;
    if (destroyed) return;
    const sizes = await Promise.all(
      Array.from({ length: doc.numPages }, (_, i) =>
        doc.getPage(i + 1).then((p) => {
          const v = p.getViewport({ scale: 1 });
          return { n: i + 1, width: v.width, height: v.height };
        }),
      ),
    );
    if (destroyed) return;
    pages.value = sizes;
    loading.value = false;
    await nextTick();
    fitWidth();
    observe();
    resizer = new ResizeObserver(() => { if (fit.value) fitWidth(); });
    resizer.observe(scroller.value);
    window.addEventListener('keydown', onKey);
  } catch (e) {
    if (!destroyed) emit('error', e?.name === 'PasswordException' ? 'This PDF is password-protected. Open it instead.' : `Could not show this PDF: ${e?.message || e}`);
  }
});

onBeforeUnmount(() => {
  destroyed = true;
  window.removeEventListener('keydown', onKey);
  observer?.disconnect();
  resizer?.disconnect();
  for (const s of state.values()) s.task?.cancel();
  if (task) task.destroy();
  else doc?.destroy();
});

function observe() {
  observer = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        const n = Number(e.target.dataset.page);
        const s = state.get(n) ?? {};
        s.visible = e.isIntersecting;
        state.set(n, s);
        if (e.isIntersecting) render(n);
        else release(n);
      }
    },
    { root: scroller.value, rootMargin: '800px 0px' },
  );
  for (const el of els.values()) observer.observe(el);
}

function setEl(n, el) {
  if (el) els.set(n, el);
  else els.delete(n);
}

async function render(n) {
  const s = state.get(n);
  const el = els.get(n);
  if (!doc || !s || !el || s.scale === scale.value) return;
  s.task?.cancel();
  const want = scale.value;
  s.scale = want;
  try {
    const page = await doc.getPage(n);
    if (destroyed || s.scale !== want) return;
    const viewport = page.getViewport({ scale: want });
    let ratio = window.devicePixelRatio || 1;
    const area = viewport.width * viewport.height * ratio * ratio;
    if (area > MAX_CANVAS_PIXELS) ratio *= Math.sqrt(MAX_CANVAS_PIXELS / area);
    const canvas = document.createElement('canvas');
    canvas.width = Math.floor(viewport.width * ratio);
    canvas.height = Math.floor(viewport.height * ratio);
    canvas.className = 'block size-full';
    s.task = page.render({ canvas, viewport, transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : null });
    await s.task.promise;
    s.task = null;
    if (destroyed || s.scale !== want) return;
    el.replaceChildren(canvas);
  } catch (e) {
    if (e?.name === 'RenderingCancelledException') return;
    s.scale = null;
    el.replaceChildren(Object.assign(document.createElement('p'), { className: 'p-4 text-sm text-rose-600', textContent: `Page ${n} could not be shown.` }));
  }
}

/** Free a page's canvas once it is far out of view (large PDFs stay light). */
function release(n) {
  const s = state.get(n);
  const el = els.get(n);
  if (!s || !el) return;
  s.task?.cancel();
  s.task = null;
  s.scale = null;
  const c = el.querySelector('canvas');
  if (c) { c.width = 0; c.height = 0; }
  el.replaceChildren();
}

function setScale(next, keepFit = false) {
  const el = scroller.value;
  if (!el) return;
  next = Math.min(MAX, Math.max(MIN, next));
  if (Math.abs(next - scale.value) < 0.001) return;
  // Keep the same spot of the document at the top of the view.
  const anchor = el.scrollTop / Math.max(1, el.scrollHeight);
  fit.value = keepFit;
  scale.value = next;
  nextTick(() => {
    el.scrollTop = anchor * el.scrollHeight;
    for (const [n, s] of state) if (s.visible) render(n);
  });
}

function fitWidth() {
  const el = scroller.value;
  if (!el || !pages.value.length) return;
  const widest = Math.max(...pages.value.map((p) => p.width));
  setScale((el.clientWidth - 48) / widest, true);
  fit.value = true;
}
const zoomIn = () => setScale(scale.value * STEP);
const zoomOut = () => setScale(scale.value / STEP);

function onScroll() {
  const el = scroller.value;
  if (!el) return;
  const mid = el.scrollTop + el.clientHeight / 3;
  let n = 1;
  for (const [k, p] of els) if (p.offsetTop <= mid) n = Math.max(n, k);
  if (n !== current.value) {
    current.value = n;
    pageInput.value = String(n);
  }
}

function goTo(n) {
  n = Math.min(pages.value.length, Math.max(1, Math.round(Number(n)) || 1));
  const el = els.get(n);
  if (el && scroller.value) scroller.value.scrollTop = el.offsetTop - 16;
  current.value = n;
  pageInput.value = String(n);
}

function onKey(e) {
  if (e.metaKey || e.ctrlKey || e.altKey) {
    if (e.key === '=' || e.key === '+') { e.preventDefault(); zoomIn(); }
    else if (e.key === '-') { e.preventDefault(); zoomOut(); }
    else if (e.key === '0') { e.preventDefault(); fitWidth(); }
    return;
  }
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
  const el = scroller.value;
  if (!el) return;
  switch (e.key) {
    case '+': case '=': e.preventDefault(); zoomIn(); break;
    case '-': case '_': e.preventDefault(); zoomOut(); break;
    case '0': e.preventDefault(); fitWidth(); break;
    case 'ArrowRight': case 'PageDown': e.preventDefault(); goTo(current.value + 1); break;
    case 'ArrowLeft': case 'PageUp': e.preventDefault(); goTo(current.value - 1); break;
    case 'ArrowDown': e.preventDefault(); el.scrollBy({ top: 60 }); break;
    case 'ArrowUp': e.preventDefault(); el.scrollBy({ top: -60 }); break;
    case 'Home': e.preventDefault(); goTo(1); break;
    case 'End': e.preventDefault(); goTo(pages.value.length); break;
  }
}
</script>

<template>
    <div class="flex h-full min-h-0 flex-col text-zinc-800 dark:text-zinc-200">
        <div class="flex items-center justify-between gap-2 border-b border-zinc-200 bg-white px-4 py-2 text-sm select-none dark:border-zinc-700 dark:bg-zinc-800">
            <div class="flex items-center gap-1.5 text-zinc-600 dark:text-zinc-300">
                <span>Page</span>
                <input v-model="pageInput" type="text" inputmode="numeric" aria-label="Page number" class="w-12 rounded-md border border-zinc-200 bg-white px-1.5 py-0.5 text-center text-zinc-900 tabular-nums dark:border-zinc-600 dark:bg-zinc-900 dark:text-white" :disabled="loading" @keydown.enter.prevent="goTo(pageInput)" @blur="pageInput = String(current)" />
                <span class="tabular-nums">of {{ pages.length || '…' }}</span>
            </div>
            <div class="flex items-center gap-1">
                <FluxButton size="xs" variant="ghost" icon="magnifying-glass-minus" aria-label="Zoom out" title="Zoom out (−)" :disabled="loading || scale <= MIN" @click="zoomOut" />
                <span class="w-12 text-center text-xs text-zinc-600 tabular-nums dark:text-zinc-300">{{ Math.round(scale * 100) }}%</span>
                <FluxButton size="xs" variant="ghost" icon="magnifying-glass-plus" aria-label="Zoom in" title="Zoom in (+)" :disabled="loading || scale >= MAX" @click="zoomIn" />
                <FluxButton size="xs" :variant="fit ? 'filled' : 'ghost'" icon="arrows-right-left" title="Fit width (0)" :disabled="loading" @click="fitWidth">Fit width</FluxButton>
            </div>
        </div>
        <div ref="scroller" class="relative min-h-0 flex-1 overflow-auto bg-zinc-100 dark:bg-zinc-900" tabindex="0" @scroll.passive="onScroll">
            <p v-if="loading" class="p-10 text-center text-sm text-zinc-500 dark:text-zinc-300">Opening the PDF…</p>
            <div v-else class="flex w-max min-w-full flex-col items-center gap-4 p-6">
                <div
                    v-for="p in pages"
                    :key="p.n"
                    :ref="(el) => setEl(p.n, el)"
                    :data-page="p.n"
                    class="shrink-0 bg-white shadow-sm ring-1 ring-black/5"
                    :style="{ width: `${Math.floor(p.width * scale)}px`, height: `${Math.floor(p.height * scale)}px` }"
                ></div>
            </div>
        </div>
    </div>
</template>
