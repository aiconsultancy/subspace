// The officer Activity view's lifecycle in the inspector (SUB decision
// 1c19f413 §6): the stream subscription, the watcher, the panel, the frame
// it redraws in and the teardown, in one place. The shell (main.js) keeps
// selection, focus and its own wiring: it asks to open or close the view
// and hears when it closed.
//
// One view is open at a time. Every callback is tied to the view it was
// made for, so one that arrives after that view closed (a stream event, a
// read settling, a frame) does nothing. Closing stops the subscription, so
// the fleet's stream re-opens without the agent, stops the watcher, which
// aborts its reads and drops what it read, cancels the frame and disposes
// of the panel.

import { createActivityWatcher } from "./activity.js";
import { createActivityPanel } from "./activity-view.js";

/**
 * @param {object} options
 * @param {{replaceChildren: Function, querySelector: Function, scrollTop: number}} options.inspector  where the view is shown
 * @param {Document} [options.doc]
 * @param {(agent: string, hooks: object) => ({client: object, stop: () => void}|null|undefined)} options.watch
 *   subscribe to `agent`'s activity on the fleet's stream; null when the fleet has no token
 * @param {(subscribed: boolean) => boolean} options.live  whether the view is live (its stream connected)
 * @param {(words: string) => void} options.toast
 * @param {(words: string) => void} options.announce
 * @param {() => void} [options.onDrawn]  after each draw (the shell ticks its clocks)
 * @param {(agent: string, restore: boolean) => void} [options.onClosed]  after the view closed;
 *   `restore` when the shell should draw what the view replaced and put focus back
 * @param {(draw: () => void) => number} [options.frame]
 * @param {(id: number) => void} [options.cancelFrame]
 * @param {typeof createActivityWatcher} [options.createWatcher]
 * @param {typeof createActivityPanel} [options.createPanel]
 */
export function createActivityController({
  inspector,
  doc = globalThis.document,
  watch,
  live,
  toast,
  announce,
  onDrawn = () => {},
  onClosed = () => {},
  frame = (draw) => globalThis.requestAnimationFrame(draw),
  cancelFrame = (id) => globalThis.cancelAnimationFrame(id),
  createWatcher = createActivityWatcher,
  createPanel = createActivityPanel,
}) {
  /** The open view: {agent, name, role, handle, watcher, panel}, or null. */
  let view = null;
  /** The frame the next draw is asked for in, or 0. */
  let pending = 0;

  function cancel() {
    if (pending) cancelFrame(pending);
    pending = 0;
  }

  /** Draw the open view now. */
  function draw() {
    cancel();
    if (!view) return;
    const state = view.watcher.view;
    // Nothing to follow on this gateway: give the stream back its plain form.
    if (state.status === "unavailable" && view.handle) {
      view.handle.stop();
      view.handle = null;
    }
    view.panel.update(state, { name: view.name, role: view.role, live: live(view.handle !== null) });
    onDrawn();
  }

  /** Close the open view, if any. `restore`: the shell draws what it replaced and puts focus back. */
  function close(restore = false) {
    const closing = view;
    if (!closing) return;
    view = null;
    cancel();
    closing.handle?.stop();
    closing.handle = null;
    closing.watcher?.stop();
    closing.panel?.dispose();
    onClosed(closing.agent, restore);
  }

  /**
   * Open the view for an officer (registration ID `agent`), closing the
   * one open. Its events ride on the fleet's one stream, re-opened with
   * `?agent=`; it is never a second stream. Returns whether it opened.
   */
  function open(agent, name, role) {
    if (!agent) return false;
    close(false);
    const owner = { agent, name: name || agent, role, handle: null, watcher: null, panel: null };
    // Callbacks for a view that has since closed do nothing.
    const mine = () => view === owner;
    owner.handle = watch(agent, {
      onEvent: (data) => mine() && owner.watcher.event(data),
      onOpen: () => mine() && owner.watcher.reopened(),
      onDown: () => mine() && owner.watcher.disconnected(),
      onRefused: () => mine() && owner.watcher.refused(),
      // The fleet's address or token changed: start again from the new stream.
      onReset: () => mine() && owner.watcher.reset(),
      // The fleet lost its token or went: nothing read from it stays on show.
      onEnded: () => {
        if (!mine()) return;
        owner.handle = null;
        close(true);
        toast("Activity closed: the fleet's address or token changed.");
      },
    });
    if (!owner.handle) {
      owner.handle = null;
      toast("Activity needs the fleet's token.");
      return false;
    }
    owner.watcher = createWatcher({
      client: owner.handle.client,
      agent,
      onChange: () => {
        if (mine() && !pending) pending = frame(draw);
      },
    });
    owner.panel = createPanel({
      doc,
      onClose: () => mine() && close(true),
      onLoad: (turn, item) => mine() && owner.watcher.loadItem(turn, item),
      onPart: (turn, item, to) => mine() && owner.watcher.loadPart(turn, item, to),
      onRetry: () => mine() && owner.watcher.retry(),
    });
    view = owner;
    inspector.replaceChildren(owner.panel.root);
    inspector.scrollTop = 0;
    owner.watcher.start();
    draw();
    inspector.querySelector("h2")?.focus({ preventScroll: true });
    announce(`Activity for ${owner.name}`);
    return true;
  }

  return {
    open,
    close,
    draw,
    /** Whether a view is open. */
    get isOpen() {
      return view !== null;
    },
    /** The open view's officer, or null. */
    get agent() {
      return view?.agent ?? null;
    },
  };
}
