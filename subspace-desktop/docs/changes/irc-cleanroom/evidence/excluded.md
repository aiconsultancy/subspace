# Excluded material: what the clean-room implementer must not see

Companion to `spec.md`. The implementer of the rewrite must not open, search, diff, blame, summarise (or ask an
agent or tool to summarise) any of the material below, before or during the work. If any of it is opened by
mistake, stop and tell the owner; the owner decides whether that person can continue.

Local locations of excluded copies are described generically in section 4.

## 1. The 15 files, in every version

| # | Path |
| ---: | --- |
| 1 | `subspace-desktop/clients/irc/src/codec.rs` |
| 2 | `subspace-desktop/clients/irc/src/connection/mod.rs` |
| 3 | `subspace-desktop/clients/irc/src/connection/tls.rs` |
| 4 | `subspace-desktop/clients/irc/src/network.rs` |
| 5 | `subspace-desktop/clients/irc/src/proto/mod.rs` |
| 6 | `subspace-desktop/clients/irc/src/proto/ctcp.rs` |
| 7 | `subspace-desktop/clients/irc/src/proto/parse.rs` |
| 8 | `subspace-desktop/clients/irc/src/proto/serialise.rs` |
| 9 | `subspace-desktop/clients/irc/src/proto/tests.rs` (its test inputs were copied from Halloy) |
| 10 | `subspace-desktop/clients/irc/src/session/mod.rs` |
| 11 | `subspace-desktop/clients/irc/src/session/batch.rs` |
| 12 | `subspace-desktop/clients/irc/src/session/caps.rs` |
| 13 | `subspace-desktop/clients/irc/src/session/isupport.rs` |
| 14 | `subspace-desktop/clients/irc/src/session/messages.rs` |
| 15 | `subspace-desktop/clients/irc/src/session/registration.rs` |

Each starts with a line `// Adapted from Halloy (GPL-3.0): …`. The same files existed earlier at
`subspace-desktop/subspace/clients/irc/src/…` before the folder move; those paths are excluded
too. The in-file test module of `codec.rs` is part of file 1.

**Practical rule while working in the repository:** before the rewrite, move the 15 files out of the working tree
(or have someone else delete them on your branch) so that editors, search tools, language servers and agents cannot
show them. Start from a branch where they are already deleted or replaced by empty stubs, and do not check out older
commits.

## 2. Repository history and derived artefacts

- Every commit's version of the 15 files: `git log -p`, `git show <rev>:<path>`, `git blame`, `git diff` across the
  rewrite, `git grep` over history, `git stash` entries, reflogs, and the object store and pack files.
- Pull request diffs, review comments and commit views that show these files, on GitHub or elsewhere, for this
  repository and for any fork of it.
- Patches, bundles and archives that contain them (release source archives, `git format-patch` output, backups).
- Build output generated from them: rustdoc source pages (`target/doc/src/subspace_client_irc/…`), expanded
  macros, coverage reports with source, and language-server caches.

## 3. Third-party sources

- Halloy's repository and any copy, fork, vendored crate or excerpt of it: <https://github.com/squidowl/halloy>
  (packages `halloy`, `irc`, `irc_proto`, `data`), including its tests.
- More generally, the source code of other IRC clients and IRC libraries while writing these modules. Their
  documentation of public APIs is fine, as are the normative references in `spec.md` 0.5 and the documentation of
  this crate's dependencies (rustls, rustls-platform-verifier, tokio, tokio-rustls, base64, chrono, thiserror).

## 4. Known copies (19 November 2026)

The exclusions also cover copies in the main clone, other worktrees, a downstream fork snapshot,
coordination logs and patches, and the specification author's temporary work and build artefacts.

## 5. Soft exclusions (no code, but written by people who read the code)

The implementer SHOULD NOT read these. They contain no source text, but compare the local code with Halloy's in a
way that could steer an implementation:

- The pre-rewrite licence review, which names upstream constructs and assesses similarity for each file;
- conversation logs, transcripts or review notes of agents or people who worked on the 15 files.

## 6. Material the implementer may use

- `spec.md`, `interface-callers.md`, this file, and `acceptance/*.rs` in this folder.
- Every other file in the repository, at the current commit: in particular the rest of `clients/irc` (sibling
  modules of `session`, `network/offline.rs`, `network/secrets.rs`, `model.rs`, `runtime.rs`, `test_support.rs`,
  `format.rs`, all tests and the example), `spaces/irc`, and the documentation. A search of the repository for
  "Halloy" finds, outside the 15 files, only prose with no code: `subspace-desktop/AGENTS.md` (Structure),
  `subspace-desktop/docs/changes/irc-board/spec.md` and `intent.md`, `subspace-desktop/docs/changes/irc-completion/intent.md`
  and `subspace-desktop/docs/changes/framework-split/research.md`. Those may be read.
- The normative references listed in `spec.md` 0.5 and the dependencies' documentation.

## 7. Residual risk the owner should know about

- The rest of the crate was written alongside the 15 files by authors who had seen Halloy. The licence note found no
  Halloy header and judged it not derived, and this spec's author found no doc snippets or comments quoting Halloy
  outside the 15 files. The modules closest in shape to a Halloy counterpart are `session/dispatch.rs` (a command to
  handler table, largely dictated by the protocol) and `session/numerics.rs`; they stay visible because the
  rewritten modules must plug into them. A second opinion from someone who may read Halloy (not the implementer)
  could confirm that these two are original.
- Interface items that the callers fix (names and shapes of `Message`, `Session` fields, handler names) carry over
  by necessity; `spec.md` frees every item that callers do not need.
