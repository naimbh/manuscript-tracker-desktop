// Icon sets for FluxIcon.
//
//   node scripts/build-icons.mjs --blade <dir>...   Rebuild src/flux/icons-full.js (every icon) from the Flux
//                                                   icon Blade files (Heroicons and Lucide).
//   node scripts/build-icons.mjs                    Write src/flux/icons.js with only the icons the app uses
//                                                   (`npm run icons`; runs before every web build).
//
// Used icons are found by scanning src/**/*.{vue,js} and src-tauri/src/**/*.rs for quoted strings and icon/name attributes
// that match an icon name, so names in constants and dynamic bindings are kept too. Add a name to EXTRA if
// an icon name is ever built from pieces at runtime.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const FULL = path.join(root, 'src/flux/icons-full.js');
const OUT = path.join(root, 'src/flux/icons.js');
const HEADER = '// Heroicons (MIT) and Lucide (ISC) icons, as used by Flux.\n';
const EXTRA = [];

const args = process.argv.slice(2);
if (args[0] === '--blade') fromBlade(args.slice(1));
else trim();

function fromBlade(dirs) {
  const out = {};
  const attrs = (s) => Object.fromEntries([...s.matchAll(/([a-zA-Z-:]+)="([^"]*)"/g)].map((m) => [m[1], m[2]]));
  for (const dir of dirs) {
    for (const file of fs.readdirSync(dir).filter((f) => f.endsWith('.blade.php') && f !== 'index.blade.php')) {
      const name = file.replace('.blade.php', '');
      const src = fs.readFileSync(path.join(dir, file), 'utf8').replace(/\{\{[\s\S]*?\}\}/g, '');
      const icon = {};
      const cases = [...src.matchAll(/case \('(\w+)'\): \?>\s*<svg([^>]*)>([\s\S]*?)<\/svg>/g)];
      if (cases.length) {
        for (const [, v, a, body] of cases) {
          const at = attrs(a.replace(/\{\{[^}]*\}\}/g, ''));
          delete at['data-flux-icon']; delete at['data-slot']; delete at['aria-hidden']; delete at.xmlns;
          icon[v] = { a: at, b: body.trim().replace(/\s+/g, ' ') };
        }
      } else {
        const m = src.match(/<svg([\s\S]*?)>([\s\S]*?)<\/svg>/);
        if (!m) continue;
        const at = attrs(m[1].replace(/\{\{[^}]*\}\}/g, ''));
        delete at['data-flux-icon']; delete at['data-slot']; delete at['aria-hidden']; delete at.xmlns; delete at['stroke-width'];
        const body = m[2].trim().replace(/\s+/g, ' ');
        for (const [v, w] of [['outline', 2], ['mini', 2.25], ['micro', 2.5]]) icon[v] = { a: { ...at, 'stroke-width': String(w) }, b: body };
      }
      out[name] = icon;
    }
  }
  fs.writeFileSync(FULL, HEADER + 'export default ' + JSON.stringify(out) + ';\n');
  console.log(Object.keys(out).length, 'icons');
}

function sources(dir, files = []) {
  for (const f of fs.readdirSync(dir)) {
    const p = path.join(dir, f);
    if (fs.statSync(p).isDirectory()) sources(p, files);
    else if (/\.(vue|js|rs)$/.test(f) && p !== FULL && p !== OUT && !f.startsWith('_')) files.push(p);
  }
  return files;
}

function trim() {
  const src = fs.readFileSync(FULL, 'utf8');
  const all = JSON.parse(src.slice(src.indexOf('{'), src.lastIndexOf('}') + 1));
  const used = new Set(EXTRA);
  // The backend sends some icon names (Desk cards), so its sources are scanned too.
  for (const file of [...sources(path.join(root, 'src')), ...sources(path.join(root, 'src-tauri/src'))]) {
    const s = fs.readFileSync(file, 'utf8');
    for (const m of s.matchAll(/['"`]([a-z0-9]+(?:-[a-z0-9]+)*)['"`]/g)) if (all[m[1]]) used.add(m[1]);
  }
  // Compact form: attribute sets and SVG bodies are stored once and referenced by index (Lucide icons share one
  // body across variants; Heroicons share four attribute sets). FluxIcon expands { variant: [attrs, body] }.
  const attrs = [];
  const bodies = [];
  const ref = (list, v) => {
    const key = typeof v === 'string' ? v : JSON.stringify(v);
    let i = list.findIndex((x) => (typeof x === 'string' ? x : JSON.stringify(x)) === key);
    if (i < 0) i = list.push(v) - 1;
    return i;
  };
  const icons = {};
  for (const name of [...used].sort()) {
    icons[name] = Object.fromEntries(Object.entries(all[name]).map(([v, d]) => [v, [ref(attrs, d.a), ref(bodies, d.b)]]));
  }
  fs.writeFileSync(
    OUT,
    HEADER +
      '// Only the icons the app uses, taken from icons-full.js. Rebuild with `npm run icons`; do not edit.\n' +
      'const A = ' + JSON.stringify(attrs) + ';\n' +
      'const B = ' + JSON.stringify(bodies) + ';\n' +
      'const I = ' + JSON.stringify(icons) + ';\n' +
      'export default Object.fromEntries(Object.entries(I).map(([n, vs]) => [n, Object.fromEntries(Object.entries(vs).map(([v, [a, b]]) => [v, { a: A[a], b: B[b] }]))]));\n',
  );
  console.log(`${used.size} of ${Object.keys(all).length} icons → src/flux/icons.js (${(fs.statSync(OUT).size / 1024).toFixed(0)} kB)`);
}
