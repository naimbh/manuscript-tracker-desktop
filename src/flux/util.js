import { Comment, Fragment, Text } from 'vue';

/** Join class parts like Flux::classes(): strings, arrays and falsy values. */
export function cx(...parts) {
  const out = [];
  const walk = (p) => {
    if (!p) return;
    if (Array.isArray(p)) p.forEach(walk);
    else out.push(String(p).trim());
  };
  parts.forEach(walk);
  return out.filter(Boolean).join(' ');
}

/** Present-attribute helper: '' when truthy, undefined (attribute removed) otherwise. */
export const flag = (v) => (v ? '' : undefined);

/** True when a slot renders something other than comments / whitespace ($slot->isNotEmpty()). */
export function hasContent(slot, props) {
  if (!slot) return false;
  const check = (nodes) =>
    nodes.some((n) => {
      if (n.type === Comment) return false;
      if (n.type === Text) return String(n.children ?? '').trim() !== '';
      if (n.type === Fragment) return check(n.children || []);
      return true;
    });
  try {
    return check(slot(props) || []);
  } catch {
    return true;
  }
}

/** Flux::applyInset(): inset may be true or a space-separated subset of 'top right bottom left'. */
export function applyInset(inset, { top, right, bottom, left }) {
  if (!inset) return '';
  const sides = inset === true || inset === '' ? ['top', 'right', 'bottom', 'left'] : String(inset).split(/\s+/);
  return cx(sides.includes('top') && top, sides.includes('right') && right, sides.includes('bottom') && bottom, sides.includes('left') && left);
}

/** Split attrs into the class part and the rest (for components that put class and other attrs on different elements). */
export function splitAttrs(attrs, keys = ['class']) {
  const picked = {};
  const rest = {};
  for (const [k, v] of Object.entries(attrs)) (keys.includes(k) ? picked : rest)[k] = v;
  return [picked, rest];
}

/** Read an attribute that may be written in kebab or camel case. */
export function attr(attrs, name) {
  if (name in attrs) return attrs[name];
  const camel = name.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  return attrs[camel];
}

/** PHP crc32() of a UTF-8 string (used for colour hashing, like Flux and the web app). */
export function crc32(str) {
  let crc = 0xffffffff;
  for (const b of new TextEncoder().encode(String(str))) {
    let c = (crc ^ b) & 0xff;
    for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1;
    crc = (crc >>> 8) ^ c;
  }
  return (crc ^ 0xffffffff) >>> 0;
}
