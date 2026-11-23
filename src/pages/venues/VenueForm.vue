<script setup>
import { ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { provideErrors } from '@/flux/errors.js';
import { modal } from '@/lib/modal.js';
import { toast } from '@/lib/toast.js';
import { confirmDialog } from '@/lib/native.js';
import { VENUE_KINDS, REVIEW_TYPES, PREPRINT_POLICIES } from '@/lib/constants.js';

const emit = defineEmits(['saved', 'deleted']);

const FIELDS = ['name', 'kind', 'field', 'publisher', 'acceptance_rate', 'first_decision', 'proceedings',
  'next_edition', 'deadline_on', 'deadline_label', 'deadline_expected', 'review_type', 'word_limit', 'decision_days', 'fee',
  'preprint_policy', 'portal_url', 'website_url', 'watch', 'notes'];

const editingId = ref(null);
const formOpen = ref(false);
const form = ref({});
const errors = ref({});
provideErrors(errors);
const busy = ref(false);

function blankForm(tab) {
  const f = Object.fromEntries(FIELDS.map((k) => [k, '']));
  f.kind = tab === 'journal' ? 'journal' : 'conference';
  f.review_type = 'double';
  f.preprint_policy = 'unknown';
  f.deadline_expected = false;
  f.watch = f.kind === 'conference';
  return f;
}

/** open(null, tab) adds a venue; open(id) edits one. */
async function open(id = null, tab = 'conference') {
  errors.value = {};
  editingId.value = id;
  if (id) {
    try {
      const v = await api('venues.get', { id });
      form.value = Object.fromEntries(FIELDS.map((k) => [k, v[k] ?? '']));
    } catch (e) {
      toast({ variant: 'danger', text: e.message });
      return;
    }
  } else {
    form.value = blankForm(tab);
  }
  formOpen.value = true;
  modal('venue-form').show();
}

async function save() {
  if (busy.value) return;
  busy.value = true;
  try {
    const args = { ...form.value, ...(editingId.value ? { id: editingId.value } : {}) };
    const r = await submit(errors, () => api('venues.save', args));
    if (!r) return;
    modal('venue-form').close();
    toast({ variant: 'success', text: editingId.value ? 'Journal or conference updated.' : 'Journal or conference added.' });
    emit('saved', r);
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  } finally {
    busy.value = false;
  }
}

async function remove() {
  if (!(await confirmDialog('Remove this journal or conference? Your submissions to it stay.', { title: 'Remove journal or conference', okLabel: 'Remove', danger: true }))) return;
  try {
    await api('venues.delete', { id: editingId.value });
    modal('venue-form').close();
    toast({ text: 'Journal or conference removed.' });
    emit('deleted', editingId.value);
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}

defineExpose({ open, remove: async (id) => { editingId.value = id; await remove(); } });
</script>

<template>
    <FluxModal name="venue-form" class="w-full max-w-2xl" @close="formOpen = false">
        <form v-if="formOpen" class="space-y-5" @submit.prevent="save">
            <FluxHeading size="lg">{{ editingId ? (form.kind === 'conference' ? 'Edit conference' : form.kind === 'journal' ? 'Edit journal' : 'Edit journal or conference') : 'Add journal or conference' }}</FluxHeading>
            <div class="grid grid-cols-3 gap-4">
                <div class="col-span-2"><FluxInput v-model="form.name" name="name" label="Name" placeholder="CHI 2027" /></div>
                <FluxSelect v-model="form.kind" name="kind" label="Type">
                    <FluxSelectOption v-for="(l, k) in VENUE_KINDS" :key="k" :value="k">{{ l }}</FluxSelectOption>
                </FluxSelect>
                <FluxInput v-model="form.publisher" name="publisher" label="Publisher" />
                <div class="col-span-2"><FluxInput v-model="form.field" name="field" label="Field" placeholder="Information systems" /></div>
            </div>

            <div v-if="form.kind === 'journal'" class="grid grid-cols-3 gap-4">
                <FluxInput v-model="form.first_decision" name="first_decision" label="First decision" placeholder="28 days (median)" />
                <FluxInput v-model="form.acceptance_rate" name="acceptance_rate" label="Acceptance rate" placeholder="12%" />
                <FluxInput v-model="form.fee" name="fee" label="Fee / APC" placeholder="No fee" />
                <FluxInput v-model="form.word_limit" name="word_limit" label="Word limit" placeholder="8,000 incl. references" />
                <FluxInput v-model="form.decision_days" name="decision_days" type="number" label="Usual days to decision" description-trailing="Starts the review clock." />
            </div>
            <div v-else class="grid grid-cols-3 gap-4">
                <div class="col-span-2"><FluxInput v-model="form.next_edition" name="next_edition" label="Next edition" placeholder="10–14 May 2027, Pittsburgh" /></div>
                <FluxInput v-model="form.acceptance_rate" name="acceptance_rate" label="Acceptance rate" placeholder="~25%" />
                <FluxInput v-model="form.deadline_on" name="deadline_on" type="date" label="Next deadline" />
                <FluxInput v-model="form.deadline_label" name="deadline_label" label="Deadline note" placeholder="Full papers" />
                <div class="flex items-end pb-2"><FluxCheckbox v-model="form.deadline_expected" name="deadline_expected" label="Date is expected, not official" /></div>
                <div class="col-span-2"><FluxInput v-model="form.proceedings" name="proceedings" label="Proceedings" placeholder="ACM Digital Library" /></div>
                <FluxInput v-model="form.decision_days" name="decision_days" type="number" label="Usual days to decision" />
            </div>

            <div class="grid grid-cols-2 gap-4">
                <FluxSelect v-model="form.review_type" name="review_type" label="Review">
                    <FluxSelectOption v-for="(l, k) in REVIEW_TYPES" :key="k" :value="k">{{ l }}</FluxSelectOption>
                </FluxSelect>
                <FluxSelect v-model="form.preprint_policy" name="preprint_policy" label="Preprint policy">
                    <FluxSelectOption v-for="(l, k) in PREPRINT_POLICIES" :key="k" :value="k">{{ l }}</FluxSelectOption>
                </FluxSelect>
                <FluxInput v-model="form.website_url" name="website_url" label="Website" placeholder="https://" />
                <FluxInput v-model="form.portal_url" name="portal_url" label="Submission portal" placeholder="https://" />
            </div>
            <FluxTextarea v-model="form.notes" name="notes" label="Notes" rows="2" />
            <FluxCheckbox v-model="form.watch" name="watch" label="Watch: refresh this venue with AI every week" />

            <div class="flex items-center justify-between">
                <div><FluxButton v-if="editingId" variant="ghost" class="!text-rose-600" @click="remove">Remove</FluxButton></div>
                <div class="flex gap-2">
                    <FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose>
                    <FluxButton type="submit" variant="primary" :disabled="busy">Save</FluxButton>
                </div>
            </div>
        </form>
    </FluxModal>
</template>
