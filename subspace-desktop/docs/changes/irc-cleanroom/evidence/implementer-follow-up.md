# Clean-room implementation, round 2: robustness fixes

The follow-up brief was run with OpenAI Codex (`gpt-6.2-sol`,
`high` reasoning), with MCP servers disabled and network access
enabled in the workspace-write sandbox. Local repository details are generalised.

The [initial brief](implementer-brief.md) and [exclusion list](excluded.md)
still apply. You are the clean-room implementer, working only in the
isolated repository from the [functional specification](spec.md) and
the public standards. A code review of your modules found the items below. Fix them with focused commits (`fix(irc): …`), tests for each, and record each change in `subspace-desktop/clients/irc/CLEANROOM.md` (a short "Round 2" section: what changed and why, sources if any).

1. **Writes have no timeout.** `Link::deliver` awaits `write_all` and `flush` inline. While a write is blocked because the peer stops reading, the task services no ticks (so the ping timeout cannot fire) and no commands. On reconfiguration, `runtime::stop` sends `Shutdown` but never aborts the task, so a stuck task can keep a ghost connection holding the nick. Wrap the output write in `tokio::time::timeout` (for example 30 s, or the ping limit) and end the connection as lost on expiry; optionally abort the task's `JoinHandle` after a grace period in `runtime::stop`. Test with a fake stream whose `poll_write` stays `Pending`.
2. **Open batches are unbounded.** `Session.batches` and each buffered batch's messages grow without limit while a server or bouncer leaves batches open. Cap open batches (for example 64) and messages per buffered batch (for example 10,000, or a multiple of the ISUPPORT `CHATHISTORY` value); on overflow drop the oldest batch or deliver what is held. Test both caps.
3. **The nick fallback never ends.** Before `001`, each 432, 433, 436 or 437 appends `_` without limit; with 432 for a nick over NICKLEN every retry is longer and still invalid. After the alternative nicks plus about 3 underscores, or once NICKLEN would be exceeded, stop with a readable error state. Test it. Note this deliberate deviation from spec section 18.2 in CLEANROOM.md.
4. **SASL can stall registration.** If the server never answers `AUTHENTICATE`, `CAP END` is never sent. In `tick`, if SASL has been in progress for more than about 30 s, mark it finished and continue registration without an account (applying the spec's progress rules). Test it.
5. **CTCP replies are not rate-limited.** A burst of VERSION, PING or TIME queries produces one NOTICE each, which can trip server flood limits. Reply at most once per sender per few seconds, and to at most N queries a minute. Test it.
6. **Empty tag keys are dropped** (spec-conformant per the IRCv3 key grammar). Record this in CLEANROOM.md as a documented clarification of spec section 7.2.
7. **Toolchain floor.** The code needs a recent compiler (`str::floor_char_boundary`, let chains, `if let` guards) but no manifest declares `rust-version`. Declare the workspace's Rust version (match the version mise pins) in `subspace-desktop/Cargo.toml` `[workspace.package]` if crates inherit from it, or in the crate's manifest otherwise.

Re-run the crate tests, `cargo test -p subspace-irc-board --locked`, fmt, clippy with `-D warnings` and desktop `make check` (Python 3.12 first on PATH, via `mise exec --`). Do not run anything that needs the owner's shared Docker containers restarted. Commit unsigned; the guard hook runs. Final report (at most 250 words): commits, test results, anything left.
