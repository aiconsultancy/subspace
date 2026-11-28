# Keep selected crew history visible

## Intent

Fleet Console reads the full crew record when the operator selects a member.
Subsequent ship events carry a bounded project summary. A preview could
disappear from Recent outcomes when the summary arrived.

## Spec

Keep the selected member's full detail alongside the ship data. Refresh it
on drill-in and every 30 seconds while the member is visible, so an outcome
outside the ship's project slice still appears. Coalesce concurrent
detail reads and preserve the last successful record when a refresh fails.

## Delivery

The regression fixture feeds a ship event after opening a crew member whose
full record has a project outside that event. It asserts the newer outcome
and idle timestamp remain visible. A stream test covers coalesced refreshes
and an in-flight full read that survives a ship update.

## Tasks

- [x] Keep crew detail separate from ship summaries.
- [x] Exercise the regression and refresh ordering with offline fixtures.
