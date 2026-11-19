// The inspector beside the map: one panel per tier, built as escaped HTML
// strings from the fleet state. Pure, so tests can render panels in Node.

import { formatBytes, formatDuration, formatLoad, formatPercent, formatUptime, plural } from "./format.js";
import { allCrew, captainLine, crewView, featureRows, idleWatch, labelOf, lookup, shipCounts, shipState, taskTook } from "./model.js";
import { avatar, BRANCH_ICON, chip, esc, goAttrs, issueTiles, since } from "./html.js";

const section = (title, body, count) =>
  body ? `<section class="sec"><h3>${esc(title)}${count !== undefined ? ` <span class="n">${esc(count)}</span>` : ""}</h3>${body}</section>` : "";

const meter = (fraction, tone = "accent") =>
  `<span class="meter t-${tone}" role="img" aria-label="${esc(formatPercent(fraction))}"><i style="--f:${Math.max(0, Math.min(1, Number(fraction) || 0)).toFixed(3)}"></i></span>`;

function header(eyebrow, title, meta) {
  return `<header class="ihead"><p class="eyebrow">${esc(eyebrow)}</p><h2 tabindex="-1">${esc(title)}</h2>${meta ? `<p class="meta mono">${esc(meta)}</p>` : ""}</header>`;
}

function shipRow(model, ship) {
  const detail = model.ships[ship.id];
  const state = shipState(ship, detail);
  const counts = shipCounts(ship, detail);
  const right = ship.available
    ? `${meter(counts.people ? counts.busy / counts.people : 0)}<span class="tl">${counts.busy}/${counts.people}<em>busy</em></span>`
    : chip("unreachable", model.unavailableSince[ship.id], model.now);
  return `<li><button class="row" type="button" ${goAttrs({ level: 1, ship: ship.id })}><span class="dot s-${esc(state)}"></span><span class="grow"><b>${esc(ship.name)}</b><span class="sub">${esc(ship.captain.name)} · ${esc(ship.repository ?? `${ship.captain.transport} · ${ship.captain.backend}`)}</span></span>${right}</button></li>`;
}

function crewRow(model, ship, member, extra = "") {
  const view = crewView(member);
  const branch = view.branch ? `${BRANCH_ICON}${esc(view.branch)}` : "no branch yet";
  return `<li><button class="row" type="button" ${goAttrs({ level: 2, ship: ship.id, crew: member.id })}>${avatar(view.name, view.state)}<span class="grow"><b>${esc(view.name)}${extra}</b><span class="sub mono">${branch}</span></span><span class="tl s-${esc(view.state)}">${since(view.since, model.now)}<em>${esc(labelOf(view.state))}</em></span></button></li>`;
}

function featureList(features) {
  const rows = featureRows(features);
  if (!rows.length) return "";
  const items = rows
    .map((f) => {
      const changed = f.changed ? `<span class="flag" title="Stargate's default is ${f.defaultEnabled ? "on" : "off"}">default ${f.defaultEnabled ? "on" : "off"}</span>` : "";
      const value = f.value ? `<span class="tag">${esc(f.value)}</span>` : "";
      return `<li class="feature${f.enabled ? " on" : ""}"><span class="sw" aria-hidden="true"></span><span class="grow"><b>${esc(f.label)}</b><span class="sub mono">${esc(f.key)}</span></span>${value}${changed}<span class="state">${f.enabled ? "On" : "Off"}</span></li>`;
    })
    .join("");
  return `<ul class="features">${items}</ul>`;
}

function hostSection(model) {
  const { host } = model.fleet;
  const usage = host.cpu.usage;
  const used = Math.max(0, host.memory.totalBytes - host.memory.availableBytes);
  const memFraction = host.memory.totalBytes ? used / host.memory.totalBytes : 0;
  const tone = (fraction, warn, alarm) => (fraction > alarm ? "red" : fraction > warn ? "yellow" : "accent");
  const row = (label, value, fraction, detail, toneName, mono = false) =>
    `<li><span class="mhead"><span class="mlabel${mono ? " mono" : ""}" title="${esc(label)}">${esc(label)}</span><span class="mvalue">${value}</span></span>${meter(fraction, toneName)}<span class="mdetail">${detail}</span></li>`;
  const loadFraction = host.cpu.load[0] / Math.max(1, host.cpu.cores);
  const rows = [
    row("CPU", usage === null ? "Sampling…" : esc(formatPercent(usage)), usage ?? 0, `${host.cpu.cores} cores`, tone(usage ?? 0, 0.65, 0.85)),
    row("Load", esc(host.cpu.load[0].toFixed(2)), loadFraction, `<span class="mono">${esc(formatLoad(host.cpu.load))}</span> · 1, 5 and 15 min`, loadFraction > 1 ? "red" : "accent"),
    row("Memory", esc(formatPercent(memFraction)), memFraction, `${esc(formatBytes(used))} used of ${esc(formatBytes(host.memory.totalBytes))}`, tone(memFraction, 0.75, 0.9)),
    ...host.disks.map((disk) => {
      const diskUsed = Math.max(0, disk.totalBytes - disk.freeBytes);
      const fraction = disk.totalBytes ? diskUsed / disk.totalBytes : 0;
      return row(disk.path, esc(formatPercent(fraction)), fraction, `${esc(formatBytes(diskUsed))} used · ${esc(formatBytes(disk.freeBytes))} free of ${esc(formatBytes(disk.totalBytes))}`, tone(fraction, 0.8, 0.9), true);
    }),
  ];
  return section("Host", `<ul class="metrics">${rows.join("")}</ul>`);
}

function systemSection(model) {
  const { host, gateway } = model.fleet;
  const vmUptime = host.uptimeSec + Math.max(0, model.now - host.sampledAt) / 1000;
  const stargateUptime = Math.max(0, model.now - gateway.startedAt) / 1000;
  return section(
    "System",
    `<dl class="kv">
      <dt>VM uptime</dt><dd><span data-uptime="${Math.round(model.now - vmUptime * 1000)}">${esc(formatUptime(vmUptime))}</span></dd>
      <dt>Stargate uptime</dt><dd><span data-uptime="${Math.round(gateway.startedAt)}">${esc(formatUptime(stargateUptime))}</span></dd>
      <dt>Stargate</dt><dd class="mono">${esc(gateway.version)}</dd>
      <dt>OpenCode</dt><dd class="mono">${gateway.opencodeVersion ? esc(gateway.opencodeVersion) : '<span class="muted">Not connected</span>'}</dd>
      <dt>Bun</dt><dd class="mono">${esc(gateway.bunVersion)}</dd>
      <dt>Host</dt><dd class="mono">${esc(host.hostname)} · ${esc(host.platform)}/${esc(host.arch)}</dd>
    </dl>`,
  );
}

function admiralSection(model) {
  const admiral = model.fleet.admiral;
  if (!admiral) return section("Rear admiral", '<p class="prog">No registration has the rear admiral role.</p>');
  const q = admiral.queue ?? {};
  return section(
    "Rear admiral",
    `<div class="who">${avatar(admiral.name, admiral.activity)}<span class="grow"><b>${esc(admiral.name)}</b><span class="sub mono">${esc(admiral.model ?? admiral.backend)} · ${esc(admiral.transport)}</span></span>${chip(admiral.activity, admiral.since, model.now)}</div>
     <p class="prog">${esc(captainLine(admiral))}. Queue: ${(q.running ?? 0) + (q.responding ?? 0)} running · ${q.queued ?? 0} queued · ${q.done ?? 0} done${q.failed ? ` · ${q.failed} failed` : ""}.</p>`,
  );
}

function issuesFor(model, entries) {
  if (!model.issues?.enabled) return [];
  const ids = [];
  for (const entry of entries) for (const id of model.issues.idsFor(entry.member)) if (!ids.includes(id)) ids.push(id);
  return ids;
}

export function fleetPanel(model, { title = null, after = "" } = {}) {
  const { fleet } = model;
  const idle = idleWatch(fleet, model.ships, 5);
  const active = allCrew(fleet, model.ships).filter((entry) => entry.view.state !== "idle" && entry.view.state !== "unprovisioned");
  const ids = issuesFor(model, active);
  const changed = featureRows(fleet.gateway.features).filter((f) => f.changed).length;
  return [
    header("Fleet · L1 · context", title ?? fleet.name, `${title && title !== fleet.name ? `${fleet.name} · ` : ""}${fleet.host.hostname} · Stargate ${fleet.gateway.version}`) + after,
    admiralSection(model),
    hostSection(model),
    systemSection(model),
    section("Ships", `<ul class="rows">${fleet.ships.map((ship) => shipRow(model, ship)).join("")}</ul>`, fleet.ships.length),
    ids.length ? section("Issues in flight", issueTiles(ids, model.issues.get, model.issues.canOpen), ids.length) : "",
    idle.length
      ? section("Idle watch", `<ul class="rows">${idle.map(({ ship, member }) => crewRow(model, ship, member, ` <span class="sub inline">· ${esc(ship.name)}</span>`)).join("")}</ul>`)
      : "",
    section("Feature flags", featureList(fleet.gateway.features), changed ? `${changed} changed` : fleet.gateway.features.length),
  ].join("");
}

export function shipPanel(model, shipId) {
  const { ship, detail } = lookup(model.fleet, model.ships, { ship: shipId });
  if (!ship) return header("Ship · L2 · containers", "Ship not found", "");
  const state = ship.available ? ship.captain.activity : "unreachable";
  const counts = shipCounts(ship, detail);
  const q = ship.captain.queue ?? {};
  let html = header("Ship · L2 · containers", ship.name, `${ship.repository ?? "No GitHub repository"} · ${ship.captain.transport} · ${ship.captain.backend}`);
  html += section(
    "Captain",
    `<div class="who">${avatar(ship.captain.name, state)}<span class="grow"><b>${esc(ship.captain.name)}</b><span class="sub">${esc(ship.available ? captainLine(ship.captain, counts.busy) : "No answer from the gateway")}</span></span>${chip(state, ship.available ? ship.captain.since : model.unavailableSince[ship.id], model.now)}</div>`,
  );
  if (!ship.available) {
    return `${html}<section class="sec warn"><p><b>Crew status unavailable.</b> ${esc(ship.error ?? "Stargate could not read this ship's crew.")} The console shows nothing it cannot read.</p></section>${section("Ship features", featureList(ship.features))}`;
  }
  html += section(
    "Queue",
    `<div class="cells"><div><b>${q.queued ?? 0}</b><span>Queued</span></div><div><b>${(q.running ?? 0) + (q.responding ?? 0)}</b><span>Running</span></div><div><b>${q.done ?? 0}</b><span>Done</span></div></div>${q.failed ? `<p class="prog">${plural(q.failed, "failed job")}.</p>` : ""}`,
  );
  if (detail) {
    html += section("Crew", `<ul class="rows">${detail.crewMembers.map((member) => crewRow(model, ship, member)).join("")}</ul>`, detail.crewMembers.length);
    const ids = issuesFor(model, detail.crewMembers.map((member) => ({ member })));
    if (ids.length) html += section("Issues", issueTiles(ids, model.issues.get, model.issues.canOpen), ids.length);
  } else {
    html += section("Crew", '<p class="prog">Reading crew…</p>');
  }
  html += section("Ship features", featureList(ship.features));
  return html;
}

function gitSection(view) {
  const git = view.git;
  if (!git) return section("Git", '<p class="prog">No checkout yet.</p>');
  if (git.error) return section("Git", `<p class="prog">Git state unavailable: ${esc(git.error)}</p>`);
  const ahead = git.ahead ?? "?";
  const behind = git.behind ?? "?";
  return section(
    "Git",
    `<dl class="kv">
      <dt>Branch</dt><dd class="mono">${git.branch ? `${BRANCH_ICON}${esc(git.branch)}` : "Detached HEAD"}</dd>
      <dt>Head</dt><dd class="mono">${esc(git.head ?? "—")}</dd>
      <dt>Base</dt><dd class="mono">${esc(git.base ?? "—")} · ${ahead} ahead · ${behind} behind</dd>
      <dt>Worktree</dt><dd>${git.changedFiles ? esc(plural(git.changedFiles, "changed file")) : git.changedFiles === 0 ? "Clean" : "—"}</dd>
      <dt>Repository</dt><dd class="mono">${esc(git.repository ?? "Not on GitHub")}</dd>
    </dl>`,
  );
}

function prSection(view) {
  const pr = view.pr;
  if (!pr) return view.git?.branch && view.git.branch !== "main" ? section("Pull request", '<p class="prog">None yet.</p>') : "";
  return section(
    "Pull request",
    `<p class="pr"><span class="mono">#${esc(pr.number)}</span> ${esc(pr.title)}</p><p class="prog">${esc(pr.state)} · checks <span class="ck-${esc(pr.checks)}">${esc(pr.checks)}</span></p>`,
  );
}

function recentList(model, view) {
  if (!view.recent.length) return '<ul class="plain"><li class="sub">None recorded yet</li></ul>';
  const items = view.recent
    .map((task) => {
      const took = taskTook(task);
      const end = task.finishedAt ?? task.updatedAt;
      return `<li><span class="dot s-${esc(task.status)}"></span><span class="grow"><span>${esc(task.summary)}</span><span class="sub">${esc(labelOf(task.status))}${took !== null ? ` · took ${esc(formatDuration(took))}` : ""}${Number.isFinite(end) ? ` · ${since(end, model.now)} ago` : ""}</span></span></li>`;
    })
    .join("");
  return `<ul class="plain">${items}</ul>`;
}

export function crewPanel(model, shipId, memberId) {
  const { ship, member } = lookup(model.fleet, model.ships, { ship: shipId, crew: memberId });
  if (!ship || !member) return header("Crew · L3 · components", "Crew member not found", "");
  const view = crewView(member);
  const ids = model.issues?.enabled ? model.issues.idsFor(member) : [];
  const live = view.task
    ? `<p class="note">${view.observed ? "Live status: observed. The worker reports this task in flight." : "Live status: not checked. Stored state does not prove a live run."}</p>`
    : "";
  const task = view.task
    ? `<p class="task">${esc(view.task.summary)}</p><p><span class="tag">${esc(view.task.phase || labelOf(view.task.status))}</span></p>${view.task.progress ? `<p class="prog">${esc(view.task.progress)}</p>` : ""}`
    : `<p class="prog">No current task.${view.lastOutcome ? ` Last outcome: ${esc(view.lastOutcome.summary)} (${esc(labelOf(view.lastOutcome.status).toLowerCase())}).` : ""}</p>`;
  const projects = view.contexts
    .map((context) => `<li><span class="dot s-${esc(context.activity)}"></span><span class="grow"><b>${esc(context.project ?? "default project")}</b><span class="sub mono">${esc(context.git?.branch ?? "no worktree yet")}${context.queuedCount ? ` · ${context.queuedCount} queued` : ""}${context.provisioned !== "ready" ? ` · ${esc(context.provisioned)}` : ""}</span></span></li>`)
    .join("");
  const more = view.queuedCount - view.queued.length;
  const queued = view.queued.length
    ? `<ol class="plain num">${view.queued.map((entry) => `<li>${esc(entry.summary)}</li>`).join("")}</ol>${more > 0 ? `<p class="prog">and ${more} more</p>` : ""}`
    : "";
  const subagents = view.subagents
    .map((entry) => `<li><span class="dot t-magenta"></span><span class="grow"><span>${esc(entry.title)}</span>${Number.isFinite(entry.updatedAt) ? `<span class="sub">updated ${since(entry.updatedAt, model.now)} ago</span>` : ""}</span></li>`)
    .join("");
  return [
    `<header class="ihead who-big">${avatar(view.name, view.state)}<div><p class="eyebrow">Crew · ${esc(ship.name)} · L3</p><h2 tabindex="-1">${esc(view.name)}</h2><p class="meta">${esc(view.role ?? "Crew member")}</p></div></header>`,
    `<div class="statusline">${chip(view.state, view.since, model.now)}<span class="sub mono">${esc(view.model ?? "default model")} · ${esc(view.backend)}</span></div>`,
    live,
    section("Current task", task),
    ids.length ? section("Issues", issueTiles(ids, model.issues.get, model.issues.canOpen), ids.length) : "",
    gitSection(view),
    prSection(view),
    section("Projects", `<ul class="plain">${projects}</ul>`, view.contexts.length),
    queued ? section("Queued", queued, view.queuedCount) : "",
    subagents ? section("Subagents", `<ul class="plain">${subagents}</ul>`, view.subagents.length) : "",
    section("Recent outcomes", recentList(model, view)),
  ].join("");
}

/** The tooltip for a hit region on the map. */
export function tooltip(model, hit) {
  const now = model.now;
  if (hit.kind === "ship") {
    const { ship, detail } = lookup(model.fleet, model.ships, { ship: hit.ship });
    if (!ship) return "";
    const counts = shipCounts(ship, detail);
    const line = ship.available ? `Captain ${esc(ship.captain.name)} · ${counts.busy} of ${counts.people} busy` : esc(ship.error ?? "Unavailable");
    return `<b>${esc(ship.name)}</b><span>${line}</span><em>Click to warp in</em>`;
  }
  if (hit.kind === "admiral") {
    const admiral = model.fleet.admiral;
    return admiral ? `<b>Rear admiral ${esc(admiral.name)}</b><span>${esc(captainLine(admiral))} · ${esc(admiral.model ?? admiral.backend)}</span>` : "";
  }
  if (hit.kind === "captain") {
    const { ship } = lookup(model.fleet, model.ships, { ship: hit.ship });
    return ship ? `<b>Captain ${esc(ship.captain.name)}</b><span>${esc(ship.available ? captainLine(ship.captain) : "No answer from the gateway")}</span>` : "";
  }
  if (hit.kind === "crew") {
    const { member } = lookup(model.fleet, model.ships, { ship: hit.ship, crew: hit.crew });
    if (!member) return "";
    const view = crewView(member);
    return `<b>${esc(view.name)}</b><span>${esc(labelOf(view.state))} · ${esc(formatDuration(now - (view.since ?? now)))}${view.branch ? ` · ${esc(view.branch)}` : ""}</span>${view.task ? `<span>${esc(view.task.summary)}</span>` : ""}<em>Click to warp in</em>`;
  }
  if (hit.kind === "ctx") {
    const context = hit.ref;
    return `<b>${esc(context.project ?? "default project")}</b><span class="mono">${esc(context.git?.branch ?? "no worktree yet")}</span><span>${esc(context.current?.summary ?? "No task")}</span>`;
  }
  if (hit.kind === "sub") return `<b>Subagent</b><span>${esc(hit.name)}</span>`;
  if (hit.kind === "recent") {
    const task = hit.ref;
    const took = taskTook(task);
    const end = task.finishedAt ?? task.updatedAt;
    return `<b>${esc(task.summary)}</b><span>${esc(labelOf(task.status))}${took !== null ? ` · took ${esc(formatDuration(took))}` : ""}${Number.isFinite(end) ? ` · ${esc(formatDuration(now - end))} ago` : ""}</span>`;
  }
  return "";
}
