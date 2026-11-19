import { PgClient } from "@effect/sql-pg";
import { Context, Effect, Layer, Option, Queue, Schedule, Schema, Stream } from "effect";
import { HttpServerResponse } from "effect/http";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { type Config } from "./config";
import { Database } from "./db";
import { CurrentMember } from "./identity";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { endpointErrors, problem, TooManyRequests } from "./problems";
import { readProject } from "./projects";
import { NullableString, Ref, Timestamp, Uuid } from "./validation";
import { Version } from "./initiative-contract";

const Change = Schema.Struct({
  projectKey: Ref,
  issueId: Uuid,
  identifier: Ref,
  parentIdentifier: NullableString,
  version: Version,
  action: Schema.String,
  auditId: Schema.optional(NullableString),
  actorType: Schema.Literals(["human", "integration", "agent"]),
  occurredAt: Timestamp,
});
const Notification = Schema.fromJsonString(
  Schema.Struct({
    organisationId: Uuid,
    projectId: Uuid,
    ...Change.fields,
  }),
);
const decode = Schema.decodeUnknownOption(Notification);

type Subscriber = {
  organisationId: string;
  memberId: string;
  projects: ReadonlySet<string>;
  queue: Queue.Queue<typeof Change.Type, import("effect/Cause").Done>;
};

export class UpdateHub extends Context.Service<
  UpdateHub,
  {
    subscribe: (subscriber: Subscriber) => "offline" | "full" | "accepted";
    remove: (subscriber: Subscriber) => void;
  }
>()("subspace/UpdateHub") {}

export function updateHubLayer(limits: Config["updates"]) {
  return Layer.effect(
    UpdateHub,
    Effect.gen(function* () {
      const pg = yield* PgClient.PgClient;
      const subscribers = new Set<Subscriber>();
      let online = false;
      const listen = Effect.gen(function* () {
        const queue = yield* pg.listen("subspace_issues");
        yield* Effect.acquireRelease(
          Effect.sync(() => {
            online = true;
          }),
          () =>
            Effect.sync(() => {
              online = false;
              for (const subscriber of subscribers) Queue.endUnsafe(subscriber.queue);
            }),
        );
        yield* Stream.runForEach(Stream.fromQueue(queue), ({ payload }) =>
          Effect.sync(() => {
            const result = decode(payload);
            if (Option.isNone(result)) return;
            const { organisationId, projectId, ...data } = result.value;
            for (const subscriber of subscribers) {
              if (
                subscriber.organisationId === organisationId &&
                subscriber.projects.has(projectId)
              ) {
                // A lagging client reconnects and reloads over REST.
                if (!Queue.offerUnsafe(subscriber.queue, data)) Queue.endUnsafe(subscriber.queue);
              }
            }
          }),
        );
      });
      yield* Effect.scoped(listen).pipe(
        Effect.catch(() => Effect.logWarning("The issue update listener disconnected.")),
        Effect.repeat(Schedule.spaced("1 second")),
        Effect.forkScoped,
      );
      return {
        subscribe(subscriber) {
          if (!online) return "offline";
          if (subscribers.size >= limits.total) return "full";
          let active = 0;
          for (const current of subscribers) {
            if (current.memberId === subscriber.memberId && ++active >= limits.perUser)
              return "full";
          }
          subscribers.add(subscriber);
          return "accepted";
        },
        remove(subscriber) {
          subscribers.delete(subscriber);
          Queue.shutdownUnsafe(subscriber.queue);
        },
      };
    }),
  );
}

export const updatesGroup = HttpApiGroup.make("updates")
  .add(
    HttpApiEndpoint.get("subscribe", "/updates", {
      query: { project: Schema.Array(Ref).check(Schema.isMinLength(1), Schema.isMaxLength(64)) },
      success: HttpApiSchema.StreamSse({ data: Schema.NullOr(Change), keepAlive: "15 seconds" }),
      error: [...endpointErrors, TooManyRequests],
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

function subscribe(references: readonly string[]) {
  return Effect.gen(function* () {
    const db = yield* Database;
    const hub = yield* UpdateHub;
    const member = yield* CurrentMember;
    const projects = yield* Effect.forEach([...new Set(references)], (reference) =>
      Effect.gen(function* () {
        const { project } = yield* readProject(db, member, reference);
        if (project.archivedAt) return yield* Effect.fail(problem(404, "Project unavailable."));
        return project.id;
      }),
    );
    const queue = yield* Queue.dropping<typeof Change.Type, import("effect/Cause").Done>(64);
    const subscriber = {
      organisationId: member.organisationId,
      memberId: member.id,
      projects: new Set(projects),
      queue,
    };
    yield* Effect.acquireRelease(
      Effect.gen(function* () {
        const admission = hub.subscribe(subscriber);
        if (admission === "offline")
          return yield* Effect.fail(problem(503, "The issue update listener is reconnecting."));
        if (admission === "full")
          return yield* Effect.fail(problem(429, "Too many live update streams."));
      }),
      () => Effect.sync(() => hub.remove(subscriber)),
    );
    const lifetime = Math.max(0, Math.min(180_000, member.tokenExpiresAt * 1000 - Date.now()));
    const changes = Stream.fromQueue(queue).pipe(
      Stream.map((data) => `event: issueChanged\ndata: ${JSON.stringify(data)}\n\n`),
    );
    const events = Stream.succeed("event: ready\ndata: null\n\n").pipe(
      Stream.concat(changes),
      Stream.interruptWhen(Effect.sleep(lifetime)),
      Stream.merge(
        Stream.fromEffectSchedule(
          Effect.succeed(": keepalive\n\n"),
          Schedule.spaced("15 seconds"),
        ).pipe(Stream.drop(1)),
        { haltStrategy: "left" },
      ),
      Stream.encodeText,
    );
    return HttpServerResponse.stream(events, {
      contentType: "text/event-stream",
      headers: { "X-Accel-Buffering": "no", "X-Content-Type-Options": "nosniff" },
    });
  });
}

export const updatesHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof updatesGroup>,
) => handlers.handle("subscribe", ({ query }) => handleDb(subscribe(query.project)));
