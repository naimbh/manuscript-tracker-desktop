import { cx } from './util.js';

/** The size, surface, text and border classes Flux gives an input (shared by FluxInput and FluxDatePicker). */
export function inputClasses({ size = null, variant = 'outline' } = {}) {
  const filled = variant === 'filled';
  return cx(
    'w-full border block disabled:shadow-none dark:shadow-none',
    'appearance-none',
    size === 'sm' ? 'text-sm rounded-md py-1.5 h-8 leading-[1.125rem]'
      : size === 'xs' ? 'text-xs rounded-md py-1.5 h-6 leading-[1.125rem]'
        : 'text-base sm:text-sm rounded-lg py-2 h-10 leading-[1.375rem]',
    filled ? 'bg-zinc-800/5 dark:bg-white/10 dark:disabled:bg-white/[7%]' : 'bg-white dark:bg-white/10 dark:disabled:bg-white/[7%]',
    filled
      ? 'text-zinc-700 placeholder-zinc-500 disabled:placeholder-zinc-400 dark:text-zinc-200 dark:placeholder-white/60 dark:disabled:placeholder-white/40'
      : 'text-zinc-700 disabled:text-zinc-500 placeholder-zinc-400 disabled:placeholder-zinc-400/70 dark:text-zinc-300 dark:disabled:text-zinc-400 dark:placeholder-zinc-400 dark:disabled:placeholder-zinc-500',
    filled ? 'border-0' : 'shadow-xs border-zinc-200 border-b-zinc-300/80 disabled:border-b-zinc-200 dark:border-white/10 dark:disabled:border-white/5',
    filled ? 'data-invalid:border-red-500' : 'data-invalid:shadow-none data-invalid:border-red-500 dark:data-invalid:border-red-500 disabled:data-invalid:border-red-500 dark:disabled:data-invalid:border-red-500',
  );
}
