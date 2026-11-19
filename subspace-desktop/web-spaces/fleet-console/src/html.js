// HTML helpers shared by the inspector, tooltip and ticker. Every value that
// came from the API goes through `esc`; markup is only ever built here and in
// inspector.js, and written with innerHTML by chrome.js.

import { formatDuration, initials } from "./format.js";
import { labelOf } from "./model.js";
import { issueStateOf } from "./issues.js";

export function esc(value) {
  return String(value ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
}

/** A span the page refreshes every second with the time since `t` (server clock). */
export function since(t, now) {
  if (!Number.isFinite(t)) return "—";
  return `<span data-since="${Math.round(t)}">${formatDuration(now - t)}</span>`;
}

export function chip(state, t, now) {
  return `<span class="chip s-${esc(state)}">${esc(labelOf(state))}${Number.isFinite(t) ? ` · ${since(t, now)}` : ""}</span>`;
}

export function avatar(name, state) {
  return `<span class="av s-${esc(state)}" aria-hidden="true">${esc(initials(name))}</span>`;
}

/** Navigation attributes for a ship or crew row. */
export function goAttrs(selection) {
  if (selection.level === -1) return 'data-go="fleets"';
  const fleet = selection.fleet ? ` data-fleet="${esc(selection.fleet)}"` : "";
  if (selection.level === 0) return `data-go="fleet"${fleet}`;
  if (selection.level === 1) return `data-go="ship"${fleet} data-ship="${esc(selection.ship)}"`;
  return `data-go="crew"${fleet} data-ship="${esc(selection.ship)}" data-crew="${esc(selection.crew)}"`;
}

export const BRANCH_ICON =
  '<svg class="ic" viewBox="0 0 16 16" aria-hidden="true"><circle cx="4.5" cy="3.5" r="1.7"/><circle cx="4.5" cy="12.5" r="1.7"/><circle cx="11.5" cy="5.5" r="1.7"/><path d="M4.5 5.2v5.6M11.5 7.2c0 3-3.5 2.5-6.6 4"/></svg>';

const GLYPHS = {
  circle: '<circle cx="8" cy="8" r="5.5"/>',
  "circle-dashed": '<circle cx="8" cy="8" r="5.5" stroke-dasharray="2.2 2.2"/>',
  "circle-dot-dashed": '<circle cx="8" cy="8" r="5.5" stroke-dasharray="2.2 2.2"/><circle cx="8" cy="8" r="1.6" fill="currentColor" stroke="none"/>',
  contrast: '<circle cx="8" cy="8" r="5.5"/><path d="M8 2.5a5.5 5.5 0 0 1 0 11z" fill="currentColor" stroke="none"/>',
  "circle-check": '<circle cx="8" cy="8" r="5.5"/><path d="M5.6 8.2l1.7 1.7 3.2-3.4"/>',
  "circle-x": '<circle cx="8" cy="8" r="5.5"/><path d="M6.2 6.2l3.6 3.6M9.8 6.2l-3.6 3.6"/>',
  "circle-slash": '<circle cx="8" cy="8" r="5.5"/><path d="M5.5 10.5l5-5"/>',
};

export function stateGlyph(kind) {
  const { glyph, tone } = issueStateOf(kind);
  return `<svg class="glyph t-${tone}" viewBox="0 0 16 16" aria-hidden="true">${GLYPHS[glyph] ?? GLYPHS.circle}</svg>`;
}

const QUIET = { pending: "Checking…", "signed-out": "Sign in to Issues to see it", unavailable: "Not found on the board", failed: "Could not load" };

/**
 * Issue tiles like IRC's: state glyph, identifier, assignee, title and state.
 * `get(id)` returns a Resolution or null; `canOpen` makes them buttons.
 */
export function issueTiles(ids, get, canOpen) {
  if (!ids.length) return "";
  const tiles = [];
  const quiet = [];
  for (const id of ids) {
    const answer = get(id);
    if (answer?.state === "available" && answer.summary) {
      const s = answer.summary;
      const body = `<span class="tile-head"><span class="mono">${esc(s.identifier)}</span>${s.assignee ? `<span>· ${esc(s.assignee)}</span>` : ""}${s.archived ? "<span>· Archived</span>" : ""}</span><span class="tile-title">${esc(s.title)}</span><span class="tile-state">${stateGlyph(s.state?.kind)}${esc(s.state?.name ?? "")}</span>`;
      tiles.push(
        canOpen
          ? `<li><button class="tile" type="button" data-issue="${esc(s.identifier)}" aria-label="Open ${esc(s.identifier)} in Issues, ${esc(s.title)}">${body}</button></li>`
          : `<li><div class="tile">${body}</div></li>`,
      );
    } else {
      quiet.push(`<li class="tile-quiet"><span class="mono">${esc(id)}</span><span>${esc(QUIET[answer?.state] ?? QUIET.pending)}</span></li>`);
    }
  }
  return `<ul class="tiles">${tiles.join("")}${quiet.join("")}</ul>`;
}
