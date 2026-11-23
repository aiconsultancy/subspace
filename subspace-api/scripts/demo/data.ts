const HOUR_MS = 3600_000;

export function startOfUtcWeek(seedDate: Date) {
  const monday = new Date(seedDate);
  monday.setUTCHours(0, 0, 0, 0);
  monday.setUTCDate(monday.getUTCDate() - ((monday.getUTCDay() + 6) % 7));
  return monday.getTime();
}

const demoWeek = startOfUtcWeek(new Date());

export function dateAt(days: number, hour = 12) {
  return new Date(demoWeek + (days * 24 + hour) * HOUR_MS).toISOString();
}

export const people = [
  { handle: "mira", name: "Mira Fenwick", role: "Flight operations lead" },
  { handle: "tavi", name: "Tavi Quill", role: "Simulator reliability lead" },
  { handle: "sela", name: "Sela Bracken", role: "Console design lead" },
];

export const labels = [
  { name: "bug", color: "#e5484d" },
  { name: "feature", color: "#5b5bd6" },
  { name: "performance", color: "#f76b15" },
  { name: "docs", color: "#30a46c" },
  { name: "design", color: "#ab4aba" },
  { name: "reliability", color: "#008c95" },
];

// Each initiative lists its fleet's Starbase and command channels and the ship
// channels of the captains working on its projects.
export const initiatives = [
  {
    name: "Navigation console",
    fleet: "aurora",
    channels: "#aurora-starbase #aurora-command #aurora-lyra #aurora-orion",
    description: "Get survey crews from route planning to the first verified burn.",
    stakeholders: ["mira", "sela"],
  },
  {
    name: "Warp diagnostics",
    fleet: "borealis",
    channels: "#borealis-starbase #borealis-command #borealis-rigel",
    description:
      "Give operators enough context to distinguish simulator drift from a real incident.",
    stakeholders: ["tavi", "mira"],
  },
  {
    name: "Relay reliability",
    fleet: "aurora",
    channels: "#aurora-starbase #aurora-command #aurora-orion",
    description: "Recover gracefully when trackside relays disappear mid-survey.",
    stakeholders: ["mira", "tavi"],
  },
];

type State =
  | "triage"
  | "backlog"
  | "unstarted"
  | "started"
  | "completed"
  | "canceled"
  | "duplicate";
type Issue = readonly [
  title: string,
  summary: string,
  state: State,
  label: string,
  priority: number,
  parent?: number,
];

type ProjectKey = "NAV" | "WARP" | "MAP" | "RELAY";

type Project = {
  key: ProjectKey;
  name: string;
  initiative: number;
  lead: string;
  captain: string;
  status: "started" | "planned";
  brief: string;
  issues: readonly Issue[];
};

export const projects: Project[] = [
  {
    key: "NAV",
    name: "Navigation console",
    initiative: 0,
    lead: "mira",
    captain: "aurora-lyra",
    status: "started",
    brief: "Pilot the route editor on the Fallow Reach survey.",
    issues: [
      [
        "Render star chart tiles off the UI thread",
        "Panning the chart stalls while tiles decode; move decoding to a worker pool.",
        "started",
        "performance",
        2,
      ],
      [
        "Course correction preview ignores relay drift",
        "The preview path assumes a fixed relay position, so plotted burns land short.",
        "unstarted",
        "bug",
        1,
      ],
      [
        "Add waypoint search to the console",
        "Find waypoints by name or sector from the command palette.",
        "backlog",
        "feature",
        3,
      ],
      [
        "Document the telemetry frame format",
        "Describe each field of the telemetry frame for captains writing new parsers.",
        "completed",
        "docs",
        4,
      ],
      [
        "Retry telemetry uploads with backoff",
        "Dropped uplinks lose a frame; retry with jittered backoff before giving up.",
        "triage",
        "bug",
        2,
      ],
      [
        "Capture a chart-pan baseline on the pilot laptop",
        "Keep the input-latency trace and the 90th-percentile frame time for the review.",
        "completed",
        "performance",
        3,
        1,
      ],
      [
        "Cancel tile decodes that leave the viewport",
        "Drop obsolete tile work when the operator reverses direction.",
        "started",
        "performance",
        2,
        1,
      ],
      [
        "Label unconfirmed burns in the route preview",
        "Use a dashed path and screen-reader label until the simulator acknowledges the plan.",
        "unstarted",
        "design",
        2,
        2,
      ],
      [
        "Retire the legacy route wizard",
        "The pilot route editor can now load every supported waypoint format.",
        "canceled",
        "feature",
        4,
      ],
      [
        "Route editor jumps when panning across a seam",
        "The recorded hop has the same trace as the chart tile stall.",
        "duplicate",
        "bug",
        3,
      ],
      [
        "Tab from a chart marker to the burn summary",
        "Keep focus order aligned with the navigation checklist.",
        "started",
        "design",
        2,
      ],
      [
        "Make the training route available offline",
        "Bundle the synthetic Fallow Reach route for disconnected rehearsal.",
        "backlog",
        "reliability",
        4,
      ],
    ],
  },
  {
    key: "WARP",
    name: "Warp diagnostics",
    initiative: 1,
    lead: "tavi",
    captain: "borealis-rigel",
    status: "started",
    brief: "Bring the simulator alert panel to the flight operations rehearsal.",
    issues: [
      [
        "Alert when plasma temperature trends upwards",
        "Raise an alert when the five-minute trend exceeds the configured threshold.",
        "started",
        "feature",
        2,
      ],
      [
        "Diagnostics report times out on long runs",
        "Reports over six hours of samples exceed the request timeout.",
        "unstarted",
        "bug",
        1,
      ],
      [
        "Export simulator runs as CSV",
        "Let engineers download a run's samples for offline analysis.",
        "backlog",
        "feature",
        3,
      ],
      [
        "Suppress alert chatter during calibration",
        "Tag planned calibration windows without muting independent pressure events.",
        "completed",
        "reliability",
        2,
      ],
      [
        "Record the trace used in alert acceptance tests",
        "Give reviewers one short run that contains both noise and a sustained rise.",
        "completed",
        "docs",
        3,
        1,
      ],
      [
        "Show the time window used by each alert",
        "Operators need to see the window size beside the measured slope.",
        "started",
        "design",
        2,
        1,
      ],
      [
        "Page diagnostics by sample range",
        "Make long reports resumable without transferring the entire run.",
        "started",
        "performance",
        2,
        2,
      ],
      [
        "Distinguish sensor silence from stable temperature",
        "Display a separate status when fewer than three samples arrive.",
        "triage",
        "bug",
        1,
      ],
      [
        "Remove the manual alarm reset command",
        "The control duplicates the acknowledgement action in the alert panel.",
        "canceled",
        "feature",
        4,
      ],
      [
        "Add an empty-state message for new test runs",
        "Link operators to the short synthetic run from the empty chart.",
        "completed",
        "design",
        4,
      ],
      [
        "Describe the simulator replay controls",
        "Document how to rewind a run without resending deploy notices.",
        "backlog",
        "docs",
        4,
      ],
      [
        "Verify alarms after a mid-run reconnect",
        "Exercise the reconnect path at the exact calibration boundary.",
        "unstarted",
        "reliability",
        2,
      ],
    ],
  },
  {
    key: "MAP",
    name: "Sector atlas",
    initiative: 0,
    lead: "sela",
    captain: "aurora-orion",
    status: "planned",
    brief: "Prepare the atlas overlays and review the pilot's visual hierarchy.",
    issues: [
      [
        "Show the survey corridor at every zoom level",
        "Keep the route leg visible crossing the two outer sectors.",
        "started",
        "design",
        2,
      ],
      [
        "Pick atlas label density by zoom band",
        "Avoid name collisions where the survey corridor crosses clustered markers.",
        "unstarted",
        "design",
        2,
        1,
      ],
      [
        "Record the high-contrast map palette",
        "Store approved map swatches and pairings for labels and route geometry.",
        "completed",
        "docs",
        3,
        1,
      ],
      [
        "Detect outdated sector packs at startup",
        "Warn when the cached pack's version precedes the route manifest.",
        "unstarted",
        "reliability",
        2,
      ],
      [
        "Cache the last viewed sector",
        "Return operators to the same bounds on their next shift.",
        "backlog",
        "feature",
        3,
      ],
      [
        "Investigate clipped labels on narrow windows",
        "The axes cover short names when the legend column is visible.",
        "triage",
        "bug",
        2,
      ],
      [
        "Keyboard shortcuts for layer toggles",
        "Make survey corridor, checkpoints and radio shadows independently selectable.",
        "backlog",
        "feature",
        4,
      ],
      [
        "Move the scale bar above the annotation tray",
        "Keep the scale legible while the crew is making notes.",
        "completed",
        "design",
        3,
      ],
      [
        "Reject route manifests with mismatched atlas versions",
        "Return the pack version needed to open the saved route.",
        "started",
        "reliability",
        2,
      ],
      [
        "Drop the separate night palette preview",
        "The theme picker already offers the approved dark map.",
        "canceled",
        "design",
        4,
      ],
      [
        "Compare tile cache hit rates after warm-up",
        "Check whether repeated passes read the local pack as expected.",
        "completed",
        "performance",
        3,
      ],
      [
        "Surface route provenance in the atlas",
        "Show which survey imported a corridor and when it was reviewed.",
        "backlog",
        "feature",
        4,
      ],
    ],
  },
  {
    key: "RELAY",
    name: "Relay handover",
    initiative: 2,
    lead: "mira",
    captain: "aurora-orion",
    status: "started",
    brief: "Rehearse a graceful handover when the far relay stops acknowledging frames.",
    issues: [
      [
        "Recover queued frames after a relay swap",
        "Carry unresolved acknowledgements across a handover without replaying confirmed frames.",
        "started",
        "reliability",
        1,
      ],
      [
        "Add sequence counters to the handover dashboard",
        "Put sent, acknowledged and replayed counts side by side.",
        "completed",
        "feature",
        2,
        1,
      ],
      [
        "Replay a duplicate acknowledgement in the simulator",
        "Capture whether reconnection leads to a second confirmation for the same frame.",
        "started",
        "bug",
        2,
        1,
      ],
      [
        "Cap the retry queue on slow relays",
        "Prefer the newest unsent position frame once the queue reaches its agreed bound.",
        "unstarted",
        "performance",
        2,
      ],
      [
        "Publish the pilot handover runbook",
        "Give the next operator the checks to perform before routing live traffic.",
        "completed",
        "docs",
        3,
      ],
      [
        "Show when the backup relay last responded",
        "Use the last valid acknowledgement in the status panel.",
        "started",
        "design",
        3,
      ],
      [
        "Retry a stale DNS lookup after stand-down",
        "The training resolver can retain an endpoint after the relay disengages.",
        "triage",
        "bug",
        2,
      ],
      [
        "Prototype peer-to-peer relay discovery",
        "Assess whether flight operations still needs a directory during rehearsals.",
        "canceled",
        "feature",
        4,
      ],
      [
        "Limit simultaneous handover candidates",
        "A noisy sector should never schedule more than one new upstream at once.",
        "backlog",
        "reliability",
        3,
      ],
      [
        "Write an operator checklist for clean rollback",
        "Include confirmation of all queued frames before the primary relay takes over.",
        "unstarted",
        "docs",
        2,
      ],
      [
        "Track reconnect time at the 95th percentile",
        "Use handover markers to separate transport delay from console rendering.",
        "backlog",
        "performance",
        3,
      ],
      [
        "Replay edge-case sequence gaps during rollout",
        "Confirm the receiver retains acknowledgements when frames arrive late.",
        "completed",
        "reliability",
        2,
      ],
    ],
  },
];

export const evidence = {
  NAV: {
    file: "navigation-console.png",
    alt: "Navigation console showing the Fallow Reach route and tile decode timings",
    report:
      "On the pilot replay, warm pans held at **41 ms**. The first cross-sector pan still peaked at 87 ms.",
    response:
      "I can confirm the cold-cache stall. The trace points to tile decode; pointer handling stayed below 4 ms.",
    next: "Use the cold pass as the release gate. Keep the image in the issue for the flight review.",
    todos: [
      "Re-run the cold-cache pan",
      "Check that leaving the viewport cancels the decode",
      "Add a trace to the operator handover",
    ],
    code: "atlas replay --route fallow-reach --cache cold",
  },
  WARP: {
    file: "thermal-alerts.png",
    alt: "Thermal alert chart with a sustained crossing of the warning band",
    report:
      "The brief 09:42 crossing was calibration noise. The sustained rise fired the alert around 09:46:45 UTC.",
    previousReport:
      "The 09:42 replay crossed the band twice. The first crossing was noise; the sustained rise came 80 seconds later.",
    response:
      "A five-minute slope plus three increasing samples catches the second crossing with no pilot false positives.",
    next: "Please keep the threshold visible in the dashboard so operations can explain why an alert fired.",
    todos: [
      "Record the 09:42 simulator segment",
      "Replay through a calibration window",
      "Review the threshold with flight operations",
    ],
    code: "warp replay thermal-rise --window 5m",
  },
  MAP: {
    file: "sector-atlas.png",
    alt: "Design mock-up of atlas overlays and label controls",
    report:
      "The new composition keeps the corridor legible in all three zoom bands. Labels still collide at the eastern crossing.",
    response:
      "I will reduce waypoint labels one band earlier. The route and danger overlays can stay pinned.",
    next: "Leave the scale bar in the mock-up and include a narrow-window view in the follow-up.",
    todos: [
      "Review three zoom bands",
      "Check at the narrow window width",
      "Ask the survey crew to confirm label priority",
    ],
    code: "atlas preview --sector fallow-reach --zoom regional",
  },
  RELAY: {
    file: "relay-rollout.png",
    alt: "Deployment dashboard tracking the relay handover rehearsal",
    report:
      "The rehearsal reached 25% with no lost frames. One retransmission appeared in the simulated far-side relay.",
    response:
      "It was a replay after a late acknowledgement. The sequence filter dropped it before delivery.",
    next: "Hold at 25% until the rollback run completes. Add the late acknowledgement trace to the handover notes.",
    todos: [
      "Record the 25% rehearsal",
      "Verify the replay counter after a late acknowledgement",
      "Exercise rollback with a non-empty queue",
    ],
    code: "relay rehearse --traffic 25 --scenario late-ack",
  },
};

export const decisions = [
  {
    key: "NAV",
    title: "Keep two tile workers on the pilot console",
    day: -16,
    status: "accepted",
    decision: "Start with two workers to leave capacity for the simulator.",
    why: "Flight operations runs the pilot on a shared laptop.",
    issues: [1, 6],
  },
  {
    key: "NAV",
    title: "Size the tile pool to the current viewport",
    day: -9,
    status: "accepted",
    decision: "Schedule only visible work and allow up to four decodes when the viewport grows.",
    why: "A bounded adaptive pool reduced panning stalls without holding obsolete frames.",
    issues: [1, 7],
    supersedes: "Keep two tile workers on the pilot console",
  },
  {
    key: "NAV",
    title: "Show drift as a separate route overlay",
    day: -4,
    status: "proposed",
    decision: "Annotate the projected relay position and keep the original burn line visible.",
    why: "Operators need to compare the planned and corrected path in one view.",
    issues: [1, 2, 8],
  },
  {
    key: "WARP",
    title: "Require a sustained rise before alerting",
    day: -11,
    status: "accepted",
    decision: "Use a five-minute slope and three consecutive increases before opening an alert.",
    why: "A single crossing caused excess alerts in the calibration replay.",
    issues: [1, 5, 6],
  },
  {
    key: "MAP",
    title: "Pin the route layer across zoom bands",
    day: -7,
    status: "accepted",
    decision: "Keep the route corridor visible and progressively hide waypoint labels.",
    why: "The survey crew uses the corridor to orient themselves before reading place names.",
    issues: [1, 2],
  },
  {
    key: "RELAY",
    title: "Reject immediate replay on handover",
    day: -6,
    status: "rejected",
    decision: "Replay all pending frames immediately after the backup relay connects.",
    why: "Waiting for the acknowledgement window avoids duplicate traffic at the receivers.",
    issues: [1, 3],
  },
] as const;
