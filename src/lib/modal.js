import { reactive } from 'vue';

/** Open state of every named <FluxModal name="...">. */
export const modalState = reactive({});

/** modal('venue-form').show() / .close() — the $flux.modal() / Flux::modal() equivalent. */
export function modal(name) {
  return {
    show() { modalState[name] = true; },
    close() { modalState[name] = false; },
    toggle() { modalState[name] = !modalState[name]; },
    get open() { return !!modalState[name]; },
  };
}

/** modals().close() closes every open modal. */
export function modals() {
  return {
    close() { for (const k of Object.keys(modalState)) modalState[k] = false; },
  };
}

export default modal;
