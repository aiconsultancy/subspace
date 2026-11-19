import { iconMarkup } from "./icons.js";

function node(doc, tag, className, text) {
  const element = doc.createElement(tag);
  if (className) element.className = className;
  if (text !== undefined) element.textContent = text;
  return element;
}

function icon(doc, spec) {
  const holder = node(doc, "span", "icon");
  holder.innerHTML = iconMarkup(spec.kind, spec.isDay); // constant markup only
  return holder;
}

function temperatureSpeech(text) {
  return text.endsWith("°") ? `${text.slice(0, -1)} degrees Celsius` : "Temperature unavailable";
}

/**
 * DOM rendering for the page. All API-derived text is set with textContent.
 * @param {Document} doc
 */
export function createUi(doc = document) {
  const $ = (id) => doc.getElementById(id);
  const el = {
    form: $("postcode-form"),
    input: $("postcode"),
    submit: $("postcode-submit"),
    refresh: $("refresh"),
    hint: $("postcode-hint"),
    error: $("postcode-error"),
    banner: $("banner"),
    announce: $("announce"),
    main: $("forecast"),
    basis: $("hero-basis"),
    place: $("place"),
    placeDetail: $("place-detail"),
    temperature: $("temperature"),
    temperatureValue: $("temperature-value"),
    temperatureText: $("temperature-text"),
    condition: $("condition"),
    range: $("range"),
    sections: $("sections"),
    hours: $("hours"),
    days: $("days"),
    details: $("details"),
    updated: $("updated"),
    niNote: $("ni-note"),
  };
  const signatures = new Map();
  let lastAnnouncement = "";

  /** Replace a list's children only when its data changed, keeping scroll state. */
  function renderList(name, container, data, build) {
    const signature = JSON.stringify(data);
    if (signatures.get(name) === signature) return;
    signatures.set(name, signature);
    container.replaceChildren(...data.map(build));
  }

  function hourItem(hour) {
    const item = node(doc, "li", "hour");
    const parts = [
      node(doc, "span", "hour-time", hour.time),
      icon(doc, hour.icon),
      node(doc, "span", "hour-chance", hour.chance),
      node(doc, "span", "hour-temp", hour.temperature),
    ];
    parts.forEach((part) => part.setAttribute("aria-hidden", "true"));
    item.append(...parts, node(doc, "span", "sr-only", hour.description));
    return item;
  }

  function dayItem(day) {
    const item = node(doc, "li", "day");
    const bar = node(doc, "span", "range-bar");
    bar.setAttribute("aria-hidden", "true");
    bar.style.setProperty("--from", `${day.barFrom.toFixed(1)}%`);
    bar.style.setProperty("--to", `${day.barTo.toFixed(1)}%`);
    bar.append(node(doc, "span", "range-fill"));
    const parts = [
      node(doc, "span", "day-name", day.label),
      icon(doc, day.icon),
      node(doc, "span", "day-chance", day.chance),
      node(doc, "span", "day-low", day.low),
      bar,
      node(doc, "span", "day-high", day.high),
    ];
    parts.forEach((part) => part.setAttribute("aria-hidden", "true"));
    item.append(...parts, node(doc, "span", "sr-only", day.description));
    return item;
  }

  function tile(detail) {
    const wrapper = node(doc, "div", `tile tile-${detail.id}`);
    wrapper.append(
      node(doc, "dt", "tile-title", detail.title),
      node(doc, "dd", "tile-value", detail.value),
      node(doc, "dd", "tile-note", detail.note),
    );
    return wrapper;
  }

  function announce(text) {
    if (text !== lastAnnouncement) {
      lastAnnouncement = text;
      el.announce.textContent = text;
    }
  }

  function setBanner(banner) {
    if (!banner) {
      el.banner.hidden = true;
      el.banner.textContent = "";
      delete el.banner.dataset.tone;
      return;
    }
    el.banner.hidden = false;
    el.banner.dataset.tone = banner.tone;
    el.banner.textContent = banner.text;
  }

  return {
    elements: el,

    /** @param {ReturnType<import("./view-model.js").buildView>} view */
    render(view, { refreshing = false, canRefresh = false } = {}) {
      setBanner(view.banner);
      el.main.setAttribute("aria-busy", String(view.mode === "loading"));
      el.refresh.hidden = !canRefresh;
      el.refresh.textContent = refreshing ? "Refreshing…" : "Refresh";
      el.updated.textContent = view.updated;
      el.niNote.hidden = !view.northernIreland;

      const ready = view.mode === "ready";
      el.sections.hidden = !ready;
      el.temperature.hidden = !ready;
      el.condition.hidden = !ready;
      el.range.hidden = !ready;

      if (!ready) {
        el.basis.textContent =
          view.mode === "loading" ? "Loading forecast…" : view.mode === "error" ? "Forecast unavailable" : "";
        el.place.textContent = view.location?.name ?? (view.mode === "empty" ? "Check the weather" : "Weather");
        el.placeDetail.textContent =
          view.mode === "empty"
            ? "Enter a UK postcode to see current conditions, an hourly forecast and the week ahead."
            : view.mode === "error"
              ? "Use Refresh to try again."
              : [view.location?.postcode, ...(view.location?.area ?? [])].filter(Boolean).join(" · ");
        doc.title = "Weather";
        announce(view.mode === "loading" ? "Loading forecast" : "");
        return;
      }

      const { hero } = view;
      el.basis.textContent = view.synthetic ? "Synthetic preview" : hero.basis;
      el.place.textContent = hero.title;
      el.placeDetail.textContent = hero.subtitle;
      el.temperatureValue.textContent = hero.temperature;
      el.temperatureText.textContent = temperatureSpeech(hero.temperature);
      el.condition.textContent = hero.condition;
      el.range.textContent = [
        `High ${hero.high}`,
        `Low ${hero.low}`,
        hero.feelsLike ? `Feels like ${hero.feelsLike}` : "",
      ].filter(Boolean).join(" · ");
      doc.title = view.synthetic ? "Synthetic preview · Weather" : `${hero.title} · Weather`;
      announce(
        view.synthetic
          ? "Showing synthetic preview data"
          : `Weather for ${hero.title}: ${hero.condition}, ${temperatureSpeech(hero.temperature)}`,
      );

      renderList("hours", el.hours, view.hours, hourItem);
      renderList("days", el.days, view.days, dayItem);
      renderList("details", el.details, view.details, tile);
    },

    setLookup({ pending, error }) {
      el.submit.textContent = pending ? "Looking up…" : "Show weather";
      el.error.textContent = error ? error.message : "";
      if (error) el.input.setAttribute("aria-invalid", "true");
      else el.input.removeAttribute("aria-invalid");
    },

    disableForm(reason) {
      el.input.disabled = true;
      el.submit.disabled = true;
      el.hint.textContent = reason;
    },
  };
}
