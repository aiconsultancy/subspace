import type { SeedIssue } from "./api";

// Each fleet's standard channels: people talk to the rear admiral in Starbase,
// the rear admiral and the captains coordinate in command, and people talk to
// a captain in its ship channel, #<fleet>-<captain-id>.
export function conversations(projectIssues: Map<string, SeedIssue[]>, screenshot: string) {
  function ref(key: string, number = 1) {
    return projectIssues.get(key)![number - 1]!.identifier;
  }

  return {
    "#subspace": [
      [
        "developer",
        "The Fallow Reach rehearsal workspace is ready. Each fleet has a Starbase, a command channel and a channel for each captain.",
      ],
      [
        "aurora-admiral",
        `Start with ${ref("NAV")} for the chart-pan replay. We will collect handover notes in #aurora-starbase.`,
      ],
      [
        "borealis-rigel",
        `The thermal replay is in ${ref("WARP")}. The first spike is intentional.`,
      ],
    ],
    "#aurora-starbase": [
      ["developer", "Flight operations has a 20-minute window for the pilot handover on Thursday."],
      [
        "aurora-admiral",
        `Navigation first, relay second. I have asked aurora-orion to hold the relay rollout at 25% until ${ref("RELAY", 3)} is reviewed.`,
      ],
    ],
    "#aurora-command": [
      [
        "aurora-admiral",
        `Navigation first, relay second. @aurora-orion Hold the rollout at 25% until ${ref("RELAY", 3)} is reviewed.`,
      ],
      [
        "aurora-orion",
        `Understood. The checklist is on ${ref("RELAY", 10)} and the rehearsal guide is https://example.com/fallow-reach/relay.`,
      ],
    ],
    "#aurora-lyra": [
      [
        "developer",
        `@aurora-lyra Can you rerun ${ref("NAV")} with a cold tile cache before the pilot review?`,
      ],
      [
        "aurora-lyra",
        "Yes. Run `atlas replay --route fallow-reach --cache cold`. The first frame is the only slow one.",
      ],
      ["aurora-lyra", `Here is the route and the decode trace from ${ref("NAV")}: ${screenshot}`],
      [
        "aurora-orion",
        `That still leaves the eastern label collision in ${ref("MAP", 2)}. I will bring the regional zoom mock-up.`,
      ],
      [
        "aurora-admiral",
        `Keep the corridor pinned for the pilot. Put the viewport decision and the cold-pass numbers on ${ref("NAV")}.`,
      ],
    ],
    "#aurora-orion": [
      [
        "aurora-orion",
        `${ref("RELAY")}: 25% of rehearsal traffic switched to the backup relay. All 240 frames were accounted for.`,
      ],
      ["developer", "Was the single replay a lost frame or a late acknowledgement?"],
      [
        "aurora-orion",
        "Late acknowledgement. `relay rehearse --traffic 25 --scenario late-ack` reproduces it without dropping a frame.",
      ],
      [
        "aurora-admiral",
        `Hold at 25%. Verify rollback with a non-empty queue in ${ref("RELAY", 10)} before increasing traffic.`,
      ],
    ],
    "#borealis-starbase": [
      [
        "borealis-admiral",
        "Deployment pilot-simulator #101 completed. No independent pressure events were suppressed.",
      ],
      [
        "developer",
        `I will run the longer diagnostics report before sign-off. The timeout is tracked in ${ref("WARP", 2)}.`,
      ],
    ],
    "#borealis-command": [
      [
        "borealis-admiral",
        `@borealis-rigel The longer diagnostics report runs before sign-off. Its timeout is tracked in ${ref("WARP", 2)}.`,
      ],
      [
        "borealis-rigel",
        "Keep the six-hour run attached to the report. It has the time range used in the pilot.",
      ],
    ],
    "#borealis-rigel": [
      [
        "developer",
        `@borealis-rigel Is the 09:42 spike in ${ref("WARP")} calibration noise or a real rise?`,
      ],
      [
        "borealis-rigel",
        "The 09:42 crossing is noise. `warp replay thermal-rise --window 5m` shows the sustained rise firing the alert around 09:46:45 UTC.",
      ],
      [
        "borealis-admiral",
        `Use the five-minute slope for the pilot. Can we show the window next to the alarm? See ${ref("WARP", 6)}.`,
      ],
      [
        "borealis-rigel",
        "Yes. The next replay will include the new label and a pressure event during calibration.",
      ],
    ],
  };
}
