import { invoke } from '@tauri-apps/api/core';

/**
 * Error thrown by api() when the backend rejects a call.
 *   err.errors  -> { field: ['message', ...] }  (validation failures; {} otherwise)
 *   err.message -> first validation message, or the backend's message
 *   err.kind    -> 'validation' | 'error'
 */
export class ApiError extends Error {
  constructor(kind, message, errors = {}) {
    super(message);
    this.name = 'ApiError';
    this.kind = kind;
    this.errors = errors;
  }
}

function normalise(e) {
  if (e instanceof ApiError) return e;
  if (e && typeof e === 'object') {
    if (e.kind === 'validation' || e.errors) {
      const errors = {};
      for (const [k, v] of Object.entries(e.errors || {})) errors[k] = Array.isArray(v) ? v.map(String) : [String(v)];
      const first = Object.values(errors)[0]?.[0] ?? e.message ?? 'The given data was invalid.';
      return new ApiError('validation', first, errors);
    }
    return new ApiError('error', String(e.message ?? 'Something went wrong.'));
  }
  return new ApiError('error', typeof e === 'string' ? e : 'Something went wrong.');
}

/** Call a backend handler: api('papers.list', { q: 'x' }). Resolves with its result, rejects with ApiError. */
export async function api(name, args = {}) {
  try {
    return await invoke('api', { name, args: JSON.parse(JSON.stringify(args ?? {})) });
  } catch (e) {
    throw normalise(e);
  }
}

/** First message for a field from an errors object (like $errors->first('field')), with `field.*` fallback. */
export function firstError(errors, field) {
  if (!errors || !field) return null;
  if (errors[field]?.length) return errors[field][0];
  const nested = Object.keys(errors).find((k) => k.startsWith(field + '.'));
  return nested ? errors[nested][0] : null;
}

/**
 * Small helper for forms: const errors = ref({}); await submit(errors, () => api(...)).
 * Clears errors, runs fn, puts validation errors into errors.value and returns undefined on failure.
 * Non-validation errors are re-thrown.
 */
export async function submit(errorsRef, fn) {
  errorsRef.value = {};
  try {
    return await fn();
  } catch (e) {
    if (e instanceof ApiError && e.kind === 'validation') {
      errorsRef.value = e.errors;
      return undefined;
    }
    throw e;
  }
}

export default api;
