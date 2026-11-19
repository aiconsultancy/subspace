import { Effect, Schema } from "effect";
import { Multipart } from "effect/http";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { CurrentMember } from "./identity";
import {
  addFile,
  addImage,
  chatBytes,
  chatUpload,
  createChatUpload,
  deleteChat,
  deleteMedia,
  issueBytes,
  listFiles,
  listImages,
  resolveUploads,
} from "./media";
import {
  Attachment,
  ChatUpload,
  ChatUploadId,
  ChatUploadInput,
  Image,
  mediaBody,
  ReferenceCards,
  ResolveInput,
} from "./media-contract";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { endpointErrors } from "./problems";
import { pageOf, PageQuery, Ref, Uuid } from "./validation";

const chatParams = { reference: ChatUploadId };
const issueParams = { reference: Ref };
const mediaParams = { id: Uuid };
const imageFile = Schema.Struct({ file: Multipart.SingleFileSchema }).pipe(
  HttpApiSchema.asMultipart({ maxParts: 1, maxFileSize: "10 MiB" }),
);
const attachmentFile = Schema.Struct({ file: Multipart.SingleFileSchema }).pipe(
  HttpApiSchema.asMultipart({ maxParts: 1, maxFileSize: 10_000_000 }),
);

export const mediaGroup = HttpApiGroup.make("media")
  .add(
    HttpApiEndpoint.post("createUpload", "/chat/uploads", {
      payload: ChatUploadInput,
      success: ChatUpload.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("readUpload", "/chat/uploads/:reference", {
      params: chatParams,
      success: ChatUpload,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("uploadBytes", "/chat/uploads/:reference/content", {
      params: chatParams,
      success: mediaBody,
      error: endpointErrors,
    }),
    HttpApiEndpoint.delete("deleteChat", "/chat/uploads/:reference", {
      params: chatParams,
      success: Schema.Boolean,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("resolve", "/resolve", {
      payload: ResolveInput,
      success: ReferenceCards,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("images", "/issues/:reference/images", {
      params: issueParams,
      query: PageQuery,
      success: pageOf(Image),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("uploadImage", "/issues/:reference/images", {
      params: issueParams,
      payload: imageFile,
      success: Image.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("attachments", "/issues/:reference/attachments", {
      params: issueParams,
      query: PageQuery,
      success: pageOf(Attachment),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("uploadFile", "/issues/:reference/attachments", {
      params: issueParams,
      payload: attachmentFile,
      success: Attachment.pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("imageBytes", "/issues/media/:id", {
      params: mediaParams,
      success: mediaBody,
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("fileBytes", "/issues/attachments/:id", {
      params: mediaParams,
      success: mediaBody,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("deleteImage", "/issues/media/:id/delete", {
      params: mediaParams,
      success: Schema.Boolean,
      error: endpointErrors,
    }),
    HttpApiEndpoint.delete("deleteFile", "/issues/attachments/:id", {
      params: mediaParams,
      success: Schema.Boolean,
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const mediaHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof mediaGroup>,
) =>
  handlers
    .handle("createUpload", ({ payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => createChatUpload(member, payload))),
    )
    .handle("readUpload", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => chatUpload(member, params.reference))),
    )
    .handle("uploadBytes", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => chatBytes(member, params.reference))),
    )
    .handle("deleteChat", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => deleteChat(member, params.reference))),
    )
    .handle("resolve", ({ payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => resolveUploads(member, payload.references)),
      ),
    )
    .handle("images", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listImages(member, params.reference, query)),
      ),
    )
    .handle("uploadImage", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => addImage(member, params.reference, payload.file)),
      ),
    )
    .handle("attachments", ({ params, query }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => listFiles(member, params.reference, query)),
      ),
    )
    .handle("uploadFile", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) => addFile(member, params.reference, payload.file)),
      ),
    )
    .handle("imageBytes", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => issueBytes(member, params.id, "image"))),
    )
    .handle("fileBytes", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => issueBytes(member, params.id, "file"))),
    )
    .handle("deleteImage", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => deleteMedia(member, params.id, "image"))),
    )
    .handle("deleteFile", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => deleteMedia(member, params.id, "file"))),
    );
