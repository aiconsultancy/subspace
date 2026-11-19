# Clean-room implementation of the IRC client protocol layer

The initial brief was run with OpenAI Codex (`gpt-6.2-sol`,
`xhigh` reasoning), with MCP servers disabled and network access
enabled in the workspace-write sandbox. Local paths are generalised.

You are the **clean-room implementer**. You write 15 Rust modules of the `subspace-client-irc` crate from a written functional specification and public standards. Nobody has shown you the previous implementation, and you must keep it that way.

- Repository: the isolated repository (a standalone Git repository with one baseline commit). Work and commit only there.
- Brief folder (read-only), containing `spec.md` (the functional spec, your primary source), `interface-callers.md` (what the rest of the code base uses), `excluded.md` (material you must not see), `acceptance/*.rs` (acceptance tests to add and pass).

## Clean-room rules (strict; the licence of the result depends on them)

1. Read `excluded.md` first and follow it. In short: read nothing outside the repository and the brief folder except the public normative references cited in `spec.md` (RFC 1459/2812, IRCv3 specifications at ircv3.net, modern.ircdocs.horse, the CTCP draft) and the official documentation of the crate's dependencies (docs.rs for rustls, rustls-platform-verifier, tokio, tokio-rustls, base64, chrono, thiserror, and so on).
2. Do not look at the source code of any other IRC client or IRC library, on the web, on crates.io, in Cargo's local registry cache, or anywhere else. In particular never open Halloy (github.com/squidowl/halloy) or the `irc`/`irc-proto` crates' source. Do not add IRC protocol crates as dependencies.
3. The 15 module files do not exist in this repository and never have in its history. Do not try to recover them from anywhere: other folders on this computer, git objects elsewhere, caches, build output, or the network.
4. If you see excluded material by accident, stop, write down what you saw and where in your final report, and don't continue the affected module.
5. Keep a short clean-room log in `subspace-desktop/clients/irc/CLEANROOM.md`: the date, that the modules were written from `spec.md` and the cited standards only, the sources you consulted (URLs), and any deviation. That file is evidence, so keep it factual.

## The work

1. Read `spec.md` completely, then `interface-callers.md`. Read the rest of the crate (`subspace-desktop/clients/irc/src/` siblings such as `lib.rs`, `model.rs`, `runtime.rs`, `session/dispatch.rs`, `session/numerics.rs`, `network/offline.rs`, `network/secrets.rs`, `format.rs`, `test_support.rs`, tests, the example) and its callers (`subspace-desktop/spaces/irc`) so your modules plug in.
2. Write the 15 files listed in `excluded.md` section 1 at the same paths: `codec.rs`, `connection/{mod,tls}.rs`, `network.rs`, `proto/{mod,ctcp,parse,serialise,tests}.rs`, `session/{mod,batch,caps,isupport,messages,registration}.rs`. Your own design and code throughout. Write `proto/tests.rs` fresh from the specs (use spec examples or invent inputs). Start every file with the crate's normal header style (no "Adapted from" line).
3. Add the acceptance tests from `acceptance/*.rs` where `spec.md` says they belong, and make them pass together with every existing test in the crate and in `spaces/irc`.
4. Quality bar: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` (as the repo configures it), and the crate's tests. Then run the desktop workspace checks the repo prescribes (`subspace-desktop/Makefile`: `make check`, using `mise`; run `mise trust` in the repo first if asked). Put the Python 3.12 executable first on `PATH`.
5. Commit in small logical conventional commits (`feat(irc): …`), unsigned (`git -c commit.gpgsign=false commit`). A pre-commit guard hook runs; never bypass it. Use British English in comments and docs. Do not push; there is no remote.

## Final report (at most 350 words)

HEAD SHA and commits; test and check results with commands; any spec gaps you resolved and how; any excluded material you came across; follow-ups.
