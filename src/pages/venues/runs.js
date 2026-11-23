import { computed, onBeforeUnmount, reactive } from 'vue';
import { api } from '@/lib/api.js';
import { on } from '@/lib/events.js';

/**
 * Live research runs of some kinds ('venues', 'citations', 'scholar', 'scan'), fed by the `ai-run` event.
 *
 *   const runs = useRuns(['venues'], { onPartial(partial, run) {}, onFinish(run) {} });
 *   const { run } = await api('venues.research', { id }); runs.track(run, { venue_id: id });
 *
 * Each run: { run_id, kind, label, status, step, progress, message, done, total, results: [partial…], result, …extra }.
 * Runs the page did not start (the scheduler, another page) are picked up too, with `extra` empty.
 * Finished runs are removed unless they are `keep` runs (batches), which stay until `dismiss(run_id)`.
 */
let partialSeq = 0;

export function useRuns(kinds, hooks = {}) {
  const runs = reactive({});
  const finished = new Set();

  function track(id, extra = {}) {
    if (!id || finished.has(id)) return runs[id] ?? null;
    const r = runs[id];
    if (r) Object.assign(r, extra);
    else runs[id] = { run_id: id, kind: kinds[0], label: null, status: 'running', step: null, progress: 0, message: null, done: 0, total: 0, results: [], result: null, ...extra };
    return runs[id];
  }

  function onEvent(p) {
    if (!p || !kinds.includes(p.kind)) return;
    const r = track(p.run_id, { kind: p.kind });
    if (!r || r.status !== 'running') return;
    r.step = p.step ?? r.step;
    r.message = p.message ?? r.message;
    r.progress = typeof p.progress === 'number' ? p.progress : r.progress;
    r.done = p.done ?? r.done;
    r.total = p.total ?? r.total;
    if (p.status === 'running') {
      if (p.partial) {
        p.partial.__k = ++partialSeq; // stable key for the animated "recent results" list
        r.results.unshift(p.partial);
        hooks.onPartial?.(p.partial, r);
      }
      return;
    }
    r.status = p.status;
    finished.add(p.run_id);
    r.result = p.partial ?? {};
    hooks.onFinish?.(r);
    if (!r.keep) delete runs[p.run_id];
  }

  /** Pick up runs still going (e.g. after switching pages). */
  async function restore() {
    try {
      const rows = await api('ai.running');
      for (const row of rows ?? []) {
        if (!kinds.includes(row.kind)) continue;
        track(row.id, { kind: row.kind, label: row.label, step: row.step, done: row.done, total: row.total, progress: row.percent ?? 0, restored: true, keep: row.total > 1 });
      }
    } catch {}
  }

  async function cancel(id) {
    try { await api('ai.cancel', { id }); } catch {}
  }

  function dismiss(id) { delete runs[id]; }

  const list = computed(() => Object.values(runs).sort((a, b) => b.run_id - a.run_id));
  const active = computed(() => list.value.filter((r) => r.status === 'running'));

  const off = on('ai-run', onEvent);
  onBeforeUnmount(off);
  restore();

  return { runs, list, active, track, cancel, dismiss, restore };
}

/** Kind of the "Update from Scholar" run (its own kind, so a run picked up later still says what it does). */
export const SCHOLAR_KIND = 'scholar';

/** What a running job is doing, by kind: "Updating from Google Scholar…". */
export function runText(run) {
  switch (run?.kind) {
    case SCHOLAR_KIND: return 'Updating from Google Scholar…';
    case 'citations': return 'Checking citations…';
    case 'venues': return 'Updating journals and conferences…';
    case 'scan': return 'Finding calls for papers…';
    default: return run?.label ? run.label + '…' : 'Working…';
  }
}

/** "3 changes to review" / "1 change to review". */
export function reviewText(n) {
  return `${n} ${n === 1 ? 'change' : 'changes'} to review`;
}
