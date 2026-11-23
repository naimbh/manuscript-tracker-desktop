import { reactive } from 'vue';

/**
 * App identity (fixed: not a setting) and the local user, shared by the layout and ui components.
 * The shell fills `user` from `app.boot` and after profile saves.
 */
export const site = Object.freeze({
  name: 'Manuscript Tracker',
  tagline: 'Offline · optional AI · open source',
  description: 'Track your manuscript submissions with ease. Fully offline, optional AI, free and open source.',
  creditName: 'Naim Bin Hasan',
  creditUrl: 'https://naimbh.com/dev',
});

/** Long-form tagline for the About screen and first-run screen. */
export const TAGLINE = 'Track your manuscript submissions with ease.';
/** Second line under the tagline (About and first-run screens). */
export const TAGLINE_2 = 'Fully offline, optional AI, free and open source.';

/** `avatar` is the profile photo as a `data:` URL (or null); the shell loads it when `avatar_path` changes. */
export const user = reactive({ id: 1, name: '', email: '', is_admin: true, avatar: null });
