import ActionMessage from './ActionMessage.vue';
import AppLogo from './AppLogo.vue';
import AppLogoIcon from './AppLogoIcon.vue';
import PlaceholderPattern from './PlaceholderPattern.vue';
import SettingsLayout from './SettingsLayout.vue';
import UiAvatar from './UiAvatar.vue';
import UiCollapse from './UiCollapse.vue';
import UiCount from './UiCount.vue';
import UiDecision from './UiDecision.vue';
import UiEmpty from './UiEmpty.vue';
import UiFooter from './UiFooter.vue';
import UiList from './UiList.vue';
import UiMenu from './UiMenu.vue';
import UiMenuItem from './UiMenuItem.vue';
import UiPreprintEntry from './UiPreprintEntry.vue';
import UiRing from './UiRing.vue';
import UiStage from './UiStage.vue';
import UiTag from './UiTag.vue';

export const components = {
  ActionMessage, AppLogo, AppLogoIcon, PlaceholderPattern, SettingsLayout,
  UiAvatar, UiCollapse, UiCount, UiDecision, UiEmpty, UiFooter, UiList, UiMenu, UiMenuItem, UiPreprintEntry, UiRing, UiStage, UiTag,
};

export default {
  install(app) {
    for (const [name, c] of Object.entries(components)) app.component(name, c);
  },
};
