import { expect, test } from "bun:test";
import { storage } from "../src/storage";
import { trackerState } from "../src/trackers";

test("tracker workflow derives the earliest open state and handles terminal children", () => {
  expect(trackerState([])).toBe("backlog");
  expect(trackerState(["unstarted", "backlog", "triage"])).toBe("triage");
  expect(trackerState(["duplicate", "canceled"])).toBe("canceled");
  expect(trackerState(["duplicate", "completed"])).toBe("completed");
  expect(trackerState(["backlog", "completed"])).toBe("started");
});

test("S3-compatible storage writes, reads and deletes using signed local requests", async () => {
  const objects = new Map<string, Uint8Array<ArrayBuffer>>();
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      if (!request.headers.get("Authorization")?.startsWith("AWS4-HMAC-SHA256 "))
        return new Response(null, { status: 403 });
      const key = new URL(request.url).pathname;
      switch (request.method) {
        case "PUT":
          objects.set(key, new Uint8Array(await request.arrayBuffer()));
          return new Response(null, { status: 200, headers: { ETag: '"example"' } });
        case "GET": {
          const body = objects.get(key);
          return body ? new Response(body) : new Response(null, { status: 404 });
        }
        case "DELETE":
          objects.delete(key);
          return new Response(null, { status: 204 });
        default:
          return new Response(null, { status: 405 });
      }
    },
  });
  try {
    const store = storage({
      STORAGE_DRIVER: "s3",
      S3_BUCKET: "subspace-example",
      S3_ENDPOINT: server.url.href,
      S3_REGION: "us-east-1",
      S3_ACCESS_KEY_ID: "example",
      S3_SECRET_ACCESS_KEY: crypto.randomUUID(),
    });
    const bytes = new TextEncoder().encode("Object content");
    await store.put("example", bytes);
    expect(await store.get("example")).toEqual(bytes);
    await store.delete("example");
    expect(objects.size).toBe(0);
  } finally {
    server.stop(true);
  }
});
