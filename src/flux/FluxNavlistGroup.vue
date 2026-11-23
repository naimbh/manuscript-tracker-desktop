<script setup>
import { ref } from 'vue';
import FluxIcon from './FluxIcon.vue';

// Matches the app's published resources/views/flux/navlist/group.blade.php.
const props = defineProps({
  expandable: { type: Boolean, default: false },
  expanded: { type: Boolean, default: true },
  heading: { type: String, default: null },
});
const open = ref(props.expanded);
</script>

<template>
    <ui-disclosure v-if="expandable && heading" class="group/disclosure" :open="open ? '' : undefined" data-flux-navlist-group>
        <button type="button" class="group/disclosure-button mb-[2px] flex h-10 w-full items-center rounded-lg text-zinc-500 hover:bg-zinc-800/5 hover:text-zinc-800 lg:h-8 dark:text-white/80 dark:hover:bg-white/[7%] dark:hover:text-white" :aria-expanded="open ? 'true' : 'false'" :data-open="open ? '' : undefined" @click="open = !open">
            <div class="pl-3 pr-4">
                <FluxIcon name="chevron-down" class="hidden size-3! group-data-open/disclosure-button:block" />
                <FluxIcon name="chevron-right" class="block size-3! group-data-open/disclosure-button:hidden" />
            </div>
            <span class="text-sm font-medium leading-none">{{ heading }}</span>
        </button>
        <div class="relative hidden space-y-[2px] pl-7 data-open:block" :data-open="open ? '' : undefined">
            <div class="absolute inset-y-[3px] left-0 ml-4 w-px bg-zinc-200 dark:bg-white/30"></div>
            <slot />
        </div>
    </ui-disclosure>
    <div v-else-if="heading" class="block space-y-[2px]">
        <div class="px-1 py-2">
            <div class="text-xs leading-none text-zinc-400">{{ heading }}</div>
        </div>
        <div><slot /></div>
    </div>
    <div v-else class="block space-y-[2px]"><slot /></div>
</template>
