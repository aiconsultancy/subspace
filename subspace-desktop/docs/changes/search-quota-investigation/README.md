# Search quota test investigation

## Intent and specification

Investigate jt's 16 November 2026 fork-review item 4: print the error from
`full_store_evicts_oldest_and_admits_new_content`, reproduce the failure during
concurrent Cargo builds, and fix its demonstrated cause without weakening the
eviction or admission assertions.

## Evidence

The requested base, `750c26383b9c`, already printed the returned upsert error
as `write {index}: {error}`. The failure diagnostic now also includes the quota
and `service.sources()` result, exposing allocated usage and record count on
failure (or the source-read error). No timeouts, retries, quotas, timestamps,
assertions or search production code were changed.

On this Azure Linux workstation, the original test passed:

- Once in isolation.
- 30 consecutive executions, initially overlapping `cargo check --workspace
  --all-targets --locked` (the build finished before the repetitions).
- 100 consecutive executions, overlapping `cargo test --workspace --locked`
  through build and test phases (the suite finished before the repetitions).
  One execution took 29.50 seconds and still passed.
- 128 executions with eight test processes running concurrently, each with
  its own temporary database.
- The subsequent complete `make check` also passed the original test.

The repeated commands invoked the already-built `engine` test binary so they
did not wait on Cargo's build-directory lock. Both Cargo load commands and all
test processes were foreground commands and were waited to completion. They
used no live services.

## Findings and remaining work

The reported failure has **not been reproduced**, so there is no captured
upsert error or demonstrated root cause from these runs. The test uses a
private temporary store, SQLite has a two-second busy timeout, and quota
admission counts allocated filesystem bytes and sidecars; none of these is
established as the cause. The timestamp order was not changed speculatively.
This is diagnostic preparation, **not a claim to have fixed the flake**.

Next: obtain the failing platform/filesystem and printed error, reproduce it
there with the expanded diagnostic, then make and verify a cause-specific fix.

## Tasks

- [x] Preserve and expand the upsert failure diagnostic.
- [x] Repeat the unchanged test during Cargo activity and concurrent tests.
- [ ] Reproduce the reported error and establish its cause.
- [ ] Prove a deterministic fix with repeated runs under load.

See [search storage](../../search.md).
