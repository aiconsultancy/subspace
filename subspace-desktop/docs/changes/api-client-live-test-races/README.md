# Make the API client's live tests reliable

The `subspace-api-client` tests failed now and then on pushes that did not
touch the crate, and passed on a retry. Each failed push cost a full re-run
of the desktop checks. Most failures were in `tests::nett::live` and
`tests::live`. Two causes were found.

## Resets from a full listen queue

The NETT fixture, like the other desktop test servers, listened with
socketserver's default queue of 5. The client under test keeps up to six
requests and its update stream open while a test polls a fixture hook.
When that burst found the queue full, macOS reset a waiting connection. A
hook call then failed with `ECONNRESET`, as in
`a_broken_stream_catches_up_through_rest_before_it_reconnects`, or a
client request was lost before the fixture logged it. Python's
`socketserver` accepts the queue length as `request_queue_size`; with 32
connections waiting to be accepted, a queue of 5 reset all but five of
them.

The NETT, Subspace API, Fleet API and GitHub fixtures now listen with a
queue of 128, and the NETT fixture's tests check that 32 connections
waiting to be accepted are all served.

## Tests racing the catch-up after ready

A few tenths of a second after a stream sends `ready`, the hub reads the
open issue and the list head again. When the issue read lands, it reads the
pull requests and starts the media lists again. Several tests acted as soon
as the hub was live, or slept for a fixed time and assumed the catch-up was
over. On a busy machine the catch-up lands later and races the test:

- the landing issue read restarts the attachment list, so a queued
  `load_attachments` is dropped;
- the catch-up's list read takes a slow answer that a test set up for its
  own reload;
- an extra detail read lands inside a test's counting window.

The tests now wait for the hub to say the catch-up is over instead of a
fixed time. In test builds each `ready` gets an id, counting from 1, and the
session records the id of its `ready` once that `ready`'s catch-up has been
taken. `Hub::caught_up_after_ready` holds once the latest `ready`'s catch-up
has been taken, its reads of the open issue and
the list head are neither out nor owed, and nothing they queued is waiting or
being read. Both `tests::live` and `tests::nett::live` wait on it. A request
path cannot tell the catch-up apart: the hub polls the open issue every
300 ms until the stream is live, and the count reads use the list's path.

Tests that took a baseline while the first reads were still out now wait
for them: the ready-scope test for the list's count reads, and the
no-updates test until the hub has no read queued or out (`Hub::idle`, also
for tests only). The burst test waits for its trailing read before its
two-second window instead of assuming it has been made by then.

The Issues Space already reads an idle attachment list again, so the app
does not lose the list as the tests did.

## Limits that only bound a failure

A run of the whole crate alongside other builds takes more than twice as
long, and the hub's own waits, such as the backoff after a dropped stream,
used much of the five-second limits on waiting for a hub state and for a
stream to connect. Both now allow 15 seconds. Passing runs return as soon as
the condition holds, so only a failing run takes longer.

In test builds the hub dropped a connected stream that had not sent `ready`
within 3 seconds. The scope tests hold the stream while they check that the
hub polls meanwhile, and on a busy machine the hub dropped it under them.
Tests now use the app's 15 seconds; none relied on the shorter limit.

## Other checks that raced the clock

The same pushes failed on checks that compared wall-clock time with a budget
sized for an idle machine:

- The NETT and Fleet API fixtures had 10 seconds to start Python and print
  their banner. They now have 60, which bounds only a fixture that never
  starts.
- The late to-do answer test counted list reads after reopening the issue
  and waited for one more, from save A's two-second slow answer. On a busy
  machine A had already landed and been read by then. The test now holds
  both answers in the mock and lets each go in turn (see Review findings and
  Follow-up review findings).
- In the NETT fixture's own tests, the slow-body test checks the headers the
  fixture recorded as sent and keeps only lower bounds; stream-ending checks
  assert why the fixture ended the stream (see Review findings); and the
  trickling-upload probe has 3 seconds against a 5-second body deadline
  instead of 0.7 against 1.
- The board-docs linear-scan check allows 20 seconds instead of 2, the
  desktop-session test waits for its command to start before interrupting
  it, and the capture test allows its fixture servers 20 seconds to stop,
  each probe capped at the time left.

## Review findings

Two model reviews of the pull request found no critical or high findings.
Their medium and low findings, and what came of each:

- **Stream ends proved too little.** A bound under the stream's lifetime,
  measured from after its headers, also passed a stream that closed only at
  its full lifetime. The fixture now records why each stream ended (closed by
  the fixture, lost authority, team archived, overflow, token expired,
  lifetime, write timed out, client gone) and the tests assert the reason.
  With the close hook made a no-op the close test failed ("'closed by the
  fixture' not found in ['lifetime']"), and with the bearer's expiry
  forgotten the expiry test failed ("'token expired' not found in ['lost
  authority']").
- **The lifetime test could hang.** It read to the end with no limit, and
  keep-alives defeat the socket timeout. Reading now has a 30-second backstop
  that shuts the socket; with the lifetime removed the test failed after
  30 seconds instead of hanging.
- **The catch-up wait matched a request path.** It accepted any answered
  list-path read after the stream opened, which the initial count reads can
  be. It now waits on the hub's own count of catch-ups, above.
- **The late to-do test could pass without the race.** Save B might not be
  pending when A's answer was handled, and `answeredAt` was taken after the
  answer was written. slowBody with `waitForRelease` now holds each body on
  its own event, and `POST /__fixture/slowBody/release` lets the oldest go.
  Each logged request has a `seq`, and an answer gets an `answerSeq` on the
  same sequence just before its last bytes go. The test holds both saves,
  lets A go only once B is held, waits for a list read after A's
  `answerSeq`, and checks from the log that B was sent before A was
  answered and is still unanswered. Releasing both at once failed it ("B is
  still unanswered"), and so did letting A's answer clear any pending save
  in the hub ("B is still pending after A's answer"). The follow-up below
  makes the test wait until A is held before it sends B.
- **The capture check could take minutes.** It now has a 20-second deadline
  with each probe capped at the time left.
- **One README rather than four files.** The change records README allows
  one `README.md` for small changes, and most fix records do that; this
  record keeps the single file.

The proof runs after these fixes found three more limits sized for an idle
machine. The large-move check had 600 ms for the moved root, the sub-issue
list and a 150 ms sub-issue read; it now has 3 seconds against 400 ms reads,
which still covers only a few of the 40. The banner test expected Python to
print within 5 seconds, now 60, and the NETT `wait_for` allows 30 seconds
instead of 10.

With the catch-up wait made to return as soon as the stream was live, the
live modules failed again in parallel runs (17 of 32 `tests::live` and 3 of
32 `tests::nett::live` runs), so the wait is what keeps them reliable.

## Follow-up review findings

The reviews left four medium and low findings open at merge, fixed in a
follow-up:

- **The catch-up counts could go out of step for good.** The hook compared
  two running totals, the streams that turned live and the catch-ups taken.
  A stream dropped after `ready` and before its catch-up, by a scope change
  or by closing within the 300 ms coalescing window, left the first total
  one ahead, and every later wait ran to its limit. Each `ready` now has an
  id, and the hook compares the latest `ready` with the latest one caught
  up, so the next `ready` replaces an abandoned catch-up.
  `a_stream_dropped_before_its_catch_up_leaves_nothing_owed` closes a
  stream straight after `ready` and waits for the next stream's catch-up;
  with the totals it timed out ("hub state did not reach expected
  outcome").
- **A held body stayed in the release queue after its limit.** The next
  `slowBody/release` then let that stale entry go and the body still held
  kept waiting. A hold now leaves the queue however its wait ends. With
  `waitForRelease` the limit can be up to 120 seconds (10 without it), and
  the late to-do test holds its answers for 120, longer than its own
  30-second waits, so only a release lets one go. The fixture test
  `test_a_held_body_whose_limit_ran_out_leaves_the_release_queue` failed
  before the change ("(200, 1) != (200, 0)").
- **The late to-do test could send B before A was held.** A commits before
  it joins the release queue, so B could join first and take A's release.
  The test now waits for A's `slowBodyHeadersStatus`, which the fixture
  logs after the hold has joined the queue, before it sends B.
- **A stream-end check could read another stream's record.** Records were
  matched by organisation and teams, which two streams can share, and why a
  stream ended was decided after delivery had stopped, so a close that came
  after the lifetime could take the credit. Each record now carries an id
  and the client's port, and the check wants exactly one record for its
  stream. The cause is decided under the delivery lock: past the deadline,
  a close or a revoke hook counts only if it came before the deadline. The
  fixture keeps the latest 1,024 records rather than every one.

The same pushes saw two tests outside this work fail once each under load:
`subspace-client-fleet`'s
`settings::tests::a_pasted_token_is_saved_replaced_and_cleared_from_settings`
and `subspace-space-lab-board`'s
`project_runtime::proxy::tests::project_runtime_proxy_has_eight_active_slots_and_stop_cancels_idle_tunnels`.
Their failure output was not kept, and the code does not show which wait
failed, so they were left as they were. The settings test was later traced
to the fleet's feed thread changing the link the page shows between the
frames of one click; see
[A followed fleet's link changes only when announced](../fleet-api-client/README.md#a-followed-fleets-link-changes-only-when-announced).
The proxy test has three wall-clock limits: a 2-second read timeout on each client,
a 100 ms connect to the fixture per attempt in the proxy, and a 2-second
bound on stopping the proxy.

## Evidence

Measured on an Apple silicon Mac with 12 cores, running the compiled test
binary directly while other builds came and went.

| Run | Before | After |
| --- | --- | --- |
| The broken-stream test, 4 loops in parallel | 7 of 200 failed | 200 of 200 passed |
| The broken-stream test, under 12 `yes` processes | none failed in 100 | 200 of 200 passed |
| `tests::nett::live`, 4 loops in parallel | 4 of 30 failed (1 loop) | 60 of 60 passed |
| The whole crate, 4 runs in parallel | 35 of 40 failed (part-way through the fix) | 40 of 40 passed |
| `tests::nett::live` and `tests::live` under 12 `yes`, 4 loops each | | 52 and 52 of 52 passed |
| The whole crate under 12 `yes`, 2 loops | | 20 of 20 passed |
| `mise run script-test`, idle and under 12 `yes` | | 3 and 3 of 3 passed |

After the review fixes:

| Run | Result |
| --- | --- |
| `tests::nett::live`, 4 loops in parallel | 60 of 60 passed |
| `tests::nett::live` and `tests::live` under 12 `yes`, 4 loops each | 52 and 52 of 52 passed |
| The whole crate, 4 runs in parallel, after the move-check and NETT limit fixes | 40 of 40 passed |
| The whole crate under 12 `yes`, 2 loops, before those two fixes | 17 of 20 passed; the 3 failures led to them |
| `mise run script-test`, idle and under 12 `yes` | 3 and 3 of 3 passed |

The `yes` processes paused whenever another build compiled, so the loaded
runs were loaded for most, not all, of their time.
