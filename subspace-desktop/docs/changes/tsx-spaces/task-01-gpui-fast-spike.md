---
status: done
track: A
depends-on: []
spec: [R-BUILD-1, R-BUILD-2, R-BUILD-3, R-BUILD-4, R-BUILD-5]
approval:
  by: coordinator on the owner's behalf, for owner review
  date: "2026-11-26"
---

# Task 01: GPUI Fast and nsh spike

- Add product feature `tsx-spaces` (in `unreleased-spaces`) that enables
  `gpui-kit/gpui-fast` and an empty `subspace-tsx-board` crate depending on
  `nsh-platform-gpui` (unused default features off).
- Build with 0.9.1. If it fails, upgrade GPUI Kit to the version nsh tests,
  behind the feature if possible, and measure existing Spaces.
- Link test: run a script in `subspace-charts`' QuickJS and in nsh's in one
  test binary. On a clash, apply the smallest fix and record it.
- Record route, `cargo tree` results, binary size and build time with and
  without the feature in `delivery.md`.

Done when `make check` passes and the feature build links and starts.
