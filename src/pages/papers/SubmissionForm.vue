<script setup>
import { ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { DECISIONS, WAITING } from '@/lib/constants.js';
import { provideErrors } from '@/flux/errors.js';
import { modal } from '@/lib/modal.js';
import { confirmDialog } from '@/lib/native.js';
import { toDateString } from '@/lib/format.js';

const props = defineProps({ paperId: { type: Number, required: true }, venues: { type: Array, default: () => [] } });
const emit = defineEmits(['saved']);

const s = ref({});
const subId = ref(null);
const errors = ref({});
const busy = ref(false);
provideErrors(errors);

/** newSubmission(venueId?) / editSubmission(sub) from the Livewire component. */
function open(sub = null, isCurrent = false, venueId = null) {
  errors.value = {};
  if (sub) {
    subId.value = sub.id;
    s.value = {
      venue_id: sub.venue_id ?? '', venue_name: sub.venue_name, manuscript_id: sub.manuscript_id ?? '', portal_url: sub.portal_url ?? '',
      submitted_on: sub.submitted_on ?? '', status_on: sub.status_on ?? '', decision: sub.decision, decision_on: sub.decision_on ?? '',
      revision_due_on: sub.revision_due_on ?? '', note: sub.note ?? '', is_current: isCurrent,
    };
  } else {
    subId.value = null;
    const v = venueId ? props.venues.find((x) => x.id === venueId) : null;
    s.value = {
      venue_id: v?.id ?? '', venue_name: v?.name ?? '', manuscript_id: '', portal_url: v?.portal_url ?? '',
      submitted_on: toDateString(new Date()), status_on: '', decision: 'submitted', decision_on: '', revision_due_on: '', note: '', is_current: true,
    };
  }
  modal('sub-form').show();
}

/** updatedSVenueId(): picking a library venue fills the name and (if empty) the portal link. */
function venueChosen(id) {
  s.value.venue_id = id;
  const v = props.venues.find((x) => String(x.id) === String(id));
  if (v) {
    s.value.venue_name = v.name;
    s.value.portal_url = s.value.portal_url || v.portal_url || '';
  }
}

async function save() {
  busy.value = true;
  try {
    const detail = await submit(errors, () => api('papers.saveSubmission', { ...s.value, paper_id: props.paperId, id: subId.value, venue_id: s.value.venue_id || null }));
    if (!detail) return;
    modal('sub-form').close();
    emit('saved', detail, 'Submission saved.');
  } finally {
    busy.value = false;
  }
}

async function remove() {
  if (!(await confirmDialog('Remove this submission from the history?', { title: 'Remove submission', okLabel: 'Remove', danger: true }))) return;
  const detail = await api('papers.deleteSubmission', { id: subId.value });
  modal('sub-form').close();
  emit('saved', detail, 'Submission removed.');
}

defineExpose({ open });
</script>

<template>
    <FluxModal name="sub-form" class="w-full max-w-xl">
        <form class="space-y-4" @submit.prevent="save">
            <FluxHeading size="lg">{{ subId ? 'Edit submission' : 'Record submission' }}</FluxHeading>
            <div class="grid gap-4 sm:grid-cols-2">
                <FluxSelect :model-value="s.venue_id" name="venue_id" label="From Journals &amp; Conferences" @update:model-value="venueChosen">
                    <FluxSelectOption value="">Type a new one below</FluxSelectOption>
                    <FluxSelectOption v-for="v in venues" :key="v.id" :value="v.id">{{ v.name }}</FluxSelectOption>
                </FluxSelect>
                <FluxInput v-model="s.venue_name" name="venue_name" label="Journal or conference" />
                <FluxInput v-model="s.manuscript_id" name="manuscript_id" label="Manuscript ID" placeholder="RNTW-2026-0123" />
                <FluxInput v-model="s.portal_url" name="portal_url" label="Portal link" placeholder="https://" />
                <FluxInput v-model="s.submitted_on" name="submitted_on" type="date" label="Submitted on" />
                <FluxInput v-model="s.status_on" name="status_on" type="date" label="Status since" description-trailing="The day the current status began, e.g. the day it went under review" />
                <FluxSelect v-model="s.decision" name="decision" label="Status"><FluxSelectOption v-for="(l, k) in DECISIONS" :key="k" :value="k">{{ l }}</FluxSelectOption></FluxSelect>
                <FluxInput v-if="!WAITING.includes(s.decision ?? 'submitted')" v-model="s.decision_on" name="decision_on" type="date" label="Decision on" />
                <FluxInput v-if="['major', 'minor'].includes(s.decision ?? '')" v-model="s.revision_due_on" name="revision_due_on" type="date" label="Revision due" />
            </div>
            <FluxCheckbox v-model="s.is_current" name="is_current" label="Current journal" description="The journal that has this paper now. Its status sets the paper’s stage, and it is listed first." />
            <FluxTextarea v-model="s.note" name="note" label="Note" rows="2" placeholder="Editor’s comments, reasons for the decision…" />
            <div class="flex items-center justify-between">
                <div><FluxButton v-if="subId" variant="ghost" class="!text-rose-600" @click="remove">Remove</FluxButton></div>
                <div class="flex gap-2"><FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose><FluxButton type="submit" variant="primary" :disabled="busy">Save</FluxButton></div>
            </div>
        </form>
    </FluxModal>
</template>
