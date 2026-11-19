import { Client } from "pg";

// Public writes own all records and validations. The API has no historical
// import endpoint, so only the timestamps of newly written fixture rows are
// adjusted here. The same connection serialises cooperating seed processes.
export async function seedHistory(connectionString: string | undefined, organisation: string) {
  if (!connectionString) throw new Error("The local stack seed requires DATABASE_URL.");
  const db = new Client({ connectionString, statement_timeout: 120_000 });
  try {
    await db.connect();
    await db.query("select pg_advisory_lock(hashtext($1))", [`subspace-demo:${organisation}`]);
  } catch (error) {
    await db.end();
    throw error;
  }
  return {
    async issue(id: string, created: string) {
      await db.query(
        `with activity as (
           update issue_activity set created_at = $2
           where issue_id = $1 and kind = 'issue.created'
         )
         update issues set created_at = $2, updated_at = $2, last_activity_at = $2,
           started_at = case when started_at is not null then $2::timestamptz end,
           completed_at = case when completed_at is not null then $2::timestamptz end,
           canceled_at = case when canceled_at is not null then $2::timestamptz end
         where id = $1 and version = 1`,
        [id, created],
      );
    },
    async comment(issue: string, id: string, created: string) {
      await db.query(
        `with stamped_activity as (
           update issue_activity set created_at = $3
           where issue_id = $1 and kind = 'comment.created' and data->>'commentId' = $2
           returning id
         ), stamped_comment as (
           update issue_comments set created_at = $3
           where issue_id = $1 and id::text = $2 and version = 1
         )
         update issues set last_activity_at = greatest($3::timestamptz, (
           select max(entry.created_at) from issue_activity entry
           where entry.issue_id = $1 and entry.id not in (select id from stamped_activity)
         )) where id = $1`,
        [issue, id, created],
      );
    },
    close: () => db.end(),
  };
}

export type SeedHistory = Awaited<ReturnType<typeof seedHistory>>;
