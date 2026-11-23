<script setup>
import { ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { STAGES } from '@/lib/constants.js';
import { provideErrors } from '@/flux/errors.js';
import { modal } from '@/lib/modal.js';

const props = defineProps({ paperId: { type: Number, required: true } });
const emit = defineEmits(['saved']);
const FIELDS = ['title', 'short_title', 'stage', 'started_on', 'abstract', 'method', 'dataset', 'keywords', 'overleaf_url', 'drive_url', 'preprint_server', 'preprint_url', 'preprint_on', 'notes'];

const p = ref({});
const errors = ref({});
const busy = ref(false);
provideErrors(errors);

function open(paper) {
  p.value = Object.fromEntries(FIELDS.map((f) => [f, paper[f] ?? '']));
  p.value.tags = (paper.tags ?? []).join(', ');
  errors.value = {};
  modal('paper-form').show();
}

async function save() {
  busy.value = true;
  try {
    const detail = await submit(errors, () => api('papers.update', { id: props.paperId, ...p.value }));
    if (!detail) return;
    modal('paper-form').close();
    emit('saved', detail, 'Paper saved.');
  } finally {
    busy.value = false;
  }
}

defineExpose({ open });
</script>

<template>
    <FluxModal name="paper-form" class="w-full max-w-2xl">
        <form class="space-y-4" @submit.prevent="save">
            <FluxHeading size="lg">Edit paper</FluxHeading>
            <FluxInput v-model="p.title" name="title" label="Title" />
            <div class="grid gap-4 sm:grid-cols-3">
                <FluxInput v-model="p.short_title" name="short_title" label="Short name" />
                <FluxSelect v-model="p.stage" name="stage" label="Stage"><FluxSelectOption v-for="(l, k) in STAGES" :key="k" :value="k">{{ l }}</FluxSelectOption></FluxSelect>
                <FluxInput v-model="p.keywords" name="keywords" label="Keywords" placeholder="comma separated" />
                <div class="sm:col-span-3"><FluxInput v-model="p.tags" name="tags" label="Tags" placeholder="Thesis, Preprint" description-trailing="Short labels such as Thesis or Preprint. Topics belong in Keywords." /></div>
                <FluxInput v-model="p.started_on" name="started_on" type="date" label="Started on" />
                <FluxInput v-model="p.method" name="method" label="Method" />
                <FluxInput v-model="p.dataset" name="dataset" label="Data" />
            </div>
            <FluxTextarea v-model="p.abstract" name="abstract" label="Abstract" rows="5" />
            <div class="grid gap-4 sm:grid-cols-2">
                <FluxInput v-model="p.overleaf_url" name="overleaf_url" label="Overleaf link" placeholder="https://" />
                <FluxInput v-model="p.drive_url" name="drive_url" label="Drive link" placeholder="https://" />
            </div>
            <div class="grid gap-4 sm:grid-cols-3">
                <FluxInput v-model="p.preprint_server" name="preprint_server" label="Preprint server" placeholder="arXiv" />
                <FluxInput v-model="p.preprint_url" name="preprint_url" label="Preprint link" placeholder="https://arxiv.org/abs/…" />
                <FluxInput v-model="p.preprint_on" name="preprint_on" type="date" label="Posted on" />
            </div>
            <FluxTextarea v-model="p.notes" name="notes" label="Notes" rows="3" />
            <div class="flex justify-end gap-2"><FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose><FluxButton type="submit" variant="primary" :disabled="busy">Save</FluxButton></div>
        </form>
    </FluxModal>
</template>
