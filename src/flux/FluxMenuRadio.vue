<script setup>
import { computed, inject, useSlots } from 'vue';
import FluxIcon from './FluxIcon.vue';
import { cx, hasContent } from './util.js';

const props = defineProps({
  value: { type: [String, Number, Boolean], default: undefined },
  checked: { type: Boolean, default: undefined },
  label: { type: String, default: null },
  suffix: { type: String, default: null },
  kbd: { type: String, default: null },
  iconTrailing: { type: String, default: null },
  iconVariant: { type: String, default: 'mini' },
  disabled: { type: Boolean, default: false },
});
const slots = useSlots();
const group = inject('fluxMenuRadioGroup', null);
const val = computed(() => props.value ?? (hasContent(slots.default) ? undefined : props.label));
const isChecked = computed(() => (group && group.value.value !== undefined && group.value.value !== null ? group.value.value == val.value : !!props.checked));
const iconClasses = computed(() => (props.iconVariant === 'outline' ? 'size-5' : ''));
const trailingClasses = computed(() => cx('ms-auto', props.iconVariant === 'outline' ? 'size-5' : null));
function pick() { if (!props.disabled) group?.select(val.value); }
</script>

<template>
    <ui-menu-radio
        class="group/menu-radio flex items-center px-2 py-1.5 w-full focus:outline-hidden rounded-md text-start text-sm font-medium [[disabled]_&]:opacity-50 [&[disabled]]:opacity-50 text-zinc-800 data-active:bg-zinc-50 dark:text-white dark:data-active:bg-zinc-600 **:data-flux-menu-item-icon:text-zinc-400 dark:**:data-flux-menu-item-icon:text-white/60 [&[data-active]_[data-flux-menu-item-icon]]:text-current"
        role="menuitemradio"
        :aria-checked="isChecked ? 'true' : 'false'"
        :checked="isChecked ? 'checked' : undefined"
        :data-checked="isChecked ? '' : undefined"
        :disabled="disabled ? '' : undefined"
        data-flux-menu-item-has-icon
        data-flux-menu-radio
        @click="pick"
    >
        <div class="w-7">
            <div class="hidden group-data-checked/menu-radio:block"><FluxIcon :variant="iconVariant" icon="check" :class="iconClasses" data-flux-menu-item-icon /></div>
        </div>
        <slot v-if="hasContent(slots.default)" /><template v-else>{{ label }}</template>
        <div v-if="kbd || suffix" class="ms-auto opacity-50 text-xs">{{ kbd || suffix }}</div>
        <FluxIcon v-if="iconTrailing" :icon="iconTrailing" :variant="iconVariant" :class="trailingClasses" data-flux-menu-item-icon />
    </ui-menu-radio>
</template>
