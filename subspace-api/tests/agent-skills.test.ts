import { expect, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { adminOnlyTools } from "../src/agent-listener";
import { mcpTools } from "../src/mcp-tools";

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
