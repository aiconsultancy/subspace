# ACP turn completion and bounded Stop

## Intent

SUBSPACE-JT-1116-ACP: jt reported a Codex ACP answer remaining busy for more
than 25 minutes after two Subspace MCP calls, with Stop unable to release
the tab. A marketplace warning also appeared during the turn.

## Proven cause and specification

The existing runtime already replied to unsupported agent requests with
`-32601`, dispatched permission requests without blocking its reader,
ignored unknown updates and notifications, and emitted `TurnDone` for every
prompt result or error. Those paths do not prove the reported initial hang.
The shared stdio reader rejects invalid JSON as a process failure; valid
unknown notification methods do not stop it.

A second code-proven path could reopen a completed host turn: the runtime
accepted transcript chunks even after the prompt response. The host resumes
its running status on a delta, while the ACP runtime remains idle and its
old Stop implementation returned without emitting a turn end. Live
transcript updates now apply only during a prompt; session-load replay and
idle session metadata still work. Stop also emits a turn end when ACP is
already idle, clearing any stale host state.

The proven defect was unbounded cancellation: `cancel` sent `session/cancel`
and relied entirely on a later prompt response. Without that response,
`busy` stayed true and the host's running turn never received `TurnDone`.
Input writes also had a 30-second timeout, longer than a short Stop grace.

- Stop gives the agent the named `CANCEL_GRACE_PERIOD` of three seconds,
  including the cancellation write and outstanding permission replies.
- Waiting permission requests receive cancelled outcomes immediately; ones
  arriving after Stop also receive cancelled outcomes. Tracked request IDs
  prevent duplicate replies from delayed approval callbacks.
- On expiry the runtime uses the ordinary turn end, confirms streamed text,
  aborts unfinished calls, kills and reaps the child and starts a replacement.
- The replacement reloads the old session when supported, otherwise opens a
  new session. Displayed transcript, title and row identity remain intact;
  replay does not duplicate or replace the locally retained transcript.
- Output and approval callbacks carry process identity, so old responses,
  notifications and exit messages cannot affect the replacement.
- The next message can be accepted during replacement and is delivered once
  the replacement is ready. Stop can also discard that queued message.
- Existing unknown-method, unknown-update, warning and prompt-stop-reason
  behavior is preserved and covered with the existing fake ACP agent.

The initial repair changed no visible copy, layout, controls or styling.
The review follow-up adds the requested restart failure error and retry
instruction. Layout, controls and styling remain unchanged. Native screenshot
evidence for that error cannot be captured on this Linux worker; the supported
`make capture` launcher requires macOS.

## Delivery

The real Codex preset was probed headlessly once with `Reply with OK`, using
the workstation's existing CLI sign-in, a temporary empty working directory,
no MCP tools and no file or terminal capability. A temporary probe read
protocol records in memory and printed only direction, method and ID;
stderr and payloads were never logged. The [method capture](protocol.txt)
shows setup, an `_auth/status_update` notification, six session updates and
the prompt response. It did not reproduce the reported hang and cannot
verify the original two-tool interaction or marketplace warning.

Tests reuse the shell fake agent and real actor. They cover unsupported
file, terminal and extension requests, a pending permission at Stop, a
permission arriving after Stop, unknown update kinds, warning notifications,
all documented stop reasons plus an unknown reason and a JSON-RPC error,
and cancellation with an agent that never answers. The cancellation boundary
uses paused Tokio time, checks immediately before and at expiry, and covers
both session-load capability choices, queued input, reused tool IDs and stale
process output.
An additional fixture emits a thinking chunk after its prompt response and
proves it cannot reopen the host turn; idle Stop still emits a turn end.

Gate and quality results are recorded in the PR body at the published head.
No dependency version or lockfile changes: Tokio's existing test utilities
are enabled only as a dev dependency for controlled time.

## Tasks

- [x] Read the reader, dispatch, turn state, repository rules and checklist.
- [x] Prove the cancellation gap without attributing unobserved causes.
- [x] Add bounded cancellation and process-isolated recovery.
- [x] Reuse the fake agent for regression coverage.
- [x] Run one method-only headless Codex protocol capture.
- [x] Update the ACP behavior documentation and change record.
- [x] Pass workspace, quality, interface and dependency gates with the
  quality baseline unchanged; prepare signed publication for the captain's PR.

## Cross-model review

All five accepted findings are addressed in a single follow-up repair:

1. **P1, failed restart:** the actor retains a conversation snapshot until
   replacement setup and selection restoration succeed. Failed spawn and
   handshake paths keep the snapshot, remove the dead process from tab order,
   report a clear restart error and retry on the next message. The tests
   `failed_spawn_retains_the_conversation_and_next_message_retries` and
   `failed_handshake_retains_the_conversation_and_next_message_retries` failed
   before the fix and now verify the retained history, title and creation time.
2. **P1, exit during Stop:** an exit with a cancellation deadline follows the
   same recovery as expiry. The process-exit fixture tests immediate exit and
   exit just before the deadline, checks one cancellation `TurnDone`, and
   completes the next message through a new process. Ordinary exits keep their
   existing behavior.
3. **P2, reused permission ID:** every tracked request includes a local
   generation, carried in its approval callback. Both wire ID and generation
   must match. The stale-callback regression holds an old callback across
   Stop and a reused wire ID; the new request waits for its own decision.
4. **P2, model and effort:** snapshots retain both choices. Both new and loaded
   sessions restore offered values sequentially, consume configuration replies,
   and publish the confirmed selection before readiness and queued input.
   The selection regression covers both setup paths and checks the final
   `SelectedModel` event against the confirmed model and effort.
5. **P2, running deadline:** a real listener/child fixture uses paused Tokio
   time to trigger automatic recovery, confirms a replacement and completes
   the next turn. It injects a late old prompt response while the new turn is
   awaiting approval and verifies it cannot end that turn. Disabling the
   listener's deadline action made this new test fail waiting for `TurnDone`;
   the action was restored before implementation verification. No production
   timer branch is disabled in the delivered change.

The four initial defect tests failed on the reviewed commit; the model test
also failed before the selection changes. These regressions use only fake
agents and local fixtures. No live Codex or application service was called
for the follow-up. The original two-MCP-call incident remains unreproduced.

The branch was rebased onto the newly fetched integration branch containing
the notification-sounds repair. Final signed heads and gate results belong
in the PR body; the quality baseline is not refreshed to hide new findings.
