<script setup>
/**
 * A keyed list whose items fade in, fade out and slide into place (≈180 ms) when they are added, removed or reordered.
 *   <UiList tag="div" class="space-y-2"><div v-for="x in rows" :key="x.id">…</div></UiList>
 * Leaving items are pinned where they were (absolute), so the rest move up smoothly. Set `:pin="false"` for
 * table rows (they just fade).
 */
defineProps({ tag: { type: String, default: 'div' }, pin: { type: Boolean, default: true } });

function pinEl(el) {
  const { offsetTop, offsetLeft, offsetWidth, offsetHeight } = el;
  Object.assign(el.style, { position: 'absolute', top: offsetTop + 'px', left: offsetLeft + 'px', width: offsetWidth + 'px', height: offsetHeight + 'px', margin: '0' });
}
</script>

<template>
    <TransitionGroup :tag="tag" name="list" :class="pin && 'relative'" @before-leave="(el) => pin && pinEl(el)">
        <slot />
    </TransitionGroup>
</template>
