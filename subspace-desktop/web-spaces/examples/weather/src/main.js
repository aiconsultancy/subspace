import { createClient } from "./api.js";
import { createController } from "./controller.js";
import { buildPreview, previewNameFromSearch } from "./preview.js";
import { applyScene, buildScene } from "./scene.js";
import { browserStorage, createStore } from "./storage.js";
import { createUi } from "./ui.js";
import { buildView } from "./view-model.js";

const REPAINT_MS = 60_000;
const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

buildScene();
const ui = createUi();
const { elements } = ui;

function paint(state) {
  const view = buildView(state, Date.now());
  applyScene(view.scene, view.synthetic ? "synthetic" : view.stale ? "stale" : "");
  ui.render(view, {
    refreshing: state.refreshing,
    canRefresh: Boolean(state.location) && !state.synthetic,
  });
  ui.setLookup(state.lookup);
}

function setUpHourlyScroller() {
  const scroller = document.getElementById("hours-scroller");
  const earlier = document.getElementById("hours-earlier");
  const later = document.getElementById("hours-later");
  const behavior = () => (reducedMotion.matches ? "auto" : "smooth");
  const scrollBy = (direction, amount = scroller.clientWidth * 0.8) =>
    scroller.scrollBy({ left: direction * amount, behavior: behavior() });

  const syncButtons = () => {
    const max = scroller.scrollWidth - scroller.clientWidth;
    earlier.setAttribute("aria-disabled", String(scroller.scrollLeft <= 1));
    later.setAttribute("aria-disabled", String(scroller.scrollLeft >= max - 1));
  };
  const guarded = (direction) => () => {
    const button = direction < 0 ? earlier : later;
    if (button.getAttribute("aria-disabled") !== "true") scrollBy(direction);
  };

  earlier.addEventListener("click", guarded(-1));
  later.addEventListener("click", guarded(1));
  scroller.addEventListener("scroll", syncButtons, { passive: true });
  new ResizeObserver(syncButtons).observe(scroller);
  new MutationObserver(syncButtons).observe(document.getElementById("hours"), { childList: true });
  scroller.addEventListener("keydown", (event) => {
    const keys = { ArrowLeft: [-1, 96], ArrowRight: [1, 96], PageUp: [-1], PageDown: [1] };
    if (event.key in keys) {
      const [direction, amount] = keys[event.key];
      scrollBy(direction, amount);
    } else if (event.key === "Home") {
      scroller.scrollTo({ left: 0, behavior: behavior() });
    } else if (event.key === "End") {
      scroller.scrollTo({ left: scroller.scrollWidth, behavior: behavior() });
    } else {
      return;
    }
    event.preventDefault();
  });
  syncButtons();
}

function startPreview(name) {
  const state = buildPreview(name, Date.now());
  ui.disableForm("Preview mode shows invented sample data. Open the board without a preview setting for live weather.");
  paint(state);
  // Repaint so the clock-based parts of the view stay current without any request.
  setInterval(() => !document.hidden && paint({ ...state, fetchedAt: Date.now() }), REPAINT_MS);
}

function startLive() {
  const controller = createController({
    client: createClient({ fetch: globalThis.fetch.bind(globalThis) }),
    store: createStore(browserStorage()),
    onChange: paint,
  });

  elements.form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const accepted = await controller.submitPostcode(elements.input.value);
    if (accepted) elements.input.value = controller.state.location.postcode;
  });
  elements.refresh.addEventListener("click", () => controller.refresh({ manual: true }));

  let ticker = null;
  const applyVisibility = () => {
    controller.setHidden(document.hidden);
    clearInterval(ticker);
    ticker = document.hidden ? null : setInterval(() => paint(controller.state), REPAINT_MS);
    if (!document.hidden) paint(controller.state);
  };
  document.addEventListener("visibilitychange", applyVisibility);

  controller.setHidden(document.hidden);
  controller.start();
  if (controller.state.location) elements.input.value = controller.state.location.postcode;
  applyVisibility();
}

/** Tell CSS where the hero text is so clouds and fog can fade out behind it. */
function trackHero() {
  const hero = document.querySelector(".hero");
  const style = document.documentElement.style;
  const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  let queued = false;
  const update = () => {
    queued = false;
    const box = hero.getBoundingClientRect();
    style.setProperty("--hero-cx", `${Math.round(box.left + box.width / 2)}px`);
    style.setProperty("--hero-cy", `${Math.round(box.top + box.height / 2)}px`);
    style.setProperty("--hero-rx", `${Math.round(Math.min(box.width / 2, 21 * rem) + 4 * rem)}px`);
    style.setProperty("--hero-ry", `${Math.round(box.height / 2 + 4 * rem)}px`);
  };
  const schedule = () => {
    if (!queued) {
      queued = true;
      requestAnimationFrame(update);
    }
  };
  const observer = new ResizeObserver(schedule);
  observer.observe(hero);
  observer.observe(document.querySelector(".app")); // a banner appearing moves the hero
  addEventListener("scroll", schedule, { passive: true });
  addEventListener("resize", schedule);
  update();
}

setUpHourlyScroller();
trackHero();
const previewName = previewNameFromSearch(location.search);
if (previewName) startPreview(previewName);
else startLive();
