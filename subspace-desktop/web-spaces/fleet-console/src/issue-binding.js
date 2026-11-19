// Ties issue references to a source of answers: the bridge's issues contract
// in live mode, the sample answers in sample mode, or nothing when the
// permission or a board is missing (then every query quietly returns empty).

import { createIssueResolver, extractIdentifiers, issueStateOf } from "./issues.js";
import { issueSources } from "./model.js";
import { SAMPLE_SCOPE, sampleResolve } from "./sample.js";

/** `getBridge()` returns the connected bridge (it is set up after this binding). */
export function createIssueBinding({ getBridge, onChange }) {
  let enabled = false;
  let scope = null;
  let resolver = null;
  let canOpen = false;
  let cache = new WeakMap();

  return {
    get enabled() {
      return enabled;
    },
    get canOpen() {
      return canOpen;
    },
    get scope() {
      return scope;
    },
    /** Choose the answer source for `sample` or live mode. */
    async setup(sample) {
      resolver?.clear();
      cache = new WeakMap();
      enabled = false;
      resolver = null;
      scope = null;
      if (sample) {
        scope = SAMPLE_SCOPE;
        resolver = createIssueResolver({ resolve: async (ids) => sampleResolve(ids), onChange });
        canOpen = true;
        enabled = true;
        return onChange();
      }
      const bridge = getBridge();
      if (bridge?.issues.available()) {
        const answer = await bridge.issues.scope();
        if (answer && Array.isArray(answer.teamKeys) && answer.teamKeys.length) {
          scope = answer;
          resolver = createIssueResolver({ resolve: (ids) => bridge.issues.resolve(ids), onChange });
          canOpen = bridge.issues.canOpen();
          enabled = true;
        }
      }
      onChange();
    },
    /** A crew member's identifiers, in order; asks the resolver for new ones. Cached per member object. */
    idsFor(member) {
      if (!enabled || !member) return [];
      const cached = cache.get(member);
      if (cached) return cached;
      const ids = extractIdentifiers(issueSources(member), scope);
      cache.set(member, ids);
      if (ids.length) resolver.want(ids);
      return ids;
    },
    get(id) {
      return resolver?.get(id) ?? null;
    },
    /** The canvas badge for a member: its first linked issue, toned by state; hidden when the board has no such issue. */
    badgeFor(member) {
      const id = this.idsFor(member)[0];
      if (!id) return null;
      const answer = resolver.get(id);
      if (answer && answer.state !== "available" && answer.state !== "pending") return null;
      return { identifier: id, tone: answer?.state === "available" ? issueStateOf(answer.summary?.state?.kind).tone : "muted" };
    },
    refresh() {
      resolver?.refresh();
    },
  };
}
