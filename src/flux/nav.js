import { computed } from 'vue';
import { currentRoute } from '@/lib/router.js';

/** Flux's automatic "current" detection: the link's path equals the current route path. */
export function useCurrent(props) {
  return computed(() => {
    if (props.current !== undefined && props.current !== null) return !!props.current;
    if (!props.href || props.as === 'div') return false;
    let h = props.href;
    if (!h.startsWith('#/')) return false;
    h = h.slice(1).split('?')[0].replace(/\/+$/, '') || '/';
    return h === (currentRoute.path.replace(/\/+$/, '') || '/');
  });
}
