<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { api } from '@/lib/api.js';
import { bus } from '@/lib/bus.js';
import { on } from '@/lib/events.js';
import { DECISIONS, FILE_KINDS, STAGES, isRejection, isWaiting, stageFor, tone } from '@/lib/constants.js';
import { before, diffForHumans, diffInDays, format, limit, plural } from '@/lib/format.js';
import { modal } from '@/lib/modal.js';
import { confirmDialog, contextMenu } from '@/lib/native.js';
import { isMac, isWindows } from '@/lib/platform.js';
import { currentRoute, route, setQuery } from '@/lib/router.js';
import { toast } from '@/lib/toast.js';
import { cache, copyText, DOT, FILE_GRADIENT, ICON_OF, openExternal, papersChanged, pending, PILL, RING_TONE, takePending, utc } from './state.js';
import { moveItem, startSort } from './sortable.js';
import PaperForm from './PaperForm.vue';
import SubmissionForm from './SubmissionForm.vue';
import AuthorForm from './AuthorForm.vue';
import { markEntering } from '@/lib/motion.js';
import FileForms from './FileForms.vue';

const props = defineProps({
  id: { type: Number, required: true },
  wide: { type: Boolean, default: true },
  backHref: { type: String, default: '#/papers' },
  dropping: { type: Boolean, default: false },
});
const emit = defineEmits(['delete', 'deleted']);

const detail = ref(cache.details.get(props.id) ?? null);
// The detail mounts per paper: its own entrance plays then (and when its tab changes), never on refreshes.
const rootEl = ref(null);
let stopEnter = null;
watch(rootEl, (el) => { if (el) { stopEnter?.(); stopEnter = markEntering(el); } });
const paperForm = ref(null);
const subForm = ref(null);
const authorForm = ref(null);
const fileForms = ref(null);

const tab = computed(() => (currentRoute.query.tab === 'files' ? 'files' : 'submissions'));
const TABS = { submissions: 'Overview', files: 'Files' };
function setTab(k) { setQuery({ tab: k === 'submissions' ? null : k }); }

function set(d) {
  detail.value = d;
  cache.details.set(props.id, d);
  document.title = d.paper.display_title || 'Papers';
}

async function load() {
  try {
    set(await api('papers.get', { id: props.id }));
    runPending();
  } catch (e) {
    if (e.message === 'Not found.') emit('deleted');
    else toast({ variant: 'danger', text: e.message });
  }
}

/** The Livewire changed(): new data, a toast, and the list/Desk/sidebar told. */
function changed(d, msg) {
  if (d) set(d.paper ? d : d.detail);
  if (msg) toast({ variant: 'success', text: msg });
  papersChanged('papers-detail', props.id);
}

async function act(name, args, msg = null) {
  try {
    const d = await api(name, args);
    changed(d, typeof msg === 'function' ? msg(d) : msg);
    return d;
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
    return null;
  }
}

/* ---------- derived (Livewire with() and the @php blocks) ---------- */
const p = computed(() => detail.value?.paper ?? {});
const subs = computed(() => detail.value?.submissions ?? []);
const cur = computed(() => subs.value.find((s) => s.id === detail.value?.current_id) ?? null);
const queue = computed(() => detail.value?.queue ?? []);
const authors = computed(() => detail.value?.authors ?? []);
const files = computed(() => detail.value?.files ?? []);
const deadlines = computed(() => detail.value?.deadlines ?? []);
const venues = computed(() => detail.value?.venues ?? []);
const risk = computed(() => detail.value?.risk ?? null);
async function dismissRisk() {
  try {
    await api('app.dismissNotice', { key: risk.value.key });
    await load();
    papersChanged('papers-detail', props.id);
    toast({ text: 'Warning dismissed for this submission. Settings → General brings dismissed warnings back.' });
  } catch (e) {
    toast({ variant: 'danger', text: e.message });
  }
}
const proof = computed(() => files.value.find((f) => f.kind === 'proof') ?? null);

const chron = computed(() => [...subs.value].sort((a, b) => ((a.submitted_on ?? '') < (b.submitted_on ?? '') ? -1 : (a.submitted_on ?? '') > (b.submitted_on ?? '') ? 1 : a.id - b.id)));
const first = computed(() => chron.value[0]?.submitted_on ?? p.value.preprint_on ?? null);
const rejections = computed(() => chron.value.filter(isRejection).length);
const hasPreprint = computed(() => !!(p.value.preprint_server || p.value.preprint_url));
const preprintName = computed(() => (hasPreprint.value ? before(p.value.preprint_server || 'Preprint', ' (') : null));
const steps = computed(() => {
  const list = chron.value.map((x) => ({ type: 'sub', sub: x, date: x.submitted_on }));
  if (hasPreprint.value) list.push({ type: 'preprint', date: p.value.preprint_on });
  const key = (x) => [x.date ?? '￿', x.type === 'preprint' ? 1 : 0];
  return list.sort((a, b) => { const [x1, y1] = key(a), [x2, y2] = key(b); return x1 < x2 ? -1 : x1 > x2 ? 1 : y1 - y2; });
});
const showNext = computed(() => queue.value.length && (!cur.value || isRejection(cur.value)));
const decisionBadge = computed(() => cur.value && !(p.value.stage === stageFor(cur.value.decision) && ['submitted', 'under_review'].includes(p.value.stage)));

/** Submission history: current journal first, the preprint in date order among the earlier ones. */
const history = computed(() => {
  const ordered = [...subs.value].sort((a, b) => (b.id === cur.value?.id) - (a.id === cur.value?.id));
  const out = [];
  let shown = false;
  ordered.forEach((s, i) => {
    const isCur = cur.value?.id === s.id;
    if (hasPreprint.value && !shown && !isCur && p.value.preprint_on && s.submitted_on && p.value.preprint_on >= s.submitted_on) {
      out.push({ type: 'preprint', key: 'pre' });
      shown = true;
    }
    out.push({ type: 'sub', key: 's' + s.id, sub: s, isCur, last: i === ordered.length - 1 });
  });
  if (hasPreprint.value && !shown) out.push({ type: 'preprint', key: 'pre' });
  return out;
});

const daysSinceFirst = computed(() => (first.value ? Math.trunc(diffInDays(first.value)) : 0));
const words = computed(() => (String(p.value.abstract ?? '').match(/[A-Za-z][A-Za-z'-]*/g) ?? []).length);
const keywords = computed(() => String(p.value.keywords ?? '').split(/[;,]/).map((k) => k.trim()).filter(Boolean));
const initials = (name) => String(name ?? '').split(' ').map((x) => Array.from(x)[0] ?? '').slice(0, 2).join('');
const authorMeta = (au) => [au.affiliation, au.email, au.orcid ? 'ORCID ' + au.orcid : null].filter(Boolean).join(' · ');
const doneN = (s) => s.revision_items.filter((x) => x.done).length;
const ringTone = (c) => (c ? (c.overdue ? 'red' : c.pct > 75 ? 'amber' : 'indigo') : 'zinc');
const fileExt = (f) => limit((String(f.original_name).includes('.') ? String(f.original_name).split('.').pop() : ''), 4, '');
const revealLabel = isMac ? 'Show in Finder' : isWindows ? 'Show in Explorer' : 'Show in folder';

/* ---------- local toggles (Alpine x-data) ---------- */
const openItems = reactive({});
const isItemsOpen = (s) => openItems[s.id] ?? (doneN(s) < s.revision_items.length || !s.revision_items.length);
const respond = reactive({});
const notesOpen = reactive({});
const abstractOpen = ref(false);
const copied = ref(false);

/* ---------- paper ---------- */
function editPaper() { paperForm.value?.open(p.value); }
function setStage(k) { act('papers.setStage', { id: props.id, stage: k }, `Stage set to ${STAGES[k]}.`); }
function onSaved(d, msg) { changed(d, msg); }

/* ---------- status history: one line of steps on each submission card ---------- */
const hasTimeline = (s) => (s.timeline?.length ?? 0) > 1;
const stepLabel = (st, k) => (st.status === 'submitted' && k > 0 ? 'Resubmitted' : DECISIONS[st.status] ?? st.status);
const stepDate = (d) => format(d, String(d).slice(0, 4) === String(new Date().getFullYear()) ? 'j M' : 'j M Y');
/** Days from one step to the next, when both have dates. */
function stepGap(line, k) {
  const a = line[k]?.on_date, b = line[k + 1]?.on_date;
  if (!a || !b) return null;
  const d = Math.round(diffInDays(a, b));
  return d >= 0 ? d : null;
}
function removeStep(st) { act('papers.removeStep', { id: st.id }, 'Step removed from the history.'); }

/* ---------- submissions ---------- */
function newSubmission() { subForm.value?.open(null); }
function editSubmission(s) { subForm.value?.open(s, s.id === cur.value?.id); }
function makeCurrent(s) { act('papers.makeCurrent', { id: s.id }, `${s.venue_name} is now the current journal.`); }
function setDecision(s, k) { act('papers.setDecision', { id: s.id, decision: k }, `Decision recorded: ${DECISIONS[k]}.`); }
async function subMenu(s, isCur) {
  const id = await contextMenu([
    { id: 'edit', label: 'Edit submission…' },
    ...(isCur ? [] : [{ id: 'current', label: 'Make current journal' }]),
    ...(s.portal_url ? [{ id: 'portal', label: 'Open portal' }] : []),
    { id: 'comments', label: 'Paste comments…' },
    { separator: true },
    { id: 'remove', label: 'Remove…', danger: true },
  ]);
  if (id === 'edit') editSubmission(s);
  else if (id === 'current') makeCurrent(s);
  else if (id === 'portal') openExternal(s.portal_url);
  else if (id === 'comments') openChecklist(s);
  else if (id === 'remove' && (await confirmDialog('Remove this submission from the history?', { title: 'Remove submission', okLabel: 'Remove', danger: true }))) {
    act('papers.deleteSubmission', { id: s.id }, 'Submission removed.');
  }
}
function submitNext() {
  act('papers.submitNext', { paper_id: props.id }, (d) => (d.submission ? `Submitted to ${d.submission.venue_name}. Add the manuscript ID when you have it.` : null));
}

/* ---------- queue ---------- */
const queueVenue = ref(null);
async function addToQueue() {
  if (!queueVenue.value) return;
  const d = await act('papers.addToQueue', { paper_id: props.id, venue_id: queueVenue.value }, (r) => (r.venue ? `${r.venue.name} added to the queue.` : null));
  if (d) queueVenue.value = null;
}
function removeQueue(q) { act('papers.removeQueue', { id: q.id }); }

/* ---------- reordering (buttons and pointer drag) ---------- */
function reorder(kind, parentId, list, from, to) {
  if (to < 0 || to >= list.length || from === to) return;
  const ids = moveItem(list.map((x) => x.id), from, to);
  list.splice(0, list.length, ...moveItem(list, from, to));
  act('papers.reorder', { kind, parent_id: parentId, ids });
}
function drag(e, kind, parentId, list, index) {
  startSort(e, e.currentTarget.closest('[data-sort-list]'), index, (from, to) => reorder(kind, parentId, list, from, to));
}

/* ---------- revision checklist ---------- */
const checklistFor = ref(null);
const comments = ref('');
function openChecklist(s) { checklistFor.value = s.id; comments.value = ''; modal('comments').show(); }
async function importComments() {
  const d = await act('papers.importComments', { submission_id: checklistFor.value, comments: comments.value }, (r) => `${r.added} reviewer ${plural('comment', r.added)} added to the checklist.`);
  if (d) modal('comments').close();
}
function toggleItem(it) { act('papers.toggleItem', { id: it.id }); }
function deleteItem(it) { act('papers.deleteItem', { id: it.id }); }
async function saveResponse(it, value) {
  if ((it.response ?? '') === value) return;
  try {
    await api('papers.saveResponse', { id: it.id, response: value });
    it.response = value.slice(0, 5000);
  } catch (e) { toast({ variant: 'danger', text: e.message }); }
}

/* ---------- authors ---------- */
function editAuthor(au = null) { authorForm.value?.open(au); }
async function copyCredit() {
  if (await copyText(detail.value.credit)) { copied.value = true; setTimeout(() => (copied.value = false), 1500); }
}

/* ---------- files ---------- */
// Files attached from a submission card stay on the Overview; other uploads switch to the Files tab.
let stayOnTab = false;
function openUpload(dropped = [], submissionId = '') { stayOnTab = !!submissionId; fileForms.value?.open(dropped, submissionId); }
function editFile(f) { stayOnTab = true; fileForms.value?.edit(f); }
const filesOf = (sub) => files.value.filter((f) => f.submission_id === sub.id);
async function onFileSaved(msg) {
  await load();
  papersChanged('papers-detail', props.id);
  if (msg) {
    toast({ variant: 'success', text: msg });
    if (tab.value !== 'files' && !stayOnTab) setTab('files');
  }
  stayOnTab = false;
}
function preview(f) { fileForms.value?.preview(f); }
async function fileAction(name, f) {
  try { await api(name, { id: f.id }); } catch (e) { toast({ variant: 'danger', text: e.message }); }
}
async function deleteFile(f) {
  if (!(await confirmDialog('Delete this file?', { title: 'Delete file', okLabel: 'Delete', danger: true }))) return;
  try {
    await api('files.delete', { id: f.id });
    await load();
    toast({ variant: 'success', text: 'File deleted.' });
    papersChanged('papers-detail', props.id);
  } catch (e) { toast({ variant: 'danger', text: e.message }); }
}
async function fileMenu(f) {
  const id = await contextMenu([
    { id: 'preview', label: 'Preview' },
    { id: 'edit', label: 'Edit details…' },
    ...(isMac ? [{ id: 'quicklook', label: 'Quick Look' }] : []),
    { id: 'open', label: 'Open' },
    { id: 'reveal', label: revealLabel },
    { id: 'save', label: 'Save a copy…' },
    { separator: true },
    { id: 'delete', label: 'Delete file…', danger: true },
  ]);
  if (id === 'preview') preview(f);
  else if (id === 'edit') editFile(f);
  else if (id === 'quicklook') fileForms.value?.quickLook(f);
  else if (id === 'open') fileAction('files.open', f);
  else if (id === 'reveal') fileAction('files.reveal', f);
  else if (id === 'save') fileForms.value?.saveCopy(f);
  else if (id === 'delete') deleteFile(f);
}

/* ---------- actions handed over from the list (context menu, file drops) ---------- */
function runPending() {
  const job = takePending(props.id);
  if (!job) return;
  if (job.action === 'new-submission') { if (tab.value !== 'submissions') setTab('submissions'); newSubmission(); }
  else if (job.action === 'attach') openUpload(job.files);
}
watch(() => pending.action, () => { if (detail.value) nextTick(runPending); });

/* ---------- external links open in the browser ---------- */
function onClick(e) {
  const a = e.target.closest?.('a[href]');
  const href = a?.getAttribute('href') ?? '';
  if (/^(https?:|mailto:)/i.test(href)) { e.preventDefault(); openExternal(href); }
}

/* ---------- tab ink ---------- */
const tabNav = ref(null);
const ink = ref({});
function measure() {
  const el = tabNav.value?.querySelector('[aria-current]');
  if (el) ink.value = { left: el.offsetLeft + 'px', width: el.offsetWidth + 'px' };
}
let ro = null;
const offs = [];
watch([tab, () => files.value.length, detail], () => nextTick(measure));
watch(tab, () => { if (rootEl.value) { stopEnter?.(); stopEnter = markEntering(rootEl.value); } });

onMounted(() => {
  if (detail.value) document.title = detail.value.paper.display_title || 'Papers';
  load();
  nextTick(measure);
  ro = new ResizeObserver(measure);
  offs.push(bus.on('papers-changed', (e) => { if (e?.source !== 'papers-detail') load(); }));
  offs.push(on('changed', (x) => { if (['papers', 'venues', 'deadlines', 'external'].includes(x?.area)) load(); }));
});
watch(tabNav, (el) => { if (el && ro) { ro.disconnect(); ro.observe(el); measure(); } });
onBeforeUnmount(() => { ro?.disconnect(); offs.forEach((f) => f()); stopEnter?.(); });
</script>

<template>
    <div v-if="detail" ref="rootEl" class="page-in relative flex min-h-0 flex-1 flex-col" data-paper-drop @click="onClick">
        <div v-if="dropping" class="pointer-events-none absolute inset-2 z-30 flex items-center justify-center rounded-2xl border-2 border-dashed border-indigo-400 bg-indigo-50/80 text-sm font-medium text-indigo-700 backdrop-blur-sm dark:bg-indigo-500/10 dark:text-indigo-300">
            <span class="flex items-center gap-2"><FluxIcon name="paper-clip" variant="mini" /> Drop files to attach them to this paper</span>
        </div>

        <!-- Header -->
        <header :class="['border-b border-zinc-200/80 px-4 select-none sm:px-6 lg:px-8 dark:border-zinc-800', isMac ? 'pt-9' : 'pt-4 sm:pt-5']" data-tauri-drag-region>
            <a v-if="!wide" :href="backHref" class="mb-3 inline-flex items-center gap-1 text-xs text-zinc-500 hover:text-indigo-600"><FluxIcon name="chevron-left" variant="micro" /> Papers</a>
            <div class="flex flex-col-reverse items-start gap-3 sm:flex-row sm:justify-between sm:gap-4" data-tauri-drag-region>
                <div class="min-w-0">
                    <div class="flex flex-wrap items-center gap-2">
                        <FluxDropdown>
                            <button type="button" class="inline-flex h-6 items-center rounded-full border border-zinc-200 px-2.5 transition hover:border-indigo-300 dark:border-zinc-700"><UiStage :stage="p.stage" /></button>
                            <FluxMenu>
                                <FluxMenuItem v-for="(l, k) in STAGES" :key="k" @click="setStage(k)">{{ l }}</FluxMenuItem>
                            </FluxMenu>
                        </FluxDropdown>
                        <UiDecision v-if="decisionBadge" :s="cur" />
                        <span v-if="p.short_title" class="text-xs text-zinc-400">{{ p.short_title }}</span>
                        <a v-for="t in p.tags ?? []" :key="t" :href="route('papers.index', { paper: id, q: '#' + t, filter: currentRoute.query.filter, sort: currentRoute.query.sort })" class="inline-flex"><UiTag :tag="t" class="h-6 px-2.5" /></a>
                    </div>
                    <h1 class="mt-2 max-w-4xl text-lg leading-snug font-semibold tracking-tight text-balance select-text sm:text-xl xl:text-[1.6rem]">{{ p.title }}</h1>
                    <p v-if="p.method" class="mt-1 text-sm text-zinc-500 select-text">{{ p.method }}</p>
                </div>
                <div class="flex shrink-0 gap-1 self-end sm:self-auto">
                    <FluxButton size="sm" icon="pencil-square" @click="editPaper">Edit</FluxButton>
                    <FluxDropdown align="end">
                        <FluxButton size="sm" variant="ghost" icon="ellipsis-horizontal" aria-label="More actions" />
                        <FluxMenu>
                            <FluxMenuItem icon="trash" variant="danger" @click="emit('delete')">Delete paper</FluxMenuItem>
                        </FluxMenu>
                    </FluxDropdown>
                </div>
            </div>

            <nav ref="tabNav" class="relative mt-5 flex gap-6 text-sm">
                <button v-for="(l, k) in TABS" :key="k" type="button" :aria-current="tab === k ? 'page' : undefined" @click="setTab(k)"
                        :class="['pb-3 font-medium transition', tab === k ? 'text-zinc-900 dark:text-white' : 'text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200']">
                    {{ l }} <span v-if="k === 'files' && files.length" class="num ml-0.5 text-xs text-zinc-400">{{ files.length }}</span>
                </button>
                <span class="tab-ink" :style="`left:${ink.left};width:${ink.width}`"></span>
            </nav>
        </header>

        <div :key="'tab-' + tab" class="@container flex-1 overflow-y-auto px-4 py-5 sm:px-6 sm:py-6 lg:px-8">
            <FluxCallout v-if="risk" variant="danger" icon="eye" class="mb-5" heading="Anonymity risk">
                <FluxCalloutText>This paper has a public preprint ({{ p.preprint_server || 'link' }}) while it is under double-anonymous review at {{ risk.venue_name }}. Check the venue’s preprint policy, and do not cite the preprint in the anonymized file.</FluxCalloutText>
                <template #controls><FluxButton size="sm" variant="subtle" icon="x-mark" aria-label="Dismiss this warning" title="Dismiss (you can bring it back in Settings → General)" @click="dismissRisk" /></template>
            </FluxCallout>

            <!-- ================= PAPER: submissions + overview ================= -->
            <div v-if="tab === 'submissions'" class="grid grid-cols-[minmax(0,1fr)] gap-6 @4xl:grid-cols-3">
                <div class="min-w-0 space-y-6 @4xl:col-span-2">
                    <section v-if="steps.length" class="panel relative overflow-hidden p-4 sm:p-5">
                        <div class="pointer-events-none absolute -top-20 -right-10 size-56 rounded-full bg-gradient-to-br from-indigo-400/10 via-fuchsia-400/5 to-transparent blur-2xl"></div>
                        <div class="relative flex flex-wrap items-baseline justify-between gap-2">
                            <h2 class="font-semibold">Journey</h2>
                            <span class="text-xs text-zinc-500">{{ first ? 'First sent ' + format(first, 'j M Y') : '' }}</span>
                        </div>

                        <!-- Stepper -->
                        <div class="relative mt-3 -mx-2 overflow-x-auto px-2 pt-2 pb-1">
                            <ol class="flex min-w-max items-start">
                                <li v-for="(step, i) in steps" :key="step.type === 'sub' ? 'j' + step.sub.id : 'pre'" class="flex items-start">
                                    <div v-if="step.type === 'preprint'" class="enter-up flex w-28 flex-col items-center text-center" :style="`--d: ${i * 0.08}s`">
                                        <span class="relative flex size-9 items-center justify-center rounded-full bg-gradient-to-br from-teal-500 to-cyan-500 text-white ring-4 ring-teal-100 dark:ring-teal-500/20"><FluxIcon name="document-arrow-up" variant="mini" class="size-4" /></span>
                                        <span class="mt-2 line-clamp-2 text-xs leading-tight font-medium text-zinc-700 dark:text-zinc-200">{{ preprintName }}</span>
                                        <span class="mt-0.5 text-[10px] text-zinc-400">{{ p.preprint_on ? format(p.preprint_on, 'M Y') : 'date not set' }}</span>
                                        <a v-if="p.preprint_url" :href="p.preprint_url" target="_blank" class="mt-1 rounded-full bg-teal-50 px-1.5 py-px text-[10px] font-medium text-teal-700 hover:underline dark:bg-teal-500/10 dark:text-teal-300">Preprint ↗</a>
                                        <span v-else class="mt-1 rounded-full bg-teal-50 px-1.5 py-px text-[10px] font-medium text-teal-700 dark:bg-teal-500/10 dark:text-teal-300">Preprint</span>
                                    </div>
                                    <div v-else class="enter-up flex w-28 flex-col items-center text-center" :style="`--d: ${i * 0.08}s`">
                                        <span :class="['relative flex size-9 items-center justify-center rounded-full text-white ring-4', RING_TONE[tone(step.sub)], cur?.id === step.sub.id ? 'shadow-lg shadow-indigo-500/30' : 'opacity-60']">
                                            <span v-if="cur?.id === step.sub.id && isWaiting(step.sub)" class="absolute inset-0 animate-ping rounded-full bg-sky-400/40"></span>
                                            <FluxIcon :name="ICON_OF[tone(step.sub)]" variant="mini" class="relative size-4" />
                                        </span>
                                        <span :class="['mt-2 line-clamp-2 text-xs leading-tight font-medium', cur?.id === step.sub.id ? 'text-zinc-900 dark:text-white' : 'text-zinc-600 dark:text-zinc-300']">{{ before(step.sub.venue_name, ' (') }}</span>
                                        <span class="mt-0.5 text-[10px] text-zinc-400">{{ format(step.sub.submitted_on, 'M Y') }}</span>
                                        <span :class="['mt-1 rounded-full px-1.5 py-px text-[10px] font-medium', PILL[tone(step.sub)]]">{{ DECISIONS[step.sub.decision] }}<template v-if="step.sub.days !== null"> · {{ step.sub.days }}d</template></span>
                                    </div>
                                    <div v-if="i < steps.length - 1" class="mt-4 h-0.5 w-6 shrink-0 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-700"><div class="grow-x h-full bg-gradient-to-r from-zinc-300 to-indigo-300 dark:from-zinc-600 dark:to-indigo-500" :style="`animation-delay: ${0.1 + i * 0.08}s`"></div></div>
                                </li>
                                <li v-if="showNext" class="flex items-start">
                                    <div class="mt-4 h-0.5 w-10 shrink-0 border-t-2 border-dashed border-violet-300"></div>
                                    <div class="flex w-32 flex-col items-center text-center">
                                        <span class="flex size-9 items-center justify-center rounded-full border-2 border-dashed border-violet-400 text-violet-500"><FluxIcon name="arrow-right" variant="mini" class="size-4" /></span>
                                        <span class="mt-2 line-clamp-2 text-xs leading-tight font-medium text-violet-600 dark:text-violet-400">{{ before(queue[0].venue_name, ' (') }}</span>
                                        <span class="mt-0.5 text-[10px] text-zinc-400">next</span>
                                    </div>
                                </li>
                            </ol>
                        </div>

                        <!-- Stat tiles -->
                        <div v-if="chron.length" class="relative mt-5 grid grid-cols-2 gap-2 sm:grid-cols-4 sm:gap-3">
                            <div class="rounded-xl bg-gradient-to-br from-indigo-50/50 to-white p-3 ring-1 ring-indigo-100/70 dark:from-indigo-500/[.06] dark:to-transparent dark:ring-indigo-500/15">
                                <p class="text-[11px] text-zinc-500">Journals tried</p><UiCount :value="chron.length" class="text-2xl font-semibold" />
                            </div>
                            <div class="rounded-xl bg-gradient-to-br from-rose-50/50 to-white p-3 ring-1 ring-rose-100/70 dark:from-rose-500/[.06] dark:to-transparent dark:ring-rose-500/15">
                                <p class="text-[11px] text-zinc-500">Rejections</p><UiCount :value="rejections" class="text-2xl font-semibold" />
                            </div>
                            <div class="rounded-xl bg-gradient-to-br from-sky-50/50 to-white p-3 ring-1 ring-sky-100/70 dark:from-sky-500/[.06] dark:to-transparent dark:ring-sky-500/15">
                                <p class="text-[11px] text-zinc-500">Days since first sent</p><UiCount :value="daysSinceFirst" class="text-2xl font-semibold" />
                            </div>
                            <div class="rounded-xl bg-gradient-to-br from-amber-50/50 to-white p-3 ring-1 ring-amber-100/70 dark:from-amber-500/[.06] dark:to-transparent dark:ring-amber-500/15">
                                <p class="text-[11px] text-zinc-500">{{ cur && isWaiting(cur) ? (cur.decision === 'pending' ? 'Days in current review' : 'Days since submission') : 'Status' }}</p>
                                <UiCount v-if="cur && isWaiting(cur)" :value="cur.days ?? 0" class="text-2xl font-semibold" />
                                <p v-else class="mt-1 text-sm font-semibold">{{ cur ? DECISIONS[cur.decision] : STAGES[p.stage] }}</p>
                            </div>
                        </div>
                    </section>

                    <div>
                        <div class="mb-4 flex flex-wrap items-end justify-between gap-3 select-none">
                            <div><h2 class="font-semibold">Submission history</h2><p class="text-xs text-zinc-500">One paper, every journal or conference it has been sent to, current one first.</p></div>
                            <FluxButton size="sm" :variant="cur && isRejection(cur) ? 'outline' : 'primary'" icon="plus" @click="newSubmission">{{ subs.length ? 'Add another submission' : 'Record submission' }}</FluxButton>
                        </div>
                        <div v-if="cur && isRejection(cur)" class="mb-5 flex flex-wrap items-center gap-3 rounded-xl border border-dashed border-indigo-300 bg-indigo-50/50 p-4 dark:border-indigo-500/30 dark:bg-indigo-500/5">
                            <FluxIcon name="arrow-path-rounded-square" class="size-6 text-indigo-500" />
                            <div class="min-w-0 flex-1"><p class="text-sm font-semibold">Sent it somewhere else?</p><p class="text-xs text-zinc-500">Add the new journal. {{ cur.venue_name }} stays in the history.</p></div>
                            <FluxButton size="sm" variant="primary" @click="newSubmission">Add new journal</FluxButton>
                        </div>

                        <template v-for="entry in history" :key="entry.key">
                            <UiPreprintEntry v-if="entry.type === 'preprint'" :paper="p" @edit="editPaper" />
                            <div v-else class="relative pb-5 pl-7 sm:pb-6 sm:pl-8">
                                <span v-if="!entry.last" class="absolute top-6 bottom-0 left-[11px] w-px bg-zinc-200 dark:bg-zinc-800"></span>
                                <span :class="['absolute top-1.5 left-0 flex size-6 items-center justify-center rounded-full ring-4 ring-[#f7f8fc] dark:ring-zinc-950 text-white', RING_TONE[tone(entry.sub)].split(' ')[0], entry.isCur ? '' : 'opacity-60']">
                                    <FluxIcon :name="ICON_OF[tone(entry.sub)]" variant="micro" class="size-3.5" />
                                </span>
                                <div :class="['panel p-4', entry.isCur ? 'ring-1 ring-indigo-200 dark:ring-indigo-500/30' : '']" @contextmenu.prevent="subMenu(entry.sub, entry.isCur)">
                                    <div class="flex flex-wrap items-start justify-between gap-3">
                                        <div class="min-w-0">
                                            <div class="flex flex-wrap items-center gap-2"><h3 class="font-semibold">{{ entry.sub.venue_name }}</h3><UiDecision :s="entry.sub" /><span v-if="entry.isCur" class="rounded-md px-1.5 py-px text-[10px] font-semibold tracking-wide text-indigo-700 uppercase ring-1 ring-indigo-200 ring-inset dark:text-indigo-300 dark:ring-indigo-500/30">Current</span></div>
                                            <p class="mt-0.5 text-sm text-zinc-500">
                                                {{ entry.sub.manuscript_id || 'No manuscript ID' }}<template v-if="!hasTimeline(entry.sub)"> · Submitted {{ entry.sub.submitted_on ? format(entry.sub.submitted_on, 'j M Y') : 'date not set' }}<template v-if="entry.sub.status_on"> · {{ DECISIONS[entry.sub.decision] }} since {{ format(entry.sub.status_on, 'j M Y') }}</template><template v-if="entry.sub.decision_on && !isWaiting(entry.sub)"> → {{ format(entry.sub.decision_on, 'j M Y') }}</template></template><template v-if="entry.sub.days !== null"> · <span class="num">{{ entry.sub.days }}</span> days</template>
                                            </p>
                                        </div>
                                        <div class="flex flex-wrap items-center gap-1">
                                            <FluxButton v-if="!entry.isCur" size="sm" variant="ghost" icon="star" title="Make this the paper’s current journal" @click="makeCurrent(entry.sub)">Set as current</FluxButton>
                                            <FluxButton v-if="entry.sub.portal_url" size="sm" :variant="entry.isCur ? 'filled' : 'outline'" icon-trailing="arrow-up-right" @click="openExternal(entry.sub.portal_url)">Open portal</FluxButton>
                                            <UiMenu>
                                                <template #trigger><FluxButton size="sm">Decision</FluxButton></template>
                                                <UiMenuItem v-for="(l, k) in DECISIONS" :key="k" @click="setDecision(entry.sub, k)">{{ l }}</UiMenuItem>
                                                <template v-if="!entry.isCur"><div class="my-1 border-t border-zinc-100 dark:border-zinc-800"></div><UiMenuItem @click="makeCurrent(entry.sub)">Make current journal</UiMenuItem></template>
                                            </UiMenu>
                                            <FluxButton size="sm" variant="ghost" icon="pencil-square" aria-label="Edit submission" @click="editSubmission(entry.sub)" />
                                        </div>
                                    </div>
                                    <ol v-if="hasTimeline(entry.sub)" class="mt-3 flex flex-wrap items-start gap-y-2.5 select-none" aria-label="Status history">
                                        <li v-for="(st, k) in entry.sub.timeline" :key="'st' + st.id" class="group/step flex items-start">
                                            <div class="flex flex-col">
                                                <div class="relative flex items-center gap-1.5">
                                                    <span :class="['shrink-0 rounded-full', k === entry.sub.timeline.length - 1 ? 'size-2.5 ring-[3px] ' + RING_TONE[tone(st.status)] : 'size-2 ' + DOT[tone(st.status)]]"></span>
                                                    <span :class="['text-xs whitespace-nowrap', k === entry.sub.timeline.length - 1 ? 'font-semibold text-zinc-900 dark:text-white' : 'text-zinc-600 dark:text-zinc-300']">{{ stepLabel(st, k) }}</span>
                                                    <button v-if="k > 0 && k < entry.sub.timeline.length - 1" type="button" class="absolute top-0.5 -right-4 rounded text-zinc-300 opacity-0 transition group-hover/step:opacity-100 hover:text-rose-500 focus-visible:opacity-100 dark:text-zinc-600" :aria-label="`Remove ${stepLabel(st, k)} from the history`" title="Remove this step" @click="removeStep(st)"><FluxIcon name="x-mark" variant="micro" class="size-3" /></button>
                                                </div>
                                                <span class="num mt-0.5 pl-3.5 text-[11px] text-zinc-400">{{ st.on_date ? stepDate(st.on_date) : 'date not set' }}</span>
                                            </div>
                                            <div v-if="k < entry.sub.timeline.length - 1" class="mx-1.5 flex w-8 shrink-0 flex-col items-center" :title="stepGap(entry.sub.timeline, k) !== null ? `${stepGap(entry.sub.timeline, k)} ${plural('day', stepGap(entry.sub.timeline, k))} between these steps` : null">
                                                <span class="mt-[7px] h-px w-full bg-zinc-200 dark:bg-zinc-700"></span>
                                                <span v-if="stepGap(entry.sub.timeline, k) !== null" class="num mt-[7px] text-[10px] leading-none text-zinc-400">{{ stepGap(entry.sub.timeline, k) }}d</span>
                                            </div>
                                        </li>
                                    </ol>
                                    <div class="mt-2.5 flex flex-wrap items-center gap-1.5 select-none">
                                        <button v-for="f in filesOf(entry.sub)" :key="'sf' + f.id" type="button" class="inline-flex max-w-full items-center gap-1.5 rounded-md bg-zinc-100 px-2 py-1 text-xs text-zinc-700 transition hover:bg-indigo-50 hover:text-indigo-700 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-indigo-500/10 dark:hover:text-indigo-300" :title="`${f.original_name} — preview`" @click="preview(f)" @contextmenu.prevent.stop="fileMenu(f)">
                                            <FluxIcon name="paper-clip" variant="micro" class="size-3.5 shrink-0" /><span class="truncate">{{ f.label || f.original_name }}</span><span class="shrink-0 text-zinc-400">{{ FILE_KINDS[f.kind] ?? f.kind_label }}</span>
                                        </button>
                                        <button type="button" class="inline-flex items-center gap-1 rounded-md px-1.5 py-1 text-xs text-zinc-400 transition hover:text-indigo-600" title="Attach a file to this submission" @click="openUpload([], entry.sub.id)"><FluxIcon name="plus" variant="micro" class="size-3" />Attach file</button>
                                    </div>
                                    <div v-if="entry.isCur && isWaiting(entry.sub) && entry.sub.days !== null" class="mt-4 flex items-center gap-4 rounded-xl bg-zinc-50 p-3 dark:bg-zinc-800/40">
                                        <UiRing :pct="entry.sub.clock?.pct ?? 0" :size="54" :tone="ringTone(entry.sub.clock)" :label="String(entry.sub.days)" sub="days" />
                                        <div class="min-w-0 flex-1 text-sm">
                                            <p class="font-medium">{{ entry.sub.days }} days in review</p>
                                            <p :class="['text-xs', entry.sub.clock?.overdue ? 'text-rose-500' : 'text-zinc-500']">
                                                <template v-if="entry.sub.clock">{{ entry.sub.clock.overdue ? (entry.sub.clock.days - entry.sub.clock.typical) + ' days past the usual ' + entry.sub.clock.typical + '. A status query is fair.' : 'Usually ' + entry.sub.clock.typical + ' days, so a decision around ' + format(entry.sub.clock.expected_on, 'j M') + '.' }}</template>
                                                <template v-else>Add the journal’s or conference’s usual decision time to start the clock.</template>
                                            </p>
                                        </div>
                                    </div>
                                    <div v-else-if="entry.sub.clock" class="mt-3"><div class="mb-1 flex justify-between text-[11px] text-zinc-500"><span>Review clock</span><span class="num">{{ entry.sub.clock.days }} / {{ entry.sub.clock.typical }} days</span></div>
                                        <div class="clock-track"><div :class="['clock-fill grow-x', entry.sub.clock.overdue ? 'bg-rose-500' : 'bg-gradient-to-r from-indigo-500 to-violet-500']" :style="`width: ${entry.sub.clock.pct}%`"></div></div></div>
                                    <p v-if="entry.sub.revision_due_on && ['major', 'minor'].includes(entry.sub.decision)" class="mt-3 text-sm text-amber-600">Revision due {{ format(entry.sub.revision_due_on, 'j M Y') }} ({{ diffForHumans(entry.sub.revision_due_on) }})</p>
                                    <div v-if="entry.sub.note" class="mt-3">
                                        <button type="button" class="flex items-center gap-1 text-xs text-zinc-500 hover:text-indigo-600" @click="notesOpen[entry.sub.id] = !notesOpen[entry.sub.id]">Notes <FluxIcon name="chevron-down" variant="micro" :class="['size-4 transition', notesOpen[entry.sub.id] && 'rotate-180']" /></button>
                                        <p v-show="notesOpen[entry.sub.id]" class="pt-2 text-sm whitespace-pre-line text-zinc-600 dark:text-zinc-300">{{ entry.sub.note }}</p>
                                    </div>

                                    <div v-if="['major', 'minor'].includes(entry.sub.decision) || entry.sub.revision_items.length" class="mt-4 rounded-lg bg-zinc-50 p-3 dark:bg-zinc-800/40">
                                        <div class="flex items-center justify-between">
                                            <button type="button" class="flex items-center gap-1 text-xs font-semibold" @click="openItems[entry.sub.id] = !isItemsOpen(entry.sub)">Reviewer comments <span class="num font-normal text-zinc-500">{{ doneN(entry.sub) }}/{{ entry.sub.revision_items.length }} answered</span><FluxIcon name="chevron-down" variant="micro" :class="['size-4 transition', isItemsOpen(entry.sub) && 'rotate-180']" /></button>
                                            <FluxButton size="xs" variant="ghost" icon="plus" @click="openChecklist(entry.sub)">Paste comments</FluxButton>
                                        </div>
                                        <div v-if="entry.sub.revision_items.length" class="clock-track mt-2 !h-1"><div class="clock-fill bg-emerald-500" :style="`width: ${Math.round(doneN(entry.sub) / entry.sub.revision_items.length * 100)}%`"></div></div>
                                        <div v-show="isItemsOpen(entry.sub)" class="mt-3 space-y-2" data-sort-list>
                                            <div v-for="(it, idx) in entry.sub.revision_items" :key="'ri' + it.id" data-sort-item class="group rounded-md bg-white p-2.5 ring-1 ring-zinc-200 dark:bg-zinc-900 dark:ring-zinc-700">
                                                <div class="flex items-start gap-2.5">
                                                    <button type="button" class="-ml-1 mt-0.5 cursor-grab touch-none text-zinc-300 opacity-0 group-hover:opacity-100 hover:text-zinc-500 active:cursor-grabbing" title="Drag to reorder" @pointerdown="drag($event, 'items', entry.sub.id, entry.sub.revision_items, idx)"><FluxIcon name="bars-2" variant="micro" /></button>
                                                    <button type="button" :class="['mt-0.5 flex size-4 shrink-0 items-center justify-center rounded border transition', it.done ? 'border-emerald-500 bg-emerald-500 text-white' : 'border-zinc-300 dark:border-zinc-600']" :aria-label="it.done ? 'Mark as not answered' : 'Mark as answered'" @click="toggleItem(it)"><FluxIcon v-if="it.done" name="check" variant="micro" class="size-3" /></button>
                                                    <p :class="['flex-1 text-sm whitespace-pre-line', it.done ? 'text-zinc-400 line-through' : '']">{{ it.comment }}</p>
                                                    <button type="button" class="text-xs text-zinc-400 hover:text-indigo-600" @click="respond[it.id] = !respond[it.id]">{{ it.response ? 'Response' : 'Respond' }}</button>
                                                    <button type="button" class="text-zinc-300 opacity-0 group-hover:opacity-100 hover:text-rose-500" aria-label="Delete comment" @click="deleteItem(it)"><FluxIcon name="x-mark" variant="micro" /></button>
                                                </div>
                                                <div v-show="respond[it.id]" class="mt-2 pl-6">
                                                    <textarea rows="3" class="w-full rounded-md border-zinc-200 bg-zinc-50 p-2 text-sm dark:border-zinc-700 dark:bg-zinc-800" placeholder="How you addressed it" :value="it.response ?? ''" @blur="saveResponse(it, $event.target.value)"></textarea>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </template>
                        <div v-if="!subs.length" class="panel"><UiEmpty title="Not submitted to a journal yet" text="Record a submission when you send this paper to a journal or conference. The review clock starts from that date.">
                            <FluxButton variant="primary" @click="newSubmission">Record submission</FluxButton></UiEmpty></div>
                    </div>

                    <div class="stagger space-y-5">
                        <div class="panel p-5">
                            <div class="mb-3 flex items-center justify-between select-none">
                                <p class="eyebrow">Authors</p>
                                <FluxButton size="xs" variant="ghost" icon="plus" @click="editAuthor()">Add author</FluxButton>
                            </div>
                            <div class="space-y-1" data-sort-list>
                                <div v-for="(au, idx) in authors" :key="'a' + au.id" data-sort-item class="group -mx-2 flex items-start gap-3 rounded-lg bg-white/0 px-2 py-2 hover:bg-zinc-50 data-[dragging]:bg-white data-[dragging]:shadow-lg dark:hover:bg-zinc-800/50 dark:data-[dragging]:bg-zinc-800" @dblclick="editAuthor(au)">
                                    <span class="flex size-9 shrink-0 cursor-grab touch-none items-center justify-center rounded-full bg-gradient-to-br from-indigo-500 to-fuchsia-500 text-xs font-semibold text-white select-none active:cursor-grabbing" title="Drag to reorder" @pointerdown="drag($event, 'authors', id, detail.authors, idx)">{{ initials(au.name) }}</span>
                                    <div class="min-w-0 flex-1">
                                        <p class="text-sm font-medium">{{ au.name }} <span v-if="au.corresponding" class="ml-1 rounded bg-indigo-50 px-1.5 py-0.5 text-[10px] font-semibold text-indigo-700 dark:bg-indigo-500/10 dark:text-indigo-300">Corresponding</span></p>
                                        <p class="text-xs text-zinc-500">{{ authorMeta(au) }}</p>
                                        <div v-if="au.roles?.length" class="mt-1.5 flex flex-wrap gap-1"><span v-for="r in au.roles" :key="r" class="rounded-full bg-zinc-100 px-2 py-0.5 text-[11px] dark:bg-zinc-800">{{ r }}</span></div>
                                    </div>
                                    <div class="flex opacity-0 transition group-focus-within:opacity-100 group-hover:opacity-100">
                                        <FluxButton size="xs" variant="ghost" icon="chevron-up" aria-label="Move up" @click="reorder('authors', id, detail.authors, idx, idx - 1)" />
                                        <FluxButton size="xs" variant="ghost" icon="chevron-down" aria-label="Move down" @click="reorder('authors', id, detail.authors, idx, idx + 1)" />
                                        <FluxButton size="xs" variant="ghost" icon="pencil-square" aria-label="Edit author" @click="editAuthor(au)" />
                                    </div>
                                </div>
                                <button v-if="!authors.length" type="button" class="text-sm text-zinc-400 hover:text-indigo-600" @click="editAuthor()">Add authors with their CRediT roles…</button>
                            </div>
                            <div v-if="detail.credit" class="mt-4 rounded-lg bg-zinc-50 p-3 dark:bg-zinc-800/40">
                                <div class="mb-1 flex items-center justify-between select-none"><span class="text-[11px] font-semibold text-zinc-500">CRediT statement</span>
                                    <button type="button" class="text-xs text-zinc-500 hover:text-indigo-600" @click="copyCredit">{{ copied ? 'Copied' : 'Copy' }}</button></div>
                                <p class="text-sm text-zinc-700 dark:text-zinc-300">{{ detail.credit }}</p>
                            </div>
                        </div>

                        <div class="panel p-5">
                            <template v-if="p.abstract">
                                <button type="button" class="group flex w-full items-center justify-between text-left" :aria-expanded="abstractOpen ? 'true' : 'false'" @click="abstractOpen = !abstractOpen">
                                    <span class="eyebrow">Abstract <span class="num ml-1 font-normal normal-case tracking-normal">{{ words }} words</span></span>
                                    <span class="flex items-center gap-1 text-xs text-zinc-500 group-hover:text-indigo-600">
                                        <span>{{ abstractOpen ? 'Collapse' : 'Expand' }}</span>
                                        <FluxIcon name="chevron-down" variant="micro" :class="['size-4 transition-transform duration-200', abstractOpen && 'rotate-180']" />
                                    </span>
                                </button>
                                <div v-show="abstractOpen">
                                    <p class="mt-3 text-sm leading-relaxed whitespace-pre-line text-zinc-700 dark:text-zinc-300">{{ p.abstract }}</p>
                                </div>
                            </template>
                            <template v-else>
                                <p class="eyebrow mb-2">Abstract</p>
                                <button type="button" class="text-sm text-zinc-400 hover:text-indigo-600" @click="editPaper">Add an abstract…</button>
                            </template>
                            <div v-if="keywords.length" class="mt-4 flex flex-wrap gap-1.5">
                                <span v-for="k in keywords" :key="k" class="rounded-full bg-zinc-100 px-2 py-0.5 text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">{{ k }}</span>
                            </div>
                        </div>

                        <div v-if="p.notes" class="panel p-5"><p class="eyebrow mb-2">Notes</p><p class="text-sm whitespace-pre-line text-zinc-700 dark:text-zinc-300">{{ p.notes }}</p></div>
                    </div>
                </div>

                <aside class="min-w-0 space-y-4">
                    <dl class="panel space-y-4 p-4 text-sm">
                        <div v-for="(v, k) in { Method: p.method, Data: p.dataset }" :key="k"><dt class="text-xs text-zinc-400 select-none">{{ k }}</dt><dd class="mt-0.5 leading-snug text-zinc-700 dark:text-zinc-200">{{ v || '—' }}</dd></div>
                        <div><dt class="text-xs text-zinc-400 select-none">Preprint</dt>
                            <dd class="mt-0.5 text-zinc-700 dark:text-zinc-200"><a v-if="p.preprint_url" :href="p.preprint_url" target="_blank" class="text-indigo-600 hover:underline">{{ p.preprint_server || 'Open' }}</a><template v-else-if="p.preprint_server">{{ p.preprint_server }}</template><template v-else> — </template>
                                <span v-if="p.preprint_on" class="text-zinc-400"> · {{ format(p.preprint_on, 'j M Y') }}</span></dd></div>
                        <div v-if="p.overleaf_url || p.drive_url"><dt class="text-xs text-zinc-400 select-none">Links</dt><dd class="mt-0.5 flex gap-3">
                            <a v-if="p.overleaf_url" :href="p.overleaf_url" target="_blank" class="text-indigo-600 hover:underline">Overleaf</a>
                            <a v-if="p.drive_url" :href="p.drive_url" target="_blank" class="text-indigo-600 hover:underline">Drive</a></dd></div>
                    </dl>

                    <button v-if="proof" type="button" class="panel lift flex w-full items-center gap-3 p-4 text-left" @click="preview(proof)">
                        <span class="flex size-10 items-center justify-center rounded-lg bg-gradient-to-br from-rose-500 to-orange-400 text-white"><FluxIcon name="document-check" variant="mini" /></span>
                        <span class="min-w-0"><span class="block text-sm font-semibold">Proof PDF</span><span class="block truncate text-xs text-zinc-500">{{ proof.original_name }}</span></span>
                    </button>

                    <div class="panel p-4">
                        <h3 class="font-semibold select-none">Up next</h3>
                        <p class="mt-0.5 text-xs text-zinc-500">If this round ends in rejection, the first journal or conference becomes the next submission in one click.</p>
                        <FluxButton v-if="(detail.needs_next_venue || p.stage === 'ready') && queue.length" class="mt-3 w-full" variant="primary" icon-trailing="arrow-right" @click="submitNext">Submit to {{ limit(queue[0].venue_name, 28) }}</FluxButton>
                        <div class="mt-3 space-y-1.5" data-sort-list>
                            <div v-for="(q, i) in queue" :key="'q' + q.id" data-sort-item class="group flex items-center gap-2 rounded-lg px-1 py-1.5 text-sm hover:bg-zinc-50 data-[dragging]:bg-white data-[dragging]:shadow-lg dark:hover:bg-zinc-800/50 dark:data-[dragging]:bg-zinc-800">
                                <span class="num flex size-5 shrink-0 cursor-grab touch-none items-center justify-center rounded-full bg-indigo-50 text-[10px] font-semibold text-indigo-600 select-none active:cursor-grabbing dark:bg-indigo-500/10" title="Drag to reorder" @pointerdown="drag($event, 'queue', id, detail.queue, i)">{{ i + 1 }}</span>
                                <span class="min-w-0 flex-1 truncate">{{ q.venue_name }}</span>
                                <span class="flex opacity-0 transition group-focus-within:opacity-100 group-hover:opacity-100">
                                    <button type="button" class="p-0.5 hover:text-indigo-600" aria-label="Move up" @click="reorder('queue', id, detail.queue, i, i - 1)"><FluxIcon name="chevron-up" variant="micro" /></button>
                                    <button type="button" class="p-0.5 hover:text-indigo-600" aria-label="Move down" @click="reorder('queue', id, detail.queue, i, i + 1)"><FluxIcon name="chevron-down" variant="micro" /></button>
                                    <button type="button" class="p-0.5 hover:text-rose-500" aria-label="Remove from queue" @click="removeQueue(q)"><FluxIcon name="x-mark" variant="micro" /></button>
                                </span>
                            </div>
                        </div>
                        <div class="mt-3 flex gap-2">
                            <FluxSelect v-model="queueVenue" size="sm" placeholder="Add a journal or conference…">
                                <FluxSelectOption v-for="v in venues" :key="v.id" :value="v.id">{{ v.name }}</FluxSelectOption>
                            </FluxSelect>
                            <FluxButton size="sm" icon="plus" aria-label="Add to queue" @click="addToQueue" />
                        </div>
                    </div>

                    <div v-if="deadlines.length" class="panel p-4">
                        <div class="flex items-center justify-between select-none">
                            <h3 class="font-semibold">Deadlines</h3>
                            <a :href="route('deadlines')" class="text-xs text-zinc-500 hover:text-indigo-600">All deadlines</a>
                        </div>
                        <ul class="mt-3 space-y-2">
                            <li v-for="d in deadlines" :key="'d' + d.id" class="flex items-start gap-2 text-sm">
                                <FluxIcon name="calendar-days" variant="micro" :class="['mt-0.5 size-4', d.done ? 'text-zinc-300' : d.days_left < 0 ? 'text-rose-500' : d.days_left <= 7 ? 'text-amber-500' : 'text-indigo-500']" />
                                <div class="min-w-0 flex-1">
                                    <p :class="['leading-snug', d.done ? 'text-zinc-400 line-through' : '']">{{ d.title }}</p>
                                    <p class="text-xs text-zinc-500">{{ format(d.due_on, 'j M Y') }}<template v-if="!d.done"> · {{ d.days_left === 0 ? 'today' : diffForHumans(d.due_on) }}</template></p>
                                </div>
                            </li>
                        </ul>
                    </div>
                </aside>
            </div>

            <!-- ================= FILES ================= -->
            <template v-if="tab === 'files'">
                <div class="mb-4 flex items-center justify-between select-none">
                    <div><h2 class="font-semibold">Files</h2><p class="text-xs text-zinc-500">All optional. Upload manuscripts, cover letters, responses or the publisher’s proof PDF. You can also drop files here.</p></div>
                    <FluxButton size="sm" variant="primary" icon="arrow-up-tray" @click="openUpload()">Upload</FluxButton>
                </div>
                <div v-if="!files.length" class="panel"><UiEmpty title="No files" text="Files are optional. Keep them here if you want the full manuscript and the proof next to the record."><FluxButton @click="openUpload()">Upload a file</FluxButton></UiEmpty></div>
                <div v-else class="stagger grid gap-3 @xl:grid-cols-2 @4xl:grid-cols-3">
                    <div v-for="f in files" :key="'f' + f.id" class="panel lift group p-4" @contextmenu.prevent="fileMenu(f)" @dblclick="preview(f)">
                        <div class="flex items-start gap-3">
                            <span :class="['flex size-10 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br text-[10px] font-bold text-white uppercase select-none', FILE_GRADIENT[f.kind] ?? 'from-zinc-400 to-zinc-500']">{{ fileExt(f) }}</span>
                            <div class="min-w-0 flex-1">
                                <p class="truncate text-sm font-medium" :title="f.original_name">{{ f.label || f.original_name }}</p>
                                <p class="text-xs text-zinc-500">{{ FILE_KINDS[f.kind] ?? f.kind_label }} · {{ f.human_size }} · {{ format(utc(f.created_at), 'j M Y') }}</p>
                                <p v-if="f.submission_name" class="mt-0.5 truncate text-xs text-indigo-600 dark:text-indigo-400">For {{ f.submission_name }}</p>
                            </div>
                        </div>
                        <div class="mt-3 flex flex-wrap items-center gap-1">
                            <FluxButton size="xs" icon="eye" @click="preview(f)">Preview</FluxButton>
                            <FluxButton size="xs" variant="ghost" icon="arrow-top-right-on-square" @click="fileAction('files.open', f)">Open</FluxButton>
                            <FluxButton size="xs" variant="ghost" icon="arrow-down-tray" aria-label="Save a copy" title="Save a copy…" @click="fileForms?.saveCopy(f)" />
                            <FluxButton size="xs" variant="ghost" icon="pencil-square" class="ml-auto" aria-label="Edit file details" title="Edit details" @click="editFile(f)" />
                            <FluxButton size="xs" variant="ghost" icon="ellipsis-horizontal" aria-label="More file actions" title="More" @click="fileMenu(f)" />
                            <FluxButton size="xs" variant="ghost" icon="trash" class="opacity-60 group-hover:opacity-100" aria-label="Delete file" title="Delete" @click="deleteFile(f)" />
                        </div>
                    </div>
                </div>
            </template>
        </div>

        <!-- ================= MODALS ================= -->
        <PaperForm ref="paperForm" :paper-id="id" @saved="onSaved" />
        <SubmissionForm ref="subForm" :paper-id="id" :venues="venues" @saved="onSaved" />
        <AuthorForm ref="authorForm" :paper-id="id" @saved="onSaved" />
        <FileForms ref="fileForms" :paper-id="id" :submissions="subs" @saved="onFileSaved" />

        <FluxModal name="comments" class="w-full max-w-xl">
            <form class="space-y-4" @submit.prevent="importComments">
                <FluxHeading size="lg">Paste reviewer comments</FluxHeading>
                <FluxText>Each paragraph or numbered point becomes one item on the checklist.</FluxText>
                <FluxTextarea v-model="comments" rows="10" placeholder="1. The sampling strategy needs more detail…&#10;2. Please discuss…" />
                <div class="flex justify-end gap-2"><FluxModalClose><FluxButton variant="ghost">Cancel</FluxButton></FluxModalClose><FluxButton type="submit" variant="primary">Add to checklist</FluxButton></div>
            </form>
        </FluxModal>
    </div>
</template>
