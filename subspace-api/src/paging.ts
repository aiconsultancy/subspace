import { createHash } from "node:crypto";
import { Effect } from "effect";
import { problem } from "./problems";

export function page<T>(
  ordered: readonly T[],
  filter: object,
  key: (item: T) => string,
  limit = 50,
  cursor?: string,
) {
  return Effect.map(cursorAnchor(filter, cursor), (anchor) => {
    const after = anchor === undefined ? ordered : ordered.filter((item) => key(item) > anchor);
    const items = after.slice(0, limit);
    const last = items.at(-1);
    const nextCursor =
      last !== undefined && after.length > limit ? cursorAfter(filter, key(last)) : null;
    return { items, nextCursor };
  });
}

export function cursorAnchor(filter: object, cursor?: string) {
  // The sort key gives live continuations even if the anchor row disappears.
  let anchor: string | undefined;
  if (cursor !== undefined) {
    try {
      if (cursor.length > 4096 || !/^[A-Za-z0-9_-]+$/.test(cursor)) throw new Error();
      const data = JSON.parse(Buffer.from(cursor, "base64url").toString("utf8")) as unknown;
      if (
        !Array.isArray(data) ||
        data.length !== 2 ||
        data[0] !== cursorScope(filter) ||
        typeof data[1] !== "string"
      )
        throw new Error();
      anchor = data[1];
    } catch {
      return Effect.fail(problem(400, "The cursor is invalid for these filters."));
    }
  }
  return Effect.succeed(anchor);
}

export function byKey<T>(key: (item: T) => string) {
  return (left: T, right: T) => {
    const a = key(left);
    const b = key(right);
    return a < b ? -1 : a > b ? 1 : 0;
  };
}

function cursorScope(filter: object) {
  const canonical = Object.fromEntries(
    Object.entries(filter).sort(([a], [b]) => a.localeCompare(b)),
  );
  return createHash("sha256").update(JSON.stringify(canonical)).digest("base64url");
}

function encodeCursor(scope: string, key: string) {
  return Buffer.from(JSON.stringify([scope, key])).toString("base64url");
}

export const cursorAfter = (filter: object, key: string) => encodeCursor(cursorScope(filter), key);
