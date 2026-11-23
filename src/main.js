import { createApp } from 'vue';
// Inter variable font with the optical-size axis: small text uses the text cut, headings the display cut.
import '@fontsource-variable/inter/opsz.css';
import './css/app.css';
import './css/desktop.css';

import App from './App.vue';
import Flux from './flux/index.js';
import Ui from './components/ui/index.js';
import { initAppearance } from './lib/appearance.js';
import { installNativeBehaviour } from './lib/native.js';
import { applyPlatform } from './lib/platform.js';
import { route } from './lib/router.js';

applyPlatform();
initAppearance();
installNativeBehaviour();

const app = createApp(App);
app.use(Flux);
app.use(Ui);
app.config.globalProperties.route = route; // route('papers.index', 12) in templates
app.mount(document.body);
