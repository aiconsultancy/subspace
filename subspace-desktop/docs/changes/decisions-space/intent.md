# A Decisions Space

Back to the [change records](../README.md).

The owner asked for "a Decisions Space. We have decisions linked to issues,
so you can view them on the issue, but having a searchable decision page,
with the details, who made it, if it was superseded etc. It can be based on
the Issues page strongly."

The Issues Space already shows the decisions linked to one issue
([Read decisions on an issue](../issue-decisions/intent.md)). This change
adds a Space of its own that lists every decision the account can see, on
the workspace shell every Space shares, from the approved
`Decisions.dc.html` mock-up: a navigator of views (one per status), makers
and projects; the shared list grouped by status; a peek in the inspector; and
a decision opened as a page in its own tab, with a clear banner when it was
superseded.

It reads only. It adds no way to log, accept, reject or supersede a decision,
and needs no new Subspace API route or client function: it reads
`GET /api/v1/decisions` with the list's filters and
`GET /api/v1/decisions/{id}`. Linked issues open in the Issues Space through
the `issues` contract, as Channels opens them.

See the [behaviour](spec.md), [verification](delivery.md) and
[checklist](tasks.md).
