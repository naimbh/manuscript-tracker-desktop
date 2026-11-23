import { listen } from '@tauri-apps/api/event';

/**
 * Listen to a backend push event. Returns an unlisten function (safe to call before the listener is ready).
 *   const off = on('notifications', (payload) => ...); onUnmounted(off);
 */
export function on(channel, cb) {
  let stop = null;
  let cancelled = false;
  listen(channel, (event) => cb(event.payload))
    .then((un) => (cancelled ? un() : (stop = un)))
    .catch(() => {});
  return () => {
    cancelled = true;
    if (stop) stop();
  };
}

export default on;
