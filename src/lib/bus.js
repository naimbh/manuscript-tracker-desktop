/** Tiny in-renderer event emitter: bus.on('papers-changed', fn) / bus.emit('papers-changed', payload). */
const handlers = new Map();

export const bus = {
  on(name, fn) {
    if (!handlers.has(name)) handlers.set(name, new Set());
    handlers.get(name).add(fn);
    return () => bus.off(name, fn);
  },
  once(name, fn) {
    const off = bus.on(name, (...a) => { off(); fn(...a); });
    return off;
  },
  off(name, fn) {
    handlers.get(name)?.delete(fn);
  },
  emit(name, ...args) {
    for (const fn of [...(handlers.get(name) || [])]) fn(...args);
  },
};

export default bus;
