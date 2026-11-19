import { and, eq, sql } from "drizzle-orm";
import { Effect, Schema } from "effect";
import { HttpApiEndpoint, HttpApiGroup, HttpApiSchema } from "effect/http-api";
import { Database } from "./db";
import { CurrentMember, type Member } from "./identity";
import { Version } from "./initiative-contract";
import { addActivity, readIssue } from "./issues";
import { BearerAuth, handleDb, ValidationErrors, versionedApi } from "./middleware";
import { endpointErrors, problem } from "./problems";
import { readProject } from "./projects";
import { issues, todos } from "./schema";
import { Todo, todoDtos, todoRows, todoScope } from "./todo-model";
import { Line, pageOf, Ref, Uuid } from "./validation";

const maximum = 200;
const Text = Schema.Trim.pipe(Schema.decodeTo(Line(500)));
export const Add = Schema.Struct({
  text: Schema.optional(Text),
  items: Schema.optional(
    Schema.Array(Schema.Struct({ text: Text })).check(
      Schema.isMinLength(1),
      Schema.isMaxLength(50),
    ),
  ),
});
export const Edit = Schema.Struct({
  version: Version,
  text: Schema.optional(Text),
  done: Schema.optional(Schema.Boolean),
});
const Delete = Schema.Struct({ version: Version });
export const Reorder = Schema.Struct({
  todoIds: Schema.Array(Uuid).check(Schema.isMaxLength(maximum)),
});

type Db = (typeof Database)["Service"];
const all = <T>(items: readonly T[]) => ({ items, nextCursor: null });

function todoActivity(
  db: Db,
  member: Member,
  issueId: string,
  kind: string,
  data: Record<string, unknown>,
) {
  return Effect.gen(function* () {
    yield* db
      .update(issues)
      .set({ lastActivityAt: sql`greatest(${issues.lastActivityAt}, now())` })
      .where(and(eq(issues.organisationId, member.organisationId), eq(issues.id, issueId)));
    yield* addActivity(db, member, issueId, kind, data);
  });
}

function writable(db: Db, member: Member, reference: string) {
  return Effect.gen(function* () {
    const row = yield* readIssue(db, member, reference);
    const [issue] = yield* db
      .select()
      .from(issues)
      .where(and(eq(issues.organisationId, member.organisationId), eq(issues.id, row.id)))
      .for("update");
    if (!issue) return yield* Effect.fail(problem(404, "Issue not found."));
    const { project } = yield* readProject(db, member, issue.projectId);
    if (issue.archivedAt || project.archivedAt)
      return yield* Effect.fail(
        problem(409, "Unarchive the issue and project before changing todos."),
      );
    return issue;
  });
}

export const list = (member: Member, reference: string) =>
  Effect.gen(function* () {
    const db = yield* Database;
    const issue = yield* readIssue(db, member, reference);
    return all(yield* todoDtos(db, member, yield* todoRows(db, member, issue.id)));
  });

export const add = (member: Member, reference: string, input: typeof Add.Type) =>
  Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        if ((input.text === undefined) === (input.items === undefined)) {
          return yield* Effect.fail(problem(400, "Supply a single text or an items list."));
        }
        const entries = input.items ?? [{ text: input.text! }];
        const issue = yield* writable(tx, member, reference);
        const rows = yield* todoRows(tx, member, issue.id);
        if (rows.length + entries.length > maximum)
          return yield* Effect.fail(problem(409, "An issue can have at most 200 todos."));
        const position = (rows.at(-1)?.position ?? -1) + 1;
        const saved = yield* tx
          .insert(todos)
          .values(
            entries.map(({ text }, index) => ({
              organisationId: member.organisationId,
              issueId: issue.id,
              createdById: member.id,
              text,
              position: position + index,
            })),
          )
          .returning();
        for (const row of saved)
          yield* todoActivity(tx, member, issue.id, "item.created", { todoId: row.id });
        return all(yield* todoDtos(tx, member, saved));
      }),
    ),
  );

export function edit(
  member: Member,
  reference: string,
  id: string,
  input: typeof Edit.Type | typeof Delete.Type,
  remove = false,
) {
  return Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const issue = yield* writable(tx, member, reference);
        const where = and(todoScope(member.organisationId, issue.id), eq(todos.id, id));
        const [row] = yield* tx.select().from(todos).where(where);
        if (!row) return yield* Effect.fail(problem(404, "Todo not found."));
        if (input.version !== row.version)
          return yield* Effect.fail(problem(409, "The todo version is stale.", row.version));
        if (remove) {
          yield* tx.delete(todos).where(where);
          yield* todoActivity(tx, member, issue.id, "item.removed", { todoId: row.id });
          return (yield* todoDtos(tx, member, [row]))[0]!;
        }
        const { text, done } = input as typeof Edit.Type;
        if (text === undefined && done === undefined)
          return yield* Effect.fail(problem(400, "Supply todo text or completion status."));
        const changedDone = done !== undefined && done !== (row.doneAt !== null);
        if ((text === undefined || text === row.text) && !changedDone)
          return (yield* todoDtos(tx, member, [row]))[0]!;
        const [updated] = yield* tx
          .update(todos)
          .set({
            text,
            ...(changedDone
              ? { doneAt: done ? new Date() : null, doneById: done ? member.id : null }
              : {}),
            version: row.version + 1,
            updatedById: member.id,
            updatedAt: sql`now()`,
          })
          .where(where)
          .returning();
        if (!updated) return yield* Effect.fail(problem(404, "Todo not found."));
        yield* todoActivity(
          tx,
          member,
          issue.id,
          changedDone ? (done ? "item.completed" : "item.reopened") : "item.edited",
          { todoId: row.id },
        );
        return (yield* todoDtos(tx, member, [updated]))[0]!;
      }),
    ),
  );
}

export const reorder = (member: Member, reference: string, requested: readonly string[]) =>
  Effect.flatMap(Database, (db) =>
    db.transaction((tx) =>
      Effect.gen(function* () {
        const issue = yield* writable(tx, member, reference);
        const rows = yield* todoRows(tx, member, issue.id);
        const old = new Map(rows.map((row) => [row.id, row.position]));
        if (
          requested.length !== rows.length ||
          new Set(requested).size !== rows.length ||
          requested.some((id) => !old.has(id))
        ) {
          return yield* Effect.fail(problem(400, "Supply every todo ID exactly once."));
        }
        if (requested.every((id, index) => rows[index]?.id === id))
          return all(yield* todoDtos(tx, member, rows));
        const scope = todoScope(member.organisationId, issue.id);
        // Move all occupied positions out of the way in this transaction, then
        // install the new order while retaining versions for unchanged positions.
        yield* tx
          .update(todos)
          .set({ position: sql`-${todos.position} - 1` })
          .where(scope);
        for (const [position, id] of requested.entries()) {
          const changed = old.get(id) !== position;
          yield* tx
            .update(todos)
            .set({
              position,
              ...(changed
                ? {
                    version: sql`${todos.version} + 1`,
                    updatedById: member.id,
                    updatedAt: sql`now()`,
                  }
                : {}),
            })
            .where(and(scope, eq(todos.id, id)));
        }
        yield* todoActivity(tx, member, issue.id, "item.moved", { todoIds: [...requested] });
        return all(yield* todoDtos(tx, member, yield* todoRows(tx, member, issue.id)));
      }),
    ),
  );

const params = { reference: Ref };
const todoParams = { ...params, todoId: Uuid };

export const todoGroup = HttpApiGroup.make("todos")
  .add(
    HttpApiEndpoint.get("listTodos", "/issues/:reference/todos", {
      params,
      success: pageOf(Todo),
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("addTodos", "/issues/:reference/todos", {
      params,
      payload: Add,
      success: pageOf(Todo).pipe(HttpApiSchema.status(201)),
      error: endpointErrors,
    }),
    HttpApiEndpoint.patch("editTodo", "/issues/:reference/todos/:todoId", {
      params: todoParams,
      payload: Edit,
      success: Todo,
      error: endpointErrors,
    }),
    HttpApiEndpoint.post("deleteTodo", "/issues/:reference/todos/:todoId/delete", {
      params: todoParams,
      payload: Delete,
      success: Todo,
      error: endpointErrors,
    }),
    HttpApiEndpoint.put("orderTodos", "/issues/:reference/todos/order", {
      params,
      payload: Reorder,
      success: pageOf(Todo),
      error: endpointErrors,
    }),
  )
  .prefix(versionedApi)
  .middleware(BearerAuth)
  .middleware(ValidationErrors);

export const todoHandlers = (
  handlers: import("effect/http-api/HttpApiBuilder").Handlers.FromGroup<typeof todoGroup>,
) =>
  handlers
    .handle("listTodos", ({ params }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => list(member, params.reference))),
    )
    .handle("addTodos", ({ params, payload }) =>
      handleDb(Effect.flatMap(CurrentMember, (member) => add(member, params.reference, payload))),
    )
    .handle("editTodo", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          edit(member, params.reference, params.todoId, payload),
        ),
      ),
    )
    .handle("deleteTodo", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          edit(member, params.reference, params.todoId, payload, true),
        ),
      ),
    )
    .handle("orderTodos", ({ params, payload }) =>
      handleDb(
        Effect.flatMap(CurrentMember, (member) =>
          reorder(member, params.reference, payload.todoIds),
        ),
      ),
    );
