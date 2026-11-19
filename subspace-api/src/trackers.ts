import { stateTypes } from "./schema";

type State = (typeof stateTypes)[number];

export function trackerState(types: Iterable<State>): State {
  const present = new Set(types);
  if (!present.size) return "backlog";
  const open = (["triage", "backlog", "unstarted"] as const).find((type) => present.has(type));
  const completed = present.has("completed");
  const started = present.has("started");
  const canceled = present.has("canceled") || present.has("duplicate");
  if (!open && !started) return completed ? "completed" : "canceled";
  if (started || completed || canceled) return "started";
  return open ?? "backlog";
}
