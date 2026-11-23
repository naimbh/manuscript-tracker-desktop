<script setup>
import { nextTick, onBeforeUnmount, ref } from 'vue';
import { AvatarCropper, MAX_ZOOM, MIN_ZOOM } from '@/lib/avatarCropper.js';

/**
 * Crop dialog for the profile photo. `open(img)` shows it with a decoded image; `save` is emitted with the
 * cropped 512 × 512 Blob. The parent closes it (`close()`) once the photo is saved. Escape or Cancel closes it.
 */
defineProps({ busy: { type: Boolean, default: false }, error: { type: String, default: '' } });
const emit = defineEmits(['save', 'closed']);

const show = ref(false);
const canvas = ref(null);
const zoom = ref(1);
let cropper = null;

const frame = () => new Promise((r) => requestAnimationFrame(() => r()));

async function open(img) {
  zoom.value = 1;
  show.value = true;
  await nextTick();
  await nextTick();
  await frame();
  cropper?.destroy();
  cropper = new AvatarCropper(canvas.value, { onChange: (s) => { zoom.value = s.zoom; } });
  cropper.setImage(img);
  canvas.value?.focus({ preventScroll: true });
}

function close() {
  show.value = false;
}

function onClosed() {
  cropper?.destroy();
  cropper = null;
  emit('closed');
}

async function save() {
  if (!cropper) return;
  emit('save', await cropper.export());
}

onBeforeUnmount(() => cropper?.destroy());
defineExpose({ open, close });
</script>

<template>
    <FluxModal v-model:show="show" :dismissible="false" class="w-full max-w-[26rem]" @close="onClosed">
        <div class="space-y-5">
            <div class="pr-8">
                <FluxHeading id="avatar-crop-title" size="lg">Crop your photo</FluxHeading>
                <FluxText class="mt-1">Drag to move it. Scroll, pinch or use the slider to zoom.</FluxText>
            </div>

            <div class="relative mx-auto aspect-square w-full max-w-[22rem] overflow-hidden rounded-xl bg-zinc-100 ring-1 ring-zinc-200 dark:bg-zinc-900 dark:ring-zinc-700">
                <canvas
                    ref="canvas"
                    tabindex="0"
                    role="img"
                    aria-label="Photo crop area"
                    aria-describedby="avatar-crop-keys"
                    class="block size-full cursor-grab touch-none outline-none select-none focus-visible:ring-2 focus-visible:ring-indigo-500 focus-visible:ring-inset active:cursor-grabbing"
                ></canvas>
            </div>
            <p id="avatar-crop-keys" class="sr-only">Use the arrow keys to move the photo (hold Shift for bigger steps), and plus or minus to zoom.</p>

            <div class="flex items-center gap-2">
                <FluxButton size="sm" variant="ghost" square icon="minus" aria-label="Zoom out" :disabled="zoom <= MIN_ZOOM" @click="cropper?.zoomBy(-0.25)" />
                <label class="sr-only" for="avatar-crop-zoom">Zoom</label>
                <input
                    id="avatar-crop-zoom"
                    type="range"
                    :min="MIN_ZOOM"
                    :max="MAX_ZOOM"
                    step="0.01"
                    :value="zoom"
                    :aria-valuetext="`${zoom.toFixed(1)}×`"
                    class="h-1.5 flex-1 cursor-pointer accent-indigo-600 dark:accent-indigo-400"
                    @input="cropper?.setZoom($event.target.valueAsNumber)"
                />
                <FluxButton size="sm" variant="ghost" square icon="plus" aria-label="Zoom in" :disabled="zoom >= MAX_ZOOM" @click="cropper?.zoomBy(0.25)" />
                <span class="w-10 text-right text-xs text-zinc-500 tabular-nums dark:text-zinc-400" aria-hidden="true">{{ zoom.toFixed(1) }}×</span>
            </div>

            <div class="flex flex-wrap items-center gap-2">
                <FluxButton size="sm" variant="subtle" icon="arrow-path" @click="cropper?.rotate()">Rotate 90°</FluxButton>
                <FluxButton size="sm" variant="subtle" icon="arrow-uturn-left" @click="cropper?.reset()">Reset</FluxButton>
            </div>

            <FluxError v-if="error" :message="error" />

            <div class="flex items-center gap-2">
                <FluxSpacer />
                <FluxButton variant="ghost" :disabled="busy" @click="close">Cancel</FluxButton>
                <FluxButton variant="primary" :loading="busy" :disabled="busy" @click="save">Save</FluxButton>
            </div>
        </div>
    </FluxModal>
</template>
