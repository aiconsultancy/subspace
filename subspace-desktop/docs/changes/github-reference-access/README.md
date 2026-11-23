# GitHub reference access

Other Spaces show short summaries of pull requests through
`github@1/resolve-pull-requests`, as Channels does on a tile. This change
decides when the PRs Space may still answer with a summary it holds after
the signed-in account loses access to the pull request. It was written in
pull request 70 ([Channels Thread tab](../channels-thread-tab/README.md)),
taken out of it before that merged (`deeaaef`), and brought back on
`fix/github-reference-access` for a pull request of its own.

## The rule

Every read of a pull request takes a tick of one clock when it starts,
whichever path makes it: a list section, the PRs Space's own detail read,
or each request of a reference read, which reads its keys one at a time.
A result is weighed by that tick, never by when it lands
(`spaces/github/src/references/access.rs`).

For each account and pull request the Space keeps two numbers: the latest
start tick of a read that landed with an access refusal, and the latest of
one that landed with a success. The pull request is refused exactly when
the first is larger. Both only rise, so the answer does not depend on the
order reads land in, and nothing else (a refresh, a sign-in or sign-out,
an account switch, a reload) lowers or clears them. While it is refused
the resolver answers `Unavailable`, never a held title, status or counts.

An access refusal is a 400, 403 or 404 that is not a rate limit. The
Framework's GitHub client keeps a plain 403 as 403 and reports a rate
limit, including a 403 with `Retry-After`, as 429, which is a failure and
decides nothing (`subspace-framework/clients/github/src/transport.rs` and
`transport/mapping.rs`). A 401 or a missing credential answers signed out.
A refusal is read again on the next refresh.

The [Spaces audit](../../spaces-audit.md) describes the same behaviour.

## What was fixed

From the reviews of pull request 70 (findings 2 and 3 of those carried
from pull request 69, 4 and 5 of the first round, 15 to 18 of the second;
see that pull request's record):

- A listed pull request kept its summary after GitHub refused the read
  (`bd1ae0a`).
- A read that started after a refresh's first section could override a
  later section's fresher list (`bd1ae0a`).
- Reference and detail reads overwrote each other's decisions, a late list
  section could bring back a refused summary, and a refusal on an opened
  pull request stuck for the session (`a7cc283`).
- A 403 with only `Retry-After` counted as a refusal (`0d23322`).
- An older refusal landing late lowered the stored refusal, and an
  unrelated account's sign-in or sign-out cleared the active account's
  refusals (`c49e37b`).
- The GitHub client reported a plain 403 as 502 (`2465e93`). Its
  `Failure` gains a `retry_after` field; `Failure` is internal to the
  client crate (its `transport` module is private), so no public signature
  changes.

From GPT-6.2 Sol's third review, its two high findings on this rule:

1. **Requests in one batch shared one tick** (`references/reads.rs`).
   Real. A reference read of several pull requests read them in turn but
   took its tick when it was queued. A detail read that started while the
   batch waited on an earlier key counted as newer than a later key's
   request, so GitHub's refusal of that request was dropped and the held
   summary kept answering. Fixed in `4986bab`: the clock is shared with the
   background read, and each request takes its tick as it starts. Test:
   `a_batch_s_later_request_is_newer_than_a_read_started_while_it_waited`,
   which failed on the code before the fix. Two older tests stood for
   "started before" by queueing a reference read before another read's
   tick; they now start the other read while the request is in flight
   (`navigation/tests/references/meanwhile.rs`).
2. **A reference reached by an alias and by its canonical key was tracked
   under two keys** (`references/reads.rs`). Not real in the current code,
   so no code changed. Every read, reference or detail, passes through
   `requested_pr`, which refuses a detail whose key is not the requested
   key ignoring ASCII case. A read under a repository's former name gets
   the pull request back under its current name, so it answers
   `Unavailable` and holds no summary; spellings that differ only in case
   share one cache key. Pinned by
   `a_pull_request_read_under_another_repository_name_is_never_cached_under_it`
   in `79b6815`.

## Open

The owner limited this change to the two findings above. These are known
and not fixed here:

- **A refusal can stick after signing in again** (Claude Opus 5.5, P2).
- **The prefetch stops on a 403.**
- **One model test is close to a tautology**
  (`references/access/model.rs`, `resolver_refuses`: its second term
  repeats the first).
- From Sol's third review, on this code: a retained refusal loses its
  retry on refresh once its cached answer is cleared, as when another
  account signs out (medium, `references/held.rs`); every 400 counts as a
  refusal, including a GraphQL validation error (medium,
  `references/reads.rs`); and the REST POST path drops `Retry-After`
  (low, `subspace-framework/clients/github/src/transport.rs`).

None of these was checked against the code for this record.

## How it was tested

After merging `main` (`9d2f850`) and bringing the work back (`b1fd284`):

- `cargo test -p subspace-github-board`: 196 passed, including the new
  test for finding 1 and the alias test.
- The finding 1 test was run against the code before the fix, with only
  the test applied, and failed on its `Unavailable` assertion.
- `cargo fmt`, `cargo clippy -p subspace-github-board --all-targets
  --all-features -- -D warnings` and `rustqual . --findings` (no findings)
  in the desktop.
- The Framework's GitHub client is unchanged from the work brought back:
  `cargo fmt --check`, clippy with warnings denied and
  `cargo test -p subspace-client-github` (152 passed) in the Framework.

The rule's model test (`references/access/model.rs`) plays every
interleaving of six reads of one pull request against the decision
functions. It does not cover the per-request ticks, which change only when
a reference read's requests take their tick, not how decisions combine.
