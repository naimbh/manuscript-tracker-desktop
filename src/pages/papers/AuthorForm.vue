<script setup>
import { ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { CREDIT } from '@/lib/constants.js';
import { provideErrors } from '@/flux/errors.js';
import { modal } from '@/lib/modal.js';
import { confirmDialog } from '@/lib/native.js';

const props = defineProps({ paperId: { type: Number, required: true } });
const emit = defineEmits(['saved']);

const a = ref({});
const authorId = ref(null);
const errors = ref({});
const busy = ref(false);
provideErrors(errors);

function open(au = null) {
  authorId.value = au?.id ?? null;
  a.value = { name: au?.name ?? '', affiliation: au?.affiliation ?? '', email: au?.email ?? '', orcid: au?.orcid ?? '', corresponding: !!au?.corresponding, roles: [...(au?.roles ?? [])] };
  errors.value = {};
  modal('author-form').show();
}

async function save() {
  busy.value = true;
  try {
    const detail = await submit(errors, () => api('papers.saveAuthor', { ...a.value, paper_id: props.paperId, id: authorId.value }));
    if (!detail) return;
    modal('author-form').close();
    emit('saved', detail, 'Author saved.');
  } finally {
    busy.value = false;
  }
}

async function remove() {
  if (!(await confirmDialog('Remove this author?', { title: 'Remove author', okLabel: 'Remove', danger: true }))) return;
  const detail = await api('papers.deleteAuthor', { id: authorId.value });
  modal('author-form').close();
  emit('saved', detail, 'Author removed.');
}

defineExpose({ open });
</script>

<template>
    <FluxModal name="author-form" class="w-full max-w-xl">
        <form class="space-y-4" @submit.prevent="save">
            <FluxHeading size="lg">{{ authorId ? 'Edit author' : 'Add author' }}</FluxHeading>
            <div class="grid gap-4 sm:grid-cols-2">
                <FluxInput v-model="a.name" name="name" label="Name" />
                <FluxInput v-model="a.affiliation" name="affiliation" label="Affiliation" />
                <FluxInput v-model="a.email" name="email" label="Email" type="email" />
                <FluxInput v-model="a.orcid" name="orcid" label="ORCID" placeholder="0000-0000-0000-0000" />
            </div>
            <FluxCheckbox v-model="a.corresponding" label="Corresponding author" />
            <div>
                <p class="mb-2 text-sm font-medium">CRediT roles</p>
                <div class="grid grid-cols-2 gap-2"><FluxCheckbox v-for="r in CREDIT" :key="r" v-model="a.roles" :value="r" :label="r" /></div>
                <FluxError name="roles" />
            </div>
            <div class="flex items-center justify-between">
                <div><FluxButton v-if="authorId" variant="ghost" class="!text-rose-600" @click="remove">Remove</FluxButton></div>
                <div class="flex gap-2"><FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose><FluxButton type="submit" variant="primary" :disabled="busy">Save</FluxButton></div>
            </div>
        </form>
    </FluxModal>
</template>
