import { Schema } from "effect";
import { HttpApiSchema } from "effect/http-api";
import { chatTypes, fileTypes, imageTypes } from "./file-validation";
import { Person } from "./projects";
import { Line, Timestamp, Uuid } from "./validation";

export const ChatUploadId = Schema.String.check(Schema.isPattern(/^UPL-[A-Za-z0-9_-]{22}$/u));
export const ChatUploadInput = Schema.Struct({
  uploadId: Uuid,
  fileName: Line(200),
  contentType: Schema.Literals(chatTypes),
  base64: Schema.String.check(Schema.isMinLength(4), Schema.isMaxLength(33_333_336)),
});
const fields = {
  fileName: Line(200),
  contentType: Schema.String,
  sizeBytes: Schema.Int,
  sha256: Schema.String,
  url: Schema.String,
};
export const ChatUpload = Schema.Struct({
  id: Uuid,
  reference: ChatUploadId,
  ...fields,
  uploadedByUserId: Uuid,
  createdAt: Timestamp,
  expiresAt: Timestamp,
}).annotate({ identifier: "ChatUpload" });
export const Image = Schema.Struct({
  id: Uuid,
  issueId: Uuid,
  ...fields,
  uploadedBy: Person,
  createdAt: Timestamp,
  relayedCrew: Schema.Null,
}).annotate({ identifier: "Image" });
export const Attachment = Schema.Struct({
  mediaId: Uuid,
  version: Schema.Int,
  kind: Schema.Literals(["file", "image"]),
  ...fields,
  expiresAt: Schema.Null,
  publicFileId: Schema.String,
}).annotate({ identifier: "Attachment" });
export const ResolveInput = Schema.Struct({
  references: Schema.Array(Schema.String.check(Schema.isMaxLength(100))).check(
    Schema.isMinLength(1),
    Schema.isMaxLength(50),
  ),
});
export const ReferenceCards = Schema.Struct({
  items: Schema.Array(
    Schema.Struct({
      reference: Schema.String,
      kind: Schema.Literal("chatUpload"),
      status: Schema.Literals(["available", "expired", "missing"]),
      upload: Schema.NullOr(ChatUpload),
    }),
  ),
}).annotate({ identifier: "ReferenceCards" });
export const mediaBody = HttpApiSchema.StreamUint8Array({
  contentType: [...new Set([...chatTypes, ...imageTypes, ...fileTypes])],
});
