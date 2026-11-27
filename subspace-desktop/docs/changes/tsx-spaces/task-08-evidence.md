---
status: done
track: A
depends-on: [03, 06]
spec: [Acceptance]
approval:
  by: coordinator on the owner's behalf, for owner review
  date: "2026-11-26"
---

# Task 08: evidence and checks

- Isolated debug build via `make capture` with `CAPTURE_FEATURES=tsx-spaces`:
  empty state, grant review, running demo, in Twilight Two light and dark.
- `make check`, `mise run quality`, `mise run audit`; fill in the
  delivery results.
