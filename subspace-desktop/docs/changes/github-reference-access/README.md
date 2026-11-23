# GitHub reference access

Other Spaces show short summaries of pull requests through
`github@1/resolve-pull-requests`, as Channels does on a tile. This change
decides when the PRs Space may still answer with a summary it holds after
the signed-in account loses access to the pull request. It was written in
pull request 70 ([Channels Thread tab](../channels-thread-tab/README.md)),
taken out of it before that merged (`deeaaef`), and brought back on
`fix/github-reference-access` for a pull request of its own.

## The rule

Every read of a pull request takes a tick of one clock on the background
thread immediately before its request starts, whichever path makes it: a
list section, the PRs Space's own detail read (after its debounce), or
each request of a reference read, which reads its keys one at a time and
lands each as it completes. A result is weighed by that tick, never by
when it lands (`spaces/github/src/references/access.rs`).

For each account and pull request the Space keeps two numbers: the latest
start tick of a read that landed with an access refusal, and the latest of
one that landed with a success. The pull request is refused exactly when
the first is larger. Both only rise, so the answer does not depend on the
order reads land in, and nothing else (a refresh, a sign-in or sign-out,
an account switch, a reload) lowers or clears them; a read whose answer a
forget drops still records its decision. While it is refused the resolver
answers `Unavailable`, never a held title, status or counts.

An access refusal is a 403 that is not a rate limit, a 404, or the 400 the
GitHub client makes from a GraphQL `NOT_FOUND`. Any other 400, such as a
query GitHub rejects, is a failure. The Framework's GitHub client keeps a
plain 403 as 403 and reports a rate limit, including a 403 with
`Retry-After`, as 429, which is a failure and decides nothing
(`subspace-framework/clients/github/src/transport.rs` and
`transport/mapping.rs`). A 401 or a missing credential answers signed out
and never overrides held data. Every refused pull request is read again
on the next refresh, also after a forget has cleared the answers.

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

GPT-6.2 Sol and Claude Opus 5.5 then reviewed pull request 73 at
`24b727c` (Sol not clear, four high; Opus clear, three medium). Each item
below was checked against the code before it was changed.

3. **A refusal stuck for the session after a forget** (Opus medium 1, Sol
   medium). Real: a refresh marked only held answers stale, and a forget
   clears them. Fixed in `2e75257`: a refresh also marks every refused
   pull request stale. The sign-out test that asserted no read ever came
   now checks the refusal is read again on the next refresh
   (`another_account_signing_out_keeps_this_account_s_refusal_until_the_next_refresh`);
   `a_refusal_is_read_again_after_a_forget_and_a_refresh` covers a pull
   request the account does not hold (`navigation/tests/references/forget.rs`).
4. **One plain 403 cancelled the whole search prefetch** (Opus medium 2).
   Real, and caused by this change: the prefetch's default mapping treats
   a 403 as a credential failure. Fixed in `a3a97e8` (the client's new
   `is_access_refusal`, additive) and `f14b45f`: a plain 403 for one item
   is transient again, as it was when the client reported it as 502; a
   rejected credential, an SSO requirement or a rate limit still stops or
   slows the run. Test: `one_item_github_refuses_never_stops_the_whole_prefetch`,
   a unit test of the mapping, not of a whole prefetch run.
5. **The detail read and the list sections took their tick before the
   request** (Sol high 1, Opus medium 3). Real. Fixed in `2c63af2`: both
   take it on the background thread immediately before the request. Test:
   `a_detail_read_takes_its_tick_when_its_request_starts_not_when_queued`.
   The section path has no test of its own.
6. **A refusal waited for the whole batch** (Sol high 2). Real. Fixed in
   `0107949`: the requests stay sequential and each lands and is announced
   as it completes. Test:
   `each_request_of_a_batch_lands_and_is_announced_as_it_completes`, which
   counts the announcements; the test executor cannot hold one request
   open while another lands.
7. **Every 400 counted as a refusal** (Sol medium, Opus low). Real. Fixed
   in `6462ce5` (the client's new `is_not_found`, additive) and `3b689f4`:
   only the 400 from a GraphQL `NOT_FOUND` refuses, recognised by GitHub's
   "Could not resolve to a" message, as the error keeps no type. Tests:
   `a_query_github_rejects_is_a_failure_not_a_refusal`,
   `a_graphql_not_found_is_a_refusal` (`navigation/tests/references/codes.rs`).
8. **Another account's sign-in or sign-out dropped this account's
   in-flight refusal** (Sol high 3). Real. Fixed in `9d7bb59`: a read whose
   answer a forget drops still records its access decision, which is keyed
   by the account that made the request. Test:
   `another_account_signing_out_never_drops_this_account_s_refusal_in_flight`.
9. **Held summaries still answer after a reference read reports signed
   out** (Sol high 4). Not changed. The reviewers disagree: Opus checked
   that a 401 or a missing credential answers signed out and never
   overrides held data, and found it sound. That is the rule this
   Space's own documentation states (`references.rs`), so the code
   follows it. Masking held data after a 401 would change that rule, which
   is the owner's decision; it is listed below.

## Open

The owner limited this change to the findings above. These are known and
not fixed here:

- **Held summaries after a 401** (Sol high 4, above): if GitHub stops
  accepting the token outside the app, a reference read answers signed
  out for that pull request, but a listed or opened pull request still
  answers from held data until the account signs out in the app.
- **The access and list maps are never pruned** (Sol low, Opus low): one
  small record per account and pull request seen, kept for the session.
- **The model test is close to a tautology** (Sol low, Opus low;
  `references/access/model.rs`, `resolver_refuses`): its second term
  repeats the first, and it drives the decision functions, not the
  resolver's `known` and `wants_read`.
- **A bare 429 and a POST 403 with only `Retry-After`** (Sol low): a 429
  without rate-limit headers or message maps to 502, and the REST POST
  path passes no rate-limit headers, so such a 403 reads as a refusal.
- **An older opened detail after access returns** (Sol low): the first
  summary after a newer reference read lifts a refusal can come from the
  opened detail read before it, not the newer read.
- **A batch's stale mark uses the batch's queue time** (Sol medium): a key
  whose request started after a refresh's first section is still marked
  stale, so it can be read once more in the same refresh.

## How it was tested

After merging `main` (`9d2f850`) and bringing the work back (`b1fd284`),
and again after the second round:

- `cargo test -p subspace-github-board`: 203 passed.
- Each new test above was run against the code before its fix, with only
  the test applied, and failed there, except two: the prefetch mapping
  test, which tests a new function and was not run against the old code,
  and `a_graphql_not_found_is_a_refusal`, which passes on both, as the
  old code counted every 400.
- `cargo fmt`, `cargo clippy -p subspace-github-board --all-targets
  --all-features -- -D warnings` and `rustqual . --findings` (no findings)
  in the desktop.
- The Framework's GitHub client: `cargo fmt`, clippy with warnings denied,
  `cargo test -p subspace-client-github` (153 passed) and `mise run
  quality` in the Framework. Its two new functions are additive; no public
  signature or mapping changes. The dashboard crate's `prefetch.rs` is
  unchanged.

The rule's model test (`references/access/model.rs`) plays every
interleaving of six reads of one pull request against the decision
functions. It does not cover when a read takes its tick, which the tests
above do, or the resolver's answers, which is open.
