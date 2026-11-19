# Specification

Activity sits after All issues. The same tab is available with key 5 and
through Commands. The team picker controls both sections. The tab preserves
its own focus, selection and scroll while the active grouped list keeps its
state. Open a row by clicking it or pressing Enter. Back and Escape return
to the row, and j and k can step among Activity issues in the detail view.

The top section follows the API's tracker and started filters. Recently
updated shows the seven-day window, newest first. Each row has an identifier,
title and state, followed by the change, its actor when available and the
age. Age uses lastActivityAt, falling back to creation if that field is
absent. A missing latest activity is displayed as Created. Unknown changes
read Updated. Agent actors are identified by role.

The two sections have independent loading, empty, error and continuation
states. Load more follows that section's cursor at 50 rows per page. A failed
continuation leaves the existing rows in place with Retry. Opening a fresh
tab reuses valid data. Leaving it, focusing the window, and scrolling send
no automatic refresh. Toolbar Reload refreshes the two heads. Team,
organisation, profile and session changes retire old work. Rows stay in
memory and are never indexed, sent to chat or written to NETT.
