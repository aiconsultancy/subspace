import { WeatherError } from "./errors.js";

const POSTCODE_API = "https://api.postcodes.io/postcodes/";

// Standard UK format without the space: GIR 0AA, or area+district then sector+unit.
const POSTCODE_PATTERN =
  /^(GIR0AA|[A-PR-UWYZ][A-HK-Y]?[0-9][0-9A-HJKPSTUW]?[0-9][ABD-HJLNP-UW-Z]{2})$/;

/**
 * Normalise user input to compact upper case, or return null when the text
 * cannot be a UK postcode.
 * @param {unknown} input
 * @returns {string | null}
 */
export function normalisePostcode(input) {
  if (typeof input !== "string") return null;
  const compact = input.replace(/\s+/g, "").toUpperCase();
  return POSTCODE_PATTERN.test(compact) ? compact : null;
}

/** "SW1A1AA" becomes "SW1A 1AA". */
export function formatPostcode(compact) {
  return `${compact.slice(0, -3)} ${compact.slice(-3)}`;
}

export function postcodeUrl(compact) {
  return `${POSTCODE_API}${encodeURIComponent(compact)}`;
}

function text(value) {
  return typeof value === "string" && value.trim() !== ""
    ? value.trim().slice(0, 80)
    : null;
}

/**
 * Validate a Postcodes.io lookup response and reduce it to what the app uses.
 * @param {unknown} json
 */
export function parsePostcodeResponse(json) {
  const result = json && typeof json === "object" ? json.result : null;
  if (!result || typeof result !== "object") {
    throw new WeatherError("bad-data");
  }
  const { latitude, longitude } = result;
  if (
    typeof latitude !== "number" ||
    typeof longitude !== "number" ||
    !Number.isFinite(latitude) ||
    !Number.isFinite(longitude)
  ) {
    throw new WeatherError("no-coordinates");
  }
  if (Math.abs(latitude) > 90 || Math.abs(longitude) > 180) {
    throw new WeatherError("bad-data");
  }
  const compact = normalisePostcode(result.postcode);
  if (!compact) throw new WeatherError("bad-data");

  const district = text(result.admin_district);
  const region = text(result.region) ?? text(result.country);
  const ward = text(result.admin_ward);
  return validateLocation({
    postcode: formatPostcode(compact),
    latitude,
    longitude,
    name: district ?? text(result.parish) ?? region ?? formatPostcode(compact),
    area: [ward, region].filter((part, i, all) => part && all.indexOf(part) === i),
    country: text(result.country),
  });
}

/**
 * Check a location object, including one read back from storage.
 * @returns the cleaned location
 * @throws WeatherError("bad-data")
 */
export function validateLocation(value) {
  if (!value || typeof value !== "object") throw new WeatherError("bad-data");
  const compact = normalisePostcode(value.postcode);
  const { latitude, longitude } = value;
  if (
    !compact ||
    !Number.isFinite(latitude) ||
    !Number.isFinite(longitude) ||
    Math.abs(latitude) > 90 ||
    Math.abs(longitude) > 180 ||
    typeof value.name !== "string" ||
    !Array.isArray(value.area)
  ) {
    throw new WeatherError("bad-data");
  }
  return {
    postcode: formatPostcode(compact),
    latitude,
    longitude,
    name: value.name.slice(0, 80),
    area: value.area.filter((part) => typeof part === "string").map((part) => part.slice(0, 80)),
    country: typeof value.country === "string" ? value.country.slice(0, 80) : null,
  };
}

/** Stable cache key for a location's coordinates. */
export function locationKey(location) {
  return `${location.latitude.toFixed(4)},${location.longitude.toFixed(4)}`;
}
