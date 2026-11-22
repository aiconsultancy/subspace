// The connection state machine for the live event stream, and the words the
// window shows for each state. Pure: time and randomness are passed in.

export const BACKOFF_BASE_MS = 1000;
export const BACKOFF_CAP_MS = 30_000;
/** Failures after a working connection that still read as "reconnecting" rather than "unreachable". */
export const RECONNECT_GRACE = 5;

/**
 * Exponential backoff with equal jitter: half the delay is fixed, half random,
 * so many windows do not reconnect in step after the fleet VM restarts.
 */
export function backoffDelay(attempt, random = Math.random, base = BACKOFF_BASE_MS, cap = BACKOFF_CAP_MS) {
  const ceiling = Math.min(cap, base * 2 ** Math.max(0, attempt - 1));
  return Math.round(ceiling / 2 + random() * (ceiling / 2));
}

export function initialConnection() {
  return { status: "idle", reason: null, attempt: 0, connectedOnce: false, retryAt: null, httpStatus: null, since: 0 };
}

/**
 * Statuses: idle, sample, connecting, connected, reconnecting, signed-out (401),
 * forbidden (403), unreachable (network, 404, repeated failures).
 *
 * Events: {type:"sample", reason}, {type:"connecting"}, {type:"open"},
 * {type:"healthy"}, {type:"failed", status?, retryAfterMs?}, {type:"closed"},
 * {type:"retry"}, {type:"stop"}.
 *
 * `open` (a 200 with an event stream) keeps the attempt count; only
 * `healthy`, sent once a stream has stayed up a while, clears it. A server
 * that answers and closes at once therefore backs off like any failure.
 */
export function nextConnection(state, event, now, random = Math.random) {
  switch (event.type) {
    case "sample":
      return { ...initialConnection(), status: "sample", reason: event.reason ?? null, since: now };
    case "connecting":
      return { ...state, status: "connecting", retryAt: null, since: state.status === "connecting" ? state.since : now };
    case "open":
      return { ...state, status: "connected", connectedOnce: true, retryAt: null, httpStatus: null, reason: null, since: now };
    case "healthy":
      return state.status === "connected" ? { ...state, attempt: 0 } : state;
    case "failed":
      return failed(state, event, now, random);
    case "closed":
      return failed(state, { type: "failed" }, now, random);
    case "retry":
      if (["reconnecting", "unreachable", "signed-out", "forbidden"].includes(state.status)) {
        return { ...state, status: "connecting", retryAt: null, since: now };
      }
      return state;
    case "stop":
      return { ...initialConnection(), since: now };
    default:
      return state;
  }
}

function failed(state, event, now, random) {
  const status = Number.isInteger(event.status) ? event.status : null;
  if (status === 401) {
    return { ...state, status: "signed-out", httpStatus: 401, retryAt: null, reason: null, since: now };
  }
  if (status === 403) {
    return { ...state, status: "forbidden", httpStatus: 403, retryAt: null, reason: null, since: now };
  }
  const attempt = state.attempt + 1;
  const delay = Math.max(backoffDelay(attempt, random), Number(event.retryAfterMs) || 0);
  const transient = status === null || status === 429 || status >= 500;
  const reconnecting = transient && state.connectedOnce && attempt <= RECONNECT_GRACE;
  const next = reconnecting ? "reconnecting" : "unreachable";
  return {
    ...state,
    status: next,
    attempt,
    httpStatus: status,
    retryAt: now + delay,
    reason: event.reason ?? null,
    since: state.status === next ? state.since : now,
  };
}

const SUBSPACE_FLEETS = "Subspace Settings › Fleets";

/**
 * Why a fleet has no token to use, from its entry's `problem` and where it
 * came from (`origin`): a short status and what to do. A token saved for
 * another address is never sent.
 */
export function tokenProblem(problem, origin = "local") {
  if (origin === "api") return apiTokenProblem(problem);
  if (problem === "token-address") return { short: "Token for another address", title: "Its token was saved for another address", body: "Edit the fleet and paste the token for this address." };
  if (problem === "token-unbound") return { short: "Paste the token again", title: "Its token needs pasting again", body: "Fleet Console now keeps each token with its address. Edit the fleet and paste its token once more." };
  return { short: "No token saved", title: "No token saved", body: "Edit the fleet in the inspector and paste its token again." };
}

/** The same for a fleet from the Subspace API, whose token is set in Subspace. */
function apiTokenProblem(problem) {
  if (problem === "token-address") return { short: "Address changed", title: "Its address changed since its token was saved", body: `Check the new address, then paste the fleet's token again in ${SUBSPACE_FLEETS}.` };
  if (problem === "token-checking") return { short: "Reading token", title: "Subspace is reading its saved token", body: "Fleet Console connects as soon as it has it." };
  if (problem === "token-store") return { short: "Token not read", title: "Subspace could not read its saved token", body: "Unlock your system keychain, then open Fleet Console again." };
  return { short: "No token saved", title: "No token saved", body: `Paste the fleet's token in ${SUBSPACE_FLEETS}.` };
}

/** True while the window should keep showing that the data may be old. */
export function isStale(state) {
  return ["reconnecting", "unreachable", "signed-out", "forbidden", "connecting"].includes(state.status);
}

const SETTINGS_PATH = "Settings > Spaces > Fleet Console";

const SAMPLE_REASONS = {
  "no-bridge": {
    title: "Sample fleets",
    body: "This page is not running inside Subspace, so it shows simulated fleets. Install it as a web Space and set the Stargate API address and token to see your own.",
  },
  "no-settings": {
    title: "Sample fleet: settings are not allowed",
    body: `Allow this app to read its settings in ${SETTINGS_PATH}, save, then choose Retry.`,
  },
  "no-endpoint": {
    title: "Sample fleet: no Stargate address",
    body: `Add the fleet's Stargate API address in ${SETTINGS_PATH}: an HTTPS address on your private network, or http://localhost:<port> through an SSH tunnel.`,
  },
  "bad-endpoint": {
    title: "Sample fleet: this address cannot be reached from a web Space",
    body: "Use an HTTPS address (Tailscale Serve, or a Cloudflare Tunnel as the README describes) or http://localhost:<port> through an SSH tunnel. Plain HTTP works only to localhost.",
  },
  "no-secrets": {
    title: "Sample fleet: the token is not allowed",
    body: `Allow this app to read its saved secrets in ${SETTINGS_PATH}, save, then choose Retry.`,
  },
  "no-secrets-api": {
    title: "Sample fleet: this Subspace cannot pass the token",
    body: "This version of Subspace does not give web Spaces their saved secrets. Update Subspace, then choose Retry.",
  },
  "no-token": {
    title: "Sample fleet: no API token",
    body: `Run stargate api token on the fleet VM and save it as the API token in ${SETTINGS_PATH}, then choose Retry.`,
  },
  // A fleet the Subspace API assigns, whose token is set in Subspace.
  "api-no-token": {
    title: "Sample fleet: your fleet's token is not saved",
    body: `Paste the fleet's Fleet API token in ${SUBSPACE_FLEETS}. Fleet Console connects as soon as it is saved.`,
  },
  "api-token-address": {
    title: "Sample fleet: your fleet's address changed",
    body: `Its token was saved for another address and is not sent to the new one. Check the new address, then paste the fleet's token again in ${SUBSPACE_FLEETS}.`,
  },
  "api-token-checking": {
    title: "Sample fleet: reading your fleet's token",
    body: "Fleet Console connects as soon as Subspace has it.",
  },
  "api-token-store": {
    title: "Sample fleet: your fleet's token could not be read",
    body: "Unlock your system keychain, then choose Retry.",
  },
};

/**
 * What the window says about a connection: a short pill label, its tone, and
 * a banner title and body with what to do. `banner` is null when all is well.
 */
export function describeConnection(state, { endpoint = "", now = 0 } = {}) {
  const wait = state.retryAt ? Math.max(0, Math.ceil((state.retryAt - now) / 1000)) : null;
  const retryText = wait === null ? "" : wait > 0 ? ` Retrying in ${wait} s.` : " Retrying now.";
  const where = endpoint || "the Stargate API";
  switch (state.status) {
    case "sample": {
      const reason = SAMPLE_REASONS[state.reason] ?? SAMPLE_REASONS["no-bridge"];
      return { label: "Sample", tone: "yellow", banner: { ...reason, retry: state.reason !== "no-bridge" } };
    }
    case "connecting":
      return { label: "Connecting", tone: "muted", banner: null };
    case "connected":
      return { label: "Live", tone: "green", banner: null };
    case "reconnecting":
      return {
        label: wait ? `Reconnecting · ${wait}s` : "Reconnecting",
        tone: "yellow",
        banner: {
          title: "Lost the event stream",
          body: `The connection to ${where} dropped. The map shows the last data received.${retryText}`,
          retry: true,
        },
      };
    case "signed-out":
      return {
        label: "Signed out",
        tone: "red",
        banner: {
          title: "Stargate refused the token (401)",
          body: `Run stargate api token on the fleet VM and save the token in ${SETTINGS_PATH}, then choose Retry. A rotated token also needs Stargate restarted.`,
          retry: true,
        },
      };
    case "forbidden":
      return {
        label: "Forbidden",
        tone: "red",
        banner: {
          title: "Stargate refused this network (403)",
          body: "The Fleet API answers only the private networks in its api.allowedNetworks. Reach it over Tailscale, a Cloudflare Tunnel, or an SSH tunnel to localhost, then choose Retry.",
          retry: true,
        },
      };
    case "unreachable":
      return {
        label: "Unreachable",
        tone: "red",
        banner: {
          title: state.httpStatus === 404 ? "No Fleet API at this address (404)" : `Cannot reach ${where}`,
          body:
            (state.httpStatus === 404
              ? "Check the address, and that Stargate's api block is enabled and Stargate was restarted."
              : state.httpStatus
                ? `Stargate answered ${state.httpStatus}. Check the fleet VM and Stargate's logs.`
                : "Check that the fleet VM is running, your private network (Tailscale, tunnel or SSH tunnel) is up, and Stargate's api.corsOrigins allows this Space.") +
            retryText,
          retry: true,
        },
      };
    default:
      return { label: "Starting", tone: "muted", banner: null };
  }
}
