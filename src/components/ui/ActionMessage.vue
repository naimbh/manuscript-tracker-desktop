<script setup>
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { bus } from '@/lib/bus.js';

// Shows "Saved." (or the slot) for 2 s whenever bus.emit(on) fires, then fades out over 1.5 s.
const props = defineProps({ on: { type: String, required: true } });
const shown = ref(false);
let timeout = null;
let off = null;
onMounted(() => { off = bus.on(props.on, () => { clearTimeout(timeout); shown.value = true; timeout = setTimeout(() => (shown.value = false), 2000); }); });
onBeforeUnmount(() => { off?.(); clearTimeout(timeout); });
</script>

<template>
    <Transition leave-active-class="transition-opacity duration-[1500ms]" leave-to-class="opacity-0">
        <div v-show="shown" class="text-sm text-gray-600"><slot>Saved.</slot></div>
    </Transition>
</template>
