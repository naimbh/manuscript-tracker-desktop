<script setup>
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';

const props = defineProps({ value: { type: [Number, String], default: 0 } });
const n = ref(0);
let raf = null;

function run(from, to) {
  cancelAnimationFrame(raf);
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) { n.value = to; return; }
  const s = performance.now();
  const step = (now) => {
    const k = Math.min(1, (now - s) / 700);
    n.value = Math.round(from + (to - from) * (1 - Math.pow(1 - k, 3)));
    if (k < 1) raf = requestAnimationFrame(step);
  };
  raf = requestAnimationFrame(step);
}
const target = () => parseInt(props.value, 10) || 0;
onMounted(() => run(0, target()));
watch(() => props.value, () => run(n.value, target()));
onBeforeUnmount(() => cancelAnimationFrame(raf));
</script>

<template>
    <span class="num">{{ n }}</span>
</template>
