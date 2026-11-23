/**
 * Carbon / Laravel Str equivalents. All dates are handled in local time.
 * Accepted date inputs: Date, 'YYYY-MM-DD' (local date), 'YYYY-MM-DD HH:MM[:SS]' (local),
 * ISO strings with 'T' (and optional Z/offset), or a millisecond timestamp.
 */

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const pad = (n, l = 2) => String(n).padStart(l, '0');

export function parseDate(v) {
  if (v === null || v === undefined || v === '') return null;
  if (v instanceof Date) return isNaN(v) ? null : new Date(v.getTime());
  if (typeof v === 'number') return new Date(v);
  const s = String(v).trim();
  let m = s.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (m) return new Date(+m[1], +m[2] - 1, +m[3]);
  m = s.match(/^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})(?::(\d{2}))?(?:\.\d+)?$/);
  if (m) return new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +(m[6] || 0));
  const d = new Date(s);
  return isNaN(d) ? null : d;
}

let testNow = null;
/** Freeze 'now' (tests only), like Carbon::setTestNow(). */
export function setTestNow(d) { testNow = d ? parseDate(d) : null; }
export const now = () => (testNow ? new Date(testNow.getTime()) : new Date());
export function today() { const d = now(); d.setHours(0, 0, 0, 0); return d; }
export function startOfDay(v) { const d = parseDate(v); if (d) d.setHours(0, 0, 0, 0); return d; }
export function addDays(v, n) { const d = parseDate(v); d.setDate(d.getDate() + n); return d; }
export function addMonths(v, n) { const d = parseDate(v); d.setMonth(d.getMonth() + n); return d; }

/** 'YYYY-MM-DD' for an <input type="date">. */
export function toDateString(v) { const d = parseDate(v); return d ? `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` : ''; }

function isoWeek(d) {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const day = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - day);
  const y0 = new Date(Date.UTC(t.getUTCFullYear(), 0, 1));
  return Math.ceil(((t - y0) / 86400000 + 1) / 7);
}

function ordinal(n) {
  if (n % 100 >= 11 && n % 100 <= 13) return 'th';
  return ['th', 'st', 'nd', 'rd'][n % 10] || 'th';
}

/** PHP date() format: format(d, 'j M Y'), format(d, 'l, j F Y'), format(d, 'Ymd\\THis\\Z'). Returns '' for empty dates. */
export function format(v, fmt = 'Y-m-d H:i:s') {
  const d = parseDate(v);
  if (!d) return '';
  const leap = new Date(d.getFullYear(), 1, 29).getDate() === 29;
  const start = new Date(d.getFullYear(), 0, 1);
  const h = d.getHours();
  const map = {
    d: () => pad(d.getDate()), D: () => DAYS[d.getDay()].slice(0, 3), j: () => d.getDate(), l: () => DAYS[d.getDay()],
    N: () => d.getDay() || 7, S: () => ordinal(d.getDate()), w: () => d.getDay(),
    z: () => Math.round((new Date(d.getFullYear(), d.getMonth(), d.getDate()) - start) / 86400000),
    W: () => pad(isoWeek(d)), F: () => MONTHS[d.getMonth()], m: () => pad(d.getMonth() + 1), M: () => MONTHS[d.getMonth()].slice(0, 3),
    n: () => d.getMonth() + 1, t: () => new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate(), L: () => (leap ? 1 : 0),
    Y: () => d.getFullYear(), y: () => pad(d.getFullYear() % 100), a: () => (h < 12 ? 'am' : 'pm'), A: () => (h < 12 ? 'AM' : 'PM'),
    g: () => h % 12 || 12, G: () => h, h: () => pad(h % 12 || 12), H: () => pad(h), i: () => pad(d.getMinutes()), s: () => pad(d.getSeconds()),
    v: () => pad(d.getMilliseconds(), 3), U: () => Math.floor(d.getTime() / 1000),
  };
  let out = '';
  for (let i = 0; i < fmt.length; i++) {
    const c = fmt[i];
    if (c === '\\') { out += fmt[++i] ?? ''; continue; }
    out += map[c] ? map[c]() : c;
  }
  return out;
}

/** Calendar difference between two dates (like PHP DateInterval), always positive. */
function interval(a, b) {
  let from = a, to = b;
  if (from > to) [from, to] = [to, from];
  let months = (to.getFullYear() - from.getFullYear()) * 12 + (to.getMonth() - from.getMonth());
  const plus = (n) => { const x = new Date(from.getTime()); x.setMonth(x.getMonth() + n); return x; };
  while (months > 0 && plus(months) > to) months--;
  const base = plus(months);
  // Whole days in wall-clock time, then the remaining time.
  let days = 0;
  let cursor = new Date(base.getTime());
  while (true) {
    const next = new Date(cursor.getTime());
    next.setDate(next.getDate() + 1);
    if (next > to) break;
    cursor = next;
    days++;
  }
  let rest = Math.floor((to - cursor) / 1000);
  const hours = Math.floor(rest / 3600); rest -= hours * 3600;
  const minutes = Math.floor(rest / 60);
  const seconds = rest - minutes * 60;
  return { y: Math.floor(months / 12), m: months % 12, d: days, h: hours, i: minutes, s: seconds };
}

const UNITS = {
  y: ['year', 'years', 'yr', 'yrs'], m: ['month', 'months', 'mo', 'mos'], w: ['week', 'weeks', 'w', 'w'], d: ['day', 'days', 'd', 'd'],
  h: ['hour', 'hours', 'h', 'h'], i: ['minute', 'minutes', 'm', 'm'], s: ['second', 'seconds', 's', 's'],
};

/**
 * Carbon 3 diffForHumans(other = null, absolute = false, short = false):
 *   diffForHumans(d) -> '3 days ago' / '2 weeks from now'; diffForHumans(d, other) -> '1 day before' / '1 week after';
 *   diffForHumans(d, null, true, true) -> '3d'. One part, truncated (45 days -> '1 month').
 */
export function diffForHumans(v, other = null, absolute = false, short = false) {
  const d = parseDate(v);
  if (!d) return '';
  const o = other ? parseDate(other) : now();
  const iv = interval(d, o);
  const w = iv.d >= 7 ? Math.floor(iv.d / 7) : 0;
  const parts = [['y', iv.y], ['m', iv.m], ['w', w], ['d', w ? 0 : iv.d], ['h', iv.h], ['i', iv.i], ['s', iv.s]];
  const [unit, count] = parts.find(([, n]) => n > 0) || ['s', 0];
  const u = UNITS[unit];
  const text = short ? `${count}${count === 1 ? u[2] : u[3]}` : `${count} ${count === 1 ? u[0] : u[1]}`;
  if (absolute) return text;
  const past = d <= o;
  if (other) return `${text} ${past ? 'before' : 'after'}`;
  return past ? `${text} ago` : `${text} from now`;
}

/** Carbon 3 diffIn*: signed, from a to b (b - a), fractional like Carbon 3. Pass absolute=true for the size only. */
export function diffInSeconds(a, b = now(), absolute = false) {
  const x = (parseDate(b) - parseDate(a)) / 1000;
  return absolute ? Math.abs(x) : x;
}
export function diffInMinutes(a, b = now(), absolute = false) { return diffInSeconds(a, b, absolute) / 60; }
export function diffInHours(a, b = now(), absolute = false) { return diffInSeconds(a, b, absolute) / 3600; }
export function diffInDays(a, b = now(), absolute = false) {
  const x = parseDate(a), y = parseDate(b);
  // Whole calendar days plus the fraction of the remaining time (DST-safe for midnight dates).
  const dx = Date.UTC(x.getFullYear(), x.getMonth(), x.getDate()), dy = Date.UTC(y.getFullYear(), y.getMonth(), y.getDate());
  const msx = x - new Date(x.getFullYear(), x.getMonth(), x.getDate()), msy = y - new Date(y.getFullYear(), y.getMonth(), y.getDate());
  const r = (dy - dx) / 86400000 + (msy - msx) / 86400000;
  return absolute ? Math.abs(r) : r;
}
/** (int) cast of a Carbon diff (truncates toward zero, like PHP). */
export const int = (n) => (n < 0 ? Math.ceil(n) : Math.floor(n)) || 0;

export const isToday = (v) => toDateString(v) === toDateString(now());
export const isPast = (v) => parseDate(v) < now();
export const isFuture = (v) => parseDate(v) > now();

/* ---------- Str helpers ---------- */

/** Str::limit(value, limit = 100, end = '...'). */
export function limit(value, n = 100, end = '...') {
  const s = String(value ?? '');
  const chars = [...s];
  if (chars.length <= n) return s;
  return chars.slice(0, n).join('').replace(/\s+$/, '') + end;
}

const IRREGULAR = { person: 'people', man: 'men', woman: 'women', child: 'children', foot: 'feet', tooth: 'teeth', mouse: 'mice', criterion: 'criteria', datum: 'data', medium: 'media', index: 'indices', appendix: 'appendices', thesis: 'theses' };
const UNCOUNTABLE = ['data', 'information', 'feedback', 'equipment', 'news', 'series', 'species', 'software', 'research', 'evidence', 'metadata', 'funding', 'advice', 'knowledge'];

function pluralise(word) {
  const lower = word.toLowerCase();
  if (UNCOUNTABLE.includes(lower)) return word;
  let out;
  if (IRREGULAR[lower]) out = IRREGULAR[lower];
  else if (/[^aeiou]y$/.test(lower)) out = word.slice(0, -1) + 'ies';
  else if (/(ss|sh|ch|x|z|s)$/.test(lower) && !/is$/.test(lower)) out = word + 'es';
  else if (/is$/.test(lower)) out = word.slice(0, -2) + 'es';
  else if (/(?:[^f]fe|[lr]f)$/.test(lower)) out = word.replace(/fe?$/, 'ves');
  else out = word + 's';
  if (word === word.toUpperCase() && word.length > 1) return out.toUpperCase();
  if (word[0] === word[0].toUpperCase()) return out[0].toUpperCase() + out.slice(1);
  return out;
}

/** Str::plural(word, count = 2): plural('comment', 1) -> 'comment', plural('reply', 3) -> 'replies'. */
export function plural(word, count = 2) {
  const n = Array.isArray(count) ? count.length : Number(count);
  return Math.abs(n) === 1 ? word : pluralise(String(word));
}

export function before(subject, search) { const s = String(subject ?? ''); if (search === '') return s; const i = s.indexOf(search); return i === -1 ? s : s.slice(0, i); }
export function beforeLast(subject, search) { const s = String(subject ?? ''); if (search === '') return s; const i = s.lastIndexOf(search); return i === -1 ? s : s.slice(0, i); }
export function after(subject, search) { const s = String(subject ?? ''); if (search === '') return s; const i = s.indexOf(search); return i === -1 ? s : s.slice(i + search.length); }
export function afterLast(subject, search) { const s = String(subject ?? ''); if (search === '') return s; const i = s.lastIndexOf(search); return i === -1 ? s : s.slice(i + search.length); }
export function contains(subject, needles) { const s = String(subject ?? ''); return [].concat(needles).some((n) => n !== '' && s.includes(n)); }
export const lower = (s) => String(s ?? '').toLowerCase();
export const upper = (s) => String(s ?? '').toUpperCase();
export const ucfirst = (s) => { s = String(s ?? ''); return s.charAt(0).toUpperCase() + s.slice(1); };
export const title = (s) => String(s ?? '').toLowerCase().replace(/(^|[\s\-_])(\p{L})/gu, (m, a, b) => a + b.toUpperCase());

/** User::initials(): first letter of each space-separated word. */
export function initials(name) {
  return String(name ?? '').split(' ').map((p) => [...p][0] ?? '').join('');
}

/** PHP number_format(n, decimals = 0, '.', ','): rounds half away from zero. */
export function numberFormat(n, decimals = 0, dec = '.', sep = ',') {
  const x = Number(n) || 0;
  const f = 10 ** decimals;
  const r = Math.sign(x) * Math.round(Math.abs(x) * f + Number.EPSILON) / f;
  const [i, d] = Math.abs(r).toFixed(decimals).split('.');
  return (r < 0 ? '-' : '') + i.replace(/\B(?=(\d{3})+(?!\d))/g, sep) + (decimals ? dec + d : '');
}
