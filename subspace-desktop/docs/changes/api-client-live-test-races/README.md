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

The tests now wait for those reads instead of a fixed time:

- In `tests::live`, for the list head read after `ready`, which only the
  catch-up makes, and then for the hub's live reads of the issue and the
  list to be neither out nor owed (`Hub::live_reads_settled`, built for
  tests only). Counting the issue reads alone was not enough: until the
  stream is live the hub polls the open issue every 300 ms, and those reads
  looked like the catch-up's.
- In `tests::nett::live`, until the mock has answered the list read after
  the stream opened, and then for the same settled reads. The NETT mock
  marks each logged request `answered` once its whole answer is written.
  Waiting for the pull request read that the first issue read's landing
  sends was not enough: an issue read owed while it was out followed it.

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
  machine A had already landed and been read by then. The NETT mock now
  records `answeredAt`, and the test waits for a list read at or after the
  time the mock answered A.
- In the NETT fixture's own tests, the slow-body test checks the headers the
  fixture recorded as sent and keeps only lower bounds; stream-ending checks
  require the stream to end before its own lifetime, which those classes
  set to 20 seconds; and the trickling-upload probe has 3 seconds against a
  5-second body deadline instead of 0.7 against 1.
- The board-docs linear-scan check allows 20 seconds instead of 2, the
  desktop-session test waits for its command to start before interrupting
  it, and the capture test allows 20 seconds for its fixture servers to
  stop.

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

The `yes` processes paused whenever another build compiled, so the loaded
runs were loaded for most, not all, of their time.
