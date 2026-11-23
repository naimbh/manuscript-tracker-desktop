<script setup>
function close(e) {
  const d = e.currentTarget.closest('dialog');
  if (d?.open) d.close();
}
</script>

<template>
    <ui-close data-flux-modal-close @click="close"><slot /></ui-close>
</template>
