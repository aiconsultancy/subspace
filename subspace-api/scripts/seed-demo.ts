// Seeds fictional demo data for the local stack through the public HTTP API:
// two fleets on the local IRC server, an initiative and a project for each,
// labels, issues and initiative stakeholders. It signs in with development
// auth as the desktop's default identity, so that user owns the data and sees
// it in the initiative picker. Running it again leaves existing records alone.
//
// bun scripts/seed-demo.ts
//
// SEED_API_URL defaults to http://127.0.0.1:3000. SEED_IRC_HOST,
// SEED_IRC_PLAIN_PORT and SEED_IRC_TLS_PORT name the IRC endpoints that
// desktops use, defaulting to 127.0.0.1, 6667 and 6697.

export {};

const api = (process.env.SEED_API_URL ?? "http://127.0.0.1:3000").replace(/\/+$/, "");
const ircHost = process.env.SEED_IRC_HOST ?? "127.0.0.1";
const plainPort = port("SEED_IRC_PLAIN_PORT", 6667);
const tlsPort = port("SEED_IRC_TLS_PORT", 6697);
const identity = {
  email: process.env.SEED_EMAIL ?? "developer@example.com",
  name: process.env.SEED_NAME ?? "Local Developer",
  organisation: process.env.SEED_ORGANISATION ?? "local",
};

function port(name: string, fallback: number) {
  const value = Number(process.env[name] ?? fallback);
  if (!Number.isInteger(value) || value < 1 || value > 65535) throw new Error(`${name} is invalid`);
  return value;
}

type Json = Record<string, any>;
let token = "";

async function call(method: string, path: string, body?: unknown, allow404 = false) {
  const response = await fetch(`${api}/api/v1${path}`, {
    method,
    headers: {
      Accept: "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (allow404 && response.status === 404) return null;
  const text = await response.text();
  if (!response.ok) throw new Error(`${method} ${path} returned ${response.status}: ${text}`);
  return text ? (JSON.parse(text) as Json) : {};
}

async function all(path: string) {
  const items: Json[] = [];
  let cursor: string | undefined;
  do {
    const separator = path.includes("?") ? "&" : "?";
    const page = (await call(
      "GET",
      `${path}${separator}limit=100${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ""}`,
    ))!;
    items.push(...page.items);
    cursor = page.nextCursor ?? undefined;
  } while (cursor);
  return items;
}

const fleets = [
  {
    slug: "aurora",
    name: "Aurora fleet",
    ircHost,
    ircPort: plainPort,
    ircTls: false,
    rearAdmiralName: "Rear Admiral Vega",
    captainNames: ["Captain Lyra", "Captain Orion"],
  },
  {
    slug: "borealis",
    name: "Borealis fleet",
    ircHost,
    ircPort: tlsPort,
    ircTls: true,
    rearAdmiralName: "Rear Admiral Polaris",
    captainNames: ["Captain Rigel"],
  },
];

const initiatives = [
  {
    name: "Navigation console",
    fleet: "aurora",
    channels: "#aurora-nav #aurora-command",
    description: "Course plotting, star charts and the navigation console's live telemetry.",
    stakeholders: [
      { name: "Product Owner", role: "Accepts navigation features", slackHandle: "@nav-product" },
      { name: "Flight Operations", role: "Signs off releases", teamsHandle: "flight-ops" },
    ],
    project: { key: "NAV", name: "Navigation console" },
  },
  {
    name: "Warp diagnostics",
    fleet: "borealis",
    channels: "#borealis-warp #borealis-command",
    description: "Diagnostics, alerting and reports for the warp core simulator.",
    stakeholders: [
      { name: "Engineering Lead", role: "Owns the diagnostics roadmap", slackHandle: "@warp-lead" },
    ],
    project: { key: "WARP", name: "Warp diagnostics" },
  },
];

const labels = [
  { name: "bug", color: "#e5484d" },
  { name: "feature", color: "#5b5bd6" },
  { name: "performance", color: "#f76b15" },
  { name: "docs", color: "#30a46c" },
];

// state is a workflow state type; priority 1 is urgent and 4 is low.
const issues: Record<string, Json[]> = {
  NAV: [
    {
      title: "Render star chart tiles off the UI thread",
      summary: "Panning the chart stalls while tiles decode; move decoding to a worker pool.",
      state: "started",
      priority: 2,
      labels: ["performance"],
      estimate: 3,
    },
    {
      title: "Course correction preview ignores relay drift",
      summary: "The preview path assumes a fixed relay position, so plotted burns land short.",
      state: "unstarted",
      priority: 1,
      labels: ["bug"],
      estimate: 2,
    },
    {
      title: "Add waypoint search to the console",
      summary: "Find waypoints by name or sector from the command palette.",
      state: "backlog",
      priority: 3,
      labels: ["feature"],
    },
    {
      title: "Document the telemetry frame format",
      summary: "Describe each field of the telemetry frame for captains writing new parsers.",
      state: "completed",
      priority: 4,
      labels: ["docs"],
    },
    {
      title: "Retry telemetry uploads with backoff",
      summary: "Dropped uplinks lose a frame; retry with jittered backoff before giving up.",
      state: "triage",
      priority: 2,
      labels: ["bug"],
    },
  ],
  WARP: [
    {
      title: "Alert when plasma temperature trends upwards",
      summary: "Raise an alert when the five-minute trend exceeds the configured threshold.",
      state: "started",
      priority: 2,
      labels: ["feature"],
      estimate: 5,
    },
    {
      title: "Diagnostics report times out on long runs",
      summary: "Reports over six hours of samples exceed the request timeout.",
      state: "unstarted",
      priority: 1,
      labels: ["bug", "performance"],
    },
    {
      title: "Export simulator runs as CSV",
      summary: "Let engineers download a run's samples for offline analysis.",
      state: "backlog",
      priority: 3,
      labels: ["feature"],
    },
  ],
};

const { accessToken } = (await call("POST", "/auth/dev/token", identity))!;
token = accessToken;
const me = (await call("GET", "/me"))!;
console.log(
  `Signed in as ${me.displayName} (${identity.email}) in organisation ${identity.organisation}`,
);

const fleetIds = new Map<string, string>();
for (const { slug, ...fields } of fleets) {
  const existing = await call("GET", `/fleets/${slug}`, undefined, true);
  const fleet = existing
    ? await call("PATCH", `/fleets/${slug}`, fields)
    : await call("POST", "/fleets", { slug, ...fields });
  fleetIds.set(slug, fleet!.id);
  console.log(
    `Fleet ${slug}: IRC ${fields.ircTls ? "ircs" : "irc"}://${fields.ircHost}:${fields.ircPort}`,
  );
}

let labelRows = await all("/labels?includeArchived=true");
for (const label of labels) {
  if (!labelRows.some((row) => row.name === label.name)) {
    labelRows.push((await call("POST", "/labels", label))!);
  }
}

const existingInitiatives = await all("/initiatives");
for (const spec of initiatives) {
  let initiative = existingInitiatives.find((row) => row.name === spec.name);
  if (!initiative) {
    initiative = (await call("POST", "/initiatives", {
      name: spec.name,
      ownerId: me.userId,
      description: spec.description,
      status: "active",
      channelConfiguration: { channels: spec.channels, memberUserIds: [me.userId] },
    }))!;
    for (const stakeholder of spec.stakeholders) {
      await call("POST", `/initiatives/${initiative.id}/stakeholders`, stakeholder);
    }
  }
  const fleetId = fleetIds.get(spec.fleet)!;
  if (initiative.fleetId !== fleetId) {
    await call("PUT", `/initiatives/${initiative.id}/fleet`, { fleetId });
  }

  let project = await call("GET", `/projects/${spec.project.key}`, undefined, true);
  if (!project) {
    project = (await call("POST", "/projects", {
      key: spec.project.key,
      name: spec.project.name,
      leadId: me.userId,
      status: "started",
    }))!;
  }
  if (!project) throw new Error(`Project ${spec.project.key} could not be created`);
  if (project.initiativeId !== initiative.id) {
    project = (await call("PUT", `/projects/${spec.project.key}/initiative`, {
      initiativeId: initiative.id,
      version: project.version,
    }))!;
  }
  if (project.estimateScale === "notUsed") {
    project = (await call("PUT", `/projects/${spec.project.key}/estimate-scale`, {
      scale: "fibonacci",
      version: project.version,
    }))!.project;
  }

  const projectId = project!.id as string;
  const page = (await call(
    "GET",
    `/issues?project=${spec.project.key}&includeArchived=true&limit=1`,
  ))!;
  if (page.items.length === 0) {
    const states = (await call("GET", `/projects/${spec.project.key}/workflow-states`))!
      .items as Json[];
    for (const issue of issues[spec.project.key] ?? []) {
      const state = states.find((row) => row.type === issue.state) ?? states[0]!;
      await call("POST", "/issues", {
        projectId,
        title: issue.title,
        summary: issue.summary,
        stateId: state.id,
        priority: issue.priority,
        estimate: issue.estimate ?? null,
        assigneeId: issue.state === "backlog" ? null : me.userId,
        labelIds: labelRows
          .filter((row) => (issue.labels as string[]).includes(row.name))
          .map((row) => row.id),
      });
    }
  }
  console.log(
    `Initiative "${spec.name}" (${spec.channels}) on fleet ${spec.fleet}, project ${spec.project.key}`,
  );
}
