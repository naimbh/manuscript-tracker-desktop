<script setup>
import { ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { provideErrors } from '@/flux/errors.js';
import { modal } from '@/lib/modal.js';
import { toast } from '@/lib/toast.js';
import { confirmDialog } from '@/lib/native.js';
import { PUBLICATION_KINDS } from '@/lib/constants.js';

const emit = defineEmits(['saved', 'deleted']);

const FIELDS = ['title', 'authors', 'kind', 'venue', 'volume', 'issue', 'pages', 'year', 'publisher',
  'citations', 'scholar_url', 'cites_url', 'doi', 'url', 'notes'];

const editingId = ref(null);
const f = ref({});
const errors = ref({});
provideErrors(errors);
const busy = ref(false);

/** open(null, userName) adds; open(row) edits. */
function open(p = null, userName = '') {
  errors.value = {};
  editingId.value = p?.id ?? null;
  f.value = Object.fromEntries(FIELDS.map((k) => [k, p?.[k] ?? '']));
  f.value.kind = p?.kind ?? 'article';
  f.value.authors = p?.authors ?? userName;
  modal('pub-form').show();
}

async function save() {
  if (busy.value) return;
  busy.value = true;
  try {
    const r = await submit(errors, () => api('publications.save', { ...f.value, ...(editingId.value ? { id: editingId.value } : {}) }));
    if (!r) return;
    modal('pub-form').close();
    toast({ variant: 'success', text: 'Publication saved.' });
    emit('saved', r);
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  } finally {
    busy.value = false;
  }
}

async function remove(id = editingId.value) {
  if (!(await confirmDialog('Remove this publication?', { title: 'Remove publication', okLabel: 'Remove', danger: true }))) return;
  try {
    await api('publications.delete', { id });
    modal('pub-form').close();
    emit('deleted', id);
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

defineExpose({ open, remove });
</script>

<template>
    <FluxModal name="pub-form" class="w-full max-w-2xl">
        <form class="space-y-4" @submit.prevent="save">
            <FluxHeading size="lg">{{ editingId ? 'Edit publication' : 'Add publication' }}</FluxHeading>
            <FluxInput v-model="f.title" name="title" label="Title" />
            <div class="grid grid-cols-3 gap-4">
                <div class="col-span-2"><FluxInput v-model="f.authors" name="authors" label="Authors" /></div>
                <FluxSelect v-model="f.kind" name="kind" label="Type"><FluxSelectOption v-for="(l, k) in PUBLICATION_KINDS" :key="k" :value="k">{{ l }}</FluxSelectOption></FluxSelect>
                <div class="col-span-2"><FluxInput v-model="f.venue" name="venue" label="Journal or conference" /></div>
                <FluxInput v-model="f.year" name="year" type="number" label="Year" />
                <FluxInput v-model="f.volume" name="volume" label="Volume" />
                <FluxInput v-model="f.issue" name="issue" label="Issue" />
                <FluxInput v-model="f.pages" name="pages" label="Pages" />
                <FluxInput v-model="f.citations" name="citations" type="number" label="Cited by" />
                <div class="col-span-2"><FluxInput v-model="f.publisher" name="publisher" label="Publisher / institution" /></div>
            </div>
            <div class="grid grid-cols-2 gap-4">
                <FluxInput v-model="f.scholar_url" name="scholar_url" label="Scholar record" placeholder="https://scholar.google.com/…" />
                <FluxInput v-model="f.cites_url" name="cites_url" label="Scholar “cited by” link" />
                <FluxInput v-model="f.doi" name="doi" label="DOI" placeholder="10.xxxx/…" />
                <FluxInput v-model="f.url" name="url" label="Publisher link" />
            </div>
            <FluxTextarea v-model="f.notes" name="notes" label="Notes" rows="2" />
            <div class="flex items-center justify-between">
                <div><FluxButton v-if="editingId" variant="ghost" class="!text-rose-600" @click="remove()">Remove</FluxButton></div>
                <div class="flex gap-2"><FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose><FluxButton type="submit" variant="primary" :disabled="busy">Save</FluxButton></div>
            </div>
        </form>
    </FluxModal>
</template>
