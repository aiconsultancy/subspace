# Comment and reply authoring

[Spec](spec.md) · [Design](design.md) · [Tasks](tasks.md) · [Delivery](delivery.md)

People can post comments and replies with image attachments in the Issues Space.

People reading an issue or tracker in Subspace need to join its discussion
without leaving the open record. Add a composer below the existing comments
timeline, root-thread replies, and image paste/attachment using the selected
NETT profile and organisation. Keep Comments as the default, with All available.

The owner ordered the read experience first: the issue workspace implementation slices 6 and 7 are merged;
slice 5's issue-view work is separately awaiting approval. This change starts
at Subspace `52d27687946306f916e0e70d4bba1a5158b07858` and keeps clear of
the parallel Properties, Record and header/workflow work. The earlier
[workspace contract](../nett-issue-workspace/spec.md) remains authoritative
for reads, Markdown rendering, privacy and capture conventions.

This record adds the missing write contract. The tracker-lead captain approved
it with amendments on 15 November and authorised PR A: text comments and
replies. The text-only media draft fence is the retry mechanism. PR B adds
image intake after A merges, from the updated integration branch. The captain
opens both PRs.

Keep drafts and attachment bytes in memory, preserve failed work, and retry
uncertain outcomes with the same draft UUID. Never infer write permission from read
permission or discard readable content because a write was refused. Use only
synthetic fixtures during worker verification. The captain and jt own the
later authorised NETT dev check with a human WorkOS sign-in.

Creating or editing issues, comment edit/delete, thread resolution,
mention picking and notification UI are follow-ons.
