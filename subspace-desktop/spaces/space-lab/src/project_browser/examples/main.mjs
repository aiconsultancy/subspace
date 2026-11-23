// Simulated data only: this example reads no host metrics and uses no network.
// dashboard.json binds output IDs to results and control IDs to accepted values.
// Saved working source runs only after Start or Apply & Restart approves its
// snapshot. Private settings are separate and excluded from that snapshot.
const history = [];
let sampleNumber = 0;
let timer = null;

function sample() {
  sampleNumber += 1;
  // Predictable waves show changes without requesting host observation rights.
  history.push([sampleNumber, Math.round(50 + 25 * Math.sin(sampleNumber / 5)),
    Math.round(60 + 15 * Math.sin(sampleNumber / 8)),
    Math.round(35 + 10 * Math.sin(sampleNumber / 11))]);
  if (history.length > 60) history.shift();
}

function chart(kind, title, labels, series) {
  return { kind, orientation: "vertical", stacked: false, title,
    subtitle: "SIMULATED — not host measurements", labels,
    categoryAxisTitle: "Sample", valueAxisTitle: "%", series,
    xScale: "number", legend: true, format: "percent" };
}

function resultPatch(context) {
  // A result targets an output ID and carries scope, time and typed value.
  // Category values match labels; a doughnut has one series per slice.
  const stamp = Date.now();
  const output = value => ({ scope: "simulated", evaluated_at_ms: stamp, value });
  const [, cpu, ram, disk] = history[history.length - 1];
  const labels = history.map(row => String(row[0]));
  const results = {
    summary: { type: "text", text: `SIMULATED CPU ${cpu}% · RAM ${ram}% · disk ${disk}% | ${context.values.mode ?? "Running"} | ${history.length} samples` },
    utilization: { type: "chart", chart: chart("doughnut", "SIMULATED CPU", ["Used", "Free"], [{ label: "Used", values: [cpu] }, { label: "Free", values: [100 - cpu] }]) },
    history: { type: "chart", chart: chart("line", "SIMULATED CPU history", labels, [{ label: "CPU", values: history.map(row => row[1]) }]) },
    memory: { type: "chart", chart: chart("area", "SIMULATED RAM history", labels, [{ label: "RAM", values: history.map(row => row[2]) }]) },
    comparison: { type: "chart", chart: chart("bar", "SIMULATED utilization", ["CPU", "RAM", "Disk"], [{ label: "Utilization", values: [cpu, ram, disk] }]) },
    rows: { type: "table", table: { columns: ["Sample", "SIMULATED CPU %", "SIMULATED RAM %", "SIMULATED disk %"], rows: [...history].reverse() } },
  };
  return { results: Object.fromEntries(Object.entries(results).map(([id, value]) => [id, output(value)])) };
}

function schedule(context) {
  // Exactly one timeout chain exists. Clearing it before rescheduling makes
  // pause immediate and applies a new rate without duplicate timers.
  if (timer !== null) clearTimeout(timer);
  timer = null;
  if ((context.values.mode ?? "Running") !== "Running") return;
  timer = setTimeout(() => {
    sample();
    try {
      context.publish(resultPatch(context));
      schedule(context);
    } catch (error) {
      // Timer exceptions are outside callback dispatch; report on stderr only.
      console.error("SIMULATED dashboard publication failed:", error);
    }
  }, Number(context.values.rate ?? 1) * 1000);
}

export function on_init(context, _event) {
  sample(); // A visible first result, even before the first timer fires.
  schedule(context);
  return resultPatch(context);
}

export function on_input(context, _event) {
  // mode is Running/Paused; rate is accepted seconds within the declared range.
  schedule(context);
  return resultPatch(context);
}

export function on_action(context, event) {
  if (event.action === "reset") {
    history.length = 0;
    sample();
  }
  return resultPatch(context);
}

// There is no on_stop callback: the helper exits the process on Stop. Callback
// errors are reported by the helper. Restart starts a new process and history.
