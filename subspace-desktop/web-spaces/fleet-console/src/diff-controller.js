// The crew diff view's lifecycle in the inspector, as activity-controller.js
// is the Activity view's: the fleet's diff handle, the watcher, the panel,
// the frame it redraws in and the teardown, in one place. The shell
// (main.js) keeps selection and its own wiring: it asks to open or close
// the view and hears when it opens and closed; the controller puts focus
// back on the View diff button the view was opened from.
//
// One view is open at a time. Closing cancels the frame, stops the watcher,
// which cancels reads still in flight or waiting to retry, drops the content
// and gives back what it held of the console's budget, stops the handle and
// destroys the panel.

import { createDiffWatcher } from "./diff.js";
import { createDiffPanel } from "./diff-view.js";

/**
 * @param {object} options
 * @param {{replaceChildren: Function, querySelector: Function, scrollTop: number}} options.inspector  where the view is shown
 * @param {Document} [options.doc]
 * @param {(hooks: object) => ({client: object, stop: () => void}|null|undefined)} options.open
 *   the fleet's diff handle; null when the fleet has no token
 * @param {() => Record<string, {crewMembers: object[]}>} options.ships
 *   the ships of the fleet you are inside, for the crew member's name and the project's live fingerprint
 * @param {(words: string) => void} options.toast
 * @param {(words: string) => void} options.announce
 * @param {() => void} [options.onOpening]  before a view opens (the shell closes the Activity view)
 * @param {(restore: boolean) => void} [options.onClosed]  after the view closed; `restore` when the shell
 *   should draw what the view replaced, before focus goes back to the View diff button
 * @param {(draw: () => void) => number} [options.frame]
 * @param {(id: number) => void} [options.cancelFrame]
 * @param {typeof createDiffWatcher} [options.createWatcher]
 * @param {typeof createDiffPanel} [options.createPanel]
 * @param {ReturnType<import("./budget.js").createBudget>|null} [options.budget]  the console's budget, shared
 *   with the Activity view (SUB decision 75c3ed93 §3); each view has its own scope of it. A view of its own when null.
 */
export function createDiffController({
  inspector,
  doc = globalThis.document,
  open: openHandle,
  ships,
  toast,
  announce,
  onOpening = () => {},
  onClosed = () => {},
  frame = (draw) => globalThis.requestAnimationFrame(draw),
  cancelFrame = (id) => globalThis.cancelAnimationFrame(id),
  createWatcher = createDiffWatcher,
  createPanel = createDiffPanel,
  budget = null,
}) {
  /** The open view: {ship, member, project, name, handle, watcher, panel}, or null. */
  let view = null;
  /** The frame the next draw is asked for in, or 0. */
  let pending = 0;

  function cancel() {
    if (pending) cancelFrame(pending);
    pending = 0;
  }

  /** The crew member and the project context a view shows. */
  function locate(ship, memberId, project) {
    const member = ships()[ship]?.crewMembers.find((entry) => entry.id === memberId);
    return { member, context: member?.contexts.find((entry) => (entry.project ?? null) === project) ?? null };
  }

  /** Draw the open view now. */
  function draw() {
    cancel();
    if (!view) return;
    view.panel.update(view.watcher.view, { name: view.name });
  }

  /** Pass the checkout's live fingerprint, from the `ship` event, to the view. */
  function observe() {
    if (view) view.watcher.observe(locate(view.ship, view.member, view.project).context?.git?.fingerprint);
  }

  /** Close the open view, if any. `restore`: the shell draws what it replaced and puts focus back. */
  function close(restore = false) {
    const closing = view;
    if (!closing) return;
    view = null;
    cancel();
    closing.watcher?.stop();
    closing.handle?.stop();
    closing.panel?.destroy();
    onClosed(restore);
    if (!restore) return;
    [...inspector.querySelectorAll("[data-diff-member]")]
      .find((button) => button.dataset.diffShip === closing.ship && button.dataset.diffMember === closing.member && (button.dataset.diffProject || null) === closing.project)
      ?.focus();
  }

  /**
   * Open the diff of one crew member's checkout of `project` (null for the
   * default project), closing the one open. Reads go to the fleet's current
   * client; content stays in memory while the view is open. Returns whether
   * it opened.
   */
  function open(ship, memberId, project) {
    if (!ship || !memberId) return false;
    onOpening();
    close(false);
    const owner = { ship, member: memberId, project, name: memberId, handle: null, watcher: null, panel: null };
    owner.name = locate(ship, memberId, project).member?.name ?? memberId;
    owner.handle = openHandle({
      onReset: () => owner.watcher?.reset(),
      onEnded: () => {
        if (view !== owner) return;
        owner.handle = null;
        close(true);
        toast("Diff closed: the fleet's address or token changed.");
      },
    });
    if (!owner.handle) {
      toast("Diffs need the fleet's token.");
      return false;
    }
    owner.watcher = createWatcher({
      client: owner.handle.client,
      target: { ship, member: memberId, project },
      ...(budget ? { budget: budget.scope("diff\u0000") } : {}),
      onChange: () => {
        if (!pending) pending = frame(draw);
      },
    });
    owner.panel = createPanel({ doc, watcher: owner.watcher, onClose: () => close(true) });
    view = owner;
    inspector.replaceChildren(owner.panel.root);
    inspector.scrollTop = 0;
    owner.watcher.start();
    observe();
    draw();
    inspector.querySelector("h2")?.focus({ preventScroll: true });
    announce(`Diff of ${project ?? "the default project"} for ${owner.name}`);
    return true;
  }

  return {
    open,
    close,
    draw,
    observe,
    /** Whether a view is open. */
    get isOpen() {
      return view !== null;
    },
  };
}
