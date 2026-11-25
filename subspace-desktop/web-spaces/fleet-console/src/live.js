// The live source: reads the fleet once, then keeps `GET /api/v1/events`
// open as a streamed fetch body, parsing it with the SSE parser. It reconnects
// with jittered backoff, treats three missed heartbeats as a dead stream, and
// reports every connection change. Timers, time and randomness are injected.
//
// It is the page's only stream for its fleet: the gateway allows few viewers.
// Officer activity rides on it, by re-opening it with `?agent=<id>` while an
// activity view is open and without once it closes (`setAgents`). `onDown`
// tells the view each time the attempt that carried its agent is gone (a new
// attempt begins, or the stream closes or fails), so it waits for the next
// answer before reading its snapshot again.

import { initialConnection, nextConnection } from "./connection.js";
import { createSseParser } from "./sse.js";

/** Stargate sends a heartbeat every 15 s; silence for this long means the stream is dead. */
export const WATCHDOG_MS = 45_000;
/** A stream that stays open this long (past one heartbeat) resets the reconnect backoff. */
export const HEALTHY_MS = 20_000;
/** How long the fleet read and the stream request may take to answer. */
export const CONNECT_TIMEOUT_MS = 15_000;
export const DETAIL_TIMEOUT_MS = 10_000;

export function createLiveSource({ client, onEvent, onConnection, onOpen, onDown, onAgentsRefused, agents: initialAgents = [], now = Date.now, random = Math.random, setTimer = setTimeout, clearTimer = clearTimeout }) {
  let connection = initialConnection();
  let controller = null;
  let retryTimer = null;
  let watchdog = null;
  let healthy = null;
  let watchdogFired = false;
  let stopped = false;
  let lastEventId = "";
  let haveFleet = false;
  /** Registration IDs whose `agent-activity` events the stream asks for. */
  let agents = [...initialAgents];
  /** How many `ship` events each ship has had, so a slow detail read can tell it is stale. */
  const shipEvents = new Map();

  function update(event) {
    connection = nextConnection(connection, event, now(), random);
    onConnection?.(connection);
    if (connection.retryAt !== null && !stopped) {
      clearTimer(retryTimer);
      retryTimer = setTimer(connect, Math.max(0, connection.retryAt - now()));
    }
  }

  /** Abort the current request after `ms` of silence: no answer, or no bytes on the stream. */
  function armWatchdog(ms = WATCHDOG_MS) {
    clearTimer(watchdog);
    watchdog = setTimer(() => {
      watchdogFired = true;
      controller?.abort();
    }, ms);
  }

  function disarmWatchdog() {
    clearTimer(watchdog);
    watchdog = null;
  }

  function deliver(event) {
    if (event.id) lastEventId = event.id;
    let data;
    try {
      data = JSON.parse(event.data);
    } catch {
      return;
    }
    if (event.type === "ship" && typeof data?.id === "string") shipEvents.set(data.id, (shipEvents.get(data.id) ?? 0) + 1);
    onEvent({ type: event.type, data });
  }

  async function readStream(body) {
    const reader = body.getReader();
    const decoder = new TextDecoder();
    const parser = createSseParser(deliver);
    armWatchdog();
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        armWatchdog();
        parser.push(decoder.decode(value, { stream: true }));
      }
      parser.push(decoder.decode());
      parser.end();
    } catch (error) {
      // A stream the parser refuses (or that failed) is not read any further.
      await reader.cancel(error).catch(() => {});
      throw error;
    }
  }

  /** `quiet` re-opens a working stream without showing "Connecting". */
  async function connect(quiet = false) {
    if (stopped) return;
    clearTimer(retryTimer);
    retryTimer = null;
    controller?.abort();
    const own = new AbortController();
    controller = own;
    watchdogFired = false;
    // Whatever the last attempt answered no longer holds.
    if (agents.length) onDown?.([...agents]);
    if (!(quiet && connection.status === "connected")) update({ type: "connecting" });
    try {
      if (!haveFleet) {
        armWatchdog(CONNECT_TIMEOUT_MS);
        const fleet = await client.fleet(own.signal);
        if (own.signal.aborted) throw new Error("aborted");
        onEvent({ type: "fleet", data: fleet });
        haveFleet = true;
      }
      armWatchdog(CONNECT_TIMEOUT_MS);
      const response = await client.events(own.signal, lastEventId, agents);
      if (own.signal.aborted) throw new Error("aborted");
      update({ type: "open" });
      // Activity events have no replay: a viewer refetches its snapshot on every open.
      onOpen?.([...agents]);
      clearTimer(healthy);
      healthy = setTimer(() => {
        healthy = null;
        if (controller === own && !stopped) update({ type: "healthy" });
      }, HEALTHY_MS);
      await readStream(response.body);
      if (controller === own && !stopped) {
        if (agents.length) onDown?.([...agents]);
        update({ type: "closed" });
      }
    } catch (error) {
      if (controller !== own || stopped) return;
      if (own.signal.aborted && !watchdogFired) return;
      // The gateway checks `agent` before the stream starts: 400 or 404 refuses
      // the subscription, not the fleet. Drop it and carry on without it.
      if (agents.length && (error?.status === 400 || error?.status === 404)) {
        const refused = agents;
        agents = [];
        onAgentsRefused?.(refused, error.status);
        if (!stopped) connect(true);
        return;
      }
      if (agents.length) onDown?.([...agents]);
      update({ type: "failed", status: error?.status, retryAfterMs: error?.retryAfterMs });
    } finally {
      if (controller === own) {
        disarmWatchdog();
        clearTimer(healthy);
        healthy = null;
      }
    }
  }

  /** A detail read for `ship`; dropped if the stream sent that ship again meanwhile, as the stream is newer. */
  async function detail(ship, read, type) {
    const signal = typeof AbortSignal !== "undefined" && AbortSignal.timeout ? AbortSignal.timeout(DETAIL_TIMEOUT_MS) : undefined;
    const seen = shipEvents.get(ship) ?? 0;
    try {
      const data = await read(signal);
      if (!stopped && (shipEvents.get(ship) ?? 0) === seen) onEvent({ type, data });
      return true;
    } catch {
      return false;
    }
  }

  return {
    get connection() {
      return connection;
    },
    start: () => connect(),
    retryNow: () => connect(),
    get agents() {
      return [...agents];
    },
    /** Ask for `agent-activity` events for these registration IDs (at most 8), re-opening the one stream if they changed. */
    setAgents(next) {
      const unique = [...new Set(next)].slice(0, 8);
      if (unique.length === agents.length && unique.every((id, i) => id === agents[i])) return;
      agents = unique;
      if (!stopped && controller) connect(true);
    },
    stop() {
      stopped = true;
      clearTimer(retryTimer);
      clearTimer(healthy);
      disarmWatchdog();
      controller?.abort();
      controller = null;
    },
    /** Fresh detail on drill-in; the stream keeps it current afterwards. */
    readShip: (id) => detail(id, (signal) => client.ship(id, signal), "ship-detail"),
    readCrew: (ship, member) => detail(ship, (signal) => client.crew(ship, member, signal), "crew-detail"),
  };
}
