<script setup>
import { after, before, beforeLast, contains, format } from '@/lib/format.js';

// A preprint shown in a paper's submission history. Emits `edit` (web: wire:click="editPaper").
defineProps({ paper: { type: Object, required: true } });
defineEmits(['edit']);
</script>

<template>
    <div class="relative pb-5 pl-7 sm:pb-6 sm:pl-8">
        <span class="absolute top-1.5 left-0 flex size-6 items-center justify-center rounded-full bg-gradient-to-br from-teal-500 to-cyan-500 text-white ring-4 ring-[#f7f8fc] dark:ring-zinc-950">
            <FluxIcon name="document-arrow-up" variant="micro" class="size-3.5" />
        </span>
        <div class="panel p-4 ring-1 ring-teal-100 dark:ring-teal-500/20">
            <div class="flex flex-wrap items-start justify-between gap-3">
                <div class="min-w-0">
                    <div class="flex flex-wrap items-center gap-2">
                        <h3 class="font-semibold">{{ before(paper.preprint_server || 'Preprint', ' (') }}</h3>
                        <span class="rounded-full bg-teal-50 px-2 py-0.5 text-xs font-medium text-teal-700 ring-1 ring-teal-200 ring-inset dark:bg-teal-500/10 dark:text-teal-300 dark:ring-teal-500/20">Preprint</span>
                    </div>
                    <p class="mt-0.5 text-sm text-zinc-500">
                        {{ paper.preprint_on ? 'Posted ' + format(paper.preprint_on, 'j M Y') : 'Posting date not set' }}
                        <template v-if="contains(paper.preprint_server, '(')"> · {{ beforeLast(after(paper.preprint_server, '('), ')') }}</template>
                    </p>
                </div>
                <div class="flex items-center gap-1">
                    <FluxButton v-if="paper.preprint_url" size="sm" icon-trailing="arrow-up-right" :href="paper.preprint_url" target="_blank">Open preprint</FluxButton>
                    <FluxButton size="sm" variant="ghost" icon="pencil-square" title="Edit preprint details" @click="$emit('edit')" />
                </div>
            </div>
            <p v-if="!paper.preprint_url" class="mt-2 text-xs text-zinc-400">Add the public link in Edit once the preprint is announced.</p>
        </div>
    </div>
</template>
