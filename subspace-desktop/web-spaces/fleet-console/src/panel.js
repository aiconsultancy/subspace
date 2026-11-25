// The inspector view: decides when the panel's HTML really changed, keeps the
// scroll position within a tier, and keeps keyboard focus where the person
// left it across re-renders and tier changes.

import { esc } from "./html.js";

/** HTML with relative times blanked, so a ticking clock alone is not a change. */
export const stripTimes = (html) => html.replace(/(<span data-(?:since|uptime)="[^"]*">)[^<]*/g, "$1");

/** A stable description of a focusable control, to find it again after a re-render. */
export function focusSignature(element) {
  if (element?.dataset?.go) return `go|${element.dataset.go}|${element.dataset.ship ?? ""}|${element.dataset.crew ?? ""}|${element.dataset.fleet ?? ""}`;
  if (element?.dataset?.issue) return `issue|${element.dataset.issue}`;
  if (element?.dataset?.activity) return `activity|${element.dataset.activity}`;
  return null;
}

/** The signature of the inspector row that navigates to `selection`. */
export function selectionSignature(selection) {
  if (selection.level === 0 && selection.fleet) return `go|fleet|||${selection.fleet}`;
  if (selection.level === 1) return `go|ship|${selection.ship}||*`;
  if (selection.level === 2) return `go|crew|${selection.ship}|${selection.crew}|*`;
  return null;
}

function findBySignature(container, signature) {
  const [kind, a, b, c, d = ""] = signature.split("|");
  const fleetMatches = (node) => d === "*" || (node.dataset.fleet ?? "") === d;
  const selector = { go: "[data-go]", issue: "[data-issue]", activity: "[data-activity]" }[kind];
  for (const node of container.querySelectorAll(selector)) {
    if (kind === "activity") {
      if (node.dataset.activity === a) return node;
      continue;
    }
    if (kind === "issue" ? node.dataset.issue === a : node.dataset.go === a && (node.dataset.ship ?? "") === b && (node.dataset.crew ?? "") === c && fleetMatches(node)) return node;
  }
  return null;
}

/** The panel shown before any fleet data has arrived. */
export function waitingPanel(description, endpoint) {
  const body = description.banner
    ? `<p class="prog"><b>${esc(description.banner.title)}.</b> ${esc(description.banner.body)}</p>`
    : '<p class="prog">Reading the fleet from Stargate…</p>';
  return `<header class="ihead"><p class="eyebrow">Fleet · L1 · context</p><h2 tabindex="-1">Fleet Console</h2>${endpoint ? `<p class="meta mono">${esc(endpoint)}</p>` : ""}</header><section class="sec">${body}</section>`;
}

/**
 * @param {{inspector: HTMLElement, regions: HTMLElement[], write: (html: string, keepScroll: boolean) => void}} options
 * `regions` are the other places navigation controls live (breadcrumb, tier
 * stack); focus that was in them moves to the inspector heading when they redraw.
 */
export function createInspectorView({ inspector, regions, write }) {
  let shownKey = "";
  let shownHtml = "";
  const within = (element) => inspector.contains(element) || regions.some((region) => region.contains(element));

  return {
    /** Capture focus before the toolbar and tier stack redraw for a tier change. */
    captureFocus() {
      const active = document.activeElement;
      return { signature: inspector.contains(active) ? focusSignature(active) : null, navigating: within(active) && active !== document.body };
    },
    /**
     * Show `html` for tier `key`. On navigation, focus goes back to the same
     * control if it still exists, else to the panel heading.
     */
    show(html, key, navigated, focus = this.captureFocus()) {
      if (!navigated && key === shownKey && stripTimes(html) === shownHtml) return false;
      write(html, key === shownKey && !navigated);
      shownKey = key;
      shownHtml = stripTimes(html);
      const again = focus.signature ? findBySignature(inspector, focus.signature) : null;
      if (again) again.focus();
      else if (focus.navigating && (navigated || document.activeElement === document.body)) inspector.querySelector("h2")?.focus({ preventScroll: true });
      return true;
    },
  };
}
