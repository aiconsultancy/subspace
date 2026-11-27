---
status: done
track: B
depends-on: []
spec: [R-SYS-1 to R-SYS-7]
approval:
  by: coordinator on the owner's behalf, for owner review
  date: "2026-11-26"
---

# Task 02: system capability crate

- New crate `spaces/tsx/crates/system` (`subspace-tsx-system`) on workspace
  `sysinfo`; no GPUI, no nsh.
- Metrics, apps and facts readings as JSON (`serde`) in spec §5 shapes;
  app grouping adapted from sysmon with its notice.
- One shared sampler thread: tiers subscribed by consumers, 1 Hz metrics,
  2 s apps, stops with no subscribers.
- Unit tests: shapes, `null` for unavailable, grouping, rate baselines,
  privacy (no PID, path, command line, host or user name in output).

Done when the crate's tests and `make check` pass.

## Evidence

Done on `feat/tsx-spaces-system` in `feat(tsx): add the read-only system
capability crate`.

- `cargo test -p subspace-tsx-system`: 20 passed, including
  `app_readings_hold_no_pid_path_command_line_or_personal_name`,
  `live_readings_hold_no_host_name_user_name_or_path`, the sysmon grouping
  tests, null-not-zero shapes, network baselines and the sampler stopping
  with no subscribers.
- `make check` and `mise run quality` pass (26 November 2026).

Decisions:

- `sysinfo` 0.40.3 with `system`, `network` and `gpu` only.
- `Sampler::latest` waits once for a CPU baseline (`MINIMUM_CPU_UPDATE_INTERVAL`)
  so the first reading has CPU; network rates stay `null` until the second.
- Host and user names are scrubbed from every string: a string that holds
  one, or a path separator, becomes `null` (apps: "Unknown application").
- Displays and the Subspace version come from the host (`FactsInput`).
- `spaces/tsx/Cargo.toml` is a placeholder so the workspace loads; task 04's
  real crate replaces it at merge.
