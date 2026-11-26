import { expect, test } from "bun:test";
import { Effect } from "effect";
import { textPage } from "../src/text-page";

test("byte offsets preserve BOMs, CRLF, trailing whitespace and every Unicode boundary", () => {
  for (const character of ["é", "€", "🪐"]) {
    for (let split = 1; split < Buffer.byteLength(character); split++) {
      const source = "a".repeat(1536 - split) + character + "\uFEFF\r\n ";
      const bytes = Buffer.from(source);
      const first = Effect.runSync(textPage(bytes.subarray(0, 1537), bytes.length, 0, null));
      expect(first.nextOffset).toBe(1536 - split);
      const offset = first.nextOffset!;
      const last = Effect.runSync(textPage(bytes.subarray(offset), bytes.length, offset, null));
      expect(last.nextOffset).toBeNull();
      expect(first.text + last.text).toBe(source);
      const middle = offset + 1;
      expect(() =>
        Effect.runSync(textPage(bytes.subarray(middle), bytes.length, middle, null)),
      ).toThrow();
    }
  }
  for (const source of ["", "\uFEFFHello\r\n", "\uFEFF"]) {
    const bytes = Buffer.from(source);
    expect(Effect.runSync(textPage(bytes, bytes.length, 0, 7))).toEqual({
      text: source,
      offset: 0,
      nextOffset: null,
      revision: 7,
    });
  }
});
