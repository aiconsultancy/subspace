// The Add / Edit fleet form in the inspector: reads and checks the fields,
// tests a connection, saves the token through Subspace's secrets and the
// list through the fleet store, and removes a fleet. main.js supplies the
// context and reacts when the fleets change.

import { describeTestError, describeTestSuccess, fleetForm, validateFleetForm } from "./fleet-panel.js";
import { MAX_LOCAL_FLEETS, newFleetId, packToken, tokenKey } from "./fleet-store.js";

const TEST_TIMEOUT_MS = 10_000;
const STORE_ERRORS = {
  newer: "A newer Fleet Console saved this list; update it to change fleets here.",
  full: `Fleet Console keeps up to ${MAX_LOCAL_FLEETS} fleets added here. Remove one to add another.`,
  invalid: "Fleet Console could not keep this fleet in its list.",
  unavailable: "This web view would not keep the fleet list. Check that site data is allowed.",
};
const ADDRESS_CHANGED = "The address changed: paste the token for the new address. A saved token is only sent to the address it was saved for.";

/**
 * @param {object} deps
 * @param {HTMLElement} deps.inspector
 * @param {(html: string) => void} deps.write           writes the inspector
 * @param {() => object} deps.bridge                    the connected bridge
 * @param {object} deps.store                           createFleetStore()
 * @param {() => object[]} deps.entries                 current fleet entries
 * @param {() => string|null} deps.blocker              why fleets cannot be added
 * @param {(endpoint, token, signal) => Promise<object>} deps.readFleet  GET /api/v1/fleet
 * @param {(change) => void} deps.onChanged             {added|edited|removed, id, name}
 * @param {() => void} deps.onClose
 */
export function createFleetForm(deps) {
  let open = null;

  const $ = (id) => deps.inspector.querySelector(`#${id}`);
  const values = () => ({ name: $("fleet-name")?.value ?? "", endpoint: $("fleet-endpoint")?.value ?? "", token: $("fleet-token")?.value ?? "" });
  const entryOf = (id) => deps.entries().find((entry) => entry.id === id) ?? null;
  const others = (id) => deps.entries().filter((entry) => entry.id !== id).map((entry) => entry.endpoint);

  function status(text, tone = "") {
    const line = $("fleet-status");
    if (!line) return;
    line.textContent = text;
    line.dataset.tone = tone;
  }

  function showErrors(errors) {
    for (const field of ["name", "endpoint", "token"]) {
      const input = $(`fleet-${field}`);
      const message = $(`fleet-${field}-error`);
      if (!input || !message) continue;
      const text = errors[field] ?? "";
      message.textContent = text;
      message.hidden = !text;
      if (text) input.setAttribute("aria-invalid", "true");
      else input.removeAttribute("aria-invalid");
    }
    const first = ["name", "endpoint", "token"].find((field) => errors[field]);
    if (first) $(`fleet-${first}`)?.focus();
  }

  /** Validate the fields; a test needs only the address and token. */
  function check(forTest) {
    const editing = open.mode === "edit";
    const typed = values();
    const result = validateFleetForm(forTest ? { ...typed, name: typed.name || "untitled" } : typed, { editing, existing: others(open.id) });
    showErrors(result.errors);
    return result.value;
  }

  /**
   * The token to use with the typed address: the one just pasted, or, when
   * editing without changing the address, the one saved for that address.
   * A saved token never goes to a different address.
   */
  function tokenFor(value) {
    if (value.token) return { token: value.token };
    const saved = open.mode === "edit" ? entryOf(open.id) : null;
    if (saved && saved.endpoint !== value.endpoint) return { error: ADDRESS_CHANGED };
    return saved?.token ? { token: saved.token } : { error: null };
  }

  async function test() {
    const value = check(true);
    if (!value) return;
    const { token, error } = tokenFor(value);
    if (!token) {
      showErrors({ token: error ?? "Paste the token to test the connection." });
      return;
    }
    status("Testing the connection…");
    try {
      const signal = typeof AbortSignal !== "undefined" && AbortSignal.timeout ? AbortSignal.timeout(TEST_TIMEOUT_MS) : undefined;
      const fleet = await deps.readFleet(value.endpoint, token, signal);
      status(describeTestSuccess(fleet), "ok");
    } catch (error) {
      status(describeTestError(error), "error");
    }
  }

  async function save() {
    const blocker = deps.blocker();
    if (blocker) {
      status(blocker, "error");
      return;
    }
    const editing = open.mode === "edit";
    if (!editing && deps.store.load().length >= MAX_LOCAL_FLEETS) {
      status(`Fleet Console keeps up to ${MAX_LOCAL_FLEETS} fleets added here. Remove one to add another.`, "error");
      return;
    }
    const value = check(false);
    if (!value) return;
    const previous = editing ? entryOf(open.id) : null;
    if (editing && !value.token && previous && previous.endpoint !== value.endpoint) {
      showErrors({ token: ADDRESS_CHANGED });
      return;
    }
    const id = editing ? open.id : newFleetId();
    const bridge = deps.bridge();
    if (value.token) {
      try {
        await bridge.setSecret(tokenKey(id), packToken(value.endpoint, value.token));
      } catch (error) {
        status(`Subspace could not keep the token: ${error?.message ?? error}`, "error");
        return;
      }
    }
    const list = deps.store.load().filter((item) => item.id !== id);
    const position = editing ? deps.store.load().findIndex((item) => item.id === id) : -1;
    const item = { id, name: value.name, endpoint: value.endpoint };
    if (position >= 0) list.splice(position, 0, item);
    else list.push(item);
    if (!deps.store.save(list)) {
      // Put the secret back as it was: no token for a new fleet, the earlier one for an edit.
      if (value.token) await bridge.setSecret(tokenKey(id), previous?.token ? packToken(previous.endpoint, previous.token) : null).catch(() => {});
      status(STORE_ERRORS[deps.store.status] ?? STORE_ERRORS.unavailable, "error");
      return;
    }
    open = null;
    deps.onChanged({ kind: editing ? "edited" : "added", id, name: value.name });
  }

  return {
    get isOpen() {
      return open !== null;
    },
    /** mode: "add", "edit" (with id) or "settings" (the default fleet). */
    open(mode, id = null) {
      const entry = id ? entryOf(id) : null;
      open = { mode, id };
      deps.write(fleetForm({ mode, entry, blocker: mode === "settings" ? null : deps.blocker() }));
      const form = $("fleet-form");
      form?.addEventListener("submit", (event) => {
        event.preventDefault();
        save();
      });
      ($("fleet-name") ?? deps.inspector.querySelector("h2"))?.focus();
    },
    close() {
      open = null;
      deps.onClose();
    },
    /** Handle a click inside the inspector; returns true when it was a form control. */
    click(target) {
      if (target.closest("[data-fleet-cancel]")) {
        this.close();
        return true;
      }
      if (target.closest("[data-fleet-test]")) {
        test();
        return true;
      }
      return false;
    },
    /** Remove a page-added fleet and its token. */
    async remove(id) {
      const entry = entryOf(id);
      if (!entry || entry.origin !== "local") return;
      let warning = null;
      try {
        await deps.bridge().setSecret(tokenKey(id), null);
      } catch (error) {
        warning = error?.message ?? String(error);
      }
      deps.store.save(deps.store.load().filter((item) => item.id !== id));
      deps.onChanged({ kind: "removed", id, name: entry.name, warning });
    },
  };
}
