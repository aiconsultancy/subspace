// What Fleet Console shares with Subspace: chat context for the selected
// crew member, and words for screen readers. Pure. Context never includes the
// endpoint, the token or anything from settings.

import { formatDuration, plural } from "./format.js";
import { crewView, labelOf, lookup, shipCounts } from "./model.js";

const MAX_LABEL = 100;
const MAX_CONTENT_BYTES = 8192;

const clip = (text, max) => (text.length <= max ? text : `${text.slice(0, max - 1)}…`);

function clipBytes(text, maxBytes) {
  const encoder = new TextEncoder();
  if (encoder.encode(text).length <= maxBytes) return text;
  let lo = 0;
  let hi = text.length;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (encoder.encode(`${text.slice(0, mid)}…`).length <= maxBytes) lo = mid;
    else hi = mid - 1;
  }
  return `${text.slice(0, lo)}…`;
}

/**
 * setContext's value for a crew selection, or null for any other tier.
 * `fleetLabel` names the fleet when there is more than one.
 */
export function crewContext(fleet, ships, selection, issueIds, now, fleetLabel = null) {
  if (selection.level !== 2) return null;
  const { ship, member } = lookup(fleet, ships, selection);
  if (!ship || !member) return null;
  const view = crewView(member);
  const lines = [
    ...(fleetLabel ? [`Fleet: ${fleetLabel}`] : []),
    `Ship: ${ship.name}${ship.repository ? ` (${ship.repository})` : ""}, captain ${ship.captain.name}`,
    `Crew member: ${view.name}${view.role ? `, ${view.role}` : ""}`,
    `State: ${labelOf(view.state)}${Number.isFinite(view.since) ? ` for ${formatDuration(now - view.since)}` : ""}${view.task ? (view.observed ? " (observed live)" : " (live status not checked)") : ""}`,
  ];
  if (view.task) {
    lines.push(`Task: ${view.task.summary}`);
    if (view.task.phase) lines.push(`Phase: ${view.task.phase}`);
    if (view.task.progress) lines.push(`Progress: ${view.task.progress}`);
  } else if (view.lastOutcome) {
    lines.push(`Last outcome: ${view.lastOutcome.summary} (${labelOf(view.lastOutcome.status).toLowerCase()})`);
  }
  if (view.git?.branch) {
    const parts = [];
    if (Number.isFinite(view.git.ahead)) parts.push(`${view.git.ahead} ahead`);
    if (Number.isFinite(view.git.behind)) parts.push(`${view.git.behind} behind`);
    if (Number.isFinite(view.git.changedFiles)) parts.push(plural(view.git.changedFiles, "changed file"));
    lines.push(`Branch: ${view.git.branch}${view.git.base ? ` against ${view.git.base}` : ""}${parts.length ? ` (${parts.join(", ")})` : ""}`);
  }
  if (view.pr) lines.push(`Pull request: #${view.pr.number} ${view.pr.title}, ${view.pr.state}, checks ${view.pr.checks}`);
  if (issueIds.length) lines.push(`Issues: ${issueIds.join(", ")}`);
  if (view.queuedCount) lines.push(`Queued: ${view.queuedCount}${view.queued[0] ? `, next “${view.queued[0].summary}”` : ""}`);
  // Subspace puts the Space title in front of the label itself.
  const label = fleetLabel ? `${view.name} on ${ship.name} · ${fleetLabel}` : `${view.name} on ${ship.name}`;
  return { label: clip(label, MAX_LABEL), content: clipBytes(lines.join("\n"), MAX_CONTENT_BYTES) };
}

/** One sentence about the Fleets tier: `fleets` are [{name, answering, bad}]. */
export function describeLandscape(fleets) {
  const answering = fleets.filter((fleet) => fleet.answering).length;
  const bad = fleets.filter((fleet) => fleet.bad).map((fleet) => fleet.name);
  return `Fleets: ${plural(fleets.length, "fleet")}, ${answering} answering${bad.length ? `; ${bad.join(", ")} cannot be read` : ""}. Use the inspector to move between tiers.`;
}

/** One sentence about the tier now shown, for the live region and the canvas's accessible name. */
export function describeTier(fleet, ships, selection) {
  if (!fleet) return "Fleet map. Waiting for fleet data.";
  if (selection.level === 0) {
    const online = fleet.ships.filter((ship) => ship.available).length;
    return `Fleet ${fleet.name}: ${plural(fleet.ships.length, "ship")}, ${online} available. Use the inspector to move between tiers.`;
  }
  const { ship, detail, member } = lookup(fleet, ships, selection);
  if (selection.level === 1) {
    if (!ship) return "Ship map.";
    if (!ship.available) return `Ship ${ship.name}: unavailable.`;
    const counts = shipCounts(ship, detail);
    return `Ship ${ship.name}: ${plural(counts.people, "crew member")}, ${counts.busy} busy. Escape pulls out to the fleet.`;
  }
  if (!member) return "Crew map.";
  const view = crewView(member);
  return `Crew member ${view.name} on ${ship.name}: ${labelOf(view.state).toLowerCase()}${view.task ? `, ${view.task.summary}` : ""}. Escape pulls out to the ship.`;
}
