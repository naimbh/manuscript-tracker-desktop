<script setup>
import { reactive, ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { provideErrors } from '@/flux/errors.js';
import { modal } from '@/lib/modal.js';
import { toast } from '@/lib/toast.js';
import { bus } from '@/lib/bus.js';
import { navigate } from '@/lib/router.js';
import { STAGES } from '@/lib/constants.js';

const blank = () => ({ title: '', short_title: '', stage: 'drafting', method: '' });
const form = reactive(blank());
const errors = ref({});
provideErrors(errors);
const busy = ref(false);

async function create() {
  if (busy.value) return;
  busy.value = true;
  try {
    const paper = await submit(errors, () => api('desk.createPaper', form));
    if (!paper) return;
    Object.assign(form, blank());
    modal('new-paper').close();
    toast({ variant: 'success', text: 'Paper added.' });
    bus.emit('papers-changed');
    navigate('papers.index', paper.id);
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  } finally {
    busy.value = false;
  }
}
</script>

<template>
    <FluxModal name="new-paper" class="w-full max-w-lg" @close="errors = {}">
        <form class="space-y-5" @submit.prevent="create">
            <div>
                <FluxHeading size="lg">New paper</FluxHeading>
                <FluxText class="mt-1">Only the title is needed. Everything else can come later.</FluxText>
            </div>

            <FluxInput v-model="form.title" name="title" label="Title" placeholder="Working title of your paper" autofocus />
            <div class="grid grid-cols-2 gap-4">
                <FluxInput v-model="form.short_title" name="short_title" label="Short name" placeholder="e.g. Thesis ch. 3" />
                <FluxSelect v-model="form.stage" name="stage" label="Stage">
                    <FluxSelectOption v-for="(v, k) in STAGES" :key="k" :value="k">{{ v }}</FluxSelectOption>
                </FluxSelect>
            </div>
            <FluxInput v-model="form.method" name="method" label="Method" placeholder="e.g. 30 interviews, survey, case study" />

            <div class="flex justify-end gap-2">
                <FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose>
                <FluxButton type="submit" variant="primary" :disabled="busy">Add paper</FluxButton>
            </div>
        </form>
    </FluxModal>
</template>
