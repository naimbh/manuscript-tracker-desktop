import { computed, inject, provide, unref } from 'vue';
import { firstError } from '@/lib/api.js';

const KEY = Symbol('fluxErrors');

/**
 * Make a page's validation errors ({ field: [msg] }, usually a ref) available to every Flux control
 * below it, so <FluxInput name="title" /> shows errors.title like <flux:error name="title"> does.
 */
export function provideErrors(errors) {
  provide(KEY, errors);
}

/** Error message for a control: its `error` prop (string or array) wins, else the provided bag by `name` (with name.* fallback). */
export function useFieldError(props) {
  const bag = inject(KEY, null);
  return computed(() => {
    const e = props.error;
    if (Array.isArray(e)) return e[0] ?? null;
    if (e) return e;
    const errors = props.errors ? unref(props.errors) : unref(bag);
    return props.name && errors ? firstError(errors, props.name) : null;
  });
}

/** Shared props for controls that show an error. */
export const errorProps = {
  name: { type: String, default: undefined },
  error: { type: [String, Array], default: undefined },
  errors: { type: Object, default: undefined },
};
