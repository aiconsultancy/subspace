---
status: done
track: B
depends-on: []
spec: [R-DEMO-1 to R-DEMO-6]
approval:
  by: coordinator on the owner's behalf, for owner review
  date: "2026-11-26"
---

# Task 03: demo package and signing script

- Install `nsh` with mise if absent. Scaffold `tsx-spaces/system-monitor/`
  (TSX, `nsh.json`, app ID `dev.subspace.system-monitor`, host capabilities
  only).
- Views: CPU overall and per core, memory, network graphs (60 samples),
  top apps, facts, rate and last-sample time, per-tier empty states.
  Theme token classes only.
- `nsh test` suite with stubbed host calls.
- `scripts/tsx-package.sh` and a mise task: create a local development
  root and release key under a gitignored folder, certify, publish a
  signed `.nshpkg`. Never commit private keys.
- `NOTICE` and `licenses/` entries for adapted sysmon code.

Done when `nsh test` passes and the script produces a package that
`nsh verify` accepts with the development root.

## Evidence

Done on `feat/tsx-spaces-system` in `feat(tsx): add the system monitor TSX
Space package` and `feat(tsx): sign TSX Space packages with a local
development key`.

- nsh CLI 0.7.1 through mise (`github:longbridge/nsh`), task-scoped tool.
- `mise run tsx-space-test`: `nsh check` clean; `nsh test` 12 passed, also
  with `TZ=Asia/Tokyo`. `make check` runs it.
- `mise run tsx-package`: writes
  `dist/tsx-spaces/dev.subspace.system-monitor-1.nshpkg` "for runtime 0.6.0
  or newer"; `nsh verify --root .local/tsx-keys/root.pub` prints `valid`.
  Keys live in the ignored `.local/tsx-keys/`.

Decisions:

- `minRuntime` 0.6.0, to match the host's `nsh-platform-gpui` pin.
- Refusal is `ERR_ACCESS_DENIED` or a handler error with message
  `not granted` (spec R-SYS-7); the custom theme token `text-edge` carries
  the design guide's text edge (spec R-SPACE-5), 16 px fallback.
- The release key is certified for `dev.subspace.*` for 90 days.
