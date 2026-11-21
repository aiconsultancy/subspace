import { sql, type SQLWrapper } from "drizzle-orm";
import { createHash } from "node:crypto";
import { Effect } from "effect";
import { problem, validationProblem } from "./problems";
import { isUuid } from "./validation";

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

function encodeScopedCursor(scope: string, key: string) {
  return Buffer.from(JSON.stringify([scope, key])).toString("base64url");
}

export const cursorAfter = (filter: object, key: string) =>
  encodeScopedCursor(cursorScope(filter), key);

export function cursorTime(column: SQLWrapper) {
  return sql<string>`to_char(${column} at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US') || '0+00:00'`;
}

export function encodeCursor(time: string, id: string) {
  return Buffer.from(`${time}|${id}`, "utf8").toString("base64");
}

export function readCursor(cursor?: string) {
  if (cursor === undefined) return Effect.succeed(undefined);
  if (cursor.length <= 256 && /^[A-Za-z0-9+/]+={0,2}$/u.test(cursor)) {
    const [time, id, extra] = Buffer.from(cursor, "base64").toString("utf8").split("|");
    if (
      extra === undefined &&
      id &&
      isUuid(id) &&
      time &&
      /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{7}\+00:00$/u.test(time) &&
      !time.startsWith("0000")
    ) {
      const milliseconds = `${time.slice(0, 23)}Z`;
      const date = new Date(milliseconds);
      if (!Number.isNaN(date.valueOf()) && date.toISOString() === milliseconds)
        return Effect.succeed({ time, id });
    }
  }
  return Effect.fail(
    validationProblem("cursor", "Use the nextCursor returned by the preceding page."),
  );
}
