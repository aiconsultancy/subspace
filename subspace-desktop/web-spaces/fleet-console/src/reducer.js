// The fleet state reducer. It applies Fleet API v1 events (`fleet`, `ship`,
// `host`, `activity`, `heartbeat`) and the page's own detail reads
// (`ship-detail`, `crew-detail`) to an immutable state. Invalid payloads are
// ignored. `receivedAt` is the page's clock when the event arrived.

import { isCrewDetail, isValidEvent, isShipDetail } from "./contract.js";

export const MAX_ACTIVITY = 40;
export const PACKET_LIFETIME_MS = 2700;

export function initialFleetState() {
  return {
    fleet: null,
    ships: {},
    activity: [],
    packets: [],
    unavailableSince: {},
    heartbeatAt: null,
    lastEventAt: null,
    skew: 0,
    version: 0,
  };
}

/** Server clock minus page clock, smoothed; a jump over two seconds is taken at once. */
function nextSkew(previous, serverTime, receivedAt) {
  if (!Number.isFinite(serverTime)) return previous;
  const sample = serverTime - receivedAt;
  if (Math.abs(sample - previous) > 2000) return sample;
  return previous * 0.8 + sample * 0.2;
}

function stripDetail(detail) {
  const { crewMembers: _members, ...ship } = detail;
  return ship;
}

function trackAvailability(previous, fleet, receivedAt) {
  const next = {};
  for (const ship of fleet.ships) {
    if (!ship.available) next[ship.id] = previous[ship.id] ?? receivedAt;
  }
  return next;
}

function applyFleet(state, fleet, receivedAt) {
  const ids = new Set(fleet.ships.map((ship) => ship.id));
  const ships = {};
  for (const [id, detail] of Object.entries(state.ships)) {
    if (!ids.has(id)) continue;
    // Keep crew detail, but take the fleet's newer ship summary.
    const summary = fleet.ships.find((ship) => ship.id === id);
    ships[id] = { ...detail, ...summary };
  }
  return {
    ...state,
    fleet,
    ships,
    unavailableSince: trackAvailability(state.unavailableSince, fleet, receivedAt),
    skew: nextSkew(state.skew, fleet.generatedAt, receivedAt),
  };
}

function applyShip(state, detail, receivedAt) {
  const ships = { ...state.ships, [detail.id]: detail };
  if (!state.fleet) return { ...state, ships };
  const summary = stripDetail(detail);
  const known = state.fleet.ships.some((ship) => ship.id === detail.id);
  const fleetShips = known
    ? state.fleet.ships.map((ship) => (ship.id === detail.id ? summary : ship))
    : [...state.fleet.ships, summary];
  const fleet = { ...state.fleet, ships: fleetShips };
  return { ...state, fleet, ships, unavailableSince: trackAvailability(state.unavailableSince, fleet, receivedAt) };
}

function applyCrewDetail(state, { ship, member }) {
  const detail = state.ships[ship];
  if (!detail) return state;
  const known = detail.crewMembers.some((entry) => entry.id === member.id);
  const crewMembers = known
    ? detail.crewMembers.map((entry) => (entry.id === member.id ? member : entry))
    : [...detail.crewMembers, member];
  return { ...state, ships: { ...state.ships, [ship]: { ...detail, crewMembers } } };
}

function packetFor(activity, receivedAt) {
  if (!activity.member) return null;
  if (activity.kind === "dispatch") return { ship: activity.ship, crew: activity.member, kind: "dispatch", t0: receivedAt };
  if (activity.kind === "outcome") return { ship: activity.ship, crew: activity.member, kind: "report", t0: receivedAt };
  return null;
}

function applyActivity(state, activity, receivedAt) {
  const activityList = [activity, ...state.activity].slice(0, MAX_ACTIVITY);
  const packet = packetFor(activity, receivedAt);
  const packets = state.packets.filter((entry) => receivedAt - entry.t0 < PACKET_LIFETIME_MS);
  return { ...state, activity: activityList, packets: packet ? [...packets, packet] : packets };
}

/**
 * @param {ReturnType<typeof initialFleetState>} state
 * @param {{type: string, data: unknown}} event
 * @param {number} receivedAt
 */
export function reduceFleet(state, event, receivedAt) {
  const { type, data } = event ?? {};
  let next = state;
  if (type === "ship-detail") {
    if (isShipDetail(data)) next = applyShip(state, data, receivedAt);
  } else if (type === "crew-detail") {
    if (isCrewDetail(data)) next = applyCrewDetail(state, data);
  } else if (!isValidEvent(type, data)) {
    return state;
  } else if (type === "fleet") {
    next = applyFleet(state, data, receivedAt);
  } else if (type === "ship") {
    next = applyShip(state, data, receivedAt);
  } else if (type === "host") {
    next = state.fleet ? { ...state, fleet: { ...state.fleet, host: data } } : state;
  } else if (type === "activity") {
    next = applyActivity(state, data, receivedAt);
  } else if (type === "heartbeat") {
    next = { ...state, heartbeatAt: data.at, skew: nextSkew(state.skew, data.at, receivedAt) };
  }
  if (next === state) return state;
  return { ...next, lastEventAt: receivedAt, version: state.version + 1 };
}

/** Packets still in flight at `now`. */
export function livePackets(state, now) {
  return state.packets.filter((packet) => now - packet.t0 < PACKET_LIFETIME_MS && now >= packet.t0);
}
