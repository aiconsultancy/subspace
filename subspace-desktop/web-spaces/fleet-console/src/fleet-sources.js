// Where fleets come from, merged into one list, from three sources: the
// Settings page (the default fleet: `endpoint` and `token`), the fleets the
// Subspace API assigns to the person's initiatives (`subspace.fleets.get`,
// with the address the API records and the token saved for it in Subspace
// Settings › Fleets, given only for that address), and the fleets added in
// the page (localStorage plus `fleet_<id>` secrets).
//
// A fleet entry: {id, name|null, endpoint, origin: "settings"|"api"|"local",
// tokenKey, token|null, problem|null}. A null name means "use the name
// Stargate reports".

import { tokenKey, unpackToken } from "./fleet-store.js";

export const DEFAULT_FLEET_ID = "default";

/** The default fleet from Settings, or the reason it cannot be used. */
export function settingsFleet({ allowed, settings, secrets, capabilities, normalizeEndpoint }) {
  if (!allowed.settings) return { entry: null, problem: "no-settings" };
  const raw = typeof settings?.endpoint === "string" ? settings.endpoint.trim() : "";
  if (!raw) return { entry: null, problem: "no-endpoint" };
  const endpoint = normalizeEndpoint(raw);
  if (!endpoint.ok) return { entry: null, problem: "bad-endpoint" };
  if (!capabilities.secrets) return { entry: null, problem: "no-secrets-api" };
  if (!allowed.secrets) return { entry: null, problem: "no-secrets" };
  const token = typeof secrets?.token === "string" ? secrets.token.trim() : "";
  if (!token) return { entry: null, problem: "no-token" };
  return { entry: { id: DEFAULT_FLEET_ID, name: null, endpoint: endpoint.url, origin: "settings", tokenKey: "token", token, problem: null }, problem: null };
}

/**
 * Fleets added in the page, with their tokens when Subspace gives them. A
 * token is used only for the address it was saved for: one kept for another
 * address, or saved without its address, leaves the fleet without a token
 * until it is pasted again.
 */
export function localFleets({ list, secrets, allowed, normalizeEndpoint }) {
  return list.flatMap((item) => {
    const endpoint = normalizeEndpoint(item.endpoint);
    if (!endpoint.ok) return [];
    const key = tokenKey(item.id);
    const { token, problem } = allowed.secrets ? unpackToken(secrets?.[key], endpoint.url, normalizeEndpoint) : { token: null, problem: "no-token" };
    return [{ id: item.id, name: item.name, endpoint: endpoint.url, origin: "local", tokenKey: key, token, problem }];
  });
}

/**
 * Fleets from the Subspace API, as `subspace.fleets.get` gives them:
 * [{id, name, endpoint|null, token|null, problem|null}]. Subspace gives a
 * token only for the address it was saved for; a fleet without a usable
 * address is left out, and one without a token keeps Subspace's reason
 * ("no-token", "token-address", "token-checking" or "token-store").
 */
export function apiFleets(list = [], normalizeEndpoint) {
  if (!Array.isArray(list)) return [];
  return list.flatMap((item) => {
    if (!item || typeof item.id !== "string" || !item.id || typeof item.endpoint !== "string") return [];
    const endpoint = normalizeEndpoint(item.endpoint);
    if (!endpoint.ok) return [];
    const token = typeof item.token === "string" && item.token.trim() ? item.token.trim() : null;
    const problem = token ? null : typeof item.problem === "string" && item.problem ? item.problem : "no-token";
    const name = typeof item.name === "string" && item.name.trim() ? item.name.trim() : null;
    return [{ id: `api_${item.id}`, name, endpoint: endpoint.url, origin: "api", tokenKey: null, token, problem }];
  });
}

/**
 * One ordered list: the Settings fleet first, then the API's, then the
 * page's. The same address appears once, from the first source that has it.
 */
export function mergeFleets({ settings = null, api = [], local = [] }) {
  const seen = new Set();
  const ids = new Set();
  const merged = [];
  for (const entry of [settings, ...api, ...local]) {
    if (!entry || seen.has(entry.endpoint) || ids.has(entry.id)) continue;
    seen.add(entry.endpoint);
    ids.add(entry.id);
    merged.push(entry);
  }
  return merged;
}

/**
 * Decide the mode from everything Subspace allows. Live when at least one
 * fleet has an address and a token; otherwise sample mode, with the reason
 * the default fleet could not be used (what the banner explains). The
 * display settings (motion, frame-time readout) come back with it.
 */
export function resolveFleets({ present, allowed, capabilities, settings, secrets, list, api = [], normalizeEndpoint }) {
  if (!present) return { mode: "sample", reason: "no-bridge", motion: null, perfReadout: false, fleets: [], canStore: false };
  const motion = allowed.settings ? (settings?.motion === "Reduced" ? "Reduced" : "Full") : null;
  const perfReadout = allowed.settings && settings?.perfReadout === "On";
  const canStore = Boolean(capabilities.secretsSet && allowed.secrets && allowed.saveSecrets);
  const fromSettings = settingsFleet({ allowed, settings, secrets, capabilities, normalizeEndpoint });
  const fleets = mergeFleets({
    settings: fromSettings.entry,
    api: apiFleets(api, normalizeEndpoint),
    local: localFleets({ list, secrets, allowed, normalizeEndpoint }),
  });
  if (!fleets.some((entry) => entry.token)) {
    return { mode: "sample", reason: fromSettings.problem ?? "no-endpoint", motion, perfReadout, fleets: [], canStore };
  }
  return { mode: "live", reason: null, motion, perfReadout, fleets, canStore, defaultProblem: fromSettings.problem };
}

/** The name to show: the person's name for the fleet, else what Stargate reports, else its host. */
export function fleetName(entry, fleet) {
  if (entry?.name) return entry.name;
  if (fleet?.name) return fleet.name;
  try {
    return new URL(entry.endpoint).host;
  } catch {
    return "Fleet";
  }
}
