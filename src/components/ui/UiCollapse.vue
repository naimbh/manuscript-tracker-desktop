<script setup>
/**
 * Opens and closes its content by height and opacity (≈180 ms) instead of popping in, so the page below slides
 * rather than jumps. Wrap a single v-if element: <UiCollapse><RunProgress v-if="batch" … /></UiCollapse>.
 */
const reduce = () => window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
const EASE = 'cubic-bezier(.2,.7,.2,1)';

function grow(el, done, open) {
  if (reduce()) return done();
  const h = el.scrollHeight;
  const style = getComputedStyle(el);
  const from = open ? { height: '0px', opacity: 0, marginTop: '0px', marginBottom: '0px', paddingTop: '0px', paddingBottom: '0px' }
    : { height: h + 'px', opacity: 1, marginTop: style.marginTop, marginBottom: style.marginBottom, paddingTop: style.paddingTop, paddingBottom: style.paddingBottom };
  const to = open ? { height: h + 'px', opacity: 1, marginTop: style.marginTop, marginBottom: style.marginBottom, paddingTop: style.paddingTop, paddingBottom: style.paddingBottom }
    : { height: '0px', opacity: 0, marginTop: '0px', marginBottom: '0px', paddingTop: '0px', paddingBottom: '0px' };
  el.style.overflow = 'hidden';
  const a = el.animate([from, to], { duration: 180, easing: EASE });
  a.onfinish = a.oncancel = () => { el.style.overflow = ''; done(); };
}
</script>

<template>
    <Transition :css="false" @enter="(el, done) => grow(el, done, true)" @leave="(el, done) => grow(el, done, false)">
        <slot />
    </Transition>
</template>
