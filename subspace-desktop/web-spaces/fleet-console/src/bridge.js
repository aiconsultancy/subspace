// A careful wrapper around `window.subspace`. Every method is feature-detected
// (older Subspace builds lack theme, secrets and issues) and permission-checked
// against `info().permissions`, and failures resolve to null instead of
// throwing, so a missing capability degrades quietly.

const fn = (value) => typeof value === "function";

/**
 * @param {object} [host] the global object (tests pass a fake)
 */
export async function connectBridge(host = globalThis) {
  const api = host.subspace;
  if (!api || !fn(api.info)) return createAbsentBridge();
  let permissions = new Set();
  const refresh = async () => {
    try {
      const info = await api.info();
      permissions = new Set(Array.isArray(info?.permissions) ? info.permissions : []);
    } catch {
      // A failed info() is transport (a timed-out request, a busy app), not a
      // revocation: keep what Settings last allowed, so a hiccup does not
      // drop a live connection to sample data and back. The first call has
      // nothing to keep.
    }
    return permissions;
  };
  await refresh();
  const allowed = (permission) => permissions.has(permission);
  const attempt = async (call) => {
    try {
      return await call();
    } catch {
      return null;
    }
  };

  return {
    present: true,
    refresh,
    has: allowed,
    capabilities() {
      return {
        theme: fn(api.theme),
        secrets: fn(api.secrets?.get),
        secretsSet: fn(api.secrets?.set),
        issues: fn(api.issues?.scope) && fn(api.issues?.resolve),
      };
    },
    settings: () => (allowed("settings") && fn(api.settings?.get) ? attempt(() => api.settings.get()) : Promise.resolve(null)),
    secrets: () => (allowed("secrets") && fn(api.secrets?.get) ? attempt(() => api.secrets.get()) : Promise.resolve(null)),
    /** Keep (or, with null, delete) a page-owned secret. Rejects with Subspace's message. */
    setSecret: (key, value) =>
      allowed("secrets") && fn(api.secrets?.set) ? api.secrets.set(key, value) : Promise.reject(new Error("This version of Subspace cannot store secrets for the page.")),
    theme: () => (fn(api.theme) ? attempt(() => api.theme()) : Promise.resolve(null)),
    onTheme(listener) {
      const handler = (event) => listener(event.detail);
      host.addEventListener?.("subspace:theme", handler);
      return () => host.removeEventListener?.("subspace:theme", handler);
    },
    issues: {
      available: () => allowed("issues") && fn(api.issues?.scope) && fn(api.issues?.resolve),
      scope: () => (allowed("issues") && fn(api.issues?.scope) ? attempt(() => api.issues.scope()) : Promise.resolve(null)),
      resolve: (ids) => (allowed("issues") && fn(api.issues?.resolve) ? api.issues.resolve(ids) : Promise.reject(new Error("Issues are not allowed."))),
      open: (id) => (allowed("issues") && fn(api.issues?.open) ? attempt(() => api.issues.open(id)) : Promise.resolve(null)),
      canOpen: () => allowed("issues") && fn(api.issues?.open),
      onChanged(listener) {
        host.addEventListener?.("subspace:issues-changed", listener);
        return () => host.removeEventListener?.("subspace:issues-changed", listener);
      },
    },
    setBadge: (value) => (allowed("badge") && fn(api.setBadge) ? attempt(() => api.setBadge(value)) : Promise.resolve(null)),
    setContext: (value) => (allowed("context") && fn(api.setContext) ? attempt(() => api.setContext(value)) : Promise.resolve(null)),
    notify: (value) => (allowed("notifications") && fn(api.notify) ? api.notify(value) : Promise.resolve(null)),
  };
}

function createAbsentBridge() {
  const none = () => Promise.resolve(null);
  return {
    present: false,
    refresh: async () => new Set(),
    has: () => false,
    capabilities: () => ({ theme: false, secrets: false, secretsSet: false, issues: false }),
    settings: none,
    secrets: none,
    setSecret: () => Promise.reject(new Error("Fleets can be added only inside Subspace.")),
    theme: none,
    onTheme: () => () => {},
    issues: { available: () => false, scope: none, resolve: () => Promise.reject(new Error("No bridge.")), open: none, canOpen: () => false, onChanged: () => () => {} },
    setBadge: none,
    setContext: none,
    notify: none,
  };
}

/**
 * Decide between live and sample mode from what the bridge allows.
 * Returns {mode: "live", endpoint, token, motion} or {mode: "sample", reason, motion}.
 */
export async function resolveConfig(bridge, normalizeEndpoint) {
  if (!bridge.present) return { mode: "sample", reason: "no-bridge", motion: null };
  if (!bridge.has("settings")) return { mode: "sample", reason: "no-settings", motion: null };
  const settings = (await bridge.settings()) ?? {};
  const motion = settings.motion === "Reduced" ? "Reduced" : "Full";
  const raw = typeof settings.endpoint === "string" ? settings.endpoint.trim() : "";
  if (!raw) return { mode: "sample", reason: "no-endpoint", motion };
  const endpoint = normalizeEndpoint(raw);
  if (!endpoint.ok) return { mode: "sample", reason: "bad-endpoint", motion };
  if (!bridge.capabilities().secrets) return { mode: "sample", reason: "no-secrets-api", motion };
  if (!bridge.has("secrets")) return { mode: "sample", reason: "no-secrets", motion };
  const secrets = (await bridge.secrets()) ?? {};
  const token = typeof secrets.token === "string" ? secrets.token.trim() : "";
  if (!token) return { mode: "sample", reason: "no-token", motion };
  return { mode: "live", endpoint: endpoint.url, token, motion };
}
