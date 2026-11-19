# Read decisions on an issue

Back to the [change records](../README.md).

Make decisions linked to an issue readable without leaving its page. The
**Decisions** tab shows current links. A decision logged in **All** opens the
same read-only panel, so a reader can follow the reasoning and related
decisions without losing their place.

The client models and the timeline's decision references already existed.
This change adds the issue-local reads, the tab and the panel. A board-wide
Decisions Space with its own filters is separate work. This change does not
add a way to record, accept, reject or supersede a decision, and needs no new
Subspace API routes: it reads `GET /api/v1/decisions?issue=` and
`GET /api/v1/decisions/{id}`.

See the [behaviour and API limits](spec.md), [verification](delivery.md) and
[delivery checklist](tasks.md).
