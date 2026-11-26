import { expect, test } from "bun:test";
import { Effect } from "effect";
import { modelImage } from "../src/media";

test("image inspection keeps the exact image and refuses oversized previews", async () => {
  const gif = Buffer.from("R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7", "base64");
  expect((await Effect.runPromise(modelImage("image/gif", gif))).data).toBe(gif.toString("base64"));
  const wide = Buffer.from(gif);
  wide.writeUInt16LE(4097, 6);
  for (const input of [
    wide,
    Buffer.concat([gif, Buffer.alloc(3_750_001 - gif.length)]),
    Buffer.from("invalid"),
  ])
    expect((await Effect.runPromise(Effect.result(modelImage("image/gif", input))))._tag).toBe(
      "Failure",
    );
});
