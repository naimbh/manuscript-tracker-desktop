/**
 * Position a fixed popover next to a reference element, like Flux's anchorable
 * (placement "bottom start" etc., gap 5px, flip when there is no room, shift to stay 5px inside the viewport).
 */
const PAD = 5;

export function anchor(el, reference, position = 'bottom start', gap = 5, offset = 0) {
  if (!el || !reference) return;
  let [side, align] = String(position || 'bottom start').split(/\s+/);
  side = { left: 'left', right: 'right', top: 'top', bottom: 'bottom' }[side] || 'bottom';
  align = align || (side === 'top' || side === 'bottom' ? 'start' : 'start');
  const rtl = document.dir === 'rtl';
  if (align === 'left') align = rtl ? 'end' : 'start';
  if (align === 'right') align = rtl ? 'start' : 'end';

  Object.assign(el.style, { position: 'fixed', inset: 'auto', margin: '0', top: '0px', left: '0px', maxHeight: '' });
  const r = reference.getBoundingClientRect();
  const w = el.offsetWidth;
  const h = el.offsetHeight;
  const vw = document.documentElement.clientWidth;
  const vh = document.documentElement.clientHeight;
  gap = Number(gap) || 0;
  offset = Number(offset) || 0;

  const place = (s) => {
    let top, left;
    if (s === 'bottom' || s === 'top') {
      top = s === 'bottom' ? r.bottom + gap : r.top - gap - h;
      if (align === 'center') left = r.left + r.width / 2 - w / 2;
      else if ((align === 'end') !== rtl) left = r.right - w - offset;
      else left = r.left + offset;
    } else {
      left = s === 'right' ? r.right + gap : r.left - gap - w;
      if (align === 'center') top = r.top + r.height / 2 - h / 2;
      else if (align === 'end') top = r.bottom - h - offset;
      else top = r.top + offset;
    }
    return { top, left };
  };
  const fits = ({ top, left }, s) =>
    s === 'bottom' ? top + h <= vh - PAD : s === 'top' ? top >= PAD : s === 'right' ? left + w <= vw - PAD : left >= PAD;
  const opposite = { top: 'bottom', bottom: 'top', left: 'right', right: 'left' };

  let p = place(side);
  if (!fits(p, side)) {
    const q = place(opposite[side]);
    if (fits(q, opposite[side])) p = q;
  }
  p.left = Math.min(Math.max(PAD, p.left), Math.max(PAD, vw - w - PAD));
  p.top = Math.min(Math.max(PAD, p.top), Math.max(PAD, vh - h - PAD));
  el.style.top = `${Math.round(p.top)}px`;
  el.style.left = `${Math.round(p.left)}px`;
}

/** Popover helpers that tolerate an already open/closed popover. */
export function showPopover(el) {
  try { if (!el.matches(':popover-open')) el.showPopover(); } catch {}
}
export function hidePopover(el) {
  try { if (el.matches(':popover-open')) el.hidePopover(); } catch {}
}
