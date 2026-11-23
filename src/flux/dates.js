/**
 * Date helpers for FluxDatePicker. Dates are local calendar days; values are 'YYYY-MM-DD' strings.
 */

export const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
export const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const pad = (n) => String(n).padStart(2, '0');

/** A local Date at midnight, or null when the parts do not form a real day (e.g. 31 Feb). */
export function makeDay(y, m, d) {
  const t = new Date(y, m, d);
  t.setHours(0, 0, 0, 0);
  if (y < 100) t.setFullYear(y);
  return t.getFullYear() === y && t.getMonth() === m && t.getDate() === d ? t : null;
}

export function todayDay() { const t = new Date(); t.setHours(0, 0, 0, 0); return t; }

/** 'YYYY-MM-DD' → Date (local midnight) or null. Also accepts Date objects. */
export function fromKey(v) {
  if (v instanceof Date) return isNaN(v) ? null : makeDay(v.getFullYear(), v.getMonth(), v.getDate());
  const m = String(v ?? '').trim().match(/^(\d{4})-(\d{2})-(\d{2})/);
  return m ? makeDay(+m[1], +m[2] - 1, +m[3]) : null;
}

export const toKey = (d) => (d ? `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` : '');
export const sameDay = (a, b) => !!a && !!b && a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
export const addDays = (d, n) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);

/** Add months, clamping the day to the target month's length (31 Jan + 1 month = 28/29 Feb). */
export function addMonths(d, n) {
  const first = new Date(d.getFullYear(), d.getMonth() + n, 1);
  const last = new Date(first.getFullYear(), first.getMonth() + 1, 0).getDate();
  return new Date(first.getFullYear(), first.getMonth(), Math.min(d.getDate(), last));
}

/** "22 Nov 2026". */
export const formatDay = (d) => (d ? `${d.getDate()} ${MONTHS[d.getMonth()].slice(0, 3)} ${d.getFullYear()}` : '');
/** "Sunday, 22 November 2026" (screen readers). */
export const longDay = (d) => `${DAYS[d.getDay()]}, ${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;

/** Start of the week holding d (weekStart 1 = Monday). */
export function weekStartOf(d, weekStart = 1) {
  return addDays(d, -((d.getDay() - weekStart + 7) % 7));
}

/** The 42 days (6 weeks) shown for a month. */
export function monthGrid(year, month, weekStart = 1) {
  const start = weekStartOf(new Date(year, month, 1), weekStart);
  return Array.from({ length: 42 }, (_, i) => addDays(start, i));
}

function monthIndex(word) {
  const w = word.toLowerCase().replace(/\.$/, '');
  if (w.length < 3) return -1;
  return MONTHS.findIndex((m) => m.toLowerCase().startsWith(w) || (w === 'sept' && m === 'September'));
}
function dayIndex(word) {
  const w = word.toLowerCase();
  if (w.length < 2) return -1;
  return DAYS.findIndex((d) => d.toLowerCase().startsWith(w));
}
const fullYear = (y, s) => (s.length <= 2 ? 2000 + y : y);

/**
 * Parse what a person types into a date field. Returns a Date, null for an empty string, or undefined when it is
 * not a date. Understands: 2026-11-22, 2026/11/22, 22/11/2026, 22.11.26, 22/11, Nov 22 2026, Nov 22, 2026,
 * 22 Nov, 22nd November 2026, today, tomorrow, yesterday, next week, next month, next year, +14, -3, +2w, +1m, +1y,
 * in 3 days, in 2 weeks, 3 days ago, monday / next fri.
 */
export function parseDateInput(text, base = todayDay()) {
  let s = String(text ?? '').trim().toLowerCase().replace(/\s+/g, ' ');
  if (s === '') return null;
  let m;

  if ((m = s.match(/^(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})$/))) return makeDay(+m[1], +m[2] - 1, +m[3]) ?? undefined;
  if ((m = s.match(/^(\d{1,2})[-/.](\d{1,2})(?:[-/.](\d{2}|\d{4}))?$/))) {
    const y = m[3] ? fullYear(+m[3], m[3]) : base.getFullYear();
    return makeDay(y, +m[2] - 1, +m[1]) ?? undefined;
  }

  const words = { today: 0, tod: 0, now: 0, tomorrow: 1, tmr: 1, tom: 1, yesterday: -1 };
  if (s in words) return addDays(base, words[s]);
  if (s === 'next week') return addDays(base, 7);
  if (s === 'next month') return addMonths(base, 1);
  if (s === 'next year') return addMonths(base, 12);
  if (s === 'last week') return addDays(base, -7);
  if (s === 'last month') return addMonths(base, -1);

  // +14, -3, +2w, +1m, +1y, in 3 days, in 2 weeks, 3 days ago
  const unit = (n, u) => {
    u = (u || 'd')[0];
    return u === 'w' ? addDays(base, 7 * n) : u === 'm' ? addMonths(base, n) : u === 'y' ? addMonths(base, 12 * n) : addDays(base, n);
  };
  if ((m = s.match(/^([+-])\s?(\d{1,4})\s?(d|days?|w|wks?|weeks?|m|mos?|months?|y|yrs?|years?)?$/))) return unit((m[1] === '-' ? -1 : 1) * +m[2], m[3]);
  if ((m = s.match(/^in (\d{1,4}|an?|one) ?(days?|weeks?|months?|years?)$/))) return unit(/^\d/.test(m[1]) ? +m[1] : 1, m[2]);
  if ((m = s.match(/^(\d{1,4}|an?|one) ?(days?|weeks?|months?|years?) ago$/))) return unit(-(/^\d/.test(m[1]) ? +m[1] : 1), m[2]);

  // monday, next fri, this friday: the next such day after today (today itself for "this").
  if ((m = s.match(/^(next |this )?([a-z]+)$/)) && dayIndex(m[2]) >= 0) {
    let n = (dayIndex(m[2]) - base.getDay() + 7) % 7;
    if (n === 0 && m[1] !== 'this ') n = 7;
    return addDays(base, n);
  }

  // Month names: "nov 22 2026", "november 22, 2026", "22 nov", "22nd of november 2026", "nov 2026" (1st).
  s = s.replace(/,/g, ' ').replace(/(\d)(st|nd|rd|th)\b/g, '$1').replace(/\bof\b/g, ' ').replace(/\s+/g, ' ').trim();
  if ((m = s.match(/^([a-z.]+) (\d{1,2})(?: (\d{2}|\d{4}))?$/)) && monthIndex(m[1]) >= 0) {
    return makeDay(m[3] ? fullYear(+m[3], m[3]) : base.getFullYear(), monthIndex(m[1]), +m[2]) ?? undefined;
  }
  if ((m = s.match(/^(\d{1,2}) ([a-z.]+)(?: (\d{2}|\d{4}))?$/)) && monthIndex(m[2]) >= 0) {
    return makeDay(m[3] ? fullYear(+m[3], m[3]) : base.getFullYear(), monthIndex(m[2]), +m[1]) ?? undefined;
  }
  if ((m = s.match(/^([a-z.]+) (\d{4})$/)) && monthIndex(m[1]) >= 0) return makeDay(+m[2], monthIndex(m[1]), 1);
  if ((m = s.match(/^(\d{4}) ([a-z.]+) (\d{1,2})$/)) && monthIndex(m[2]) >= 0) return makeDay(+m[1], monthIndex(m[2]), +m[3]) ?? undefined;

  return undefined;
}
