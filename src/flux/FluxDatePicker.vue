<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useAttrs, watch } from 'vue';
import FluxIcon from './FluxIcon.vue';
import FluxWithField from './FluxWithField.vue';
import { hidePopover, showPopover } from './anchor.js';
import { errorProps, useFieldError } from './errors.js';
import { inputClasses } from './inputClasses.js';
import { cx } from './util.js';
import {
  DAYS, MONTHS, addDays, addMonths, formatDay, fromKey, longDay, makeDay, monthGrid, parseDateInput, sameDay, toKey, todayDay, weekStartOf,
} from './dates.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  ...errorProps,
  modelValue: { type: [String, Date], default: null }, // 'YYYY-MM-DD', '' or null
  label: { type: String, default: null },
  description: { type: String, default: null },
  descriptionTrailing: { type: String, default: null },
  badge: { type: String, default: null },
  placeholder: { type: String, default: 'Pick a date' },
  size: { type: String, default: null },
  variant: { type: String, default: 'outline' },
  disabled: { type: Boolean, default: false },
  required: { type: Boolean, default: false },
  invalid: { type: Boolean, default: false },
  clearable: { type: Boolean, default: true },
  min: { type: [String, Date], default: null },
  max: { type: [String, Date], default: null },
  weekStart: { type: Number, default: 1 }, // 0 = Sunday, 1 = Monday
  align: { type: String, default: 'start' }, // start | end: which field edge the calendar lines up with
});
const emit = defineEmits(['update:modelValue', 'change']);
const attrs = useAttrs();
const message = useFieldError(props);

const popId = `flux-date-${Math.random().toString(36).slice(2, 10)}`;

const wrap = ref(null);
const input = ref(null);
const pop = ref(null);
const grid = ref(null);

const isOpen = ref(false);
const view = ref('days'); // days | months | years
const viewYear = ref(0);
const viewMonth = ref(0);
const cursor = ref(todayDay()); // the keyboard-focused day
const text = ref('');
const dirty = ref(false);
const bad = ref(false);

const selected = computed(() => fromKey(props.modelValue));
const minDay = computed(() => fromKey(props.min));
const maxDay = computed(() => fromKey(props.max));
const today = computed(() => { void isOpen.value; return todayDay(); }); // refreshed each time the popover opens
const inRange = (d) => !!d && !(minDay.value && d < minDay.value) && !(maxDay.value && d > maxDay.value);
const clamp = (d) => (minDay.value && d < minDay.value ? minDay.value : maxDay.value && d > maxDay.value ? maxDay.value : d);

const isInvalid = computed(() => props.invalid || bad.value || !!message.value);
const hasValue = computed(() => !!selected.value);
const showClear = computed(() => props.clearable && !props.required && !props.disabled && (hasValue.value || (dirty.value && text.value !== '')));

const classes = computed(() => cx(
  inputClasses({ size: props.size, variant: props.variant }),
  'ps-3',
  showClear.value ? 'pe-16' : 'pe-10',
  'cursor-default focus:cursor-text tabular-nums',
  attrs['class:input'],
));
const btnSize = computed(() => (props.size === 'sm' || props.size === 'xs' ? 'size-5.5' : 'size-7'));
const inputAttrs = computed(() => {
  const rest = {};
  for (const [k, v] of Object.entries(attrs)) if (k !== 'class' && !k.includes(':')) rest[k] = v;
  return rest;
});

/* ---------- value <-> text ---------- */
function syncText() { if (!dirty.value) text.value = formatDay(selected.value); }
watch(() => props.modelValue, syncText, { immediate: true });

function setValue(d) {
  const key = toKey(d);
  const changed = key !== toKey(selected.value) || (!d && props.modelValue !== '' && props.modelValue != null);
  dirty.value = false;
  bad.value = false;
  text.value = formatDay(d);
  if (changed) { emit('update:modelValue', key); emit('change', key); }
}
let badTimer = null;
function reject() {
  bad.value = true;
  clearTimeout(badTimer);
  const el = input.value;
  if (el) { el.classList.remove('flux-date-shake'); void el.offsetWidth; el.classList.add('flux-date-shake'); }
}
/** Parse typed text; returns false (and flags the field) when it is not a usable date. */
function commit() {
  if (!dirty.value) return true;
  const d = parseDateInput(text.value, todayDay());
  if (d === undefined || (d && !inRange(d)) || (d === null && props.required)) { reject(); return false; }
  setValue(d);
  return true;
}
function revert() { dirty.value = false; syncText(); }

/* ---------- popover ---------- */
function showMonthOf(d) { viewYear.value = d.getFullYear(); viewMonth.value = d.getMonth(); }
/**
 * Place the calendar below the field, or above it when there is more room there. Unlike anchor() it never covers the
 * field: when neither side fits it switches to compact cells, then caps the height (the calendar scrolls).
 */
const GAP = 6;
const PAD = 8;
function measure() {
  const el = pop.value;
  Object.assign(el.style, { position: 'fixed', inset: 'auto', margin: '0', top: '0px', left: '0px', maxHeight: '' });
  return el.offsetHeight;
}
function reposition() {
  const el = pop.value;
  if (!isOpen.value || !el || !wrap.value) return;
  const r = wrap.value.getBoundingClientRect();
  const vw = document.documentElement.clientWidth;
  const vh = document.documentElement.clientHeight;
  const below = vh - r.bottom - GAP - PAD;
  const above = r.top - GAP - PAD;
  el.removeAttribute('data-compact'); // data-compact: smaller day cells when the window is too short
  let h = measure();
  if (h > below && h > above) { el.setAttribute('data-compact', ''); h = measure(); }
  const down = h <= below || (h > above && below >= above);
  const room = down ? below : above;
  if (h > room) el.style.maxHeight = `${Math.max(160, room)}px`;
  const height = Math.min(h, Math.max(160, room));
  const top = down ? r.bottom + GAP : r.top - GAP - height;
  const rtl = document.dir === 'rtl';
  const w = el.offsetWidth;
  let left = (props.align === 'end') !== rtl ? r.right - w : r.left;
  left = Math.min(Math.max(PAD, left), Math.max(PAD, vw - w - PAD));
  el.style.top = `${Math.round(top)}px`;
  el.style.left = `${Math.round(left)}px`;
  el.dataset.side = down ? 'bottom' : 'top';
}

function open() {
  if (props.disabled || isOpen.value || !pop.value) return;
  const typed = dirty.value ? parseDateInput(text.value, todayDay()) : null;
  cursor.value = clamp(typed || selected.value || todayDay());
  showMonthOf(cursor.value);
  view.value = 'days';
  showPopover(pop.value);
  isOpen.value = true;
  reposition();
}
function close(restoreFocus = false) {
  if (!isOpen.value) return;
  hidePopover(pop.value);
  isOpen.value = false;
  if (restoreFocus) input.value?.focus({ preventScroll: true });
}
function toggleFromIcon() {
  if (props.disabled) return;
  if (isOpen.value) { close(); return; }
  input.value?.focus({ preventScroll: true });
  open();
}
function focusCursor() {
  nextTick(() => {
    const sel = view.value === 'days' ? `[data-date="${toKey(cursor.value)}"]` : '[data-cursor]';
    grid.value?.querySelector(sel)?.focus({ preventScroll: true });
  });
}
function gridHasFocus() { return !!grid.value && grid.value.contains(document.activeElement); }

/* ---------- picking ---------- */
function pick(d) {
  if (!inRange(d)) return;
  setValue(d);
  close(true);
}
function moveTo(d) {
  cursor.value = d;
  showMonthOf(d);
  focusCursor();
}
function step(dir) {
  const wasFocused = gridHasFocus();
  if (view.value === 'days') {
    const d = new Date(viewYear.value, viewMonth.value + dir, 1);
    showMonthOf(d);
    cursor.value = addMonths(cursor.value, dir);
    if (cursor.value.getMonth() !== d.getMonth()) cursor.value = d;
  } else {
    viewYear.value += dir * (view.value === 'years' ? 12 : 1);
  }
  if (wasFocused) focusCursor();
}
function chooseMonth(m) {
  viewMonth.value = m;
  cursor.value = makeDay(viewYear.value, m, Math.min(cursor.value.getDate(), new Date(viewYear.value, m + 1, 0).getDate()));
  view.value = 'days';
  focusCursor();
}
function chooseYear(y) {
  viewYear.value = y;
  view.value = 'months';
  focusCursor();
}
function setView(v) {
  view.value = view.value === v ? 'days' : v;
  focusCursor();
}

/* ---------- grids ---------- */
const weekdays = computed(() => Array.from({ length: 7 }, (_, i) => DAYS[(i + props.weekStart) % 7]));
const weeks = computed(() => {
  const days = monthGrid(viewYear.value, viewMonth.value, props.weekStart);
  return Array.from({ length: 6 }, (_, w) => days.slice(w * 7, w * 7 + 7));
});
const yearStart = computed(() => Math.floor(viewYear.value / 12) * 12);
const years = computed(() => Array.from({ length: 12 }, (_, i) => yearStart.value + i));
const monthOk = (y, m) => !(minDay.value && new Date(y, m + 1, 0) < minDay.value) && !(maxDay.value && new Date(y, m, 1) > maxDay.value);
const yearOk = (y) => !(minDay.value && y < minDay.value.getFullYear()) && !(maxDay.value && y > maxDay.value.getFullYear());

const headerLabel = computed(() => (view.value === 'years' ? `${yearStart.value} – ${yearStart.value + 11}` : `${MONTHS[viewMonth.value]} ${viewYear.value}`));
const prevLabel = computed(() => ({ days: 'Previous month', months: 'Previous year', years: 'Previous 12 years' }[view.value]));
const nextLabel = computed(() => ({ days: 'Next month', months: 'Next year', years: 'Next 12 years' }[view.value]));

function dayClasses(d) {
  const sel = sameDay(d, selected.value);
  const out = d.getMonth() !== viewMonth.value;
  const ok = inRange(d);
  return cx(
    'relative flex size-9.5 group-data-[compact]/dp:size-8 items-center justify-center rounded-lg text-sm tabular-nums outline-none transition-colors duration-100',
    'focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:ring-offset-1 focus-visible:ring-offset-white dark:focus-visible:ring-indigo-400 dark:focus-visible:ring-offset-zinc-800',
    sel ? 'bg-indigo-600 font-semibold text-white shadow-xs hover:bg-indigo-500 dark:bg-indigo-500 dark:hover:bg-indigo-400'
      : !ok ? 'cursor-not-allowed text-zinc-300 line-through decoration-zinc-300/70 dark:text-zinc-600 dark:decoration-zinc-600/70'
        : out ? 'text-zinc-400 hover:bg-zinc-100 hover:text-zinc-700 dark:text-zinc-500 dark:hover:bg-white/5 dark:hover:text-zinc-200'
          : 'font-medium text-zinc-800 hover:bg-zinc-100 dark:text-zinc-100 dark:hover:bg-white/10',
    !sel && sameDay(d, today.value) ? 'ring-1 ring-inset ring-indigo-500/70 text-indigo-700 dark:ring-indigo-400/70 dark:text-indigo-300' : '',
  );
}
function cellClasses(isSel, isCur, ok) {
  return cx(
    'flex h-10 w-full items-center justify-center rounded-lg text-sm outline-none transition-colors duration-100',
    'focus-visible:ring-2 focus-visible:ring-indigo-500 dark:focus-visible:ring-indigo-400',
    isSel ? 'bg-indigo-600 font-semibold text-white hover:bg-indigo-500 dark:bg-indigo-500 dark:hover:bg-indigo-400'
      : !ok ? 'cursor-not-allowed text-zinc-300 dark:text-zinc-600'
        : 'font-medium text-zinc-700 hover:bg-zinc-100 dark:text-zinc-200 dark:hover:bg-white/10',
    !isSel && isCur ? 'ring-1 ring-inset ring-indigo-500/70 dark:ring-indigo-400/70' : '',
  );
}

/* ---------- quick picks ---------- */
const quick = computed(() => {
  const t = today.value;
  return [
    { label: 'Today', d: t },
    { label: 'Tomorrow', d: addDays(t, 1) },
    { label: 'In 1 week', d: addDays(t, 7) },
    { label: 'In 2 weeks', d: addDays(t, 14) },
    { label: 'In 1 month', d: addMonths(t, 1) },
  ];
});
function clear() {
  if (props.required) return;
  setValue(null);
  close(true);
}

/* ---------- keyboard ---------- */
function onInputKey(e) {
  if (e.key === 'ArrowDown' || (e.key === 'ArrowUp' && e.altKey)) {
    e.preventDefault();
    if (!commit()) return;
    open();
    focusCursor();
  } else if (e.key === 'Enter') {
    if (dirty.value) { e.preventDefault(); if (commit()) close(); }
    else if (isOpen.value) { e.preventDefault(); close(); }
  } else if (e.key === 'Escape') {
    if (isOpen.value) { e.preventDefault(); e.stopPropagation(); close(); }
    else if (dirty.value) { e.preventDefault(); e.stopPropagation(); revert(); bad.value = false; }
  } else if (e.key === 'Tab') {
    close();
  }
}
function onPopKey(e) {
  if (e.key === 'Escape') {
    e.preventDefault();
    e.stopPropagation();
    if (view.value !== 'days') { view.value = 'days'; focusCursor(); } else close(true);
    return;
  }
  if (!grid.value?.contains(e.target)) return;
  const keys = ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End'];
  if (!keys.includes(e.key)) return;
  e.preventDefault();
  const rtl = document.dir === 'rtl';
  const h = e.key === 'ArrowLeft' ? (rtl ? 1 : -1) : e.key === 'ArrowRight' ? (rtl ? -1 : 1) : 0;
  if (view.value === 'days') {
    const c = cursor.value;
    let d = c;
    if (h) d = addDays(c, h);
    else if (e.key === 'ArrowUp') d = addDays(c, -7);
    else if (e.key === 'ArrowDown') d = addDays(c, 7);
    else if (e.key === 'PageUp') d = addMonths(c, e.shiftKey ? -12 : -1);
    else if (e.key === 'PageDown') d = addMonths(c, e.shiftKey ? 12 : 1);
    else if (e.key === 'Home') d = weekStartOf(c, props.weekStart);
    else if (e.key === 'End') d = addDays(weekStartOf(c, props.weekStart), 6);
    moveTo(d);
  } else {
    const cols = view.value === 'months' ? 3 : 4;
    const delta = h || (e.key === 'ArrowUp' ? -cols : e.key === 'ArrowDown' ? cols : e.key === 'PageUp' ? -12 : e.key === 'PageDown' ? 12 : 0);
    if (view.value === 'months') {
      let m = viewMonth.value + delta;
      if (e.key === 'Home') m = 0; else if (e.key === 'End') m = 11;
      const d = new Date(viewYear.value, m, 1);
      viewYear.value = d.getFullYear(); viewMonth.value = d.getMonth();
    } else {
      let y = viewYear.value + delta;
      if (e.key === 'Home') y = yearStart.value; else if (e.key === 'End') y = yearStart.value + 11;
      viewYear.value = y;
    }
    focusCursor();
  }
}

/* ---------- field events ---------- */
function onInput(e) {
  text.value = e.target.value;
  dirty.value = true;
  bad.value = false;
  const d = parseDateInput(text.value, todayDay());
  if (d && isOpen.value) { cursor.value = d; showMonthOf(d); view.value = 'days'; }
}
function onBlur(e) {
  const to = e.relatedTarget;
  if (dirty.value && !commit()) {
    revert();
    badTimer = setTimeout(() => { if (!dirty.value) bad.value = false; }, 1600);
  }
  if (to && !wrap.value?.contains(to)) close();
}
function onClick() { if (!isOpen.value) open(); }
function onFocusOut(e) {
  const to = e.relatedTarget;
  if (isOpen.value && to && !wrap.value?.contains(to)) close();
}
function onDocPointer(e) {
  if (!isOpen.value) return;
  if (wrap.value?.contains(e.target)) return;
  close();
}
function onScroll(e) { if (isOpen.value && !pop.value?.contains(e.target)) reposition(); }

onMounted(() => {
  document.addEventListener('pointerdown', onDocPointer, true);
  window.addEventListener('resize', reposition);
  window.addEventListener('scroll', onScroll, true);
});
onBeforeUnmount(() => {
  close();
  document.removeEventListener('pointerdown', onDocPointer, true);
  window.removeEventListener('resize', reposition);
  window.removeEventListener('scroll', onScroll, true);
});

defineExpose({ focus: () => input.value?.focus(), open, close, el: input });
</script>

<template>
    <FluxWithField :label="label" :description="description" :description-trailing="descriptionTrailing" :badge="badge" :error="message">
        <div ref="wrap" :class="['w-full relative block', attrs.class]" data-flux-input data-flux-date-picker @focusout="onFocusOut">
            <input
                ref="input"
                type="text"
                inputmode="text"
                autocomplete="off"
                autocorrect="off"
                autocapitalize="off"
                spellcheck="false"
                :class="classes"
                v-bind="inputAttrs"
                :value="text"
                :name="name"
                :placeholder="placeholder"
                :disabled="disabled"
                :required="required"
                role="combobox"
                aria-haspopup="dialog"
                :aria-expanded="isOpen ? 'true' : 'false'"
                :aria-controls="popId"
                :aria-invalid="isInvalid ? 'true' : undefined"
                :data-invalid="isInvalid ? '' : undefined"
                data-flux-control
                data-flux-group-target
                @input="onInput"
                @blur="onBlur"
                @keydown="onInputKey"
                @click="onClick"
                @animationend="$event.target.classList.remove('flux-date-shake')"
            >
            <div class="absolute inset-y-0 end-0 flex items-center gap-0.5 pe-1.5">
                <button
                    v-if="showClear"
                    type="button"
                    tabindex="-1"
                    aria-label="Clear date"
                    :class="[btnSize, 'flex items-center justify-center rounded-md text-zinc-400 transition-colors hover:bg-zinc-800/5 hover:text-zinc-700 dark:text-white/50 dark:hover:bg-white/10 dark:hover:text-white']"
                    @mousedown.prevent
                    @click="clear"
                ><FluxIcon name="x-mark" variant="micro" /></button>
                <button
                    type="button"
                    tabindex="-1"
                    :disabled="disabled"
                    :aria-label="isOpen ? 'Close calendar' : 'Open calendar'"
                    :class="[btnSize, 'flex items-center justify-center rounded-md transition-colors disabled:pointer-events-none', isOpen ? 'text-indigo-600 dark:text-indigo-300' : 'text-zinc-400 hover:bg-zinc-800/5 hover:text-zinc-700 dark:text-white/50 dark:hover:bg-white/10 dark:hover:text-white']"
                    @mousedown.prevent
                    @click="toggleFromIcon"
                ><FluxIcon name="calendar-days" variant="mini" class="size-[1.125rem]" /></button>
            </div>

            <div
                :id="popId"
                ref="pop"
                popover="manual"
                role="dialog"
                aria-modal="false"
                :aria-label="label ? `Choose ${label.toLowerCase()}` : 'Choose a date'"
                class="group/dp flux-date-pop w-[19rem] max-w-[calc(100vw-10px)] overflow-x-hidden overflow-y-auto overscroll-contain rounded-xl border border-zinc-200 bg-white p-0 text-zinc-800 shadow-xl shadow-zinc-900/10 select-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100 dark:shadow-black/40"
                data-flux-date-popover
                @mousedown.prevent
                @pointerdown.stop
                @click.stop
                @keydown="onPopKey"
            >
                <div class="flex items-center justify-between gap-1 px-2.5 pt-2.5 pb-1.5 group-data-[compact]/dp:pt-1.5 group-data-[compact]/dp:pb-1">
                    <button type="button" :aria-label="prevLabel" class="flex size-8 items-center justify-center rounded-lg text-zinc-500 outline-none hover:bg-zinc-100 hover:text-zinc-800 focus-visible:ring-2 focus-visible:ring-indigo-500 dark:text-zinc-400 dark:hover:bg-white/10 dark:hover:text-white" @click="step(-1)"><FluxIcon name="chevron-left" variant="mini" /></button>
                    <div class="flex items-center gap-0.5" aria-live="polite">
                        <span class="sr-only">{{ headerLabel }}</span>
                        <button
                            v-if="view === 'days'"
                            type="button"
                            aria-label="Choose month"
                            class="rounded-lg px-2 py-1 text-sm font-semibold outline-none hover:bg-zinc-100 focus-visible:ring-2 focus-visible:ring-indigo-500 dark:hover:bg-white/10"
                            @click="setView('months')"
                        >{{ MONTHS[viewMonth] }}</button>
                        <button
                            type="button"
                            :aria-label="view === 'years' ? 'Back to days' : 'Choose year'"
                            :class="['rounded-lg px-2 py-1 text-sm font-semibold tabular-nums outline-none hover:bg-zinc-100 focus-visible:ring-2 focus-visible:ring-indigo-500 dark:hover:bg-white/10', view === 'days' ? 'text-zinc-500 dark:text-zinc-400' : '']"
                            @click="setView('years')"
                        >{{ view === 'years' ? headerLabel : viewYear }}<FluxIcon name="chevron-down" variant="micro" :class="['ms-0.5 inline size-3.5 align-[-2px] text-zinc-400 transition-transform duration-100', view !== 'days' ? 'rotate-180' : '']" /></button>
                    </div>
                    <button type="button" :aria-label="nextLabel" class="flex size-8 items-center justify-center rounded-lg text-zinc-500 outline-none hover:bg-zinc-100 hover:text-zinc-800 focus-visible:ring-2 focus-visible:ring-indigo-500 dark:text-zinc-400 dark:hover:bg-white/10 dark:hover:text-white" @click="step(1)"><FluxIcon name="chevron-right" variant="mini" /></button>
                </div>

                <div ref="grid" class="px-2.5 pb-2">
                    <div v-if="view === 'days'" :key="`d-${viewYear}-${viewMonth}`" role="grid" :aria-label="headerLabel" class="flux-date-fade">
                        <div role="row" class="grid grid-cols-7 pb-1">
                            <span v-for="w in weekdays" :key="w" role="columnheader" :aria-label="w" class="flex h-7 items-center justify-center text-[11px] font-medium uppercase tracking-wide text-zinc-400 dark:text-zinc-500">{{ w.slice(0, 2) }}</span>
                        </div>
                        <div v-for="(week, i) in weeks" :key="i" role="row" class="grid grid-cols-7 justify-items-center">
                            <div v-for="d in week" :key="toKey(d)" role="gridcell" :aria-selected="sameDay(d, selected) ? 'true' : 'false'">
                                <button
                                    type="button"
                                    :tabindex="sameDay(d, cursor) ? 0 : -1"
                                    :data-date="toKey(d)"
                                    :aria-label="longDay(d)"
                                    :aria-current="sameDay(d, today) ? 'date' : undefined"
                                    :aria-disabled="inRange(d) ? undefined : 'true'"
                                    :class="dayClasses(d)"
                                    @click="pick(d)"
                                >{{ d.getDate() }}</button>
                            </div>
                        </div>
                    </div>
                    <div v-else-if="view === 'months'" :key="`m-${viewYear}`" role="grid" :aria-label="`Months of ${viewYear}`" class="flux-date-fade grid h-[16.25rem] group-data-[compact]/dp:h-[14rem] grid-cols-3 content-evenly gap-x-1.5">
                        <button
                            v-for="(m, i) in MONTHS"
                            :key="m"
                            type="button"
                            role="gridcell"
                            :tabindex="i === viewMonth ? 0 : -1"
                            :data-cursor="i === viewMonth ? '' : undefined"
                            :aria-label="`${m} ${viewYear}`"
                            :aria-selected="selected && selected.getFullYear() === viewYear && selected.getMonth() === i ? 'true' : 'false'"
                            :aria-disabled="monthOk(viewYear, i) ? undefined : 'true'"
                            :class="cellClasses(selected && selected.getFullYear() === viewYear && selected.getMonth() === i, today.getFullYear() === viewYear && today.getMonth() === i, monthOk(viewYear, i))"
                            @click="monthOk(viewYear, i) && chooseMonth(i)"
                        >{{ m.slice(0, 3) }}</button>
                    </div>
                    <div v-else :key="`y-${yearStart}`" role="grid" :aria-label="headerLabel" class="flux-date-fade grid h-[16.25rem] group-data-[compact]/dp:h-[14rem] grid-cols-4 content-evenly gap-x-1.5">
                        <button
                            v-for="y in years"
                            :key="y"
                            type="button"
                            role="gridcell"
                            :tabindex="y === viewYear ? 0 : -1"
                            :data-cursor="y === viewYear ? '' : undefined"
                            :aria-label="String(y)"
                            :aria-selected="selected && selected.getFullYear() === y ? 'true' : 'false'"
                            :aria-disabled="yearOk(y) ? undefined : 'true'"
                            :class="[cellClasses(selected && selected.getFullYear() === y, today.getFullYear() === y, yearOk(y)), 'tabular-nums']"
                            @click="yearOk(y) && chooseYear(y)"
                        >{{ y }}</button>
                    </div>
                </div>

                <div class="grid grid-cols-3 gap-1.5 border-t border-zinc-100 bg-zinc-50/70 px-2.5 py-2.5 dark:border-white/5 dark:bg-black/10 group-data-[compact]/dp:py-1.5">
                    <button
                        v-for="q in quick"
                        :key="q.label"
                        type="button"
                        :disabled="!inRange(q.d)"
                        :aria-label="`${q.label}, ${longDay(q.d)}`"
                        :class="['h-7 group-data-[compact]/dp:h-6 rounded-md px-2 text-xs font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-indigo-500 disabled:cursor-not-allowed disabled:opacity-40', sameDay(q.d, selected) ? 'bg-indigo-50 text-indigo-700 ring-1 ring-inset ring-indigo-200 dark:bg-indigo-500/15 dark:text-indigo-200 dark:ring-indigo-400/30' : 'bg-white text-zinc-700 ring-1 ring-inset ring-zinc-200 hover:bg-zinc-100 dark:bg-white/5 dark:text-zinc-200 dark:ring-white/10 dark:hover:bg-white/10']"
                        @click="pick(q.d)"
                    >{{ q.label }}</button>
                    <button
                        v-if="!required"
                        type="button"
                        :disabled="!hasValue"
                        class="flex h-7 group-data-[compact]/dp:h-6 items-center justify-center gap-1 rounded-md px-2 text-xs font-medium text-zinc-500 outline-none transition-colors hover:bg-zinc-100 hover:text-zinc-800 focus-visible:ring-2 focus-visible:ring-indigo-500 disabled:cursor-not-allowed disabled:opacity-40 dark:text-zinc-400 dark:hover:bg-white/10 dark:hover:text-white"
                        @click="clear"
                    ><FluxIcon name="x-mark" variant="micro" class="size-3.5" />Clear</button>
                </div>
            </div>
        </div>
    </FluxWithField>
</template>

<style>
[data-flux-date-popover]:popover-open { animation: flux-date-in 120ms cubic-bezier(.2, .7, .2, 1); }
@keyframes flux-date-in { from { opacity: 0; transform: translateY(-3px) scale(.985); } to { opacity: 1; transform: none; } }
.flux-date-fade { animation: flux-date-fade 100ms ease-out; }
@keyframes flux-date-fade { from { opacity: .35; } to { opacity: 1; } }
.flux-date-shake { animation: flux-date-shake 150ms ease-in-out; }
@keyframes flux-date-shake { 0%, 100% { transform: none; } 25% { transform: translateX(-3px); } 75% { transform: translateX(3px); } }
@media (prefers-reduced-motion: reduce) {
    [data-flux-date-popover]:popover-open, .flux-date-fade, .flux-date-shake { animation: none; }
}
</style>
