import { Effect, Result, Schema } from "effect";
import { HttpServerRequest, HttpServerResponse } from "effect/http";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { CurrentMember } from "./identity";
import { mcpTools, ToolName } from "./mcp-tools";
import { BearerAuth } from "./middleware";
import { endpointErrors, problem } from "./problems";
import { Line } from "./validation";

const protocols = ["2025-11-25", "2025-06-18", "2025-03-26"];
const maxWireBytes = 256_000;

const Message = Schema.Struct({
  jsonrpc: Schema.Literal("2.0"),
  method: Line(100),
  id: Schema.optional(Schema.Union([Schema.String, Schema.Int])),
  params: Schema.optional(Schema.Record(Schema.String, Schema.Unknown)),
});
const jsonResponse = (id: string | number, result: unknown) =>
  HttpServerResponse.jsonUnsafe({ jsonrpc: "2.0", id, result });
const rpcError = (id: string | number | null, code: number, message: string, status = 200) =>
  HttpServerResponse.jsonUnsafe({ jsonrpc: "2.0", id, error: { code, message } }, { status });
const toolResult = (data: unknown, isError = false) => ({
  content: [{ type: "text", text: JSON.stringify(data) }],
  ...(typeof data === "object" && data !== null && !Array.isArray(data)
    ? { structuredContent: data }
    : {}),
  isError,
});
const call = Schema.decodeUnknownResult(
  Schema.Struct({
    name: ToolName,
    arguments: Schema.optional(Schema.Record(Schema.String, Schema.Unknown)),
  }),
);

function admitted(
  request: HttpServerRequest.HttpServerRequest,
): Effect.Effect<void, (typeof endpointErrors)[number]["Type"]> {
  if (request.headers.origin !== undefined) {
    return Effect.fail(problem(403, "This MCP endpoint accepts native clients."));
  }
  const protocol = request.headers["mcp-protocol-version"];
  if (protocol !== undefined && !protocols.includes(protocol)) {
    return Effect.fail(problem(400, "The MCP protocol version is unsupported."));
  }
  return Effect.void;
}

function receive(request: HttpServerRequest.HttpServerRequest) {
  return Effect.gen(function* () {
    yield* admitted(request);
    if (
      request.headers["content-type"]?.split(";")[0]?.trim().toLowerCase() !== "application/json"
    ) {
      return rpcError(null, -32600, "Use application/json.", 415);
    }
    if (Number(request.headers["content-length"]) > maxWireBytes) {
      return rpcError(null, -32600, "Request exceeds the MCP size limit.", 413);
    }
    const raw = yield* request.arrayBuffer.pipe(
      Effect.mapError(() => problem(400, "The MCP request body could not be read.")),
    );
    if (raw.byteLength > maxWireBytes)
      return rpcError(null, -32600, "Request exceeds the MCP size limit.", 413);
    const parsed = Schema.decodeUnknownResult(Schema.fromJsonString(Schema.Unknown))(
      new TextDecoder().decode(raw),
    );
    if (Result.isFailure(parsed)) return rpcError(null, -32700, "Invalid JSON.", 400);
    const decoded = Schema.decodeUnknownResult(Message)(parsed.success);
    if (Result.isFailure(decoded)) return rpcError(null, -32600, "Invalid JSON-RPC request.", 400);
    const { method, id, params } = decoded.success;
    if (id === undefined) {
      return method.startsWith("notifications/")
        ? HttpServerResponse.empty({ status: 202 })
        : rpcError(null, -32600, "A request ID is required.", 400);
    }
    if (method === "initialize") {
      const requested = params?.protocolVersion;
      const protocolVersion =
        typeof requested === "string" && protocols.includes(requested) ? requested : protocols[0];
      return jsonResponse(id, {
        protocolVersion,
        serverInfo: { name: "Subspace API", version: "1.0.0" },
        capabilities: { tools: {} },
      });
    }
    if (method === "ping") return jsonResponse(id, {});
    if (method === "tools/list") {
      const tools = Object.entries(mcpTools).map(
        ([name, { description, inputSchema, annotations }]) => ({
          name,
          description,
          inputSchema,
          annotations,
        }),
      );
      return jsonResponse(id, { tools });
    }
    if (method !== "tools/call") return rpcError(id, -32601, "Method not found.");
    const input = call(params);
    if (Result.isFailure(input) || !Object.hasOwn(mcpTools, input.success.name)) {
      return rpcError(id, -32602, "Unknown tool or invalid arguments.");
    }
    const member = yield* CurrentMember;
    const tool = mcpTools[input.success.name as keyof typeof mcpTools];
    const result = yield* tool
      .call({ ...member, source: "mcp" }, input.success.arguments ?? {})
      .pipe(Effect.result);
    return jsonResponse(
      id,
      Result.isSuccess(result) ? toolResult(result.success) : toolResult(result.failure, true),
    );
  });
}

export const mcpGroup = HttpApiGroup.make("mcp")
  .add(
    HttpApiEndpoint.post("receive", "/mcp", {
      payload: Message,
      success: [Schema.Unknown, HttpApiSchema.NoContent],
      error: endpointErrors,
    }),
    HttpApiEndpoint.get("listen", "/mcp", {
      success: HttpApiSchema.NoContent,
      error: endpointErrors,
    }),
  )
  .middleware(BearerAuth);

export const mcpHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof mcpGroup>,
) =>
  handlers
    .handleRaw("receive", ({ request }) => receive(request))
    .handleRaw("listen", ({ request }) =>
      Effect.as(
        admitted(request),
        HttpServerResponse.empty({
          status: 405,
          headers: { Allow: "POST" },
        }),
      ),
    );
