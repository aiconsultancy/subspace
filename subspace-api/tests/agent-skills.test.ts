import { expect, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { adminOnlyTools } from "../src/agent-listener";
import { mcpInstructions, mcpTools, onBehalfMarker } from "../src/mcp-tools";

// The agent skills in the repository root's agent-skills/ teach agents this API's MCP tools.
const root = join(import.meta.dir, "../../agent-skills");
const harnesses = ["claude", "codex", "opencode"];

function skillFiles(directory: string): string[] {
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) return skillFiles(path);
    return name.endsWith(".md") ? [path] : [];
  });
}

test("every MCP tool an agent skill names is in the API's tool catalogue", () => {
  const named = new Map<string, string[]>();
  for (const path of skillFiles(root)) {
    // Helper scripts such as subspace_token.py are files, not tools.
    for (const [name] of readFileSync(path, "utf8").matchAll(/\bsubspace_[a-z_]+\b(?!\.py)/gu))
      named.set(name, [...(named.get(name) ?? []), path]);
  }
  const unknown = [...named.keys()].filter((name) => !Object.hasOwn(mcpTools, name));
  expect(unknown).toEqual([]);
  expect(named.size).toBeGreaterThan(40);
});

test("the agent skills never teach an administrator tool as something an agent does", () => {
  for (const path of skillFiles(root))
    for (const name of adminOnlyTools) expect(readFileSync(path, "utf8")).not.toContain(name);
});

test("each harness's tracker skill shares the same rules", () => {
  const shared = harnesses.map((harness) => {
    const text = readFileSync(join(root, harness, "subspace-tracker/SKILL.md"), "utf8");
    const match = /<!-- shared:start -->[\s\S]*<!-- shared:end -->/u.exec(text);
    expect(match).not.toBeNull();
    return match![0];
  });
  expect(new Set(shared).size).toBe(1);
});

// MCP clients may cut long server instructions and tool descriptions (Claude Code keeps about the
// first 2 KB of each), so the filing rules must fit whole.
const clientLimit = 2048;

test("the MCP server's instructions and tool descriptions fit whole and name only an agent's tools", () => {
  const texts: [string, string][] = [
    ["instructions", mcpInstructions],
    ...Object.entries(mcpTools).map(([name, tool]): [string, string] => [name, tool.description]),
  ];
  for (const [owner, text] of texts) {
    expect([owner, text.length <= clientLimit]).toEqual([owner, true]);
    for (const [name] of text.matchAll(/\bsubspace_[a-z_]+\b/gu)) {
      expect([owner, name, Object.hasOwn(mcpTools, name)]).toEqual([owner, name, true]);
      expect([owner, name, adminOnlyTools.has(name)]).toEqual([owner, name, false]);
    }
  }
});

test("the MCP server states the tracker skill's rules for filing work at the point of use", () => {
  const skill = readFileSync(join(root, "claude/subspace-tracker/SKILL.md"), "utf8");
  const shared = /<!-- shared:start -->[\s\S]*<!-- shared:end -->/u.exec(skill)![0];
  // The skill and the server agree on the marker and on the size of an issue.
  expect(shared).toContain(onBehalfMarker);
  expect(shared).toContain("one working session");
  const filing = {
    instructions: mcpInstructions,
    subspace_create_issue: mcpTools.subspace_create_issue.description,
  };
  for (const [owner, text] of Object.entries(filing)) {
    for (const rule of [
      "subspace-tracker skill",
      "subspace_list_issues",
      "closed and archived",
      "one working session",
      "small finding, review note or follow-up",
      "one issue per area",
      'type "tracker"',
      "parentId",
      "keep trees shallow",
      "direct children",
      'kind "human"',
      onBehalfMarker,
      'kind "agent"',
    ])
      expect([owner, rule, text.includes(rule)]).toEqual([owner, rule, true]);
  }
});
