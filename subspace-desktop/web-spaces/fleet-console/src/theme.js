// Theme tones: Subspace's theme from the bridge (`subspace.theme()` and the
// `subspace:theme` event), mapped to CSS custom properties and to RGB floats
// for the shaders. Without the bridge, styles.css holds One Dark and the light
// values and follows prefers-color-scheme.

export const TONES = ["bg", "pane", "line", "hair", "select", "fg", "strong", "muted", "accent", "red", "orange", "yellow", "green", "cyan", "blue", "magenta"];
/** Tones the page derives: the stage behind the map and the panel shadow. */
export const DERIVED = ["stage"];

export const UI_FALLBACK = 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif';
export const MONO_FALLBACK = 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';

/** `#rgb`, `#rrggbb` or `#rrggbbaa` to [r, g, b] in 0..255; anything else is null. */
export function parseHex(text) {
  let s = String(text ?? "").trim().replace(/^#/, "");
  if (/^[0-9a-f]{3}$/i.test(s)) s = [...s].map((c) => c + c).join("");
  if (!/^[0-9a-f]{6}([0-9a-f]{2})?$/i.test(s)) return null;
  const n = parseInt(s.slice(0, 6), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** `rgb(1, 2, 3)` or a hex string to [r, g, b]; falls back to grey. */
export function parseColor(text) {
  const hex = parseHex(text);
  if (hex) return hex;
  const match = String(text ?? "").match(/rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i);
  if (match) return [Number(match[1]), Number(match[2]), Number(match[3])].map((v) => Math.round(Math.max(0, Math.min(255, v))));
  return [128, 128, 128];
}

const toHex = (rgb) => `#${rgb.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("")}`;
const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);

/** Relative luminance 0..1 of an sRGB colour, for contrast and light/dark decisions. */
export function luminance([r, g, b]) {
  const lin = (v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function contrastRatio(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** The stage colour: a little darker than the background in dark themes, lighter in light ones. */
export function deriveStage(bg, mode) {
  const rgb = parseColor(bg);
  return toHex(mode === "light" ? mix(rgb, [255, 255, 255], 0.5) : mix(rgb, [0, 0, 0], 0.3));
}

/** A CSS font-family value: the theme's family first, quoted when needed, then fallbacks. */
export function fontStack(family, fallback) {
  const name = String(family ?? "").trim();
  if (!name || /[;{}<>]/.test(name)) return fallback;
  const first = /^["'].*["']$/.test(name) || name.includes(",") || /^[\w-]+$/.test(name) ? name : `"${name.replace(/"/g, "")}"`;
  return `${first}, ${fallback}`;
}

/** CSS custom properties for a bridge theme, or null when the theme is malformed. */
export function themeToCssVars(theme) {
  if (!theme || typeof theme !== "object" || !theme.tones) return null;
  const mode = theme.mode === "light" ? "light" : "dark";
  const vars = {};
  for (const tone of TONES) {
    const rgb = parseHex(theme.tones[tone]);
    if (!rgb) return null;
    vars[`--${tone}`] = toHex(rgb);
  }
  vars["--stage"] = deriveStage(vars["--bg"], mode);
  vars["--gshadow"] = mode === "light" ? "#00000033" : "#00000080";
  vars["--ui"] = fontStack(theme.fonts?.ui, UI_FALLBACK);
  vars["--mono"] = fontStack(theme.fonts?.mono, MONO_FALLBACK);
  return { mode, vars };
}

/**
 * Read the live tones from computed CSS, for the renderers.
 * `read(name)` returns a custom property's value.
 */
export function readPalette(read) {
  const rgb = {};
  for (const tone of [...TONES, ...DERIVED]) rgb[tone] = parseColor(read(`--${tone}`));
  const dark = luminance(rgb.bg) < 0.18;
  const ui = read("--ui").trim() || UI_FALLBACK;
  const mono = read("--mono").trim() || MONO_FALLBACK;
  return { rgb, dark, fonts: { ui, mono } };
}

/** [r, g, b] in 0..1 for a tone. */
export function toneFloat(palette, tone) {
  const rgb = palette.rgb[tone] ?? palette.rgb.muted ?? [128, 128, 128];
  return [rgb[0] / 255, rgb[1] / 255, rgb[2] / 255];
}
