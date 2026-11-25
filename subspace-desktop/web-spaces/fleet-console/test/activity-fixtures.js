// Officer activity in the shapes of Stargate's V1AgentActivity (#355) and the
// `agent-activity` event. Every name and value is invented.

export const T0 = 1_790_000_000_000;
export const GENERATION = "gen-example-1";

export const tokens = (input, output, reasoning = 0, cacheRead = 0, cacheWrite = 0) => ({ input, output, reasoning, cacheRead, cacheWrite });

export function stepItem(id, seq, fields = {}) {
  return { id, seq, startedAt: T0, updatedAt: T0 + 5, kind: "step", status: "completed", model: { provider: "example", id: "model-large", variant: null }, firstTokenAt: T0 + 1, streamedAt: T0 + 4, endedAt: T0 + 5, finish: "stop", tokens: tokens(1200, 80, 10, 900, 0), cost: 0.0123, error: null, ...fields };
}

export function toolItem(id, seq, fields = {}) {
  return {
    id,
    seq,
    startedAt: T0 + 10,
    updatedAt: T0 + 20,
    kind: "tool",
    callId: `call_${id}`,
    name: "bash",
    status: "completed",
    rawInput: '{"command":"ls"}',
    input: { command: "ls" },
    executed: true,
    metadata: { output: "README.md\nsrc\n" },
    content: [{ type: "text", text: "README.md\nsrc\n" }],
    error: null,
    ...fields,
  };
}

export function textItem(id, seq, text, fields = {}) {
  return { id, seq, startedAt: T0 + 30, updatedAt: T0 + 31, kind: "text", status: "completed", text, ...fields };
}

export function turn(id, seq, fields = {}) {
  return { id, seq, status: "completed", startedAt: T0, updatedAt: T0 + 60, endedAt: T0 + 60, reconnects: 0, usage: { steps: 1, tokens: tokens(1200, 80, 10, 900, 0), cost: 0.0123 }, items: [], ...fields };
}

/** A snapshot: one ended turn and one running one, with a paged tool call. */
export function activitySnapshot({ seq = 9, generation = GENERATION, collected = true } = {}) {
  return {
    agent: { id: "example-captain", role: "captain", backend: collected ? "opencode" : "claude", collected },
    turns: collected
      ? [
          turn("turn-a", 4, { items: [stepItem("i1", 1), toolItem("i2", 2), textItem("i3", 3, "Done.")] }),
          turn("turn-b", 6, {
            status: "running",
            endedAt: null,
            usage: null,
            items: [
              stepItem("i1", 7, { status: "running", tokens: null, cost: null, endedAt: null, finish: null }),
              { id: "i2", seq: 9, startedAt: T0 + 70, updatedAt: T0 + 80, kind: "tool", status: "completed", callId: "call_big", name: "read", paged: { pages: 2, bytes: 70_000 } },
            ],
          }),
        ]
      : [],
    cursor: { generation, seq },
  };
}

/** `agent-activity` event data. */
export function activityEvent(seq, change, { agentId = "example-captain", turnId = "turn-b", generation = GENERATION } = {}) {
  return { agentId, turnId, generation, seq, at: T0 + seq, change };
}
