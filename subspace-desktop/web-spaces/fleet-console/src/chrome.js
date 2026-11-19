// The window chrome around the map: breadcrumb, toolbar stats, connection
// pill and banner, the tier stack, the activity ticker, tooltip and toast.
// Everything API-derived is escaped by html.js before it reaches innerHTML.

import { formatClock, formatDuration, formatUptime } from "./format.js";
import { fleetStats, lookup } from "./model.js";
import { esc, goAttrs, since } from "./html.js";

const $ = (id) => document.getElementById(id);

/** The breadcrumb's HTML. Pure, for tests. */
export function crumbsHtml(fleet, ships, selection, fleetName, landscape = false) {
  const { ship, member } = lookup(fleet, ships, selection);
  const sep = '<span class="sep" aria-hidden="true">›</span>';
  let html = "";
  if (landscape) {
    html += selection.level === -1 ? '<span class="crumb cur" aria-current="page"><b>Fleets</b></span>' : '<button class="crumb" type="button" data-go="fleets"><b>Fleets</b></button>';
  }
  if (selection.level >= 0) {
    const label = landscape ? `<b>${esc(fleetName)}</b>` : `Fleet <b>${esc(fleetName)}</b>`;
    if (landscape) html += sep;
    html += selection.level === 0 ? `<span class="crumb cur" aria-current="page">${label}</span>` : `<button class="crumb" type="button" ${goAttrs({ level: 0, fleet: selection.fleet })}>${label}</button>`;
  }
  if (selection.level >= 1) {
    const shipName = ship?.name ?? selection.ship;
    html += sep;
    html += selection.level === 1 ? `<span class="crumb cur" aria-current="page">${esc(shipName)}</span>` : `<button class="crumb" type="button" ${goAttrs({ level: 1, fleet: selection.fleet, ship: selection.ship })}>${esc(shipName)}</button>`;
  }
  if (selection.level === 2) html += `${sep}<span class="crumb cur" aria-current="page">${esc(member?.name ?? selection.crew)}</span>`;
  return html;
}

/** The tier stack's HTML: L0 Fleets only when there are two or more fleets. Pure, for tests. */
export function depthHtml(selection, landscape = false) {
  const tiers = [
    ...(landscape ? [{ level: -1, name: "Fleets", c4: "landscape", ok: true }] : []),
    { level: 0, name: "Fleet", c4: "context", ok: selection.level >= 0 || !landscape },
    { level: 1, name: "Ship", c4: "containers", ok: Boolean(selection.ship) },
    { level: 2, name: "Crew", c4: "components", ok: Boolean(selection.crew) },
  ];
  return tiers
    .map((tier) => {
      const current = tier.level === selection.level;
      const fleet = selection.fleet;
      const target = tier.level === -1 ? { level: -1 } : tier.level === 0 ? { level: 0, fleet } : tier.level === 1 ? { level: 1, fleet, ship: selection.ship } : { level: 2, fleet, ship: selection.ship, crew: selection.crew };
      const attrs = tier.ok && !current ? goAttrs(target) : "disabled";
      return `<button type="button" class="tier${current ? " on" : ""}" ${attrs} ${current ? 'aria-current="true"' : ""} aria-label="${tier.name} tier, L${tier.level + 1} ${tier.c4}"><span class="pl"><i style="--d:${tier.level}"></i></span><span><b>${tier.name}</b><em>L${tier.level + 1} · ${tier.c4}</em></span></button>`;
    })
    .join("");
}

export function createChrome() {
  const el = {
    crumbs: $("crumbs"),
    stats: $("stats"),
    conn: $("conn"),
    connLabel: $("conn-label"),
    banner: $("banner"),
    bannerTitle: $("banner-title"),
    bannerBody: $("banner-body"),
    retry: $("retry"),
    depth: $("depth"),
    hint: $("hint"),
    sampleChip: $("sample-chip"),
    ticker: $("ticker"),
    tickerLabel: $("ticker-label"),
    tip: $("tip"),
    toast: $("toast"),
    announce: $("announce"),
    inspector: $("inspector"),
    scene: $("scene"),
  };
  let toastTimer = 0;
  let lastTickerKey = "";
  let lastStats = "";
  let lastCrumbs = "";
  let lastDepth = "";

  return {
    el,
    /** `Fleets › <fleet> › <ship> › <crew>`, or `Fleet <name> › …` with one fleet. */
    crumbs(fleet, ships, selection, fleetName, landscape = false) {
      const html = crumbsHtml(fleet, ships, selection, fleetName, landscape);
      if (html !== lastCrumbs) {
        el.crumbs.innerHTML = html;
        lastCrumbs = html;
      }
    },
    /** Totals across fleets, for the Fleets tier. */
    landscapeStats(totals) {
      const html = `<span class="stat"><em>Fleets</em><b>${totals.answering}/${totals.fleets}</b></span><span class="stat"><em>Ships</em><b>${totals.shipsOnline}/${totals.shipsTotal}</b></span><span class="stat"><em>Crew busy</em><b>${totals.busy}/${totals.people}</b></span>`;
      if (html !== lastStats) {
        el.stats.innerHTML = html;
        lastStats = html;
      }
    },
    stats(fleet, ships, now) {
      const stats = fleetStats(fleet, ships);
      const html = stats
        ? `<span class="stat"><em>Ships</em><b>${stats.shipsOnline}/${stats.shipsTotal}</b></span><span class="stat"><em>Crew busy</em><b>${stats.busy}/${stats.people}</b></span><span class="stat optional"><em>Queued</em><b>${stats.queued}</b></span>${stats.uncertain ? `<span class="stat optional"><em>Uncertain</em><b>${stats.uncertain}</b></span>` : ""}${stats.longestIdle ? `<span class="stat optional"><em>Longest idle</em><b>${since(stats.longestIdle.since, now)}</b></span>` : ""}`
        : "";
      const key = html.replace(/<span data-since[^>]*>[^<]*<\/span>/g, "");
      if (key !== lastStats) {
        el.stats.innerHTML = html;
        lastStats = key;
      }
    },
    connection(description, sample) {
      el.conn.dataset.tone = description.tone;
      el.connLabel.textContent = description.label;
      el.sampleChip.hidden = !sample;
      el.tickerLabel.textContent = sample ? "Sample · simulated events" : "Live · /api/v1/events";
      const banner = description.banner;
      el.banner.hidden = !banner;
      if (banner) {
        el.banner.dataset.tone = description.tone === "red" ? "red" : "yellow";
        el.bannerTitle.textContent = `${banner.title}.`;
        el.bannerBody.textContent = banner.body;
        el.retry.hidden = !banner.retry;
      }
    },
    depth(selection, landscape = false) {
      const html = depthHtml(selection, landscape);
      if (html !== lastDepth) {
        el.depth.innerHTML = html;
        lastDepth = html;
      }
      el.hint.textContent = [
        "Click a fleet to warp in",
        landscape ? "Click a ship to warp in · Esc pulls out" : "Click a ship to warp in",
        "Click a crew member to warp in · Esc pulls out",
        "Esc pulls out · hover the 24-hour ring",
      ][selection.level + 1];
    },
    ticker(activity, fleet, fresh) {
      const key = activity.map((entry) => `${entry.at}:${entry.text}`).join("|");
      if (key === lastTickerKey) return;
      lastTickerKey = key;
      const nameOf = (id) => fleet?.ships.find((ship) => ship.id === id)?.name ?? id;
      el.ticker.innerHTML = activity
        .slice(0, 14)
        .map((entry, i) => `<li class="${fresh && i === 0 ? "new" : ""}"><time>${esc(formatClock(entry.at))}</time><b>${esc(nameOf(entry.ship))}</b><span>${esc(entry.text)}</span>${entry.branch ? `<code>${esc(entry.branch)}</code>` : ""}</li>`)
        .join("");
    },
    inspector(html, keepScroll) {
      const top = el.inspector.scrollTop;
      el.inspector.innerHTML = html;
      el.inspector.scrollTop = keepScroll ? top : 0;
    },
    showTip(html, x, y, W, H) {
      if (!html) {
        el.tip.hidden = true;
        return;
      }
      el.tip.innerHTML = html;
      el.tip.hidden = false;
      const tw = el.tip.offsetWidth;
      const th = el.tip.offsetHeight;
      el.tip.style.left = `${Math.max(8, Math.min(W - tw - 10, x + 16))}px`;
      el.tip.style.top = `${y + 16 + th > H - 8 ? Math.max(8, y - th - 12) : y + 16}px`;
    },
    hideTip() {
      el.tip.hidden = true;
    },
    toast(message) {
      el.toast.textContent = message;
      el.toast.hidden = false;
      clearTimeout(toastTimer);
      toastTimer = setTimeout(() => {
        el.toast.hidden = true;
      }, 2800);
    },
    announce(message) {
      el.announce.textContent = message;
    },
    /** Refresh every relative time on the page. */
    tickTimes(now) {
      for (const node of document.querySelectorAll("[data-since]")) node.textContent = formatDuration(now - Number(node.dataset.since));
      for (const node of document.querySelectorAll("[data-uptime]")) node.textContent = formatUptime((now - Number(node.dataset.uptime)) / 1000);
    },
  };
}
