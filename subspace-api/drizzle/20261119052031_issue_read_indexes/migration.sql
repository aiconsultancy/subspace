CREATE INDEX "issues_created_order_idx" ON "issues" ("organisation_id",(-floor(extract(epoch from ("created_at" at time zone 'UTC')) * 1000)::bigint),"id");--> statement-breakpoint
CREATE INDEX "issues_priority_order_idx" ON "issues" ("organisation_id",coalesce(nullif("priority", 0), 5),(-floor(extract(epoch from ("created_at" at time zone 'UTC')) * 1000)::bigint),"id");--> statement-breakpoint
CREATE INDEX "issues_activity_order_idx" ON "issues" ("organisation_id",(-floor(extract(epoch from ("last_activity_at" at time zone 'UTC')) * 1000)::bigint),(-floor(extract(epoch from ("created_at" at time zone 'UTC')) * 1000)::bigint),"id");--> statement-breakpoint
CREATE INDEX "issues_activity_since_idx" ON "issues" ("organisation_id","last_activity_at");--> statement-breakpoint
CREATE INDEX "issues_manual_order_idx" ON "issues" ("organisation_id","number","id");--> statement-breakpoint
CREATE INDEX "issues_parent_order_idx" ON "issues" ("organisation_id","parent_id","number","id");
