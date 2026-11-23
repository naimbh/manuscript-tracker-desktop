import { reactive } from 'vue';

/** The toast currently shown by <FluxToast>. */
export const toastState = reactive({ current: null, seq: 0 });

/**
 * toast({ variant: 'success' | 'warning' | 'danger', heading, text, duration }) — Flux::toast() equivalent.
 * toast('Saved.') and toast('Saved.', 'Heading') work too. duration in ms (default 5000, 0 = stays until closed).
 */
export function toast(...args) {
  let o = {};
  if (typeof args[0] === 'string') {
    o.text = args.shift();
    if (typeof args[0] === 'string') o.heading = args.shift();
    Object.assign(o, args[0] || {});
  } else {
    o = { ...(args[0] || {}) };
  }
  toastState.seq += 1;
  toastState.current = {
    id: toastState.seq,
    variant: o.variant || '',
    heading: o.heading || '',
    text: o.text || '',
    duration: o.duration === undefined ? 5000 : Number(o.duration),
  };
}

export function dismissToast() {
  toastState.current = null;
}

export default toast;
