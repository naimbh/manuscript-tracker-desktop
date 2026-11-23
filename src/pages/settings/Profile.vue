<script setup>
import { onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { api, submit } from '@/lib/api.js';
import { bus } from '@/lib/bus.js';
import { confirmDialog, dropTarget, onFileDrop } from '@/lib/native.js';
import { toast } from '@/lib/toast.js';
import { user } from '@/lib/site.js';
import { provideErrors } from '@/flux/errors.js';
import { setAvatar } from '@/components/shell/shell.js';
import { blobToDataUrl, loadImage, MESSAGES } from '@/lib/avatarCropper.js';
import AvatarCropDialog from './AvatarCropDialog.vue';

const form = reactive({ name: '', email: '' });
const errors = ref({});
provideErrors(errors);
const busy = ref(false);
const erasing = ref(false);

// ---- Profile photo ----
const crop = ref(null);
const photoError = ref('');
const cropError = ref('');
const photoBusy = ref(false);
const saving = ref(false);
const dragOver = ref(false);

/** Open the crop dialog with a source image `{ data }` from the backend (picked or dropped). */
async function openCrop(source) {
  if (!source) return;
  let img;
  try {
    img = await loadImage(source.data);
  } catch {
    photoError.value = MESSAGES.decode;
    return;
  }
  cropError.value = '';
  await crop.value.open(img);
}

async function readPhoto(action, args = {}) {
  photoError.value = '';
  photoBusy.value = true;
  try {
    await openCrop(await api(action, args));
  } catch (e) {
    photoError.value = e.errors?.photo?.[0] || e.message;
  } finally {
    photoBusy.value = false;
  }
}

const pickPhoto = () => readPhoto('settings.pickAvatar');
// Back to the upload button when the crop dialog closes (keyboard users keep their place).
const focusUpload = () => document.querySelector('[data-photo-upload]')?.focus({ preventScroll: true });

async function savePhoto(blob) {
  saving.value = true;
  cropError.value = '';
  try {
    const r = await api('settings.saveAvatar', { data: await blobToDataUrl(blob) });
    setAvatar(r.user?.avatar_path, r.url);
    crop.value.close();
    toast({ variant: 'success', text: 'Photo saved.' });
  } catch (e) {
    cropError.value = e.errors?.photo?.[0] || e.message;
  } finally {
    saving.value = false;
  }
}

async function removePhoto() {
  const ok = await confirmDialog('Your initials will be shown instead.', { title: 'Remove your photo?', okLabel: 'Remove', danger: true });
  if (!ok) return;
  photoError.value = '';
  try {
    await api('settings.removeAvatar');
    setAvatar(null, null);
    toast({ variant: 'success', text: 'Photo removed.' });
  } catch (e) {
    photoError.value = e.message;
  }
}

// Files dropped on the photo row (native drag and drop: the window receives the paths).
const overPhoto = (position) => !!dropTarget(position)?.closest?.('[data-avatar-drop]');
const offDrop = onFileDrop(({ type, position, files }) => {
  if (type === 'enter' || type === 'over') dragOver.value = overPhoto(position);
  else if (type === 'leave') dragOver.value = false;
  else if (type === 'drop') {
    const hit = dragOver.value || overPhoto(position);
    dragOver.value = false;
    if (!hit || !files.length || saving.value) return;
    if (files.length > 1) { photoError.value = 'Drop one image at a time.'; return; }
    readPhoto('settings.readAvatar', { token: files[0].token });
  }
});
onBeforeUnmount(offDrop);

onMounted(async () => {
    const p = await api('settings.profile');
    form.name = p.name ?? '';
    form.email = p.email ?? '';
});

async function updateProfileInformation() {
    busy.value = true;
    try {
        const u = await submit(errors, () => api('settings.saveProfile', form));
        if (!u) return;
        Object.assign(user, { name: u.name, email: u.email });
        bus.emit('profile-updated', u.name);
    } catch (e) {
        toast({ variant: 'danger', text: e.message });
    } finally {
        busy.value = false;
    }
}

async function eraseAll() {
    const ok = await confirmDialog(
        'This deletes your papers, submissions, journals and conferences, files, settings, saved API keys and local backups from this computer. It cannot be undone. Export a backup first if you may need your data again.',
        { title: 'Erase all data?', okLabel: 'Erase all data', danger: true },
    );
    if (!ok) return;
    erasing.value = true;
    try {
        await api('settings.eraseAll');
        try { window.localStorage.clear(); } catch {}
        window.location.hash = '#/welcome';
        window.location.reload();
    } catch (e) {
        erasing.value = false;
        toast({ variant: 'danger', text: e.message });
    }
}
</script>

<template>
    <section class="w-full">
        <div class="relative mx-auto mb-6 w-full max-w-6xl">
            <FluxHeading size="xl" :level="1" class="max-sm:!text-xl">Settings</FluxHeading>
            <FluxSubheading size="lg" class="mb-4 max-sm:!text-sm sm:mb-6">Manage your profile and account settings</FluxSubheading>
            <FluxSeparator variant="subtle" />
        </div>

        <SettingsLayout heading="Profile" subheading="Update your name and email address">
            <div class="mt-6" role="group" aria-labelledby="photo-label">
                <FluxLabel id="photo-label">Photo</FluxLabel>
                <div
                    data-avatar-drop
                    :class="['mt-2 -ml-2 flex items-center gap-4 rounded-2xl p-2 transition', dragOver ? 'bg-indigo-50 outline-2 outline-indigo-500 outline-dashed dark:bg-indigo-500/10' : '']"
                >
                    <button
                        type="button"
                        class="group relative shrink-0 rounded-full focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-500"
                        :aria-label="user.avatar ? 'Change photo' : 'Upload photo'"
                        :disabled="photoBusy"
                        @click="pickPhoto"
                    >
                        <UiAvatar :user="user" size="size-16" />
                        <span class="pointer-events-none absolute inset-[2px] flex items-center justify-center rounded-full bg-zinc-950/45 text-white opacity-0 transition group-hover:opacity-100 group-focus-visible:opacity-100">
                            <FluxIcon name="camera" variant="mini" class="size-5" />
                        </span>
                    </button>
                    <div class="min-w-0 space-y-2">
                        <div class="flex flex-wrap items-center gap-2">
                            <FluxButton data-photo-upload size="sm" icon="arrow-up-tray" :loading="photoBusy" :disabled="photoBusy" @click="pickPhoto">Upload photo…</FluxButton>
                            <FluxButton v-if="user.avatar" size="sm" variant="ghost" icon="trash" @click="removePhoto">Remove</FluxButton>
                        </div>
                        <FluxText size="sm">PNG, JPEG or WebP, up to 10 MB. You can also drop an image here.</FluxText>
                    </div>
                </div>
                <FluxError :message="photoError || undefined" />
            </div>

            <AvatarCropDialog ref="crop" :busy="saving" :error="cropError" @save="savePhoto" @closed="focusUpload" />

            <form class="my-6 w-full space-y-6" @submit.prevent="updateProfileInformation">
                <FluxInput v-model="form.name" label="Name" type="text" name="name" required autofocus autocomplete="name" />

                <div>
                    <FluxInput v-model="form.email" label="Email" type="email" name="email" autocomplete="email" description-trailing="Optional." />
                </div>

                <div class="flex items-center gap-4">
                    <div class="flex items-center justify-end">
                        <FluxButton variant="primary" type="submit" class="w-full" :disabled="busy">Save</FluxButton>
                    </div>

                    <ActionMessage class="me-3" on="profile-updated">Saved.</ActionMessage>
                </div>
            </form>

            <section class="mt-10 space-y-6">
                <div class="relative mb-5">
                    <FluxHeading>Erase all data</FluxHeading>
                    <FluxSubheading>Delete everything this app has stored on this computer and start again</FluxSubheading>
                </div>

                <FluxButton variant="danger" :loading="erasing" :disabled="erasing" @click="eraseAll">Erase all data</FluxButton>
            </section>
        </SettingsLayout>
    </section>
</template>
