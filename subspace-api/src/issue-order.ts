import { sql, type SQLWrapper } from "drizzle-orm";

// Preserve the millisecond precision of the original JavaScript sort keys.
export const issueTimeOrder = (column: SQLWrapper) =>
  sql`(-floor(extract(epoch from (${column} at time zone 'UTC')) * 1000)::bigint)`;
export const issuePriorityOrder = (column: SQLWrapper) => sql`coalesce(nullif(${column}, 0), 5)`;
