// src/core/encoding-overrides.ts
function applyEncodingOverrides(template, encodings, chartProperties) {
  const actions = template.encodingActions;
  if (!actions || actions.length === 0 || !chartProperties) return encodings;
  let result = encodings;
  for (const action of actions) {
    const override = chartProperties[action.key];
    if (override !== void 0) {
      result = action.set(result, override);
    }
  }
  return result;
}

// src/core/aggregate.ts
function applyAggregation(encodings, data) {
  if (!data || data.length === 0) return data;
  const specs = [];
  for (const enc of Object.values(encodings)) {
    if (!enc || !enc.aggregate) continue;
    const op = enc.aggregate;
    if (op !== "count" && !enc.field) continue;
    const target = op === "count" ? "_count" : `${enc.field}_${op}`;
    specs.push({ field: enc.field, op, target });
  }
  if (specs.length === 0) return data;
  const firstRow = data[0];
  const allPresent = specs.every(
    (s) => Object.prototype.hasOwnProperty.call(firstRow, s.target)
  );
  if (allPresent) return data;
  const groupFields = [];
  const seen = /* @__PURE__ */ new Set();
  for (const enc of Object.values(encodings)) {
    if (!enc || enc.aggregate || !enc.field) continue;
    if (seen.has(enc.field)) continue;
    seen.add(enc.field);
    groupFields.push(enc.field);
  }
  const groups = /* @__PURE__ */ new Map();
  for (const row of data) {
    const key = JSON.stringify(groupFields.map((f) => row[f] ?? null));
    let bucket = groups.get(key);
    if (!bucket) {
      bucket = [];
      groups.set(key, bucket);
    }
    bucket.push(row);
  }
  const toNum = (v) => typeof v === "number" ? v : Number(v);
  const reduceOp = (rows, spec) => {
    if (spec.op === "count") return rows.length;
    const nums = rows.map((r) => toNum(r[spec.field])).filter((v) => Number.isFinite(v));
    if (nums.length === 0) return 0;
    const sum = nums.reduce((a, b) => a + b, 0);
    return spec.op === "sum" ? sum : sum / nums.length;
  };
  const out = [];
  for (const rows of groups.values()) {
    const head = rows[0];
    const aggregated = {};
    for (const f of groupFields) aggregated[f] = head[f];
    for (const spec of specs) {
      const val = reduceOp(rows, spec);
      aggregated[spec.target] = val;
      if (spec.op !== "count" && spec.field) aggregated[spec.field] = val;
    }
    out.push(aggregated);
  }
  return out;
}

// src/core/chart-transitions.ts
var CHART_TRANSITIONS = {
  // ── Categorical comparison — D × M (§4.1) ──────────────────────────────
  "Bar Chart": [
    // Ordered-axis bridge into the trend family (§4.9). Only when the domain
    // axis is temporal/ordinal — an unordered nominal bar never sprouts a line.
    // orientDomainAxis:'x' re-orients a horizontal bar so time stays horizontal.
    { to: "Line Chart", label: "Line", requireOrderedAxis: true, orientDomainAxis: "x" },
    { to: "Area Chart", label: "Area", requireOrderedAxis: true, requireNonNegative: true, orientDomainAxis: "x" },
    // Same D×M signature, lighter ink.
    { to: "Lollipop Chart", label: "Lollipop" }
  ],
  "Lollipop Chart": [
    { to: "Bar Chart", label: "Bar" }
  ],
  "Grouped Bar Chart": [
    {
      to: "Stacked Bar Chart",
      label: "Stacked",
      route: { from: "group", to: "color", mode: "move" },
      requireDiscreteSource: true
    },
    // A 2-sided grouped bar reads as a population pyramid (mirrored).
    {
      to: "Pyramid Chart",
      label: "Pyramid",
      route: { from: "group", to: "color", mode: "move" },
      requireDiscreteSource: true,
      maxSourceCardinality: 2
    }
  ],
  "Stacked Bar Chart": [
    {
      to: "Grouped Bar Chart",
      label: "Grouped",
      route: { from: "color", to: "group", mode: "move" },
      requireDiscreteSource: true,
      maxSourceCardinality: 12
    }
  ],
  // Population pyramid = a 2-sided category × measure; its complement is the
  // side-by-side grouped bar (the 2 sides dodged instead of mirrored).
  "Pyramid Chart": [
    {
      to: "Grouped Bar Chart",
      label: "Grouped",
      route: { from: "color", to: "group", mode: "move" },
      requireDiscreteSource: true
    }
  ],
  // ── Trend over an ordered domain — T × M (§4.2) ────────────────────────
  "Line Chart": [
    { to: "Area Chart", label: "Area", requireNonNegative: true },
    // Back to discrete-period comparison; only readable with few ticks.
    { to: "Bar Chart", label: "Bar", maxCategoryCardinality: 30 },
    // Small-multiple trend strips (one per series) — needs a series. Route
    // the series onto `color` (from wherever it sits — color OR a column/row
    // facet) so the Sparkline template picks it up as its row series.
    { to: "Sparkline", label: "Sparklines", requireSeries: true, route: { from: "series", to: "color", mode: "move" } }
  ],
  "Area Chart": [
    { to: "Line Chart", label: "Line" },
    { to: "Bar Chart", label: "Bar", maxCategoryCardinality: 30 },
    { to: "Streamgraph", label: "Stream", requireSeries: true, requireNonNegative: true, route: { from: "series", to: "color", mode: "move" } }
  ],
  // Small-multiple trend table → a single overlaid multi-series line.
  "Sparkline": [
    { to: "Line Chart", label: "Line" }
  ],
  // Flowing composition → back to baseline-anchored trend / area. Both reads
  // are safe; note Streamgraph → Line is intentionally *one-directional* (there
  // is no Line → Streamgraph — see the note above).
  "Streamgraph": [
    { to: "Area Chart", label: "Area" },
    { to: "Line Chart", label: "Line" }
  ],
  // ── Two-measure relationship — M₁ × M₂ (§4.3) ──────────────────────────
  "Scatter Plot": [
    {
      to: "Strip Plot",
      label: "Jitter",
      route: { from: "series", to: "x", mode: "swap", spill: "color" }
    },
    // Add a fitted trend layer over the same cloud — only a clean
    // two-measure scatter (both axes quantitative, no size bubble).
    { to: "Regression", label: "Trend", requireBiaxialMeasure: true, requireNoSize: true }
  ],
  "Regression": [
    { to: "Scatter Plot", label: "Scatter" }
  ],
  "Strip Plot": [
    {
      to: "Scatter Plot",
      label: "Scatter",
      route: { from: "color", to: "x", mode: "swap", spill: "color" }
    },
    // A strip plot is a per-category distribution: box (summary) + violin
    // (density) are the same {x:category, y:measure} layout, no route.
    { to: "Boxplot", label: "Box" },
    { to: "Violin Plot", label: "Violin" }
  ],
  // ── Univariate distribution — M (§4.4) ─────────────────────────────────
  "Histogram": [
    { to: "Density Plot", label: "Density" },
    { to: "ECDF Plot", label: "ECDF" }
  ],
  "Density Plot": [
    { to: "Histogram", label: "Histogram" },
    { to: "ECDF Plot", label: "ECDF" }
  ],
  "ECDF Plot": [
    { to: "Histogram", label: "Histogram" },
    { to: "Density Plot", label: "Density" }
  ],
  "Boxplot": [
    { to: "Violin Plot", label: "Violin" },
    { to: "Strip Plot", label: "Strip" }
  ],
  "Violin Plot": [
    { to: "Boxplot", label: "Box" },
    { to: "Strip Plot", label: "Strip" }
  ]
};
function getChartTransitions(chart) {
  if (!chart) return [];
  return CHART_TRANSITIONS[chart] ?? [];
}

// src/core/pivot.ts
var DISCRETE_TYPES = /* @__PURE__ */ new Set(["nominal", "ordinal"]);
function isDiscrete(enc) {
  return !!enc?.field && !!enc.type && DISCRETE_TYPES.has(enc.type);
}
function isMeasure(enc) {
  return !!enc?.field && (enc.type === "quantitative" || !!enc.aggregate);
}
function isTemporal(enc) {
  return enc?.type === "temporal";
}
function temporalActsDiscrete(template) {
  return template.markCognitiveChannel !== "position";
}
function clone(encodings) {
  const out = {};
  for (const [ch, enc] of Object.entries(encodings)) {
    out[ch] = { ...enc };
  }
  return out;
}
function distinctCount(data, field) {
  if (!field || !Array.isArray(data)) return 0;
  const seen = /* @__PURE__ */ new Set();
  for (const row of data) {
    if (row && row[field] != null) seen.add(row[field]);
  }
  return seen.size;
}
function makeCartesianPivot(opts = {}) {
  return {
    key: opts.key ?? "pivot",
    label: opts.label ?? "View",
    transpose: opts.transpose ?? [],
    permute: opts.permute ?? [],
    shift: opts.shift ?? [],
    facetBudget: opts.facetBudget ?? 12,
    transitions: opts.transitions
  };
}
var CHANNEL_ORDER = ["x", "y", "color", "size", "group", "column", "row"];
function orderPair(a, b) {
  const ia = CHANNEL_ORDER.indexOf(a);
  const ib = CHANNEL_ORDER.indexOf(b);
  return ia <= ib ? [a, b] : [b, a];
}
var CHANNEL_DISPLAY = {
  x: "X",
  y: "Y",
  color: "Color",
  size: "Size",
  group: "Groups",
  column: "Columns",
  row: "Rows",
  detail: "Detail",
  opacity: "Opacity"
};
function chDisplay(ch) {
  return CHANNEL_DISPLAY[ch] ?? ch.charAt(0).toUpperCase() + ch.slice(1);
}
function changedChannels(a, b) {
  const out = /* @__PURE__ */ new Set();
  for (const ch of /* @__PURE__ */ new Set([...Object.keys(a), ...Object.keys(b)])) {
    if (a[ch]?.field !== b[ch]?.field) out.add(ch);
  }
  return out;
}
function transposeState(base, template, pair) {
  const [a, b] = orderPair(pair[0], pair[1]);
  const ea = base[a];
  const eb = base[b];
  if (!ea?.field || !eb?.field) return null;
  if (!temporalActsDiscrete(template) && (isTemporal(ea) || isTemporal(eb))) return null;
  const next = clone(base);
  next[a] = { ...eb };
  next[b] = { ...ea };
  return { id: `flip:${a}-${b}`, label: `${chDisplay(a)} \u21C4 ${chDisplay(b)}`, enc: next };
}
function channelProfile(enc, template) {
  if (!enc?.field) return null;
  if (isMeasure(enc)) return "measure";
  if (isDiscrete(enc) || isTemporal(enc) && temporalActsDiscrete(template)) return "category";
  return "time";
}
function permuteSwapState(base, template, pair) {
  const [a, b] = orderPair(pair[0], pair[1]);
  const posCh = a === "x" || a === "y" ? a : null;
  const auxCh = b;
  if (!posCh || auxCh !== "color" && auxCh !== "size") return null;
  const posEnc = base[posCh];
  const auxEnc = base[auxCh];
  if (!posEnc?.field || !auxEnc?.field) return null;
  if (posEnc.field === auxEnc.field) return null;
  const profile = channelProfile(posEnc, template);
  if (!profile || profile !== channelProfile(auxEnc, template)) return null;
  const id = `swap:${a}-${b}`;
  const label = `${chDisplay(a)} \u21C4 ${chDisplay(b)}`;
  if (profile === "measure") {
    if (template.markCognitiveChannel !== "position") return null;
    const next = clone(base);
    next[posCh] = measureCore(auxEnc);
    next[auxCh] = measureCore(posEnc);
    return { id, label, enc: next };
  }
  if (profile === "category") {
    if (auxCh !== "color") return null;
    const next = clone(base);
    next[posCh] = { ...auxEnc };
    next.color = { ...posEnc };
    return { id, label, enc: next };
  }
  return null;
}
function measureCore(enc) {
  const core = { field: enc.field, type: enc.type };
  if (enc.aggregate) core.aggregate = enc.aggregate;
  return core;
}
var GROUPING_CHANNELS = ["color", "group", "column", "row"];
function routeBudget(target, facetBudget) {
  if (target === "column" || target === "row") return facetBudget;
  if (target === "group") return 12;
  return 20;
}
function routeLabel(from, to) {
  return `${chDisplay(from)} \u21C4 ${chDisplay(to)}`;
}
function findTransitionSeries(base, candidates, channels) {
  for (const channel of candidates) {
    if (channels.includes(channel) && isDiscrete(base[channel])) {
      return { channel, enc: base[channel] };
    }
  }
  return null;
}
function seriesRoutingStates(base, template, data, shiftChannels, facetBudget, preferredFacet) {
  const channels = template.channels ?? [];
  const out = [];
  const identitySource = isDiscrete(base.color) ? "color" : !base.color?.field && isDiscrete(base.group) ? "group" : void 0;
  if (identitySource && shiftChannels.includes(identitySource) && channels.includes(identitySource)) {
    const identityEncoding = base[identitySource];
    const card = distinctCount(data, identityEncoding.field);
    const facetTargets = preferredFacet ? [preferredFacet] : ["column", "row"];
    for (const target of facetTargets) {
      if (!shiftChannels.includes(target) || !channels.includes(target)) continue;
      if (base[target]?.field || card > routeBudget(target, facetBudget)) continue;
      const next = clone(base);
      delete next[identitySource];
      next[target] = { ...identityEncoding };
      out.push({
        id: `augment:${target}`,
        enc: next,
        label: `Color + ${chDisplay(target)}`,
        augmentation: {
          kind: "facet-identity",
          sourceChannel: identitySource,
          facetChannel: target,
          colorEncoding: { ...identityEncoding }
        }
      });
    }
  }
  const facetSource = ["column", "row"].find(
    (channel) => shiftChannels.includes(channel) && channels.includes(channel) && isDiscrete(base[channel])
  );
  if (facetSource) {
    const facetEncoding = base[facetSource];
    const card = distinctCount(data, facetEncoding.field);
    for (const target of shiftChannels) {
      if (target === facetSource || target === "group") continue;
      if ((target === "column" || target === "row") && preferredFacet && target !== preferredFacet) continue;
      if (!channels.includes(target) || base[target]?.field) continue;
      if (card > routeBudget(target, facetBudget)) continue;
      const next = clone(base);
      delete next[facetSource];
      next[target] = { ...facetEncoding };
      out.push({ id: `series:${target}`, enc: next, label: routeLabel(facetSource, target) });
    }
  }
  return out;
}
function preferredFacetTarget(base) {
  const domain = domainAxisEnc(base);
  return domain === base.y ? "row" : "column";
}
function changedFacetTarget(authored, transformed) {
  for (const channel of ["column", "row"]) {
    if (transformed[channel]?.field && transformed[channel]?.field !== authored[channel]?.field) {
      return channel;
    }
  }
  return void 0;
}
function domainAxisEnc(base) {
  if (base.x?.field && !isMeasure(base.x)) return base.x;
  if (base.y?.field && !isMeasure(base.y)) return base.y;
  return void 0;
}
function measureAxisEnc(base) {
  if (base.x?.field && isMeasure(base.x)) return base.x;
  if (base.y?.field && isMeasure(base.y)) return base.y;
  return void 0;
}
function transitionGatesPass(base, data, t) {
  if (t.requireOrderedAxis) {
    const domain = domainAxisEnc(base);
    if (!domain || !(domain.type === "temporal" || domain.type === "ordinal")) return false;
  }
  if (t.requireNonNegative) {
    const measure = measureAxisEnc(base);
    if (measure?.field) {
      for (const row of data) {
        const v = row?.[measure.field];
        if (typeof v === "number" && v < 0) return false;
      }
    }
  }
  if (t.maxCategoryCardinality != null) {
    const domain = domainAxisEnc(base);
    if (domain?.field && distinctCount(data, domain.field) > t.maxCategoryCardinality) return false;
  }
  if (t.requireNoSeries) {
    for (const ch of GROUPING_CHANNELS) {
      if (isDiscrete(base[ch])) return false;
    }
  }
  if (t.requireSeries) {
    const seriesChannels = ["color", "group", "detail", "column", "row"];
    if (!seriesChannels.some((ch) => isDiscrete(base[ch]))) return false;
  }
  if (t.requireBiaxialMeasure) {
    if (!isMeasure(base.x) || !isMeasure(base.y)) return false;
  }
  if (t.requireNoSize) {
    if (base.size?.field) return false;
  }
  return true;
}
function transitionState(base, data, template, t) {
  if (!transitionGatesPass(base, data, t)) return null;
  const enc = clone(base);
  const route = t.route;
  if (route) {
    const fromCh = route.from === "series" ? findTransitionSeries(base, GROUPING_CHANNELS, template.channels ?? [])?.channel : route.from;
    if (!fromCh) return null;
    const srcEnc = base[fromCh];
    if (!srcEnc?.field) return null;
    if (t.requireDiscreteSource && !isDiscrete(srcEnc)) return null;
    if (t.maxSourceCardinality != null && distinctCount(data, srcEnc.field) > t.maxSourceCardinality) return null;
    const mode = route.mode ?? "move";
    const dstEnc = base[route.to];
    if (mode === "swap") {
      const spillCh = route.spill ?? fromCh;
      if (spillCh !== fromCh && base[spillCh]?.field) return null;
      enc[route.to] = { ...srcEnc };
      delete enc[fromCh];
      if (dstEnc?.field) enc[spillCh] = { ...dstEnc };
      else delete enc[spillCh];
    } else {
      if (fromCh !== route.to) {
        if (dstEnc?.field) return null;
        delete enc[fromCh];
        enc[route.to] = { ...srcEnc };
      }
    }
  }
  if (t.orientDomainAxis) {
    const target = t.orientDomainAxis;
    const other = target === "x" ? "y" : "x";
    const domainOnOther = !!enc[other]?.field && !isMeasure(enc[other]);
    const targetFreeForDomain = !enc[target]?.field || isMeasure(enc[target]);
    if (domainOnOther && targetFreeForDomain) {
      const a = enc[target];
      const b = enc[other];
      if (b) enc[target] = { ...b };
      else delete enc[target];
      if (a) enc[other] = { ...a };
      else delete enc[other];
    }
  }
  return { enc, chartType: t.to, label: t.label };
}
function pivotSteps(template, enc, data, opts, resolveTemplate) {
  const def = template.pivot;
  if (!def) return [];
  const includeTransitions = opts?.transitions !== false;
  const steps = [];
  for (const pair of def.transpose ?? []) {
    if (pair.length !== 2) continue;
    const s = transposeState(enc, template, [pair[0], pair[1]]);
    if (s) steps.push({ id: s.id, label: s.label, enc: s.enc });
  }
  for (const block of def.permute ?? []) {
    for (let i = 0; i < block.length; i++) {
      for (let j = i + 1; j < block.length; j++) {
        const s = permuteSwapState(enc, template, [block[i], block[j]]);
        if (s) steps.push({ id: s.id, label: s.label, enc: s.enc });
      }
    }
  }
  if (def.shift && def.shift.length) {
    const preferredFacet = opts?.preferFacetTarget ? preferredFacetTarget(enc) : void 0;
    for (const s of seriesRoutingStates(enc, template, data, def.shift, def.facetBudget ?? 12, preferredFacet)) {
      steps.push({ id: s.id, label: s.label, enc: s.enc, augmentation: s.augmentation });
    }
  }
  if (includeTransitions) {
    for (const t of getChartTransitions(template.chart)) {
      if (resolveTemplate && !resolveTemplate(t.to)) continue;
      const st = transitionState(enc, data, template, t);
      if (st) steps.push({ id: `type:${t.to}`, label: `\u03B8_\u2192${t.label.toLowerCase()}`, enc: st.enc, chartType: st.chartType });
    }
  }
  return steps;
}
function encodingKey(enc, chartType) {
  const cells = Object.keys(enc).filter((ch) => enc[ch]?.field).sort().map((ch) => {
    const e = enc[ch];
    return `${ch}=${e.field}/${e.type ?? ""}/${e.aggregate ?? ""}`;
  });
  return `${chartType ?? ""}::${cells.join(",")}`;
}
function isRenderableState(template, enc) {
  const channels = template.channels ?? [];
  if (channels.includes("x") && channels.includes("y")) {
    if (!enc.x?.field || !enc.y?.field) return false;
  }
  return true;
}
var MAX_PIVOT_STATES = 12;
function computePivot(template, base, data, resolveTemplate, opts) {
  const def = template.pivot;
  if (!def) return null;
  const key = opts?.key ?? def.key ?? "pivot";
  const label = opts?.label ?? def.label ?? "View";
  const includeTransitions = opts?.includeTransitions !== false;
  const ids = ["default"];
  const labels = ["Default"];
  const statesById = {
    default: clone(base)
  };
  const augmentationById = {
    default: void 0
  };
  const chartTypeById = {
    default: void 0
  };
  const seen = /* @__PURE__ */ new Set([encodingKey(base, void 0)]);
  const queue = [{ id: "default", label: "Default", enc: clone(base), chartType: void 0, template, augmentation: void 0 }];
  const authoredChart = template.chart;
  while (queue.length > 0 && ids.length < MAX_PIVOT_STATES) {
    const cur = queue.shift();
    for (const step of pivotSteps(cur.template, cur.enc, data, {
      transitions: includeTransitions,
      preferFacetTarget: opts?.preferFacetTarget
    }, resolveTemplate)) {
      let nextChartType = step.chartType ?? cur.chartType;
      if (nextChartType === authoredChart) nextChartType = void 0;
      const resolved = step.chartType ? resolveTemplate?.(step.chartType) : void 0;
      const nextTemplate = step.chartType ? resolved ?? { ...cur.template, pivot: void 0 } : cur.template;
      if (!isRenderableState(nextTemplate, step.enc)) continue;
      if (opts?.preferFacetTarget) {
        const facetTarget = changedFacetTarget(base, step.enc);
        if (facetTarget && facetTarget !== preferredFacetTarget(step.enc)) continue;
      }
      if (step.chartType === void 0 && cur.id !== "default") {
        const already = changedChannels(base, cur.enc);
        const now = changedChannels(cur.enc, step.enc);
        let overlaps = false;
        for (const ch of now) {
          if (already.has(ch)) {
            overlaps = true;
            break;
          }
        }
        if (overlaps) continue;
      }
      const fp = encodingKey(step.enc, nextChartType);
      if (seen.has(fp)) continue;
      seen.add(fp);
      const id = cur.id === "default" ? step.id : `${cur.id}|${step.id}`;
      const stepLabel = cur.id === "default" ? step.label : `${cur.label} \xB7 ${step.label}`;
      ids.push(id);
      labels.push(stepLabel);
      statesById[id] = step.enc;
      chartTypeById[id] = nextChartType;
      const augmentation = step.chartType ? void 0 : step.augmentation ?? cur.augmentation;
      augmentationById[id] = augmentation;
      queue.push({ id, label: stepLabel, enc: step.enc, chartType: nextChartType, template: nextTemplate, augmentation });
      if (ids.length >= MAX_PIVOT_STATES) break;
    }
  }
  return { key, label, ids, labels, statesById, augmentationById, chartTypeById };
}
function applyPivot(template, base, data, chartProperties, resolveTemplate) {
  const comp = computePivot(template, base, data, resolveTemplate);
  if (!comp || comp.ids.length <= 1) {
    return { encodings: base, augmentation: void 0, chartType: void 0, surface: void 0 };
  }
  const stored = chartProperties?.[comp.key];
  const id = typeof stored === "string" && comp.ids.includes(stored) ? stored : comp.ids[0];
  const index = comp.ids.indexOf(id);
  return {
    encodings: comp.statesById[id],
    augmentation: comp.augmentationById[id],
    chartType: comp.chartTypeById[id],
    surface: {
      key: comp.key,
      label: comp.label,
      length: comp.ids.length,
      index,
      ids: comp.ids,
      labels: comp.labels
    }
  };
}
var TRANSFORM_CHART_TYPE_KEY = "chartType";
var TRANSFORM_ARRANGE_KEY = "arrange";
function buildSurface(comp, id) {
  const index = Math.max(0, comp.ids.indexOf(id));
  return {
    key: comp.key,
    label: comp.label,
    length: comp.ids.length,
    index,
    ids: comp.ids,
    labels: comp.labels
  };
}
function computeArrangeStates(template, base, data) {
  return computePivot(template, base, data, void 0, {
    includeTransitions: false,
    key: TRANSFORM_ARRANGE_KEY,
    label: "Arrange",
    preferFacetTarget: true
  });
}
function computeChartTypeStates(template, base, data, resolveTemplate) {
  const transitions = getChartTransitions(template.chart);
  if (transitions.length === 0) return null;
  const ids = ["default"];
  const labels = [template.chart];
  const statesById = { default: clone(base) };
  const augmentationById = { default: void 0 };
  const chartTypeById = { default: void 0 };
  const seenChartTypes = /* @__PURE__ */ new Set([template.chart]);
  for (const t of transitions) {
    if (resolveTemplate && !resolveTemplate(t.to)) continue;
    const st = transitionState(base, data, template, t);
    if (!st) continue;
    if (seenChartTypes.has(st.chartType)) continue;
    seenChartTypes.add(st.chartType);
    const id = `type:${t.to}`;
    ids.push(id);
    labels.push(st.chartType);
    statesById[id] = st.enc;
    chartTypeById[id] = st.chartType;
  }
  if (ids.length <= 1) return null;
  return { key: TRANSFORM_CHART_TYPE_KEY, label: "Chart type", ids, labels, statesById, augmentationById, chartTypeById };
}
function resolveTransformOverrides(chartProperties) {
  let chartTypeId = chartProperties?.[TRANSFORM_CHART_TYPE_KEY];
  let arrangeId = chartProperties?.[TRANSFORM_ARRANGE_KEY];
  if (typeof chartTypeId !== "string") chartTypeId = void 0;
  if (typeof arrangeId !== "string") arrangeId = void 0;
  if (chartTypeId === void 0 && arrangeId === void 0) {
    const legacy = chartProperties?.pivot;
    if (typeof legacy === "string" && legacy.length > 0 && legacy !== "default") {
      const tokens = legacy.split("|");
      const typeIdx = tokens.findIndex((t) => t.startsWith("type:"));
      if (typeIdx >= 0) {
        chartTypeId = tokens[typeIdx];
        const local = tokens.slice(0, typeIdx);
        arrangeId = local.length ? local.join("|") : void 0;
      } else {
        arrangeId = legacy;
      }
    }
  }
  return { chartTypeId, arrangeId };
}
function applyTransform(template, base, data, chartProperties, resolveTemplate) {
  const { chartTypeId, arrangeId } = resolveTransformOverrides(chartProperties);
  let effectiveTemplate = template;
  let effectiveEnc = base;
  let chartType;
  let chartTypeSurface;
  const ctComp = computeChartTypeStates(template, base, data, resolveTemplate);
  if (ctComp && ctComp.ids.length > 1) {
    const id = chartTypeId && ctComp.ids.includes(chartTypeId) ? chartTypeId : "default";
    effectiveEnc = ctComp.statesById[id];
    chartType = ctComp.chartTypeById[id];
    if (chartType) {
      const resolved = resolveTemplate?.(chartType);
      if (resolved) effectiveTemplate = resolved;
    }
    chartTypeSurface = buildSurface(ctComp, id);
  }
  let encodings = effectiveEnc;
  let augmentation;
  let arrangeSurface;
  const arrComp = computeArrangeStates(effectiveTemplate, effectiveEnc, data);
  if (arrComp && arrComp.ids.length > 1) {
    const id = arrangeId && arrComp.ids.includes(arrangeId) ? arrangeId : arrComp.ids[0];
    encodings = arrComp.statesById[id];
    augmentation = arrComp.augmentationById[id];
    arrangeSurface = buildSurface(arrComp, id);
  }
  return {
    encodings,
    augmentation,
    chartType,
    surface: { chartType: chartTypeSurface, arrange: arrangeSurface }
  };
}

// src/chartjs/colormap.ts
var CHARTJS_COLOR_MAPS = [
  {
    id: "cat10",
    type: "categorical",
    supportsDiscrete: true,
    supportsContinuous: false,
    background: "any",
    maxCategories: 10,
    colorblindSafe: false,
    colors: [
      "#36a2eb",
      // blue
      "#ff6384",
      // red
      "#ffcd56",
      // yellow
      "#4bc0c0",
      // teal
      "#9966ff",
      // purple
      "#ff9f40",
      // orange
      "#2ecc71",
      // green
      "#34495e",
      // dark blue-grey
      "#e74c3c",
      // red-orange
      "#95a5a6"
      // grey
    ]
  },
  {
    id: "cat20",
    type: "categorical",
    supportsDiscrete: true,
    supportsContinuous: false,
    background: "any",
    maxCategories: 20,
    colorblindSafe: false,
    colors: [
      "#36a2eb",
      "#9ad0f5",
      "#ff6384",
      "#ff99aa",
      "#ffcd56",
      "#ffe39f",
      "#4bc0c0",
      "#8fdede",
      "#9966ff",
      "#c3a3ff",
      "#ff9f40",
      "#ffc078",
      "#2ecc71",
      "#7ee2a8",
      "#34495e",
      "#5d6d7e",
      "#e74c3c",
      "#f1948a",
      "#95a5a6",
      "#cfd4d6"
    ]
  },
  {
    id: "viridis",
    type: "sequential",
    supportsDiscrete: true,
    supportsContinuous: true,
    background: "any",
    colorblindSafe: true,
    colors: [
      "#440154",
      "#46327e",
      "#365c8d",
      "#277f8e",
      "#1fa187",
      "#4ac16d",
      "#a0da39",
      "#fde725"
    ]
  },
  {
    id: "RdBu",
    type: "diverging",
    supportsDiscrete: true,
    supportsContinuous: true,
    background: "any",
    diverging: true,
    preferredMidpoint: 0,
    colors: [
      "#b2182b",
      "#d6604d",
      "#f4a582",
      "#fddbc7",
      "#f7f7f7",
      "#d1e5f0",
      "#92c5de",
      "#4393c3",
      "#2166ac"
    ]
  }
];
function getMapById(id) {
  if (!id) return void 0;
  const key = String(id).toLowerCase();
  return CHARTJS_COLOR_MAPS.find((m) => m.id.toLowerCase() === key);
}
function getPaletteForScheme(id) {
  const entry = getMapById(id);
  return entry?.colors;
}
function pickChartJsPalette(decision) {
  if (!decision) {
    const fallback2 = getPaletteForScheme("cat10");
    return fallback2 && fallback2.length ? fallback2 : [];
  }
  const { schemeType, schemeId, categoryCount } = decision;
  if (schemeId) {
    const fromId = getPaletteForScheme(schemeId);
    if (fromId && fromId.length > 0) {
      return fromId;
    }
  }
  const mapsOfType = CHARTJS_COLOR_MAPS.filter((m) => m.type === schemeType);
  if (schemeType === "categorical") {
    const k = categoryCount ?? 0;
    if (mapsOfType.length) {
      const candidates = mapsOfType.filter((m) => m.supportsDiscrete);
      if (candidates.length) {
        const byCapacity = candidates.filter((m) => m.maxCategories == null || m.maxCategories >= k).sort((a, b) => (a.maxCategories ?? Infinity) - (b.maxCategories ?? Infinity));
        const picked = byCapacity[0] ?? candidates[0];
        if (picked.colors.length) {
          return picked.colors;
        }
      }
    }
    const fallback2 = getPaletteForScheme("cat10");
    if (fallback2 && fallback2.length) {
      return fallback2;
    }
  } else if (schemeType === "sequential") {
    const seq = mapsOfType.find((m) => m.supportsContinuous) ?? getMapById("viridis");
    if (seq && seq.colors.length) {
      return seq.colors;
    }
  } else if (schemeType === "diverging") {
    const divergingFirst = mapsOfType.find((m) => m.diverging) ?? getMapById("RdBu");
    if (divergingFirst && divergingFirst.colors.length) {
      return divergingFirst.colors;
    }
  }
  const fallback = getPaletteForScheme("cat10");
  return fallback && fallback.length ? fallback : [];
}

// src/chartjs/templates/utils.ts
var isDiscrete2 = (type) => type === "nominal" || type === "ordinal";
function extractCategories(data, field, ordinalSortOrder) {
  const seen = /* @__PURE__ */ new Set();
  const result = [];
  for (const row of data) {
    const val = row[field];
    if (val != null) {
      const key = String(val);
      if (!seen.has(key)) {
        seen.add(key);
        result.push(key);
      }
    }
  }
  if (ordinalSortOrder && ordinalSortOrder.length > 0) {
    const orderMap = new Map(ordinalSortOrder.map((v, i) => [v, i]));
    result.sort((a, b) => {
      const ia = orderMap.get(a);
      const ib = orderMap.get(b);
      if (ia !== void 0 && ib !== void 0) return ia - ib;
      if (ia !== void 0) return -1;
      if (ib !== void 0) return 1;
      return 0;
    });
  }
  return result;
}
function resolveCategoryOrder(data, catField, opts) {
  const base = extractCategories(data, catField, opts?.ordinalSortOrder);
  if (!opts?.sortBy) return base;
  const agg = /* @__PURE__ */ new Map();
  for (const row of data) {
    const cat = String(row[catField] ?? "");
    const v = Number(row[opts.sortBy]);
    if (Number.isFinite(v)) agg.set(cat, (agg.get(cat) ?? 0) + v);
  }
  const dir = opts.sortOrder === "ascending" ? 1 : -1;
  return [...base].sort((a, b) => dir * ((agg.get(a) ?? 0) - (agg.get(b) ?? 0)));
}
function groupBy(data, field) {
  const groups = /* @__PURE__ */ new Map();
  for (const row of data) {
    const key = String(row[field] ?? "");
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(row);
  }
  return groups;
}
function coerceUnixMsForChartJs(raw) {
  if (raw == null) return NaN;
  if (typeof raw === "number" && Number.isFinite(raw)) {
    return raw < 1e12 ? Math.round(raw * 1e3) : raw;
  }
  if (raw instanceof Date) {
    const t2 = raw.getTime();
    return Number.isFinite(t2) ? t2 : NaN;
  }
  const t = new Date(String(raw)).getTime();
  return Number.isFinite(t) ? t : NaN;
}
var DEFAULT_COLORS = [
  "rgba(54, 162, 235, 1)",
  // blue
  "rgba(255, 99, 132, 1)",
  // red
  "rgba(255, 206, 86, 1)",
  // yellow
  "rgba(75, 192, 192, 1)",
  // teal
  "rgba(153, 102, 255, 1)",
  // purple
  "rgba(255, 159, 64, 1)",
  // orange
  "rgba(46, 204, 113, 1)",
  // green
  "rgba(52, 73, 94, 1)",
  // dark blue-grey
  "rgba(231, 76, 60, 1)",
  // red-orange
  "rgba(149, 165, 166, 1)"
  // grey
];
var DEFAULT_BG_COLORS = [
  "rgba(54, 162, 235, 0.6)",
  "rgba(255, 99, 132, 0.6)",
  "rgba(255, 206, 86, 0.6)",
  "rgba(75, 192, 192, 0.6)",
  "rgba(153, 102, 255, 0.6)",
  "rgba(255, 159, 64, 0.6)",
  "rgba(46, 204, 113, 0.6)",
  "rgba(52, 73, 94, 0.6)",
  "rgba(231, 76, 60, 0.6)",
  "rgba(149, 165, 166, 0.6)"
];
function hexToRgb(hex) {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const intVal = parseInt(m[1], 16);
  return {
    r: intVal >> 16 & 255,
    g: intVal >> 8 & 255,
    b: intVal & 255
  };
}
function rgbaFromHex(hex, alpha) {
  const rgb = hexToRgb(hex);
  if (!rgb) return hex;
  const a = Math.max(0, Math.min(1, alpha));
  return `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${a})`;
}
function applyAlphaToColor(color, alpha) {
  const a = Math.max(0, Math.min(1, alpha));
  if (color.startsWith("#")) {
    return rgbaFromHex(color, a);
  }
  if (color.startsWith("rgba")) {
    return color.replace(
      /rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)/,
      (_m, r, g, b) => `rgba(${r}, ${g}, ${b}, ${a})`
    );
  }
  if (color.startsWith("rgb(")) {
    return color.replace(
      /rgb\((\d+),\s*(\d+),\s*(\d+)\)/,
      (_m, r, g, b) => `rgba(${r}, ${g}, ${b}, ${a})`
    );
  }
  return color;
}
function getChartJsPalette(ctx, preferred = "color") {
  const decisions = ctx.colorDecisions;
  const decision = preferred === "color" ? decisions?.color ?? decisions?.group : decisions?.group ?? decisions?.color;
  const palette = pickChartJsPalette(decision);
  if (palette.length > 0) {
    return palette;
  }
  return DEFAULT_COLORS;
}
function getSeriesBorderColor(palette, index) {
  if (!palette.length) {
    return DEFAULT_COLORS[index % DEFAULT_COLORS.length];
  }
  return palette[index % palette.length];
}
function getSeriesBackgroundColor(palette, index, alpha = 0.6) {
  const border = getSeriesBorderColor(palette, index);
  return applyAlphaToColor(border, alpha);
}
function detectAxes(channelSemantics) {
  const xCS = channelSemantics.x;
  const yCS = channelSemantics.y;
  if (xCS && isDiscrete2(xCS.type)) {
    return { categoryAxis: "x", valueAxis: "y" };
  }
  if (yCS && isDiscrete2(yCS.type)) {
    return { categoryAxis: "y", valueAxis: "x" };
  }
  return { categoryAxis: "x", valueAxis: "y" };
}
function buildCategoryAlignedData(rows, xField, yField, categories) {
  const map = /* @__PURE__ */ new Map();
  for (const row of rows) {
    const key = String(row[xField] ?? "");
    const val = row[yField];
    if (val != null && !isNaN(val)) {
      map.set(key, (map.get(key) ?? 0) + Number(val));
    }
  }
  return categories.map((cat) => map.get(cat) ?? null);
}

// src/chartjs/templates/scatter.ts
function computePointRadius(width, height, pointCount) {
  const canvasArea = width * height;
  const areaPerPoint = canvasArea / Math.max(1, pointCount);
  const idealRadius = Math.sqrt(areaPerPoint * 0.05) / 2;
  return Math.max(2, Math.min(6, Math.round(idealRadius)));
}
var cjsScatterPlotDef = {
  chart: "Scatter Plot",
  template: { mark: "circle", encoding: {} },
  channels: ["x", "y", "color", "size", "opacity", "column", "row"],
  markCognitiveChannel: "position",
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const xField = channelSemantics.x?.field;
    const yField = channelSemantics.y?.field;
    const colorField = channelSemantics.color?.field;
    if (!xField || !yField) return;
    const opacity = chartProperties?.opacity ?? 1;
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "scatter",
      data: { datasets: [] },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            type: "linear",
            title: { display: true, text: xField }
          },
          y: {
            type: "linear",
            title: { display: true, text: yField }
          }
        },
        plugins: {
          tooltip: { enabled: true }
        }
      }
    };
    if (channelSemantics.x?.zero) {
      config.options.scales.x.beginAtZero = channelSemantics.x.zero.zero !== false;
    }
    if (channelSemantics.y?.zero) {
      config.options.scales.y.beginAtZero = channelSemantics.y.zero.zero !== false;
    }
    if (colorField) {
      const groups = /* @__PURE__ */ new Map();
      for (const row of table) {
        const key = String(row[colorField] ?? "");
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push({ x: row[xField], y: row[yField] });
      }
      let colorIdx = 0;
      for (const [name, data] of groups) {
        config.data.datasets.push({
          label: name,
          data,
          backgroundColor: getSeriesBackgroundColor(palette, colorIdx, opacity),
          borderColor: getSeriesBorderColor(palette, colorIdx),
          borderWidth: 1,
          pointRadius: 4
        });
        colorIdx++;
      }
      config.options.plugins.legend = { display: true };
    } else {
      const data = table.map((row) => ({ x: row[xField], y: row[yField] }));
      config.data.datasets.push({
        data,
        backgroundColor: getSeriesBackgroundColor(palette, 0, opacity),
        borderColor: getSeriesBorderColor(palette, 0),
        borderWidth: 1,
        pointRadius: 4
      });
      config.options.plugins.legend = { display: false };
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "opacity", label: "Opacity", type: "continuous", min: 0.1, max: 1, step: 0.05, defaultValue: 1 }
  ],
  pivot: makeCartesianPivot({
    transpose: [["x", "y"]],
    permute: [["x", "y", "color", "size"]],
    shift: ["color", "group", "column", "row"]
    // θ transitions declared centrally in core/chart-transitions.ts.
  }),
  postProcess: (option, ctx) => {
    if (!option.data?.datasets) return;
    const w = option._width || ctx.canvasSize.width;
    const h = option._height || ctx.canvasSize.height;
    const pointCount = ctx.table.length;
    const radius = computePointRadius(w, h, pointCount);
    for (const ds of option.data.datasets) {
      if (ds.pointRadius == null || ds.pointRadius === 4) {
        ds.pointRadius = radius;
      }
    }
  }
};

// src/chartjs/templates/connected-scatter.ts
function sortByOrder(rows, field) {
  if (!field) return rows;
  const tagged = rows.map((row, idx) => ({ row, idx, key: row[field] }));
  const present = tagged.filter((t) => t.key != null && t.key !== "");
  const allNumeric = present.length > 0 && present.every((t) => typeof t.key === "number" || typeof t.key === "string" && t.key.trim() !== "" && !isNaN(Number(t.key)));
  const allDates = !allNumeric && present.length > 0 && present.every((t) => !isNaN(Date.parse(String(t.key))));
  const rank = (k) => {
    if (allNumeric) return Number(k);
    if (allDates) return Date.parse(String(k));
    return String(k);
  };
  return [...tagged].sort((a, b) => {
    const ra = rank(a.key);
    const rb = rank(b.key);
    if (ra < rb) return -1;
    if (ra > rb) return 1;
    return a.idx - b.idx;
  }).map((t) => t.row);
}
function toPoints(rows, xField, yField) {
  return rows.map((r) => ({ x: Number(r[xField]), y: Number(r[yField]) }));
}
var cjsConnectedScatterDef = {
  chart: "Connected Scatter Plot",
  template: { mark: "line", encoding: {} },
  channels: ["x", "y", "order", "color", "detail", "column", "row"],
  markCognitiveChannel: "position",
  instantiate: (spec, ctx) => {
    const { channelSemantics, table } = ctx;
    const xCS = channelSemantics.x;
    const yCS = channelSemantics.y;
    const orderField = channelSemantics.order?.field;
    const groupField = channelSemantics.color?.field ?? channelSemantics.detail?.field;
    if (!xCS?.field || !yCS?.field) return;
    const xField = xCS.field;
    const yField = yCS.field;
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "scatter",
      data: { datasets: [] },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            type: "linear",
            title: { display: true, text: xField },
            ticks: { font: { size: 10 } }
          },
          y: {
            type: "linear",
            title: { display: true, text: yField },
            ticks: { font: { size: 10 } }
          }
        },
        plugins: {
          tooltip: { enabled: true },
          legend: { display: false }
        }
      }
    };
    if (channelSemantics.x?.zero) {
      config.options.scales.x.beginAtZero = channelSemantics.x.zero.zero !== false;
    }
    if (channelSemantics.y?.zero) {
      config.options.scales.y.beginAtZero = channelSemantics.y.zero.zero !== false;
    }
    const baseDataset = {
      // Straight segments + visible points connecting the trajectory.
      showLine: true,
      tension: 0,
      borderWidth: 2,
      pointRadius: 4,
      fill: false,
      // Don't clip points at the chart-area edge: a point that lands
      // exactly on an axis bound would otherwise have its marker cut off.
      clip: false
    };
    if (groupField) {
      const groups = groupBy(table, groupField);
      config.options.plugins.legend = { display: true };
      let colorIdx = 0;
      for (const [name, rows] of groups) {
        const sorted = sortByOrder(rows, orderField);
        config.data.datasets.push({
          label: name,
          data: toPoints(sorted, xField, yField),
          borderColor: getSeriesBorderColor(palette, colorIdx),
          backgroundColor: getSeriesBackgroundColor(palette, colorIdx, 1),
          ...baseDataset
        });
        colorIdx++;
      }
    } else {
      const sorted = sortByOrder(table, orderField);
      config.data.datasets.push({
        label: yField,
        data: toPoints(sorted, xField, yField),
        borderColor: getSeriesBorderColor(palette, 0),
        backgroundColor: getSeriesBackgroundColor(palette, 0, 1),
        ...baseDataset
      });
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  }
};

// src/chartjs/templates/bubble.ts
function makeRadiusScale(values, rMin, rMax) {
  const finite = values.filter((v) => typeof v === "number" && isFinite(v));
  if (finite.length === 0) {
    const mid = Math.round((rMin + rMax) / 2);
    return () => mid;
  }
  const min = Math.min(...finite);
  const max = Math.max(...finite);
  if (min === max) {
    const mid = Math.round((rMin + rMax) / 2);
    return () => mid;
  }
  const aMin = rMin * rMin;
  const aMax = rMax * rMax;
  return (v) => {
    if (typeof v !== "number" || !isFinite(v)) return rMin;
    const t = (v - min) / (max - min);
    const area = aMin + t * (aMax - aMin);
    return Math.max(rMin, Math.sqrt(area));
  };
}
var cjsBubbleChartDef = {
  chart: "Bubble Chart",
  template: { mark: "circle", encoding: {} },
  channels: ["x", "y", "size", "color", "opacity", "column", "row"],
  markCognitiveChannel: "position",
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const xField = channelSemantics.x?.field;
    const yField = channelSemantics.y?.field;
    const sizeField = channelSemantics.size?.field;
    const colorField = channelSemantics.color?.field;
    if (!xField || !yField) return;
    const opacity = chartProperties?.opacity ?? 0.6;
    const palette = getChartJsPalette(ctx, "color");
    const sizeValues = sizeField ? table.map((row) => Number(row[sizeField])) : [];
    const radiusScale = makeRadiusScale(sizeValues, 5, 24);
    const toPoint = (row) => {
      const v = sizeField ? Number(row[sizeField]) : NaN;
      return {
        x: Number(row[xField]),
        y: Number(row[yField]),
        r: sizeField ? radiusScale(v) : 8,
        // Raw size value retained so postProcess can rescale to canvas.
        _v: v
      };
    };
    const config = {
      type: "bubble",
      data: { datasets: [] },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          // Bubble charts scale to the data extent (not a zero
          // baseline) and pad both ends with `grace` so large bubbles
          // sitting at the min/max aren't clipped by the plot edge.
          x: { type: "linear", grace: "10%", title: { display: true, text: xField } },
          y: { type: "linear", grace: "10%", title: { display: true, text: yField } }
        },
        plugins: {
          tooltip: { enabled: true }
        }
      }
    };
    if (colorField) {
      const groups = /* @__PURE__ */ new Map();
      for (const row of table) {
        const key = String(row[colorField] ?? "");
        if (!groups.has(key)) groups.set(key, []);
        groups.get(key).push(toPoint(row));
      }
      let colorIdx = 0;
      for (const [name, data] of groups) {
        config.data.datasets.push({
          label: name,
          data,
          backgroundColor: getSeriesBackgroundColor(palette, colorIdx, opacity),
          borderColor: getSeriesBorderColor(palette, colorIdx),
          borderWidth: 1
        });
        colorIdx++;
      }
      config.options.plugins.legend = { display: true };
    } else {
      config.data.datasets.push({
        data: table.map(toPoint),
        backgroundColor: getSeriesBackgroundColor(palette, 0, opacity),
        borderColor: getSeriesBorderColor(palette, 0),
        borderWidth: 1
      });
      config.options.plugins.legend = { display: false };
    }
    config._sizeField = sizeField ?? null;
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "opacity", label: "Opacity", type: "continuous", min: 0.1, max: 1, step: 0.05, defaultValue: 0.6 }
  ],
  postProcess: (option, ctx) => {
    if (!option.data?.datasets) return;
    const sizeField = option._sizeField ?? null;
    delete option._sizeField;
    if (!sizeField) {
      for (const ds of option.data.datasets) for (const pt of ds.data) delete pt._v;
      return;
    }
    const allValues = [];
    for (const ds of option.data.datasets) {
      for (const pt of ds.data) allValues.push(pt._v);
    }
    const w = option._width || ctx.canvasSize.width;
    const h = option._height || ctx.canvasSize.height;
    const minDim = Math.min(w, h);
    const count = Math.max(1, allValues.length);
    const rMaxByDensity = Math.sqrt(w * h / count * 0.08);
    const rMax = Math.max(8, Math.min(34, Math.round(Math.min(minDim * 0.09, rMaxByDensity))));
    const rMin = Math.max(3, Math.round(rMax * 0.22));
    const radiusScale = makeRadiusScale(allValues, rMin, rMax);
    for (const ds of option.data.datasets) {
      for (const pt of ds.data) {
        const v = pt._v;
        if (typeof v === "number" && isFinite(v)) pt.r = radiusScale(v);
        delete pt._v;
      }
    }
  }
};

// src/chartjs/templates/jitter.ts
var isDiscrete3 = (type) => type === "nominal" || type === "ordinal";
function clamp01(value) {
  return Math.max(0, Math.min(1, value));
}
function hexToRgb2(color) {
  const hex = color.trim().replace(/^#/, "");
  if (!/^[0-9a-fA-F]{6}$/.test(hex)) return void 0;
  return [
    parseInt(hex.slice(0, 2), 16),
    parseInt(hex.slice(2, 4), 16),
    parseInt(hex.slice(4, 6), 16)
  ];
}
function continuousColor(palette, value, min, max, alpha) {
  const colors = palette.length > 0 ? palette : ["#440154", "#fde725"];
  const span = max > min ? max - min : 1;
  const scaled = clamp01((value - min) / span) * (colors.length - 1);
  const lo = Math.floor(scaled);
  const hi = Math.min(colors.length - 1, Math.ceil(scaled));
  const t = scaled - lo;
  const left = hexToRgb2(colors[lo]);
  const right = hexToRgb2(colors[hi]);
  if (!left || !right) return colors[lo];
  const mix = (a, b) => Math.round(a + (b - a) * t);
  return `rgba(${mix(left[0], right[0])}, ${mix(left[1], right[1])}, ${mix(left[2], right[2])}, ${alpha})`;
}
function jitter(seed) {
  let state = seed;
  return () => {
    state = state * 1103515245 + 12345 & 2147483647;
    return state / 2147483647 * 2 - 1;
  };
}
var cjsStripPlotDef = {
  chart: "Strip Plot",
  template: { mark: "circle", encoding: {} },
  channels: ["x", "y", "color", "size", "column", "row"],
  markCognitiveChannel: "position",
  declareLayoutMode: () => ({
    paramOverrides: { defaultBandSize: 50, minStep: 16 }
  }),
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const xCS = channelSemantics.x;
    const yCS = channelSemantics.y;
    const xField = xCS?.field;
    const yField = yCS?.field;
    const colorField = channelSemantics.color?.field;
    const colorType = channelSemantics.color?.type;
    const isContinuousColor = !!colorField && (colorType === "quantitative" || colorType === "temporal");
    const isTemporalColor = colorType === "temporal";
    if (!xField || !yField) return;
    const xIsDiscrete = isDiscrete3(xCS?.type);
    const yIsDiscrete = isDiscrete3(yCS?.type);
    const catAxis = xIsDiscrete ? "x" : yIsDiscrete ? "y" : "x";
    const contAxis = catAxis === "x" ? "y" : "x";
    const catField = catAxis === "x" ? xField : yField;
    const contField = contAxis === "x" ? xField : yField;
    const catSemantics = catAxis === "x" ? xCS : yCS;
    const categories = extractCategories(table, catField, catSemantics?.ordinalSortOrder);
    const categoryIndex = new Map(categories.map((category, index) => [String(category), index]));
    const nextJitter = jitter(42);
    const jitterHalfWidth = 0.3;
    const palette = getChartJsPalette(ctx, isDiscrete3(colorType) ? "color" : "group");
    const opacity = chartProperties?.opacity ?? 0.7;
    const explicitPointSize = Number(chartProperties?.pointSize ?? 0);
    const pointRadius = explicitPointSize > 0 ? Math.max(1, Math.min(10, Math.round(explicitPointSize / 12))) : 3;
    const toColorValue = (value) => {
      if (value == null) return NaN;
      return isTemporalColor ? new Date(value).getTime() : Number(value);
    };
    const continuousColorValues = isContinuousColor && colorField ? table.map((row) => toColorValue(row[colorField])).filter((value) => Number.isFinite(value)) : [];
    const colorMin = continuousColorValues.length ? Math.min(...continuousColorValues) : 0;
    const colorMax = continuousColorValues.length ? Math.max(...continuousColorValues) : 1;
    const pointColors = isContinuousColor && colorField ? table.map((row) => continuousColor(palette, toColorValue(row[colorField]), colorMin, colorMax, opacity)) : void 0;
    const buildPoint = (row) => {
      const category = String(row[catField] ?? "");
      const index = categoryIndex.get(category) ?? 0;
      const categoryValue = index + nextJitter() * jitterHalfWidth;
      const continuousValue = row[contField];
      return catAxis === "x" ? { x: categoryValue, y: continuousValue } : { x: continuousValue, y: categoryValue };
    };
    const categoryScale = {
      type: "linear",
      min: -0.5,
      max: Math.max(0.5, categories.length - 0.5),
      title: { display: true, text: catField },
      ticks: {
        stepSize: 1,
        callback(value) {
          const index = Math.round(Number(value));
          return categories[index] ?? "";
        }
      }
    };
    const continuousScale = {
      type: "linear",
      title: { display: true, text: contField },
      ticks: { font: { size: 10 } }
    };
    const config = {
      type: "scatter",
      data: { datasets: [] },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: catAxis === "x" ? { x: categoryScale, y: continuousScale } : { x: continuousScale, y: categoryScale },
        plugins: {
          tooltip: { enabled: true },
          legend: { display: !!colorField && isDiscrete3(colorType) }
        }
      }
    };
    if (colorField && isDiscrete3(colorType)) {
      let colorIndex = 0;
      for (const [name, rows] of groupBy(table, colorField)) {
        config.data.datasets.push({
          label: name,
          data: rows.map(buildPoint),
          backgroundColor: getSeriesBackgroundColor(palette, colorIndex, opacity),
          borderColor: getSeriesBorderColor(palette, colorIndex),
          borderWidth: 1,
          pointRadius
        });
        colorIndex++;
      }
    } else {
      config.data.datasets.push({
        data: table.map(buildPoint),
        backgroundColor: pointColors ?? getSeriesBackgroundColor(palette, 0, opacity),
        borderColor: pointColors ?? getSeriesBorderColor(palette, 0),
        borderWidth: 1,
        pointRadius
      });
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "stepWidth", label: "Jitter", type: "continuous", min: 10, max: 100, step: 5, defaultValue: 20 },
    { key: "pointSize", label: "Size", type: "continuous", min: 0, max: 150, step: 5, defaultValue: 0 },
    { key: "opacity", label: "Opacity", type: "continuous", min: 0, max: 1, step: 0.05, defaultValue: 0 }
  ],
  pivot: makeCartesianPivot({
    // θ (Strip → Scatter) declared centrally in core/chart-transitions.ts.
  })
};

// src/core/axis-detection.ts
var isDiscrete4 = (type) => type === "nominal" || type === "ordinal";
var getFieldCardinality = (field, table) => new Set(table.map((row) => row[field]).filter((value) => value != null)).size;
function resolveDiscreteType(currentType, field, table) {
  if (currentType === "nominal") return "nominal";
  if (currentType === "ordinal") return "ordinal";
  if (currentType === "temporal") return "ordinal";
  if (currentType === "quantitative" && field && table.length > 0) {
    return getFieldCardinality(field, table) <= 20 ? "ordinal" : "nominal";
  }
  return "nominal";
}
function detectBandedAxisFromSemantics(channelSemantics, table, options = {}) {
  const xType = channelSemantics.x?.type;
  const yType = channelSemantics.y?.type;
  if (xType && isDiscrete4(xType)) return { axis: "x" };
  if (yType && isDiscrete4(yType)) return { axis: "y" };
  if (xType && yType) {
    if (xType === "quantitative" && yType !== "quantitative") {
      return { axis: "y" };
    }
    if (yType === "quantitative" && xType !== "quantitative") {
      return { axis: "x" };
    }
    return { axis: options.preferAxis || "x" };
  }
  if (xType) {
    const newType = resolveDiscreteType(xType, channelSemantics.x?.field, table);
    return { axis: "x", resolvedTypes: { x: newType } };
  }
  if (yType) {
    const newType = resolveDiscreteType(yType, channelSemantics.y?.field, table);
    return { axis: "y", resolvedTypes: { y: newType } };
  }
  return null;
}
function detectBandedAxisForceDiscrete(channelSemantics, table, options = {}) {
  const result = detectBandedAxisFromSemantics(channelSemantics, table, options);
  if (!result) return null;
  const axis = result.axis;
  const semantics = channelSemantics[axis];
  if (!semantics) return result;
  if (!isDiscrete4(semantics.type)) {
    const newType = resolveDiscreteType(semantics.type, semantics.field, table);
    return {
      axis,
      resolvedTypes: { ...result.resolvedTypes, [axis]: newType }
    };
  }
  return result;
}

// src/core/band-dodge.ts
var DEFAULT_NESTED_SNAP_THRESHOLD = 0.9;
function recommendMode(maxPerBand, globalCount, nestedFraction, threshold) {
  if (maxPerBand <= 1) return "none";
  if (nestedFraction >= threshold) return "none";
  if (maxPerBand >= globalCount) return "global";
  return "local";
}
function planBandDodge(table, axisField, subField, options) {
  const perBand = /* @__PURE__ */ new Map();
  const global = /* @__PURE__ */ new Set();
  for (const row of table) {
    global.add(row[subField]);
    const key = row[axisField];
    let bandSet = perBand.get(key);
    if (!bandSet) perBand.set(key, bandSet = /* @__PURE__ */ new Set());
    bandSet.add(row[subField]);
  }
  const globalCount = Math.max(1, global.size);
  const bandCount = perBand.size;
  let maxPerBand = 0;
  let singleValuedBands = 0;
  let completeBands = 0;
  for (const bandSet of perBand.values()) {
    if (bandSet.size > maxPerBand) maxPerBand = bandSet.size;
    if (bandSet.size <= 1) singleValuedBands++;
    if (bandSet.size === globalCount) completeBands++;
  }
  const threshold = DEFAULT_NESTED_SNAP_THRESHOLD;
  const nestedFraction = bandCount > 0 ? singleValuedBands / bandCount : 1;
  const mode = recommendMode(maxPerBand, globalCount, nestedFraction, threshold);
  return {
    mode,
    dodge: mode !== "none",
    laneCount: globalCount,
    ambiguous: maxPerBand > 1 && completeBands < bandCount,
    maxPerBand,
    global: globalCount,
    bandCount
  };
}
function laneCountForMode(plan, mode) {
  if (mode === "global") return plan.global;
  if (mode === "local") return Math.max(1, plan.maxPerBand);
  return 1;
}
function resolveDodge(plan, override) {
  let mode = override === "none" || override === "local" || override === "global" ? override : plan.mode;
  if (mode !== "none" && plan.maxPerBand <= 1) mode = "none";
  return { mode, laneCount: laneCountForMode(plan, mode) };
}

// src/core/encoding-actions.ts
var isMeasureEnc = (e) => !!e?.field && (!!e.aggregate || e.type === "quantitative");
var isDiscreteCategoryEnc = (e) => !!e?.field && !e.aggregate && e.type !== "quantitative" && e.type !== "temporal";
function resolveSortChannels(encodings, candidates) {
  const category = candidates.find((c) => isDiscreteCategoryEnc(encodings[c]));
  const measure = candidates.find((c) => isMeasureEnc(encodings[c]));
  if (!category || !measure || category === measure) return null;
  return { category, measure };
}
function makeSortAction(options) {
  const candidates = ["x", "y"];
  return {
    key: "sort",
    label: "Sort",
    dependencies: candidates,
    isApplicable: (ctx) => resolveSortChannels(ctx.encodings, candidates) !== null,
    control: {
      type: "discrete",
      options: [
        { value: void 0, label: "Default" },
        { value: "value-desc", label: "Value \u2193" },
        { value: "value-asc", label: "Value \u2191" }
      ]
    },
    get: (encodings) => {
      const resolved = resolveSortChannels(encodings, candidates);
      if (!resolved) return void 0;
      const { category, measure } = resolved;
      const enc = encodings[category];
      if (enc.sortBy === measure) {
        return enc.sortOrder === "descending" ? "value-desc" : "value-asc";
      }
      return void 0;
    },
    set: (encodings, value) => {
      const resolved = resolveSortChannels(encodings, candidates);
      if (!resolved) return encodings;
      const { category, measure } = resolved;
      const base = encodings[category];
      let next;
      switch (value) {
        case "value-asc":
          next = { ...base, sortBy: measure, sortOrder: "ascending" };
          break;
        case "value-desc":
          next = { ...base, sortBy: measure, sortOrder: "descending" };
          break;
        default:
          next = { ...base, sortBy: void 0, sortOrder: void 0 };
      }
      return { ...encodings, [category]: next };
    }
  };
}

// src/chartjs/templates/bar.ts
var isDiscrete5 = (type) => type === "nominal" || type === "ordinal";
var cjsBarChartDef = {
  chart: "Bar Chart",
  template: { mark: "bar", encoding: {} },
  channels: ["x", "y", "color", "opacity", "column", "row"],
  markCognitiveChannel: "length",
  declareLayoutMode: (cs, table) => {
    const result = detectBandedAxisFromSemantics(cs, table, { preferAxis: "x" });
    return {
      axisFlags: result ? { [result.axis]: { banded: true } } : { x: { banded: true } },
      resolvedTypes: result?.resolvedTypes
    };
  },
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const { categoryAxis, valueAxis } = detectAxes(channelSemantics);
    const catField = channelSemantics[categoryAxis]?.field;
    const valField = channelSemantics[valueAxis]?.field;
    if (!catField || !valField) return;
    const catCS = channelSemantics[categoryAxis];
    const catEnc = ctx.encodings?.[categoryAxis];
    const sortByField = catEnc?.sortBy ? channelSemantics[catEnc.sortBy]?.field : void 0;
    const categories = resolveCategoryOrder(table, catField, {
      ordinalSortOrder: catCS?.ordinalSortOrder,
      sortBy: sortByField,
      sortOrder: catEnc?.sortOrder
    });
    const values = buildCategoryAlignedData(table, catField, valField, categories);
    const isHorizontal = categoryAxis === "y";
    const palette = getChartJsPalette(ctx);
    const config = {
      type: "bar",
      data: {
        labels: categories,
        datasets: [{
          label: valField,
          data: values,
          backgroundColor: getSeriesBackgroundColor(palette, 0),
          borderColor: getSeriesBorderColor(palette, 0),
          borderWidth: 1,
          borderRadius: chartProperties?.cornerRadius ?? 0
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        indexAxis: isHorizontal ? "y" : "x",
        scales: {
          x: {
            title: { display: true, text: isHorizontal ? valField : catField },
            ...isHorizontal ? {} : {}
          },
          y: {
            title: { display: true, text: isHorizontal ? catField : valField }
          }
        },
        plugins: {
          legend: { display: false },
          tooltip: { enabled: true }
        }
      }
    };
    const valScale = isHorizontal ? "x" : "y";
    const valCS = channelSemantics[valueAxis];
    if (valCS?.zero) {
      config.options.scales[valScale].beginAtZero = valCS.zero.zero !== false;
    } else {
      config.options.scales[valScale].beginAtZero = true;
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "cornerRadius", label: "Corners", type: "continuous", min: 0, max: 15, step: 1, defaultValue: 0 }
  ],
  encodingActions: [makeSortAction()],
  pivot: makeCartesianPivot({
    transpose: [["x", "y"]],
    permute: [["x", "y", "color"]],
    shift: ["color", "column", "row"]
  })
};
var cjsStackedBarChartDef = {
  chart: "Stacked Bar Chart",
  template: { mark: "bar", encoding: {} },
  channels: ["x", "y", "color", "column", "row"],
  markCognitiveChannel: "length",
  declareLayoutMode: (cs, table) => {
    const result = detectBandedAxisFromSemantics(cs, table, { preferAxis: "x" });
    return {
      axisFlags: result ? { [result.axis]: { banded: true } } : { x: { banded: true } },
      resolvedTypes: result?.resolvedTypes,
      paramOverrides: { continuousMarkCrossSection: { x: 20, y: 20, seriesCountAxis: "auto" } }
    };
  },
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const { categoryAxis, valueAxis } = detectAxes(channelSemantics);
    const colorField = channelSemantics.color?.field;
    const hasStackSeries = !!colorField;
    const catField = channelSemantics[categoryAxis]?.field;
    const valField = channelSemantics[valueAxis]?.field;
    if (!catField || !valField) return;
    const catCS = channelSemantics[categoryAxis];
    const catEnc = ctx.encodings?.[categoryAxis];
    const sortByField = catEnc?.sortBy ? channelSemantics[catEnc.sortBy]?.field : void 0;
    const categories = resolveCategoryOrder(table, catField, {
      ordinalSortOrder: catCS?.ordinalSortOrder,
      sortBy: sortByField,
      sortOrder: catEnc?.sortOrder
    });
    const isHorizontal = categoryAxis === "y";
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "bar",
      data: {
        labels: categories,
        datasets: []
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        indexAxis: isHorizontal ? "y" : "x",
        scales: {
          x: {
            stacked: hasStackSeries,
            title: { display: true, text: isHorizontal ? valField : catField }
          },
          y: {
            stacked: hasStackSeries,
            title: { display: true, text: isHorizontal ? catField : valField }
          }
        },
        plugins: {
          legend: { display: !!colorField },
          tooltip: { enabled: true }
        }
      }
    };
    if (colorField) {
      const groups = groupBy(table, colorField);
      let colorIdx = 0;
      for (const [name, rows] of groups) {
        const values = buildCategoryAlignedData(rows, catField, valField, categories);
        config.data.datasets.push({
          label: name,
          data: values,
          backgroundColor: getSeriesBackgroundColor(palette, colorIdx),
          borderColor: getSeriesBorderColor(palette, colorIdx),
          borderWidth: 1
        });
        colorIdx++;
      }
    } else {
      const values = buildCategoryAlignedData(table, catField, valField, categories);
      config.data.datasets.push({
        label: valField,
        data: values,
        backgroundColor: getSeriesBackgroundColor(palette, 0),
        borderColor: getSeriesBorderColor(palette, 0),
        borderWidth: 1
      });
    }
    const valScaleS = isHorizontal ? "x" : "y";
    const valCSs = channelSemantics[valueAxis];
    if (valCSs?.zero) {
      config.options.scales[valScaleS].beginAtZero = valCSs.zero.zero !== false;
    } else {
      config.options.scales[valScaleS].beginAtZero = true;
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  encodingActions: [makeSortAction()],
  pivot: makeCartesianPivot({
    transpose: [["x", "y"]],
    permute: [["x", "y", "color"]],
    shift: ["color", "group", "column", "row"]
    // θ (→ Grouped Bar) declared centrally in core/chart-transitions.ts.
  })
};
var cjsGroupedBarChartDef = {
  chart: "Grouped Bar Chart",
  template: { mark: "bar", encoding: {} },
  channels: ["x", "y", "group", "color", "column", "row"],
  markCognitiveChannel: "length",
  declareLayoutMode: (cs, table, chartProperties) => {
    const result = detectBandedAxisForceDiscrete(cs, table, { preferAxis: "x" });
    const decl = {
      axisFlags: result ? { [result.axis]: { banded: true } } : { x: { banded: true } },
      resolvedTypes: result?.resolvedTypes,
      paramOverrides: { continuousMarkCrossSection: { x: 20, y: 20, seriesCountAxis: "auto" } }
    };
    const groupField = cs.group?.field || cs.color?.field;
    const axisField = result?.axis ? cs[result.axis]?.field : isDiscrete5(cs.x?.type) ? cs.x?.field : cs.y?.field;
    if (groupField && axisField) {
      const plan = planBandDodge(table, axisField, groupField);
      const { mode } = resolveDodge(plan, chartProperties?.dodge);
      if (mode === "local") decl.groupLaneCount = Math.max(1, plan.maxPerBand);
    }
    return decl;
  },
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const { categoryAxis, valueAxis } = detectAxes(channelSemantics);
    const groupField = channelSemantics.group?.field || channelSemantics.color?.field;
    const catField = channelSemantics[categoryAxis]?.field;
    const valField = channelSemantics[valueAxis]?.field;
    if (!catField || !valField) return;
    const catCS = channelSemantics[categoryAxis];
    const catEnc = ctx.encodings?.[categoryAxis];
    const sortByField = catEnc?.sortBy ? channelSemantics[catEnc.sortBy]?.field : void 0;
    const categories = resolveCategoryOrder(table, catField, {
      ordinalSortOrder: catCS?.ordinalSortOrder,
      sortBy: sortByField,
      sortOrder: catEnc?.sortOrder
    });
    const isHorizontal = categoryAxis === "y";
    const palette = getChartJsPalette(ctx, "group");
    const dodgePlan = groupField ? planBandDodge(table, catField, groupField) : null;
    const dodgeMode = dodgePlan ? resolveDodge(dodgePlan, chartProperties?.dodge).mode : "none";
    const config = {
      type: "bar",
      data: {
        labels: categories,
        datasets: []
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        indexAxis: isHorizontal ? "y" : "x",
        scales: {
          x: {
            title: { display: true, text: isHorizontal ? valField : catField }
          },
          y: {
            title: { display: true, text: isHorizontal ? catField : valField }
          }
        },
        plugins: {
          legend: { display: !!groupField },
          tooltip: { enabled: true }
        }
      }
    };
    if (groupField && dodgeMode === "local") {
      const globalGroups = [...new Set(table.map((r) => String(r[groupField] ?? "")))].filter(Boolean).sort();
      const colorFor = (g) => getSeriesBackgroundColor(palette, Math.max(0, globalGroups.indexOf(g)));
      const borderFor = (g) => getSeriesBorderColor(palette, Math.max(0, globalGroups.indexOf(g)));
      const perBand = /* @__PURE__ */ new Map();
      for (const cat of categories) perBand.set(cat, []);
      for (const r of table) {
        const cat = String(r[catField] ?? "");
        const g = String(r[groupField] ?? "");
        if (!perBand.has(cat) || !g) continue;
        const arr = perBand.get(cat);
        if (!arr.includes(g)) arr.push(g);
      }
      for (const arr of perBand.values()) arr.sort();
      const maxPerBand = Math.max(1, ...[...perBand.values()].map((a) => a.length));
      const valAt = /* @__PURE__ */ new Map();
      for (const r of table) {
        const v = Number(r[valField]);
        if (isFinite(v)) valAt.set(`${r[catField]}\0${r[groupField]}`, v);
      }
      for (let lane = 0; lane < maxPerBand; lane++) {
        const data = [];
        const bg = [];
        const bd = [];
        for (const cat of categories) {
          const g = perBand.get(cat)?.[lane];
          if (g === void 0) {
            data.push(null);
            bg.push("transparent");
            bd.push("transparent");
            continue;
          }
          const v = valAt.get(`${cat}\0${g}`);
          data.push(v === void 0 ? null : v);
          bg.push(colorFor(g));
          bd.push(borderFor(g));
        }
        config.data.datasets.push({
          label: `__lane${lane}`,
          data,
          backgroundColor: bg,
          borderColor: bd,
          borderWidth: 1
        });
      }
      config.options.plugins.legend = {
        display: true,
        labels: {
          generateLabels: () => globalGroups.map((g) => ({
            text: g,
            fillStyle: colorFor(g),
            strokeStyle: borderFor(g),
            lineWidth: 1
          }))
        }
      };
    } else if (groupField) {
      const groups = groupBy(table, groupField);
      let colorIdx = 0;
      for (const [name, rows] of groups) {
        const values = buildCategoryAlignedData(rows, catField, valField, categories);
        config.data.datasets.push({
          label: name,
          data: values,
          backgroundColor: getSeriesBackgroundColor(palette, colorIdx),
          borderColor: getSeriesBorderColor(palette, colorIdx),
          borderWidth: 1
        });
        colorIdx++;
      }
    } else {
      const values = buildCategoryAlignedData(table, catField, valField, categories);
      config.data.datasets.push({
        label: valField,
        data: values,
        backgroundColor: getSeriesBackgroundColor(palette, 0),
        borderColor: getSeriesBorderColor(palette, 0),
        borderWidth: 1
      });
    }
    const valScaleG = isHorizontal ? "x" : "y";
    const valCSg = channelSemantics[valueAxis];
    if (valCSg?.zero) {
      config.options.scales[valScaleG].beginAtZero = valCSg.zero.zero !== false;
    } else {
      config.options.scales[valScaleG].beginAtZero = true;
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    {
      key: "dodge",
      label: "Dodge",
      type: "discrete",
      options: [
        { value: "auto", label: "Auto" },
        { value: "local", label: "Local (compact)" },
        { value: "global", label: "Global (aligned)" }
      ],
      defaultValue: "auto",
      check: (ctx) => {
        const isDisc = (t) => t === "nominal" || t === "ordinal";
        const groupField = ctx.channelSemantics?.group?.field ?? ctx.channelSemantics?.color?.field ?? ctx.encodings?.group?.field ?? ctx.encodings?.color?.field;
        const axisField = isDisc(ctx.channelSemantics?.x?.type) ? ctx.channelSemantics?.x?.field : ctx.channelSemantics?.y?.field;
        const rows = ctx.data;
        if (!groupField || !axisField || !rows) return { applicable: false };
        const plan = planBandDodge(rows, axisField, groupField);
        return { applicable: plan.ambiguous, recommendedValue: plan.mode === "none" ? "auto" : plan.mode };
      }
    }
  ],
  encodingActions: [makeSortAction()],
  pivot: makeCartesianPivot({
    transpose: [["x", "y"]],
    permute: [["x", "y", "color"]],
    shift: ["color", "group", "column", "row"]
    // θ (→ Stacked Bar) declared centrally in core/chart-transitions.ts.
  })
};

// src/chartjs/templates/lollipop.ts
var STEM_COLOR = "#000000";
var STEM_WIDTH_PX = 1.5;
var STEM_LABEL = "__stem__";
function dotRadiusFromProperty(dotSize) {
  const diameterPx = Math.max(6, Math.min(10 + (dotSize - 80) / 40, 16));
  return diameterPx / 2;
}
var cjsLollipopChartDef = {
  chart: "Lollipop Chart",
  template: { mark: "bar", encoding: {} },
  channels: ["x", "y", "color", "column", "row"],
  markCognitiveChannel: "length",
  declareLayoutMode: (cs, table) => {
    const result = detectBandedAxisFromSemantics(cs, table, { preferAxis: "x" });
    return {
      axisFlags: result ? { [result.axis]: { banded: true } } : { x: { banded: true } },
      resolvedTypes: result?.resolvedTypes
    };
  },
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const { categoryAxis, valueAxis } = detectAxes(channelSemantics);
    const catField = channelSemantics[categoryAxis]?.field;
    const valField = channelSemantics[valueAxis]?.field;
    if (!catField || !valField || table.length === 0) return;
    const colorField = channelSemantics.color?.field;
    const categories = extractCategories(
      table,
      catField,
      channelSemantics[categoryAxis]?.ordinalSortOrder
    );
    const stemData = buildCategoryAlignedData(table, catField, valField, categories);
    const isHorizontal = categoryAxis === "y";
    const pointRadius = dotRadiusFromProperty(Number(chartProperties?.dotSize ?? 80));
    const palette = getChartJsPalette(ctx, "color");
    const datasets = [{
      type: "bar",
      label: STEM_LABEL,
      data: stemData,
      barThickness: STEM_WIDTH_PX,
      backgroundColor: STEM_COLOR,
      borderWidth: 0,
      order: 2
    }];
    const dotDataset = (label, data, colorIndex) => ({
      type: "line",
      label,
      data,
      showLine: false,
      pointRadius,
      pointHoverRadius: pointRadius + 2,
      borderColor: getSeriesBorderColor(palette, colorIndex),
      backgroundColor: getSeriesBorderColor(palette, colorIndex),
      pointBorderColor: "#fff",
      pointBorderWidth: 1,
      order: 1
    });
    if (colorField) {
      let i = 0;
      for (const [name, rows] of groupBy(table, colorField)) {
        datasets.push(dotDataset(
          name,
          buildCategoryAlignedData(rows, catField, valField, categories),
          i
        ));
        i++;
      }
    } else {
      datasets.push(dotDataset(valField, stemData, 0));
    }
    const zeroDecision = channelSemantics[valueAxis]?.zero;
    const config = {
      type: "bar",
      data: { labels: categories, datasets },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        ...isHorizontal ? { indexAxis: "y" } : {},
        scales: {
          [categoryAxis]: {
            title: { display: true, text: catField }
          },
          [valueAxis]: {
            type: "linear",
            beginAtZero: zeroDecision ? zeroDecision.zero !== false : true,
            title: { display: true, text: valField }
          }
        },
        plugins: {
          legend: {
            display: !!colorField,
            labels: { filter: (item) => item.text !== STEM_LABEL }
          },
          tooltip: {
            enabled: true,
            filter: (item) => item.dataset?.label !== STEM_LABEL
          }
        }
      }
    };
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "dotSize", label: "Dot Size", type: "continuous", min: 20, max: 300, step: 10, defaultValue: 80 }
  ]
};

// src/chartjs/templates/combo.ts
function isNumericField(table, field) {
  let total = 0;
  let numeric = 0;
  for (const row of table) {
    const v = row[field];
    if (v == null || v === "") continue;
    total++;
    if (typeof v === "number" ? isFinite(v) : !isNaN(Number(v))) numeric++;
  }
  return total > 0 && numeric / total >= 0.9;
}
var cjsComboChartDef = {
  chart: "Combo Chart",
  template: { mark: "bar", encoding: {} },
  channels: ["x", "y", "column", "row"],
  markCognitiveChannel: "length",
  declareLayoutMode: (cs, table) => {
    const result = detectBandedAxisFromSemantics(cs, table, { preferAxis: "x" });
    return {
      axisFlags: result ? { [result.axis]: { banded: true } } : { x: { banded: true } },
      resolvedTypes: result?.resolvedTypes
    };
  },
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const catField = channelSemantics.x?.field;
    const barField = channelSemantics.y?.field;
    if (!catField || !barField || table.length === 0) return;
    const lineField = chartProperties?.lineField && chartProperties.lineField in table[0] ? chartProperties.lineField : Object.keys(table[0]).find(
      (k) => k !== catField && k !== barField && isNumericField(table, k)
    );
    const categories = extractCategories(table, catField, channelSemantics.x?.ordinalSortOrder);
    const barData = buildCategoryAlignedData(table, catField, barField, categories);
    const palette = getChartJsPalette(ctx, "color");
    const datasets = [{
      type: "bar",
      label: barField,
      data: barData,
      yAxisID: "y",
      order: 2,
      backgroundColor: getSeriesBackgroundColor(palette, 0),
      borderColor: getSeriesBorderColor(palette, 0),
      borderWidth: 1,
      borderRadius: chartProperties?.cornerRadius ?? 0
    }];
    if (lineField) {
      const lineData = buildCategoryAlignedData(table, catField, lineField, categories);
      datasets.push({
        type: "line",
        label: lineField,
        data: lineData,
        yAxisID: "y1",
        order: 1,
        borderColor: getSeriesBorderColor(palette, 1),
        backgroundColor: "transparent",
        borderWidth: 2,
        pointRadius: 3,
        tension: 0.3,
        fill: false
      });
    }
    const config = {
      type: "bar",
      data: { labels: categories, datasets },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            title: { display: true, text: catField }
          },
          y: {
            type: "linear",
            position: "left",
            beginAtZero: channelSemantics.y?.zero ? channelSemantics.y.zero.zero !== false : true,
            title: { display: true, text: barField }
          },
          ...lineField ? {
            y1: {
              type: "linear",
              position: "right",
              title: { display: true, text: lineField },
              // Don't draw the right axis grid over the bars.
              grid: { drawOnChartArea: false }
            }
          } : {}
        },
        plugins: {
          legend: { display: true },
          tooltip: { enabled: true }
        }
      }
    };
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "cornerRadius", label: "Corners", type: "continuous", min: 0, max: 15, step: 1, defaultValue: 0 }
  ]
};

// src/chartjs/templates/line.ts
var isDiscrete6 = (type) => type === "nominal" || type === "ordinal";
var cjsLineChartDef = {
  chart: "Line Chart",
  template: { mark: "line", encoding: {} },
  channels: ["x", "y", "color", "opacity", "column", "row"],
  markCognitiveChannel: "position",
  declareLayoutMode: () => ({
    paramOverrides: { continuousMarkCrossSection: { x: 100, y: 20, seriesCountAxis: "auto" }, facetAspectRatioResistance: 0.5 }
  }),
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, fullTable, chartProperties } = ctx;
    const xCS = channelSemantics.x;
    const yCS = channelSemantics.y;
    const colorField = channelSemantics.color?.field;
    if (!xCS?.field || !yCS?.field) return;
    const xField = xCS.field;
    const yField = yCS.field;
    const xIsDiscrete = isDiscrete6(xCS.type);
    const xIsTemporal = xCS.type === "temporal";
    const mapContinuousX = (raw) => xIsTemporal ? coerceUnixMsForChartJs(raw) : raw;
    let continuousXExtent;
    if (!xIsDiscrete) {
      const xNums = (fullTable ?? table).map((r) => mapContinuousX(r[xField])).filter((v) => typeof v === "number" && Number.isFinite(v));
      if (xNums.length > 0) {
        continuousXExtent = { min: Math.min(...xNums), max: Math.max(...xNums) };
      }
    }
    const categories = xIsDiscrete ? extractCategories(table, xField, xCS.ordinalSortOrder) : void 0;
    const interpolate = chartProperties?.interpolate;
    const tension = interpolate === "monotone" || interpolate === "basis" || interpolate === "cardinal" || interpolate === "catmull-rom" ? 0.4 : 0;
    const stepped = interpolate === "step" ? "middle" : interpolate === "step-before" ? "before" : interpolate === "step-after" ? "after" : false;
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "line",
      data: {
        labels: categories || [],
        datasets: []
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            type: xIsDiscrete ? "category" : "linear",
            title: { display: true, text: xField },
            ...continuousXExtent ? { min: continuousXExtent.min, max: continuousXExtent.max } : {},
            ticks: {
              font: { size: 10 },
              ...xIsTemporal ? {
                maxTicksLimit: 4,
                autoSkip: true,
                maxRotation: 0,
                callback(v) {
                  const n = typeof v === "number" ? v : Number(v);
                  if (!Number.isFinite(n)) return String(v);
                  const spanDays = continuousXExtent ? (continuousXExtent.max - continuousXExtent.min) / 864e5 : 0;
                  const opts = spanDays > 60 ? { month: "short", year: "numeric" } : { month: "short", day: "numeric", year: "numeric" };
                  return new Date(n).toLocaleDateString(void 0, opts);
                }
              } : {}
            }
          },
          y: {
            type: "linear",
            title: { display: true, text: yField },
            ticks: { font: { size: 10 } }
          }
        },
        plugins: {
          tooltip: { enabled: true }
        }
      }
    };
    if (channelSemantics.y?.zero) {
      config.options.scales.y.beginAtZero = channelSemantics.y.zero.zero !== false;
    }
    if (colorField) {
      const groups = groupBy(table, colorField);
      config.options.plugins.legend = { display: true };
      let colorIdx = 0;
      for (const [name, rows] of groups) {
        const data = xIsDiscrete ? buildCategoryAlignedData(rows, xField, yField, categories) : rows.map((r) => ({ x: mapContinuousX(r[xField]), y: r[yField] })).filter((p) => p.y != null && (xIsTemporal ? Number.isFinite(p.x) : true));
        config.data.datasets.push({
          label: name,
          data,
          borderColor: getSeriesBorderColor(palette, colorIdx),
          backgroundColor: "transparent",
          tension,
          stepped,
          pointRadius: 3,
          fill: false
        });
        colorIdx++;
      }
    } else {
      const data = xIsDiscrete ? categories.map((cat) => {
        const row = table.find((r) => String(r[xField]) === cat);
        return row ? row[yField] : null;
      }) : table.map((r) => ({ x: mapContinuousX(r[xField]), y: r[yField] })).filter((p) => p.y != null && (xIsTemporal ? Number.isFinite(p.x) : true));
      config.data.datasets.push({
        label: yField,
        data,
        borderColor: getSeriesBorderColor(palette, 0),
        backgroundColor: "transparent",
        tension,
        stepped,
        pointRadius: 3,
        fill: false
      });
      config.options.plugins.legend = { display: false };
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    {
      key: "interpolate",
      label: "Curve",
      type: "discrete",
      options: [
        { value: void 0, label: "Default (linear)" },
        { value: "linear", label: "Linear" },
        { value: "monotone", label: "Monotone (smooth)" },
        { value: "step", label: "Step" },
        { value: "step-before", label: "Step Before" },
        { value: "step-after", label: "Step After" },
        { value: "basis", label: "Basis (smooth)" },
        { value: "cardinal", label: "Cardinal" },
        { value: "catmull-rom", label: "Catmull-Rom" }
      ]
    }
  ],
  pivot: makeCartesianPivot({
    permute: [["y", "color"]],
    shift: ["color", "group", "column", "row"]
  })
};

// src/core/type-registry.ts
var TYPE_REGISTRY = {
  // --- Temporal: DateTime ---
  DateTime: { t0: "Temporal", t1: "DateTime", visEncodings: ["temporal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Date: { t0: "Temporal", t1: "DateTime", visEncodings: ["temporal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Time: { t0: "Temporal", t1: "DateTime", visEncodings: ["temporal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Timestamp: { t0: "Temporal", t1: "DateTime", visEncodings: ["temporal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  // --- Temporal: DateGranule ---
  Year: { t0: "Temporal", t1: "DateGranule", visEncodings: ["temporal", "ordinal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "integer", zeroBaseline: "arbitrary", zeroPad: 0.03 },
  Quarter: { t0: "Temporal", t1: "DateGranule", visEncodings: ["ordinal"], aggRole: "dimension", domainShape: "cyclic", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Month: { t0: "Temporal", t1: "DateGranule", visEncodings: ["ordinal"], aggRole: "dimension", domainShape: "cyclic", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Week: { t0: "Temporal", t1: "DateGranule", visEncodings: ["ordinal"], aggRole: "dimension", domainShape: "cyclic", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Day: { t0: "Temporal", t1: "DateGranule", visEncodings: ["ordinal"], aggRole: "dimension", domainShape: "cyclic", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Hour: { t0: "Temporal", t1: "DateGranule", visEncodings: ["ordinal"], aggRole: "dimension", domainShape: "cyclic", diverging: "none", formatClass: "integer", zeroBaseline: "arbitrary", zeroPad: 0 },
  YearMonth: { t0: "Temporal", t1: "DateGranule", visEncodings: ["temporal", "ordinal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  YearQuarter: { t0: "Temporal", t1: "DateGranule", visEncodings: ["temporal", "ordinal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  YearWeek: { t0: "Temporal", t1: "DateGranule", visEncodings: ["temporal", "ordinal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Decade: { t0: "Temporal", t1: "DateGranule", visEncodings: ["temporal", "ordinal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "integer", zeroBaseline: "arbitrary", zeroPad: 0.03 },
  // --- Temporal: Duration ---
  Duration: { t0: "Temporal", t1: "Duration", visEncodings: ["quantitative"], aggRole: "additive", domainShape: "open", diverging: "none", formatClass: "unit-suffix", zeroBaseline: "meaningful", zeroPad: 0 },
  // --- Measure: Amount ---
  Amount: { t0: "Measure", t1: "Amount", visEncodings: ["quantitative"], aggRole: "additive", domainShape: "open", diverging: "none", formatClass: "currency", zeroBaseline: "meaningful", zeroPad: 0 },
  Price: { t0: "Measure", t1: "Amount", visEncodings: ["quantitative"], aggRole: "intensive", domainShape: "open", diverging: "none", formatClass: "currency", zeroBaseline: "meaningful", zeroPad: 0 },
  // --- Measure: Physical ---
  Quantity: { t0: "Measure", t1: "Physical", visEncodings: ["quantitative"], aggRole: "additive", domainShape: "open", diverging: "none", formatClass: "unit-suffix", zeroBaseline: "meaningful", zeroPad: 0 },
  Temperature: { t0: "Measure", t1: "Physical", visEncodings: ["quantitative"], aggRole: "intensive", domainShape: "open", diverging: "conditional", formatClass: "unit-suffix", zeroBaseline: "arbitrary", zeroPad: 0.05 },
  // --- Measure: Proportion ---
  Percentage: { t0: "Measure", t1: "Proportion", visEncodings: ["quantitative"], aggRole: "intensive", domainShape: "bounded", diverging: "none", formatClass: "percent", zeroBaseline: "contextual", zeroPad: 0 },
  // --- Measure: SignedMeasure ---
  Profit: { t0: "Measure", t1: "SignedMeasure", visEncodings: ["quantitative"], aggRole: "signed-additive", domainShape: "open", diverging: "conditional", formatClass: "decimal", zeroBaseline: "meaningful", zeroPad: 0 },
  PercentageChange: { t0: "Measure", t1: "SignedMeasure", visEncodings: ["quantitative"], aggRole: "intensive", domainShape: "open", diverging: "conditional", formatClass: "percent", zeroBaseline: "contextual", zeroPad: 0.05 },
  Sentiment: { t0: "Measure", t1: "SignedMeasure", visEncodings: ["quantitative"], aggRole: "intensive", domainShape: "open", diverging: "inherent", formatClass: "decimal", zeroBaseline: "meaningful", zeroPad: 0 },
  Correlation: { t0: "Measure", t1: "SignedMeasure", visEncodings: ["quantitative"], aggRole: "intensive", domainShape: "bounded", diverging: "inherent", formatClass: "decimal", zeroBaseline: "meaningful", zeroPad: 0 },
  // --- Measure: GenericMeasure ---
  Count: { t0: "Measure", t1: "GenericMeasure", visEncodings: ["quantitative"], aggRole: "additive", domainShape: "open", diverging: "none", formatClass: "integer", zeroBaseline: "meaningful", zeroPad: 0 },
  Number: { t0: "Measure", t1: "GenericMeasure", visEncodings: ["quantitative"], aggRole: "additive", domainShape: "open", diverging: "none", formatClass: "decimal", zeroBaseline: "meaningful", zeroPad: 0 },
  // --- Discrete ---
  Rank: { t0: "Discrete", t1: "Rank", visEncodings: ["ordinal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "integer", zeroBaseline: "arbitrary", zeroPad: 0.08 },
  Score: { t0: "Discrete", t1: "Score", visEncodings: ["quantitative", "ordinal"], aggRole: "intensive", domainShape: "bounded", diverging: "conditional", formatClass: "decimal", zeroBaseline: "contextual", zeroPad: 0.05 },
  ID: { t0: "Identifier", t1: "ID", visEncodings: ["nominal"], aggRole: "identifier", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "arbitrary", zeroPad: 0 },
  // --- Geographic ---
  Latitude: { t0: "Geographic", t1: "GeoCoordinate", visEncodings: ["quantitative", "geographic"], aggRole: "dimension", domainShape: "fixed", diverging: "none", formatClass: "decimal", zeroBaseline: "arbitrary", zeroPad: 0.02 },
  Longitude: { t0: "Geographic", t1: "GeoCoordinate", visEncodings: ["quantitative", "geographic"], aggRole: "dimension", domainShape: "fixed", diverging: "none", formatClass: "decimal", zeroBaseline: "arbitrary", zeroPad: 0.02 },
  Country: { t0: "Geographic", t1: "GeoPlace", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  State: { t0: "Geographic", t1: "GeoPlace", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  City: { t0: "Geographic", t1: "GeoPlace", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Region: { t0: "Geographic", t1: "GeoPlace", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Address: { t0: "Geographic", t1: "GeoPlace", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  ZipCode: { t0: "Geographic", t1: "GeoPlace", visEncodings: ["nominal"], aggRole: "identifier", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  // --- Categorical: Entity ---
  Category: { t0: "Categorical", t1: "Entity", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Name: { t0: "Categorical", t1: "Entity", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  // --- Categorical: Coded ---
  Status: { t0: "Categorical", t1: "Coded", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Boolean: { t0: "Categorical", t1: "Coded", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "fixed", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  Direction: { t0: "Categorical", t1: "Coded", visEncodings: ["ordinal", "nominal"], aggRole: "dimension", domainShape: "cyclic", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  // --- Categorical: Binned ---
  Range: { t0: "Categorical", t1: "Binned", visEncodings: ["ordinal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 },
  // --- Fallbacks ---
  Unknown: { t0: "Categorical", t1: "Entity", visEncodings: ["nominal"], aggRole: "dimension", domainShape: "open", diverging: "none", formatClass: "plain", zeroBaseline: "none", zeroPad: 0 }
};
var UNKNOWN_ENTRY = {
  t0: "Categorical",
  t1: "Entity",
  visEncodings: ["nominal"],
  aggRole: "dimension",
  domainShape: "open",
  diverging: "none",
  formatClass: "plain",
  zeroBaseline: "none",
  zeroPad: 0
};
function getRegistryEntry(semanticType) {
  return TYPE_REGISTRY[semanticType] ?? UNKNOWN_ENTRY;
}
function isRegistered(semanticType) {
  return semanticType in TYPE_REGISTRY;
}
function getRegisteredTypes() {
  return Object.keys(TYPE_REGISTRY);
}

// src/core/semantic-types.ts
var measureTypes = new Set(
  getRegisteredTypes().filter((t) => {
    const e = getRegistryEntry(t);
    return ["additive", "intensive", "signed-additive"].includes(e.aggRole) && e.t1 !== "Score";
  })
);
var nonMeasureNumericTypes = /* @__PURE__ */ new Set([
  "Rank",
  "ID",
  "Score",
  "Year",
  "Month",
  "Day",
  "Hour",
  "Latitude",
  "Longitude"
]);
var categoricalTypes = new Set(
  getRegisteredTypes().filter((t) => {
    const e = getRegistryEntry(t);
    return e.visEncodings.includes("nominal") && e.aggRole !== "identifier" || e.t1 === "Binned";
  })
);
var ordinalTypes = new Set(
  getRegisteredTypes().filter((t) => {
    const e = getRegistryEntry(t);
    return e.visEncodings.includes("ordinal");
  })
);
function getVisCategory(semanticType) {
  if (!semanticType || !isRegistered(semanticType)) return null;
  return getRegistryEntry(semanticType).visEncodings[0] ?? null;
}
function inferVisCategory(values) {
  if (values.length === 0) return "nominal";
  const isBoolean = (v) => v === true || v === false || Object.prototype.toString.call(v) === "[object Boolean]";
  const isNumber = (v) => !isNaN(+v) && !(Object.prototype.toString.call(v) === "[object Date]");
  const looksLikeDate = (s) => /^\d|^(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)/i.test(s.trim());
  const isDate = (v) => {
    if (v instanceof Date) return !isNaN(v.getTime());
    if (typeof v === "string") return looksLikeDate(v) && !isNaN(Date.parse(v));
    return !isNaN(Date.parse(v));
  };
  const nonNull = values.filter((v) => v != null);
  if (nonNull.length === 0) return "nominal";
  if (nonNull.every(isBoolean)) return "nominal";
  if (nonNull.every(isNumber)) return "quantitative";
  if (nonNull.every(isDate)) return "temporal";
  return "nominal";
}
function isMeasureType(semanticType) {
  return measureTypes.has(semanticType);
}
function isTimeSeriesType(semanticType) {
  const entry = getRegistryEntry(semanticType);
  return entry.t0 === "Temporal" && entry.t1 !== "Duration";
}
function isCategoricalType(semanticType) {
  return categoricalTypes.has(semanticType);
}
function isOrdinalType(semanticType) {
  return ordinalTypes.has(semanticType);
}
function isGeoCoordinateType(semanticType) {
  return getRegistryEntry(semanticType).t1 === "GeoCoordinate";
}
function isGeoLocationString(semanticType) {
  return getRegistryEntry(semanticType).t1 === "GeoPlace";
}
function isNonMeasureNumeric(semanticType) {
  return nonMeasureNumericTypes.has(semanticType);
}
function getZeroClass(semanticType) {
  const baseline = getRegistryEntry(semanticType).zeroBaseline;
  if (baseline === "none") return "unknown";
  return baseline;
}
var ZERO_BASELINE_GAP_THRESHOLD = 0.5;
function dataFarFromZero(values) {
  if (!values || values.length === 0) return false;
  const dataMin = Math.min(...values);
  const dataMax = Math.max(...values);
  if (dataMin <= 0 || dataMax <= 0) return false;
  return dataMin / dataMax >= ZERO_BASELINE_GAP_THRESHOLD;
}
function computeZeroDecision(semanticType, channel, markType, values) {
  const isBarLike = ["bar", "area", "rect"].includes(markType);
  const isScatterMark = markType === "circle" || markType === "point";
  const isPositional = ["x", "y"].includes(channel);
  const entry = getRegistryEntry(semanticType);
  const zeroClass = getZeroClass(semanticType);
  if (zeroClass === "meaningful") {
    if (isBarLike) {
      return { zero: true, domainPadFraction: 0, zeroClass, forced: true, uncertain: false };
    }
    if (isPositional && isScatterMark) {
      if (values && values.length > 0 && Math.min(...values) <= 0) {
        return { zero: true, domainPadFraction: 0, zeroClass, forced: true, uncertain: false };
      }
      return {
        zero: false,
        domainPadFraction: entry.zeroPad || 0.05,
        zeroClass,
        forced: false,
        uncertain: true
      };
    }
    return {
      zero: true,
      domainPadFraction: 0,
      zeroClass,
      forced: false,
      uncertain: dataFarFromZero(values)
    };
  }
  if (zeroClass === "arbitrary") {
    if (isBarLike && values && values.length > 0) {
      const dataMin = Math.min(...values);
      if (dataMin <= 0) {
        return { zero: true, domainPadFraction: 0, zeroClass, forced: true, uncertain: false };
      }
    }
    return {
      zero: false,
      domainPadFraction: entry.zeroPad || 0.05,
      zeroClass,
      forced: false,
      uncertain: false
    };
  }
  if (zeroClass === "contextual" && values && values.length > 0) {
    const dataMin = Math.min(...values);
    const dataMax = Math.max(...values);
    if (dataMin <= 0) {
      return { zero: true, domainPadFraction: 0, zeroClass, forced: true, uncertain: false };
    }
    const proximity = dataMax > 0 ? dataMin / dataMax : 0;
    if (proximity < 0.3) {
      return { zero: true, domainPadFraction: 0, zeroClass, forced: false, uncertain: false };
    }
    if (isBarLike) {
      return { zero: true, domainPadFraction: 0, zeroClass, forced: true, uncertain: false };
    }
    return { zero: false, domainPadFraction: 0.05, zeroClass, forced: false, uncertain: false };
  }
  if (isBarLike && isPositional) {
    return { zero: true, domainPadFraction: 0, zeroClass: "unknown", forced: true, uncertain: false };
  }
  return { zero: false, domainPadFraction: 0.05, zeroClass: "unknown", forced: true, uncertain: false };
}
var DIVERGING_WARM_HIGH = "blueorange";
var DIVERGING_WARM_LOW = "redblue";
function getRecommendedColorScheme(semanticType, encodingType, uniqueValueCount = 10, fieldName = "", values = [], colorHint) {
  const pickScheme = (schemes, name) => {
    let hash = 0;
    for (let i = 0; i < name.length; i++) {
      hash = (hash << 5) - hash + name.charCodeAt(i);
      hash = hash & hash;
    }
    return schemes[Math.abs(hash) % schemes.length];
  };
  if (!semanticType) {
    if (encodingType === "quantitative") {
      return { scheme: "viridis", type: "sequential", reason: "default for quantitative" };
    }
    if (encodingType === "ordinal") {
      return { scheme: "blues", type: "sequential", reason: "default for ordinal" };
    }
    return {
      scheme: uniqueValueCount > 10 ? "tableau20" : "tableau10",
      type: "categorical",
      reason: "default for categorical"
    };
  }
  if (semanticType === "Temperature") {
    if (colorHint?.type === "diverging") {
      return { scheme: DIVERGING_WARM_HIGH, type: "diverging", reason: "temperature diverging around freezing point, warm end high" };
    }
    return { scheme: "reds", type: "sequential", reason: "temperature single-direction uses sequential" };
  }
  if (semanticType === "Percentage") {
    if (colorHint?.type === "diverging") {
      return { scheme: DIVERGING_WARM_LOW, type: "diverging", reason: "percentage spans positive and negative, red is the losing side" };
    }
    return { scheme: "oranges", type: "sequential", reason: "percentage all same sign uses sequential" };
  }
  if (["Price", "Amount"].includes(semanticType)) {
    if (colorHint?.type === "diverging") {
      return { scheme: DIVERGING_WARM_LOW, type: "diverging", reason: "financial data spans positive and negative, red is the losing side" };
    }
    return { scheme: "goldgreen", type: "sequential", reason: "financial data uses gold-green" };
  }
  if (semanticType === "Score") {
    if (colorHint?.type === "diverging") {
      return { scheme: DIVERGING_WARM_LOW, type: "diverging", reason: "score diverging around midpoint, red is the poor side" };
    }
    return { scheme: "yelloworangebrown", type: "sequential", reason: "scores use warm sequential" };
  }
  if (semanticType === "Rank") {
    return { scheme: "purples", type: "sequential", reason: "ranks use single-hue sequential" };
  }
  if (semanticType === "Range") {
    return { scheme: "blues", type: "sequential", reason: "range groups use sequential" };
  }
  if (ordinalTypes.has(semanticType) && ["Year", "Quarter", "Month", "Week", "Day", "Hour", "Decade"].includes(semanticType)) {
    return { scheme: "viridis", type: "sequential", reason: "temporal granules use perceptually uniform" };
  }
  if (getRegistryEntry(semanticType ?? "").t1 === "GeoPlace") {
    if (uniqueValueCount <= 10) {
      return { scheme: "tableau10", type: "categorical", reason: "places are named categories, at a contrast that survives thin marks" };
    }
    return { scheme: "tableau20", type: "categorical", reason: "many regions use large categorical" };
  }
  if (["Status", "Boolean"].includes(semanticType)) {
    return { scheme: "set1", type: "categorical", reason: "status uses high-contrast categorical" };
  }
  if (semanticType === "Category") {
    return {
      scheme: uniqueValueCount > 10 ? "tableau20" : "tableau10",
      type: "categorical",
      reason: "categories use standard categorical"
    };
  }
  if (semanticType === "Name") {
    return {
      scheme: uniqueValueCount > 8 ? "tableau20" : "tableau10",
      type: "categorical",
      reason: "names use readable categorical"
    };
  }
  if (semanticType === "Duration") {
    return { scheme: "oranges", type: "sequential", reason: "duration uses intensity-based sequential" };
  }
  if (measureTypes.has(semanticType)) {
    if (colorHint?.type === "diverging") {
      const signed = getRegistryEntry(semanticType).t1 === "SignedMeasure";
      return signed ? { scheme: DIVERGING_WARM_LOW, type: "diverging", reason: "signed measure, red is the negative side" } : { scheme: DIVERGING_WARM_HIGH, type: "diverging", reason: "measure with no valence, warm end high" };
    }
    const sequentialSchemes = ["viridis", "blues", "greens", "reds", "yelloworangebrown", "goldgreen"];
    return {
      scheme: pickScheme(sequentialSchemes, fieldName),
      type: "sequential",
      reason: "measures use perceptually uniform sequential"
    };
  }
  if (ordinalTypes.has(semanticType) || encodingType === "ordinal") {
    const ordinalSchemes = ["blues", "greens", "purples", "oranges"];
    return {
      scheme: pickScheme(ordinalSchemes, fieldName),
      type: "sequential",
      reason: "ordinal data uses sequential scheme"
    };
  }
  if (encodingType === "nominal" || encodingType === "temporal") {
    return {
      scheme: uniqueValueCount > 10 ? "tableau20" : "tableau10",
      type: "categorical",
      reason: "default categorical palette"
    };
  }
  return { scheme: "viridis", type: "sequential", reason: "universal fallback" };
}
var MONTH_FULL = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
var MONTH_ABBR3 = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
var MONTH_NUM = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];
var DOW_FULL = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
var DOW_ABBR3 = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
var DOW_ABBR2 = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
var DOW_FULL_SUN = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
var DOW_ABBR3_SUN = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
var QUARTER_LABELS = ["Q1", "Q2", "Q3", "Q4"];
var COMPASS_8 = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
var COMPASS_8_FULL = ["North", "Northeast", "East", "Southeast", "South", "Southwest", "West", "Northwest"];
var COMPASS_4 = ["N", "E", "S", "W"];
var COMPASS_4_FULL = ["North", "East", "South", "West"];
var ORDINAL_SEQUENCES = {
  Month: [
    { labels: MONTH_FULL, caseInsensitive: true },
    { labels: MONTH_ABBR3, caseInsensitive: true },
    { labels: MONTH_NUM, caseInsensitive: false }
  ],
  Day: [
    { labels: DOW_FULL, caseInsensitive: true },
    { labels: DOW_ABBR3, caseInsensitive: true },
    { labels: DOW_ABBR2, caseInsensitive: true },
    { labels: DOW_FULL_SUN, caseInsensitive: true },
    { labels: DOW_ABBR3_SUN, caseInsensitive: true }
  ],
  Quarter: [
    { labels: QUARTER_LABELS, caseInsensitive: true }
  ],
  Direction: [
    { labels: COMPASS_8, caseInsensitive: true },
    { labels: COMPASS_8_FULL, caseInsensitive: true },
    { labels: COMPASS_4, caseInsensitive: true },
    { labels: COMPASS_4_FULL, caseInsensitive: true }
  ]
};
function buildLookup(seq) {
  const m = /* @__PURE__ */ new Map();
  for (let i = 0; i < seq.labels.length; i++) {
    const key = seq.caseInsensitive ? seq.labels[i].toLowerCase() : seq.labels[i];
    m.set(key, i);
  }
  return m;
}
function matchSequence(values, sequences) {
  const uniqueValues = [...new Set(values.map((v) => v != null ? String(v) : ""))].filter((v) => v !== "");
  if (uniqueValues.length === 0) return void 0;
  for (const seq of sequences) {
    const lookup = buildLookup(seq);
    const matched = [];
    const unmatched = [];
    for (const val of uniqueValues) {
      const key = seq.caseInsensitive ? val.toLowerCase() : val;
      const idx = lookup.get(key);
      if (idx !== void 0) {
        matched.push({ value: val, index: idx });
      } else {
        unmatched.push(val);
      }
    }
    if (matched.length >= uniqueValues.length * 0.6 && matched.length >= 2) {
      matched.sort((a, b) => a.index - b.index);
      const result = matched.map((m) => m.value);
      result.push(...unmatched);
      return result;
    }
  }
  return void 0;
}
function inferOrdinalSortOrder(semanticType, values) {
  const sequences = ORDINAL_SEQUENCES[semanticType];
  if (sequences) {
    return matchSequence(values, sequences);
  }
  if (!semanticType || semanticType === "Category" || semanticType === "Unknown") {
    for (const seqs of Object.values(ORDINAL_SEQUENCES)) {
      const result = matchSequence(values, seqs);
      if (result) return result;
    }
  }
  return void 0;
}

// src/core/field-semantics.ts
function toTypeString(input) {
  if (!input) return "";
  if (typeof input === "string") return input;
  return input.semanticType || "";
}
function normalizeAnnotation(input) {
  if (!input) return { semanticType: "Unknown" };
  if (typeof input === "string") return { semanticType: input || "Unknown" };
  return { ...input, semanticType: input.semanticType || "Unknown" };
}
var CURRENCY_MAP = {
  USD: "$",
  EUR: "\u20AC",
  GBP: "\xA3",
  JPY: "\xA5",
  CNY: "\xA5",
  KRW: "\u20A9",
  INR: "\u20B9",
  BRL: "R$",
  CAD: "CA$",
  AUD: "A$",
  CHF: "CHF",
  SEK: "kr",
  NOK: "kr",
  DKK: "kr"
};
var UNIT_SUFFIX_MAP = {
  // Temperature
  "\xB0C": "\xB0C",
  "\xB0F": "\xB0F",
  C: "\xB0C",
  F: "\xB0F",
  // Mass
  kg: " kg",
  lb: " lb",
  // Distance
  km: " km",
  mi: " mi",
  m: " m",
  ft: " ft",
  // Speed
  "km/h": " km/h",
  mph: " mph",
  // Time
  sec: " s",
  min: " min",
  hr: " hr",
  seconds: " s",
  minutes: " min",
  hours: " hr",
  // Percentage (handled by formatClass, but allow explicit suffix)
  "%": "%"
};
function detectPercentageRepresentation(values) {
  if (values.length === 0) return "0-100";
  const abs = values.map(Math.abs);
  const countBelow1 = abs.filter((v) => v <= 1).length;
  if (countBelow1 / abs.length >= 0.8) return "0-1";
  return "0-100";
}
function detectPrecision(values) {
  let maxDecimals = 0;
  for (const v of values) {
    if (!Number.isFinite(v)) continue;
    const s = v.toFixed(10);
    const dot = s.indexOf(".");
    if (dot === -1) continue;
    let end = s.length - 1;
    while (end > dot && s[end] === "0") end--;
    const decimals = end > dot ? end - dot : 0;
    if (decimals > maxDecimals) maxDecimals = decimals;
  }
  return Math.min(maxDecimals, 4);
}
function precisionFormat(values, useGrouping = true, signMode = "") {
  const p = detectPrecision(values);
  const group = useGrouping ? "," : "";
  if (p === 0) return `${signMode}${group}d`;
  return `${signMode}${group}.${p}f`;
}
function resolveFormat(semanticType, annotation, values) {
  const entry = getRegistryEntry(semanticType);
  const unit = annotation.unit;
  const currencyPrefix = unit ? CURRENCY_MAP[unit.toUpperCase()] ?? CURRENCY_MAP[unit] : void 0;
  const unitSuffix = unit ? UNIT_SUFFIX_MAP[unit] : void 0;
  const nums = values.filter((v) => typeof v === "number" && !isNaN(v));
  switch (entry.formatClass) {
    case "currency": {
      const pfx = currencyPrefix;
      if (pfx) {
        const axisPattern = semanticType === "Price" ? ",.2f" : precisionFormat(nums);
        return {
          format: { pattern: axisPattern, prefix: pfx },
          tooltipFormat: { pattern: ",.2f", prefix: pfx }
        };
      }
      return { tooltipFormat: { pattern: ",.2f" } };
    }
    case "percent": {
      if (!annotation.intrinsicDomain) {
        return { tooltipFormat: { pattern: precisionFormat(nums) } };
      }
      const rep = detectPercentageRepresentation(nums);
      if (rep === "0-1") {
        const p = detectPrecision(nums);
        const axisP = Math.max(0, p - 2);
        const tipP = Math.min(axisP + 1, 4);
        return {
          format: { pattern: `.${axisP}~%` },
          tooltipFormat: { pattern: `.${tipP}%` }
        };
      }
      return {
        tooltipFormat: { pattern: precisionFormat(nums, false), suffix: "%" }
      };
    }
    case "unit-suffix":
      return {
        tooltipFormat: unitSuffix ? { pattern: precisionFormat(nums), suffix: unitSuffix } : { pattern: precisionFormat(nums) }
      };
    case "integer":
      if (semanticType === "Year" || semanticType === "Decade") {
        return {};
      }
      return { tooltipFormat: { pattern: ",d" } };
    case "decimal":
      return { tooltipFormat: { pattern: precisionFormat(nums) } };
    case "plain":
    default:
      return {};
  }
}
function resolveDefaultVisType(semanticType, values) {
  if (!isRegistered(semanticType)) {
    return inferVisCategory(values);
  }
  const entry = getRegistryEntry(semanticType);
  const candidates = entry.visEncodings;
  if (candidates.length === 1) {
    if (candidates[0] === "quantitative") {
      const nonNull = values.filter((v) => v != null);
      const allNumeric = nonNull.length > 0 && nonNull.every((v) => typeof v === "number" || typeof v === "string" && !isNaN(+v) && v.trim() !== "");
      if (!allNumeric) {
        return inferVisCategory(values);
      }
    }
    return candidates[0];
  }
  if (candidates.includes("quantitative") && candidates.includes("ordinal")) {
    const distinct = new Set(values.filter((v) => v != null)).size;
    return distinct <= 12 ? "ordinal" : "quantitative";
  }
  if (candidates.includes("temporal") && candidates.includes("ordinal")) {
    const distinct = new Set(values.filter((v) => v != null)).size;
    return distinct <= 6 ? "ordinal" : "temporal";
  }
  if (candidates.includes("geographic") && candidates.includes("quantitative")) {
    return "quantitative";
  }
  return candidates[0];
}
function resolveAggregationDefault(semanticType) {
  const entry = getRegistryEntry(semanticType);
  switch (entry.aggRole) {
    case "additive":
      return "sum";
    case "signed-additive":
      return "sum";
    case "intensive":
      return "average";
    case "dimension":
      return void 0;
    case "identifier":
      return void 0;
    default:
      return void 0;
  }
}
function resolveZeroClassFromAnnotation(semanticType, domain) {
  if (domain && domain[0] > 0) return "arbitrary";
  return getZeroClass(semanticType);
}
function resolveScaleType(semanticType, values) {
  const entry = getRegistryEntry(semanticType);
  const eligible = entry.aggRole === "additive" && entry.domainShape === "open" && entry.t1 !== "GenericMeasure";
  if (!eligible) return void 0;
  if (values.length < 10) return void 0;
  const filtered = values.filter((v) => typeof v === "number" && !isNaN(v) && isFinite(v));
  if (filtered.length < 10) return void 0;
  const min = Math.min(...filtered);
  const max = Math.max(...filtered);
  if (max <= 0 || min === max) return void 0;
  if (min < 0) return void 0;
  const positiveMin = Math.min(...filtered.filter((v) => v > 0));
  if (positiveMin > 0 && max / positiveMin >= 1e6) {
    const hasZeros = filtered.some((v) => v === 0);
    return hasZeros ? "symlog" : "log";
  }
  return void 0;
}
function mergeIntrinsicWithData(intrinsic, values, hard) {
  if (hard) {
    return { min: intrinsic[0], max: intrinsic[1], clamp: true };
  }
  const nums = values.filter((v) => typeof v === "number" && !isNaN(v));
  if (nums.length === 0) {
    return { min: intrinsic[0], max: intrinsic[1], clamp: false };
  }
  const dataMin = Math.min(...nums);
  const dataMax = Math.max(...nums);
  return {
    min: Math.min(intrinsic[0], dataMin),
    max: Math.max(intrinsic[1], dataMax),
    clamp: false
  };
}
function snapToBoundHeuristic(intrinsic, values) {
  const nums = values.filter((v) => typeof v === "number" && !isNaN(v));
  if (nums.length === 0) return void 0;
  const [lo, hi] = intrinsic;
  const range = hi - lo;
  if (range <= 0) return void 0;
  const dataMin = Math.min(...nums);
  const dataMax = Math.max(...nums);
  const zeroInside = lo < 0 && hi > 0;
  const thresholdLo = 0.25 * (zeroInside ? 0 - lo : range);
  const thresholdHi = 0.25 * (zeroInside ? hi : range);
  let snapMin;
  let snapMax;
  if (dataMin >= lo && dataMin <= lo + thresholdLo) {
    snapMin = lo;
  }
  if (dataMax <= hi && dataMax >= hi - thresholdHi) {
    snapMax = hi;
  }
  if (snapMin === void 0 && snapMax === void 0) return void 0;
  return { min: snapMin, max: snapMax, clamp: false };
}
function resolveDomainConstraint(semanticType, annotation, values) {
  const entry = getRegistryEntry(semanticType);
  if (annotation.intrinsicDomain) {
    if (entry.t1 === "Proportion" || entry.t1 === "SignedMeasure") {
      return snapToBoundHeuristic(annotation.intrinsicDomain, values);
    }
    return mergeIntrinsicWithData(annotation.intrinsicDomain, values, false);
  }
  if (semanticType === "Latitude") return mergeIntrinsicWithData([-90, 90], values, true);
  if (semanticType === "Longitude") return mergeIntrinsicWithData([-180, 180], values, true);
  if (semanticType === "Correlation") return mergeIntrinsicWithData([-1, 1], values, true);
  if (semanticType === "Percentage") {
    const nums = values.filter((v) => typeof v === "number" && !isNaN(v));
    if (nums.length > 0) {
      const rep = detectPercentageRepresentation(nums);
      const M = rep === "0-1" ? 1 : 100;
      return snapToBoundHeuristic([0, M], values);
    }
  }
  return void 0;
}
function resolveTickConstraint(semanticType, domain) {
  const entry = getRegistryEntry(semanticType);
  if (entry.formatClass === "integer") {
    const tc = { integersOnly: true, minStep: 1 };
    if (domain) {
      const span = domain[1] - domain[0];
      if (span <= 20 && span > 0) {
        tc.exactTicks = [];
        for (let i = domain[0]; i <= domain[1]; i++) {
          tc.exactTicks.push(i);
        }
      }
    }
    return tc;
  }
  if (semanticType === "Score" && domain) {
    const span = domain[1] - domain[0];
    if (span >= 2) {
      const tc = { integersOnly: true, minStep: 1 };
      if (span <= 20) {
        tc.exactTicks = [];
        for (let i = domain[0]; i <= domain[1]; i++) {
          tc.exactTicks.push(i);
        }
      }
      return tc;
    }
  }
  return void 0;
}
function resolveCanonicalOrder(semanticType, annotation, values) {
  if (annotation.sortOrder && annotation.sortOrder.length > 0) {
    return annotation.sortOrder;
  }
  return inferOrdinalSortOrder(semanticType, values);
}
function resolveCyclic(semanticType) {
  const entry = getRegistryEntry(semanticType);
  return entry.domainShape === "cyclic";
}
function resolveReversed(semanticType, channel) {
  if (semanticType === "Rank") {
    return channel !== "x";
  }
  return false;
}
function resolveNice(semanticType, domainConstraint) {
  if (domainConstraint?.clamp) return false;
  if (domainConstraint && domainConstraint.min !== void 0 && domainConstraint.max !== void 0) {
    return false;
  }
  const entry = getRegistryEntry(semanticType);
  if (entry.domainShape === "fixed") return false;
  return true;
}
function resolveDivergingInfo(semanticType, annotation, values) {
  const entry = getRegistryEntry(semanticType);
  if (annotation.divergingMidpoint !== void 0) {
    return { midpoint: annotation.divergingMidpoint, inherent: true, source: "annotation" };
  }
  if (semanticType === "Temperature" && annotation.unit) {
    const unitMidpoints = {
      "\xB0C": 0,
      "\xB0F": 32,
      "K": 273.15,
      C: 0,
      F: 32
    };
    const mid = unitMidpoints[annotation.unit];
    if (mid !== void 0) {
      return { midpoint: mid, inherent: false, source: "unit" };
    }
  }
  if (entry.diverging === "inherent") {
    return { midpoint: 0, inherent: true, source: "type-intrinsic" };
  }
  if (entry.diverging === "conditional") {
    return { midpoint: 0, inherent: false, source: "type-intrinsic" };
  }
  if (annotation.intrinsicDomain) {
    return {
      midpoint: (annotation.intrinsicDomain[0] + annotation.intrinsicDomain[1]) / 2,
      inherent: false,
      source: "domain"
    };
  }
  if (values.length > 0) {
    const min = Math.min(...values);
    const max = Math.max(...values);
    if (min < 0 && max > 0) {
      return { midpoint: 0, inherent: false, source: "data" };
    }
  }
  return void 0;
}
function resolveColorSchemeHint(semanticType, annotation, values) {
  const entry = getRegistryEntry(semanticType);
  const nums = values.filter((v) => typeof v === "number" && !isNaN(v));
  const divInfo = resolveDivergingInfo(semanticType, annotation, nums);
  if (divInfo) {
    const min = nums.length > 0 ? Math.min(...nums) : 0;
    const max = nums.length > 0 ? Math.max(...nums) : 0;
    const spansBothSides = min < divInfo.midpoint && max > divInfo.midpoint;
    if (divInfo.inherent || spansBothSides) {
      return {
        type: "diverging",
        divergingMidpoint: divInfo.midpoint,
        inherentlyDiverging: divInfo.inherent
      };
    }
  }
  if (entry.visEncodings.includes("quantitative")) {
    return { type: "sequential" };
  }
  return { type: "categorical" };
}
function resolveBinningSuggested(semanticType, domain) {
  const entry = getRegistryEntry(semanticType);
  if (!entry.visEncodings.includes("quantitative")) return false;
  if (entry.aggRole === "identifier" || entry.aggRole === "dimension") return false;
  if (semanticType === "Year" || semanticType === "Decade") return false;
  if (domain && domain[1] - domain[0] <= 20) return false;
  if (semanticType === "Score" && !domain) return false;
  return true;
}
function resolveStackable(semanticType) {
  const entry = getRegistryEntry(semanticType);
  switch (entry.aggRole) {
    case "additive":
      return "sum";
    case "signed-additive":
      return "sum";
    case "intensive":
      if (semanticType === "Percentage") return "normalize";
      return false;
    case "dimension":
      return false;
    case "identifier":
      return false;
    default:
      return false;
  }
}
function resolveSortDirection(semanticType) {
  if (semanticType === "Rank") return "descending";
  return "ascending";
}
function resolveFieldSemantics(input, fieldName, values) {
  const annotation = normalizeAnnotation(input);
  const semanticType = annotation.semanticType;
  const numericValues = values.filter((v) => typeof v === "number" && !isNaN(v) && isFinite(v));
  const defaultVisType = resolveDefaultVisType(semanticType, values);
  const { format, tooltipFormat } = resolveFormat(semanticType, annotation, values);
  let aggregationDefault = resolveAggregationDefault(semanticType);
  let zeroClass = resolveZeroClassFromAnnotation(semanticType, annotation.intrinsicDomain);
  const scaleType = resolveScaleType(semanticType, numericValues);
  const domainConstraint = resolveDomainConstraint(semanticType, annotation, values);
  const canonicalOrder = resolveCanonicalOrder(semanticType, annotation, values);
  const cyclic = resolveCyclic(semanticType);
  let binningSuggested = resolveBinningSuggested(semanticType, annotation.intrinsicDomain);
  const sortDirection = resolveSortDirection(semanticType);
  if (!isRegistered(semanticType) && defaultVisType === "quantitative") {
    if (!aggregationDefault) aggregationDefault = "sum";
    if (zeroClass === "unknown") zeroClass = "meaningful";
    binningSuggested = true;
  }
  return {
    semanticAnnotation: annotation,
    defaultVisType,
    format,
    tooltipFormat,
    aggregationDefault,
    zeroClass,
    scaleType: scaleType ?? void 0,
    domainConstraint,
    canonicalOrder,
    cyclic,
    sortDirection,
    binningSuggested
  };
}

// src/chartjs/templates/bump.ts
var RANK_SEMANTIC_TYPES = /* @__PURE__ */ new Set(["Rank", "Score", "Level"]);
var isDiscrete7 = (type) => type === "nominal" || type === "ordinal";
var cjsBumpChartDef = {
  chart: "Bump Chart",
  template: { mark: "line", encoding: {} },
  channels: ["x", "y", "color", "detail", "column", "row"],
  markCognitiveChannel: "position",
  declareLayoutMode: () => ({
    paramOverrides: { continuousMarkCrossSection: { x: 80, y: 20, seriesCountAxis: "auto" } }
  }),
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, fullTable, semanticTypes } = ctx;
    const xCS = channelSemantics.x;
    const yCS = channelSemantics.y;
    const colorField = channelSemantics.color?.field;
    if (!xCS?.field || !yCS?.field) return;
    const xField = xCS.field;
    const yField = yCS.field;
    const ySemType = toTypeString(semanticTypes?.[yField]);
    const xSemType = toTypeString(semanticTypes?.[xField]);
    const yIsRank = RANK_SEMANTIC_TYPES.has(ySemType);
    const xIsRank = RANK_SEMANTIC_TYPES.has(xSemType);
    const rankOnY = yIsRank && !xIsRank;
    const xIsDiscrete = isDiscrete7(xCS.type);
    const xIsTemporal = xCS.type === "temporal";
    const mapContinuousX = (raw) => xIsTemporal ? coerceUnixMsForChartJs(raw) : raw;
    let continuousXExtent;
    if (!xIsDiscrete) {
      const xNums = (fullTable ?? table).map((r) => mapContinuousX(r[xField])).filter((v) => typeof v === "number" && Number.isFinite(v));
      if (xNums.length > 0) {
        continuousXExtent = { min: Math.min(...xNums), max: Math.max(...xNums) };
      }
    }
    const categories = xIsDiscrete ? extractCategories(table, xField, xCS.ordinalSortOrder) : void 0;
    const sortRowsByX = (rows) => [...rows].sort((a, b) => {
      const ax = mapContinuousX(a[xField]);
      const bx = mapContinuousX(b[xField]);
      if (typeof ax === "number" && typeof bx === "number") return ax - bx;
      return String(ax).localeCompare(String(bx));
    });
    const rankValues = table.map((r) => Number(r[yField])).filter((v) => !isNaN(v) && isFinite(v));
    const maxRank = rankValues.length ? Math.max(...rankValues) : 1;
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "line",
      data: { labels: categories || [], datasets: [] },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            type: xIsDiscrete ? "category" : "linear",
            title: { display: true, text: xField },
            ...continuousXExtent ? { min: continuousXExtent.min, max: continuousXExtent.max } : {},
            ticks: {
              font: { size: 10 },
              ...xIsTemporal ? {
                maxTicksLimit: 4,
                autoSkip: true,
                maxRotation: 0,
                callback(v) {
                  const n = typeof v === "number" ? v : Number(v);
                  if (!Number.isFinite(n)) return String(v);
                  const spanDays = continuousXExtent ? (continuousXExtent.max - continuousXExtent.min) / 864e5 : 0;
                  const opts = spanDays > 60 ? { month: "short", year: "numeric" } : { month: "short", day: "numeric", year: "numeric" };
                  return new Date(n).toLocaleDateString(void 0, opts);
                }
              } : {}
            }
          },
          y: rankOnY ? {
            type: "linear",
            reverse: true,
            min: 1,
            max: maxRank,
            title: { display: true, text: yField },
            ticks: { font: { size: 10 }, stepSize: 1, precision: 0 }
          } : {
            type: "linear",
            title: { display: true, text: yField },
            ticks: { font: { size: 10 } }
          }
        },
        plugins: {
          tooltip: { enabled: true }
        }
      }
    };
    if (!rankOnY && channelSemantics.y?.zero) {
      config.options.scales.y.beginAtZero = channelSemantics.y.zero.zero !== false;
    }
    const baseDataset = {
      tension: 0.4,
      pointRadius: 3,
      fill: false,
      backgroundColor: "transparent"
    };
    if (colorField) {
      const groups = groupBy(table, colorField);
      config.options.plugins.legend = { display: true };
      let colorIdx = 0;
      for (const [name, rows] of groups) {
        const data = xIsDiscrete ? buildCategoryAlignedData(rows, xField, yField, categories) : sortRowsByX(rows).map((r) => ({ x: mapContinuousX(r[xField]), y: r[yField] })).filter((p) => p.y != null && (xIsTemporal ? Number.isFinite(p.x) : true));
        config.data.datasets.push({
          label: name,
          data,
          borderColor: getSeriesBorderColor(palette, colorIdx),
          ...baseDataset
        });
        colorIdx++;
      }
    } else {
      const data = xIsDiscrete ? categories.map((cat) => {
        const row = table.find((r) => String(r[xField]) === cat);
        return row ? row[yField] : null;
      }) : sortRowsByX(table).map((r) => ({ x: mapContinuousX(r[xField]), y: r[yField] })).filter((p) => p.y != null && (xIsTemporal ? Number.isFinite(p.x) : true));
      config.data.datasets.push({
        label: yField,
        data,
        borderColor: getSeriesBorderColor(palette, 0),
        ...baseDataset
      });
      config.options.plugins.legend = { display: false };
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  }
};

// src/chartjs/templates/slope.ts
function orderPeriods(categories) {
  if (categories.length <= 1) return categories;
  const allNumeric = categories.every((c) => c.trim() !== "" && !isNaN(Number(c)));
  if (allNumeric) return [...categories].sort((a, b) => Number(a) - Number(b));
  const allDates = categories.every((c) => !isNaN(Date.parse(c)));
  if (allDates) return [...categories].sort((a, b) => Date.parse(a) - Date.parse(b));
  return categories;
}
var cjsSlopeChartDef = {
  chart: "Slope Chart",
  template: { mark: "line", encoding: {} },
  channels: ["x", "y", "color", "detail", "column", "row"],
  markCognitiveChannel: "position",
  declareLayoutMode: () => ({
    axisFlags: { x: { banded: true } },
    paramOverrides: {
      defaultBandSize: 120,
      continuousMarkCrossSection: { x: 0, y: 0, seriesCountAxis: "auto" }
    }
  }),
  instantiate: (spec, ctx) => {
    const { channelSemantics, table } = ctx;
    const xCS = channelSemantics.x;
    const yCS = channelSemantics.y;
    const groupField = channelSemantics.color?.field ?? channelSemantics.detail?.field;
    if (!xCS?.field || !yCS?.field) return;
    const xField = xCS.field;
    const yField = yCS.field;
    const categories = orderPeriods(
      extractCategories(table, xField, xCS.ordinalSortOrder)
    );
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "line",
      data: { labels: categories, datasets: [] },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            type: "category",
            // Inset the two period bands so the end points/labels
            // are not clipped against the plot edges.
            offset: true,
            title: { display: true, text: xField },
            ticks: { font: { size: 10 } }
          },
          y: {
            type: "linear",
            title: { display: true, text: yField },
            ticks: { font: { size: 10 } }
          }
        },
        plugins: {
          tooltip: { enabled: true },
          legend: { display: false }
        }
      }
    };
    if (channelSemantics.y?.zero) {
      config.options.scales.y.beginAtZero = channelSemantics.y.zero.zero !== false;
    }
    const baseDataset = {
      // Straight segments + visible end points.
      tension: 0,
      pointRadius: 4,
      backgroundColor: "transparent",
      fill: false
    };
    if (groupField) {
      const groups = groupBy(table, groupField);
      config.options.plugins.legend = { display: true };
      let colorIdx = 0;
      for (const [name, rows] of groups) {
        config.data.datasets.push({
          label: name,
          data: buildCategoryAlignedData(rows, xField, yField, categories),
          borderColor: getSeriesBorderColor(palette, colorIdx),
          ...baseDataset
        });
        colorIdx++;
      }
    } else {
      config.data.datasets.push({
        label: yField,
        data: buildCategoryAlignedData(table, xField, yField, categories),
        borderColor: getSeriesBorderColor(palette, 0),
        ...baseDataset
      });
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  }
};

// src/chartjs/templates/area.ts
var isDiscrete8 = (type) => type === "nominal" || type === "ordinal";
var cjsAreaChartDef = {
  chart: "Area Chart",
  template: { mark: "area", encoding: {} },
  channels: ["x", "y", "color", "opacity", "column", "row"],
  markCognitiveChannel: "area",
  declareLayoutMode: () => ({
    paramOverrides: { continuousMarkCrossSection: { x: 100, y: 20, seriesCountAxis: "auto" }, facetAspectRatioResistance: 0.5 }
  }),
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const xCS = channelSemantics.x;
    const yCS = channelSemantics.y;
    const colorField = channelSemantics.color?.field;
    if (!xCS?.field || !yCS?.field) return;
    const xField = xCS.field;
    const yField = yCS.field;
    const xIsDiscrete = isDiscrete8(xCS.type);
    const xIsTemporal = xCS.type === "temporal";
    const mapContinuousX = (raw) => xIsTemporal ? coerceUnixMsForChartJs(raw) : raw;
    const categories = xIsDiscrete ? extractCategories(table, xField, xCS.ordinalSortOrder) : void 0;
    const opacity = chartProperties?.opacity ?? 0.4;
    const stackMode = chartProperties?.stackMode;
    const stacked = stackMode !== "layered";
    const interpolate = chartProperties?.interpolate;
    const tension = interpolate === "monotone" || interpolate === "basis" || interpolate === "cardinal" || interpolate === "catmull-rom" ? 0.4 : 0;
    const stepped = interpolate === "step" ? "middle" : interpolate === "step-before" ? "before" : interpolate === "step-after" ? "after" : false;
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "line",
      data: {
        labels: categories || [],
        datasets: []
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            type: xIsDiscrete ? "category" : "linear",
            title: { display: true, text: xField },
            ticks: {
              font: { size: 10 },
              ...xIsTemporal ? {
                maxTicksLimit: 8,
                callback(v) {
                  const n = typeof v === "number" ? v : Number(v);
                  if (!Number.isFinite(n)) return String(v);
                  return new Date(n).toLocaleDateString(void 0, {
                    month: "short",
                    day: "numeric",
                    year: "numeric"
                  });
                }
              } : {}
            }
          },
          y: {
            type: "linear",
            title: { display: true, text: yField },
            stacked,
            ticks: { font: { size: 10 } }
          }
        },
        plugins: {
          tooltip: { enabled: true },
          filler: { propagate: true }
        }
      }
    };
    if (channelSemantics.y?.zero) {
      config.options.scales.y.beginAtZero = channelSemantics.y.zero.zero !== false;
    }
    if (colorField) {
      const groups = groupBy(table, colorField);
      config.options.plugins.legend = { display: true };
      let colorIdx = 0;
      for (const [name, rows] of groups) {
        const data = xIsDiscrete ? buildCategoryAlignedData(rows, xField, yField, categories) : rows.map((r) => ({ x: mapContinuousX(r[xField]), y: r[yField] })).filter((p) => p.y != null && (xIsTemporal ? Number.isFinite(p.x) : true));
        const borderColor = getSeriesBorderColor(palette, colorIdx);
        const bgColor = getSeriesBackgroundColor(palette, colorIdx, opacity);
        config.data.datasets.push({
          label: name,
          data,
          borderColor,
          backgroundColor: bgColor,
          tension,
          stepped,
          fill: stacked ? "stack" : "origin",
          pointRadius: 2
        });
        colorIdx++;
      }
    } else {
      const data = xIsDiscrete ? categories.map((cat) => {
        const row = table.find((r) => String(r[xField]) === cat);
        return row ? row[yField] : null;
      }) : table.map((r) => ({ x: mapContinuousX(r[xField]), y: r[yField] })).filter((p) => p.y != null && (xIsTemporal ? Number.isFinite(p.x) : true));
      config.data.datasets.push({
        label: yField,
        data,
        borderColor: getSeriesBorderColor(palette, 0),
        backgroundColor: getSeriesBackgroundColor(palette, 0, opacity),
        tension,
        stepped,
        fill: "origin",
        pointRadius: 2
      });
      config.options.plugins.legend = { display: false };
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    {
      key: "interpolate",
      label: "Curve",
      type: "discrete",
      options: [
        { value: void 0, label: "Default (linear)" },
        { value: "linear", label: "Linear" },
        { value: "monotone", label: "Monotone (smooth)" },
        { value: "step", label: "Step" },
        { value: "step-before", label: "Step Before" },
        { value: "step-after", label: "Step After" },
        { value: "basis", label: "Basis (smooth)" },
        { value: "cardinal", label: "Cardinal" },
        { value: "catmull-rom", label: "Catmull-Rom" }
      ]
    },
    {
      key: "opacity",
      label: "Opacity",
      type: "continuous",
      min: 0.1,
      max: 1,
      step: 0.05,
      defaultValue: 0.4,
      check: (ctx) => ({
        applicable: !!ctx.encodings.color?.field && ctx.chartProperties?.stackMode === "layered"
      })
    },
    {
      key: "stackMode",
      label: "Stack",
      type: "discrete",
      options: [
        { value: void 0, label: "Stacked (default)" },
        { value: "layered", label: "Layered (overlap)" }
      ]
    }
  ]
};

// src/chartjs/templates/range-area.ts
var isDiscrete9 = (type) => type === "nominal" || type === "ordinal";
var cjsRangeAreaChartDef = {
  chart: "Range Area Chart",
  template: { mark: "area", encoding: {} },
  channels: ["x", "y", "y2", "color", "column", "row"],
  markCognitiveChannel: "area",
  declareLayoutMode: () => ({
    paramOverrides: {
      continuousMarkCrossSection: { x: 100, y: 20, seriesCountAxis: "auto" },
      facetAspectRatioResistance: 0.5
    }
  }),
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const xCS = channelSemantics.x;
    const yCS = channelSemantics.y;
    const y2CS = channelSemantics.y2;
    const colorField = channelSemantics.color?.field;
    if (!xCS?.field || !yCS?.field || !y2CS?.field) return;
    const xField = xCS.field;
    const lowField = yCS.field;
    const highField = y2CS.field;
    const xIsDiscrete = isDiscrete9(xCS.type);
    const xIsTemporal = xCS.type === "temporal";
    const mapContinuousX = (raw) => xIsTemporal ? coerceUnixMsForChartJs(raw) : Number(raw);
    const valueTitle = lowField === highField ? lowField : `${lowField}, ${highField}`;
    const categories = xIsDiscrete ? extractCategories(table, xField, xCS.ordinalSortOrder) : void 0;
    const opacity = chartProperties?.opacity ?? 0.3;
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "line",
      data: { labels: categories ?? [], datasets: [] },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            type: xIsDiscrete ? "category" : "linear",
            title: { display: true, text: xField },
            ticks: {
              font: { size: 10 },
              ...xIsTemporal ? {
                maxTicksLimit: 8,
                callback(v) {
                  const n = typeof v === "number" ? v : Number(v);
                  if (!Number.isFinite(n)) return String(v);
                  return new Date(n).toLocaleDateString(void 0, {
                    month: "short",
                    day: "numeric",
                    year: "numeric"
                  });
                }
              } : {}
            }
          },
          y: {
            type: "linear",
            // A ranged area reads its extent, not its distance from
            // zero — fit the band rather than forcing a zero baseline.
            title: { display: true, text: valueTitle },
            ticks: { font: { size: 10 } }
          }
        },
        plugins: {
          tooltip: { enabled: true },
          legend: { display: false }
        }
      }
    };
    const buildBoundData = (rows, field) => {
      if (xIsDiscrete) return buildCategoryAlignedData(rows, xField, field, categories);
      return rows.map((r) => ({ x: mapContinuousX(r[xField]), y: Number(r[field]) })).filter((p) => Number.isFinite(p.x) && Number.isFinite(p.y)).sort((a, b) => a.x - b.x);
    };
    const pushBand = (rows, name, colorIdx) => {
      const lowerData = buildBoundData(rows, lowField);
      const upperData = buildBoundData(rows, highField);
      const border = getSeriesBorderColor(palette, colorIdx);
      const lowerIndex = config.data.datasets.length;
      config.data.datasets.push({
        label: name,
        data: lowerData,
        borderColor: border,
        backgroundColor: "transparent",
        borderWidth: 1,
        pointRadius: 0,
        fill: false,
        tension: 0,
        _rangeBound: "lower"
      });
      config.data.datasets.push({
        label: name,
        data: upperData,
        borderColor: border,
        backgroundColor: getSeriesBackgroundColor(palette, colorIdx, opacity),
        borderWidth: 1,
        pointRadius: 0,
        fill: { target: lowerIndex },
        tension: 0,
        _rangeBound: "upper"
      });
    };
    if (colorField) {
      const groups = groupBy(table, colorField);
      let idx = 0;
      for (const [name, rows] of groups) {
        pushBand(rows, String(name), idx);
        idx++;
      }
      config.options.plugins.legend = {
        display: true,
        labels: {
          filter: (item, data) => data.datasets[item.datasetIndex]?._rangeBound !== "lower"
        }
      };
    } else {
      pushBand(table, lowField, 0);
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  }
};

// src/chartjs/templates/pie.ts
function sortSlicesInPlace(labels, values, sortSlices) {
  if (sortSlices !== "descending" && sortSlices !== "ascending") return;
  const idx = values.map((_, i) => i);
  idx.sort((a, b) => sortSlices === "descending" ? values[b] - values[a] : values[a] - values[b]);
  const sortedLabels = idx.map((i) => labels[i]);
  const sortedValues = idx.map((i) => values[i]);
  labels.splice(0, labels.length, ...sortedLabels);
  values.splice(0, values.length, ...sortedValues);
}
var SORT_SLICES_PROPERTY = {
  key: "sortSlices",
  label: "Sort slices",
  type: "discrete",
  options: [
    { value: "none", label: "Data order" },
    { value: "descending", label: "Largest first" },
    { value: "ascending", label: "Smallest first" }
  ],
  defaultValue: "none"
};
var cjsPieChartDef = {
  chart: "Pie Chart",
  template: { mark: "arc", encoding: {} },
  channels: ["size", "color", "column", "row"],
  markCognitiveChannel: "area",
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const colorField = channelSemantics.color?.field;
    const sizeField = channelSemantics.size?.field;
    const labels = [];
    const values = [];
    const palette = getChartJsPalette(ctx, "color");
    if (colorField && sizeField) {
      const agg = /* @__PURE__ */ new Map();
      for (const row of table) {
        const cat = String(row[colorField] ?? "");
        const val = Number(row[sizeField]) || 0;
        agg.set(cat, (agg.get(cat) ?? 0) + val);
      }
      const categories = extractCategories(table, colorField, channelSemantics.color?.ordinalSortOrder);
      for (const cat of categories) {
        labels.push(cat);
        values.push(agg.get(cat) ?? 0);
      }
    } else if (colorField) {
      const counts = /* @__PURE__ */ new Map();
      for (const row of table) {
        const cat = String(row[colorField] ?? "");
        counts.set(cat, (counts.get(cat) ?? 0) + 1);
      }
      const categories = extractCategories(table, colorField, channelSemantics.color?.ordinalSortOrder);
      for (const cat of categories) {
        labels.push(cat);
        values.push(counts.get(cat) ?? 0);
      }
    } else if (sizeField) {
      for (const row of table) {
        const val = Number(row[sizeField]) || 0;
        labels.push(String(val));
        values.push(val);
      }
    }
    const innerRadius = chartProperties?.innerRadius ?? 0;
    const isDoughnut = innerRadius > 0;
    sortSlicesInPlace(labels, values, chartProperties?.sortSlices);
    const config = {
      type: isDoughnut ? "doughnut" : "pie",
      data: {
        labels,
        datasets: [{
          data: values,
          backgroundColor: labels.map((_, i) => getSeriesBackgroundColor(palette, i, 0.6)),
          borderColor: labels.map((_, i) => getSeriesBorderColor(palette, i)),
          borderWidth: 1
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: {
          legend: { display: true, position: "right" },
          tooltip: { enabled: true }
        },
        ...isDoughnut ? { cutout: `${innerRadius}%` } : {}
      },
      // Canvas size from context (no axes)
      _width: Math.max(ctx.canvasSize.width, 300),
      _height: Math.max(ctx.canvasSize.height, 250)
    };
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "innerRadius", label: "Donut", type: "continuous", min: 0, max: 60, step: 5, defaultValue: 0 },
    SORT_SLICES_PROPERTY
  ]
};

// src/chartjs/templates/doughnut.ts
var cjsDoughnutChartDef = {
  chart: "Doughnut Chart",
  template: { mark: "arc", encoding: {} },
  channels: ["size", "color", "column", "row"],
  markCognitiveChannel: "area",
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const colorField = channelSemantics.color?.field;
    const sizeField = channelSemantics.size?.field;
    const labels = [];
    const values = [];
    const palette = getChartJsPalette(ctx, "color");
    if (colorField && sizeField) {
      const agg = /* @__PURE__ */ new Map();
      for (const row of table) {
        const cat = String(row[colorField] ?? "");
        const val = Number(row[sizeField]) || 0;
        agg.set(cat, (agg.get(cat) ?? 0) + val);
      }
      const categories = extractCategories(table, colorField, channelSemantics.color?.ordinalSortOrder);
      for (const cat of categories) {
        labels.push(cat);
        values.push(agg.get(cat) ?? 0);
      }
    } else if (colorField) {
      const counts = /* @__PURE__ */ new Map();
      for (const row of table) {
        const cat = String(row[colorField] ?? "");
        counts.set(cat, (counts.get(cat) ?? 0) + 1);
      }
      const categories = extractCategories(table, colorField, channelSemantics.color?.ordinalSortOrder);
      for (const cat of categories) {
        labels.push(cat);
        values.push(counts.get(cat) ?? 0);
      }
    } else if (sizeField) {
      for (const row of table) {
        const val = Number(row[sizeField]) || 0;
        labels.push(String(val));
        values.push(val);
      }
    }
    const cutout = chartProperties?.innerRadius ?? 55;
    sortSlicesInPlace(labels, values, chartProperties?.sortSlices);
    const config = {
      type: "doughnut",
      data: {
        labels,
        datasets: [{
          data: values,
          backgroundColor: labels.map((_, i) => getSeriesBackgroundColor(palette, i, 0.6)),
          borderColor: labels.map((_, i) => getSeriesBorderColor(palette, i)),
          borderWidth: 1
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        cutout: `${cutout}%`,
        plugins: {
          legend: { display: true, position: "right" },
          tooltip: { enabled: true }
        }
      },
      _width: Math.max(ctx.canvasSize.width, 300),
      _height: Math.max(ctx.canvasSize.height, 250)
    };
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "innerRadius", label: "Hole", type: "continuous", min: 20, max: 80, step: 5, defaultValue: 55 },
    {
      key: "sortSlices",
      label: "Sort slices",
      type: "discrete",
      options: [
        { value: "none", label: "Data order" },
        { value: "descending", label: "Largest first" },
        { value: "ascending", label: "Smallest first" }
      ],
      defaultValue: "none"
    }
  ]
};

// src/chartjs/templates/histogram.ts
var cjsHistogramDef = {
  chart: "Histogram",
  template: { mark: "bar", encoding: {} },
  channels: ["x", "color", "column", "row"],
  markCognitiveChannel: "length",
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const xField = channelSemantics.x?.field;
    const colorField = channelSemantics.color?.field;
    if (!xField) return;
    const palette = getChartJsPalette(ctx, "color");
    const numValues = table.map((r) => Number(r[xField])).filter((v) => isFinite(v));
    if (numValues.length === 0) return;
    const binCount = chartProperties?.binCount || 10;
    const minVal = Math.min(...numValues);
    const maxVal = Math.max(...numValues);
    const range = maxVal - minVal;
    const binWidth = range > 0 ? range / binCount : 1;
    const categories = Array.from({ length: binCount }, (_, i) => {
      const lo = (minVal + i * binWidth).toFixed(1);
      const hi = (minVal + (i + 1) * binWidth).toFixed(1);
      return `${lo}\u2013${hi}`;
    });
    const config = {
      type: "bar",
      data: {
        labels: categories,
        datasets: []
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            title: { display: true, text: xField }
          },
          y: {
            title: { display: true, text: "Count" },
            beginAtZero: true
          }
        },
        plugins: {
          tooltip: { enabled: true }
        }
      }
    };
    if (!colorField) {
      const counts = new Array(binCount).fill(0);
      for (const v of numValues) {
        let idx = Math.floor((v - minVal) / binWidth);
        if (idx >= binCount) idx = binCount - 1;
        counts[idx]++;
      }
      config.data.datasets.push({
        label: "Count",
        data: counts,
        backgroundColor: getSeriesBackgroundColor(palette, 0),
        borderColor: getSeriesBorderColor(palette, 0),
        borderWidth: 1,
        barPercentage: 1,
        categoryPercentage: 1
      });
      config.options.plugins.legend = { display: false };
    } else {
      const groupValues = /* @__PURE__ */ new Map();
      for (const row of table) {
        const v = Number(row[xField]);
        if (!isFinite(v)) continue;
        const g = String(row[colorField] ?? "");
        if (!groupValues.has(g)) groupValues.set(g, []);
        groupValues.get(g).push(v);
      }
      config.options.scales.x.stacked = true;
      config.options.scales.y.stacked = true;
      let colorIdx = 0;
      for (const [name, vals] of groupValues) {
        const counts = new Array(binCount).fill(0);
        for (const v of vals) {
          let idx = Math.floor((v - minVal) / binWidth);
          if (idx >= binCount) idx = binCount - 1;
          counts[idx]++;
        }
        config.data.datasets.push({
          label: name,
          data: counts,
          backgroundColor: getSeriesBackgroundColor(palette, colorIdx),
          borderColor: getSeriesBorderColor(palette, colorIdx),
          borderWidth: 1,
          barPercentage: 1,
          categoryPercentage: 1
        });
        colorIdx++;
      }
      config.options.plugins.legend = { display: true };
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "binCount", label: "Max Bins", type: "continuous", min: 5, max: 50, step: 1, defaultValue: 0 }
  ]
};

// src/chartjs/templates/ecdf.ts
function ecdfPoints(values) {
  const sorted = values.filter((v) => Number.isFinite(v)).sort((a, b) => a - b);
  const n = sorted.length;
  const out = [];
  if (n === 0) return out;
  let i = 0;
  while (i < n) {
    let j = i;
    while (j + 1 < n && sorted[j + 1] === sorted[i]) j++;
    out.push({ x: sorted[i], y: (j + 1) / n });
    i = j + 1;
  }
  return out;
}
var cjsEcdfPlotDef = {
  chart: "ECDF Plot",
  template: { mark: "line", encoding: {} },
  channels: ["x", "color", "detail", "column", "row"],
  markCognitiveChannel: "position",
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const xField = channelSemantics.x?.field;
    const groupField = channelSemantics.color?.field ?? channelSemantics.detail?.field;
    if (!xField) return;
    const showPoints = !!chartProperties?.showPoints;
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "line",
      data: { datasets: [] },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            type: "linear",
            title: { display: true, text: xField },
            ticks: { font: { size: 10 } }
          },
          y: {
            type: "linear",
            min: 0,
            max: 1,
            title: { display: true, text: "Cumulative proportion" },
            ticks: { font: { size: 10 } }
          }
        },
        plugins: {
          tooltip: { enabled: true },
          legend: { display: false }
        }
      }
    };
    const pushDataset = (name, values, idx) => {
      config.data.datasets.push({
        label: name,
        data: ecdfPoints(values),
        borderColor: getSeriesBorderColor(palette, idx),
        backgroundColor: "transparent",
        // step-after: hold the proportion until the next value, then jump.
        stepped: "after",
        pointRadius: showPoints ? 3 : 0,
        borderWidth: 2,
        fill: false,
        tension: 0
      });
    };
    if (groupField) {
      const groups = groupBy(table, groupField);
      config.options.plugins.legend = { display: true };
      let idx = 0;
      for (const [name, rows] of groups) {
        const values = rows.map((r) => Number(r[xField])).filter((v) => !isNaN(v));
        pushDataset(String(name), values, idx);
        idx++;
      }
    } else {
      const values = table.map((r) => Number(r[xField])).filter((v) => !isNaN(v));
      pushDataset(xField, values, 0);
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    { key: "showPoints", label: "Points", type: "binary", defaultValue: false }
  ]
};

// src/chartjs/templates/radar.ts
var cjsRadarChartDef = {
  chart: "Radar Chart",
  template: { mark: "point", encoding: {} },
  channels: ["x", "y", "color", "column", "row"],
  markCognitiveChannel: "position",
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties } = ctx;
    const axisField = channelSemantics.x?.field;
    const valueField = channelSemantics.y?.field;
    const groupField = channelSemantics.color?.field;
    if (!axisField || !valueField) return;
    const metrics = extractCategories(table, axisField, channelSemantics.x?.ordinalSortOrder);
    if (metrics.length < 2) return;
    const filled = chartProperties?.filled !== false;
    const fillOpacity = chartProperties?.fillOpacity ?? 0.3;
    const palette = getChartJsPalette(ctx, "color");
    const config = {
      type: "radar",
      data: {
        labels: metrics,
        datasets: []
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          r: {
            beginAtZero: true,
            ticks: { display: true },
            pointLabels: { display: true }
          }
        },
        plugins: {
          tooltip: { enabled: true }
        }
      },
      // Canvas size from context (no axes)
      _width: Math.max(ctx.canvasSize.width, 350),
      _height: Math.max(ctx.canvasSize.height, 300)
    };
    if (groupField) {
      const groups = groupBy(table, groupField);
      config.options.plugins.legend = { display: true };
      let colorIdx = 0;
      for (const [name, rows] of groups) {
        const metricVals = /* @__PURE__ */ new Map();
        for (const row of rows) {
          const m = String(row[axisField]);
          const v = Number(row[valueField]) || 0;
          if (!metricVals.has(m)) metricVals.set(m, { sum: 0, count: 0 });
          const entry = metricVals.get(m);
          entry.sum += v;
          entry.count++;
        }
        const values = metrics.map((m) => {
          const entry = metricVals.get(m);
          return entry ? Math.round(entry.sum / entry.count * 100) / 100 : 0;
        });
        const borderColor = getSeriesBorderColor(palette, colorIdx);
        const bgColor = getSeriesBackgroundColor(palette, colorIdx, fillOpacity);
        config.data.datasets.push({
          label: name,
          data: values,
          borderColor,
          backgroundColor: filled ? bgColor : "transparent",
          pointBackgroundColor: borderColor,
          fill: filled
        });
        colorIdx++;
      }
    } else {
      const metricVals = /* @__PURE__ */ new Map();
      for (const row of table) {
        const m = String(row[axisField]);
        const v = Number(row[valueField]) || 0;
        if (!metricVals.has(m)) metricVals.set(m, { sum: 0, count: 0 });
        const entry = metricVals.get(m);
        entry.sum += v;
        entry.count++;
      }
      const values = metrics.map((m) => {
        const entry = metricVals.get(m);
        return entry ? Math.round(entry.sum / entry.count * 100) / 100 : 0;
      });
      config.data.datasets.push({
        label: valueField,
        data: values,
        borderColor: getSeriesBorderColor(palette, 0),
        backgroundColor: filled ? getSeriesBackgroundColor(palette, 0, fillOpacity) : "transparent",
        pointBackgroundColor: getSeriesBorderColor(palette, 0),
        fill: filled
      });
      config.options.plugins.legend = { display: false };
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    {
      key: "filled",
      label: "Fill",
      type: "discrete",
      options: [
        { value: true, label: "Filled (default)" },
        { value: false, label: "Outline only" }
      ]
    },
    { key: "fillOpacity", label: "Opacity", type: "continuous", min: 0.05, max: 0.8, step: 0.05, defaultValue: 0.3 }
  ]
};

// src/chartjs/templates/rose.ts
var cjsRoseChartDef = {
  chart: "Rose Chart",
  template: { mark: "arc", encoding: {} },
  channels: ["x", "y", "color", "column", "row"],
  markCognitiveChannel: "area",
  instantiate: (spec, ctx) => {
    const { channelSemantics, table } = ctx;
    const catField = channelSemantics.x?.field;
    const valField = channelSemantics.y?.field;
    const colorField = channelSemantics.color?.field;
    if (!catField || !valField) return;
    const palette = getChartJsPalette(ctx, "color");
    const categories = extractCategories(table, catField, channelSemantics.x?.ordinalSortOrder);
    if (categories.length === 0) return;
    let labels;
    let values;
    let bgColors;
    let borderColors;
    if (colorField) {
      const groups = groupBy(table, colorField);
      [...groups.keys()];
      const catTotals = /* @__PURE__ */ new Map();
      for (const cat of categories) catTotals.set(cat, 0);
      for (const [, rows] of groups) {
        for (const row of rows) {
          const cat = String(row[catField] ?? "");
          const val = Number(row[valField]) || 0;
          if (catTotals.has(cat)) {
            catTotals.set(cat, catTotals.get(cat) + val);
          }
        }
      }
      labels = categories;
      values = categories.map((c) => catTotals.get(c) ?? 0);
      bgColors = categories.map((_, i) => getSeriesBackgroundColor(palette, i, 0.6));
      borderColors = categories.map((_, i) => getSeriesBorderColor(palette, i));
    } else {
      const catAgg = /* @__PURE__ */ new Map();
      for (const row of table) {
        const cat = String(row[catField] ?? "");
        const val = Number(row[valField]) || 0;
        catAgg.set(cat, (catAgg.get(cat) ?? 0) + val);
      }
      labels = categories;
      values = categories.map((c) => catAgg.get(c) ?? 0);
      bgColors = categories.map((_, i) => getSeriesBackgroundColor(palette, i, 0.6));
      borderColors = categories.map((_, i) => getSeriesBorderColor(palette, i));
    }
    sortSlicesInPlace(labels, values, ctx.chartProperties?.sortSlices);
    const rawValues = values.slice();
    const radii = values.map((v) => Math.sqrt(Math.max(0, v)));
    const alignment = ctx.chartProperties?.alignment ?? "left";
    const n = categories.length;
    const startAngle = alignment === "center" && n > 0 ? -(180 / n) : 0;
    const config = {
      type: "polarArea",
      data: {
        labels,
        datasets: [{
          data: radii,
          backgroundColor: bgColors,
          borderColor: borderColors,
          borderWidth: 1
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          r: {
            beginAtZero: true,
            startAngle,
            // Radii are sqrt(value); the raw tick numbers would
            // misrepresent the scale, so hide them (true values are
            // surfaced in the tooltip).
            ticks: { display: false }
          }
        },
        plugins: {
          legend: { display: true, position: "right" },
          tooltip: {
            enabled: true,
            callbacks: {
              // Radius is sqrt-transformed; report the true value.
              label: (item) => {
                const raw = rawValues[item.dataIndex];
                const shown = raw != null ? raw : item.raw;
                const name = item.label != null ? item.label : "";
                return `${name}: ${shown}`;
              }
            }
          }
        }
      },
      _width: Math.max(ctx.canvasSize.width, 350),
      _height: Math.max(ctx.canvasSize.height, 300)
    };
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: [
    {
      key: "alignment",
      label: "Alignment",
      type: "discrete",
      options: [
        { value: "left", label: "Left (default)" },
        { value: "center", label: "Center" }
      ]
    },
    {
      key: "sortSlices",
      label: "Sort slices",
      type: "discrete",
      options: [
        { value: "none", label: "Data order" },
        { value: "descending", label: "Largest first" },
        { value: "ascending", label: "Smallest first" }
      ],
      defaultValue: "none"
    }
  ]
};

// src/chart-types/gantt.ts
var GANTT_PROPERTIES = [
  { key: "taskHeight", label: "Task height", type: "continuous", min: 40, max: 90, step: 5, defaultValue: 70 },
  { key: "cornerRadius", label: "Corners", type: "continuous", min: 0, max: 8, step: 1, defaultValue: 2 },
  { key: "intervalLabels", label: "Labels", type: "binary", defaultValue: false }
];
function sortGanttRows(rows) {
  return [...rows].sort((a, b) => a.start - b.start || a.inputIndex - b.inputIndex);
}
function coerceGanttEndpoint(value, temporal) {
  if (value == null) return NaN;
  if (!temporal) return Number(value);
  if (value instanceof Date) return value.getTime();
  if (typeof value === "number") return Math.abs(value) < 1e11 ? value * 1e3 : value;
  return Date.parse(String(value));
}
function isGanttTemporal(resolvedType, semanticType) {
  if (resolvedType === "temporal") return true;
  const typeName = toTypeString(semanticType);
  return typeName ? isTimeSeriesType(typeName) : false;
}
function compactNumber(value) {
  return Number.isInteger(value) ? String(value) : String(Number(value.toFixed(2)));
}
var GANTT_DURATION_UNITS = [
  { minimumMs: 864e5, divisorMs: 864e5, suffix: "d" },
  { minimumMs: 36e5, divisorMs: 36e5, suffix: "h" },
  { minimumMs: 6e4, divisorMs: 6e4, suffix: "min" },
  { minimumMs: 1e3, divisorMs: 1e3, suffix: "s" },
  { minimumMs: 0, divisorMs: 1, suffix: "ms" }
];
function formatGanttDuration(durationMs) {
  const unit = GANTT_DURATION_UNITS.find(({ minimumMs }) => Math.abs(durationMs) >= minimumMs);
  return `${compactNumber(durationMs / unit.divisorMs)}${unit.suffix}`;
}
function formatGanttLabel(start, end, temporal) {
  const duration = end - start;
  if (!temporal) return compactNumber(duration);
  return formatGanttDuration(duration);
}
function ganttLabelReservePx(rows, temporal) {
  const maxCharacters = rows.reduce((max, row) => Math.max(max, formatGanttLabel(row.start, row.end, temporal).length), 0);
  return Math.max(40, maxCharacters * 7 + 10);
}

// src/chartjs/templates/gantt.ts
function fmtDate(ms) {
  if (!Number.isFinite(ms)) return "";
  return new Date(ms).toISOString().slice(0, 10);
}
var cjsGanttChartDef = {
  chart: "Gantt Chart",
  template: { mark: "bar", encoding: {} },
  channels: ["y", "x", "x2", "color", "column", "row"],
  markCognitiveChannel: "position",
  declareLayoutMode: () => ({ axisFlags: { y: { banded: true } } }),
  instantiate: (spec, ctx) => {
    const { channelSemantics, table, chartProperties, semanticTypes } = ctx;
    const taskField = channelSemantics.y?.field;
    const startField = channelSemantics.x?.field;
    const endField = channelSemantics.x2?.field;
    const colorField = channelSemantics.color?.field;
    if (!taskField || !startField || !endField || table.length === 0) return;
    const temporal = isGanttTemporal(channelSemantics.x?.type, semanticTypes[startField]);
    const num = (v) => coerceGanttEndpoint(v, temporal);
    const rows = sortGanttRows(table.map((r, inputIndex) => ({
      task: String(r[taskField] ?? ""),
      start: num(r[startField]),
      end: num(r[endField]),
      group: colorField != null ? String(r[colorField] ?? "") : void 0,
      inputIndex
    })).filter((r) => r.task && Number.isFinite(r.start) && Number.isFinite(r.end)));
    const taskHeight = chartProperties?.taskHeight ?? 70;
    const cornerRadius = chartProperties?.cornerRadius ?? 2;
    const intervalLabels = chartProperties?.intervalLabels === true;
    const labelReserve = ganttLabelReservePx(rows, temporal);
    const groups = colorField ? Array.from(new Set(rows.map((r) => r.group ?? ""))) : [];
    const groupIndex = /* @__PURE__ */ new Map();
    groups.forEach((g, i) => groupIndex.set(g, i));
    const colorFor = (group) => {
      const i = group != null ? groupIndex.get(group) ?? 0 : 0;
      return {
        bg: DEFAULT_BG_COLORS[i % DEFAULT_BG_COLORS.length],
        border: DEFAULT_COLORS[i % DEFAULT_COLORS.length]
      };
    };
    const labels = rows.map((r) => r.task);
    const data = rows.map((r) => [r.start, r.end]);
    const bg = rows.map((r) => colorFor(r.group).bg);
    const border = rows.map((r) => colorFor(r.group).border);
    const config = {
      type: "bar",
      data: {
        labels,
        datasets: [{
          label: taskField,
          data,
          backgroundColor: bg,
          borderColor: border,
          borderWidth: 1,
          borderRadius: cornerRadius,
          borderSkipped: false,
          barPercentage: taskHeight / 100
        }]
      },
      options: {
        indexAxis: "y",
        responsive: true,
        maintainAspectRatio: false,
        ...intervalLabels ? { layout: { padding: { right: labelReserve } } } : {},
        scales: {
          x: {
            beginAtZero: false,
            title: { display: !temporal, text: startField },
            ...temporal ? { ticks: { callback: (v) => fmtDate(Number(v)) } } : {}
          },
          y: { title: { display: false } }
        },
        plugins: {
          legend: colorField && groups.length > 1 ? {
            display: true,
            labels: {
              generateLabels: () => groups.map((g) => {
                const c = colorFor(g);
                return {
                  text: g,
                  fillStyle: c.bg,
                  strokeStyle: c.border,
                  lineWidth: 1
                };
              })
            }
          } : { display: false },
          tooltip: {
            callbacks: {
              label: (item) => {
                const [s, e] = item.raw;
                const fs = temporal ? fmtDate(s) : s;
                const fe = temporal ? fmtDate(e) : e;
                return `${fs} \u2192 ${fe}`;
              }
            }
          }
        }
      }
    };
    if (intervalLabels) {
      config.plugins = [{
        id: "ganttLabels",
        afterDatasetsDraw: (chart) => {
          const { ctx: canvasContext } = chart;
          canvasContext.save();
          canvasContext.fillStyle = "#333333";
          canvasContext.font = "10px sans-serif";
          canvasContext.textAlign = "left";
          canvasContext.textBaseline = "middle";
          chart.getDatasetMeta(0).data.forEach((bar, index) => {
            const row = rows[index];
            if (!row) return;
            canvasContext.fillText(
              formatGanttLabel(row.start, row.end, temporal),
              bar.x + 4,
              bar.y
            );
          });
          canvasContext.restore();
        }
      }];
    }
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  },
  properties: GANTT_PROPERTIES
};

// src/chart-types/waterfall.ts
function waterfallLastReconciles(values) {
  if (values.length < 2) return false;
  let cumPrev = 0;
  for (let i = 0; i < values.length - 1; i++) {
    if (!Number.isFinite(values[i])) return false;
    cumPrev += values[i];
  }
  const last = values[values.length - 1];
  if (!Number.isFinite(last)) return false;
  const tol = Math.max(1e-6, 5e-3 * Math.abs(cumPrev));
  return Math.abs(last - cumPrev) <= tol;
}
function recommendedTotalsMode(values) {
  return waterfallLastReconciles(values) ? "both" : "first";
}
function resolveTotalsMode(values, explicit) {
  if (explicit === "none" || explicit === "first" || explicit === "last" || explicit === "both") {
    return explicit;
  }
  return recommendedTotalsMode(values);
}

// src/chartjs/templates/waterfall.ts
var COLOR = {
  startEnd: { bg: "rgba(84, 112, 198, 0.65)", border: "rgba(84, 112, 198, 1)" },
  increase: { bg: "rgba(145, 204, 117, 0.65)", border: "rgba(145, 204, 117, 1)" },
  decrease: { bg: "rgba(238, 102, 102, 0.65)", border: "rgba(238, 102, 102, 1)" }
};
var cjsWaterfallChartDef = {
  chart: "Waterfall Chart",
  template: { mark: "bar", encoding: {} },
  channels: ["x", "y", "color", "column", "row"],
  markCognitiveChannel: "length",
  declareLayoutMode: () => ({ axisFlags: { x: { banded: true } } }),
  instantiate: (spec, ctx) => {
    const { channelSemantics, table } = ctx;
    const catField = channelSemantics.x?.field;
    const valField = channelSemantics.y?.field;
    const colorField = channelSemantics.color?.field;
    if (!catField || !valField || table.length === 0) return;
    const categories = extractCategories(table, catField, channelSemantics.x?.ordinalSortOrder);
    const rows = categories.map((cat) => table.find((r) => String(r[catField]) === cat)).filter(Boolean);
    const values = rows.map((r) => Number(r[valField]) || 0);
    const totalsMode = resolveTotalsMode(values, ctx.chartProperties?.totals);
    const wantFirst = totalsMode === "first" || totalsMode === "both";
    const wantLast = totalsMode === "last" || totalsMode === "both";
    const types = colorField ? rows.map((r) => String(r[colorField] ?? "delta")) : values.map((_, i) => wantFirst && i === 0 ? "start" : wantLast && i === values.length - 1 ? "end" : "delta");
    const cumulative = [];
    let acc = 0;
    for (const v of values) {
      acc += v;
      cumulative.push(acc);
    }
    const data = [];
    const bg = [];
    const border = [];
    for (let i = 0; i < values.length; i++) {
      const v = values[i];
      const t = types[i];
      const isTotal = t === "start" || t === "end";
      const top = t === "end" ? cumulative[i] - v : cumulative[i];
      const prev = isTotal ? 0 : cumulative[i] - v;
      const lo = Math.min(prev, top);
      const hi = Math.max(prev, top);
      data.push([lo, hi]);
      const c = isTotal ? COLOR.startEnd : top >= prev ? COLOR.increase : COLOR.decrease;
      bg.push(c.bg);
      border.push(c.border);
    }
    const legendItems = [
      { text: "Start/End", color: COLOR.startEnd },
      { text: "Increase", color: COLOR.increase },
      { text: "Decrease", color: COLOR.decrease }
    ];
    const config = {
      type: "bar",
      data: {
        labels: categories,
        datasets: [{
          label: valField,
          data,
          backgroundColor: bg,
          borderColor: border,
          borderWidth: 1,
          borderSkipped: false
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: { title: { display: true, text: catField } },
          y: { beginAtZero: true, title: { display: true, text: valField } }
        },
        plugins: {
          legend: {
            display: true,
            labels: {
              generateLabels: () => legendItems.map((it) => ({
                text: it.text,
                fillStyle: it.color.bg,
                strokeStyle: it.color.border,
                lineWidth: 1
              }))
            }
          },
          tooltip: {
            callbacks: {
              label: (item) => {
                const [lo, hi] = item.raw;
                return `${valField}: ${Math.round((hi - lo) * 100) / 100}`;
              }
            }
          }
        }
      }
    };
    Object.assign(spec, config);
    delete spec.mark;
    delete spec.encoding;
  }
};

// src/chartjs/templates/index.ts
var cjsTemplateDefs = {
  "Scatter & Point": [cjsScatterPlotDef, cjsConnectedScatterDef, cjsBubbleChartDef, cjsStripPlotDef],
  "Bar": [cjsBarChartDef, cjsGroupedBarChartDef, cjsStackedBarChartDef, cjsLollipopChartDef, cjsComboChartDef, cjsHistogramDef, cjsWaterfallChartDef, cjsGanttChartDef],
  "Line & Area": [cjsLineChartDef, cjsBumpChartDef, cjsSlopeChartDef, cjsAreaChartDef, cjsRangeAreaChartDef, cjsEcdfPlotDef],
  "Part-to-Whole": [cjsPieChartDef, cjsDoughnutChartDef],
  "Polar": [cjsRadarChartDef, cjsRoseChartDef]
};
var cjsAllTemplateDefs = Object.values(cjsTemplateDefs).flat();
function cjsGetTemplateDef(chartType) {
  return cjsAllTemplateDefs.find((t) => t.chart === chartType);
}
function cjsGetTemplateChannels(chartType) {
  return cjsGetTemplateDef(chartType)?.channels || [];
}

// src/core/decisions.ts
function visCategoryToVLType(vc) {
  switch (vc) {
    case "quantitative":
      return "quantitative";
    case "ordinal":
      return "ordinal";
    case "temporal":
      return "temporal";
    case "geographic":
      return "quantitative";
    case "nominal":
    default:
      return "nominal";
  }
}
function validateTemporalParsing(data, fieldName, fromRegistry) {
  const sampleValues = [];
  const seen = /* @__PURE__ */ new Set();
  for (const row of data) {
    const value = row[fieldName];
    if (value == null) continue;
    const key = value instanceof Date ? `date:${value.getTime()}` : `${typeof value}:${String(value)}`;
    if (seen.has(key)) continue;
    seen.add(key);
    sampleValues.push(value);
    if (sampleValues.length >= 15) break;
  }
  if (sampleValues.length === 0) return false;
  if (sampleValues.length <= 1) return false;
  const looksTemporalValue = (val) => {
    if (val instanceof Date) return true;
    if (typeof val === "number") {
      if (val >= 1500 && val <= 2200 && val % 1 === 0) return true;
      if (val > 864e5 && val < 42e11) return true;
      return false;
    }
    if (typeof val === "string") {
      const trimmed = val.trim();
      if (!trimmed) return false;
      if (/^\d{4}$/.test(trimmed)) return true;
      return !Number.isNaN(Date.parse(trimmed));
    }
    return false;
  };
  const passingCount = sampleValues.filter(looksTemporalValue).length;
  const minFraction = fromRegistry ? 0.3 : 0.5;
  return passingCount / sampleValues.length >= minFraction;
}
function resolveTemporalEncoding(visCategory, channel, data, fieldName, fromRegistry) {
  if (["size", "column", "row"].includes(channel)) {
    return { vlType: "ordinal", visCategory, channelOverride: true, cardinalityGuard: false };
  }
  if (channel === "color") {
    const uniqueCount = new Set(data.map((r) => r[fieldName])).size;
    if (uniqueCount <= 12) {
      return { vlType: "ordinal", visCategory, channelOverride: true, cardinalityGuard: false };
    }
  }
  if (!validateTemporalParsing(data, fieldName, fromRegistry)) {
    return { vlType: "ordinal", visCategory, channelOverride: false, cardinalityGuard: false };
  }
  return { vlType: "temporal", visCategory, channelOverride: false, cardinalityGuard: false };
}
function applyOrdinalGuards(visCategory, channel, data, fieldName, fieldValues, fromRegistry) {
  const numericVals = fieldValues.filter((v) => v != null && !isNaN(+v)).map(Number);
  if (numericVals.length > 0) {
    const uniqueCount = new Set(numericVals).size;
    const hasFractions = numericVals.some((v) => v % 1 !== 0);
    if (!fromRegistry && hasFractions && uniqueCount > 20) {
      return { vlType: "quantitative", visCategory, channelOverride: false, cardinalityGuard: true };
    }
    if (!hasFractions && uniqueCount > 12 && ["color", "group"].includes(channel)) {
      return { vlType: "quantitative", visCategory, channelOverride: true, cardinalityGuard: true };
    }
    if (!hasFractions && uniqueCount > 12 && ["x", "y"].includes(channel)) {
      return { vlType: "quantitative", visCategory, channelOverride: true, cardinalityGuard: true };
    }
  }
  return { vlType: "ordinal", visCategory, channelOverride: false, cardinalityGuard: false };
}
function disambiguateMultiEncoding(candidates, channel, data, fieldName, fieldValues) {
  const has = (vc) => candidates.includes(vc);
  if (has("temporal") && has("ordinal")) {
    return resolveTemporalEncoding("temporal", channel, data, fieldName, true);
  }
  if (has("quantitative") && has("ordinal")) {
    if (["color", "group"].includes(channel)) {
      const uniqueCount = new Set(data.map((r) => r[fieldName])).size;
      if (uniqueCount <= 12) {
        return { vlType: "ordinal", visCategory: "ordinal", channelOverride: false, cardinalityGuard: false };
      }
      return { vlType: "quantitative", visCategory: "quantitative", channelOverride: false, cardinalityGuard: true };
    }
    if (["column", "row"].includes(channel)) {
      return { vlType: "ordinal", visCategory: "ordinal", channelOverride: false, cardinalityGuard: false };
    }
    return { vlType: "quantitative", visCategory: "quantitative", channelOverride: false, cardinalityGuard: false };
  }
  if (has("quantitative") && has("geographic")) {
    return { vlType: "quantitative", visCategory: "quantitative", channelOverride: false, cardinalityGuard: false };
  }
  if (has("ordinal") && has("nominal")) {
    if (["color", "group"].includes(channel)) {
      return { vlType: "nominal", visCategory: "nominal", channelOverride: false, cardinalityGuard: false };
    }
    return { vlType: "ordinal", visCategory: "ordinal", channelOverride: false, cardinalityGuard: false };
  }
  const fallback = candidates[0];
  return { vlType: visCategoryToVLType(fallback), visCategory: fallback, channelOverride: false, cardinalityGuard: false };
}
function resolveEncodingType(semanticType, fieldValues, channel, data, fieldName) {
  if (semanticType && isRegistered(semanticType)) {
    const entry = getRegistryEntry(semanticType);
    const candidates = entry.visEncodings;
    if (candidates.length > 1) {
      return disambiguateMultiEncoding(candidates, channel, data, fieldName);
    }
    const baseType = candidates[0];
    if (baseType === "quantitative") {
      const nonNull = fieldValues.filter((v) => v != null);
      const allNumeric = nonNull.length > 0 && nonNull.every((v) => typeof v === "number" || typeof v === "string" && !isNaN(+v) && v.trim() !== "");
      if (!allNumeric) {
        const inferred = inferVisCategory(fieldValues);
        return {
          vlType: visCategoryToVLType(inferred),
          visCategory: inferred,
          channelOverride: false,
          cardinalityGuard: false
        };
      }
    }
    if (baseType === "temporal") {
      return resolveTemporalEncoding(baseType, channel, data, fieldName, true);
    }
    if (baseType === "ordinal") {
      return applyOrdinalGuards(baseType, channel, data, fieldName, fieldValues, true);
    }
    return {
      vlType: visCategoryToVLType(baseType),
      visCategory: baseType,
      channelOverride: false,
      cardinalityGuard: false
    };
  }
  const visCategory = inferVisCategory(fieldValues);
  const channelOverride = false;
  const cardinalityGuard = false;
  switch (visCategory) {
    case "temporal":
      return resolveTemporalEncoding(visCategory, channel, data, fieldName, false);
    case "ordinal":
      return applyOrdinalGuards(visCategory, channel, data, fieldName, fieldValues, false);
    case "quantitative":
      return { vlType: "quantitative", visCategory, channelOverride, cardinalityGuard };
    case "geographic":
      return { vlType: "quantitative", visCategory, channelOverride, cardinalityGuard };
    case "nominal":
    default:
      return { vlType: "nominal", visCategory, channelOverride, cardinalityGuard };
  }
}
var DEFAULT_GAS_PRESSURE_PARAMS = {
  markCrossSection: 30,
  elasticity: 0.3,
  maxStretch: 1.5
};
function computeGasPressure(xValues, yValues, xDomain, yDomain, canvasWidth, canvasHeight, params = DEFAULT_GAS_PRESSURE_PARAMS) {
  const N = xValues.length;
  if (N <= 1 || canvasWidth <= 0 || canvasHeight <= 0) {
    return { stretchX: 1, stretchY: 1, rawStretchX: 1, rawStretchY: 1 };
  }
  const sigma1dDefault = Math.sqrt(params.markCrossSection);
  const computeAxisStretch = (values, domain, baseDim, sigma1d) => {
    if (baseDim <= 0 || values.length <= 1) return [1, 1];
    const range = domain[1] - domain[0];
    if (range <= 0) return [1, 1];
    const pxPerUnit = baseDim / range;
    const seen = /* @__PURE__ */ new Set();
    for (const v of values) {
      seen.add(Math.round((v - domain[0]) * pxPerUnit));
    }
    const uniquePositions = seen.size;
    const pressure = uniquePositions * sigma1d / baseDim;
    if (pressure <= 1) return [1, 1];
    const raw = Math.pow(pressure, params.elasticity);
    return [Math.min(params.maxStretch, raw), raw];
  };
  const sigma1dX = params.markCrossSectionX != null ? Math.sqrt(params.markCrossSectionX) : sigma1dDefault;
  const sigma1dY = params.markCrossSectionY != null ? Math.sqrt(params.markCrossSectionY) : sigma1dDefault;
  const computeStretchForAxis = (values, domain, baseDim, sigma1d, sigmaRaw, itemCountOverride) => {
    if (itemCountOverride != null && sigmaRaw > 0) {
      const pressure = itemCountOverride * sigmaRaw / baseDim;
      if (pressure <= 1) return [1, 1];
      const raw = Math.pow(pressure, params.elasticity);
      return [Math.min(params.maxStretch, raw), raw];
    }
    return sigma1d > 0 ? computeAxisStretch(values, domain, baseDim, sigma1d) : [1, 1];
  };
  const sigmaRawX = params.markCrossSectionX ?? params.markCrossSection;
  const sigmaRawY = params.markCrossSectionY ?? params.markCrossSection;
  const [stretchX, rawStretchX] = computeStretchForAxis(xValues, xDomain, canvasWidth, sigma1dX, sigmaRawX, params.xItemCountOverride);
  const [stretchY, rawStretchY] = computeStretchForAxis(yValues, yDomain, canvasHeight, sigma1dY, sigmaRawY, params.yItemCountOverride);
  return { stretchX, stretchY, rawStretchX, rawStretchY };
}
function computeElasticBudget(itemCount, baseDimension, params) {
  if (itemCount <= 0) {
    return { budget: baseDimension, stretchFactor: 1 };
  }
  const pressure = itemCount * params.defaultStepSize / baseDimension;
  if (pressure <= 1) {
    return { budget: baseDimension, stretchFactor: 1 };
  }
  const stretchFactor = Math.min(params.maxStretch, Math.pow(pressure, params.elasticity));
  return {
    budget: baseDimension * stretchFactor,
    stretchFactor
  };
}
function computeAxisStep(nominalCount, continuousCount, baseDimension, params) {
  const itemCount = nominalCount > 0 ? nominalCount : continuousCount;
  if (itemCount > 0) {
    const fit = Math.max(0, Math.min(1, params.bandStepFit ?? 0));
    const baseSpanStep = baseDimension / itemCount;
    const capacityStep = baseDimension * (params.bandStepFitCapacity ?? 1) / itemCount;
    const preferredStep = baseSpanStep > params.defaultStepSize ? params.defaultStepSize * (1 - fit) + capacityStep * fit : params.defaultStepSize;
    if (preferredStep <= baseSpanStep) {
      return { step: Math.floor(preferredStep), budget: baseDimension, itemCount };
    }
    const { budget } = computeElasticBudget(itemCount, baseDimension, {
      ...params,
      defaultStepSize: preferredStep
    });
    return {
      step: Math.floor(Math.min(preferredStep, budget / itemCount)),
      budget,
      itemCount
    };
  }
  return { step: params.defaultStepSize, budget: baseDimension, itemCount: 0 };
}
function computeLabelSizing(effectiveStep, hasDiscreteItems, opts) {
  const baseFont = opts?.baseFont ?? 10;
  const minFont = opts?.minFont;
  const defaultLimit = 100;
  if (!hasDiscreteItems) {
    return { fontSize: baseFont, labelLimit: defaultLimit };
  }
  let fontSize = Math.max(minFont, Math.min(baseFont, effectiveStep - 1));
  let labelLimit = Math.max(30, Math.min(100, effectiveStep * 8));
  let labelAngle;
  let labelAlign;
  let labelBaseline;
  if (effectiveStep < 10) {
    labelAngle = -90;
    fontSize = Math.max(minFont, Math.min(baseFont - 2, effectiveStep));
    labelLimit = 40;
    labelAlign = "right";
    labelBaseline = "middle";
  } else if (effectiveStep < 16) {
    labelAngle = -45;
    fontSize = Math.max(minFont, Math.min(baseFont - 1, effectiveStep));
    labelLimit = 60;
    labelAlign = "right";
    labelBaseline = "top";
  }
  return { fontSize, labelLimit, labelAngle, labelAlign, labelBaseline };
}
function computeFontSizing(minPlotDimension, opts) {
  const baseLabel = opts?.baseLabelFontSize ?? 10;
  const baseTitle = opts?.baseTitleFontSize ?? 11;
  const minDim = minPlotDimension || 320;
  const ratio = minDim >= 220 ? 1 : Math.max(0.7, minDim / 220);
  const atMostNative = (base) => Math.round(Math.max(base - 2, Math.min(base, base * ratio)));
  const tickBase = atMostNative(baseLabel);
  const titleFontSize = atMostNative(baseTitle);
  const legendFontSize = Math.max(baseTitle - 2, titleFontSize - 1);
  return { tickBase, titleFontSize, legendFontSize };
}

// src/core/resolve-semantics.ts
var MAX_TIMESTAMP_SEC = 4102444800;
var MAX_TIMESTAMP_MS = 41024448e5;
function isLikelyTimestamp(val) {
  if (val >= 1e9 && val <= MAX_TIMESTAMP_SEC) return true;
  if (val > MAX_TIMESTAMP_SEC && val <= MAX_TIMESTAMP_MS) return true;
  return false;
}
function timestampToMs(val) {
  return val <= MAX_TIMESTAMP_SEC ? val * 1e3 : val;
}
function inferImplicitSemanticType(fieldName, values) {
  const tokens = fieldName.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);
  if (!tokens.includes("year")) return "";
  const observed = values.filter((value) => value != null && value !== "");
  if (new Set(observed.map(String)).size <= 1) return "";
  const allYears = observed.every((value) => {
    const numeric = typeof value === "number" ? value : Number(value);
    return Number.isInteger(numeric) && numeric >= 1500 && numeric <= 2200;
  });
  return allYears ? "Year" : "";
}
function analyzeTemporalField(fieldValues) {
  const dates = [];
  let nonNull = 0;
  for (const v of fieldValues.slice(0, 100)) {
    if (v == null) continue;
    nonNull++;
    const d = v instanceof Date ? v : new Date(v);
    if (!isNaN(d.getTime())) dates.push(d);
  }
  if (dates.length < 2 || dates.length < nonNull * 0.5) return null;
  const monthSet = new Set(dates.map((d) => d.getUTCMonth()));
  const daySet = new Set(dates.map((d) => d.getUTCDate()));
  const hourSet = new Set(dates.map((d) => d.getUTCHours()));
  const minuteSet = new Set(dates.map((d) => d.getUTCMinutes()));
  const secondSet = new Set(dates.map((d) => d.getUTCSeconds()));
  const yearSet = new Set(dates.map((d) => d.getUTCFullYear()));
  const isSmallSpread = (s, maxSpread = 1) => {
    if (s.size <= 1) return true;
    const arr = [...s];
    return Math.max(...arr) - Math.min(...arr) <= maxSpread;
  };
  const same = {
    month: monthSet.size === 1,
    day: daySet.size === 1,
    hour: isSmallSpread(hourSet, 1),
    minute: minuteSet.size === 1,
    second: secondSet.size === 1
  };
  const sameYear = yearSet.size === 1;
  const sameMonth = sameYear && same.month;
  const sameDay = sameMonth && same.day;
  return { dates, same, sameYear, sameMonth, sameDay };
}
function computeDataVotes(same) {
  const votes = [0, 0, 0, 0, 0, 0];
  if (same.second) votes[5] += 1;
  if (same.minute && same.second) votes[5] += 1;
  if (same.hour && same.minute && same.second) votes[5] += 1;
  if (same.day && same.hour && same.minute && same.second) votes[5] += 2;
  if (same.month && same.day && same.hour && same.minute && same.second) votes[5] += 3;
  if (same.second) votes[4] += 1;
  if (same.minute && same.second) votes[4] += 1;
  if (same.hour && same.minute && same.second) votes[4] += 1;
  if (same.day && same.hour && same.minute && same.second) votes[4] += 2;
  if (!same.month && same.day && same.hour && same.minute && same.second) votes[4] += 3;
  if (same.second) votes[3] += 1;
  if (same.minute && same.second) votes[3] += 1;
  if (same.hour && same.minute && same.second) votes[3] += 1;
  if (!same.day && same.hour && same.minute && same.second) votes[3] += 3;
  if (same.second) votes[2] += 1;
  if (same.minute && same.second) votes[2] += 1;
  if (!same.hour && same.minute && same.second) votes[2] += 3;
  if (same.second) votes[1] += 1;
  if (!same.minute && same.second) votes[1] += 3;
  if (!same.second) votes[0] += 4;
  return votes;
}
var SEMANTIC_LEVEL = {
  Year: 5,
  Decade: 5,
  YearMonth: 4,
  Month: 4,
  YearQuarter: 4,
  Quarter: 4,
  Date: 3,
  Day: 3,
  Hour: 2,
  DateTime: 1,
  Timestamp: 0
};
function pickBestLevel(votes) {
  let bestLevel = 0;
  let bestScore = votes[0];
  for (let i = 1; i <= 5; i++) {
    if (votes[i] >= bestScore) {
      bestScore = votes[i];
      bestLevel = i;
    }
  }
  return { level: bestLevel, score: bestScore };
}
function levelToFormat(level, analysis) {
  switch (level) {
    case 5:
      return "%Y";
    case 4:
      return analysis.sameYear ? "%b" : "%b %Y";
    case 3:
      return analysis.sameYear ? "%b %d" : "%b %d, %Y";
    case 2:
      return analysis.sameDay ? "%H:00" : "%b %d %H:00";
    case 1:
      return analysis.sameDay ? "%H:%M" : "%b %d %H:%M";
    case 0:
      return analysis.sameDay ? "%H:%M:%S" : "%b %d %H:%M:%S";
    default:
      return null;
  }
}
function resolveTemporalFormat(fieldValues, semanticType) {
  const analysis = analyzeTemporalField(fieldValues);
  if (!analysis) return null;
  const votes = computeDataVotes(analysis.same);
  const semLevel = SEMANTIC_LEVEL[semanticType];
  if (semLevel !== void 0) votes[semLevel] += 3;
  const { level } = pickBestLevel(votes);
  return levelToFormat(level, analysis);
}
function expandToFullYear(val) {
  const trimmed = val.trim();
  if (/^\d{2}$/.test(trimmed)) {
    const n = parseInt(trimmed, 10);
    return String(n <= 49 ? 2e3 + n : 1900 + n);
  }
  return val;
}
function convertTemporalData(data, semanticTypes) {
  if (data.length === 0) return data;
  const keys = Object.keys(data[0]);
  const fieldValues = Object.fromEntries(keys.map((key) => [key, data.map((row) => row[key])]));
  const effectiveSemanticTypes = Object.fromEntries(keys.map((key) => [
    key,
    toTypeString(semanticTypes[key]) || inferImplicitSemanticType(key, fieldValues[key])
  ]));
  const temporalKeys = keys.filter((k) => {
    const st = effectiveSemanticTypes[k];
    const vc = inferVisCategory(fieldValues[k]);
    const stCategory = st ? getVisCategory(st) : null;
    return vc === "temporal" || stCategory === "temporal" || st === "Decade";
  });
  if (temporalKeys.length === 0) return data;
  const values = structuredClone(data);
  return values.map((r) => {
    for (const temporalKey of temporalKeys) {
      const val = r[temporalKey];
      const st = effectiveSemanticTypes[temporalKey];
      if (typeof val === "number") {
        if (st === "Year" || st === "Decade") {
          r[temporalKey] = `${Math.floor(val)}`;
        } else if (isLikelyTimestamp(val)) {
          r[temporalKey] = new Date(timestampToMs(val)).toISOString();
        } else {
          r[temporalKey] = String(val);
        }
      } else if (val instanceof Date) {
        r[temporalKey] = val.toISOString();
      } else {
        if ((st === "Year" || st === "Decade") && typeof val === "string") {
          r[temporalKey] = expandToFullYear(val);
        } else {
          r[temporalKey] = String(val);
        }
      }
    }
    return r;
  });
}
function resolveChannelSemantics(encodings, data, semanticTypes, convertedData) {
  const result = {};
  const temporalData = convertedData ?? data;
  for (const [channel, encoding] of Object.entries(encodings)) {
    const fieldName = encoding.field;
    if (!fieldName && encoding.aggregate !== "count") continue;
    if (!fieldName && encoding.aggregate === "count") {
      result[channel] = {
        field: "_count",
        semanticAnnotation: { semanticType: "Count" },
        type: "quantitative",
        aggregationDefault: "sum"
      };
      continue;
    }
    if (!fieldName) continue;
    const rawAnnotation = semanticTypes[fieldName];
    const suppliedSemanticType = typeof rawAnnotation === "string" ? rawAnnotation || "" : rawAnnotation?.semanticType ?? "";
    const fieldValues = data.map((r) => r[fieldName]);
    const semanticType = suppliedSemanticType || inferImplicitSemanticType(fieldName, fieldValues);
    const typeDecision = resolveEncodingType(
      semanticType,
      fieldValues,
      channel,
      data,
      fieldName
    );
    let resolvedType = typeDecision.vlType;
    if (encoding.type) {
      resolvedType = encoding.type;
    } else if (channel === "column" || channel === "row") {
      if (resolvedType !== "nominal" && resolvedType !== "ordinal") {
        resolvedType = "nominal";
      }
    }
    if (resolvedType === "quantitative") {
      const sampleValues = data.slice(0, 15).filter((r) => r[fieldName] != void 0).map((r) => r[fieldName]);
      const isoDateRegex = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})?$/;
      if (sampleValues.length > 0 && sampleValues.every((val) => isoDateRegex.test(`${val}`.trim()))) {
        resolvedType = "temporal";
      }
    }
    const fc = resolveFieldSemantics(rawAnnotation || semanticType, fieldName, fieldValues);
    const annotation = fc.semanticAnnotation;
    const tickConstraint = resolveTickConstraint(annotation.semanticType, annotation.intrinsicDomain);
    const reversed = resolveReversed(annotation.semanticType, channel);
    const nice = resolveNice(annotation.semanticType, fc.domainConstraint);
    const stackable = resolveStackable(annotation.semanticType);
    const cs = {
      field: fieldName,
      semanticAnnotation: annotation,
      type: resolvedType,
      // From FieldSemantics (data identity)
      format: fc.format,
      tooltipFormat: fc.tooltipFormat,
      aggregationDefault: fc.aggregationDefault,
      scaleType: fc.scaleType,
      domainConstraint: fc.domainConstraint,
      cyclic: fc.cyclic || void 0,
      sortDirection: fc.sortDirection,
      binningSuggested: fc.binningSuggested || void 0,
      // Channel-specific visualization decisions
      nice,
      tickConstraint,
      reversed: reversed || void 0,
      stackable
    };
    if (encoding.aggregate) {
      if (encoding.aggregate === "count") {
        cs.field = "_count";
        cs.type = "quantitative";
      } else {
        cs.field = `${fieldName}_${encoding.aggregate}`;
        cs.type = "quantitative";
      }
    }
    if ((channel === "color" || channel === "group") && fieldName) {
      if (encoding.scheme && encoding.scheme !== "default") {
        cs.colorScheme = {
          scheme: encoding.scheme,
          type: "categorical",
          reason: "explicit user scheme"
        };
      } else {
        const encodingVLType = cs.type;
        const colorHint = resolveColorSchemeHint(semanticType, annotation, fieldValues);
        const uniqueValues = [...new Set(fieldValues)];
        cs.colorScheme = getRecommendedColorScheme(
          semanticType,
          encodingVLType,
          uniqueValues.length,
          fieldName,
          fieldValues,
          { type: colorHint.type }
        );
        if (cs.colorScheme.type === "diverging" && encodingVLType === "quantitative") {
          const nums = fieldValues.filter((v) => typeof v === "number" && !isNaN(v));
          const divInfo = resolveDivergingInfo(semanticType, annotation, nums);
          if (divInfo) {
            cs.colorScheme.domainMid = divInfo.midpoint;
          }
        }
      }
    }
    if (cs.type === "temporal" || semanticType && getVisCategory(semanticType) === "temporal") {
      const convertedFieldValues = temporalData.map((r) => r[fieldName]);
      const fmt = resolveTemporalFormat(convertedFieldValues, semanticType);
      if (fmt) cs.temporalFormat = fmt;
    }
    if (cs.type === "ordinal" || cs.type === "nominal") {
      if (!encoding.sortOrder && !encoding.sortBy) {
        const ordinalSort = inferOrdinalSortOrder(semanticType, fieldValues);
        if (ordinalSort) {
          cs.ordinalSortOrder = ordinalSort;
        }
      }
    }
    result[channel] = cs;
  }
  return result;
}

// src/core/filter-overflow.ts
function filterOverflow(channelSemantics, declaration, encodings, data, budgets, allMarkTypes) {
  const effectiveType = (ch) => declaration.resolvedTypes?.[ch] ?? channelSemantics[ch]?.type;
  const effectiveField = (ch) => {
    if (channelSemantics[ch]?.field) return channelSemantics[ch].field;
    return void 0;
  };
  const isDiscreteType = (t) => t === "nominal" || t === "ordinal";
  const nominalCounts = {
    x: 0,
    y: 0,
    column: 0,
    row: 0,
    group: 0
  };
  const truncations = [];
  const warnings = [];
  let filteredData = data;
  const groupField = channelSemantics.group?.field;
  if (groupField) {
    nominalCounts.group = new Set(data.map((r) => r[groupField])).size;
  }
  const strategyContext = {
    data,
    channelSemantics,
    encodings,
    allMarkTypes
  };
  const strategy = declaration.overflowStrategy ?? defaultOverflowStrategy;
  for (const channel of ["x", "y", "column", "row", "color"]) {
    const fieldName = effectiveField(channel);
    const type = effectiveType(channel);
    if (!fieldName) continue;
    const maxToKeep = budgets.maxValues[channel] ?? Infinity;
    if (!isDiscreteType(type)) {
      if (channel === "column" || channel === "row") {
        const uniqueValues2 = [...new Set(filteredData.map((r) => r[fieldName]))];
        nominalCounts[channel] = Math.min(uniqueValues2.length, maxToKeep);
        if (uniqueValues2.length > maxToKeep) {
          const sorted = [...uniqueValues2].sort();
          const valuesToKeep = sorted.slice(0, maxToKeep);
          const omittedCount = uniqueValues2.length - valuesToKeep.length;
          warnings.push({
            severity: "warning",
            code: "overflow",
            message: `${omittedCount} of ${uniqueValues2.length} values in '${fieldName}' were omitted (showing first ${valuesToKeep.length}).`,
            channel,
            field: fieldName
          });
          const keepSet = new Set(valuesToKeep);
          filteredData = filteredData.filter((row) => keepSet.has(row[fieldName]));
        }
      }
      continue;
    }
    const uniqueValues = [...new Set(filteredData.map((r) => r[fieldName]))];
    nominalCounts[channel] = Math.min(uniqueValues.length, maxToKeep);
    if (uniqueValues.length > maxToKeep) {
      const valuesToKeep = strategy(channel, fieldName, uniqueValues, maxToKeep, strategyContext);
      const omittedCount = uniqueValues.length - valuesToKeep.length;
      const placeholder = `...${omittedCount} items omitted`;
      warnings.push({
        severity: "warning",
        code: "overflow",
        message: `${omittedCount} of ${uniqueValues.length} values in '${fieldName}' were omitted (showing first ${valuesToKeep.length} in sort order).`,
        channel,
        field: fieldName
      });
      truncations.push({
        severity: "warning",
        code: "overflow",
        message: `${omittedCount} of ${uniqueValues.length} values in '${fieldName}' were omitted (showing first ${valuesToKeep.length} in sort order).`,
        channel,
        field: fieldName,
        keptValues: valuesToKeep,
        omittedCount,
        placeholder
      });
      if (channel !== "color") {
        filteredData = filteredData.filter((row) => valuesToKeep.includes(row[fieldName]));
      }
    }
  }
  return { filteredData, nominalCounts, truncations, warnings };
}
var defaultOverflowStrategy = (channel, fieldName, uniqueValues, maxToKeep, context) => {
  const { data, channelSemantics, encodings, allMarkTypes } = context;
  const encoding = encodings[channel];
  const sortBy = encoding?.sortBy;
  const sortOrder = encoding?.sortOrder;
  let sortField;
  let sortFieldType;
  let isDescending = false;
  if (sortBy) {
    if (sortBy === "x" || sortBy === "y" || sortBy === "color") {
      const sortCS = channelSemantics[sortBy];
      sortField = sortCS?.field;
      sortFieldType = sortCS?.type;
      isDescending = sortOrder === "descending" || sortOrder !== "ascending" && sortBy !== channel;
    } else {
      try {
        const sortedList = JSON.parse(sortBy);
        if (Array.isArray(sortedList)) {
          const orderedValues = sortOrder === "descending" ? sortedList.reverse() : sortedList;
          return orderedValues.filter((v) => uniqueValues.includes(v)).slice(0, maxToKeep);
        }
      } catch {
      }
      isDescending = sortOrder === "descending";
    }
  }
  if (sortField && sortFieldType === "quantitative") {
    let aggregateOp = Math.max;
    let initialValue = -Infinity;
    if (allMarkTypes.has("bar") && sortField !== channelSemantics.color?.field) {
      aggregateOp = (x, y) => x + y;
      initialValue = 0;
    }
    const valueAggregates = /* @__PURE__ */ new Map();
    for (const row of data) {
      const fieldValue = row[fieldName];
      const sortValue = Number(row[sortField] ?? 0);
      if (valueAggregates.has(fieldValue)) {
        valueAggregates.set(fieldValue, aggregateOp(valueAggregates.get(fieldValue), sortValue));
      } else {
        valueAggregates.set(fieldValue, aggregateOp(initialValue, sortValue));
      }
    }
    return Array.from(valueAggregates.entries()).map(([value, agg]) => ({ value, agg })).sort((a, b) => isDescending ? b.agg - a.agg : a.agg - b.agg).slice(0, maxToKeep).map((v) => v.value);
  }
  const canonicalOrder = channelSemantics[channel]?.ordinalSortOrder;
  if (!sortBy && !sortOrder && canonicalOrder?.length) {
    const present = new Set(uniqueValues);
    const ordered = canonicalOrder.filter((value) => present.has(value));
    const canonicalValues = new Set(ordered);
    ordered.push(...uniqueValues.filter((value) => !canonicalValues.has(value)));
    return ordered.slice(0, maxToKeep);
  }
  const fieldOriginalType = inferVisCategory(data.map((r) => r[fieldName]));
  if (fieldOriginalType === "quantitative" || channel === "color") {
    return [...uniqueValues].sort((a, b) => Number(a) - Number(b)).slice(0, maxToKeep);
  }
  if (channel === "column" || channel === "row") {
    return uniqueValues.slice(0, maxToKeep);
  }
  if (sortOrder === "descending") {
    return [...uniqueValues].sort((a, b) => String(b).localeCompare(String(a), void 0, { numeric: true })).slice(0, maxToKeep);
  }
  if (sortOrder === "ascending") {
    return [...uniqueValues].sort((a, b) => String(a).localeCompare(String(b), void 0, { numeric: true })).slice(0, maxToKeep);
  }
  return uniqueValues.slice(0, maxToKeep);
};

// src/core/compute-layout.ts
var VL_SHORT_DISCRETE_CATEGORY_COUNT = 4;
var VL_SHORT_DISCRETE_LABEL_MAX_LEN = 8;
var APPROX_CHAR_WIDTH_RATIO = 0.62;
var SPARSE_FIT_BAND_CEILING = 100;
function computeDiscreteLabelStats(field, table) {
  if (!field) return null;
  const uniques = /* @__PURE__ */ new Set();
  for (const row of table) {
    const v = row[field];
    if (v == null || v === "") continue;
    uniques.add(String(v));
  }
  if (uniques.size === 0) return null;
  const labels = [...uniques];
  return {
    count: labels.length,
    maxLen: Math.max(...labels.map((s) => s.length)),
    allNumeric: labels.every((s) => s.trim() !== "" && isFinite(Number(s)))
  };
}
function discreteYAxisShouldUseHorizontalLabels(field, channelType, table) {
  if (!field) return false;
  if (channelType === "quantitative") return true;
  const stats = computeDiscreteLabelStats(field, table);
  if (!stats) return false;
  if (stats.count > VL_SHORT_DISCRETE_CATEGORY_COUNT) return false;
  return stats.maxLen <= VL_SHORT_DISCRETE_LABEL_MAX_LEN;
}
function resolveStretchCaps(options) {
  const def = options.maxStretch ?? DEFAULT_MAX_STRETCH;
  return {
    x: Math.max(1, options.maxStretchX ?? def),
    y: Math.max(1, options.maxStretchY ?? def)
  };
}
var DEFAULT_BASE_SIZE = { width: 400, height: 320 };
var DEFAULT_MAX_STRETCH = 1.5;
function resolveBaseSize(specBaseSize, ceiling) {
  const base = specBaseSize ?? { ...DEFAULT_BASE_SIZE };
  if (!ceiling) return { width: base.width, height: base.height };
  return {
    width: Math.min(base.width, ceiling.width),
    height: Math.min(base.height, ceiling.height)
  };
}
function resolveFacetColumnsOption(chartProperties) {
  const raw = chartProperties?.facetColumns;
  if (raw == null) return void 0;
  const n = Number(raw);
  return Number.isFinite(n) && n >= 1 ? Math.floor(n) : void 0;
}
function deriveStretchCaps(baseSize, ceiling, options) {
  const def = options.maxStretch ?? DEFAULT_MAX_STRETCH;
  return {
    maxStretchX: ceiling ? Math.max(1, ceiling.width / baseSize.width) : def,
    maxStretchY: ceiling ? Math.max(1, ceiling.height / baseSize.height) : def
  };
}
var CELL_BAND_SIZE = 28;
var SQUARE_CELL_TOLERANCE = 1.5;
function computeLayout(channelSemantics, declaration, table, canvasSize, options = {}, facetGrid) {
  const {
    elasticity: elasticityVal = 0.5,
    facetElasticity: facetElasticityVal = 0.3,
    minStep: minStepVal = 6,
    minSubplotSize: minSubplotVal = 60,
    stepPadding: stepPaddingVal = 0.1,
    bandStepFit: bandStepFitVal = 0,
    maintainContinuousAxisRatio = false,
    continuousMarkCrossSection,
    facetAspectRatioResistance = 0
  } = options;
  const { x: maxStretchX, y: maxStretchY } = resolveStretchCaps(options);
  const defaultChartWidth = canvasSize.width;
  const defaultChartHeight = canvasSize.height;
  const fixW = options.facetFixedPadding?.width ?? 0;
  const fixH = options.facetFixedPadding?.height ?? 0;
  const gap = options.facetGap ?? 0;
  const baseRefSize = 300;
  const sizeRatio = Math.max(defaultChartWidth, defaultChartHeight) / baseRefSize;
  const baseBandSize = options.defaultBandSize ?? 20;
  const defaultStepSize = Math.round(baseBandSize * Math.max(1, sizeRatio));
  const isDiscreteType = (t) => t === "nominal" || t === "ordinal";
  const effectiveTypes = {};
  for (const [ch, cs] of Object.entries(channelSemantics)) {
    effectiveTypes[ch] = declaration.resolvedTypes?.[ch] || cs.type;
  }
  const axisFlags = declaration.axisFlags || {};
  const xBanded = axisFlags.x?.banded ?? false;
  const yBanded = axisFlags.y?.banded ?? false;
  const effectiveBandStepFit = xBanded && yBanded ? 0 : bandStepFitVal;
  const maxBandSize = options.maxBandSize == null ? effectiveBandStepFit > 0 ? Math.max(baseBandSize, SPARSE_FIT_BAND_CEILING) : baseBandSize : Math.max(baseBandSize, options.maxBandSize);
  const maxStepSize = options.maxBandSize == null && effectiveBandStepFit > 0 ? Math.max(defaultStepSize, SPARSE_FIT_BAND_CEILING) : Math.round(maxBandSize * Math.max(1, sizeRatio));
  const nominalCount = {
    x: 0,
    y: 0,
    column: 0,
    row: 0,
    group: 0
  };
  for (const channel of ["x", "y", "column", "row", "color"]) {
    const cs = channelSemantics[channel];
    if (!cs?.field) continue;
    const effectiveType = effectiveTypes[channel] || cs.type;
    if (!isDiscreteType(effectiveType)) continue;
    const uniqueValues = [...new Set(table.map((r) => r[cs.field]))];
    nominalCount[channel] = uniqueValues.length;
  }
  let groupField = channelSemantics.group?.field;
  if (!groupField && declaration.colorActsAsGroup) {
    const colorCS = channelSemantics.color;
    const colorType = effectiveTypes.color ?? colorCS?.type;
    const axisField = isDiscreteType(effectiveTypes.x ?? channelSemantics.x?.type) ? channelSemantics.x?.field : channelSemantics.y?.field;
    if (colorCS?.field && isDiscreteType(colorType) && colorCS.field !== axisField) {
      groupField = colorCS.field;
    }
  }
  if (groupField) {
    const groupAxisField = isDiscreteType(effectiveTypes.x ?? channelSemantics.x?.type) ? channelSemantics.x?.field : channelSemantics.y?.field;
    if (groupAxisField === groupField) {
      groupField = void 0;
    } else if (groupAxisField && planBandDodge(table, groupAxisField, groupField).maxPerBand <= 1) {
      groupField = void 0;
    }
  }
  let groupAxis;
  if (groupField) {
    nominalCount.group = declaration.groupLaneCount ?? new Set(table.map((r) => r[groupField])).size;
    if (isDiscreteType(effectiveTypes.x ?? channelSemantics.x?.type)) groupAxis = "x";
    else if (isDiscreteType(effectiveTypes.y ?? channelSemantics.y?.type)) groupAxis = "y";
  }
  const xGroupMultiplier = groupAxis === "x" && nominalCount.group > 1 ? nominalCount.group : 1;
  const yGroupMultiplier = groupAxis === "y" && nominalCount.group > 1 ? nominalCount.group : 1;
  const xTotalNominalCount = nominalCount.x * xGroupMultiplier;
  const yTotalNominalCount = nominalCount.y * yGroupMultiplier;
  const MIN_GROUP_GAP_PX = 3;
  let xContinuousAsDiscrete = 0;
  let yContinuousAsDiscrete = 0;
  for (const axis of ["x", "y"]) {
    const cs = channelSemantics[axis];
    if (!cs?.field) continue;
    const effectiveType = effectiveTypes[axis] || cs.type;
    if (isDiscreteType(effectiveType)) continue;
    const isBanded = axis === "x" ? xBanded : yBanded;
    const isBinned = declaration.binnedAxes?.[axis];
    if (!isBanded && !isBinned) continue;
    let count;
    if (isBinned) {
      const binDef = declaration.binnedAxes[axis];
      count = typeof binDef === "object" && binDef.maxbins ? binDef.maxbins : 10;
    } else {
      count = new Set(table.map((r) => r[cs.field])).size;
    }
    if (count <= 1) continue;
    if (axis === "x") {
      xContinuousAsDiscrete = count;
    } else {
      yContinuousAsDiscrete = count;
    }
  }
  let facetCols = 1;
  let facetRows = 1;
  if (facetGrid) {
    facetCols = facetGrid.columns;
    facetRows = facetGrid.rows;
  } else {
    if (nominalCount.column > 0) facetCols = nominalCount.column;
    if (nominalCount.row > 0) facetRows = nominalCount.row;
  }
  const LOG_PX_PER_DECADE = 40;
  let logBoostX = 0;
  let logBoostY = 0;
  for (const axis of ["x", "y"]) {
    const cs = channelSemantics[axis];
    if (!cs?.field || !cs.scaleType) continue;
    if (cs.scaleType !== "log" && cs.scaleType !== "symlog") continue;
    const vals = table.map((r) => r[cs.field]).filter((v) => typeof v === "number" && v > 0 && isFinite(v));
    if (vals.length < 2) continue;
    const decades = Math.log10(Math.max(...vals)) - Math.log10(Math.min(...vals));
    const needed = Math.ceil(Math.max(1, decades)) * LOG_PX_PER_DECADE;
    if (axis === "x") logBoostX = needed;
    else logBoostY = needed;
  }
  const minContinuousSize = Math.max(10, minStepVal);
  const minContinuousSizeX = Math.max(minContinuousSize, logBoostX);
  const minContinuousSizeY = Math.max(minContinuousSize, logBoostY);
  let subplotWidth;
  if (facetCols > 1) {
    const stretch = Math.min(maxStretchX, Math.pow(facetCols, facetElasticityVal));
    subplotWidth = Math.round(Math.max(
      minContinuousSizeX,
      (defaultChartWidth * stretch - fixW) / facetCols - gap
    ));
  } else {
    subplotWidth = defaultChartWidth;
  }
  let subplotHeight;
  if (facetRows > 1) {
    const stretch = Math.min(maxStretchY, Math.pow(facetRows, facetElasticityVal));
    subplotHeight = Math.round(Math.max(
      minContinuousSizeY,
      (defaultChartHeight * stretch - fixH) / facetRows - gap
    ));
  } else {
    subplotHeight = defaultChartHeight;
  }
  const xIsContinuousNonBanded = xTotalNominalCount === 0 && xContinuousAsDiscrete === 0;
  const yIsContinuousNonBanded = yTotalNominalCount === 0 && yContinuousAsDiscrete === 0;
  const bothContinuousNonBanded = xIsContinuousNonBanded && yIsContinuousNonBanded;
  if (facetAspectRatioResistance > 0 && !bothContinuousNonBanded && (facetCols > 1 || facetRows > 1)) {
    const baseAR = defaultChartWidth / defaultChartHeight;
    const facetAR = subplotWidth / subplotHeight;
    const arDrift = facetAR / baseAR;
    if (arDrift < 1) {
      subplotHeight = Math.round(
        Math.max(minContinuousSizeY, subplotHeight * Math.pow(arDrift, facetAspectRatioResistance))
      );
    } else if (arDrift > 1) {
      subplotWidth = Math.round(
        Math.max(minContinuousSizeX, subplotWidth * Math.pow(1 / arDrift, facetAspectRatioResistance))
      );
    }
  }
  if (bothContinuousNonBanded) {
    const xCS = channelSemantics.x;
    const yCS = channelSemantics.y;
    if (xCS?.field && yCS?.field) {
      const isTempX = (effectiveTypes.x || xCS.type) === "temporal";
      const isTempY = (effectiveTypes.y || yCS.type) === "temporal";
      const xNumeric = [];
      const yNumeric = [];
      for (const row of table) {
        let xv = row[xCS.field];
        let yv = row[yCS.field];
        if (xv == null || yv == null) continue;
        if (isTempX) xv = +new Date(xv);
        else xv = +xv;
        if (isTempY) yv = +new Date(yv);
        else yv = +yv;
        if (isNaN(xv) || isNaN(yv)) continue;
        xNumeric.push(xv);
        yNumeric.push(yv);
      }
      if (xNumeric.length > 1) {
        const xMin = Math.min(...xNumeric);
        const xMax = Math.max(...xNumeric);
        const yMin = Math.min(...yNumeric);
        const yMax = Math.max(...yNumeric);
        const xDomain = [xMin, xMax];
        const yDomain = [yMin, yMax];
        if (xCS.zero?.zero) {
          if (xDomain[0] > 0) xDomain[0] = 0;
          if (xDomain[1] < 0) xDomain[1] = 0;
        }
        if (yCS.zero?.zero) {
          if (yDomain[0] > 0) yDomain[0] = 0;
          if (yDomain[1] < 0) yDomain[1] = 0;
        }
        const xDataCoverage = xDomain[1] - xDomain[0] > 0 ? (xMax - xMin) / (xDomain[1] - xDomain[0]) : 1;
        const yDataCoverage = yDomain[1] - yDomain[0] > 0 ? (yMax - yMin) / (yDomain[1] - yDomain[0]) : 1;
        const BANKING_COVERAGE_THRESHOLD = 0.2;
        let gasPressureParams = DEFAULT_GAS_PRESSURE_PARAMS;
        if (continuousMarkCrossSection != null) {
          if (typeof continuousMarkCrossSection === "number") {
            gasPressureParams = { ...DEFAULT_GAS_PRESSURE_PARAMS, markCrossSection: continuousMarkCrossSection };
          } else {
            const maxCS = Math.max(continuousMarkCrossSection.x, continuousMarkCrossSection.y);
            gasPressureParams = {
              ...DEFAULT_GAS_PRESSURE_PARAMS,
              markCrossSection: maxCS,
              markCrossSectionX: continuousMarkCrossSection.x,
              markCrossSectionY: continuousMarkCrossSection.y,
              ...continuousMarkCrossSection.elasticity != null && { elasticity: continuousMarkCrossSection.elasticity },
              ...continuousMarkCrossSection.maxStretch != null && { maxStretch: continuousMarkCrossSection.maxStretch }
            };
            if (continuousMarkCrossSection.seriesCountAxis) {
              const resolvedAxis = continuousMarkCrossSection.seriesCountAxis === "auto" ? "y" : continuousMarkCrossSection.seriesCountAxis;
              const nSeries = countDistinctSeries(channelSemantics, table);
              if (resolvedAxis === "y") {
                gasPressureParams.yItemCountOverride = nSeries;
              } else {
                gasPressureParams.xItemCountOverride = nSeries;
              }
            }
          }
        }
        const perSubplotCanvasW = facetCols > 1 ? Math.max(
          minContinuousSizeX,
          (defaultChartWidth * Math.min(maxStretchX, Math.pow(facetCols, facetElasticityVal)) - fixW) / facetCols - gap
        ) : defaultChartWidth;
        const perSubplotCanvasH = facetRows > 1 ? Math.max(
          minContinuousSizeY,
          (defaultChartHeight * Math.min(maxStretchY, Math.pow(facetRows, facetElasticityVal)) - fixH) / facetRows - gap
        ) : defaultChartHeight;
        const idealResult = computeGasPressure(
          xNumeric,
          yNumeric,
          xDomain,
          yDomain,
          perSubplotCanvasW,
          perSubplotCanvasH,
          gasPressureParams
        );
        const isConnected = typeof continuousMarkCrossSection === "object" && !!continuousMarkCrossSection.seriesCountAxis;
        const useBanking = xDataCoverage >= BANKING_COVERAGE_THRESHOLD && yDataCoverage >= BANKING_COVERAGE_THRESHOLD;
        let idealW;
        let idealH;
        const rawW = perSubplotCanvasW * idealResult.rawStretchX;
        const rawH = perSubplotCanvasH * idealResult.rawStretchY;
        if (useBanking) {
          const seriesFields = [];
          const colorField = channelSemantics.color?.field;
          const detailField = channelSemantics.detail?.field;
          if (colorField) seriesFields.push(colorField);
          if (detailField && detailField !== colorField) seriesFields.push(detailField);
          const perPointSeriesKeys = new Array(xNumeric.length);
          if (seriesFields.length === 0) {
            perPointSeriesKeys.fill("");
          } else {
            let idx = 0;
            for (const row of table) {
              const xv = xCS?.field ? row[xCS.field] : void 0;
              const yv = yCS?.field ? row[yCS.field] : void 0;
              if (xv == null || yv == null) continue;
              const xn = isTempX ? +new Date(xv) : +xv;
              const yn = isTempY ? +new Date(yv) : +yv;
              if (isNaN(xn) || isNaN(yn)) continue;
              perPointSeriesKeys[idx++] = seriesFields.map((f) => String(row[f] ?? "")).join("\0");
            }
          }
          const bankingAR = computeBankingAR(
            xNumeric,
            yNumeric,
            xDomain,
            yDomain,
            perPointSeriesKeys,
            isConnected
          );
          const BANKING_BLEND = 0.5;
          const gasAR = rawW / rawH;
          const blendedAR = gasAR > 0 && bankingAR > 0 ? Math.exp((1 - BANKING_BLEND) * Math.log(gasAR) + BANKING_BLEND * Math.log(bankingAR)) : bankingAR;
          const rawArea = rawW * rawH;
          const maxArea = perSubplotCanvasW * perSubplotCanvasH * Math.max(maxStretchX, maxStretchY);
          const area = Math.min(rawArea, maxArea);
          idealW = Math.sqrt(area * blendedAR);
          idealH = Math.sqrt(area / blendedAR);
        } else {
          idealW = rawW;
          idealH = rawH;
        }
        const availW = facetCols > 1 ? Math.max(minContinuousSizeX, (defaultChartWidth * maxStretchX - fixW) / facetCols - gap) : defaultChartWidth * maxStretchX;
        const availH = facetRows > 1 ? Math.max(minContinuousSizeY, (defaultChartHeight * maxStretchY - fixH) / facetRows - gap) : defaultChartHeight * maxStretchY;
        const scaleX = idealW > availW ? availW / idealW : 1;
        const scaleY = idealH > availH ? availH / idealH : 1;
        const fitScale = Math.min(scaleX, scaleY);
        let finalW = idealW * fitScale;
        let finalH = idealH * fitScale;
        finalW = Math.max(finalW, minContinuousSizeX);
        finalH = Math.max(finalH, minContinuousSizeY);
        subplotWidth = Math.round(finalW);
        subplotHeight = Math.round(finalH);
      }
    }
  } else if (xIsContinuousNonBanded || yIsContinuousNonBanded) {
    const contAxis = xIsContinuousNonBanded ? "x" : "y";
    const otherAxisHasDiscreteItems = contAxis === "x" ? yTotalNominalCount > 0 || yContinuousAsDiscrete > 0 : xTotalNominalCount > 0 || xContinuousAsDiscrete > 0;
    let seriesStretchApplied = false;
    if (typeof continuousMarkCrossSection === "object" && continuousMarkCrossSection.seriesCountAxis) {
      const resolvedAxis = continuousMarkCrossSection.seriesCountAxis === "auto" ? contAxis : continuousMarkCrossSection.seriesCountAxis;
      if (resolvedAxis === contAxis) {
        const sigmaPerSeries = contAxis === "x" ? continuousMarkCrossSection.x : continuousMarkCrossSection.y;
        const baseDim = contAxis === "x" ? subplotWidth : subplotHeight;
        const nSeries = countDistinctSeries(channelSemantics, table);
        const pressure = nSeries * sigmaPerSeries / baseDim;
        const elast = continuousMarkCrossSection.elasticity ?? DEFAULT_GAS_PRESSURE_PARAMS.elasticity;
        const maxS = continuousMarkCrossSection.maxStretch ?? DEFAULT_GAS_PRESSURE_PARAMS.maxStretch;
        if (pressure > 1) {
          const stretch = Math.min(maxS, Math.pow(pressure, elast));
          if (contAxis === "x") {
            subplotWidth = Math.round(subplotWidth * stretch);
          } else {
            subplotHeight = Math.round(subplotHeight * stretch);
          }
        }
        seriesStretchApplied = true;
      }
    }
    if (!seriesStretchApplied && !otherAxisHasDiscreteItems) {
      const contCS = channelSemantics[contAxis];
      if (contCS?.field) {
        const isTemporal2 = (effectiveTypes[contAxis] || contCS.type) === "temporal";
        const contValues = [];
        for (const row of table) {
          let v = row[contCS.field];
          if (v == null) continue;
          if (isTemporal2) v = +new Date(v);
          else v = +v;
          if (!isNaN(v)) contValues.push(v);
        }
        const sigma1d = Math.sqrt(DEFAULT_GAS_PRESSURE_PARAMS.markCrossSection);
        const baseDim = contAxis === "x" ? subplotWidth : subplotHeight;
        const pressure1d = contValues.length * sigma1d / baseDim;
        if (pressure1d > 1) {
          const stretch1d = Math.min(
            DEFAULT_GAS_PRESSURE_PARAMS.maxStretch,
            Math.pow(pressure1d, DEFAULT_GAS_PRESSURE_PARAMS.elasticity)
          );
          if (contAxis === "x") {
            subplotWidth = Math.round(subplotWidth * stretch1d);
          } else {
            subplotHeight = Math.round(subplotHeight * stretch1d);
          }
        }
      }
    }
  }
  const elasticParamsX = {
    elasticity: elasticityVal,
    maxStretch: maxStretchX,
    defaultStepSize,
    bandStepFit: effectiveBandStepFit,
    bandStepFitCapacity: Math.max(
      1,
      (options.bandStepFitCapacityX ?? defaultChartWidth) / defaultChartWidth
    ),
    minStep: minStepVal
  };
  const elasticParamsY = {
    elasticity: elasticityVal,
    maxStretch: maxStretchY,
    defaultStepSize,
    bandStepFit: effectiveBandStepFit,
    bandStepFitCapacity: Math.max(
      1,
      (options.bandStepFitCapacityY ?? defaultChartHeight) / defaultChartHeight
    ),
    minStep: minStepVal
  };
  const xAxis = computeAxisStep(xTotalNominalCount, xContinuousAsDiscrete, subplotWidth, elasticParamsX);
  const yAxis = computeAxisStep(yTotalNominalCount, yContinuousAsDiscrete, subplotHeight, elasticParamsY);
  const xIsDiscrete = xTotalNominalCount > 0;
  const yIsDiscrete = yTotalNominalCount > 0;
  const xHasGrouping = groupAxis === "x" && nominalCount.group > 0;
  const yHasGrouping = groupAxis === "y" && nominalCount.group > 0;
  let xStepSize;
  let yStepSize;
  let xStepUnit;
  let yStepUnit;
  if (xIsDiscrete && xHasGrouping) {
    const itemsPerGroup = nominalCount.group;
    const defaultGroupStep = itemsPerGroup * maxStepSize;
    const minGroupStep = Math.max(Math.ceil(MIN_GROUP_GAP_PX / stepPaddingVal), 2 * itemsPerGroup);
    const groupElasticX = {
      ...elasticParamsX,
      defaultStepSize: elasticParamsX.defaultStepSize * itemsPerGroup
    };
    const groupAxis2 = computeAxisStep(nominalCount.x, 0, subplotWidth, groupElasticX);
    const groupStep = Math.max(minGroupStep, Math.min(defaultGroupStep, groupAxis2.step));
    xStepSize = groupStep;
    xStepUnit = "group";
  } else if (xIsDiscrete) {
    xStepSize = Math.max(minStepVal, Math.min(maxStepSize, xAxis.step));
  } else if (xContinuousAsDiscrete > 0) {
    xStepSize = Math.max(minStepVal, Math.min(maxStepSize, xAxis.step));
  } else {
    xStepSize = defaultStepSize;
  }
  if (yIsDiscrete && yHasGrouping) {
    const itemsPerGroup = nominalCount.group;
    const defaultGroupStep = itemsPerGroup * maxStepSize;
    const minGroupStep = Math.max(Math.ceil(MIN_GROUP_GAP_PX / stepPaddingVal), 2 * itemsPerGroup);
    const groupElasticY = {
      ...elasticParamsY,
      defaultStepSize: elasticParamsY.defaultStepSize * itemsPerGroup
    };
    const groupAxis2 = computeAxisStep(nominalCount.y, 0, subplotHeight, groupElasticY);
    const groupStep = Math.max(minGroupStep, Math.min(defaultGroupStep, groupAxis2.step));
    yStepSize = groupStep;
    yStepUnit = "group";
  } else if (yIsDiscrete) {
    yStepSize = Math.max(minStepVal, Math.min(maxStepSize, yAxis.step));
  } else if (yContinuousAsDiscrete > 0) {
    yStepSize = Math.max(minStepVal, Math.min(maxStepSize, yAxis.step));
  } else {
    yStepSize = defaultStepSize;
  }
  for (const axis of ["x", "y"]) {
    const count = axis === "x" ? xContinuousAsDiscrete : yContinuousAsDiscrete;
    if (count <= 0) continue;
    const stepSize = axis === "x" ? xStepSize : yStepSize;
    const continuousSize = Math.round(stepSize * (count + 1));
    if (axis === "x") {
      subplotWidth = continuousSize;
    } else {
      subplotHeight = continuousSize;
    }
  }
  const maxSubplotW = (defaultChartWidth * maxStretchX - fixW) / facetCols - gap;
  const maxSubplotH = (defaultChartHeight * maxStretchY - fixH) / facetRows - gap;
  if (xTotalNominalCount > 0) {
    const divisor = xStepUnit === "group" ? nominalCount.x : xTotalNominalCount;
    const cap = Math.max(minStepVal, Math.floor(maxSubplotW / divisor));
    if (xStepSize > cap) xStepSize = cap;
  }
  if (xContinuousAsDiscrete > 0) {
    const cap = Math.max(minStepVal, Math.floor(maxSubplotW / (xContinuousAsDiscrete + 1)));
    if (xStepSize > cap) xStepSize = cap;
  }
  if (yTotalNominalCount > 0) {
    const divisor = yStepUnit === "group" ? nominalCount.y : yTotalNominalCount;
    const cap = Math.max(minStepVal, Math.floor(maxSubplotH / divisor));
    if (yStepSize > cap) yStepSize = cap;
  }
  if (yContinuousAsDiscrete > 0) {
    const cap = Math.max(minStepVal, Math.floor(maxSubplotH / (yContinuousAsDiscrete + 1)));
    if (yStepSize > cap) yStepSize = cap;
  }
  for (const axis of ["x", "y"]) {
    const count = axis === "x" ? xContinuousAsDiscrete : yContinuousAsDiscrete;
    if (count <= 0) continue;
    const stepSize = axis === "x" ? xStepSize : yStepSize;
    if (axis === "x") subplotWidth = Math.round(stepSize * (count + 1));
    else subplotHeight = Math.round(stepSize * (count + 1));
  }
  const isConnectedMark = typeof continuousMarkCrossSection === "object" && !!continuousMarkCrossSection.seriesCountAxis;
  const bothDiscreteConnected = isConnectedMark && xTotalNominalCount > 0 && yTotalNominalCount > 0 && !xHasGrouping && !yHasGrouping;
  if (bothDiscreteConnected && typeof continuousMarkCrossSection === "object") {
    const csX = continuousMarkCrossSection.x ?? 0;
    const csY = continuousMarkCrossSection.y ?? 0;
    if (csX > 0 && csY > 0) {
      const RUN_AR_CAP = 2;
      const runIsX = csX >= csY;
      const crossCS = runIsX ? csY : csX;
      const runBudget = runIsX ? Math.floor(maxSubplotW / xTotalNominalCount) : Math.floor(maxSubplotH / yTotalNominalCount);
      const cross = Math.max(
        minStepVal,
        Math.min(runIsX ? yStepSize : xStepSize, crossCS)
      );
      const ratio = Math.min(RUN_AR_CAP, Math.max(1, Math.max(csX, csY) / Math.min(csX, csY)));
      const run = Math.max(
        runIsX ? xStepSize : yStepSize,
        Math.min(runBudget, Math.round(cross * ratio))
      );
      if (runIsX) {
        xStepSize = run;
        yStepSize = cross;
      } else {
        yStepSize = run;
        xStepSize = cross;
      }
    }
  }
  if (xTotalNominalCount > 0 && yTotalNominalCount > 0 && !xHasGrouping && !yHasGrouping && !bothDiscreteConnected) {
    const capX = Math.floor(maxSubplotW / xTotalNominalCount);
    const capY = Math.floor(maxSubplotH / yTotalNominalCount);
    const generous = CELL_BAND_SIZE;
    const wanted = Math.max(generous, Math.min(xStepSize, yStepSize));
    const square = Math.min(capX, capY, wanted);
    const widest = Math.max(xStepSize, yStepSize);
    if (square >= minStepVal && square * SQUARE_CELL_TOLERANCE >= widest) {
      xStepSize = square;
      yStepSize = square;
    }
  }
  subplotWidth = Math.min(subplotWidth, Math.round(maxSubplotW));
  subplotHeight = Math.min(subplotHeight, Math.round(maxSubplotH));
  const targetBandAR = options.targetBandAR;
  if (targetBandAR && targetBandAR > 0) {
    const xIsBanded = xTotalNominalCount > 0 || xContinuousAsDiscrete > 0;
    const yIsBanded = yTotalNominalCount > 0 || yContinuousAsDiscrete > 0;
    if (xIsBanded && !yIsBanded) {
      const actualBandAR = subplotHeight / xStepSize;
      if (actualBandAR > targetBandAR) {
        const idealH = xStepSize * targetBandAR;
        const blendedH = Math.exp(
          0.5 * Math.log(subplotHeight) + 0.5 * Math.log(idealH)
        );
        subplotHeight = Math.round(
          Math.max(minContinuousSizeY, Math.min(blendedH, subplotHeight))
        );
      }
    } else if (yIsBanded && !xIsBanded) {
      const actualBandAR = subplotWidth / yStepSize;
      if (actualBandAR > targetBandAR) {
        const idealW = yStepSize * targetBandAR;
        const blendedW = Math.exp(
          0.5 * Math.log(subplotWidth) + 0.5 * Math.log(idealW)
        );
        subplotWidth = Math.round(
          Math.max(minContinuousSizeX, Math.min(blendedW, subplotWidth))
        );
      }
    }
  }
  const xHasDiscreteItems = xTotalNominalCount > 0 || xContinuousAsDiscrete > 0;
  const yHasDiscreteItems = yTotalNominalCount > 0 || yContinuousAsDiscrete > 0;
  const fontSizing = computeFontSizing(Math.min(subplotWidth, subplotHeight), {
    baseLabelFontSize: options.baseLabelFontSize,
    baseTitleFontSize: options.baseTitleFontSize
  });
  const labelOpts = { baseFont: fontSizing.tickBase, minFont: 6 };
  let xLabel = computeLabelSizing(xStepSize, xHasDiscreteItems, labelOpts);
  let yLabel = computeLabelSizing(yStepSize, yHasDiscreteItems, labelOpts);
  if (xHasDiscreteItems) {
    const xf = channelSemantics.x?.field;
    const xt = effectiveTypes.x || channelSemantics.x?.type;
    const stats = computeDiscreteLabelStats(xf, table);
    if (stats) {
      const numericLike = xt === "quantitative" || stats.allNumeric;
      let labelPx = stats.maxLen * xLabel.fontSize * APPROX_CHAR_WIDTH_RATIO;
      const fewShortStrings = !numericLike && stats.count <= VL_SHORT_DISCRETE_CATEGORY_COUNT && stats.maxLen <= VL_SHORT_DISCRETE_LABEL_MAX_LEN;
      if (fewShortStrings || numericLike && labelPx <= xStepSize) {
        if (labelPx > xStepSize) {
          const desiredStep = Math.ceil(labelPx) + 6;
          const cap = Math.max(minStepVal, Math.floor(maxSubplotW / stats.count));
          if (desiredStep <= cap) {
            xStepSize = Math.max(xStepSize, desiredStep);
            xLabel = computeLabelSizing(xStepSize, xHasDiscreteItems, labelOpts);
            labelPx = stats.maxLen * xLabel.fontSize * APPROX_CHAR_WIDTH_RATIO;
          }
        }
        if (labelPx <= xStepSize) {
          xLabel = {
            ...xLabel,
            labelAngle: 0,
            labelAlign: "center",
            labelBaseline: "top"
          };
        } else {
          xLabel = {
            ...xLabel,
            labelAngle: -45,
            labelAlign: "right",
            labelBaseline: "top"
          };
        }
      } else if (numericLike && labelPx > xStepSize && xLabel.labelAngle === void 0) {
        xLabel = {
          ...xLabel,
          labelAngle: -45,
          labelAlign: "right",
          labelBaseline: "top"
        };
      }
    }
  }
  if (yHasDiscreteItems) {
    const yf = channelSemantics.y?.field;
    const yt = effectiveTypes.y || channelSemantics.y?.type;
    if (discreteYAxisShouldUseHorizontalLabels(yf, yt, table)) {
      yLabel = {
        ...yLabel,
        labelAngle: 0,
        labelAlign: "right",
        labelBaseline: "middle"
      };
    }
  }
  const unifiedTickFont = Math.min(xLabel.fontSize, yLabel.fontSize);
  if (xLabel.fontSize !== unifiedTickFont) xLabel = { ...xLabel, fontSize: unifiedTickFont };
  if (yLabel.fontSize !== unifiedTickFont) yLabel = { ...yLabel, fontSize: unifiedTickFont };
  return {
    subplotWidth,
    subplotHeight,
    xStep: xStepSize,
    yStep: yStepSize,
    xStepUnit,
    yStepUnit,
    xContinuousAsDiscrete,
    yContinuousAsDiscrete,
    xNominalCount: xTotalNominalCount,
    yNominalCount: yTotalNominalCount,
    xLabel,
    yLabel,
    titleFontSize: fontSizing.titleFontSize,
    legendFontSize: fontSizing.legendFontSize,
    stepPadding: stepPaddingVal,
    facet: facetCols > 1 || facetRows > 1 ? {
      columns: facetCols,
      rows: facetRows,
      subplotWidth,
      subplotHeight
    } : void 0,
    effectiveFacetGap: gap,
    truncations: []
    // Overflow truncations are handled by filterOverflow
  };
}
function countDistinctSeries(channelSemantics, data) {
  const seriesFields = [];
  const colorField = channelSemantics.color?.field;
  const detailField = channelSemantics.detail?.field;
  if (colorField) seriesFields.push(colorField);
  if (detailField && detailField !== colorField) seriesFields.push(detailField);
  if (seriesFields.length === 0) return 1;
  const seriesKeys = /* @__PURE__ */ new Set();
  for (const row of data) {
    const key = seriesFields.map((f) => String(row[f] ?? "")).join("\0");
    seriesKeys.add(key);
  }
  return seriesKeys.size;
}
function computeBankingAR(xValues, yValues, xDomain, yDomain, seriesKeys, isConnected) {
  const MIN_AR = 0.5;
  const MAX_AR = 3;
  const xRange = xDomain[1] - xDomain[0];
  const yRange = yDomain[1] - yDomain[0];
  if (xRange <= 0 || yRange <= 0) return 1;
  if (!isConnected) {
    const n = xValues.length;
    let sumX = 0, sumY = 0;
    for (let i = 0; i < n; i++) {
      sumX += (xValues[i] - xDomain[0]) / xRange;
      sumY += (yValues[i] - yDomain[0]) / yRange;
    }
    const meanX = sumX / n;
    const meanY = sumY / n;
    let varX = 0, varY = 0;
    for (let i = 0; i < n; i++) {
      const dx = (xValues[i] - xDomain[0]) / xRange - meanX;
      const dy = (yValues[i] - yDomain[0]) / yRange - meanY;
      varX += dx * dx;
      varY += dy * dy;
    }
    const sdX = Math.sqrt(varX / n);
    const sdY = Math.sqrt(varY / n);
    if (sdY <= 0) return MAX_AR;
    if (sdX <= 0) return MIN_AR;
    const sdRatio = sdX / sdY;
    const ar2 = sdRatio > 1 ? 1 + (sdRatio - 1) * 0.3 : 1 - (1 - sdRatio) * 0.3;
    return Math.min(MAX_AR, Math.max(MIN_AR, ar2));
  }
  const seriesMap = /* @__PURE__ */ new Map();
  for (let i = 0; i < xValues.length; i++) {
    const key = seriesKeys[i];
    let arr = seriesMap.get(key);
    if (!arr) {
      arr = [];
      seriesMap.set(key, arr);
    }
    arr.push({ x: xValues[i], y: yValues[i] });
  }
  for (const pts of seriesMap.values()) {
    pts.sort((a, b) => a.x - b.x);
  }
  const scaleMedians = [];
  let maxSeriesLen = 0;
  for (const pts of seriesMap.values()) {
    if (pts.length > maxSeriesLen) maxSeriesLen = pts.length;
  }
  const maxScale = Math.max(0, Math.floor(Math.log2(maxSeriesLen)) - 1);
  for (let scale = 0; scale <= maxScale; scale++) {
    const windowSize = 1 << scale;
    const absSlopes = [];
    for (const pts of seriesMap.values()) {
      const n = pts.length;
      if (n < 2) continue;
      const smoothed = [];
      for (let i = 0; i < n; i += windowSize) {
        const end = Math.min(i + windowSize, n);
        let sx = 0, sy = 0;
        for (let j = i; j < end; j++) {
          sx += pts[j].x;
          sy += pts[j].y;
        }
        const cnt = end - i;
        smoothed.push({ x: sx / cnt, y: sy / cnt });
      }
      for (let i = 1; i < smoothed.length; i++) {
        const dx = (smoothed[i].x - smoothed[i - 1].x) / xRange;
        const dy = (smoothed[i].y - smoothed[i - 1].y) / yRange;
        if (dx === 0) continue;
        absSlopes.push(Math.abs(dy / dx));
      }
    }
    if (absSlopes.length === 0) continue;
    absSlopes.sort((a, b) => a - b);
    const mid = absSlopes.length >> 1;
    const median = absSlopes.length % 2 === 1 ? absSlopes[mid] : (absSlopes[mid - 1] + absSlopes[mid]) / 2;
    if (median > 0) {
      scaleMedians.push(median);
    }
  }
  if (scaleMedians.length === 0) return 1;
  let logSum = 0;
  for (const m of scaleMedians) {
    logSum += Math.log(m);
  }
  const combinedSlope = Math.exp(logSum / scaleMedians.length);
  if (combinedSlope <= 0) return MAX_AR;
  const ar = Math.max(1, combinedSlope);
  return Math.min(MAX_AR, Math.max(MIN_AR, ar));
}
function computeChannelBudgets(channelSemantics, declaration, data, canvasSize, options) {
  const {
    minStep: minStepVal = 6,
    stepPadding: stepPaddingVal = 0.1,
    maxColorValues: maxColorVal = 24
  } = options;
  const { x: maxStretchX, y: maxStretchY } = resolveStretchCaps(options);
  const fixW = options.facetFixedPadding?.width ?? 0;
  const fixH = options.facetFixedPadding?.height ?? 0;
  const gap = options.facetGap ?? 0;
  const isDiscreteType = (t) => t === "nominal" || t === "ordinal";
  const effectiveType = (ch) => declaration.resolvedTypes?.[ch] ?? channelSemantics[ch]?.type;
  const facetGrid = computeFacetGrid(
    channelSemantics,
    declaration,
    data,
    canvasSize,
    options
  );
  const facetCols = facetGrid?.columns ?? 1;
  const facetRows = facetGrid?.rows ?? 1;
  const maxSubplotW = Math.max(
    options.minSubplotSize ?? 60,
    (canvasSize.width * maxStretchX - fixW) / facetCols - gap
  );
  const maxSubplotH = Math.max(
    options.minSubplotSize ?? 60,
    (canvasSize.height * maxStretchY - fixH) / facetRows - gap
  );
  const groupField = channelSemantics.group?.field;
  let groupCount = 0;
  let groupAxis;
  if (groupField) {
    groupCount = new Set(data.map((r) => r[groupField])).size;
    if (isDiscreteType(effectiveType("x"))) groupAxis = "x";
    else if (isDiscreteType(effectiveType("y"))) groupAxis = "y";
  }
  const xGroupMultiplier = groupAxis === "x" && groupCount > 1 ? groupCount : 1;
  const yGroupMultiplier = groupAxis === "y" && groupCount > 1 ? groupCount : 1;
  const MIN_GROUP_GAP_PX = 3;
  const xMinGroupStep = xGroupMultiplier > 1 ? Math.max(Math.ceil(MIN_GROUP_GAP_PX / stepPaddingVal), 2 * xGroupMultiplier) : minStepVal;
  const yMinGroupStep = yGroupMultiplier > 1 ? Math.max(Math.ceil(MIN_GROUP_GAP_PX / stepPaddingVal), 2 * yGroupMultiplier) : minStepVal;
  let maxXToKeep = Math.floor(maxSubplotW / xMinGroupStep);
  let maxYToKeep = Math.floor(maxSubplotH / yMinGroupStep);
  if (facetGrid) {
    const canvasXCap = Math.max(1, Math.floor(canvasSize.width / xMinGroupStep));
    const canvasYCap = Math.max(1, Math.floor(canvasSize.height / yMinGroupStep));
    if (maxXToKeep > canvasXCap || maxYToKeep > canvasYCap) {
      maxXToKeep = Math.min(maxXToKeep, canvasXCap);
      maxYToKeep = Math.min(maxYToKeep, canvasYCap);
      const colField = channelSemantics.column?.field;
      const rowField = channelSemantics.row?.field;
      const colCount = colField ? new Set(data.map((r) => r[colField])).size : 0;
      if (colCount > 1 && !rowField) {
        const tighterW = Math.max(
          options.minSubplotSize ?? 60,
          maxXToKeep * xMinGroupStep
        );
        const totalW = canvasSize.width * maxStretchX - fixW;
        const totalH = canvasSize.height * maxStretchY - fixH;
        const revisedMaxCols = Math.max(1, Math.floor(
          totalW / (tighterW + gap)
        ));
        const revisedMaxRows = Math.max(1, Math.floor(
          totalH / ((options.minSubplotSize ?? 60) + gap)
        ));
        const maxTotal = revisedMaxCols * revisedMaxRows;
        const effectiveCount = Math.min(colCount, maxTotal);
        const visRows = Math.ceil(effectiveCount / revisedMaxCols);
        const visCols = Math.ceil(effectiveCount / visRows);
        facetGrid.columns = visCols;
        facetGrid.rows = visRows;
        facetGrid.maxColumnValues = maxTotal;
      }
    }
  }
  const maxValues = {
    x: maxXToKeep,
    y: maxYToKeep,
    column: facetGrid?.maxColumnValues ?? Infinity,
    row: facetGrid?.maxRowValues ?? Infinity,
    color: maxColorVal
  };
  return { maxValues, facetGrid };
}
function computeFacetGrid(channelSemantics, declaration, data, canvasSize, options) {
  const { x: msX, y: msY } = resolveStretchCaps(options);
  const fixW = options.facetFixedPadding?.width ?? 0;
  const fixH = options.facetFixedPadding?.height ?? 0;
  const gap = options.facetGap ?? 0;
  const minStep = options.minStep ?? 6;
  const stepPadding = options.stepPadding ?? 0.1;
  const baseMinSubplot = options.minSubplotSize ?? 60;
  const isDiscreteType = (t) => t === "nominal" || t === "ordinal";
  const maxW = canvasSize.width * msX - fixW;
  const maxH = canvasSize.height * msY - fixH;
  const MIN_GROUP_GAP_PX = 3;
  const groupField = channelSemantics.group?.field;
  let groupCount = 0;
  let groupAxis;
  if (groupField) {
    groupCount = new Set(data.map((r) => r[groupField])).size;
    const xType = declaration.resolvedTypes?.x ?? channelSemantics.x?.type;
    const yType = declaration.resolvedTypes?.y ?? channelSemantics.y?.type;
    if (isDiscreteType(xType)) groupAxis = "x";
    else if (isDiscreteType(yType)) groupAxis = "y";
  }
  let minSubplotWidth = baseMinSubplot;
  let minSubplotHeight = baseMinSubplot;
  const LOG_PX_PER_DECADE_FACET = 40;
  for (const axis of ["x", "y"]) {
    const cs = channelSemantics[axis];
    if (!cs?.field || !cs.scaleType) continue;
    if (cs.scaleType !== "log" && cs.scaleType !== "symlog") continue;
    const vals = data.map((r) => r[cs.field]).filter((v) => typeof v === "number" && v > 0 && isFinite(v));
    if (vals.length < 2) continue;
    const decades = Math.log10(Math.max(...vals)) - Math.log10(Math.min(...vals));
    const needed = Math.ceil(Math.max(1, decades)) * LOG_PX_PER_DECADE_FACET;
    if (axis === "x") minSubplotWidth = Math.max(minSubplotWidth, needed);
    else minSubplotHeight = Math.max(minSubplotHeight, needed);
  }
  for (const axis of ["x", "y"]) {
    const cs = channelSemantics[axis];
    if (!cs?.field) continue;
    const effectiveType = declaration.resolvedTypes?.[axis] ?? cs.type;
    const isBanded = declaration.axisFlags?.[axis]?.banded === true;
    if (!isDiscreteType(effectiveType) && !isBanded) continue;
    const valueCount = new Set(data.map((r) => r[cs.field])).size;
    const axisGroupCount = groupAxis === axis && groupCount > 1 ? groupCount : 1;
    const maxDim = axis === "x" ? maxW : maxH;
    let perCategoryStep;
    if (axisGroupCount > 1) {
      const minGroupStep = Math.max(
        Math.ceil(MIN_GROUP_GAP_PX / stepPadding),
        2 * axisGroupCount
      );
      perCategoryStep = Math.max(minStep * axisGroupCount, minGroupStep);
    } else {
      perCategoryStep = minStep;
    }
    const dataDrivenMin = Math.min(perCategoryStep * valueCount, maxDim);
    const minDim = Math.max(baseMinSubplot, dataDrivenMin);
    if (axis === "x") {
      minSubplotWidth = minDim;
    } else {
      minSubplotHeight = minDim;
    }
  }
  const xIsCont = (() => {
    const cs = channelSemantics.x;
    if (!cs?.field) return false;
    const t = declaration.resolvedTypes?.x ?? cs.type;
    return !isDiscreteType(t) && !(declaration.axisFlags?.x?.banded === true);
  })();
  const yIsCont = (() => {
    const cs = channelSemantics.y;
    if (!cs?.field) return false;
    const t = declaration.resolvedTypes?.y ?? cs.type;
    return !isDiscreteType(t) && !(declaration.axisFlags?.y?.banded === true);
  })();
  if (xIsCont && yIsCont) {
    const xCS = channelSemantics.x;
    const yCS = channelSemantics.y;
    if (xCS?.field && yCS?.field) {
      const isTempX = (declaration.resolvedTypes?.x ?? xCS.type) === "temporal";
      const isTempY = (declaration.resolvedTypes?.y ?? yCS.type) === "temporal";
      const cmcs = options.continuousMarkCrossSection;
      const isConn = typeof cmcs === "object" && !!cmcs.seriesCountAxis;
      const xNum = [];
      const yNum = [];
      const sKeys = [];
      const sFields = [];
      const colF = channelSemantics.column?.field;
      const rowF = channelSemantics.row?.field;
      if (colF) sFields.push(colF);
      if (rowF) sFields.push(rowF);
      const cf = channelSemantics.color?.field;
      const df = channelSemantics.detail?.field;
      if (cf) sFields.push(cf);
      if (df && df !== cf) sFields.push(df);
      for (const row of data) {
        const xv = row[xCS.field];
        const yv = row[yCS.field];
        if (xv == null || yv == null) continue;
        const xn = isTempX ? +new Date(xv) : +xv;
        const yn = isTempY ? +new Date(yv) : +yv;
        if (isNaN(xn) || isNaN(yn)) continue;
        xNum.push(xn);
        yNum.push(yn);
        sKeys.push(sFields.length > 0 ? sFields.map((f) => String(row[f] ?? "")).join("\0") : "");
      }
      if (xNum.length > 1) {
        const xMin = Math.min(...xNum);
        const xMax = Math.max(...xNum);
        const yMin = Math.min(...yNum);
        const yMax = Math.max(...yNum);
        const xDom = [xMin, xMax];
        const yDom = [yMin, yMax];
        if (xCS.zero?.zero) {
          if (xDom[0] > 0) xDom[0] = 0;
          if (xDom[1] < 0) xDom[1] = 0;
        }
        if (yCS.zero?.zero) {
          if (yDom[0] > 0) yDom[0] = 0;
          if (yDom[1] < 0) yDom[1] = 0;
        }
        const ar = computeBankingAR(xNum, yNum, xDom, yDom, sKeys, isConn);
        if (ar >= 1) {
          minSubplotWidth = Math.max(
            minSubplotWidth,
            Math.round(baseMinSubplot * Math.min(ar, msX))
          );
          minSubplotHeight = Math.max(minSubplotHeight, baseMinSubplot);
        } else {
          minSubplotWidth = Math.max(minSubplotWidth, baseMinSubplot);
          minSubplotHeight = Math.max(
            minSubplotHeight,
            Math.round(baseMinSubplot * Math.min(1 / ar, msY))
          );
        }
      }
    }
  }
  const effectiveW = maxW;
  const effectiveH = maxH;
  const maxFacetColumns = Math.max(1, Math.floor(
    effectiveW / (minSubplotWidth + gap)
  ));
  const maxFacetRows = Math.max(1, Math.floor(
    effectiveH / (minSubplotHeight + gap)
  ));
  const colField = channelSemantics.column?.field;
  const rowField = channelSemantics.row?.field;
  if (!colField && !rowField) return void 0;
  const colCount = colField ? new Set(data.map((r) => r[colField])).size : 0;
  const rowCount = rowField ? new Set(data.map((r) => r[rowField])).size : 0;
  if (colCount === 0 && rowCount === 0) return void 0;
  const forcedCols = options.facetColumns != null && options.facetColumns >= 1 ? Math.min(Math.max(1, Math.floor(options.facetColumns)), Math.max(1, colCount)) : void 0;
  if (colCount > 0 && rowCount === 0) {
    if (forcedCols != null) {
      const nRows2 = Math.ceil(colCount / forcedCols);
      return {
        columns: forcedCols,
        rows: nRows2,
        maxColumnValues: forcedCols * nRows2,
        maxRowValues: Math.max(maxFacetRows, nRows2)
      };
    }
    if (colCount <= maxFacetColumns) {
      return {
        columns: colCount,
        rows: 1,
        maxColumnValues: colCount,
        maxRowValues: maxFacetRows
      };
    }
    let nCols = maxFacetColumns;
    let nRows = Math.ceil(colCount / nCols);
    while (nCols > 2 && colCount % nCols === 1) {
      nCols--;
      nRows = Math.ceil(colCount / nCols);
    }
    const visRows = Math.min(nRows, maxFacetRows);
    const maxTotal = nCols * visRows;
    return {
      columns: nCols,
      rows: visRows,
      maxColumnValues: maxTotal,
      maxRowValues: maxFacetRows
    };
  }
  return {
    columns: Math.max(1, Math.min(colCount, maxFacetColumns)),
    rows: Math.max(1, Math.min(rowCount, maxFacetRows)),
    maxColumnValues: maxFacetColumns,
    maxRowValues: maxFacetRows
  };
}

// src/core/color-decisions.ts
function inferColorChannelPrimary(channel, chartType) {
  if (channel === "color" || channel === "group") return true;
  return false;
}
function decideSchemeTypeFromChannel(channel, cs) {
  const hint = cs?.colorScheme;
  if (hint) {
    if (hint.type === "diverging") {
      return {
        schemeType: "diverging",
        // resolve-semantics 里用 domainMid 表示 diverging 中点
        divergingMidpoint: hint.domainMid
      };
    }
    if (hint.type === "sequential") {
      return { schemeType: "sequential" };
    }
    if (hint.type === "categorical") {
      const semType2 = cs?.semanticAnnotation?.semanticType;
      const isRankLike = semType2 === "Rank";
      if (isRankLike) {
        return { schemeType: "sequential" };
      }
      if (cs?.type === "temporal" && channel === "color") {
        return { schemeType: "sequential" };
      }
      return { schemeType: "categorical" };
    }
  }
  const encType = cs?.type;
  const semType = cs?.semanticAnnotation?.semanticType;
  if (semType === "Correlation") {
    return { schemeType: "diverging", divergingMidpoint: 0 };
  }
  if (encType === "quantitative" || encType === "temporal") {
    return { schemeType: "sequential" };
  }
  return { schemeType: "categorical" };
}
function countDistinctValues(table, field) {
  if (!field) return void 0;
  const set = /* @__PURE__ */ new Set();
  for (const row of table) {
    if (row == null) continue;
    set.add(row[field]);
  }
  return set.size;
}
function decideColorForChannel(channel, ctx) {
  const encoding = ctx.encodings[channel];
  const cs = ctx.channelSemantics[channel];
  if (!encoding || !cs?.field) return void 0;
  const dataDriven = true;
  const primary = inferColorChannelPrimary(channel);
  if (encoding.scheme && encoding.scheme !== "default") {
    const distinct2 = countDistinctValues(ctx.table, cs.field);
    const { schemeType: schemeType2 } = decideSchemeTypeFromChannel(channel, cs);
    return {
      channel,
      schemeType: schemeType2,
      schemeId: encoding.scheme,
      categoryCount: distinct2,
      primary,
      dataDriven
    };
  }
  const { schemeType, divergingMidpoint } = decideSchemeTypeFromChannel(channel, cs);
  const distinct = countDistinctValues(ctx.table, cs.field);
  return {
    channel,
    schemeType,
    divergingMidpoint,
    categoryCount: distinct,
    primary,
    dataDriven
  };
}
function decideColorMaps(ctx) {
  const result = {
    color: void 0,
    group: void 0,
    fill: void 0,
    stroke: void 0
  };
  const channels = ["color", "group"];
  for (const ch of channels) {
    const decision = decideColorForChannel(ch, ctx);
    if (decision) {
      result[ch] = decision;
    }
  }
  return result;
}

// src/chartjs/instantiate-spec.ts
function cjsApplyLayoutToSpec(config, context, warnings) {
  const { channelSemantics, layout, canvasSize } = context;
  const hasAxes = !!(config.options?.scales?.x || config.options?.scales?.y);
  config.type === "radar";
  if (hasAxes && !config._width) {
    const PADDING = 80;
    const xIsDiscrete = layout.xNominalCount > 0 || layout.xContinuousAsDiscrete > 0;
    const yIsDiscrete = layout.yNominalCount > 0 || layout.yContinuousAsDiscrete > 0;
    let plotWidth;
    let plotHeight;
    if (xIsDiscrete && layout.xStepUnit !== "group") {
      const xItemCount = layout.xNominalCount || layout.xContinuousAsDiscrete || 0;
      plotWidth = xItemCount > 0 ? layout.xStep * xItemCount : layout.subplotWidth || canvasSize.width;
    } else {
      plotWidth = layout.subplotWidth || canvasSize.width;
    }
    if (yIsDiscrete && layout.yStepUnit !== "group") {
      const yItemCount = layout.yNominalCount || layout.yContinuousAsDiscrete || 0;
      plotHeight = yItemCount > 0 ? layout.yStep * yItemCount : layout.subplotHeight || canvasSize.height;
    } else {
      plotHeight = layout.subplotHeight || canvasSize.height;
    }
    const legendGutter = cjsLegendLikelyVisible(config) ? 96 : 0;
    config._width = plotWidth + PADDING + legendGutter;
    config._height = plotHeight + PADDING;
  }
  if (config.data?.datasets) {
    const barDatasets = config.data.datasets.filter(
      (ds) => config.type === "bar" || ds.type === "bar"
    );
    if (barDatasets.length > 0 && hasAxes) {
      const bandPadding = layout.stepPadding;
      const categoryPct = 1 - bandPadding;
      for (const ds of barDatasets) {
        if (ds.categoryPercentage == null) {
          ds.categoryPercentage = categoryPct;
        }
      }
    }
  }
  if (hasAxes && config.options?.scales?.x && layout.xLabel) {
    if (!config.options.scales.x.ticks) config.options.scales.x.ticks = {};
    if (layout.xLabel.labelAngle && layout.xLabel.labelAngle !== 0) {
      config.options.scales.x.ticks.maxRotation = Math.abs(layout.xLabel.labelAngle);
      config.options.scales.x.ticks.minRotation = Math.abs(layout.xLabel.labelAngle);
    }
    if (layout.xLabel.fontSize) {
      config.options.scales.x.ticks.font = {
        ...config.options.scales.x.ticks.font || {},
        size: layout.xLabel.fontSize
      };
    }
    if (config.options.scales.x.title?.display) {
      config.options.scales.x.title.font = {
        ...config.options.scales.x.title.font || {},
        size: layout.titleFontSize
      };
    }
  }
  if (hasAxes && config.options?.scales?.y && layout.yLabel) {
    if (!config.options.scales.y.ticks) config.options.scales.y.ticks = {};
    if (layout.yLabel.fontSize) {
      config.options.scales.y.ticks.font = {
        ...config.options.scales.y.ticks.font || {},
        size: layout.yLabel.fontSize
      };
    }
    if (config.options.scales.y.title?.display) {
      config.options.scales.y.title.font = {
        ...config.options.scales.y.title.font || {},
        size: layout.titleFontSize
      };
    }
  }
  if (config.options?.plugins?.legend) {
    const legend = config.options.plugins.legend;
    if (!legend.labels) legend.labels = {};
    legend.labels.font = { ...legend.labels.font || {}, size: layout.legendFontSize };
  }
  if (config.options?.plugins?.title?.display) {
    config.options.plugins.title.font = {
      ...config.options.plugins.title.font || {},
      size: layout.titleFontSize
    };
  }
  if (layout.truncations && layout.truncations.length > 0) {
    for (const trunc of layout.truncations) {
      warnings.push({
        severity: "warning",
        code: "overflow",
        message: trunc.message,
        channel: trunc.channel,
        field: trunc.field
      });
      if (config.data?.labels && Array.isArray(config.data.labels)) {
        config.data.labels.push(trunc.placeholder);
      }
    }
  }
  cjsApplyLegendRightColumn(config);
}
function cjsLegendLikelyVisible(config) {
  const legend = config.options?.plugins?.legend;
  if (legend?.display === false) return false;
  if (legend?.display === true) return true;
  const n = config.data?.datasets?.length ?? 0;
  return n > 1;
}
function cjsLegendEntryCount(config) {
  const t = config.type;
  if (t === "pie" || t === "doughnut") {
    return config.data?.labels?.length ?? 0;
  }
  return config.data?.datasets?.length ?? 0;
}
function cjsApplyLegendRightColumn(config) {
  if (!cjsLegendLikelyVisible(config)) return;
  if (!config.options) config.options = {};
  if (!config.options.plugins) config.options.plugins = {};
  const prev = config.options.plugins.legend ?? {};
  const prevLabels = prev.labels ?? {};
  const prevFont = prevLabels.font ?? {};
  const entryCount = cjsLegendEntryCount(config);
  const highCardinality = entryCount >= 16;
  const fontSize = prevFont.size ?? (highCardinality ? 8 : 10);
  const boxW = prevLabels.boxWidth ?? (highCardinality ? 8 : 10);
  const boxH = prevLabels.boxHeight ?? (highCardinality ? 8 : 10);
  config.options.plugins.legend = {
    ...prev,
    position: "right",
    labels: {
      ...prevLabels,
      font: {
        ...prevFont,
        size: fontSize
      },
      boxWidth: boxW,
      boxHeight: boxH
    }
  };
}
function cjsApplyTooltips(config) {
  if (!config.options) config.options = {};
  if (!config.options.plugins) config.options.plugins = {};
  if (!config.options.plugins.tooltip) {
    config.options.plugins.tooltip = { enabled: true };
  }
}

// src/core/static-series.ts
var STATIC_SERIES_KEY_COLUMN = "__flint_series_key";
var STATIC_SERIES_VALUE_COLUMN = "__flint_series_value";
var MEASURE_CHANNELS = /* @__PURE__ */ new Set(["x", "y"]);
function coerceEncodingValue(value) {
  if (typeof value === "string") {
    return { field: value };
  }
  if (Array.isArray(value)) {
    return value.map((entry) => typeof entry === "string" ? { field: entry } : entry);
  }
  return value;
}
function normalizeEncodingShorthand(encodings) {
  const out = {};
  for (const [channel, value] of Object.entries(encodings)) {
    out[channel] = coerceEncodingValue(value);
  }
  return out;
}
function normalizeStaticSeries(rawEncodings, data, semanticTypes) {
  const encodings = normalizeEncodingShorthand(rawEncodings);
  const arrayChannels = [];
  for (const [channel2, enc] of Object.entries(encodings)) {
    if (Array.isArray(enc)) {
      arrayChannels.push({ channel: channel2, entries: enc });
    }
  }
  if (arrayChannels.length === 0) {
    return {
      encodings,
      data
    };
  }
  if (arrayChannels.length > 1) {
    const channelNames = arrayChannels.map((c) => c.channel).join(", ");
    throw new Error(
      `Static series (array encoding) found on multiple channels: ${channelNames}. Only one channel may use array encoding at a time.`
    );
  }
  const { channel, entries } = arrayChannels[0];
  if (!MEASURE_CHANNELS.has(channel)) {
    throw new Error(
      `Static series (array encoding) is only allowed on measure channels (${[...MEASURE_CHANNELS].join(", ")}), not "${channel}".`
    );
  }
  if (entries.length < 2) {
    throw new Error(
      `Static series requires at least 2 fields, got ${entries.length} on channel "${channel}".`
    );
  }
  const fields = [];
  for (const entry of entries) {
    if (!entry.field) {
      throw new Error(
        `Each static series entry must have a "field" property.`
      );
    }
    fields.push(entry.field);
  }
  const fieldSet = new Set(fields);
  if (fieldSet.size !== fields.length) {
    throw new Error(
      `Static series contains duplicate fields. Each field must be unique.`
    );
  }
  if (data.length > 0) {
    const dataColumns = new Set(Object.keys(data[0]));
    for (const field of fields) {
      if (!dataColumns.has(field)) {
        throw new Error(
          `Static series field "${field}" not found in data columns. Available columns: ${[...dataColumns].join(", ")}`
        );
      }
    }
  }
  for (const entry of entries) {
    const field = entry.field;
    const explicitType = entry.type;
    if (explicitType === "nominal" || explicitType === "ordinal") {
      throw new Error(
        `Static series field "${field}" has type "${explicitType}" \u2014 only quantitative or temporal fields are allowed in static series.`
      );
    }
    if (!explicitType && data.length > 0) {
      const semType = semanticTypes[field];
      const semTypeStr = typeof semType === "string" ? semType : semType?.semanticType || "";
      const fromRegistry = semTypeStr ? getVisCategory(semTypeStr) : null;
      const inferred = fromRegistry ?? inferVisCategory(data.map((r) => r[field]));
      if (inferred === "nominal" || inferred === "ordinal") {
        throw new Error(
          `Static series field "${field}" infers as "${inferred}" from data \u2014 only quantitative or temporal fields are allowed in static series.`
        );
      }
    }
  }
  const colorEnc = encodings.color;
  if (colorEnc && !Array.isArray(colorEnc) && colorEnc.field) {
    throw new Error(
      `Cannot use static series on "${channel}" when the color channel is already bound to field "${colorEnc.field}". Static series implicitly uses the color channel for series discrimination.`
    );
  }
  const foldedData = foldData(data, fields);
  const normalizedEncodings = {};
  for (const [ch, enc] of Object.entries(encodings)) {
    if (ch === channel) {
      normalizedEncodings[ch] = { field: STATIC_SERIES_VALUE_COLUMN, type: "quantitative" };
    } else if (Array.isArray(enc)) {
      normalizedEncodings[ch] = enc[0];
    } else {
      normalizedEncodings[ch] = enc;
    }
  }
  const colorScheme = !Array.isArray(colorEnc) && colorEnc?.scheme ? colorEnc.scheme : void 0;
  normalizedEncodings.color = {
    field: STATIC_SERIES_KEY_COLUMN,
    type: "nominal",
    ...colorScheme ? { scheme: colorScheme } : {}
  };
  const metadata = {
    channel,
    fields,
    keyColumn: STATIC_SERIES_KEY_COLUMN,
    valueColumn: STATIC_SERIES_VALUE_COLUMN
  };
  return {
    encodings: normalizedEncodings,
    data: foldedData,
    staticSeries: metadata
  };
}
function foldData(data, fields) {
  const fieldSet = new Set(fields);
  const result = [];
  for (const row of data) {
    const baseRow = {};
    for (const [key, value] of Object.entries(row)) {
      if (!fieldSet.has(key)) {
        baseRow[key] = value;
      }
    }
    for (const field of fields) {
      const value = row[field];
      if (value == null) continue;
      result.push({
        ...baseRow,
        [STATIC_SERIES_KEY_COLUMN]: field,
        [STATIC_SERIES_VALUE_COLUMN]: value
      });
    }
  }
  return result;
}

// src/core/normalize-properties.ts
function normalizeChartProperties(properties, chartProperties) {
  const warnings = [];
  if (!properties || !chartProperties) {
    return { chartProperties, warnings };
  }
  let result;
  const ensureCopy = () => {
    if (!result) result = { ...chartProperties };
    return result;
  };
  for (const def of properties) {
    if (def.type !== "discrete") continue;
    if (!(def.key in chartProperties)) continue;
    const value = chartProperties[def.key];
    if (value == null) continue;
    if (def.options.some((o) => o.value === value)) continue;
    const byLabel = typeof value === "string" ? def.options.find(
      (o) => o.label != null && o.label.toLowerCase() === value.trim().toLowerCase()
    ) : void 0;
    if (byLabel) {
      ensureCopy()[def.key] = byLabel.value;
      warnings.push({
        severity: "info",
        code: "coerced-option-label",
        message: `chartProperties.${def.key}: '${value}' is a display label; using the accepted value '${byLabel.value}' instead.`
      });
      continue;
    }
    const accepted = def.options.map((o) => o.value == null ? "(default)" : `'${o.value}'`).join(", ");
    const copy = ensureCopy();
    delete copy[def.key];
    warnings.push({
      severity: "warning",
      code: "invalid-option-value",
      message: `chartProperties.${def.key}: '${value}' is not a valid option (accepted: ${accepted}). Falling back to the default.`
    });
  }
  return { chartProperties: result ?? chartProperties, warnings };
}

// src/chartjs/assemble.ts
function applyFieldDisplayNames(config, names) {
  if (!names) return;
  const displayName = (value) => typeof value === "string" ? names[value] ?? value : value;
  for (const scale of Object.values(config.options?.scales ?? {})) {
    if (scale?.title?.text) scale.title.text = displayName(scale.title.text);
  }
  for (const dataset of config.data?.datasets ?? []) {
    if (dataset?.label) dataset.label = displayName(dataset.label);
  }
}
function assembleChartjs(input) {
  const chartType = input.chart_spec.chartType;
  const semanticTypes = input.semantic_types ?? {};
  const sizeCeiling = input.chart_spec.canvasSize;
  const baseSize = resolveBaseSize(input.chart_spec.baseSize, sizeCeiling);
  const canvasSize = baseSize;
  const options = input.options ?? {};
  let chartTemplate = cjsGetTemplateDef(chartType);
  if (!chartTemplate) {
    throw new Error(`Unknown Chart.js chart type: ${chartType}. Use cjsAllTemplateDefs to see available types.`);
  }
  const warnings = [];
  const normalizedProps = normalizeChartProperties(
    chartTemplate.properties,
    input.chart_spec.chartProperties
  );
  const chartProperties = normalizedProps.chartProperties;
  warnings.push(...normalizedProps.warnings);
  const rawData = input.data.values ?? [];
  const normalized = normalizeStaticSeries(
    input.chart_spec.encodings,
    rawData,
    semanticTypes
  );
  let data = normalized.data;
  const staticSeries = normalized.staticSeries;
  const prelimConvertedData = convertTemporalData(data, semanticTypes);
  const prelimSemantics = resolveChannelSemantics(
    normalized.encodings,
    data,
    semanticTypes,
    prelimConvertedData
  );
  const typedRawEncodings = {};
  for (const [ch, enc] of Object.entries(normalized.encodings)) {
    typedRawEncodings[ch] = enc.type ? enc : { ...enc, type: prelimSemantics[ch]?.type };
  }
  const authoredTemplate = chartTemplate;
  const transformed = applyTransform(chartTemplate, typedRawEncodings, data, chartProperties, cjsGetTemplateDef);
  if (transformed.chartType && transformed.chartType !== chartType) {
    const swapped = cjsGetTemplateDef(transformed.chartType);
    if (swapped) chartTemplate = swapped;
  }
  const encodings = applyEncodingOverrides(chartTemplate, transformed.encodings, chartProperties);
  data = applyAggregation(encodings, data);
  const tplMark = chartTemplate.template?.mark;
  const templateMarkType = typeof tplMark === "string" ? tplMark : tplMark?.type;
  const convertedData = convertTemporalData(data, semanticTypes);
  const channelSemantics = resolveChannelSemantics(
    encodings,
    data,
    semanticTypes,
    convertedData
  );
  const effectiveMarkType = templateMarkType || "point";
  for (const [channel, cs] of Object.entries(channelSemantics)) {
    if ((channel === "x" || channel === "y") && cs.type === "quantitative") {
      const numericValues = data.map((r) => r[cs.field]).filter((v) => v != null && typeof v === "number" && !isNaN(v));
      cs.zero = computeZeroDecision(
        cs.semanticAnnotation.semanticType,
        channel,
        effectiveMarkType,
        numericValues
      );
    }
  }
  const declaration = chartTemplate.declareLayoutMode ? chartTemplate.declareLayoutMode(channelSemantics, data, chartProperties) : {};
  const effectiveOptions = {
    // Chart.js fills its canvas natively — a wider default band size
    // matches its generous category spacing behavior.
    defaultBandSize: 30,
    // Chart.js categories natively fill the available chart area.
    bandStepFit: 1,
    // Chart.js has no default maxBarThickness, so sparse bars fill the
    // canvas. Allow bands to expand past the base to match, capped so a
    // couple of bars don't span the whole canvas.
    maxBandSize: 100,
    // Chart.js native font default (12 for ticks, legend, and title).
    baseLabelFontSize: 12,
    baseTitleFontSize: 12,
    ...options,
    ...declaration.paramOverrides || {}
  };
  Object.assign(effectiveOptions, deriveStretchCaps(baseSize, sizeCeiling, effectiveOptions));
  effectiveOptions.bandStepFitCapacityX = sizeCeiling?.width ?? baseSize.width;
  effectiveOptions.bandStepFitCapacityY = sizeCeiling?.height ?? baseSize.height;
  effectiveOptions.facetColumns = resolveFacetColumnsOption(input.chart_spec.chartProperties);
  const {
    addTooltips: addTooltipsOpt = false
  } = effectiveOptions;
  const allMarkTypes = /* @__PURE__ */ new Set();
  if (templateMarkType) allMarkTypes.add(templateMarkType);
  const budgets = computeChannelBudgets(
    channelSemantics,
    declaration,
    convertedData,
    canvasSize,
    effectiveOptions
  );
  const facetGridResult = budgets.facetGrid;
  const overflowResult = filterOverflow(
    channelSemantics,
    declaration,
    encodings,
    convertedData,
    budgets,
    allMarkTypes
  );
  const values = overflowResult.filteredData;
  warnings.push(...overflowResult.warnings);
  const layoutResult = computeLayout(
    channelSemantics,
    declaration,
    values,
    canvasSize,
    effectiveOptions,
    facetGridResult
  );
  layoutResult.truncations = overflowResult.truncations;
  const resolvedEncodings = {};
  for (const [channel, encoding] of Object.entries(encodings)) {
    const cs = channelSemantics[channel];
    if (cs) {
      resolvedEncodings[channel] = {
        field: cs.field,
        type: cs.type,
        aggregate: encoding.aggregate
      };
    }
  }
  const instantiateContext = {
    channelSemantics,
    layout: layoutResult,
    table: values,
    fullTable: convertedData,
    resolvedEncodings,
    encodings,
    chartProperties,
    staticSeries,
    canvasSize,
    semanticTypes,
    chartType,
    assembleOptions: effectiveOptions,
    colorDecisions: decideColorMaps({
      encodings,
      channelSemantics,
      table: values})
  };
  const colField = channelSemantics.column?.field;
  const rowField = channelSemantics.row?.field;
  const hasFacet = !!(colField || rowField);
  const hasAxes = chartTemplate.channels.includes("x") || chartTemplate.channels.includes("y");
  let cjsConfig;
  if (hasFacet && hasAxes) {
    const colValues = colField ? [...new Set(values.map((r) => String(r[colField])))] : [""];
    const rowValues = rowField ? [...new Set(values.map((r) => String(r[rowField])))] : [""];
    const facetLegend = [];
    const yField = channelSemantics.y?.field;
    const colorField = channelSemantics.color?.field;
    let sharedYDomain;
    if (yField) {
      const nums = values.map((r) => r[yField]).filter((v) => typeof v === "number" && Number.isFinite(v));
      if (nums.length > 0) {
        const rawMin = Math.min(...nums);
        const rawMax = Math.max(...nums);
        const forceZero = !!channelSemantics.y?.zero?.zero;
        const min = forceZero ? Math.min(0, rawMin) : rawMin;
        const max = forceZero ? Math.max(0, rawMax) : rawMax;
        sharedYDomain = niceBounds(min, max);
      }
    }
    const axisGutter = sharedYDomain ? estimateYAxisGutter(sharedYDomain) : 0;
    const maxColsPerRow = colField && !rowField ? facetGridResult?.columns ?? colValues.length : colValues.length;
    const wrapColumnOnly = !!colField && !rowField && maxColsPerRow < colValues.length;
    const gridRows = [];
    if (wrapColumnOnly) {
      for (let i = 0; i < colValues.length; i += maxColsPerRow) {
        gridRows.push(
          colValues.slice(i, i + maxColsPerRow).map((cv) => ({ colVal: cv, rowVal: "" }))
        );
      }
    } else {
      for (let ri = 0; ri < rowValues.length; ri++) {
        gridRows.push(colValues.map((cv) => ({ colVal: cv, rowVal: rowValues[ri] })));
      }
    }
    const panelRows = [];
    for (let ri = 0; ri < gridRows.length; ri++) {
      const rowPanels = [];
      const cells = gridRows[ri];
      for (let ci = 0; ci < cells.length; ci++) {
        const { colVal, rowVal } = cells[ci];
        const panelData = values.filter((r) => {
          if (colField && String(r[colField]) !== colVal) return false;
          if (rowField && String(r[rowField]) !== rowVal) return false;
          return true;
        });
        const panelConfig = structuredClone(chartTemplate.template);
        const panelContext = {
          ...instantiateContext,
          table: panelData,
          layout: layoutResult
        };
        chartTemplate.instantiate(panelConfig, panelContext);
        if (!panelConfig.options) panelConfig.options = {};
        if (!panelConfig.options.plugins) panelConfig.options.plugins = {};
        panelConfig.options.plugins.legend = {
          ...panelConfig.options.plugins.legend || {},
          display: false,
          position: "right"
        };
        cjsApplyLayoutToSpec(panelConfig, panelContext, []);
        if (addTooltipsOpt) cjsApplyTooltips(panelConfig);
        if (chartTemplate.postProcess) chartTemplate.postProcess(panelConfig, panelContext);
        if (ci > 0 && panelConfig.options?.scales?.y) {
          const yScale = panelConfig.options.scales.y;
          yScale.ticks = { ...yScale.ticks || {}, display: false };
          yScale.title = { ...yScale.title || {}, display: false };
          yScale.border = { ...yScale.border || {}, display: false };
          if (typeof panelConfig._width === "number") {
            panelConfig._width = Math.max(40, panelConfig._width - axisGutter);
          }
        }
        const xScale = panelConfig.options?.scales?.x;
        if (xScale && typeof xScale.ticks?.callback === "function") {
          xScale.ticks = { ...xScale.ticks || {}, align: "inner" };
        }
        if (facetLegend.length === 0 && colorField && Array.isArray(panelConfig.data?.datasets)) {
          for (const ds of panelConfig.data.datasets) {
            const label = String(ds?.label ?? "").trim();
            if (!label) continue;
            const color = String(ds?.borderColor ?? ds?.backgroundColor ?? "#666");
            facetLegend.push({ label, color });
          }
        }
        rowPanels.push({
          key: `${ri}:${ci}`,
          rowIndex: ri,
          colIndex: ci,
          rowHeader: rowField ? rowVal : void 0,
          colHeader: colField ? colVal : void 0,
          config: panelConfig
        });
      }
      panelRows.push(rowPanels);
    }
    cjsConfig = cjsCombineFacetPanels(
      panelRows,
      !!colField,
      !!rowField,
      sharedYDomain,
      axisGutter,
      wrapColumnOnly
    );
    cjsConfig._facetLegend = facetLegend;
  } else {
    cjsConfig = structuredClone(chartTemplate.template);
    chartTemplate.instantiate(cjsConfig, instantiateContext);
    cjsApplyLayoutToSpec(cjsConfig, instantiateContext, warnings);
    if (addTooltipsOpt) cjsApplyTooltips(cjsConfig);
    if (chartTemplate.postProcess) chartTemplate.postProcess(cjsConfig, instantiateContext);
  }
  if (warnings.length > 0) {
    cjsConfig._warnings = warnings;
  }
  cjsConfig._dataLength = values.length;
  if (transformed.surface) {
    cjsConfig._transform = transformed.surface;
  }
  const legacyPivot = applyPivot(authoredTemplate, typedRawEncodings, data, chartProperties, cjsGetTemplateDef);
  if (legacyPivot.surface) {
    cjsConfig._pivot = legacyPivot.surface;
  }
  applyFieldDisplayNames(cjsConfig, input.field_display_names);
  return cjsConfig;
}
function getChartjsPivot(input) {
  const spec = assembleChartjs(input);
  return spec && spec._pivot ? spec._pivot : void 0;
}
function getChartjsTransform(input) {
  const spec = assembleChartjs(input);
  return spec && spec._transform ? spec._transform : void 0;
}
function niceBounds(min, max, targetTicks = 5) {
  if (!(Number.isFinite(min) && Number.isFinite(max)) || max <= min) {
    return { min, max };
  }
  const niceNum = (range, round) => {
    const exp = Math.floor(Math.log10(range));
    const frac = range / 10 ** exp;
    let niceFrac;
    if (round) {
      niceFrac = frac < 1.5 ? 1 : frac < 3 ? 2 : frac < 7 ? 5 : 10;
    } else {
      niceFrac = frac <= 1 ? 1 : frac <= 2 ? 2 : frac <= 5 ? 5 : 10;
    }
    return niceFrac * 10 ** exp;
  };
  const step = niceNum(niceNum(max - min, false) / Math.max(1, targetTicks - 1), true);
  return {
    min: Math.floor(min / step) * step,
    max: Math.ceil(max / step) * step
  };
}
function estimateYAxisGutter(domain) {
  const fmt = (v) => {
    const r = Math.round(v);
    return Math.abs(r) >= 1e3 ? r.toLocaleString() : String(r);
  };
  const chars = Math.max(fmt(domain.min).length, fmt(domain.max).length);
  return Math.ceil(chars * 6.5) + 30;
}
function cjsCombineFacetPanels(panelRows, hasColHeader, hasRowHeader, sharedYDomain, axisGutter = 0, colHeaderPerRow = false) {
  const rows = panelRows.length;
  const cols = Math.max(1, ...panelRows.map((r) => r.length));
  const ref = panelRows[0]?.[0]?.config;
  const panelH = ref?._height || 300;
  const gap = 16;
  const colHeaderH = hasColHeader ? 22 : 0;
  const rowHeaderW = hasRowHeader ? 28 : 0;
  const colWidths = Array.from(
    { length: cols },
    (_, ci) => panelRows[0]?.[ci]?.config?._width || ref?._width || 400
  );
  const totalPanelsW = colWidths.reduce((a, b) => a + b, 0);
  const headerBands = colHeaderPerRow ? rows : hasColHeader ? 1 : 0;
  return {
    _facet: true,
    _facetPanels: panelRows,
    _facetRows: rows,
    _facetCols: cols,
    _facetSharedYDomain: sharedYDomain,
    _facetAxisGutter: axisGutter,
    _facetColHeaderPerRow: colHeaderPerRow,
    _width: rowHeaderW + totalPanelsW + (cols - 1) * gap,
    _height: headerBands * colHeaderH + rows * panelH + (rows - 1) * gap
  };
}

// src/core/recommendation.ts
var FAMILY_XY_STANDARD = {
  x: "category",
  y: "measure",
  color: "series",
  opacity: "auxiliary",
  size: "auxiliary",
  shape: "auxiliary",
  detail: "auxiliary",
  group: "series",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_XY_HORIZONTAL = {
  y: "category",
  x: "measure",
  color: "series",
  opacity: "auxiliary",
  size: "auxiliary",
  shape: "auxiliary",
  detail: "auxiliary",
  group: "series",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_PIE = {
  color: "category",
  size: "measure",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_ROSE = {
  x: "category",
  y: "measure",
  color: "series",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_RADAR = {
  x: "category",
  y: "measure",
  color: "series",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_MAP = {
  latitude: "geo",
  longitude: "geo",
  color: "series",
  size: "auxiliary",
  opacity: "auxiliary"
};
var FAMILY_CHOROPLETH = {
  id: "geo",
  color: "measure",
  detail: "auxiliary"
};
var FAMILY_CANDLESTICK = {
  x: "category",
  open: "price",
  high: "price",
  low: "price",
  close: "price",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_HISTOGRAM = {
  x: "measure",
  color: "series",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_DENSITY = {
  x: "measure",
  color: "series",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_DENSITY_2D = {
  x: "measure",
  y: "measure2",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_HEATMAP = {
  x: "category",
  y: "category",
  color: "measure",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_GAUGE = {
  size: "measure",
  column: "facetCol"
};
var FAMILY_FUNNEL = {
  y: "category",
  size: "measure"
};
var FAMILY_TREEMAP = {
  color: "category",
  size: "measure",
  detail: "auxiliary",
  group: "auxiliary"
};
var FAMILY_SANKEY = {
  x: "category",
  y: "category",
  size: "measure"
};
var FAMILY_GANTT = {
  y: "category",
  x: "measure",
  x2: "measure2",
  color: "series",
  detail: "auxiliary",
  column: "facetCol",
  row: "facetRow"
};
var FAMILY_BULLET = {
  y: "category",
  x: "measure",
  goal: "measure2",
  color: "series",
  column: "facetCol",
  row: "facetRow"
};
var CHART_ROLE_MAP = {
  // Axis-based (x/y standard)
  "Bar Chart": FAMILY_XY_STANDARD,
  "Pyramid Chart": FAMILY_XY_HORIZONTAL,
  "Grouped Bar Chart": FAMILY_XY_STANDARD,
  "Stacked Bar Chart": FAMILY_XY_STANDARD,
  "Lollipop Chart": FAMILY_XY_STANDARD,
  "Waterfall Chart": FAMILY_XY_STANDARD,
  "Gantt Chart": FAMILY_GANTT,
  "Bullet Chart": FAMILY_BULLET,
  "Bar Table": FAMILY_XY_HORIZONTAL,
  "Line Chart": FAMILY_XY_STANDARD,
  "Bump Chart": FAMILY_XY_STANDARD,
  "Area Chart": FAMILY_XY_STANDARD,
  "Streamgraph": FAMILY_XY_STANDARD,
  "Scatter Plot": FAMILY_XY_STANDARD,
  "Connected Scatter Plot": FAMILY_XY_STANDARD,
  "Regression": FAMILY_XY_STANDARD,
  "Ranged Dot Plot": FAMILY_XY_STANDARD,
  "Boxplot": FAMILY_XY_STANDARD,
  "Strip Plot": FAMILY_XY_STANDARD,
  // Pie-like
  "Pie Chart": FAMILY_PIE,
  // Polar
  "Rose Chart": FAMILY_ROSE,
  "Radar Chart": FAMILY_RADAR,
  // Heatmap
  "Heatmap": FAMILY_HEATMAP,
  // Histogram / Density
  "Histogram": FAMILY_HISTOGRAM,
  "Density Plot": FAMILY_DENSITY,
  "Density Contour": FAMILY_DENSITY_2D,
  // Geographic
  "Map": FAMILY_MAP,
  "Choropleth": FAMILY_CHOROPLETH,
  // Financial
  "Candlestick Chart": FAMILY_CANDLESTICK,
  // ECharts-only
  "Gauge Chart": FAMILY_GAUGE,
  "Funnel Chart": FAMILY_FUNNEL,
  "Treemap": FAMILY_TREEMAP,
  "Sunburst Chart": FAMILY_TREEMAP,
  "Sankey Diagram": FAMILY_SANKEY
};
function getChannelRole(chartType, channel) {
  const roleMap = CHART_ROLE_MAP[chartType];
  if (roleMap && channel in roleMap) return roleMap[channel];
  if (channel === "column") return "facetCol";
  if (channel === "row") return "facetRow";
  return "auxiliary";
}
function findChannelsByRole(chartType, templateChannels, role) {
  return templateChannels.filter((ch) => getChannelRole(chartType, ch) === role);
}
var FALLBACK_CHAIN = {
  measure2: ["measure", "auxiliary"],
  series: ["auxiliary"],
  category: ["series", "auxiliary"],
  measure: ["auxiliary"],
  geo: ["category"],
  price: ["measure", "auxiliary"]
};
var ROLE_PRIORITY = {
  category: 0,
  measure: 1,
  series: 2,
  facetCol: 3,
  facetRow: 4,
  measure2: 5,
  auxiliary: 6,
  geo: 7,
  price: 8
};
function adaptChannels(sourceType, targetType, targetChannels, encodings, data, semanticTypes, recommendFn) {
  if (data && data.length > 0) {
    return adaptViaRecommendation(sourceType, targetType, targetChannels, encodings, data, semanticTypes ?? {});
  }
  return adaptViaRoles(sourceType, targetType, targetChannels, encodings);
}
function adaptViaRecommendation(sourceType, targetType, targetChannels, encodings, data, semanticTypes, _recommendFn) {
  const FACET_CHANNELS = ["column", "row"];
  let facetedData = data;
  const prePinned = {};
  const prePinnedFields = /* @__PURE__ */ new Set();
  for (const ch of FACET_CHANNELS) {
    const field = encodings[ch];
    if (field && targetChannels.includes(ch)) {
      prePinned[ch] = field;
      prePinnedFields.add(field);
      if (facetedData.length > 0) {
        const firstVal = facetedData[0][field];
        facetedData = facetedData.filter((row) => row[field] === firstVal);
      }
    }
  }
  const tv = buildTableView(facetedData, semanticTypes);
  const isFieldCompatibleWithRole = (role, field) => {
    const ft = tv.fieldType[field] ?? "nominal";
    const st = tv.fieldSemanticType[field] ?? "";
    const card = tv.fieldLevels[field]?.length ?? 0;
    switch (role) {
      // 'category' is for true discrete axes (nominal/ordinal/temporal).
      // Quantitative fields — even low-cardinality ones — must NOT
      // satisfy this role, otherwise a measure can land on the
      // category axis (e.g. Bar Table y) and push the real discrete
      // field onto color.
      case "category":
        return !isQuantitativeField(ft, st) && isDiscreteLike(ft, st, card);
      case "measure":
        return isQuantitativeField(ft, st);
      case "series":
        return isDiscreteLike(ft, st, card);
      case "geo":
        return isGeoCoordinateType(st) || ft === "quantitative";
      case "facetCol":
      case "facetRow":
        return isDiscreteLike(ft, st, card);
      case "auxiliary":
        return true;
      default:
        return true;
    }
  };
  const assignCost = (srcCh, field, targetCh) => {
    const targetRole = getChannelRole(targetType, targetCh);
    if (!isFieldCompatibleWithRole(targetRole, field)) return Infinity;
    const srcRole = getChannelRole(sourceType, srcCh);
    if (srcCh === targetCh && srcRole === targetRole) return 0;
    if (srcRole === targetRole) return 0.5;
    if (srcCh === targetCh) return 1;
    return 1;
  };
  const COST_DROP = 1.5;
  const entries = Object.entries(encodings).filter(([ch, f]) => f && !FACET_CHANNELS.includes(ch) && !prePinnedFields.has(f));
  const availableTargets = targetChannels.filter((ch) => !(ch in prePinned));
  let bestCost = Infinity;
  let bestAssignment = {};
  const usedTargets = /* @__PURE__ */ new Set();
  function solve(idx, currentCost, assignment) {
    if (currentCost >= bestCost) return;
    if (idx === entries.length) {
      bestCost = currentCost;
      bestAssignment = { ...assignment };
      return;
    }
    const [srcCh, field] = entries[idx];
    for (const tch of availableTargets) {
      if (usedTargets.has(tch)) continue;
      const cost = assignCost(srcCh, field, tch);
      if (cost === Infinity) continue;
      usedTargets.add(tch);
      assignment[tch] = field;
      solve(idx + 1, currentCost + cost, assignment);
      delete assignment[tch];
      usedTargets.delete(tch);
    }
    solve(idx + 1, currentCost + COST_DROP, assignment);
  }
  solve(0, 0, {});
  const result = { ...prePinned, ...bestAssignment };
  return result;
}
function adaptViaRoles(sourceType, targetType, targetChannels, encodings) {
  const result = {};
  const filledEncodings = [];
  for (const [ch, field] of Object.entries(encodings)) {
    if (field) {
      filledEncodings.push({ channel: ch, role: getChannelRole(sourceType, ch), field });
    }
  }
  filledEncodings.sort((a, b) => ROLE_PRIORITY[a.role] - ROLE_PRIORITY[b.role]);
  const assigned = /* @__PURE__ */ new Set();
  for (const { channel: srcCh, role: srcRole, field } of filledEncodings) {
    let placed = false;
    if (targetChannels.includes(srcCh) && !assigned.has(srcCh)) {
      if (getChannelRole(targetType, srcCh) === srcRole) {
        result[srcCh] = field;
        assigned.add(srcCh);
        placed = true;
      }
    }
    if (!placed) {
      placed = tryAssign(srcRole, field, targetType, targetChannels, result, assigned, srcCh);
    }
    if (!placed) {
      const chain = FALLBACK_CHAIN[srcRole];
      if (chain) {
        for (const fallbackRole of chain) {
          placed = tryAssign(fallbackRole, field, targetType, targetChannels, result, assigned, srcCh);
          if (placed) break;
        }
      }
    }
  }
  return result;
}
function tryAssign(role, field, targetType, targetChannels, result, assigned, preferredName) {
  const candidates = findChannelsByRole(targetType, targetChannels, role).filter((ch) => !assigned.has(ch));
  if (candidates.length === 0) return false;
  const best = preferredName && candidates.includes(preferredName) ? preferredName : candidates[0];
  result[best] = field;
  assigned.add(best);
  return true;
}
function buildTableView(data, semanticTypes) {
  const names = data.length > 0 ? Object.keys(data[0]) : [];
  const fieldType = {};
  const fieldSemanticType = {};
  const fieldLevels = {};
  for (const name of names) {
    const values = data.map((r) => r[name]);
    const semanticType = semanticTypes[name] || "";
    fieldType[name] = semanticType && getVisCategory(semanticType) || inferVisCategory(values);
    fieldSemanticType[name] = semanticType;
    fieldLevels[name] = [...new Set(data.map((r) => r[name]).filter((v) => v != null))];
  }
  return { names, fieldType, fieldSemanticType, fieldLevels, rows: data };
}
var Pref = { STRONG: 3, OK: 2, WEAK: 1, EXCLUDE: -Infinity };
function resolveAssignment(tv, used, channelPrefs) {
  const candidates = tv.names.filter((n) => !used.has(n) && (!isLikelyIdentifierOrRank(n) || tv.preferredFields?.has(n)));
  const C = channelPrefs.length;
  const F = candidates.length;
  if (F < C) return {};
  const scores = [];
  for (let ci = 0; ci < C; ci++) {
    scores[ci] = [];
    for (let fi = 0; fi < F; fi++) {
      const name = candidates[fi];
      const type = tv.fieldType[name] ?? "nominal";
      const st = tv.fieldSemanticType[name] ?? "";
      const card = tv.fieldLevels[name]?.length ?? 0;
      scores[ci][fi] = channelPrefs[ci].pref(name, type, st, card, card > 0);
    }
  }
  let bestScore = -Infinity;
  let bestAssign;
  const perm = new Array(C);
  const usedF = new Uint8Array(F);
  function search(depth, totalScore) {
    if (depth === C) {
      if (totalScore > bestScore) {
        bestScore = totalScore;
        bestAssign = [...perm];
      }
      return;
    }
    for (let fi = 0; fi < F; fi++) {
      if (usedF[fi]) continue;
      const s = scores[depth][fi];
      if (s === -Infinity) continue;
      if (totalScore + s <= bestScore - (C - depth - 1) * Pref.STRONG) continue;
      perm[depth] = fi;
      usedF[fi] = 1;
      search(depth + 1, totalScore + s);
      usedF[fi] = 0;
    }
  }
  search(0, 0);
  if (!bestAssign) return {};
  const result = {};
  for (let ci = 0; ci < C; ci++) {
    const fieldName = candidates[bestAssign[ci]];
    result[channelPrefs[ci].channel] = fieldName;
    used.add(fieldName);
  }
  return result;
}
function isTemporalField(type, semanticType) {
  return type === "temporal" || isTimeSeriesType(semanticType);
}
function isQuantitativeField(type, semanticType) {
  if (isTemporalField(type, semanticType)) return false;
  if (type !== "quantitative") return false;
  if (isNonMeasureNumeric(semanticType)) return false;
  return isMeasureType(semanticType) || semanticType === "";
}
function isOrdinalField(type, semanticType, hasLevels) {
  if (hasLevels) return true;
  return isOrdinalType(semanticType);
}
function isCategoricalFieldCheck(type, semanticType) {
  if (isTemporalField(type, semanticType)) return false;
  if (isQuantitativeField(type, semanticType)) return false;
  return type === "nominal" || isCategoricalType(semanticType);
}
function isDiscreteLike(type, semanticType, cardinality, maxCard = 50) {
  if (isCategoricalFieldCheck(type, semanticType)) return true;
  if (isTemporalField(type, semanticType)) return true;
  if (isOrdinalType(semanticType)) return true;
  if (type === "quantitative" && cardinality > 0 && cardinality <= maxCard) return true;
  return false;
}
function nameMatches(name, patterns) {
  const lower = name.toLowerCase();
  return patterns.some((p) => lower === p) || patterns.some((p) => lower.includes(p));
}
function isLikelyIdentifierOrRank(name) {
  const lower = name.toLowerCase();
  const idPatterns = ["rank", "id", "index", "idx", "row", "order", "position", "pos"];
  return idPatterns.some((p) => lower === p || lower.endsWith("_" + p) || lower.endsWith(p));
}
function pick(tv, used, predicate) {
  const candidates = [];
  for (const name of tv.names) {
    if (used.has(name)) continue;
    const type = tv.fieldType[name] ?? "nominal";
    const semanticType = tv.fieldSemanticType[name] ?? "";
    const cardinality = tv.fieldLevels[name]?.length ?? 0;
    const hasLevels = cardinality > 0;
    if (predicate(name, type, semanticType, cardinality, hasLevels)) {
      candidates.push(name);
    }
  }
  if (candidates.length === 0) return void 0;
  if (tv.preferredFields) {
    const preferred = candidates.filter((n) => tv.preferredFields.has(n));
    if (preferred.length > 0) {
      const chosen2 = preferred[0];
      used.add(chosen2);
      return chosen2;
    }
  }
  const chosen = candidates[0];
  used.add(chosen);
  return chosen;
}
var pickQuantitative = (tv, u) => pick(tv, u, (name, ty, st) => isQuantitativeField(ty, st) && (!isLikelyIdentifierOrRank(name) || !!tv.preferredFields?.has(name)));
var pickTemporal = (tv, u) => pick(tv, u, (_n, ty, st) => isTemporalField(ty, st));
var pickNominal = (tv, u) => pick(tv, u, (_n, ty, st) => isCategoricalFieldCheck(ty, st));
var pickLowCardNominal = (tv, u, maxCard = 30) => pick(tv, u, (_n, ty, st, card) => isCategoricalFieldCheck(ty, st) && card > 0 && card <= maxCard);
var pickOrdinal = (tv, u) => pick(tv, u, (_n, ty, st, _card, hasLevels) => isOrdinalField(ty, st, hasLevels));
var pickDiscrete = (tv, u) => pick(tv, u, (name, ty, st, card) => isDiscreteLike(ty, st, card) && (!isLikelyIdentifierOrRank(name) || !!tv.preferredFields?.has(name)));
var pickLowCardDiscrete = (tv, u, maxCard = 30) => pick(
  tv,
  u,
  (name, ty, st, card) => isDiscreteLike(ty, st, card, maxCard) && card > 0 && card <= maxCard && (!isLikelyIdentifierOrRank(name) || !!tv.preferredFields?.has(name))
);
var pickSeriesAxis = (tv, u) => pickTemporal(tv, u) ?? pickOrdinal(tv, u) ?? pickNominal(tv, u);
var pickQuantitativeByName = (tv, u, patterns) => pick(tv, u, (name, ty, st) => isQuantitativeField(ty, st) && nameMatches(name, patterns));
function pickAllQuantitative(tv, used) {
  const result = [];
  for (const name of tv.names) {
    if (used.has(name)) continue;
    const type = tv.fieldType[name] ?? "nominal";
    const semanticType = tv.fieldSemanticType[name] ?? "";
    if (isQuantitativeField(type, semanticType) && (!isLikelyIdentifierOrRank(name) || tv.preferredFields?.has(name))) {
      result.push(name);
    }
  }
  for (const name of result) used.add(name);
  return result;
}
function hasMultipleValuesPerField(tv, fieldName) {
  if (!fieldName || !tv.rows || tv.rows.length === 0) return false;
  const seen = /* @__PURE__ */ new Set();
  for (const row of tv.rows) {
    const val = row[fieldName];
    if (seen.has(val)) return true;
    seen.add(val);
  }
  return false;
}
function isValidGroupingField(tv, xField, colorField) {
  if (!xField || !colorField || !tv.rows || tv.rows.length === 0) return false;
  const seen = /* @__PURE__ */ new Set();
  for (const row of tv.rows) {
    const key = `${row[xField]}|||${row[colorField]}`;
    if (seen.has(key)) return false;
    seen.add(key);
  }
  return true;
}
function lowestCardinality(tv, candidates) {
  let best = candidates[0];
  let bestCard = tv.fieldLevels[best]?.length ?? Infinity;
  for (let i = 1; i < candidates.length; i++) {
    const card = tv.fieldLevels[candidates[i]]?.length ?? Infinity;
    if (card < bestCard) {
      best = candidates[i];
      bestCard = card;
    }
  }
  return best;
}
function pickValidGroupingField(tv, used, xField, maxCard = 20) {
  const candidates = [];
  for (const name of tv.names) {
    if (used.has(name)) continue;
    const type = tv.fieldType[name] ?? "nominal";
    const semanticType = tv.fieldSemanticType[name] ?? "";
    const cardinality = tv.fieldLevels[name]?.length ?? 0;
    if (!isDiscreteLike(type, semanticType, cardinality, maxCard)) continue;
    if (cardinality <= 0 || cardinality > maxCard) continue;
    if (isLikelyIdentifierOrRank(name) && !tv.preferredFields?.has(name)) continue;
    if (isValidGroupingField(tv, xField, name)) candidates.push(name);
  }
  if (candidates.length === 0) return void 0;
  if (tv.preferredFields) {
    const preferred = candidates.filter((n) => tv.preferredFields.has(n));
    if (preferred.length > 0) {
      const chosen2 = lowestCardinality(tv, preferred);
      used.add(chosen2);
      return chosen2;
    }
  }
  const chosen = lowestCardinality(tv, candidates);
  used.add(chosen);
  return chosen;
}
function isValidLineSeriesData(tv, xField, colorField) {
  if (!tv.rows || tv.rows.length === 0) return false;
  const xColorCombinations = /* @__PURE__ */ new Set();
  const colorGroupCounts = /* @__PURE__ */ new Map();
  for (const row of tv.rows) {
    const xVal = row[xField];
    const colorVal = colorField ? row[colorField] : "__single__";
    const xColorKey = `${xVal}|||${colorVal}`;
    if (xColorCombinations.has(xColorKey)) return false;
    xColorCombinations.add(xColorKey);
    colorGroupCounts.set(colorVal, (colorGroupCounts.get(colorVal) ?? 0) + 1);
  }
  let validGroups = 0;
  let totalGroups = 0;
  for (const count of colorGroupCounts.values()) {
    totalGroups++;
    if (count >= 2) validGroups++;
  }
  return totalGroups > 0 && validGroups / totalGroups > 0.5;
}
function pickLineChartColorField(tv, used, xField, maxCard = 20) {
  const candidates = [];
  for (const name of tv.names) {
    if (used.has(name)) continue;
    const type = tv.fieldType[name] ?? "nominal";
    const semanticType = tv.fieldSemanticType[name] ?? "";
    const cardinality = tv.fieldLevels[name]?.length ?? 0;
    if (!isDiscreteLike(type, semanticType, cardinality, maxCard)) continue;
    if (cardinality <= 0 || cardinality > maxCard) continue;
    if (isLikelyIdentifierOrRank(name) && !tv.preferredFields?.has(name)) continue;
    if (isValidLineSeriesData(tv, xField, name)) candidates.push(name);
  }
  if (candidates.length === 0) return void 0;
  if (tv.preferredFields) {
    const preferred = candidates.filter((n) => tv.preferredFields.has(n));
    if (preferred.length > 0) {
      const chosen2 = lowestCardinality(tv, preferred);
      used.add(chosen2);
      return chosen2;
    }
  }
  const chosen = lowestCardinality(tv, candidates);
  used.add(chosen);
  return chosen;
}
function calculateMultiplicity(tv, xField, colorField) {
  if (!tv.rows || tv.rows.length === 0) return 1;
  const groups = /* @__PURE__ */ new Set();
  for (const row of tv.rows) {
    const key = colorField ? `${row[xField]}|||${row[colorField]}` : `${row[xField]}`;
    groups.add(key);
  }
  return tv.rows.length / groups.size;
}
function pickBestGroupingField(tv, used, xField, maxMultiplicity = 5) {
  const baseMultiplicity = calculateMultiplicity(tv, xField);
  if (baseMultiplicity <= 1) return void 0;
  let bestField;
  let bestMultiplicity = baseMultiplicity;
  for (const name of tv.names) {
    if (used.has(name)) continue;
    const type = tv.fieldType[name] ?? "nominal";
    const semanticType = tv.fieldSemanticType[name] ?? "";
    const cardinality = tv.fieldLevels[name]?.length ?? 0;
    if (!isDiscreteLike(type, semanticType, cardinality)) continue;
    if (isLikelyIdentifierOrRank(name) && !tv.preferredFields?.has(name)) continue;
    const multiplicity = calculateMultiplicity(tv, xField, name);
    if (multiplicity < bestMultiplicity) {
      bestMultiplicity = multiplicity;
      bestField = name;
      if (multiplicity <= 1) break;
    }
  }
  if (bestField && bestMultiplicity < baseMultiplicity && bestMultiplicity <= maxMultiplicity) {
    used.add(bestField);
    return bestField;
  }
  return void 0;
}
function recommendChannels(chartType, data, semanticTypes, recommendFn) {
  const fn = getRecommendation;
  return fn(chartType, buildTableView(data, semanticTypes));
}
function getRecommendation(chartType, tv) {
  const used = /* @__PURE__ */ new Set();
  const rec = {};
  const assign = (channel, fieldName) => {
    if (fieldName) rec[channel] = fieldName;
  };
  switch (chartType) {
    case "Scatter Plot": {
      const yField = pickQuantitative(tv, used) ?? pickTemporal(tv, used) ?? pickNominal(tv, used);
      const xField = pickQuantitative(tv, used) ?? pickTemporal(tv, used) ?? pickNominal(tv, used);
      if (!xField || !yField) return {};
      assign("x", xField);
      assign("y", yField);
      assign("color", pickLowCardNominal(tv, used));
      break;
    }
    case "Bar Chart":
    case "Stacked Bar Chart": {
      const xField = pickDiscrete(tv, used);
      const yField = pickQuantitative(tv, used);
      if (!xField || !yField) return {};
      assign("x", xField);
      assign("y", yField);
      if (hasMultipleValuesPerField(tv, xField)) {
        assign("color", pickBestGroupingField(tv, used, xField));
      }
      break;
    }
    case "Grouped Bar Chart": {
      const xField = pickDiscrete(tv, used);
      const yField = pickQuantitative(tv, used);
      if (!xField || !yField) return {};
      const seriesField = pickValidGroupingField(tv, used, xField, 20);
      if (!seriesField) return {};
      assign("x", xField);
      assign("y", yField);
      assign("group", seriesField);
      break;
    }
    case "Histogram": {
      const xField = pickQuantitative(tv, used);
      if (!xField) return {};
      assign("x", xField);
      break;
    }
    case "Heatmap": {
      const heatmapResult = resolveAssignment(tv, used, [
        {
          channel: "x",
          pref: (_n, ty, st, card) => {
            if (isTimeSeriesType(st)) return Pref.STRONG;
            if (isCategoricalType(st)) return Pref.OK;
            if (isOrdinalType(st)) return Pref.OK;
            if (isNonMeasureNumeric(st)) return Pref.OK;
            if (ty === "nominal") return Pref.OK;
            if (ty === "temporal") return Pref.STRONG;
            if (ty === "quantitative" && card > 0 && card <= 50) return Pref.WEAK;
            return Pref.EXCLUDE;
          }
        },
        {
          channel: "y",
          pref: (_n, ty, st, card) => {
            if (isCategoricalType(st)) return Pref.STRONG;
            if (isTimeSeriesType(st)) return Pref.OK;
            if (isOrdinalType(st)) return Pref.OK;
            if (isNonMeasureNumeric(st)) return Pref.OK;
            if (ty === "nominal") return Pref.STRONG;
            if (ty === "temporal") return Pref.OK;
            if (ty === "quantitative" && card > 0 && card <= 50) return Pref.WEAK;
            return Pref.EXCLUDE;
          }
        },
        {
          channel: "color",
          pref: (_n, ty, st) => {
            if (isMeasureType(st)) return Pref.STRONG;
            if (isOrdinalType(st)) return Pref.OK;
            if (ty === "quantitative" && !st) return Pref.STRONG;
            if (ty === "temporal") return Pref.WEAK;
            if (ty === "nominal") return Pref.WEAK;
            return Pref.EXCLUDE;
          }
        }
      ]);
      if (!heatmapResult["x"] || !heatmapResult["y"] || !heatmapResult["color"]) return {};
      assign("x", heatmapResult["x"]);
      assign("y", heatmapResult["y"]);
      assign("color", heatmapResult["color"]);
      break;
    }
    case "Line Chart": {
      const xField = pickSeriesAxis(tv, used);
      const yField = pickQuantitative(tv, used);
      if (!xField || !yField) return {};
      assign("x", xField);
      assign("y", yField);
      if (!isValidLineSeriesData(tv, xField, void 0)) {
        const colorField = pickLineChartColorField(tv, used, xField, 20) ?? pickLineChartColorField(tv, used, xField, 200);
        if (!colorField) return {};
        assign("color", colorField);
      }
      break;
    }
    case "Boxplot": {
      const xField = pickDiscrete(tv, used);
      const yField = pickQuantitative(tv, used);
      if (!xField || !yField) return {};
      assign("x", xField);
      assign("y", yField);
      break;
    }
    case "Pie Chart": {
      const sizeField = pickQuantitative(tv, used);
      const colorField = pickLowCardDiscrete(tv, used, 12);
      if (!sizeField || !colorField) return {};
      assign("size", sizeField);
      assign("color", colorField);
      break;
    }
    case "Area Chart": {
      const xField = pickSeriesAxis(tv, used);
      const yField = pickQuantitative(tv, used);
      if (!xField || !yField) return {};
      assign("x", xField);
      assign("y", yField);
      assign("color", pickLineChartColorField(tv, used, xField, 20));
      break;
    }
    case "Streamgraph": {
      const streamResult = resolveAssignment(tv, used, [
        {
          channel: "x",
          pref: (_n, ty, st, card) => {
            if (isTimeSeriesType(st)) return Pref.STRONG;
            if (isOrdinalType(st)) return Pref.OK;
            if (isCategoricalType(st)) return Pref.OK;
            if (isNonMeasureNumeric(st)) return Pref.OK;
            if (ty === "temporal") return Pref.STRONG;
            if (ty === "nominal") return Pref.OK;
            if (ty === "quantitative" && card > 0 && card <= 50) return Pref.OK;
            return Pref.EXCLUDE;
          }
        },
        {
          channel: "y",
          pref: (_n, ty, st, card) => {
            if (isMeasureType(st)) return Pref.STRONG;
            if (isTimeSeriesType(st)) return Pref.EXCLUDE;
            if (isCategoricalType(st)) return Pref.EXCLUDE;
            if (isNonMeasureNumeric(st)) return Pref.EXCLUDE;
            if (ty === "quantitative" && !st)
              return card > 20 ? Pref.STRONG : Pref.OK;
            return Pref.EXCLUDE;
          }
        },
        {
          channel: "color",
          pref: (_n, ty, st, card) => {
            if (isCategoricalType(st)) return Pref.STRONG;
            if (isOrdinalType(st)) return Pref.OK;
            if (isTimeSeriesType(st)) return Pref.OK;
            if (ty === "nominal") return Pref.STRONG;
            if (ty === "temporal" || ty === "ordinal") return Pref.OK;
            if (isDiscreteLike(ty, st, card, 20)) return Pref.WEAK;
            return Pref.EXCLUDE;
          }
        }
      ]);
      if (!streamResult["x"] || !streamResult["y"] || !streamResult["color"]) return {};
      assign("x", streamResult["x"]);
      assign("y", streamResult["y"]);
      assign("color", streamResult["color"]);
      break;
    }
    case "Radar Chart": {
      const xField = pickDiscrete(tv, used) ?? pickLowCardDiscrete(tv, used, 20);
      const yField = pickQuantitative(tv, used);
      if (!xField || !yField) return {};
      assign("x", xField);
      assign("y", yField);
      assign("color", pickLowCardDiscrete(tv, used, 20));
      break;
    }
    case "Candlestick Chart": {
      const xField = pickTemporal(tv, used) ?? pick(tv, used, (name) => nameMatches(name, ["date", "time", "day", "datetime", "timestamp", "period"])) ?? pickQuantitativeByName(tv, used, ["date", "time", "day"]) ?? pickDiscrete(tv, used);
      if (!xField) return {};
      assign("x", xField);
      const openField = pickQuantitativeByName(tv, used, ["open"]);
      const highField = pickQuantitativeByName(tv, used, ["high"]);
      const lowField = pickQuantitativeByName(tv, used, ["low"]);
      const closeField = pickQuantitativeByName(tv, used, ["close"]);
      if (openField && highField && lowField && closeField) {
        assign("open", openField);
        assign("high", highField);
        assign("low", lowField);
        assign("close", closeField);
      } else {
        const quants = pickAllQuantitative(tv, used);
        if (quants.length >= 4) {
          assign("open", quants[0]);
          assign("high", quants[1]);
          assign("low", quants[2]);
          assign("close", quants[3]);
        }
      }
      break;
    }
  }
  return rec;
}

// src/core/chart-type-recommendation.ts
var ID_NAME_PATTERNS = ["id", "index", "idx", "row", "order", "position", "pos"];
function looksLikeIdentifier(name) {
  const lower = name.toLowerCase();
  return ID_NAME_PATTERNS.some((p) => lower === p || lower.endsWith("_" + p));
}
function classifyRole(name, type, semanticType) {
  const st = semanticType;
  if (isGeoCoordinateType(st)) {
    if (st === "Latitude" || nameMatches(name, ["latitude", "lat"])) return "latitude";
    if (st === "Longitude" || nameMatches(name, ["longitude", "lon", "lng", "long"])) return "longitude";
    return "other";
  }
  if (isGeoLocationString(st)) return "geoPlace";
  if (type === "temporal" || isTimeSeriesType(st)) return "temporal";
  if (st === "ID" || looksLikeIdentifier(name)) return "identifier";
  if (type === "quantitative" && !isNonMeasureNumeric(st) && (isMeasureType(st) || st === "")) {
    return "measure";
  }
  if (isOrdinalType(st)) return "ordinal";
  if (type === "nominal" || isCategoricalType(st)) return "categorical";
  return "other";
}
function profileData(data, semanticTypes) {
  const tv = buildTableView(data, semanticTypes);
  const fields = tv.names.map((name) => ({
    name,
    type: tv.fieldType[name] ?? "nominal",
    semanticType: tv.fieldSemanticType[name] ?? "",
    cardinality: tv.fieldLevels[name]?.length ?? 0,
    role: classifyRole(name, tv.fieldType[name] ?? "nominal", tv.fieldSemanticType[name] ?? "")
  }));
  const by = (r) => fields.filter((f) => f.role === r);
  const categoricals = by("categorical");
  const ordinals = by("ordinal");
  const geoPlaces = by("geoPlace");
  const temporals = by("temporal");
  return {
    fields,
    measures: by("measure"),
    temporals,
    categoricals,
    ordinals,
    geoPlaces,
    latitudes: by("latitude"),
    longitudes: by("longitude"),
    identifiers: by("identifier"),
    dimensions: [...categoricals, ...ordinals, ...geoPlaces, ...temporals],
    rowCount: tv.rows.length
  };
}
var LOW_CARD_SERIES = 12;
var LOW_CARD_AXIS = 25;
var PIE_MAX_SLICES = 8;
var CHOROPLETH_SEMANTIC = /* @__PURE__ */ new Set(["Country", "State"]);
var CHOROPLETH_NAME_HINTS = ["country", "state", "province", "nation"];
function rankChartTypes(profile) {
  const {
    measures,
    temporals,
    categoricals,
    ordinals,
    geoPlaces,
    latitudes,
    longitudes,
    rowCount
  } = profile;
  const catLike = [...categoricals, ...ordinals, ...geoPlaces];
  const dims = profile.dimensions;
  const lowCardCatLike = catLike.filter((f) => f.cardinality >= 2 && f.cardinality <= LOW_CARD_SERIES);
  const lowCardDims = dims.filter((f) => f.cardinality >= 2 && f.cardinality <= LOW_CARD_AXIS);
  const hasMeasure = measures.length >= 1;
  const order = [];
  const acc = /* @__PURE__ */ new Map();
  const add = (chartType, score, reason) => {
    const cur = acc.get(chartType);
    if (!cur) {
      order.push(chartType);
      acc.set(chartType, { score, reasons: [reason] });
    } else {
      if (score > cur.score) cur.score = score;
      if (!cur.reasons.includes(reason)) cur.reasons.push(reason);
    }
  };
  if (latitudes.length >= 1 && longitudes.length >= 1) {
    add("Map", 96, "has latitude + longitude coordinates");
  }
  const choroGeo = geoPlaces.filter(
    (f) => CHOROPLETH_SEMANTIC.has(f.semanticType) || nameMatches(f.name, CHOROPLETH_NAME_HINTS)
  );
  if (choroGeo.length >= 1 && hasMeasure) {
    add("Choropleth", 92, "has a geographic region field + a measure");
  }
  if (temporals.length >= 1 && hasMeasure) {
    add("Line Chart", 88, "has a time field + a measure (trend over time)");
    add("Area Chart", 66, "has a time field + a measure");
  }
  if (measures.length >= 2) {
    add("Scatter Plot", 84, "has two or more measures (relationship)");
  }
  if (dims.length >= 1 && hasMeasure) {
    add("Bar Chart", 80, "has a category axis + a measure");
  }
  if (hasMeasure && dims.length === 0) {
    add("Histogram", 82, "has a measure with no category (distribution)");
  }
  if (catLike.length >= 2 && lowCardCatLike.length >= 1 && hasMeasure) {
    add("Grouped Bar Chart", 72, "has two categories + a measure");
    add("Stacked Bar Chart", 70, "has two categories + a measure");
  }
  if (lowCardDims.length >= 2 && hasMeasure) {
    add("Heatmap", 74, "has two discrete fields + a measure (matrix)");
  }
  if (catLike.length === 1 && temporals.length === 0 && hasMeasure) {
    const c = catLike[0];
    const oneRowPerCategory = rowCount <= c.cardinality * 1.5;
    if (c.cardinality >= 2 && c.cardinality <= PIE_MAX_SLICES && oneRowPerCategory) {
      add("Pie Chart", 64, "a few categories that sum to a whole");
    }
  }
  if (catLike.length >= 1 && hasMeasure && temporals.length === 0) {
    const smallest = catLike.reduce((a, b) => b.cardinality < a.cardinality ? b : a);
    if (smallest.cardinality >= 1 && rowCount >= smallest.cardinality * 2) {
      add("Boxplot", 58, "multiple measure values per group (spread)");
      add("Strip Plot", 52, "multiple measure values per group");
    }
  }
  if (!hasMeasure && dims.length >= 1) {
    add("Bar Chart", 60, "categories without a measure (counts)");
    if (lowCardDims.length >= 2) add("Heatmap", 55, "two discrete fields (cross-tab counts)");
  }
  if (order.length === 0) {
    if (measures.length >= 2) add("Scatter Plot", 20, "fallback for numeric data");
    else add("Bar Chart", 15, "fallback");
  }
  return order.map((chartType) => ({ chartType, score: acc.get(chartType).score, reasons: acc.get(chartType).reasons })).sort((a, b) => b.score - a.score);
}
function recommendChartTypesDetailed(data, semanticTypes, options = {}) {
  const profile = profileData(data, semanticTypes);
  let ranked = rankChartTypes(profile);
  if (options.supportedTypes) {
    const allowed = new Set(options.supportedTypes);
    ranked = ranked.filter((s) => allowed.has(s.chartType));
  }
  if (options.max != null) ranked = ranked.slice(0, options.max);
  return ranked;
}
function recommendChartTypes(data, semanticTypes, options = {}) {
  return recommendChartTypesDetailed(data, semanticTypes, options).map((s) => s.chartType);
}

// src/chartjs/recommendation.ts
function cjsAdaptChart(sourceType, targetType, encodings, data, semanticTypes) {
  const targetChannels = cjsGetTemplateChannels(targetType);
  return adaptChannels(sourceType, targetType, targetChannels, encodings, data, semanticTypes);
}
function cjsRecommendEncodings(chartType, data, semanticTypes) {
  const rec = recommendChannels(chartType, data, semanticTypes);
  const validChannels = cjsGetTemplateChannels(chartType);
  const result = {};
  for (const [ch, field] of Object.entries(rec)) {
    if (validChannels.includes(ch)) result[ch] = field;
  }
  return result;
}
var CJS_SUPPORTED_TYPES = cjsAllTemplateDefs.map((d) => d.chart);
function cjsRecommendChartTypes(data, semanticTypes, options = {}) {
  return recommendChartTypes(data, semanticTypes, { ...options, supportedTypes: CJS_SUPPORTED_TYPES });
}
function cjsRecommendCharts(data, semanticTypes, options = {}) {
  const { max, ...rest } = options;
  const charts = cjsRecommendChartTypes(data, semanticTypes, rest).map((chartType) => ({ chartType, encodings: cjsRecommendEncodings(chartType, data, semanticTypes) })).filter((s) => Object.keys(s.encodings).length > 0);
  return max != null ? charts.slice(0, max) : charts;
}

export { assembleChartjs, cjsAdaptChart, cjsAllTemplateDefs, cjsApplyLayoutToSpec, cjsApplyTooltips, cjsGetTemplateChannels, cjsGetTemplateDef, cjsRecommendChartTypes, cjsRecommendCharts, cjsRecommendEncodings, cjsTemplateDefs, getChartjsPivot, getChartjsTransform };
//# sourceMappingURL=index.js.map
//# sourceMappingURL=index.js.map