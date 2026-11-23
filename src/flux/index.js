import FluxAvatar from './FluxAvatar.vue';
import FluxButton from './FluxButton.vue';
import FluxCallout from './FluxCallout.vue';
import FluxCalloutHeading from './FluxCalloutHeading.vue';
import FluxCalloutText from './FluxCalloutText.vue';
import FluxCheckbox from './FluxCheckbox.vue';
import FluxCommandPalette from './FluxCommandPalette.vue';
import FluxDescription from './FluxDescription.vue';
import FluxDropdown from './FluxDropdown.vue';
import FluxError from './FluxError.vue';
import FluxField from './FluxField.vue';
import FluxHeader from './FluxHeader.vue';
import FluxHeading from './FluxHeading.vue';
import FluxIcon from './FluxIcon.vue';
import FluxInput from './FluxInput.vue';
import FluxLabel from './FluxLabel.vue';
import FluxMain from './FluxMain.vue';
import FluxMenu from './FluxMenu.vue';
import FluxMenuGroup from './FluxMenuGroup.vue';
import FluxMenuHeading from './FluxMenuHeading.vue';
import FluxMenuItem from './FluxMenuItem.vue';
import FluxMenuRadio from './FluxMenuRadio.vue';
import FluxMenuRadioGroup from './FluxMenuRadioGroup.vue';
import FluxMenuSeparator from './FluxMenuSeparator.vue';
import FluxModal from './FluxModal.vue';
import FluxModalClose from './FluxModalClose.vue';
import FluxModalTrigger from './FluxModalTrigger.vue';
import FluxNavbar from './FluxNavbar.vue';
import FluxNavbarItem from './FluxNavbarItem.vue';
import FluxNavlist from './FluxNavlist.vue';
import FluxNavlistBadge from './FluxNavlistBadge.vue';
import FluxNavlistGroup from './FluxNavlistGroup.vue';
import FluxNavlistItem from './FluxNavlistItem.vue';
import FluxPagination from './FluxPagination.vue';
import FluxProfile from './FluxProfile.vue';
import FluxRadio from './FluxRadio.vue';
import FluxRadioGroup from './FluxRadioGroup.vue';
import FluxRadioIndicator from './FluxRadioIndicator.vue';
import FluxSelect from './FluxSelect.vue';
import FluxSelectOption from './FluxSelectOption.vue';
import FluxSeparator from './FluxSeparator.vue';
import FluxSidebar from './FluxSidebar.vue';
import FluxSidebarToggle from './FluxSidebarToggle.vue';
import FluxSpacer from './FluxSpacer.vue';
import FluxSubheading from './FluxSubheading.vue';
import FluxSwitch from './FluxSwitch.vue';
import FluxText from './FluxText.vue';
import FluxTextarea from './FluxTextarea.vue';
import FluxToast from './FluxToast.vue';
import FluxTooltip from './FluxTooltip.vue';
import FluxTooltipContent from './FluxTooltipContent.vue';

export const components = {
  FluxAvatar, FluxButton, FluxCallout, FluxCalloutHeading, FluxCalloutText, FluxCheckbox, FluxCommandPalette, FluxDescription, FluxDropdown,
  FluxError, FluxField, FluxHeader, FluxHeading, FluxIcon, FluxInput, FluxLabel, FluxMain, FluxMenu, FluxMenuGroup,
  FluxMenuHeading, FluxMenuItem, FluxMenuRadio, FluxMenuRadioGroup, FluxMenuSeparator, FluxModal, FluxModalClose,
  FluxModalTrigger, FluxNavbar, FluxNavbarItem, FluxNavlist, FluxNavlistBadge, FluxNavlistGroup, FluxNavlistItem,
  FluxPagination, FluxProfile, FluxRadio, FluxRadioGroup, FluxRadioIndicator, FluxSelect, FluxSelectOption, FluxSeparator, FluxSidebar,
  FluxSidebarToggle, FluxSpacer, FluxSubheading, FluxSwitch, FluxText, FluxTextarea, FluxToast, FluxTooltip, FluxTooltipContent,
};

export { provideErrors } from './errors.js';

export default {
  install(app) {
    for (const [name, c] of Object.entries(components)) app.component(name, c);
  },
};
