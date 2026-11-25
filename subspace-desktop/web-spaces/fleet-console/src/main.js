// Fleet Console: page wiring. Resolves the fleets (the Settings fleet, the
// page's own, later the Subspace API's) or sample mode, feeds each fleet's
// events through its reducer, draws frames on demand with WebGL2 (or
// Canvas 2D), and keeps the inspector, chrome and Subspace in step.

import { connectBridge } from "./bridge.js";
import { createApiClient, normalizeEndpoint } from "./api.js";
import { createLiveSource } from "./live.js";
import { createSampleActivitySource, createSampleSource } from "./sample-source.js";
import { createActivityWatcher } from "./activity.js";
import { createActivityPanel } from "./activity-view.js";
import { initialFleetState, livePackets } from "./reducer.js";
import { describeConnection, initialConnection, tokenProblem } from "./connection.js";
import { chain, createNavigator, fleetTier, LANDSCAPE, parentOf, sameSelection } from "./camera.js";
import { composeFrame } from "./scene/index.js";
import { fleetStatus } from "./scene/landscape.js";
import { createStars, stepStars } from "./stars.js";
import { createGlRenderer } from "./gl/renderer.js";
import { createCanvasRenderer } from "./canvas2d.js";
import { canvasFallback, createSceneRenderer } from "./renderers.js";
import { createOverlay } from "./overlay.js";
import { readPalette, themeToCssVars } from "./theme.js";
import { REFRESH_MS } from "./issues.js";
import { createIssueBinding } from "./issue-binding.js";
import { allCrew, lookup } from "./model.js";
import { createAlertWatcher, createNotifier, createWatcherSet } from "./alerts.js";
import { coalesce } from "./coalesce.js";
import { crewContext, describeLandscape, describeTier } from "./effects.js";
import { createChrome } from "./chrome.js";
import { crewPanel, fleetPanel, shipPanel, tooltip } from "./inspector.js";
import { createInspectorView, selectionSignature, waitingPanel } from "./panel.js";
import { ambientFps, createGovernor, LEVELS } from "./quality.js";
import { createFrameStats, createPerfToggle, hudLines, isPerfToggleKey, mountHud } from "./perf.js";
import { createFleetManager, createSampleFleetManager } from "./fleet-manager.js";
import { fleetName, resolveFleets } from "./fleet-sources.js";
import { browserStorage, createFleetStore } from "./fleet-store.js";
import { addBlocker, addFleetBar, fleetSection, landscapePanel, unreadableFleetPanel } from "./fleet-panel.js";
import { createFleetForm } from "./fleet-form.js";
import { esc } from "./html.js";

const params = new URLSearchParams(location.search);
const preview = { theme: params.get("theme"), motion: params.get("motion"), renderer: params.get("renderer"), perf: params.get("perf") === "1" };
// `?quality=N` pins a quality level (0 is the full look) and turns the governor off.
const pinnedLevel = params.has("quality") ? Math.max(0, Math.min(LEVELS.length - 1, Math.round(Number(params.get("quality")) || 0))) : null;
// `?slowmo=20` runs animations twenty times slower, for checking the warp frame by frame.
const slowmo = Math.min(100, Math.max(1, Number(params.get("slowmo")) || 1));
const clock = () => performance.now() / slowmo;
const root = document.documentElement;
const chrome = createChrome();
const stage = document.getElementById("stage");
const labels = document.getElementById("labels");
const reducedQuery = matchMedia("(prefers-reduced-motion: reduce)");
const darkQuery = matchMedia("(prefers-color-scheme: dark)");
const store = createFleetStore(browserStorage());

const app = {
  bridge: null,
  manager: null,
  mode: null,
  reason: null,
  sample: true,
  blocker: null,
  motionSetting: "Full",
  // The active fleet's reducer state and connection: the fleet you are inside.
  state: initialFleetState(),
  connection: initialConnection(),
  palette: null,
  hover: null,
  focusHover: null,
  hits: [],
  W: 800,
  H: 560,
  dpr: 1,
  badge: undefined,
  contextKey: "",
  lastSel: fleetTier(null),
  freshActivity: false,
  confirmRemove: null,
  /** The open Activity view: {agent, name, role, handle, watcher, panel}, or null. */
  activity: null,
  activityFrame: 0,
};

// Read once and kept current: the media query's getter is costly in a frame loop.
let systemReduced = reducedQuery.matches;
const reduced = () => systemReduced || app.motionSetting === "Reduced" || preview.motion === "reduced";
const serverNow = () => Date.now() + app.state.skew;
/** The Fleets tier exists with two or more fleets. */
const landscape = () => (app.manager?.list().length ?? 0) >= 2;
const nav = createNavigator({ reduced, start: clock(), landscape });
const governor = createGovernor({ pinned: pinnedLevel });
const stats = createFrameStats();
const perfToggle = createPerfToggle({ forced: preview.perf });
/** Ambient motion slows down after this long without pointer or keyboard input. */
const IDLE_AFTER_MS = 45_000;
let lastInput = performance.now();
const issues = createIssueBinding({
  getBridge: () => app.bridge,
  onChange: () => {
    markInspector();
    requestRender();
  },
});

// ---------- theme ----------

function refreshPalette() {
  const style = getComputedStyle(root);
  app.palette = readPalette((name) => style.getPropertyValue(name));
  renderer.setPalette(app.palette);
  overlay.setPalette(app.palette);
  requestRender();
}

function applyTheme(theme) {
  const mapped = themeToCssVars(theme);
  if (!mapped) return;
  for (const [name, value] of Object.entries(mapped.vars)) root.style.setProperty(name, value);
  root.dataset.mode = mapped.mode;
  refreshPalette();
}

if (preview.theme === "light") root.dataset.preview = "light";
else if (preview.theme === "dark") root.dataset.mode = "dark";
darkQuery.addEventListener?.("change", () => refreshPalette());

// ---------- renderers ----------

/** A fresh canvas in place of `canvas`, which a WebGL2 context has claimed. */
function replaceCanvas(canvas) {
  const replacement = canvas.cloneNode(false);
  canvas.replaceWith(replacement);
  return replacement;
}

let { renderer, canvas: sceneCanvas } = createSceneRenderer({
  canvas: chrome.el.scene,
  preferGl: preview.renderer !== "2d",
  createGl: (canvas) => createGlRenderer(canvas, { onLost: fallbackToCanvas, onRestored: () => requestRender() }),
  createCanvas: createCanvasRenderer,
  replaceCanvas,
});
const overlay = createOverlay(labels);
let stars = createStars(renderer.kind === "webgl2" ? 380 : 260);
root.dataset.renderer = renderer.kind;

function fallbackToCanvas() {
  if (renderer.kind !== "webgl2") return;
  ({ renderer, canvas: sceneCanvas } = canvasFallback({ renderer, canvas: sceneCanvas, createCanvas: createCanvasRenderer, replaceCanvas }));
  stars = createStars(260);
  root.dataset.renderer = renderer.kind;
  renderer.setQuality(governor.settings);
  renderer.resize(app.W, app.H, app.dpr);
  if (app.palette) renderer.setPalette(app.palette);
  updateCanvasLabel();
  requestRender();
}

/** The tier stack's box on the stage, and the frame-time readout's when shown: the Fleets tier's labels keep clear of them. */
function measureAvoid() {
  const box = stage.getBoundingClientRect();
  app.avoid = [chrome.el.depth, stage.querySelector(".perf-hud:not([hidden])")].flatMap((element) => {
    const rect = element?.getBoundingClientRect();
    return rect?.width && rect.height ? [{ x0: rect.left - box.left - 8, y0: rect.top - box.top - 8, x1: rect.right - box.left + 8, y1: rect.bottom - box.top + 8 }] : [];
  });
}

new ResizeObserver(() => {
  const box = stage.getBoundingClientRect();
  app.W = Math.max(1, box.width);
  app.H = Math.max(1, box.height);
  app.dpr = Math.min(2, window.devicePixelRatio || 1);
  renderer.resize(app.W, app.H, app.dpr);
  overlay.resize(app.W, app.H, app.dpr);
  measureAvoid();
  requestRender();
}).observe(stage);
const avoidObserver = new ResizeObserver(() => {
  measureAvoid();
  requestRender();
});
avoidObserver.observe(chrome.el.depth);

// ---------- frame loop ----------

let rafId = 0;
let lastDraw = clock();
let lastDrawReal = performance.now();
let lastCallback = null;
let overlayOnly = false;
let ambientFrames = 0;
let lastGpu = null;

for (const type of ["pointermove", "pointerdown", "keydown", "wheel"]) {
  addEventListener(type, () => {
    lastInput = performance.now();
  }, { passive: true, capture: true });
}


// ---------- fleets ----------

const fleetView = (fleet) => ({ ...fleet, name: fleetName(fleet.entry, fleet.state.fleet) });
const fleetsView = () => (app.manager ? app.manager.list().map(fleetView) : []);
const activeFleet = () => (app.manager?.active ? fleetView(app.manager.get(app.manager.active)) : null);
const currentFleetId = () => nav.state.sel.fleet ?? app.manager?.active ?? null;
const activeEndpoint = () => activeFleet()?.entry.endpoint ?? "";

/** The active fleet's state and connection, after a switch or a change. */
function syncActive() {
  const fleet = app.manager?.active ? app.manager.get(app.manager.active) : null;
  app.state = fleet ? fleet.state : initialFleetState();
  app.connection = fleet ? fleet.connection : initialConnection();
}

let dataTimer = 0;
function scheduleData() {
  if (!dataTimer) dataTimer = setTimeout(applyData, 60);
}

function onFleetChange(id, event) {
  if (id === app.manager?.active) {
    syncActive();
    if (event?.type === "activity") {
      app.freshActivity = true;
      if (event.data.member) requestRender();
    }
  }
  if (!event) updateConnection();
  scheduleData();
}

/** Totals across fleets for the Fleets tier's toolbar. */
function landscapeTotals() {
  const totals = { fleets: 0, answering: 0, shipsOnline: 0, shipsTotal: 0, busy: 0, people: 0 };
  for (const fleet of fleetsView()) {
    totals.fleets += 1;
    const status = fleetStatus(fleet);
    const data = fleet.state.fleet;
    if (!data || status.bad) continue;
    totals.answering += 1;
    totals.shipsOnline += data.ships.filter((ship) => ship.available).length;
    totals.shipsTotal += data.ships.length;
    for (const ship of data.ships) {
      totals.busy += ship.crew?.busy ?? 0;
      totals.people += ship.crew?.people ?? 0;
    }
  }
  return totals;
}

/** What the toolbar pill and banner say: the active fleet inside a fleet, all fleets on the Fleets tier. */
function connectionDisplay() {
  if (app.sample) return describeConnection({ ...initialConnection(), status: "sample", reason: app.reason }, {});
  if (nav.state.sel.level === -1) {
    const fleets = fleetsView();
    const bad = fleets.filter((fleet) => fleetStatus(fleet).bad).length;
    const live = fleets.length - bad;
    return { label: bad ? `${live} of ${fleets.length} live` : `Live · ${fleets.length} fleets`, tone: bad === fleets.length ? "red" : bad ? "yellow" : "green", banner: null };
  }
  const fleet = activeFleet();
  if (fleet?.connection.reason === "no-token") {
    const problem = tokenProblem(fleet.entry.problem, fleet.entry.origin);
    return { label: "No token", tone: "red", banner: { title: `${fleet.name}: ${problem.title.charAt(0).toLowerCase()}${problem.title.slice(1)}`, body: problem.body, retry: Boolean(problem.retry) } };
  }
  return describeConnection(app.connection, { endpoint: activeEndpoint(), now: Date.now() });
}

function updateConnection() {
  chrome.connection(connectionDisplay(), app.sample);
}

const form = createFleetForm({
  inspector: chrome.el.inspector,
  write: (html) => chrome.inspector(html, false),
  bridge: () => app.bridge,
  store,
  entries: () => (app.manager?.sample ? [] : (app.manager?.list() ?? []).map((fleet) => fleet.entry)),
  blocker: () => app.blocker,
  readFleet: (endpoint, token, signal) => createApiClient({ endpoint, token, fetch: globalThis.fetch.bind(globalThis) }).fleet(signal),
  onChanged: async (change) => {
    app.confirmRemove = null;
    const before = app.manager?.list().length ?? 0;
    await reconfigure();
    const count = app.manager?.list().length ?? 0;
    const word = { added: "Added", edited: "Saved", removed: "Removed" }[change.kind];
    chrome.toast(change.warning ? `${word} ${change.name}, but Subspace kept its token: ${change.warning}` : `${word} ${change.name}.`);
    if (change.kind === "added" && count >= 2 && before < count) go(LANDSCAPE);
    else renderInspector(true);
  },
  onClose: () => renderInspector(true),
});

/** Inspector buttons for fleets: add, edit, remove (with a confirmation step). */
function fleetClick(target) {
  if (form.click(target)) return true;
  if (target.closest("[data-fleet-add]")) {
    form.open("add");
    return true;
  }
  const edit = target.closest("[data-fleet-edit]");
  if (edit) {
    const fleet = app.manager?.get(edit.dataset.fleetEdit);
    if (fleet) form.open(fleet.entry.origin === "settings" ? "settings" : "edit", fleet.id);
    return true;
  }
  const remove = target.closest("[data-fleet-remove]");
  if (remove) {
    app.confirmRemove = remove.dataset.fleetRemove;
    renderInspector(true);
    chrome.el.inspector.querySelector("[data-fleet-remove-confirm]")?.focus();
    return true;
  }
  if (target.closest("[data-fleet-remove-cancel]")) {
    app.confirmRemove = null;
    renderInspector(true);
    return true;
  }
  const confirm = target.closest("[data-fleet-remove-confirm]");
  if (confirm) {
    form.remove(confirm.dataset.fleetRemoveConfirm);
    return true;
  }
  return false;
}

/** The tab badge across fleets: uncertain crew, unavailable ships, and fleets that cannot be read. */
function totalBadge() {
  let total = 0;
  for (const fleet of fleetsView()) {
    if (fleetStatus(fleet).bad) {
      total += 1;
      continue;
    }
    const data = fleet.state.fleet;
    if (!data) continue;
    total += data.ships.filter((ship) => !ship.available).length;
    total += allCrew(data, fleet.state.ships).filter((entry) => entry.view.state === "uncertain").length;
  }
  return total > 0 ? String(Math.min(total, 9999)) : null;
}

const notifier = createNotifier({ send: (message) => app.bridge.notify(message) });
/** One alert watcher per fleet and address, naming the fleet when there are several. */
const watchers = createWatcherSet((fleet) =>
  createAlertWatcher({
    push: (alerts) => notifier.push(landscape() ? alerts.map((alert) => ({ ...alert, title: `${fleetName(app.manager.get(fleet.id)?.entry, app.manager.get(fleet.id)?.state.fleet)}: ${alert.title}` })) : alerts),
  }),
);

function fleetComplete(state) {
  const fleet = state.fleet;
  return Boolean(fleet) && fleet.ships.every((ship) => !ship.available || state.ships[ship.id]);
}

/** Ask for a frame. `textOnly` frames refresh the labels without redrawing the scene. */
function requestRender(textOnly = false) {
  overlayOnly = textOnly ? overlayOnly || !rafId : false;
  if (!rafId && !document.hidden) rafId = requestAnimationFrame(frame);
}

function applyQuality() {
  renderer.setQuality(governor.settings);
  root.dataset.quality = String(governor.level);
}

function issueBadge(shipId, memberId) {
  const member = app.state.ships[shipId]?.crewMembers.find((entry) => entry.id === memberId);
  return issues.badgeFor(member);
}

/** A fleet's data for a tier that shows it. */
function stateFor(id) {
  const fleet = app.manager?.get(id ?? app.manager.active);
  if (!fleet) return null;
  const description = describeConnection(fleet.connection, { endpoint: fleet.entry.endpoint, now: Date.now() });
  return {
    fleet: fleet.state.fleet,
    ships: fleet.state.ships,
    unavailableSince: fleet.state.unavailableSince,
    name: fleetName(fleet.entry, fleet.state.fleet),
    active: fleet.id === app.manager.active,
    emptyMessage: fleet.connection.reason === "no-token" ? tokenProblem(fleet.entry.problem, fleet.entry.origin).title : description.banner ? description.banner.title : "Connecting to Stargate…",
    emptyDetail: `Stargate API · ${fleet.entry.endpoint}`,
  };
}

function sceneContext(t) {
  const description = connectionDisplay();
  return {
    W: app.W,
    H: app.H,
    now: serverNow(),
    clock: Date.now(),
    anim: t,
    reduced: reduced(),
    dark: app.palette.dark,
    fleet: app.state.fleet,
    ships: app.state.ships,
    packets: livePackets(app.state, Date.now()),
    unavailableSince: app.state.unavailableSince,
    fleets: fleetsView(),
    avoid: app.avoid ?? [],
    stateFor: app.manager ? stateFor : null,
    sample: app.sample,
    hover: app.hover ?? app.focusHover,
    focus: null,
    bracket: 0,
    issueBadge,
    emptyTitle: "Fleet Console",
    emptyDetail: activeEndpoint() ? `Stargate API · ${activeEndpoint()}` : "",
    emptyMessage: description.banner ? description.banner.title : "Connecting to Stargate…",
  };
}

function frame() {
  rafId = 0;
  const now = performance.now();
  const callbackInterval = lastCallback === null ? null : now - lastCallback;
  lastCallback = now;
  const t = clock();
  if (!app.palette) return;
  const textOnly = overlayOnly;
  overlayOnly = false;
  const busy = nav.busy();
  const packets = livePackets(app.state, Date.now()).length > 0;
  const continuous = busy || packets || !reduced();
  const ambientOnly = !busy && !packets && !reduced();
  if (continuous) governor.observeCallback(callbackInterval);
  const idle = now - lastInput > IDLE_AFTER_MS;
  const target = ambientOnly ? 1000 / ambientFps(governor.level, document.hasFocus() && !idle) : governor.refresh;
  if (ambientOnly && !textOnly && now - lastDrawReal < target - 4) {
    rafId = requestAnimationFrame(frame);
    return;
  }
  const dt = Math.min(0.05, Math.max(0, (t - lastDraw) / 1000));
  const drawnInterval = now - lastDrawReal;
  lastDraw = t;
  lastDrawReal = now;
  nav.tick(t);
  if (!sameSelection(nav.state.sel, app.lastSel)) onSelection();
  const composed = composeFrame(nav.state, sceneContext(t), t, dt);
  if (!textOnly) {
    const motion = stepStars(stars, dt, composed.velocity, reduced());
    // Ambient light changes slowly: recompute bloom on every other ambient frame.
    ambientFrames = ambientOnly ? ambientFrames + 1 : 0;
    renderer.render(composed, stars, motion, t, { reuseBloom: ambientOnly && ambientFrames % 2 === 0 });
  }
  if (overlay.render(composed)) stats.overlay();
  app.hits = composed.interactive ? composed.hits : [];
  const work = performance.now() - now;
  if (continuous && !textOnly) {
    stats.frame(drawnInterval, work);
    if (governor.sample({ interval: drawnInterval, target, work, gpu: lastGpu }, now)) applyQuality();
  }
  if (nav.busy() || packets || !reduced()) rafId = requestAnimationFrame(frame);
  else lastCallback = null;
}

document.addEventListener("visibilitychange", () => {
  app.manager?.setHidden?.(document.hidden);
  if (document.hidden) {
    cancelAnimationFrame(rafId);
    rafId = 0;
    lastCallback = null;
    governor.reset();
    return;
  }
  lastDraw = clock();
  lastDrawReal = performance.now();
  requestRender();
  reconfigure();
  issues.refresh();
});
reducedQuery.addEventListener?.("change", (event) => {
  systemReduced = event.matches;
  root.dataset.motion = reduced() ? "reduced" : "full";
  requestRender();
});

// ---------- navigation ----------

function go(target) {
  app.hover = null;
  chrome.hideTip();
  nav.go(target, clock());
  if (!sameSelection(nav.state.sel, app.lastSel)) onSelection();
  requestRender();
}

function describeSelection(sel) {
  if (sel.level === -1) return describeLandscape(fleetsView().map((fleet) => ({ name: fleet.name, answering: Boolean(fleet.state.fleet) && !fleetStatus(fleet).bad, bad: fleetStatus(fleet).bad })));
  return describeTier(app.state.fleet, app.state.ships, sel);
}

function updateCanvasLabel() {
  sceneCanvas.setAttribute("aria-label", describeSelection(nav.state.sel));
}

function updateChrome() {
  const sel = nav.state.sel;
  chrome.crumbs(app.state.fleet, app.state.ships, sel, activeFleet()?.name ?? "Fleet Console", landscape());
  chrome.depth(sel, landscape());
  if (sel.level === -1) chrome.landscapeStats(landscapeTotals());
  else chrome.stats(app.state.fleet, app.state.ships, serverNow());
}

function onSelection() {
  const sel = nav.state.sel;
  const previous = app.lastSel;
  app.lastSel = sel;
  // Inside a fleet, that fleet streams; the Fleets tier polls the rest.
  if (sel.level >= 0 && sel.fleet && app.manager && sel.fleet !== app.manager.active) {
    app.manager.activate(sel.fleet);
    syncActive();
    app.contextKey = "";
    chrome.ticker(app.state.activity, app.state.fleet, false);
  }
  app.manager?.setLandscape(sel.level === -1);
  if (form.isOpen) form.close();
  closeActivity(false);
  app.confirmRemove = null;
  const focus = inspectorView.captureFocus();
  // Pulling out: land on the row for the tier just left.
  if (focus.navigating && sel.level < previous.level) focus.signature = selectionSignature(chain(previous, landscape())[sel.level + (landscape() ? 2 : 1)]);
  updateChrome();
  renderInspector(true, focus);
  updateCanvasLabel();
  updateConnection();
  chrome.announce(describeSelection(sel));
  if (sel.level >= 1) app.manager?.readShip(sel.fleet, sel.ship);
  if (sel.level === 2) app.manager?.readCrew(sel.fleet, sel.ship, sel.crew);
  shareContext();
}

/** Pull out when the selected fleet, ship or member disappears from the data. */
function keepSelectionValid() {
  const sel = nav.state.sel;
  if (nav.state.warp || !app.manager) return;
  const first = app.manager.list()[0]?.id ?? null;
  if (sel.level === -1 && !landscape()) nav.jump(fleetTier(app.manager.active ?? first), clock());
  else if (sel.level >= 0 && (!sel.fleet || !app.manager.get(sel.fleet))) nav.jump(landscape() ? LANDSCAPE : fleetTier(app.manager.active ?? first), clock());
  else if (sel.level >= 1 && app.state.fleet) {
    const { ship, detail, member } = lookup(app.state.fleet, app.state.ships, sel);
    if (!ship) nav.jump(fleetTier(sel.fleet), clock());
    else if (sel.level === 2 && detail && !member) nav.jump(parentOf(sel), clock());
  }
  if (!sameSelection(nav.state.sel, app.lastSel)) onSelection();
}

function selectionFrom(element) {
  const kind = element.dataset.go;
  const fleet = element.dataset.fleet ?? currentFleetId();
  if (kind === "fleets") return LANDSCAPE;
  if (kind === "ship") return { level: 1, fleet, ship: element.dataset.ship, crew: null };
  if (kind === "crew") return { level: 2, fleet, ship: element.dataset.ship, crew: element.dataset.crew };
  return fleetTier(fleet);
}

function hitAt(x, y) {
  for (let i = app.hits.length - 1; i >= 0; i -= 1) {
    const hit = app.hits[i];
    if (hit.rect) {
      const [rx, ry, rw, rh] = hit.rect;
      if (x >= rx && x <= rx + rw && y >= ry && y <= ry + rh) return hit;
    } else if (Math.hypot(x - hit.x, y - hit.y) <= hit.r) return hit;
  }
  return null;
}

function landscapeTooltip(hit) {
  if (hit.kind === "hub") return `<b>Subspace</b><span>This desktop, reading ${fleetsView().length} fleets</span>`;
  const fleet = fleetsView().find((entry) => entry.id === hit.fleet);
  if (!fleet) return "";
  return `<b>${esc(fleet.name)}</b><span>${esc(fleetStatus(fleet).text)}</span><span class="mono">${esc(fleet.entry.endpoint)}</span><em>Click to warp in</em>`;
}

stage.addEventListener("pointermove", (event) => {
  if (event.target.closest(".depth")) return;
  const box = stage.getBoundingClientRect();
  const x = event.clientX - box.left;
  const y = event.clientY - box.top;
  const hit = nav.busy() ? null : hitAt(x, y);
  const id = hit ? hit.id : null;
  if (id !== app.hover) {
    app.hover = id;
    requestRender();
  }
  sceneCanvas.style.cursor = hit && ["ship", "crew", "fleet"].includes(hit.kind) ? "pointer" : "default";
  let html = "";
  if (hit && event.pointerType !== "touch") html = hit.kind === "fleet" || hit.kind === "hub" ? landscapeTooltip(hit) : app.state.fleet ? tooltip(inspectorModel(), hit) : "";
  chrome.showTip(html, x, y, app.W, app.H);
});
stage.addEventListener("pointerleave", () => {
  app.hover = null;
  chrome.hideTip();
  requestRender();
});
stage.addEventListener("click", (event) => {
  if (event.target.closest(".depth") || nav.busy()) return;
  const box = stage.getBoundingClientRect();
  const hit = hitAt(event.clientX - box.left, event.clientY - box.top);
  const fleet = currentFleetId();
  if (hit?.kind === "fleet") go(fleetTier(hit.fleet));
  else if (hit?.kind === "ship") go({ level: 1, fleet, ship: hit.ship, crew: null });
  else if (hit?.kind === "crew") go({ level: 2, fleet, ship: hit.ship, crew: hit.crew });
});

document.addEventListener("keydown", (event) => {
  // Shift+P on the map (or with nothing focused) shows or hides the frame-time readout.
  if (isPerfToggleKey(event) && (stage.contains(event.target) || event.target === document.body)) {
    event.preventDefault();
    const on = perfToggle.toggle();
    applyPerf();
    chrome.announce(on ? "Frame-time readout shown. Shift+P hides it." : "Frame-time readout hidden.");
    return;
  }
  if (event.key !== "Escape") return;
  if (form.isOpen && chrome.el.inspector.contains(event.target)) {
    event.preventDefault();
    form.close();
    return;
  }
  if (app.activity && (chrome.el.inspector.contains(event.target) || event.target === document.body)) {
    event.preventDefault();
    closeActivity(true);
    return;
  }
  if (/^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName)) return;
  const target = nav.target();
  const parent = parentOf(target, landscape());
  if (sameSelection(parent, target)) return;
  event.preventDefault();
  go(parent);
});

document.addEventListener("click", (event) => {
  if (fleetClick(event.target)) return;
  const activity = event.target.closest("[data-activity]");
  if (activity) {
    openActivity(activity.dataset.activity, activity.dataset.activityName, activity.dataset.activityRole);
    return;
  }
  const goTarget = event.target.closest("[data-go]");
  if (goTarget && !goTarget.disabled) {
    go(selectionFrom(goTarget));
    return;
  }
  const issue = event.target.closest("[data-issue]");
  if (issue) openIssue(issue.dataset.issue);
});

// A focused inspector row lights its node on the map, as hovering does.
document.addEventListener("focusin", (event) => {
  const row = event.target.closest?.("[data-go]");
  const kind = row?.dataset.go;
  const next =
    kind === "ship" ? `ship:${row.dataset.ship}` : kind === "crew" ? `crew:${row.dataset.ship}:${row.dataset.crew}` : kind === "fleet" && nav.state.sel.level === -1 ? `fleet:${row.dataset.fleet}` : null;
  if (next !== app.focusHover) {
    app.focusHover = next;
    requestRender();
  }
});

chrome.el.retry.addEventListener("click", () => {
  reconfigure(true);
});

// ---------- inspector ----------

function inspectorModel() {
  return {
    fleet: app.state.fleet,
    ships: app.state.ships,
    now: serverNow(),
    unavailableSince: app.state.unavailableSince,
    issues: {
      enabled: issues.enabled,
      idsFor: (member) => issues.idsFor(member),
      get: (id) => issues.get(id),
      canOpen: issues.canOpen,
    },
  };
}

const inspectorView = createInspectorView({
  inspector: chrome.el.inspector,
  regions: [chrome.el.crumbs, chrome.el.depth],
  write: (html, keepScroll) => chrome.inspector(html, keepScroll),
});
let inspectorTimer = 0;

function panelHtml(sel) {
  if (!app.manager) return waitingPanel(connectionDisplay(), "");
  const total = app.manager.list().length;
  if (sel.level === -1) return landscapePanel({ fleets: fleetsView(), sample: app.sample, blocker: app.blocker });
  const fleet = activeFleet();
  if (!fleet) return waitingPanel(connectionDisplay(), "");
  const section = { fleet, total, sample: app.sample, confirming: app.confirmRemove === fleet.id, now: Date.now() };
  if (!app.state.fleet) return unreadableFleetPanel(section);
  const model = inspectorModel();
  if (sel.level === 0) return fleetPanel(model, { title: total > 1 ? fleet.name : null, after: addFleetBar(total) }) + fleetSection(section);
  if (sel.level === 1) return shipPanel(model, sel.ship);
  return crewPanel(model, sel.ship, sel.crew);
}

function renderInspector(navigated, focus) {
  clearTimeout(inspectorTimer);
  inspectorTimer = 0;
  // The form owns the inspector while it is open; data updates wait.
  if (form.isOpen) return;
  // So does the Activity view, which redraws itself.
  if (app.activity) {
    drawActivity();
    return;
  }
  const sel = nav.state.sel;
  const key = `${sel.level}:${sel.fleet}:${sel.ship}:${sel.crew}:${app.confirmRemove}`;
  if (inspectorView.show(panelHtml(sel), key, navigated, focus)) chrome.tickTimes(serverNow());
}

// ---------- officer activity ----------

/**
 * Open the Activity view for an officer (registration ID `agent`) of the
 * fleet you are inside. Its events ride on that fleet's one stream, re-opened
 * with `?agent=`; it is never a second stream. Content stays in memory.
 */
function openActivity(agent, name, role) {
  if (!agent || !app.manager?.active) return;
  closeActivity(false);
  const view = { agent, name: name || agent, role, handle: null, watcher: null, panel: null };
  view.handle = app.manager.watchActivity?.(app.manager.active, agent, {
    onEvent: (data) => view.watcher?.event(data),
    onOpen: () => view.watcher?.reopened(),
    onDown: () => view.watcher?.disconnected(),
    onRefused: () => view.watcher?.refused(),
    // The fleet's address or token changed: start again from the new stream.
    onReset: () => view.watcher?.reset(),
    // The fleet lost its token or went: nothing read from it stays on show.
    onEnded: () => {
      if (app.activity !== view) return;
      view.handle = null;
      closeActivity(true);
      chrome.toast("Activity closed: the fleet's address or token changed.");
    },
  });
  if (!view.handle) {
    chrome.toast("Activity needs the fleet's token.");
    return;
  }
  view.watcher = createActivityWatcher({ client: view.handle.client, agent, onChange: () => markActivity() });
  view.panel = createActivityPanel({
    doc: document,
    onClose: () => closeActivity(true),
    onLoad: (turn, item) => view.watcher?.loadItem(turn, item),
    onRetry: () => view.watcher?.retry(),
  });
  app.activity = view;
  const inspector = chrome.el.inspector;
  inspector.replaceChildren(view.panel.root);
  inspector.scrollTop = 0;
  view.watcher.start();
  drawActivity();
  inspector.querySelector("h2")?.focus({ preventScroll: true });
  chrome.announce(`Activity for ${view.name}`);
}

function markActivity() {
  if (!app.activityFrame) app.activityFrame = requestAnimationFrame(drawActivity);
}

function drawActivity() {
  if (app.activityFrame) cancelAnimationFrame(app.activityFrame);
  app.activityFrame = 0;
  const view = app.activity;
  if (!view) return;
  const state = view.watcher.view;
  // Nothing to follow on this gateway: give the stream back its plain form.
  if (state.status === "unavailable" && view.handle) {
    view.handle.stop();
    view.handle = null;
  }
  const live = app.sample || (view.handle !== null && activeFleet()?.connection.status === "connected");
  view.panel.update(state, { name: view.name, role: view.role, live });
  chrome.tickTimes(serverNow());
}

/** Close the Activity view: the stream re-opens without the agent and the content is dropped. */
function closeActivity(restore) {
  const view = app.activity;
  if (!view) return;
  app.activity = null;
  if (app.activityFrame) cancelAnimationFrame(app.activityFrame);
  app.activityFrame = 0;
  view.handle?.stop();
  view.watcher?.stop();
  if (!restore) return;
  renderInspector(true);
  [...chrome.el.inspector.querySelectorAll("[data-activity]")].find((button) => button.dataset.activity === view.agent)?.focus();
}

function markInspector() {
  if (!inspectorTimer) inspectorTimer = setTimeout(() => renderInspector(false), 180);
}

// ---------- data ----------

function applyData() {
  dataTimer = 0;
  updateChrome();
  chrome.ticker(app.state.activity, app.state.fleet, app.freshActivity && !reduced());
  app.freshActivity = false;
  keepSelectionValid();
  markInspector();
  updateCanvasLabel();
  requestRender();
  if (app.sample) return;
  const fleets = app.manager?.list() ?? [];
  watchers.retain(fleets);
  for (const fleet of fleets) {
    if (!fleet.state.fleet) continue;
    if (fleet.streaming && !fleetComplete(fleet.state)) continue;
    watchers.for(fleet).observe(fleet.state.fleet, fleet.state.ships);
  }
  setBadge(totalBadge());
  shareContext();
}

function setBadge(text) {
  if (text === app.badge) return;
  app.badge = text;
  app.bridge.setBadge(text);
}

function shareContext() {
  if (!app.bridge) return;
  const sel = nav.state.sel;
  const { member } = lookup(app.state.fleet, app.state.ships, sel);
  const fleetLabel = landscape() ? activeFleet()?.name ?? null : null;
  const value = app.sample || !member ? null : crewContext(app.state.fleet, app.state.ships, sel, issues.idsFor(member), serverNow(), fleetLabel);
  // Compare without the running time so the context is not resent every second.
  const key = value ? `${value.label}\n${value.content.replace(/ for \d+[smhd][^\n(]*/, "")}` : "";
  if (key === app.contextKey) return;
  app.contextKey = key;
  app.bridge.setContext(value);
}

// ---------- issues ----------

function openIssue(id) {
  if (app.sample) {
    chrome.toast(`Sample data. With a live fleet this opens ${id} in Issues.`);
    return;
  }
  app.bridge.issues.open(id);
}

// ---------- configuration ----------

const configure = coalesce((retry) => applyConfig(retry));

/**
 * Re-read settings, secrets and the fleet list; switch or update the fleets.
 * Calls made while one runs share it, and a Retry among them runs once more.
 */
function reconfigure(retry = false) {
  if (!app.bridge) return Promise.resolve();
  return configure(retry);
}

/** `retry` is the person's Retry: only then may Subspace read its keychain again. */
async function readConfig(retry = false) {
  const bridge = app.bridge;
  if (bridge.present) await bridge.refresh();
  const allowed = { settings: bridge.has("settings"), secrets: bridge.has("secrets"), saveSecrets: bridge.has("save-secrets") };
  const capabilities = bridge.capabilities();
  const settings = allowed.settings ? ((await bridge.settings()) ?? {}) : {};
  const secrets = allowed.secrets ? ((await bridge.secrets()) ?? {}) : {};
  const api = allowed.secrets && capabilities.fleets ? ((await bridge.fleets(retry)) ?? []) : [];
  app.blocker = addBlocker({ present: bridge.present, capabilities, allowed });
  return resolveFleets({ present: bridge.present, allowed, capabilities, settings, secrets, list: store.load(), api, normalizeEndpoint });
}

async function startManager(mode, fleets) {
  closeActivity(false);
  app.manager?.stop();
  app.mode = mode;
  app.sample = mode === "sample";
  watchers.clear();
  app.contextKey = "";
  app.manager =
    mode === "live"
      ? createFleetManager({
          entries: fleets,
          createClient: (entry) => createApiClient({ endpoint: entry.endpoint, token: entry.token, fetch: globalThis.fetch.bind(globalThis) }),
          createLive: (options) => createLiveSource(options),
          onChange: onFleetChange,
        })
      : createSampleFleetManager({
          createSource: ({ variant, onEvent }) => createSampleSource({ variant, onEvent }),
          createActivity: (options) => createSampleActivitySource(options),
          onChange: onFleetChange,
        });
  if (app.sample) {
    setBadge(null);
    app.bridge.setContext(null);
  }
  await issues.setup(app.sample);
  const first = app.manager.list().find((fleet) => fleet.entry.token || app.sample)?.id ?? app.manager.list()[0]?.id ?? null;
  if (first) app.manager.activate(first);
  syncActive();
  app.manager.setHidden?.(document.hidden);
  app.manager.start();
  nav.jump(landscape() ? LANDSCAPE : fleetTier(first), clock());
  onSelection();
  applyData();
}

async function applyConfig(retry) {
  const config = await readConfig(retry);
  app.motionSetting = config.motion ?? "Full";
  root.dataset.motion = reduced() ? "reduced" : "full";
  perfToggle.setSetting(config.perfReadout);
  applyPerf();
  if (config.mode === "sample") {
    app.reason = config.reason;
    if (app.mode !== "sample") await startManager("sample", []);
    else updateConnection();
    return;
  }
  app.reason = null;
  if (app.mode !== "live") {
    await startManager("live", config.fleets);
    return;
  }
  app.manager.update(config.fleets);
  if (!app.manager.active || !app.manager.get(app.manager.active)) {
    const first = app.manager.list()[0]?.id;
    if (first) app.manager.activate(first);
  }
  syncActive();
  if (retry) {
    if (nav.state.sel.level === -1) for (const fleet of app.manager.list()) app.manager.retry(fleet.id);
    else app.manager.retry(app.manager.active);
  }
  applyData();
  updateConnection();
}

// ---------- performance readout ----------

/**
 * The frame-time readout: frame times, CPU and GPU split where measurable,
 * and the quality level. Shown by `?perf=1`, the setting or Shift+P; built
 * the first time it shows, and updated only while it is visible.
 */
let perfReadout = null;

function createPerfReadout() {
  const gpuTimer = renderer.enableGpuTimer?.((ms) => {
    lastGpu = ms;
    stats.gpu(ms);
  });
  const hud = mountHud(stage);
  avoidObserver.observe(hud.element);
  const snapshot = () => {
    const pixels = renderer.pixels ?? { width: 0, height: 0, scale: 1 };
    const settings = governor.settings;
    return { ...stats.snapshot(), level: governor.level, label: settings.label, bloom: settings.bloom, scale: pixels.scale, width: pixels.width, height: pixels.height, renderer: renderer.kind, verdict: governor.verdict, refresh: governor.refresh, history: governor.history, gpuTimer: Boolean(gpuTimer) };
  };
  Object.defineProperty(window, "__fleetPerf", { value: Object.freeze({ snapshot }) });
  let timer = 0;
  const update = () => {
    if (document.hidden) return;
    const info = snapshot();
    hud.update(hudLines(info, stats.rates(performance.now()), { ...info, pinned: governor.pinned }));
  };
  return {
    show(on) {
      hud.show(on);
      if (on && !timer) {
        stats.rates(performance.now());
        update();
        timer = setInterval(update, 500);
      } else if (!on && timer) {
        clearInterval(timer);
        timer = 0;
      }
    },
  };
}

function applyPerf() {
  const on = perfToggle.shown;
  if (!on && !perfReadout) return;
  perfReadout ??= createPerfReadout();
  perfReadout.show(on);
}


// ---------- start ----------

async function start() {
  app.bridge = await connectBridge();
  if (app.bridge.present) {
    const theme = await app.bridge.theme();
    if (theme && !preview.theme) applyTheme(theme);
    app.bridge.onTheme((next) => {
      if (!preview.theme) applyTheme(next);
    });
    app.bridge.onFleetsChanged(() => reconfigure());
    app.bridge.issues.onChanged(async () => {
      if (app.sample) return;
      const scope = await app.bridge.issues.scope();
      if (JSON.stringify(scope) !== JSON.stringify(issues.scope)) await issues.setup(false);
      else issues.refresh();
    });
  }
  if (!app.palette) refreshPalette();
  applyQuality();
  applyPerf();
  chrome.depth(nav.state.sel);
  await reconfigure();
  requestRender();
}

setInterval(() => {
  if (document.hidden) return;
  chrome.tickTimes(serverNow());
  if (app.connection.retryAt) updateConnection();
  // Running times tick once a second; only the labels need redrawing for that.
  if (reduced()) requestRender(true);
}, 1000);

setInterval(() => {
  if (!document.hidden) issues.refresh();
}, REFRESH_MS);

// Settings have no change event: look again while waiting for setup or a new token.
setInterval(() => {
  if (!document.hidden && app.bridge?.present && (app.sample || ["signed-out", "forbidden"].includes(app.connection.status))) reconfigure();
}, 30_000);
window.addEventListener("focus", () => {
  if (!form.isOpen) reconfigure();
});

start().catch((error) => {
  console.error("Fleet Console could not start.", error);
  chrome.toast("Fleet Console could not start. Reload the page.");
});
