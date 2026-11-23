/**
 * Reordering with pointer events (HTML5 drag and drop is unavailable on Windows while the window accepts file drops).
 *
 *   <div ref="listEl"> <div v-for="(x, i) in items" data-sort-item> <button @pointerdown="drag($event, i)">grip</button> … </div> </div>
 *   const drag = (e, i) => startSort(e, listEl.value, i, (from, to) => save(move(ids, from, to)));
 *
 * The dragged row follows the pointer; the others slide out of the way; `onDrop(from, to)` runs when the order changed.
 */
export function startSort(e, container, from, onDrop) {
  if (!container || e.button !== 0) return;
  const items = [...container.querySelectorAll(':scope > [data-sort-item]')];
  const el = items[from];
  if (!el || items.length < 2) return;
  e.preventDefault();
  const handle = e.currentTarget;
  try { handle.setPointerCapture(e.pointerId); } catch { /* synthetic or already released */ }
  const rects = items.map((x) => x.getBoundingClientRect());
  const gap = rects.length > 1 ? Math.max(0, rects[1].top - rects[0].bottom) : 0;
  const shift = rects[from].height + gap;
  const startY = e.clientY;
  let to = from;
  let moved = false;

  const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  for (const [i, x] of items.entries()) if (i !== from && !reduce) x.style.transition = 'transform 150ms ease';

  function move(ev) {
    const dy = ev.clientY - startY;
    if (!moved && Math.abs(dy) < 4) return;
    if (!moved) {
      moved = true;
      el.style.position = 'relative';
      el.style.zIndex = '20';
      el.dataset.dragging = '';
      document.body.style.cursor = 'grabbing';
    }
    el.style.transform = `translateY(${dy}px)`;
    const center = rects[from].top + rects[from].height / 2 + dy;
    to = from;
    for (let i = from + 1; i < rects.length; i++) if (center > rects[i].top + rects[i].height / 2) to = i;
    for (let i = from - 1; i >= 0; i--) if (center < rects[i].top + rects[i].height / 2) to = i;
    for (const [i, x] of items.entries()) {
      if (i === from) continue;
      const y = from < to && i > from && i <= to ? -shift : to < from && i >= to && i < from ? shift : 0;
      x.style.transform = y ? `translateY(${y}px)` : '';
    }
  }

  function end() {
    handle.removeEventListener('pointermove', move);
    handle.removeEventListener('pointerup', end);
    handle.removeEventListener('pointercancel', end);
    document.body.style.cursor = '';
    for (const x of items) {
      x.style.transform = '';
      x.style.transition = '';
      x.style.position = '';
      x.style.zIndex = '';
      delete x.dataset.dragging;
    }
    if (moved) {
      // Swallow the click that follows a drag.
      window.addEventListener('click', (c) => { c.stopPropagation(); c.preventDefault(); }, { capture: true, once: true });
      setTimeout(() => window.dispatchEvent(new Event('click')), 0);
    }
    if (moved && to !== from) onDrop(from, to);
  }

  handle.addEventListener('pointermove', move);
  handle.addEventListener('pointerup', end);
  handle.addEventListener('pointercancel', end);
}

/** A copy of `list` with the item at `from` moved to `to`. */
export function moveItem(list, from, to) {
  const out = [...list];
  const [x] = out.splice(from, 1);
  out.splice(to, 0, x);
  return out;
}
