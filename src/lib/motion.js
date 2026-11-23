import { onBeforeUnmount, onMounted } from 'vue';

/**
 * Entrance animations (.page-in, .stagger, .grow-x, .grow-y, .draw, .enter-up) only play inside an element that has
 * `data-entering`. The shell sets it on the route view for the first moments after a route opens; a panel that
 * mounts on its own (the paper detail) calls useEntrance(rootRef). Data refreshes later never replay them.
 */
export const ENTER_MS = 1600;

export function markEntering(el, ms = ENTER_MS) {
  if (!el) return () => {};
  el.setAttribute('data-entering', '');
  const t = setTimeout(() => el.removeAttribute('data-entering'), ms);
  return () => { clearTimeout(t); el.removeAttribute('data-entering'); };
}

export function useEntrance(elRef, ms = ENTER_MS) {
  let stop = null;
  onMounted(() => { stop = markEntering(elRef.value?.$el ?? elRef.value, ms); });
  onBeforeUnmount(() => stop?.());
}
