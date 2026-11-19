// Fleets in the inspector: the landscape panel, each fleet's connection
// section with Edit and Remove, and the Add / Edit fleet form. HTML builders
// and validation are pure; main.js wires the events.

import { normalizeEndpoint } from "./api.js";
import { cleanName } from "./fleet-store.js";
import { describeConnection } from "./connection.js";
import { fleetStatus } from "./scene/landscape.js";
import { esc } from "./html.js";

const SETTINGS_PATH = "Settings › Spaces › Fleet Console";

const ORIGIN = { settings: "Settings", local: "Added here", api: "Subspace", sample: "Sample" };

/** Validate the form. `existing` lists the other fleets' normalized addresses. */
export function validateFleetForm({ name, endpoint, token }, { editing = false, existing = [] } = {}) {
  const errors = {};
  const cleanedName = cleanName(name);
  if (!cleanedName) errors.name = "Give the fleet a name.";
  const address = normalizeEndpoint(endpoint);
  if (!String(endpoint ?? "").trim()) errors.endpoint = "Enter the Stargate API address.";
  else if (!address.ok) {
    errors.endpoint =
      address.reason === "insecure"
        ? "Use HTTPS, or http://localhost through an SSH tunnel: a web Space cannot reach plain HTTP on other hosts."
        : address.reason === "credentials"
          ? "Leave the user name and password out of the address."
          : "Enter a full address with its scheme, as in https://fleet.example.com";
  } else if (existing.includes(address.url)) errors.endpoint = "This address is already one of your fleets.";
  const secret = String(token ?? "").trim();
  if (!secret && !editing) errors.token = "Paste the token from stargate api token.";
  else if (secret.length > 4096 || /[\u0000-\u001f\u007f]/.test(secret)) errors.token = "That does not look like a Stargate token.";
  const ok = Object.keys(errors).length === 0;
  return { ok, errors, value: ok ? { name: cleanedName, endpoint: address.url, token: secret || null } : null };
}

/** Words for a failed test connection. */
export function describeTestError(error) {
  const status = error?.status;
  if (status === 401) return "Stargate refused the token (401). Check it, or create a new one with stargate api token.";
  if (status === 403) return "Stargate refused this network (403). Reach it over your private network: Tailscale, a Cloudflare Tunnel with Access, or an SSH tunnel.";
  if (status === 404) return "No Fleet API at this address (404). Check the address and that Stargate's api block is enabled.";
  if (Number.isInteger(status)) return `Stargate answered ${status}. Check the fleet VM and Stargate's logs.`;
  if (error?.name === "TimeoutError" || error?.name === "AbortError") return "Stargate did not answer within ten seconds.";
  return "Cannot reach this address. Check that the fleet VM is running, your private network is up, and Stargate's api.corsOrigins allows this Space.";
}

/** Words for a successful test connection. */
export function describeTestSuccess(fleet) {
  const ships = fleet.ships?.length ?? 0;
  return `Connected to “${fleet.name}”: ${ships} ship${ships === 1 ? "" : "s"}, Stargate ${fleet.gateway?.version ?? "unknown"}.`;
}

/** Why adding fleets is not possible here, or null when it is. */
export function addBlocker({ present, capabilities, allowed }) {
  if (!present) return "Adding fleets needs Subspace: this page is not running inside it. Install Fleet Console as a web Space to add your own fleets.";
  if (!capabilities.secretsSet) return "This version of Subspace cannot store tokens for more fleets yet. Update Subspace to add fleets here; the fleet in Settings keeps working.";
  if (!allowed.secrets) return `Allow this app to read its saved secrets in ${SETTINGS_PATH}, then try again: each fleet's token is kept there.`;
  return null;
}

const host = (endpoint) => {
  try {
    return new URL(endpoint).host;
  } catch {
    return endpoint;
  }
};

function fleetRow(fleet) {
  const status = fleetStatus(fleet);
  const dot = status.bad ? "unreachable" : status.busy ? "running" : fleet.state.fleet ? "idle" : "queued";
  const edit = fleet.entry.origin === "local" ? `<button class="mini" type="button" data-fleet-edit="${esc(fleet.id)}" aria-label="Edit ${esc(fleet.name)}">Edit</button>` : "";
  return `<li class="fleet-row"><button class="row" type="button" data-go="fleet" data-fleet="${esc(fleet.id)}"><span class="dot s-${dot}"></span><span class="grow"><b>${esc(fleet.name)}</b><span class="sub mono">${esc(host(fleet.entry.endpoint))}</span></span><span class="tl t-${status.bad ? "red" : "muted"}">${esc(status.text)}</span></button>${edit}</li>`;
}

/** What the Fleets tier says under sample fleets: how to see real ones here. */
export function sampleNote(blocker) {
  return blocker
    ? `These are sample fleets. ${blocker}`
    : "These are sample fleets. Add a fleet, or set the Stargate API address in Settings, to see your own.";
}

/** The Fleets (L0) panel. */
export function landscapePanel({ fleets, sample, blocker }) {
  const answering = fleets.filter((fleet) => fleet.state.fleet && !fleetStatus(fleet).bad).length;
  return [
    `<header class="ihead"><p class="eyebrow">Fleets · L0 · landscape</p><h2 tabindex="-1">Fleets</h2><p class="meta mono">${fleets.length} fleets · ${answering} answering</p></header>`,
    `<section class="sec"><h3>Fleets <span class="n">${fleets.length}</span></h3><ul class="rows">${fleets.map(fleetRow).join("")}</ul></section>`,
    `<section class="sec"><div class="actions"><button class="btn" type="button" data-fleet-add>Add fleet</button></div>${sample ? `<p class="prog">${esc(sampleNote(blocker))}</p>` : blocker ? `<p class="prog">${esc(blocker)}</p>` : '<p class="prog">Each fleet is one VM running Stargate. The fleet you are inside streams live; the others are read every 30 seconds while this view is open.</p>'}</section>`,
  ].join("");
}

/** The connection section of a fleet's panel, with Edit, Remove and Add. */
export function fleetSection({ fleet, total, sample, confirming, now }) {
  const status = fleetStatus(fleet);
  const description = describeConnection(fleet.connection, { endpoint: fleet.entry.endpoint, now });
  const origin = ORIGIN[fleet.entry.origin] ?? fleet.entry.origin;
  const problem =
    fleet.connection.reason === "no-token"
      ? `<p class="prog warn-text">No token is saved for this fleet. Edit it and paste the token again.</p>`
      : description.banner && !sample
        ? `<p class="prog${status.bad ? " warn-text" : ""}"><b>${esc(description.banner.title)}.</b> ${esc(description.banner.body)}</p>`
        : "";
  let actions = "";
  if (confirming) {
    actions = `<p class="prog">Remove ${esc(fleet.name)}? Its saved token is deleted too.</p><div class="actions"><button class="btn danger" type="button" data-fleet-remove-confirm="${esc(fleet.id)}">Remove</button><button class="btn" type="button" data-fleet-remove-cancel>Keep it</button></div>`;
  } else if (fleet.entry.origin === "local") {
    actions = `<div class="actions"><button class="btn" type="button" data-fleet-edit="${esc(fleet.id)}">Edit</button><button class="btn" type="button" data-fleet-remove="${esc(fleet.id)}">Remove</button></div>`;
  } else if (fleet.entry.origin === "settings") {
    actions = `<p class="prog">This fleet's address and token come from ${SETTINGS_PATH}.</p>`;
  }
  return `<section class="sec"><h3>Connection</h3><dl class="kv"><dt>Address</dt><dd class="mono">${esc(fleet.entry.endpoint)}</dd><dt>State</dt><dd>${esc(sample ? "Sample data" : description.label)}${status.bad ? "" : fleet.streaming ? " · live stream" : fleet.lastReadAt ? " · read every 30 s" : ""}</dd><dt>From</dt><dd>${esc(origin)}</dd></dl>${problem}${actions}</section>`;
}

/** With a single fleet, Add fleet sits under the heading, where it is seen. */
export function addFleetBar(total) {
  return total === 1 ? '<div class="actions add-bar"><button class="btn" type="button" data-fleet-add>Add fleet</button><span class="sub">Watch another Stargate VM beside this one.</span></div>' : "";
}

/** A fleet that cannot be read, in place of its fleet panel. */
export function unreadableFleetPanel({ fleet, total, sample, confirming, now }) {
  return `<header class="ihead"><p class="eyebrow">Fleet · L1 · context</p><h2 tabindex="-1">${esc(fleet.name)}</h2><p class="meta mono">${esc(host(fleet.entry.endpoint))}</p></header>${addFleetBar(total)}${fleetSection({ fleet, total, sample, confirming, now })}`;
}

/** The Add / Edit fleet form, or why it cannot be used. */
export function fleetForm({ mode, entry, blocker }) {
  const title = mode === "edit" ? `Edit ${entry.name}` : mode === "settings" ? "The first fleet" : "Add fleet";
  const head = `<header class="ihead"><p class="eyebrow">Fleets</p><h2 tabindex="-1">${esc(title)}</h2></header>`;
  if (mode === "settings") {
    return `${head}<section class="sec"><p class="prog">The first fleet's address and token are set in ${SETTINGS_PATH}, so the Settings page stays the one place to change them.</p><div class="actions"><button class="btn" type="button" data-fleet-cancel>Done</button></div></section>`;
  }
  if (blocker) {
    return `${head}<section class="sec"><p class="prog">${esc(blocker)}</p><div class="actions"><button class="btn" type="button" data-fleet-cancel>Close</button></div></section>`;
  }
  const editing = mode === "edit";
  const field = (id, label, input, hint) =>
    `<div class="field-row"><label for="fleet-${id}">${label}</label>${input}<p class="field-hint" id="fleet-${id}-hint">${hint}</p><p class="field-error" id="fleet-${id}-error" hidden></p></div>`;
  return `${head}<form class="fleet-form" id="fleet-form" novalidate>
    ${field("name", "Name", `<input id="fleet-name" name="name" type="text" maxlength="60" autocomplete="off" spellcheck="false" value="${esc(editing ? entry.name : "")}" aria-describedby="fleet-name-hint fleet-name-error">`, "What you call this fleet, such as Production or Lab.")}
    ${field("endpoint", "Stargate API address", `<input id="fleet-endpoint" name="endpoint" type="url" inputmode="url" autocomplete="off" spellcheck="false" placeholder="https://fleet.example.com" value="${esc(editing ? entry.endpoint : "")}" aria-describedby="fleet-endpoint-hint fleet-endpoint-error">`, "HTTPS on your private network (Tailscale, Cloudflare Access), or http://localhost:&lt;port&gt; through an SSH tunnel.")}
    ${field("token", "API token", `<input id="fleet-token" name="token" type="password" autocomplete="off" spellcheck="false" ${editing ? 'placeholder="Saved. Leave blank to keep it."' : ""} aria-describedby="fleet-token-hint fleet-token-error">`, editing ? "Paste a new token only to replace the saved one." : "From stargate api token on the fleet VM. Subspace keeps it with this Space's secrets.")}
    <p class="form-status" id="fleet-status" role="status"></p>
    <div class="actions"><button class="btn" type="button" data-fleet-test>Test connection</button><button class="btn primary" type="submit">${editing ? "Save" : "Add fleet"}</button><button class="btn" type="button" data-fleet-cancel>Cancel</button></div>
  </form>`;
}
