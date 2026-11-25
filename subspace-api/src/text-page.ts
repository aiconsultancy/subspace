import { Effect, Schema } from "effect";
import { Database } from "./db";
import { documentRevision, readDocument } from "./documents";
import { type Member } from "./identity";
import { Version } from "./initiative-contract";
import { mediaTextRange } from "./media";
import { problem } from "./problems";
import { isUuid, Ref } from "./validation";

export const TextPageInput = Schema.Struct({
  kind: Schema.Literals(["document", "attachment", "chatUpload"]),
  reference: Ref,
  offset: Schema.optional(Schema.Int.check(Schema.isGreaterThanOrEqualTo(0))),
  revision: Schema.optional(Version),
});

// Even entirely newline-filled pages fit within a 2,000-line tool output budget.
const pageBytes = 1536;

export function textPage(bytes: Uint8Array, size: number, offset: number, revision: number | null) {
  let length = Math.min(bytes.length, pageBytes);
  while (length > 0 && length < bytes.length && (bytes[length]! & 0xc0) === 0x80) length--;
  return Effect.try({
    try: () => ({
      text: new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(
        bytes.subarray(0, length),
      ),
      offset,
      nextOffset: offset + length < size ? offset + length : null,
      revision,
    }),
    catch: () => problem(400, "Use a UTF-8 character boundary from nextOffset."),
  });
}

export function readTextPage(member: Member, input: typeof TextPageInput.Type) {
  return Effect.gen(function* () {
    const { kind, reference, revision, offset = 0 } = input;
    if (kind === "document") {
      if (offset && revision === undefined)
        return yield* Effect.fail(
          problem(400, "Use the document revision returned on the first page."),
        );
      const db = yield* Database;
      const { doc, rev } = yield* readDocument(db, member, reference);
      const content =
        revision === undefined || revision === rev.number
          ? rev.content
          : (yield* documentRevision(db, member, doc.id, revision)).content;
      const bytes = new TextEncoder().encode(content);
      if (offset > bytes.length)
        return yield* Effect.fail(problem(400, "The offset is beyond the end of the text."));
      return yield* textPage(
        bytes.subarray(offset, offset + pageBytes + 1),
        bytes.length,
        offset,
        revision ?? rev.number,
      );
    }
    if (revision !== undefined)
      return yield* Effect.fail(problem(400, "Only documents take a revision."));
    if (kind === "attachment" && !isUuid(reference))
      return yield* Effect.fail(problem(400, "Use an attachment media UUID."));
    const { bytes, size } = yield* mediaTextRange(member, reference, kind, offset, pageBytes + 1);
    return yield* textPage(bytes, size, offset, null);
  });
}
