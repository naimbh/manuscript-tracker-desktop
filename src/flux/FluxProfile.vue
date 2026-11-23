<script setup>
import { computed } from 'vue';
import FluxAvatar from './FluxAvatar.vue';
import FluxIcon from './FluxIcon.vue';

const props = defineProps({
  name: { type: String, default: null },
  initials: { type: String, default: null },
  avatar: { type: String, default: null },
  circle: { type: Boolean, default: false },
  chevron: { type: Boolean, default: true },
  iconTrailing: { type: String, default: null },
  iconVariant: { type: String, default: 'micro' },
});
const trailing = computed(() => props.iconTrailing ?? (props.chevron ? 'chevron-down' : null));
const iconClasses = computed(() => ['text-zinc-400 dark:text-white/80 group-hover:text-zinc-800 dark:group-hover:text-white', props.iconVariant === 'outline' ? 'size-4' : '']);
</script>

<template>
    <button type="button" class="group flex items-center rounded-lg has-data-[circle=true]:rounded-full [ui-dropdown>&]:w-full p-1 hover:bg-zinc-800/5 dark:hover:bg-white/15" data-flux-profile>
        <div class="shrink-0">
            <slot name="avatar"><FluxAvatar :src="avatar" size="sm" :circle="circle" :name="name" :initials="initials" /></slot>
        </div>
        <span v-if="name" class="mx-2 text-sm text-zinc-500 dark:text-white/80 group-hover:text-zinc-800 dark:group-hover:text-white font-medium truncate">{{ name }}</span>
        <div v-if="trailing" class="shrink-0 ms-auto size-8 flex justify-center items-center"><FluxIcon :icon="trailing" :variant="iconVariant" :class="iconClasses" /></div>
    </button>
</template>
