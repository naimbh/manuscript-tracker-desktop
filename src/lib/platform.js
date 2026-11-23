/** Platform flags for desktop-specific chrome. */
const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
export const isMac = /Mac|iPhone|iPad/.test(ua);
export const isWindows = /Windows/.test(ua);
/** Label of the primary modifier: '⌘' on macOS, 'Ctrl' elsewhere. */
export const modKey = isMac ? '⌘' : 'Ctrl';
/** Human label for a shortcut combo: kbd('mod+k') -> '⌘K' / 'Ctrl+K'. */
export function kbd(combo) {
  const parts = combo.split('+').map((p) => p.trim());
  const names = { mod: modKey, shift: isMac ? '⇧' : 'Shift', alt: isMac ? '⌥' : 'Alt', escape: 'Esc', enter: '↵', backslash: '\\' };
  const out = parts.map((p) => names[p.toLowerCase()] ?? (p.length === 1 ? p.toUpperCase() : p[0].toUpperCase() + p.slice(1)));
  return isMac ? out.join('') : out.join('+');
}

/**
 * Apply the platform and window material from app.boot ({ platform: 'macos'|'windows'|'linux', material: 'sidebar'|'mica'|null })
 * as classes on <html>: platform-mac | platform-windows | platform-linux, and material-sidebar | material-mica.
 * main.js applies a user-agent guess first; call this again with the boot data.
 */
export function applyPlatform({ platform = null, material = null } = {}) {
  const el = document.documentElement;
  const p = platform ?? (isMac ? 'macos' : isWindows ? 'windows' : 'linux');
  el.classList.toggle('platform-mac', p === 'macos');
  el.classList.toggle('platform-windows', p === 'windows');
  el.classList.toggle('platform-linux', p === 'linux');
  el.classList.toggle('material-sidebar', material === 'sidebar');
  el.classList.toggle('material-mica', material === 'mica');
}
