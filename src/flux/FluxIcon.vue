<script>
import { h } from 'vue';
import icons from './icons.js';

const SIZES = {
  outline: '[:where(&)]:size-6',
  solid: '[:where(&)]:size-6',
  mini: '[:where(&)]:size-5',
  micro: '[:where(&)]:size-4',
};

export default {
  name: 'FluxIcon',
  props: {
    name: { type: String, default: null },
    icon: { type: String, default: null },
    variant: { type: String, default: 'outline' },
  },
  render() {
    const name = this.name ?? this.icon;
    const variant = SIZES[this.variant] ? this.variant : 'outline';
    const set = icons[name];
    if (!set && import.meta.env.DEV) console.warn(`FluxIcon: unknown icon "${name}"`);
    const def = set ? set[variant] || set.outline || Object.values(set)[0] : { a: { viewBox: '0 0 24 24', fill: 'none' }, b: '' };
    const cls = ['shrink-0', SIZES[variant]];
    if (name === 'loading') cls.push('animate-spin');
    return h('svg', {
      class: cls.join(' '),
      'data-flux-icon': '',
      xmlns: 'http://www.w3.org/2000/svg',
      ...def.a,
      'aria-hidden': 'true',
      'data-slot': 'icon',
      innerHTML: def.b,
    });
  },
};
</script>
