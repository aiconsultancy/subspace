---
status: done
track: A
depends-on: [02, 05]
spec: [R-CAP-1, R-CAP-4, R-CAP-5, R-SYS-6, R-SYS-7]
approval:
  by: coordinator on the owner's behalf, for owner review
  date: "2026-11-26"
---

# Task 06: host channel to the system capability

- Register the three host handlers per view, checking the grant again in
  the handler; push readings with `host.send`.
- Subscribe the shared sampler only while a granted view is visible;
  revoke stops data at once.
- Tests for grant enforcement and sampler start and stop.
