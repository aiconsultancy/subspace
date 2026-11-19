// Notifications for changes worth interrupting someone for: a crew task that
// becomes uncertain, and a ship that becomes unavailable. The first snapshot
// only sets the baseline, so opening the Space never notifies. Subspace accepts
// one notification every five seconds; alerts that arrive faster are merged.

import { allCrew } from "./model.js";

export const NOTIFY_SPACING_MS = 5200;

/** What alerts compare: uncertain crew and unavailable ships, by key. */
export function alertSnapshot(fleet, ships) {
  const snapshot = new Map();
  if (!fleet) return snapshot;
  for (const ship of fleet.ships) {
    snapshot.set(`ship:${ship.id}`, { available: ship.available, name: ship.name, error: ship.error ?? null });
  }
  for (const { ship, member, view } of allCrew(fleet, ships)) {
    snapshot.set(`crew:${ship.id}:${member.id}`, {
      uncertain: view.state === "uncertain",
      name: member.name,
      ship: ship.name,
      task: view.task?.summary ?? view.lastOutcome?.summary ?? null,
    });
  }
  return snapshot;
}

/** Alerts for keys that turned bad between two snapshots. */
export function diffAlerts(previous, next) {
  const alerts = [];
  for (const [key, now] of next) {
    const before = previous.get(key);
    if (!before) continue;
    if (key.startsWith("ship:") && before.available && !now.available) {
      alerts.push({
        key,
        title: `${now.name} is unavailable`,
        body: now.error ? `Fleet Console cannot read its crew: ${now.error}` : "Fleet Console cannot read its crew.",
      });
    } else if (key.startsWith("crew:") && !before.uncertain && now.uncertain) {
      alerts.push({
        key,
        title: `${now.name} has no confirmed outcome`,
        body: `${now.ship}${now.task ? `: “${now.task}”` : ""}. Stored state does not prove the task is still running.`,
      });
    }
  }
  return alerts;
}

const clip = (text, max) => (text.length <= max ? text : `${text.slice(0, max - 1)}…`);

/** Merge several alerts into one notification. */
export function mergeAlerts(alerts) {
  if (alerts.length === 1) return { title: clip(alerts[0].title, 100), body: clip(alerts[0].body, 500) };
  return {
    title: clip(`${alerts.length} fleet alerts`, 100),
    body: clip(alerts.map((alert) => alert.title).join(" · "), 500),
  };
}

/**
 * Sends merged alerts no faster than one per NOTIFY_SPACING_MS.
 * `send({title, body})` returns a promise; failures are dropped quietly.
 */
export function createNotifier({ send, now = Date.now, setTimer = setTimeout }) {
  let pending = [];
  let lastSent = -Infinity;
  let timer = null;

  function flush() {
    timer = null;
    if (pending.length === 0) return;
    const message = mergeAlerts(pending);
    pending = [];
    lastSent = now();
    Promise.resolve()
      .then(() => send(message))
      .catch(() => {});
  }

  return {
    push(alerts) {
      if (!alerts.length) return;
      pending.push(...alerts);
      if (timer !== null) return;
      const wait = Math.max(0, lastSent + NOTIFY_SPACING_MS - now());
      timer = setTimer(flush, wait);
    },
  };
}

/** Holds the baseline and turns state changes into notifier pushes. */
export function createAlertWatcher(notifier) {
  let baseline = null;
  return {
    /** Call with each new state once the first full snapshot has arrived. */
    observe(fleet, ships) {
      const snapshot = alertSnapshot(fleet, ships);
      if (baseline === null) {
        baseline = snapshot;
        return [];
      }
      const alerts = diffAlerts(baseline, snapshot);
      // Keep keys that vanished, so a ship that disappears and returns broken still alerts.
      baseline = new Map([...baseline, ...snapshot]);
      notifier.push(alerts);
      return alerts;
    },
    reset() {
      baseline = null;
    },
  };
}
