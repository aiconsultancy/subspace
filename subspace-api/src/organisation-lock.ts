import { eq } from "drizzle-orm";
import { Database } from "./db";
import { organisations } from "./schema";

type Db = (typeof Database)["Service"];

/**
 * Locks the organisation's row FOR NO KEY UPDATE, which every write to its members, agents,
 * fleets, ships and invites takes before their rows. A leaf module, so that any domain module can
 * take it without importing another domain.
 */
export const lockOrganisation = (db: Db, organisationId: string) =>
  db
    .select({ id: organisations.id })
    .from(organisations)
    .where(eq(organisations.id, organisationId))
    .for("no key update");
