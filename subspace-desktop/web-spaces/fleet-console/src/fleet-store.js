// The fleets a person adds in the page, kept in localStorage: only the
// non-secret part (id, name, address). Tokens live in Subspace's secrets
// under `fleet_<id>`. Every storage call is wrapped: a private window, full
// storage or blocked site data leaves the list empty rather than breaking the
// page.

export const STORE_KEY = "fleet-console.fleets";
export const STORE_VERSION = 1;
export const MAX_LOCAL_FLEETS = 24;
const ID_PATTERN = /^f[a-z0-9]{6,16}$/;

/** A new fleet id: `f` and ten base-36 characters, usable in a secret key. */
export function newFleetId(random = Math.random) {
  let id = "f";
  while (id.length < 11) id += Math.floor(random() * 36).toString(36);
  return id;
}

/** The Subspace secret key holding a local fleet's token. */
export const tokenKey = (id) => `fleet_${id}`;

/**
 * A page fleet's secret: its token together with the normalised address it
 * was saved for, so the token is never sent anywhere else.
 */
export function packToken(endpoint, token) {
  return JSON.stringify({ v: 1, endpoint, token });
}

/**
 * The token in a page fleet's secret, only when it was saved for `endpoint`
 * (both normalised with `normalize`). Returns {token, problem}: problem is
 * "no-token", "token-address" (saved for another address) or "token-unbound"
 * (saved without an address, by an earlier Fleet Console).
 */
export function unpackToken(value, endpoint, normalize = (url) => ({ ok: true, url })) {
  if (typeof value !== "string" || !value.trim()) return { token: null, problem: "no-token" };
  let data = null;
  try {
    data = JSON.parse(value);
  } catch {
    // A plain token, as saved before tokens carried their address.
  }
  if (!data || typeof data !== "object" || data.v !== 1 || typeof data.endpoint !== "string" || typeof data.token !== "string") return { token: null, problem: "token-unbound" };
  const bound = normalize(data.endpoint);
  if (!bound.ok || bound.url !== endpoint) return { token: null, problem: "token-address" };
  const token = data.token.trim();
  return token ? { token, problem: null } : { token: null, problem: "no-token" };
}

export function cleanName(name) {
  return String(name ?? "").replace(/[\u0000-\u001f\u007f]/g, "").trim().slice(0, 60);
}

function cleanEntry(entry) {
  if (!entry || typeof entry !== "object") return null;
  const id = typeof entry.id === "string" && ID_PATTERN.test(entry.id) ? entry.id : null;
  const name = cleanName(entry.name);
  const endpoint = typeof entry.endpoint === "string" ? entry.endpoint.trim() : "";
  if (!id || !name || !endpoint || endpoint.length > 2000) return null;
  return { id, name, endpoint };
}

/**
 * @param {Storage|null} storage  localStorage, or a fake
 * Returns {load, save, status}. `status` is "ok", "empty", "unavailable"
 * (storage throws), "corrupt" (unreadable data, replaced on the next save),
 * "newer" (a later version wrote it; never overwritten), or after a refused
 * save "full" (more than MAX_LOCAL_FLEETS) or "invalid" (an entry it cannot
 * keep). A save stores the whole list or nothing.
 */
export function createFleetStore(storage) {
  let status = "empty";

  function load() {
    let raw;
    try {
      raw = storage ? storage.getItem(STORE_KEY) : null;
    } catch {
      status = "unavailable";
      return [];
    }
    if (raw === null || raw === undefined) {
      status = storage ? "empty" : "unavailable";
      return [];
    }
    let data;
    try {
      data = JSON.parse(raw);
    } catch {
      status = "corrupt";
      return [];
    }
    if (!data || typeof data !== "object" || !Number.isInteger(data.version)) {
      status = "corrupt";
      return [];
    }
    if (data.version > STORE_VERSION) {
      status = "newer";
      return [];
    }
    if (!Array.isArray(data.fleets)) {
      status = "corrupt";
      return [];
    }
    const seen = new Set();
    const fleets = [];
    for (const entry of data.fleets) {
      const clean = cleanEntry(entry);
      if (!clean || seen.has(clean.id)) continue;
      seen.add(clean.id);
      fleets.push(clean);
      if (fleets.length >= MAX_LOCAL_FLEETS) break;
    }
    status = "ok";
    return fleets;
  }

  /** Save the whole list; returns true when stored. Refuses to clobber a newer version's data, or to drop entries. */
  function save(fleets) {
    if (status === "newer" || !storage) return false;
    if (fleets.length > MAX_LOCAL_FLEETS) {
      status = "full";
      return false;
    }
    const clean = fleets.map(cleanEntry).filter(Boolean);
    if (clean.length !== fleets.length || new Set(clean.map((entry) => entry.id)).size !== clean.length) {
      status = "invalid";
      return false;
    }
    try {
      storage.setItem(STORE_KEY, JSON.stringify({ version: STORE_VERSION, fleets: clean }));
      status = "ok";
      return true;
    } catch {
      status = "unavailable";
      return false;
    }
  }

  return {
    load,
    save,
    get status() {
      return status;
    },
  };
}

/** localStorage, or null where reading it throws. */
export function browserStorage() {
  try {
    const storage = globalThis.localStorage;
    storage?.getItem(STORE_KEY);
    return storage ?? null;
  } catch {
    return null;
  }
}
