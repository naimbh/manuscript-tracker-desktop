<script setup>
import { computed, useAttrs, useSlots } from 'vue';
import FluxIcon from './FluxIcon.vue';
import FluxTooltip from './FluxTooltip.vue';
import { applyInset, cx, hasContent } from './util.js';

defineOptions({ inheritAttrs: false });

const props = defineProps({
  variant: { type: String, default: 'outline' },
  size: { type: String, default: 'base' },
  type: { type: String, default: 'button' },
  align: { type: String, default: 'center' },
  square: { type: Boolean, default: undefined },
  icon: { type: String, default: null },
  iconLeading: { type: String, default: null },
  iconTrailing: { type: String, default: null },
  iconVariant: { type: String, default: null },
  // undefined: spinner markup on submit buttons (it shows while the button is :disabled, like Livewire's form submit);
  // true: spinner shown now (data-flux-loading); false: never.
  loading: { type: Boolean, default: undefined },
  color: { type: String, default: null },
  inset: { type: [String, Boolean], default: null },
  kbd: { type: String, default: null },
  href: { type: String, default: null },
  as: { type: String, default: null },
  tooltip: { type: String, default: null },
  tooltipPosition: { type: String, default: 'top' },
  tooltipKbd: { type: String, default: null },
});

const attrs = useAttrs();
const slots = useSlots();

const leading = computed(() => props.icon ?? props.iconLeading);
const isSquare = computed(() => props.square ?? (!hasContent(slots.default) && !(leading.value && props.iconTrailing)));
const iconVariant = computed(() => props.iconVariant ?? (props.size === 'xs' ? 'micro' : isSquare.value ? 'mini' : 'micro'));
const trailingVariant = computed(() => attrs['icon-trailing:variant'] ?? iconVariant.value);
const iconClasses = computed(() => cx(iconVariant.value === 'outline' ? (isSquare.value && props.size !== 'xs' ? 'size-5' : 'size-4') : '', attrs['icon:class']));
const trailingClasses = computed(() => cx(trailingVariant.value === 'outline' ? (isSquare.value && props.size !== 'xs' ? 'size-5' : 'size-4') : '', attrs['icon-trailing:class']));
const isSubmit = computed(() => props.type === 'submit');
const loading = computed(() => props.loading ?? isSubmit.value);
const GRAYS = ['slate', 'gray', 'zinc', 'neutral', 'stone', 'mauve', 'olive', 'mist', 'taupe'];
const isColored = computed(() => !!props.color && ['filled', 'outline', 'ghost', 'subtle'].includes(props.variant) && !GRAYS.includes(props.color));

const ACCENTS = {
  slate: '[--color-accent:var(--color-slate-800)] [--color-accent-content:var(--color-slate-800)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-white)] dark:[--color-accent-content:var(--color-white)] dark:[--color-accent-foreground:var(--color-slate-800)]',
  gray: '[--color-accent:var(--color-gray-800)] [--color-accent-content:var(--color-gray-800)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-white)] dark:[--color-accent-content:var(--color-white)] dark:[--color-accent-foreground:var(--color-gray-800)]',
  zinc: '[--color-accent:var(--color-zinc-800)] [--color-accent-content:var(--color-zinc-800)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-white)] dark:[--color-accent-content:var(--color-white)] dark:[--color-accent-foreground:var(--color-zinc-800)]',
  neutral: '[--color-accent:var(--color-neutral-800)] [--color-accent-content:var(--color-neutral-800)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-white)] dark:[--color-accent-content:var(--color-white)] dark:[--color-accent-foreground:var(--color-neutral-800)]',
  stone: '[--color-accent:var(--color-stone-800)] [--color-accent-content:var(--color-stone-800)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-white)] dark:[--color-accent-content:var(--color-white)] dark:[--color-accent-foreground:var(--color-stone-800)]',
  red: '[--color-accent:var(--color-red-500)] [--color-accent-content:var(--color-red-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-red-500)] dark:[--color-accent-content:var(--color-red-400)] dark:[--color-accent-foreground:var(--color-white)]',
  orange: '[--color-accent:var(--color-orange-500)] [--color-accent-content:var(--color-orange-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-orange-400)] dark:[--color-accent-content:var(--color-orange-400)] dark:[--color-accent-foreground:var(--color-orange-950)]',
  amber: '[--color-accent:var(--color-amber-400)] [--color-accent-content:var(--color-amber-600)] [--color-accent-foreground:var(--color-amber-950)] dark:[--color-accent:var(--color-amber-400)] dark:[--color-accent-content:var(--color-amber-400)] dark:[--color-accent-foreground:var(--color-amber-950)]',
  yellow: '[--color-accent:var(--color-yellow-400)] [--color-accent-content:var(--color-yellow-600)] [--color-accent-foreground:var(--color-yellow-950)] dark:[--color-accent:var(--color-yellow-400)] dark:[--color-accent-content:var(--color-yellow-400)] dark:[--color-accent-foreground:var(--color-yellow-950)]',
  lime: '[--color-accent:var(--color-lime-400)] [--color-accent-content:var(--color-lime-600)] [--color-accent-foreground:var(--color-lime-900)] dark:[--color-accent:var(--color-lime-400)] dark:[--color-accent-content:var(--color-lime-400)] dark:[--color-accent-foreground:var(--color-lime-950)]',
  green: '[--color-accent:var(--color-green-600)] [--color-accent-content:var(--color-green-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-green-600)] dark:[--color-accent-content:var(--color-green-400)] dark:[--color-accent-foreground:var(--color-white)]',
  emerald: '[--color-accent:var(--color-emerald-600)] [--color-accent-content:var(--color-emerald-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-emerald-600)] dark:[--color-accent-content:var(--color-emerald-400)] dark:[--color-accent-foreground:var(--color-white)]',
  teal: '[--color-accent:var(--color-teal-600)] [--color-accent-content:var(--color-teal-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-teal-600)] dark:[--color-accent-content:var(--color-teal-400)] dark:[--color-accent-foreground:var(--color-white)]',
  cyan: '[--color-accent:var(--color-cyan-600)] [--color-accent-content:var(--color-cyan-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-cyan-600)] dark:[--color-accent-content:var(--color-cyan-400)] dark:[--color-accent-foreground:var(--color-white)]',
  sky: '[--color-accent:var(--color-sky-600)] [--color-accent-content:var(--color-sky-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-sky-600)] dark:[--color-accent-content:var(--color-sky-400)] dark:[--color-accent-foreground:var(--color-white)]',
  blue: '[--color-accent:var(--color-blue-500)] [--color-accent-content:var(--color-blue-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-blue-500)] dark:[--color-accent-content:var(--color-blue-400)] dark:[--color-accent-foreground:var(--color-white)]',
  indigo: '[--color-accent:var(--color-indigo-500)] [--color-accent-content:var(--color-indigo-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-indigo-500)] dark:[--color-accent-content:var(--color-indigo-300)] dark:[--color-accent-foreground:var(--color-white)]',
  violet: '[--color-accent:var(--color-violet-500)] [--color-accent-content:var(--color-violet-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-violet-500)] dark:[--color-accent-content:var(--color-violet-400)] dark:[--color-accent-foreground:var(--color-white)]',
  purple: '[--color-accent:var(--color-purple-500)] [--color-accent-content:var(--color-purple-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-purple-500)] dark:[--color-accent-content:var(--color-purple-300)] dark:[--color-accent-foreground:var(--color-white)]',
  fuchsia: '[--color-accent:var(--color-fuchsia-600)] [--color-accent-content:var(--color-fuchsia-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-fuchsia-600)] dark:[--color-accent-content:var(--color-fuchsia-400)] dark:[--color-accent-foreground:var(--color-white)]',
  pink: '[--color-accent:var(--color-pink-600)] [--color-accent-content:var(--color-pink-600)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-pink-600)] dark:[--color-accent-content:var(--color-pink-400)] dark:[--color-accent-foreground:var(--color-white)]',
  rose: '[--color-accent:var(--color-rose-500)] [--color-accent-content:var(--color-rose-500)] [--color-accent-foreground:var(--color-white)] dark:[--color-accent:var(--color-rose-500)] dark:[--color-accent-content:var(--color-rose-400)] dark:[--color-accent-foreground:var(--color-white)]',
};

const TINTS = {
  red: '[--color-button-tint:var(--color-red-50)] [--color-button-soft-foreground:var(--color-red-200)] [--color-button-muted:var(--color-red-300)] [--color-button-soft:var(--color-red-400)] [--color-button-border:var(--color-red-500)] [--color-button-strong:var(--color-red-700)] [--color-button-deep:var(--color-red-950)]',
  orange: '[--color-button-tint:var(--color-orange-50)] [--color-button-soft-foreground:var(--color-orange-200)] [--color-button-muted:var(--color-orange-300)] [--color-button-soft:var(--color-orange-400)] [--color-button-border:var(--color-orange-500)] [--color-button-strong:var(--color-orange-700)] [--color-button-deep:var(--color-orange-950)]',
  amber: '[--color-button-tint:var(--color-amber-50)] [--color-button-soft-foreground:var(--color-amber-200)] [--color-button-muted:var(--color-amber-300)] [--color-button-soft:var(--color-amber-400)] [--color-button-border:var(--color-amber-500)] [--color-button-strong:var(--color-amber-700)] [--color-button-deep:var(--color-amber-950)]',
  yellow: '[--color-button-tint:var(--color-yellow-50)] [--color-button-soft-foreground:var(--color-yellow-200)] [--color-button-muted:var(--color-yellow-300)] [--color-button-soft:var(--color-yellow-400)] [--color-button-border:var(--color-yellow-500)] [--color-button-strong:var(--color-yellow-700)] [--color-button-deep:var(--color-yellow-950)]',
  lime: '[--color-button-tint:var(--color-lime-50)] [--color-button-soft-foreground:var(--color-lime-200)] [--color-button-muted:var(--color-lime-300)] [--color-button-soft:var(--color-lime-400)] [--color-button-border:var(--color-lime-500)] [--color-button-strong:var(--color-lime-700)] [--color-button-deep:var(--color-lime-950)]',
  green: '[--color-button-tint:var(--color-green-50)] [--color-button-soft-foreground:var(--color-green-200)] [--color-button-muted:var(--color-green-300)] [--color-button-soft:var(--color-green-400)] [--color-button-border:var(--color-green-500)] [--color-button-strong:var(--color-green-700)] [--color-button-deep:var(--color-green-950)]',
  emerald: '[--color-button-tint:var(--color-emerald-50)] [--color-button-soft-foreground:var(--color-emerald-200)] [--color-button-muted:var(--color-emerald-300)] [--color-button-soft:var(--color-emerald-400)] [--color-button-border:var(--color-emerald-500)] [--color-button-strong:var(--color-emerald-700)] [--color-button-deep:var(--color-emerald-950)]',
  teal: '[--color-button-tint:var(--color-teal-50)] [--color-button-soft-foreground:var(--color-teal-200)] [--color-button-muted:var(--color-teal-300)] [--color-button-soft:var(--color-teal-400)] [--color-button-border:var(--color-teal-500)] [--color-button-strong:var(--color-teal-700)] [--color-button-deep:var(--color-teal-950)]',
  cyan: '[--color-button-tint:var(--color-cyan-50)] [--color-button-soft-foreground:var(--color-cyan-200)] [--color-button-muted:var(--color-cyan-300)] [--color-button-soft:var(--color-cyan-400)] [--color-button-border:var(--color-cyan-500)] [--color-button-strong:var(--color-cyan-700)] [--color-button-deep:var(--color-cyan-950)]',
  sky: '[--color-button-tint:var(--color-sky-50)] [--color-button-soft-foreground:var(--color-sky-200)] [--color-button-muted:var(--color-sky-300)] [--color-button-soft:var(--color-sky-400)] [--color-button-border:var(--color-sky-500)] [--color-button-strong:var(--color-sky-700)] [--color-button-deep:var(--color-sky-950)]',
  blue: '[--color-button-tint:var(--color-blue-50)] [--color-button-soft-foreground:var(--color-blue-200)] [--color-button-muted:var(--color-blue-300)] [--color-button-soft:var(--color-blue-400)] [--color-button-border:var(--color-blue-500)] [--color-button-strong:var(--color-blue-700)] [--color-button-deep:var(--color-blue-950)]',
  indigo: '[--color-button-tint:var(--color-indigo-50)] [--color-button-soft-foreground:var(--color-indigo-200)] [--color-button-muted:var(--color-indigo-300)] [--color-button-soft:var(--color-indigo-400)] [--color-button-border:var(--color-indigo-500)] [--color-button-strong:var(--color-indigo-700)] [--color-button-deep:var(--color-indigo-950)]',
  violet: '[--color-button-tint:var(--color-violet-50)] [--color-button-soft-foreground:var(--color-violet-200)] [--color-button-muted:var(--color-violet-300)] [--color-button-soft:var(--color-violet-400)] [--color-button-border:var(--color-violet-500)] [--color-button-strong:var(--color-violet-700)] [--color-button-deep:var(--color-violet-950)]',
  purple: '[--color-button-tint:var(--color-purple-50)] [--color-button-soft-foreground:var(--color-purple-200)] [--color-button-muted:var(--color-purple-300)] [--color-button-soft:var(--color-purple-400)] [--color-button-border:var(--color-purple-500)] [--color-button-strong:var(--color-purple-700)] [--color-button-deep:var(--color-purple-950)]',
  fuchsia: '[--color-button-tint:var(--color-fuchsia-50)] [--color-button-soft-foreground:var(--color-fuchsia-200)] [--color-button-muted:var(--color-fuchsia-300)] [--color-button-soft:var(--color-fuchsia-400)] [--color-button-border:var(--color-fuchsia-500)] [--color-button-strong:var(--color-fuchsia-700)] [--color-button-deep:var(--color-fuchsia-950)]',
  pink: '[--color-button-tint:var(--color-pink-50)] [--color-button-soft-foreground:var(--color-pink-200)] [--color-button-muted:var(--color-pink-300)] [--color-button-soft:var(--color-pink-400)] [--color-button-border:var(--color-pink-500)] [--color-button-strong:var(--color-pink-700)] [--color-button-deep:var(--color-pink-950)]',
  rose: '[--color-button-tint:var(--color-rose-50)] [--color-button-soft-foreground:var(--color-rose-200)] [--color-button-muted:var(--color-rose-300)] [--color-button-soft:var(--color-rose-400)] [--color-button-border:var(--color-rose-500)] [--color-button-strong:var(--color-rose-700)] [--color-button-deep:var(--color-rose-950)]',
};

const classes = computed(() => {
  const v = props.variant;
  const sq = isSquare.value;
  const lead = !!leading.value;
  const trail = !!props.iconTrailing;
  const col = isColored.value;
  const size = props.size;
  const sizes = {
    base: 'h-10 text-sm rounded-lg gap-2 ' + (sq ? 'w-10' : (lead ? 'ps-3' : 'ps-4') + ' ' + (trail ? 'pe-3' : 'pe-4')),
    sm: 'h-8 text-sm rounded-md gap-2 ' + (sq ? 'w-8' : (lead ? 'ps-2' : 'ps-3') + ' ' + (trail ? 'pe-2' : 'pe-3')),
    xs: 'h-6 text-xs rounded-md gap-1 ' + (sq ? 'w-6' : (lead ? 'ps-1' : 'ps-2') + ' ' + (trail ? 'pe-1' : 'pe-2')),
  };
  const insets = {
    base: sq ? { top: '-mt-2.5', right: '-me-2.5', bottom: '-mb-2.5', left: '-ms-2.5' } : { top: '-mt-2.5', right: trail ? '-me-3' : '-me-4', bottom: '-mb-3', left: lead ? '-ms-3' : '-ms-4' },
    sm: sq ? { top: '-mt-1.5', right: '-me-1.5', bottom: '-mb-1.5', left: '-ms-1.5' } : { top: '-mt-1.5', right: trail ? '-me-2' : '-me-3', bottom: '-mb-1.5', left: lead ? '-ms-2' : '-ms-3' },
    xs: sq ? { top: '-mt-1', right: '-me-1', bottom: '-mb-1', left: '-ms-1' } : { top: '-mt-1', right: trail ? '-me-1' : '-me-2', bottom: '-mb-1', left: lead ? '-ms-1' : '-ms-2' },
  };

  let bg;
  if (col && v === 'filled') bg = 'bg-[color-mix(in_oklab,_var(--color-button-soft),_transparent_80%)] hover:bg-[color-mix(in_oklab,_var(--color-button-soft),_transparent_70%)] dark:bg-[color-mix(in_oklab,_var(--color-button-soft),_transparent_60%)] dark:hover:bg-[color-mix(in_oklab,_var(--color-button-soft),_transparent_50%)]';
  else if (col && v === 'outline') bg = 'bg-white hover:bg-[color-mix(in_oklab,_var(--color-button-soft)_6%,_white)] dark:bg-zinc-700 dark:hover:bg-[color-mix(in_oklab,_var(--color-accent-content)_10%,_var(--color-zinc-700))]';
  else if (col && v === 'ghost') bg = 'bg-transparent hover:bg-[color-mix(in_oklab,_var(--color-button-soft),_transparent_80%)] active:bg-[color-mix(in_oklab,_var(--color-button-soft),_transparent_70%)] dark:hover:bg-[color-mix(in_oklab,_var(--color-button-soft),_transparent_60%)] dark:active:bg-[color-mix(in_oklab,_var(--color-button-soft),_transparent_50%)]';
  else if (col && v === 'subtle') bg = 'bg-transparent hover:bg-(--color-button-tint) dark:hover:bg-[color-mix(in_oklab,_var(--color-button-deep),_transparent_28%)]';
  else bg = {
    primary: 'bg-[var(--color-accent)] hover:bg-[color-mix(in_oklab,_var(--color-accent),_transparent_10%)]',
    filled: 'bg-zinc-800/5 hover:bg-zinc-800/10 dark:bg-white/10 dark:hover:bg-white/20',
    outline: 'bg-white hover:bg-zinc-50 dark:bg-zinc-700 dark:hover:bg-zinc-600/75',
    danger: 'bg-red-500 hover:bg-red-600 dark:bg-red-600 dark:hover:bg-red-500',
    ghost: 'bg-transparent hover:bg-zinc-800/5 dark:hover:bg-white/15 in-data-flux-sidebar:data-open:bg-zinc-800/5 dark:in-data-flux-sidebar:data-open:bg-white/15',
    subtle: 'bg-transparent hover:bg-zinc-800/5 dark:hover:bg-white/15 in-data-flux-sidebar:data-open:bg-zinc-800/5 dark:in-data-flux-sidebar:data-open:bg-white/15',
  }[v];

  let text;
  if (col && v === 'filled') text = 'text-(--color-button-strong) dark:text-(--color-button-soft-foreground)';
  else if (col && v === 'outline') text = 'text-(--color-button-strong) dark:text-(--color-accent-content)';
  else if (col && v === 'ghost') text = 'text-(--color-button-strong) dark:text-(--color-button-muted)';
  else if (col && v === 'subtle') text = 'text-[color-mix(in_oklab,_var(--color-accent-content),_var(--color-zinc-500)_46%)] hover:text-(--color-button-strong) dark:text-[color-mix(in_oklab,_var(--color-button-muted),_var(--color-zinc-400)_48%)] dark:hover:text-(--color-button-muted)';
  else text = {
    primary: 'text-[var(--color-accent-foreground)]',
    filled: 'text-zinc-800 dark:text-white',
    outline: 'text-zinc-800 dark:text-white',
    danger: 'text-white',
    ghost: 'text-zinc-800 dark:text-white',
    subtle: 'text-zinc-500 hover:text-zinc-800 dark:text-zinc-400 dark:hover:text-white in-data-flux-sidebar:data-open:text-zinc-800 dark:in-data-flux-sidebar:data-open:text-white',
  }[v];

  let border;
  if (col && v === 'outline') border = 'border border-[color-mix(in_oklab,_var(--color-button-border)_18%,_var(--color-zinc-200))] border-b-[color-mix(in_oklab,_color-mix(in_oklab,_var(--color-button-border)_26%,_var(--color-zinc-300))_80%,_transparent)] hover:border-[color-mix(in_oklab,_var(--color-button-border)_28%,_var(--color-zinc-200))] hover:border-b-[color-mix(in_oklab,_var(--color-button-border)_36%,_var(--color-zinc-200))] dark:border-[color-mix(in_oklab,_var(--color-accent-content)_22%,_var(--color-zinc-600))] dark:border-b-[color-mix(in_oklab,_var(--color-accent-content)_30%,_var(--color-zinc-600))] dark:hover:border-[color-mix(in_oklab,_var(--color-accent-content)_32%,_var(--color-zinc-600))] dark:hover:border-b-[color-mix(in_oklab,_var(--color-accent-content)_40%,_var(--color-zinc-600))]';
  else border = {
    primary: 'border border-black/10 dark:border-0',
    outline: 'border border-zinc-200 hover:border-zinc-200 disabled:border-zinc-200 border-b-zinc-300/80 dark:border-zinc-600 dark:hover:border-zinc-600 dark:disabled:border-zinc-600',
  }[v] || '';

  const shadow = {
    primary: 'shadow-[inset_0px_1px_--theme(--color-white/.2)]',
    danger: 'shadow-[inset_0px_1px_var(--color-red-500),inset_0px_2px_--theme(--color-white/.15)] dark:shadow-none',
    outline: { base: 'shadow-xs', sm: 'shadow-xs', xs: 'shadow-none' }[size],
  }[v] || '';

  const group = {
    ghost: '',
    subtle: '',
    outline: '[[data-flux-button-group]_&]:border-s-0 [:is([data-flux-button-group]>&:first-child,_[data-flux-button-group]_:first-child>&)]:border-s-[1px]',
    filled: '[[data-flux-button-group]_&]:border-e [:is([data-flux-button-group]>&:last-child,_[data-flux-button-group]_:last-child>&)]:border-e-0 [[data-flux-button-group]_&]:border-zinc-200/80 dark:[[data-flux-button-group]_&]:border-zinc-900/50',
    danger: '[[data-flux-button-group]_&]:border-e [:is([data-flux-button-group]>&:last-child,_[data-flux-button-group]_:last-child>&)]:border-e-0 [[data-flux-button-group]_&]:border-red-600 dark:[[data-flux-button-group]_&]:border-red-900/25',
    primary: '[[data-flux-button-group]_&]:border-e-0 [:is([data-flux-button-group]>&:last-child,_[data-flux-button-group]_:last-child>&)]:border-e-[1px] dark:[:is([data-flux-button-group]>&:last-child,_[data-flux-button-group]_:last-child>&)]:border-e-0 [:is([data-flux-button-group]>&:not(:first-child),_[data-flux-button-group]_:not(:first-child)>&)]:border-s-[1px]! [:is([data-flux-button-group]>&:not(:first-child),_[data-flux-button-group]_:not(:first-child)>&)]:border-s-[color-mix(in_srgb,var(--color-accent-foreground),transparent_85%)]',
  }[v];

  const sub = isSubmit.value;
  const load = loading.value ? [
    '*:transition-opacity',
    sub ? '[&[disabled]>:not([data-flux-loading-indicator])]:opacity-0' : '[&[data-loading]>:not([data-flux-loading-indicator])]:opacity-0 [&[data-flux-loading]>:not([data-flux-loading-indicator])]:opacity-0',
    sub ? '[&[disabled]>[data-flux-loading-indicator]]:opacity-100' : '[&[data-loading]>[data-flux-loading-indicator]]:opacity-100 [&[data-flux-loading]>[data-flux-loading-indicator]]:opacity-100',
    sub ? '[&[disabled]]:pointer-events-none' : 'data-loading:pointer-events-none data-flux-loading:pointer-events-none',
  ] : [];

  return cx(
    'relative items-center font-medium justify-center whitespace-nowrap',
    'disabled:opacity-50 dark:disabled:opacity-50 disabled:cursor-default disabled:pointer-events-none disabled:shadow-none',
    { start: 'justify-start', center: 'justify-center', end: 'justify-end' }[props.align],
    sizes[size],
    'inline-flex',
    props.inset ? applyInset(props.inset, insets[size]) : '',
    bg, text, border, shadow, group, load,
    (v === 'primary' || col) && props.color ? ACCENTS[props.color] : '',
    col ? TINTS[props.color] : '',
  );
});

const tag = computed(() => (props.as === 'div' && !props.href ? 'div' : props.as === 'a' || props.href ? 'a' : 'button'));

const bound = computed(() => {
  const rest = {};
  for (const [k, val] of Object.entries(attrs)) if (!k.includes(':')) rest[k] = val;
  const base = tag.value === 'a' ? { href: props.href } : tag.value === 'button' ? { type: props.type } : {};
  return {
    ...base,
    ...rest,
    class: [classes.value, attrs.class],
    'data-flux-button': '',
    'data-flux-button-square': isSquare.value ? '' : undefined,
    'data-flux-group-target': ['subtle', 'ghost'].includes(props.variant) ? undefined : '',
    'data-flux-loading': props.loading === true && !isSubmit.value ? '' : undefined,
    disabled: tag.value === 'button' ? (props.loading === true && isSubmit.value ? true : attrs.disabled) : undefined,
  };
});
</script>

<template>
    <FluxTooltip v-if="tooltip" class="inline-flex" :content="tooltip" :position="tooltipPosition" :kbd="tooltipKbd">
        <component :is="tag" v-bind="bound">
            <div v-if="loading" class="absolute inset-0 flex items-center justify-center opacity-0" data-flux-loading-indicator><FluxIcon icon="loading" :variant="iconVariant" :class="iconClasses" /></div>
            <FluxIcon v-if="leading" :icon="leading" :variant="iconVariant" :class="iconClasses" /><slot v-else name="icon" />
            <span v-if="(loading || leading || iconTrailing) && hasContent($slots.default)"><slot /></span><slot v-else />
            <div v-if="kbd" class="text-xs text-zinc-400 dark:text-zinc-400">{{ kbd }}</div>
            <FluxIcon v-if="iconTrailing" :icon="iconTrailing" :variant="trailingVariant" :class="trailingClasses" /><slot v-else name="iconTrailing" />
        </component>
    </FluxTooltip>
    <component :is="tag" v-else v-bind="bound">
        <div v-if="loading" class="absolute inset-0 flex items-center justify-center opacity-0" data-flux-loading-indicator><FluxIcon icon="loading" :variant="iconVariant" :class="iconClasses" /></div>
        <FluxIcon v-if="leading" :icon="leading" :variant="iconVariant" :class="iconClasses" /><slot v-else name="icon" />
        <span v-if="(loading || leading || iconTrailing) && hasContent($slots.default)"><slot /></span><slot v-else />
        <div v-if="kbd" class="text-xs text-zinc-400 dark:text-zinc-400">{{ kbd }}</div>
        <FluxIcon v-if="iconTrailing" :icon="iconTrailing" :variant="trailingVariant" :class="trailingClasses" /><slot v-else name="iconTrailing" />
    </component>
</template>
