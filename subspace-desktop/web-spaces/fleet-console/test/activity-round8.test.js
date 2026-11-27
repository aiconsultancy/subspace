// Round 8 of https://github.com/aiconsultancy/subspace/pull/115 (SUB
// decision cf21d510): Phobos's round 7 delta probes, rebuilt as tests,
// with the cases around them, each named for the finding it checks.

import assert from "node:assert/strict";
import { test } from "node:test";
import { PART_MAX_BYTES } from "../src/activity.js";
import { servedPart, servedUnit } from "../src/document.js";
import { itemDocument, measureSource } from "../src/activity-view.js";
import { activityEvent, textItem, toolItem } from "./activity-fixtures.js";
import { officer, pagedWith, prepare, settle, windowsIn } from "./activity-helpers.js";

const TURN = "turn-b";
const PAGING = { pageBytes: 65536, partPages: 4 };
/** A live item that grows the pinned inline charge by `size` bytes. */
const growth = (size, seq = 10) => activityEvent(seq, { type: "item", item: textItem(`growth-${seq}`, seq, "g".repeat(size)) });

// ---------- P2-1: inline growth plans every refit against the final floor ----------

test("r8 1 (probe): inline growth that needs both displayed wholes in parts turns both into pagers, each sending its first page", async () => {
  const a = pagedWith("a".repeat(600 * 1024), "ia");
  const b = pagedWith("b".repeat(600 * 1024), "ib");
  a.snapshot.turns[1].items.push(b.snapshot.turns[1].items[0]);
  const { watcher, client, panel } = await prepare(a.snapshot, 2 * (a.pages[0].bytes + b.pages[0].bytes) + 20_000);
  try {
    const pagesFor = (id) => (id === "ia" ? a.pages : b.pages);
    client.activityItemPage = async (agent, turn, id, page) => pagesFor(id)[page];
    await watcher.loadItem(TURN, "ia");
    await watcher.loadItem(TURN, "ib");
    panel.update(watcher.view, officer);
    assert.equal(windowsIn(panel.root).length, 2);
    assert.equal(watcher.view.budget.requested, `${TURN}\u0000ib`);
    const requested = [];
    client.activityItemPage = async (agent, turn, id, page) => {
      requested.push([id, page]);
      return pagesFor(id)[page];
    };
    watcher.event(growth(1400 * 1024));
    await settle();
    await settle();
    const { budget } = watcher.view;
    assert.ok(budget.limit - budget.bytes("inline") > 2 * (PART_MAX_BYTES + 65536), "both handovers fit once both wholes are in parts");
    assert.equal(watcher.view.full(TURN, "ib")?.mode, "parts");
    assert.deepEqual({ mode: watcher.view.full(TURN, "ia")?.mode, sent: requested.some(([id]) => id === "ia") }, { mode: "parts", sent: true }, "A did not go back to Load in full");
    assert.ok(budget.used <= budget.limit);
  } finally {
    watcher.stop();
  }
});

// ---------- P2-2: a window inside a text part's other fields maps to that field ----------

test("r8 2 (probe): a window inside a text part's annotation opens the served part and unit that hold it", () => {
  const item = toolItem("ia", 9, { content: [{ type: "text", text: "t".repeat(300_000), annotation: { payload: "a".repeat(210_000) } }] });
  const doc = itemDocument(item);
  const json = JSON.stringify(item);
  const unit = measureSource({ kind: "whole", item }).bounds[1];
  const raw = json.indexOf("aaaaaaaa") + unit - doc.indexOf("aaaaaaaa");
  assert.equal(doc[unit], "a", "section 2 starts inside annotation.payload");
  assert.equal(json[raw], "a");
  assert.equal(servedUnit(item, unit), raw);
  assert.deepEqual(servedPart(item, servedUnit(item, unit), PAGING), { part: 1, unit: 103410 });
});

test("r8 2: every character of a text part's other fields, and of its text, maps to itself in the item as served", () => {
  const item = toolItem("ia", 9, { content: [{ type: "text", text: "T-start-" + "t".repeat(40) + "-T-end", annotation: { payload: "A-start-" + "a".repeat(40) + "-A-end", n: 7 }, cache: "C-mark" }] });
  const doc = itemDocument(item);
  const json = JSON.stringify(item);
  for (const mark of ["T-start", "-T-end", "A-start", "-A-end", "C-mark"]) {
    const at = doc.indexOf(mark);
    assert.ok(at >= 0, mark);
    for (let i = 0; i < mark.length; i += 1) assert.equal(json[servedUnit(item, at + i)], mark[i], `${mark}[${i}]`);
  }
});
