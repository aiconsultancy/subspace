// Original condition icons. Each is a constant SVG string built only from
// literals, so the markup is safe to insert as HTML.

const SUN_RAYS = [0, 45, 90, 135, 180, 225, 270, 315]
  .map((angle) => `<line x1="24" y1="4" x2="24" y2="9" transform="rotate(${angle} 24 24)"/>`)
  .join("");

const sun = (x = 24, y = 24, scale = 1) =>
  `<g transform="translate(${x - 24 * scale} ${y - 24 * scale}) scale(${scale})">` +
  `<circle cx="24" cy="24" r="9" fill="#ffd45e"/>` +
  `<g stroke="#ffd45e" stroke-width="3" stroke-linecap="round">${SUN_RAYS}</g></g>`;

const moon = (x = 24, y = 24, scale = 1) =>
  `<path transform="translate(${x - 24 * scale} ${y - 24 * scale}) scale(${scale})" ` +
  `d="M30 8a17 17 0 1 0 11 29 14 14 0 0 1-11-29z" fill="#e9eeff"/>`;

const CLOUD_PATH = "M14 40a9 9 0 0 1-.6-18A12 12 0 0 1 36 25a7.5 7.5 0 0 1 .5 15z";
const cloud = (fill = "#f3f7fc", transform = "") =>
  `<path transform="${transform}" d="${CLOUD_PATH}" fill="${fill}"/>`;

const RAIN = [14, 23, 32]
  .map((x) => `<line x1="${x}" y1="40" x2="${x - 3}" y2="46"/>`)
  .join("");
const SNOW = [14, 24, 34]
  .map((x) => `<circle cx="${x}" cy="44" r="1.8"/>`)
  .join("");
const FOG = [30, 36, 42]
  .map((y, i) => `<line x1="${8 + i * 2}" y1="${y}" x2="${40 - i * 2}" y2="${y}"/>`)
  .join("");

const ICONS = {
  clearDay: sun(),
  clearNight: moon(),
  partlyDay: sun(17, 17, 0.7) + cloud("#f3f7fc", "translate(2 -2)"),
  partlyNight: moon(17, 17, 0.7) + cloud("#f3f7fc", "translate(2 -2)"),
  cloudy: cloud("#e1e8f1"),
  fog:
    cloud("#dde4ee", "translate(0 -8)") +
    `<g stroke="#dde4ee" stroke-width="3" stroke-linecap="round">${FOG}</g>`,
  rain:
    cloud("#cfd9e6", "translate(0 -6)") +
    `<g stroke="#7cc4ff" stroke-width="3" stroke-linecap="round" transform="translate(0 -2)">${RAIN}</g>`,
  snow:
    cloud("#e5ecf5", "translate(0 -6)") +
    `<g fill="#ffffff" transform="translate(0 -2)">${SNOW}</g>`,
  storm:
    cloud("#aab6c7", "translate(0 -6)") +
    `<path d="M26 32l-7 11h5l-2 7 9-12h-5z" fill="#ffd45e"/>`,
};

/**
 * SVG markup for a condition kind.
 * @param {string} kind clear | partly | cloudy | fog | rain | snow | storm
 * @param {boolean} isDay
 */
export function iconMarkup(kind, isDay) {
  let body;
  switch (kind) {
    case "clear":
      body = isDay ? ICONS.clearDay : ICONS.clearNight;
      break;
    case "partly":
      body = isDay ? ICONS.partlyDay : ICONS.partlyNight;
      break;
    default:
      body = ICONS[kind] ?? ICONS.cloudy;
  }
  return `<svg viewBox="0 0 48 48" aria-hidden="true" focusable="false">${body}</svg>`;
}
