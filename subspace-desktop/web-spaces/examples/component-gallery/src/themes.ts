import oneDark from './themes/one-dark.json';
import cupertinoDark from './themes/cupertino-dark.json';
import type { ThemeId } from './catalogue';

// Snapshot projection only. Runtime never reads the native project's folders.
const themes = { 'one-dark': oneDark, 'cupertino-dark': cupertinoDark };
const defaults = {
  '--row-font': '13px', '--mono-font': '13px', '--heading-font': '15px',
  '--pad-x': '12px', '--surface-radius': '6px',
};
// greybeard: opaque composites only for controls over this fixture's uniform
// muted panel. These preserve the committed native reference renderer's
// rounding, not every GPU's alpha blend. Re-measure before using another surface.
// GPUI Component 0.9.1: input_background() mixes input at 0.3 alpha;
// disabled input_style() mixes at 0.8, then Input applies opacity(0.5).
// Button defaults: success/warning 0.2, outlined danger 0.1 with 0.4 border,
// disabled primary 0.15. Only their fixture rest-state composites are stored.
const fixtureFills = {
  'one-dark': {
    '--fixture-input-fill': '#1e2026', '--fixture-disabled-input-fill': '#21242a',
    '--fixture-accept-fill': '#2f392e', '--fixture-caution-fill': '#3f382f',
    '--fixture-reject-fill': '#2a2024', '--fixture-reject-border': '#733e45',
    '--fixture-disabled-button-fill': '#25333d',
  },
  'cupertino-dark': {
    '--fixture-input-fill': '#373734', '--fixture-disabled-input-fill': '#393836',
    '--fixture-accept-fill': '#395041', '--fixture-caution-fill': '#534933',
    '--fixture-reject-fill': '#473835', '--fixture-reject-border': '#914d46',
    '--fixture-disabled-button-fill': '#36404f',
  },
};

export function applyTheme(id: ThemeId) {
  const theme = themes[id];
  const style = document.documentElement.style;
  for (const [key, value] of Object.entries(theme.colors)) {
    style.setProperty(`--${key.replaceAll('.', '-')}`, value);
  }
  for (const [key, value] of Object.entries({ ...defaults, ...theme.subspace.density })) {
    style.setProperty(key, value);
  }
  for (const [key, value] of Object.entries(fixtureFills[id])) {
    style.setProperty(key, value);
  }
  style.setProperty('--input-radius', `${theme.radius}px`);
  document.documentElement.dataset.theme = id;
}
