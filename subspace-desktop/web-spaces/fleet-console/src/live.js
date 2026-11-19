// The live source: reads the fleet once, then keeps `GET /api/v1/events`
// open as a streamed fetch body, parsing it with the SSE parser. It reconnects
// with jittered backoff, treats three missed heartbeats as a dead stream, and
// reports every connection change. Timers, time and randomness are injected.

import { initialConnection, nextConnection } from "./connection.js";
import { createSseParser } from "./sse.js";

/** Stargate sends a heartbeat every 15 s; silence for this long means the stream is dead. */
export const WATCHDOG_MS = 45_000;
export const DETAIL_TIMEOUT_MS = 10_000;

export function createLiveSource({ client, onEvent, onConnection, now = Date.now, random = Math.random, setTimer = setTimeout, clearTimer = clearTimeout }) {
  let connection = initialConnection();
  let controller = null;
  let retryTimer = null;
  let watchdog = null;
  let watchdogFired = false;
  let stopped = false;
  let lastEventId = "";
  let haveFleet = false;

  function update(event) {
    connection = nextConnection(connection, event, now(), random);
    onConnection?.(connection);
    if (connection.retryAt !== null && !stopped) {
      clearTimer(retryTimer);
      retryTimer = setTimer(connect, Math.max(0, connection.retryAt - now()));
    }
  }

  function armWatchdog() {
    clearTimer(watchdog);
    watchdog = setTimer(() => {
      watchdogFired = true;
      controller?.abort();
    }, WATCHDOG_MS);
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
    onEvent({ type: event.type, data });
  }

  async function readStream(body) {
    const reader = body.getReader();
    const decoder = new TextDecoder();
    const parser = createSseParser(deliver);
    armWatchdog();
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      armWatchdog();
      parser.push(decoder.decode(value, { stream: true }));
    }
    parser.push(decoder.decode());
    parser.end();
  }

  async function connect() {
    if (stopped) return;
    clearTimer(retryTimer);
    retryTimer = null;
    controller?.abort();
    const own = new AbortController();
    controller = own;
    watchdogFired = false;
    update({ type: "connecting" });
    try {
      if (!haveFleet) {
        const fleet = await client.fleet(own.signal);
        if (own.signal.aborted) throw new Error("aborted");
        onEvent({ type: "fleet", data: fleet });
        haveFleet = true;
      }
      const response = await client.events(own.signal, lastEventId);
      if (own.signal.aborted) throw new Error("aborted");
      update({ type: "open" });
      await readStream(response.body);
      if (controller === own && !stopped) update({ type: "closed" });
    } catch (error) {
      if (controller !== own || stopped) return;
      if (own.signal.aborted && !watchdogFired) return;
      update({ type: "failed", status: error?.status, retryAfterMs: error?.retryAfterMs });
    } finally {
      if (controller === own) disarmWatchdog();
    }
  }

  async function detail(read, type) {
    const signal = typeof AbortSignal !== "undefined" && AbortSignal.timeout ? AbortSignal.timeout(DETAIL_TIMEOUT_MS) : undefined;
    try {
      const data = await read(signal);
      if (!stopped) onEvent({ type, data });
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
    stop() {
      stopped = true;
      clearTimer(retryTimer);
      disarmWatchdog();
      controller?.abort();
      controller = null;
    },
    /** Fresh detail on drill-in; the stream keeps it current afterwards. */
    readShip: (id) => detail((signal) => client.ship(id, signal), "ship-detail"),
    readCrew: (ship, member) => detail((signal) => client.crew(ship, member, signal), "crew-detail"),
  };
}
