<script setup>
import { defineAsyncComponent, onBeforeUnmount, ref, shallowRef } from 'vue';
import { api, submit } from '@/lib/api.js';
import { FILE_KINDS } from '@/lib/constants.js';
import { provideErrors } from '@/flux/errors.js';
import { modal } from '@/lib/modal.js';
import { toast } from '@/lib/toast.js';
import { isMac, isWindows } from '@/lib/platform.js';

// pdf.js (and its worker) load only when a PDF is previewed.
const PdfViewer = defineAsyncComponent(() => import('./PdfViewer.vue'));

// The upload form (file picked in the native dialog or dropped from Finder/Explorer) and the file preview.
const props = defineProps({ paperId: { type: Number, required: true }, submissions: { type: Array, default: () => [] } });
const emit = defineEmits(['saved']);

const files = ref([]);
const fileKind = ref('manuscript');
const fileLabel = ref('');
const fileSubmission = ref('');
const errors = ref({});
const busy = ref(false);
provideErrors(errors);

// The file being edited (kind, label and submission of a stored file), or null when uploading.
const editing = ref(null);

/** Guess the kind from a file name: proofs, cover letters, responses to reviewers, anonymized copies. */
function guessKind(name = '') {
  if (/proof/i.test(name)) return 'proof';
  if (/cover/i.test(name)) return 'cover';
  if (/respon|rebuttal|reply/i.test(name)) return 'response';
  if (/anon|blind/i.test(name)) return 'anonymized';
  if (/supp|appendix/i.test(name)) return 'supplementary';
  return 'manuscript';
}

function open(dropped = [], submissionId = '') {
  editing.value = null;
  files.value = dropped;
  fileLabel.value = '';
  fileSubmission.value = submissionId || '';
  fileKind.value = dropped.length === 1 ? guessKind(dropped[0].name) : 'manuscript';
  errors.value = {};
  modal('file-form').show();
}

function edit(f) {
  editing.value = f;
  files.value = [];
  fileKind.value = f.kind;
  fileLabel.value = f.label ?? '';
  fileSubmission.value = f.submission_id ?? '';
  errors.value = {};
  modal('file-form').show();
}

async function choose() {
  try {
    const picked = await api('files.pick');
    if (picked) {
      files.value = [picked];
      if (fileKind.value === 'manuscript') fileKind.value = guessKind(picked.name);
    }
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

async function save() {
  if (editing.value) {
    busy.value = true;
    try {
      const row = await submit(errors, () => api('files.update', {
        id: editing.value.id, kind: fileKind.value, label: fileLabel.value || null, submission_id: fileSubmission.value || null,
      }));
      if (row) { modal('file-form').close(); emit('saved', 'File details saved.'); }
    } finally {
      busy.value = false;
    }
    return;
  }
  if (!files.value.length) { errors.value = { upload: ['Choose a file first.'] }; return; }
  busy.value = true;
  try {
    let n = 0;
    for (const f of files.value) {
      const row = await submit(errors, () => api('files.attach', {
        ...(f.token ? { token: f.token } : { path: f.path }), paper_id: props.paperId, kind: fileKind.value,
        label: fileLabel.value || null, submission_id: fileSubmission.value || null,
      }));
      if (!row) break;
      n++;
    }
    if (n === files.value.length) {
      modal('file-form').close();
      emit('saved', n === 1 ? 'File uploaded.' : `${n} files uploaded.`);
    } else if (n > 0) {
      files.value = files.value.slice(n);
      emit('saved', null);
    }
  } finally {
    busy.value = false;
  }
}

/* ---------- preview ---------- */
// PDFs render with pdf.js, images in an <img>, text as escaped monospace text. Other types open in Quick Look
// on macOS; elsewhere the modal offers the default app and the file manager.
const revealLabel = isMac ? 'Show in Finder' : isWindows ? 'Show in Explorer' : 'Show in folder';
const previewFile = ref(null);
const pv = ref({ status: 'idle' }); // status: loading | pdf | image | text | other | too_large | error
const pdfData = shallowRef(null);
const imageUrl = ref(null);
let seq = 0;

function mb(bytes) {
  return `${(bytes / 1048576).toFixed(bytes >= 10 * 1048576 ? 0 : 1)} MB`;
}

function decode(b64, into, at) {
  const bin = atob(b64);
  for (let i = 0; i < bin.length; i++) into[at + i] = bin.charCodeAt(i);
  return bin.length;
}

/** Read up to `max` bytes of a stored file through `files.chunk` (the backend reads it from disk in pieces). */
async function readBytes(meta, max, mine) {
  const total = Math.min(meta.size, max);
  const out = new Uint8Array(total);
  let offset = 0;
  while (offset < total) {
    const r = await api('files.chunk', { id: meta.id, offset, length: Math.min(meta.chunk, total - offset) });
    if (mine !== seq) return null;
    if (!r.length) break;
    offset += decode(r.data, out, offset);
    pv.value = { ...pv.value, progress: Math.round((offset / Math.max(1, total)) * 100) };
    if (r.eof) break;
  }
  return offset === total ? out : out.subarray(0, offset);
}

async function preview(f) {
  const mine = ++seq;
  clearPreview();
  previewFile.value = f;
  pv.value = { status: 'loading', progress: 0 };
  let meta;
  try {
    meta = await api('files.preview', { id: f.id });
  } catch (e) {
    if (mine !== seq) return;
    pv.value = { status: 'error', message: e.message };
    modal('preview').show();
    return;
  }
  if (mine !== seq) return;
  // macOS: types the app cannot draw go straight to the native Quick Look panel.
  if (meta.kind === 'other' && meta.quick_look) {
    try {
      await api('files.quickLook', { id: f.id });
      if (mine === seq) { modal('preview').close(); closePreview(); }
      return;
    } catch (e) {
      if (mine !== seq) return;
      pv.value = { status: 'error', message: e.message, meta };
      modal('preview').show();
      return;
    }
  }
  pv.value = { status: 'loading', progress: 0, meta };
  modal('preview').show();
  if (meta.kind === 'other') { pv.value = { status: 'other', meta }; return; }
  if (meta.too_large) { pv.value = { status: 'too_large', meta }; return; }
  try {
    const bytes = await readBytes(meta, meta.limit, mine);
    if (!bytes || mine !== seq) return;
    if (meta.kind === 'pdf') {
      pdfData.value = bytes;
      pv.value = { status: 'pdf', meta };
    } else if (meta.kind === 'image') {
      imageUrl.value = URL.createObjectURL(new Blob([bytes], { type: meta.mime || 'application/octet-stream' }));
      pv.value = { status: 'image', meta };
    } else {
      const text = new TextDecoder('utf-8', { fatal: false }).decode(bytes);
      pv.value = { status: 'text', meta, text, truncated: meta.size > bytes.length };
    }
  } catch (e) {
    if (mine === seq) pv.value = { status: 'error', message: e.message, meta };
  }
}

function previewError(message) {
  pv.value = { status: 'error', message, meta: pv.value.meta };
}

function clearPreview() {
  if (imageUrl.value) URL.revokeObjectURL(imageUrl.value);
  imageUrl.value = null;
  pdfData.value = null;
}

function closePreview() {
  seq++;
  clearPreview();
  previewFile.value = null;
  pv.value = { status: 'idle' };
}

async function act(name) {
  const f = previewFile.value;
  if (!f) return;
  try {
    await api(name, { id: f.id });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

async function saveCopy(f) {
  try {
    const r = await api('files.saveAs', { id: f.id });
    if (r?.saved) toast({ variant: 'success', text: 'Copy saved.' });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}
onBeforeUnmount(closePreview);

defineExpose({ open, edit, preview, saveCopy, quickLook: (f) => api('files.quickLook', { id: f.id }).catch((e) => toast({ variant: 'danger', text: e.message })) });
</script>

<template>
    <FluxModal name="file-form" class="w-full max-w-lg">
        <form class="space-y-4" @submit.prevent="save">
            <FluxHeading size="lg">{{ editing ? 'File details' : 'Upload a file' }}</FluxHeading>
            <p v-if="editing" class="truncate text-sm text-zinc-500" :title="editing.original_name">{{ editing.original_name }} · {{ editing.human_size }}</p>
            <button v-else type="button" class="flex w-full cursor-pointer flex-col items-center justify-center rounded-xl border-2 border-dashed border-zinc-300 p-6 text-center transition hover:border-indigo-400 hover:bg-indigo-50/40 dark:border-zinc-700 dark:hover:bg-indigo-500/5" @click="choose">
                <FluxIcon name="cloud-arrow-up" class="size-8 text-zinc-400" />
                <span class="mt-2 text-sm font-medium">{{ files.length ? files.map((f) => f.name).join(', ') : 'Choose a file' }}</span>
                <span class="text-xs text-zinc-500">PDF, Word, LaTeX zip… up to 50 MB · or drop files here</span>
            </button>
            <p v-if="errors.upload" class="text-sm text-rose-600">{{ errors.upload[0] }}</p>
            <p v-if="errors.paper_id || errors.submission_id" class="text-sm text-rose-600">{{ (errors.paper_id || errors.submission_id)[0] }}</p>
            <div class="grid gap-4 sm:grid-cols-2">
                <FluxSelect v-model="fileKind" name="kind" label="Kind"><FluxSelectOption v-for="(l, k) in FILE_KINDS" :key="k" :value="k">{{ l }}</FluxSelectOption></FluxSelect>
                <FluxInput v-model="fileLabel" name="label" label="Label (optional)" placeholder="v3, after R1" />
            </div>
            <FluxSelect v-if="submissions.length" v-model="fileSubmission" name="submission_id" label="For submission (optional)">
                <FluxSelectOption value="">None</FluxSelectOption>
                <FluxSelectOption v-for="sub in submissions" :key="sub.id" :value="sub.id">{{ sub.venue_name }}</FluxSelectOption>
            </FluxSelect>
            <div class="flex justify-end gap-2"><FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose><FluxButton type="submit" variant="primary" :loading="busy" :disabled="busy">{{ editing ? 'Save' : 'Upload' }}</FluxButton></div>
        </form>
    </FluxModal>

    <FluxModal name="preview" class="w-full max-w-5xl !p-0" @close="closePreview">
        <div v-if="previewFile" class="text-zinc-800 dark:text-zinc-200">
            <div class="flex items-center justify-between gap-3 border-b border-zinc-200 px-5 py-3 dark:border-zinc-700">
                <p class="min-w-0 truncate text-sm font-medium text-zinc-900 dark:text-white" :title="previewFile.original_name">{{ previewFile.label || previewFile.original_name }}</p>
                <div class="mr-8 flex shrink-0 gap-1">
                    <FluxButton v-if="pv.meta ? pv.meta.quick_look : isMac" size="sm" variant="ghost" icon="eye" @click="act('files.quickLook')">Quick Look</FluxButton>
                    <FluxButton size="sm" variant="ghost" icon="arrow-top-right-on-square" @click="act('files.open')">Open</FluxButton>
                    <FluxButton size="sm" variant="ghost" icon="arrow-down-tray" @click="saveCopy(previewFile)">Save a copy…</FluxButton>
                </div>
            </div>
            <div class="h-[78vh] overflow-hidden rounded-b-xl bg-zinc-100 dark:bg-zinc-900">
                <div v-if="pv.status === 'loading'" class="flex h-full flex-col items-center justify-center gap-2 text-sm text-zinc-500 dark:text-zinc-300">
                    <FluxIcon name="loading" class="size-6" />
                    <span>Loading preview…<template v-if="pv.progress"> {{ pv.progress }}%</template></span>
                </div>
                <PdfViewer v-else-if="pv.status === 'pdf' && pdfData" :data="pdfData" @error="previewError" />
                <div v-else-if="pv.status === 'image'" class="flex h-full items-center justify-center overflow-auto p-6">
                    <img :src="imageUrl" :alt="previewFile.original_name" class="max-h-full max-w-full object-contain shadow-sm" @error="previewError('This image could not be shown. Open it instead.')" />
                </div>
                <div v-else-if="pv.status === 'text'" class="flex h-full flex-col">
                    <p v-if="pv.truncated" class="border-b border-zinc-200 px-5 py-2 text-xs text-zinc-500 dark:border-zinc-700 dark:text-zinc-300">Showing the first {{ mb(pv.meta.limit) }} of {{ mb(pv.meta.size) }}. Open the file to see all of it.</p>
                    <pre class="min-h-0 flex-1 overflow-auto p-5 font-mono text-xs leading-relaxed whitespace-pre-wrap break-words text-zinc-800 select-text dark:text-zinc-200">{{ pv.text }}</pre>
                </div>
                <div v-else class="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
                    <FluxIcon :name="pv.status === 'error' ? 'exclamation-triangle' : 'document'" class="size-10 text-zinc-400 dark:text-zinc-500" />
                    <p class="font-medium text-zinc-900 dark:text-white">
                        <template v-if="pv.status === 'error'">The preview could not be shown</template>
                        <template v-else-if="pv.status === 'too_large'">This file is too large to preview</template>
                        <template v-else>Preview isn’t available for this file type</template>
                    </p>
                    <p class="max-w-md text-sm text-zinc-500 dark:text-zinc-300">
                        <template v-if="pv.status === 'error'">{{ pv.message }}</template>
                        <template v-else-if="pv.status === 'too_large'">It is {{ mb(pv.meta.size) }}; previews go up to {{ mb(pv.meta.limit) }}. Open it in its default app instead.</template>
                        <template v-else>Open it in its default app, or show it in the folder where the app keeps it.</template>
                    </p>
                    <div class="mt-2 flex flex-wrap justify-center gap-2">
                        <FluxButton v-if="pv.meta ? pv.meta.quick_look : isMac" size="sm" icon="eye" @click="act('files.quickLook')">Quick Look</FluxButton>
                        <FluxButton size="sm" variant="primary" icon="arrow-top-right-on-square" @click="act('files.open')">Open</FluxButton>
                        <FluxButton size="sm" icon="folder-open" @click="act('files.reveal')">{{ revealLabel }}</FluxButton>
                    </div>
                </div>
            </div>
        </div>
    </FluxModal>
</template>
