<script setup>
import { computed, ref } from 'vue';
import { api } from '@/lib/api.js';
import { modal } from '@/lib/modal.js';
import { toast } from '@/lib/toast.js';
import { format, limit } from '@/lib/format.js';
import { updateBadge } from '@/components/shell/shell.js';
import { reviewText } from './runs.js';
import { usePager } from '@/lib/usePager.js';

/**
 * Review what research found before it replaces your data: one flyout per page.
 *   <ReviewPanel ref="review" name="review-venues" :kinds="['venue']" @changed="load" />
 *   review.value.open()            // everything of these kinds
 *   review.value.open(venueId)     // one venue / publication
 */
const props = defineProps({
  name: { type: String, required: true },
  kinds: { type: Array, required: true },
  heading: { type: String, default: 'Review changes' },
});
const emit = defineEmits(['changed']);

const groups = ref([]);
const target = ref(null);
const loading = ref(false);
const busy = ref(false);

async function load() {
  loading.value = true;
  try {
    const lists = await Promise.all(props.kinds.map((kind) => api('proposals.list', { kind, ...(target.value ? { target_id: target.value } : {}) })));
    groups.value = lists.flatMap((l) => l.groups ?? []);
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  } finally {
    loading.value = false;
  }
}

async function open(targetId = null) {
  target.value = targetId || null;
  groups.value = [];
  modal(props.name).show();
  await load();
}

// Long reviews are paged by item (a group may continue on the next page).
const flat = computed(() => groups.value.flatMap((g) => g.items.map((p) => ({ p, g }))));
const pager = usePager(flat, { query: null });
const pageGroups = computed(() => {
  const out = [];
  for (const { p, g } of pager.items.value) {
    const last = out[out.length - 1];
    if (last && last.g === g) last.items.push(p);
    else out.push({ g, items: [p] });
  }
  return out;
});

const count = computed(() => groups.value.reduce((n, g) => n + g.items.length, 0));

async function act(action, args, done) {
  if (busy.value) return;
  busy.value = true;
  try {
    const r = await api(action, args);
    const n = r.applied ?? r.dismissed ?? 0;
    if (done) toast({ variant: 'success', text: done(n) });
    emit('changed');
    updateBadge();
    await load();
    if (!count.value) modal(props.name).close();
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  } finally {
    busy.value = false;
  }
}

const accepted = (n) => `${n} ${n === 1 ? 'change' : 'changes'} applied.`;
const dismissed = (n) => `${n} ${n === 1 ? 'change' : 'changes'} dismissed.`;

const acceptOne = (p) => act('proposals.apply', { ids: [p.id] });
const dismissOne = (p) => act('proposals.dismiss', { ids: [p.id] });
const acceptGroup = (g) => act('proposals.apply', { kind: g.kind, target_id: g.target_id }, accepted);
const dismissGroup = (g) => act('proposals.dismiss', { kind: g.kind, target_id: g.target_id }, dismissed);
const ids = () => groups.value.flatMap((g) => g.items.map((p) => p.id));
const acceptAll = () => act('proposals.apply', { ids: ids() }, accepted);
const acceptVerified = () => act('proposals.apply', { ids: groups.value.flatMap((g) => g.items.filter((p) => p.verified).map((p) => p.id)) }, accepted);
const dismissAll = () => act('proposals.dismiss', { ids: ids() }, dismissed);
const hasVerified = computed(() => groups.value.some((g) => g.items.some((p) => p.verified)));
const hasUnverified = computed(() => groups.value.some((g) => g.items.some((p) => !p.verified)));

/** A proposal value as text (null for an empty value). */
function show(v, field) {
  if (v === null || v === undefined || v === '') return null;
  if (typeof v === 'boolean') return v ? 'Yes' : 'No';
  if (field === 'deadline_on' && /^\d{4}-\d{2}-\d{2}/.test(String(v))) return format(String(v).slice(0, 10), 'j M Y');
  if (Array.isArray(v)) return v.join(', ');
  if (typeof v === 'object') {
    const all = v.all ?? v;
    if (all && ('citations' in all || 'h' in all)) {
      return [all.citations != null && `${all.citations} citations`, all.h != null && `h-index ${all.h}`, all.i10 != null && `i10-index ${all.i10}`].filter(Boolean).join(' · ');
    }
    return JSON.stringify(v);
  }
  return String(v);
}

defineExpose({ open, load });
</script>

<template>
    <FluxModal :name="name" variant="flyout" class="w-full max-w-xl">
        <div class="space-y-5">
            <div>
                <FluxHeading size="lg">{{ heading }}</FluxHeading>
                <FluxText class="mt-1">The research found these values. Nothing changes until you accept it.</FluxText>
            </div>

            <div v-if="count" class="flex flex-wrap items-center gap-2 select-none">
                <span class="num mr-auto text-xs text-zinc-500">{{ reviewText(count) }}</span>
                <FluxButton v-if="hasVerified && hasUnverified" size="sm" variant="ghost" :disabled="busy" @click="acceptVerified">Accept verified</FluxButton>
                <FluxButton size="sm" variant="ghost" :disabled="busy" @click="dismissAll">Dismiss all</FluxButton>
                <FluxButton size="sm" variant="primary" icon="check" :disabled="busy" @click="acceptAll">Accept all</FluxButton>
            </div>

            <div v-if="loading && !groups.length" class="flex items-center gap-2 py-8 text-sm text-zinc-500"><FluxIcon name="arrow-path" variant="micro" class="animate-spin" /> Loading…</div>
            <p v-else-if="!groups.length" class="rounded-xl border border-dashed border-zinc-200 px-4 py-10 text-center text-sm text-zinc-500 dark:border-zinc-700">Nothing to review. Everything found has been applied or dismissed.</p>

            <UiList class="space-y-5">
            <section v-for="{ g, items } in pageGroups" :key="g.kind + g.target_id" class="space-y-2">
                <div class="flex items-center gap-2 select-none">
                    <h3 class="min-w-0 flex-1 truncate text-sm font-semibold" :title="g.target">{{ g.target ?? g.kind_label }}</h3>
                    <FluxButton v-if="groups.length > 1 && g.items.length > 1" size="xs" variant="ghost" :disabled="busy" @click="dismissGroup(g)">Dismiss all</FluxButton>
                    <FluxButton v-if="groups.length > 1 && g.items.length > 1" size="xs" variant="ghost" icon="check" :disabled="busy" @click="acceptGroup(g)">Accept all</FluxButton>
                </div>
                <UiList class="divide-y divide-zinc-100 rounded-xl border border-zinc-200 dark:divide-zinc-700/70 dark:border-zinc-700">
                    <article v-for="p in items" :key="p.id" class="space-y-2 p-3">
                        <div class="flex items-center gap-2">
                            <span class="text-xs font-medium text-zinc-500 select-none">{{ p.field_label }}</span>
                            <span v-if="p.verified" class="inline-flex items-center gap-0.5 rounded-full bg-emerald-50 px-1.5 py-px text-[10px] font-semibold text-emerald-700 select-none dark:bg-emerald-500/10 dark:text-emerald-300"><FluxIcon name="check-badge" variant="micro" class="size-3" />verified</span>
                            <span v-else class="rounded-full border border-dashed border-amber-400 px-1.5 text-[10px] font-medium text-amber-600 select-none" title="The quote was not found on the source page. Check it before accepting.">unverified</span>
                            <span class="flex-1"></span>
                            <FluxButton size="xs" variant="ghost" :disabled="busy" @click="dismissOne(p)">Dismiss</FluxButton>
                            <FluxButton size="xs" variant="filled" icon="check" :disabled="busy" @click="acceptOne(p)">Accept</FluxButton>
                        </div>
                        <div class="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-start gap-2 text-sm">
                            <span class="break-words text-zinc-500" :class="show(p.current, p.field) !== null && 'line-through decoration-zinc-300'">{{ show(p.current, p.field) ?? 'Empty' }}</span>
                            <FluxIcon name="arrow-right" variant="micro" class="mt-1 size-3.5 text-zinc-400" />
                            <span class="font-medium break-words">{{ show(p.proposed, p.field) ?? 'Empty' }}</span>
                        </div>
                        <p v-if="p.quote || p.source_url" class="text-xs text-zinc-500">
                            <span v-if="p.quote" class="italic">“{{ limit(p.quote, 280) }}”</span>
                            <span v-if="p.quote && p.source_url"> — </span>
                            <a v-if="p.source_url" :href="p.source_url" class="inline-flex items-center gap-0.5 font-medium text-indigo-600 hover:underline dark:text-indigo-400" :title="p.source_url">{{ p.source_name || 'Source' }} <FluxIcon name="arrow-up-right" variant="micro" class="size-3" /></a>
                        </p>
                        <p v-else-if="p.source_name" class="text-xs text-zinc-400">From {{ p.source_name }}</p>
                    </article>
                </UiList>
            </section>
            </UiList>
            <FluxPagination :pager="pager" :sizes="false" label="Changes" />
        </div>
    </FluxModal>
</template>
