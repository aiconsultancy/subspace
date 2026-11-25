// One runtime per fleet: its reducer state and its connection state. The
// fleet you are inside streams `GET /api/v1/events` as before; every other
// fleet gets one `GET /api/v1/fleet` on load and another every 30 seconds
// while the Fleets tier is visible and the page is not hidden. Reads never
// overlap per fleet, and a failed read waits for the next interval rather
// than hammering a host.

import { initialFleetState, reduceFleet } from "./reducer.js";
import { initialConnection, nextConnection } from "./connection.js";
import { SAMPLE_FLEETS } from "./sample.js";

export const POLL_MS = 30_000;
export const POLL_TIMEOUT_MS = 10_000;

/** When `runtime` should next be read, or Infinity. Pure, for the scheduler and tests. */
export function nextPollAt(runtime, { landscape, interval = POLL_MS, hidden = false }) {
  if (hidden || !runtime.client || runtime.streaming || runtime.inflight) return Infinity;
  if (runtime.lastReadAt === null) return 0;
  return landscape ? runtime.lastReadAt + interval : Infinity;
}

function makeRuntime(entry, createClient, now) {
  const runtime = {
    entry,
    client: entry.token ? createClient(entry) : null,
    state: initialFleetState(),
    connection: initialConnection(),
    live: null,
    streaming: false,
    inflight: false,
    lastReadAt: null,
  };
  if (!entry.token) runtime.connection = { ...initialConnection(), status: "signed-out", reason: "no-token", since: now };
  return runtime;
}

const sameSource = (a, b) => a.endpoint === b.endpoint && a.token === b.token;

/**
 * @param {object} options
 * @param {object[]} options.entries                 fleet entries from fleet-sources.js
 * @param {(entry) => object} options.createClient   an API client for an entry
 * @param {(options) => object} options.createLive   createLiveSource-compatible
 * @param {(id, event?) => void} options.onChange    a fleet's state or connection changed
 */
export function createFleetManager({ entries, createClient, createLive, onChange, now = Date.now, setTimer = setTimeout, clearTimer = clearTimeout, pollMs = POLL_MS }) {
  const runtimes = new Map();
  let order = [];
  let active = null;
  let landscape = false;
  let hidden = false;
  let timer = null;
  let stopped = false;
  /** The open activity view, if any: {fleet, agent, onEvent, onOpen, onDown, onRefused, onReset, onEnded}. One stream carries it. */
  let watch = null;

  /** The fleet behind the open view lost its client or went: the view ends. */
  function endWatch() {
    const ended = watch;
    watch = null;
    ended?.onEnded?.();
  }

  const apply = (id, event) => {
    const runtime = runtimes.get(id);
    if (!runtime) return;
    const next = reduceFleet(runtime.state, event, now());
    if (next === runtime.state) return;
    runtime.state = next;
    onChange?.(id, event);
  };

  const setConnection = (id, event) => {
    const runtime = runtimes.get(id);
    if (!runtime) return;
    runtime.connection = nextConnection(runtime.connection, event, now());
    onChange?.(id);
  };

  async function poll(id) {
    const runtime = runtimes.get(id);
    if (!runtime?.client || runtime.streaming || runtime.inflight) return;
    runtime.inflight = true;
    // Only the first read shows "connecting"; later reads keep the last state.
    if (runtime.connection.status === "idle") setConnection(id, { type: "connecting" });
    const signal = typeof AbortSignal !== "undefined" && AbortSignal.timeout ? AbortSignal.timeout(POLL_TIMEOUT_MS) : undefined;
    try {
      const fleet = await runtime.client.fleet(signal);
      if (runtimes.get(id) !== runtime || runtime.streaming) return;
      apply(id, { type: "fleet", data: fleet });
      // A whole read is a healthy result for a polled fleet: it clears the failure count.
      runtime.connection = nextConnection(nextConnection(runtime.connection, { type: "open" }, now()), { type: "healthy" }, now());
      onChange?.(id);
    } catch (error) {
      if (runtimes.get(id) !== runtime || runtime.streaming) return;
      runtime.connection = nextConnection(runtime.connection, { type: "failed", status: error?.status, retryAfterMs: error?.retryAfterMs }, now());
      // A polled fleet is read again on the poll interval, not on the stream's backoff.
      if (runtime.connection.retryAt !== null) runtime.connection = { ...runtime.connection, retryAt: landscape ? now() + pollMs : null };
      onChange?.(id);
    } finally {
      runtime.inflight = false;
      runtime.lastReadAt = now();
      schedule();
    }
  }

  function check() {
    timer = null;
    const t = now();
    for (const id of order) {
      if (nextPollAt(runtimes.get(id), { landscape, interval: pollMs, hidden }) <= t) poll(id);
    }
    schedule();
  }

  function schedule() {
    if (timer !== null) clearTimer(timer);
    timer = null;
    if (stopped) return;
    let next = Infinity;
    for (const runtime of runtimes.values()) next = Math.min(next, nextPollAt(runtime, { landscape, interval: pollMs, hidden }));
    if (next < Infinity) timer = setTimer(check, Math.max(0, next - now()));
  }

  function stream(id) {
    const runtime = runtimes.get(id);
    if (!runtime?.client) return;
    runtime.streaming = true;
    // Only this runtime's stream speaks for the view: a replaced one's late answer is ignored.
    const watching = () => runtimes.get(id) === runtime && watch?.fleet === id;
    runtime.live = createLive({
      client: runtime.client,
      agents: watch?.fleet === id ? [watch.agent] : [],
      onEvent: (event) => {
        if (event.type !== "agent-activity") apply(id, event);
        else if (watching()) watch.onEvent(event.data);
      },
      onOpen: (agents) => {
        if (watching() && agents.includes(watch.agent)) watch.onOpen();
      },
      onDown: (agents) => {
        if (watching() && agents.includes(watch.agent)) watch.onDown?.();
      },
      onAgentsRefused: (agents, status) => {
        if (watching() && agents.includes(watch.agent)) watch.onRefused?.(status);
      },
      onConnection: (connection) => {
        if (runtimes.get(id) !== runtime) return;
        runtime.connection = connection;
        onChange?.(id);
      },
    });
    runtime.live.start();
  }

  function unstream(id) {
    const runtime = runtimes.get(id);
    if (!runtime?.live) return;
    runtime.live.stop();
    runtime.live = null;
    runtime.streaming = false;
    runtime.lastReadAt = now();
  }

  function reconcile(next) {
    const ids = new Set(next.map((entry) => entry.id));
    for (const [id, runtime] of runtimes) {
      if (ids.has(id)) continue;
      runtime.live?.stop();
      runtimes.delete(id);
      if (active === id) active = null;
      if (watch?.fleet === id) endWatch();
    }
    for (const entry of next) {
      const existing = runtimes.get(entry.id);
      if (existing && sameSource(existing.entry, entry)) {
        existing.entry = entry;
        continue;
      }
      const wasActive = active === entry.id;
      if (existing) existing.live?.stop();
      const runtime = makeRuntime(entry, createClient, now());
      runtimes.set(entry.id, runtime);
      // A new address or token: what the view read from the old one must go,
      // before the new stream can answer. Without a token there is nothing to read.
      if (existing && watch?.fleet === entry.id) {
        if (runtime.client) watch.onReset?.();
        else endWatch();
      }
      if (wasActive) stream(entry.id);
    }
    order = next.map((entry) => entry.id);
  }

  reconcile(entries);

  return {
    sample: false,
    /** Fleets in order: {id, entry, state, connection, streaming}. */
    list() {
      return order.map((id) => this.get(id));
    },
    get(id) {
      const runtime = runtimes.get(id);
      if (!runtime) return null;
      return { id, entry: runtime.entry, state: runtime.state, connection: runtime.connection, streaming: runtime.streaming, lastReadAt: runtime.lastReadAt };
    },
    get active() {
      return active;
    },
    /** Make `id` the streamed fleet; the previous one goes back to polling. */
    activate(id) {
      if (stopped || id === active || !runtimes.has(id)) return;
      if (active) unstream(active);
      active = id;
      stream(id);
      schedule();
    },
    /** Polling runs only while the Fleets tier is visible (and once on load). */
    setLandscape(visible) {
      if (landscape === visible) return;
      landscape = visible;
      if (visible) check();
      else schedule();
    },
    /** No polling while the page is hidden; showing it again reads what is due. */
    setHidden(value) {
      if (hidden === value) return;
      hidden = value;
      if (hidden) schedule();
      else check();
    },
    /** Start: the one read on load for every fleet that is not streamed. */
    start() {
      check();
    },
    retry(id) {
      const runtime = runtimes.get(id);
      if (!runtime) return;
      if (runtime.streaming) runtime.live.retryNow();
      else poll(id);
    },
    readShip(id, ship) {
      return runtimes.get(id)?.live?.readShip(ship) ?? Promise.resolve(false);
    },
    readCrew(id, ship, crew) {
      return runtimes.get(id)?.live?.readCrew(ship, crew) ?? Promise.resolve(false);
    },
    /**
     * Follow one officer's activity on fleet `id`: its stream re-opens with
     * `?agent=<agent>` (never a second stream) and hands over that agent's
     * events. `onOpen` runs each time the stream opens with it, `onDown` when
     * that attempt is gone, `onRefused` when the gateway refuses the
     * subscription (the stream carries on without it), `onReset` when the
     * fleet's address or token changes (the view drops what it read and waits
     * for the new stream) and `onEnded` when the fleet loses its token or
     * goes. Returns a client whose reads always use the fleet's current
     * client, so none goes to an old address or with an old token, and
     * `stop()`, which re-opens the stream without the agent; or null when the
     * fleet has no client.
     */
    watchActivity(id, agent, { onEvent, onOpen, onDown, onRefused, onReset, onEnded }) {
      const runtime = runtimes.get(id);
      if (!runtime?.client) return null;
      const own = { fleet: id, agent, onEvent, onOpen, onDown, onRefused, onReset, onEnded };
      if (watch && watch.fleet !== id) runtimes.get(watch.fleet)?.live?.setAgents([]);
      watch = own;
      runtime.live?.setAgents([agent]);
      const current = () => {
        const client = watch === own ? runtimes.get(id)?.client : null;
        if (!client) throw new Error("The activity view no longer has a fleet to read.");
        return client;
      };
      return {
        client: {
          activity: async (...args) => current().activity(...args),
          activityItemPage: async (...args) => current().activityItemPage(...args),
        },
        stop() {
          if (watch !== own) return;
          watch = null;
          runtimes.get(id)?.live?.setAgents([]);
        },
      };
    },
    /** New entries from the sources: keeps unchanged fleets, rebuilds changed ones, reads new ones once. */
    update(next) {
      reconcile(next);
      check();
    },
    stop() {
      stopped = true;
      if (timer !== null) clearTimer(timer);
      timer = null;
      for (const runtime of runtimes.values()) runtime.live?.stop();
    },
    /** For tests: run the scheduler now. */
    checkNow: () => check(),
  };
}

/**
 * Sample mode's fleets: two simulations that keep changing and one fleet that
 * never answers, with the same interface as the live manager.
 */
export function createSampleFleetManager({ createSource, createActivity, onChange, now = Date.now }) {
  const runtimes = new Map();
  const order = SAMPLE_FLEETS.map((fleet) => fleet.id);
  let active = order[0];
  for (const fleet of SAMPLE_FLEETS) {
    const entry = { id: fleet.id, name: fleet.name ?? null, endpoint: fleet.endpoint, origin: "sample", tokenKey: null, token: null, problem: null };
    const runtime = { entry, state: initialFleetState(), connection: initialConnection(), source: null };
    runtime.connection = fleet.variant
      ? nextConnection(runtime.connection, { type: "sample", reason: "no-bridge" }, now())
      : { ...initialConnection(), status: "unreachable", since: now() };
    if (fleet.variant) {
      runtime.source = createSource({
        variant: fleet.variant,
        onEvent: (event) => {
          const next = reduceFleet(runtime.state, event, now());
          if (next === runtime.state) return;
          runtime.state = next;
          onChange?.(fleet.id, event);
        },
      });
    }
    runtimes.set(fleet.id, runtime);
  }
  return {
    sample: true,
    list() {
      return order.map((id) => this.get(id));
    },
    get(id) {
      const runtime = runtimes.get(id);
      return runtime ? { id, entry: runtime.entry, state: runtime.state, connection: runtime.connection, streaming: id === active, lastReadAt: null } : null;
    },
    get active() {
      return active;
    },
    activate(id) {
      if (runtimes.has(id)) active = id;
    },
    setLandscape() {},
    start() {
      for (const runtime of runtimes.values()) runtime.source?.start();
    },
    retry() {},
    readShip: () => Promise.resolve(true),
    readCrew: () => Promise.resolve(true),
    /** A simulated officer turn in place of the stream's activity events. */
    watchActivity(id, agent, { onEvent, onOpen }) {
      const fleet = runtimes.get(id)?.state.fleet;
      const officer = fleet?.admiral?.id === agent ? fleet.admiral : fleet?.ships.find((ship) => ship.captain.id === agent)?.captain;
      if (!createActivity || !officer) return null;
      const source = createActivity({ agent, role: officer.role, backend: officer.backend, onEvent, onOpen });
      source.start();
      return { client: source.client, stop: () => source.stop() };
    },
    update() {},
    stop() {
      for (const runtime of runtimes.values()) runtime.source?.stop();
    },
  };
}
