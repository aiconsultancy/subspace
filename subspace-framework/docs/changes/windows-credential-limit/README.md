# Long secrets in Windows Credential Manager

## Intent

Signing in on Windows must keep a session of any reasonable size. A sign-in
session with access, refresh and ID tokens is about 2,100 characters, but
`Keychain` saved each secret as one credential, and one Windows credential
holds at most 2,560 bytes (`CRED_MAX_CREDENTIAL_BLOB_SIZE`). The keyring
crate stores a password there as UTF-16, so nothing over 1,280 UTF-16 code
units could be saved. The save failed and the session never persisted. The
macOS Keychain and the Secret Service have no such limit.

The limit is enforced by the keyring crate before Windows is called:
`validate_password` in `windows-native-keyring-store` 1.1.0 (the Windows
store of `keyring` 4.2.0) encodes the password as UTF-16LE and returns
`TooLong` when the blob is over 2,560 bytes. The same crate limits the user
name, our account name, to 513 bytes and the target name,
`<account>.<service>`, to 32,767, and Windows compares target names without
case. Reading back, its `extract_password` returns an odd-length blob, or
one that is not valid UTF-16LE, as `BadEncoding` bytes rather than text.

## Spec

- `Keychain::load`, `save` and `delete` keep their signatures, and no caller
  changes. On Windows each works for secrets of at least 64 KiB.
- Every secret that fits one credential, 1,280 UTF-16 code units, is stored
  exactly as before: one credential under the account's name holding the
  secret itself, as text, verbatim. Text in that credential is always read
  back verbatim, whatever it looks like, so everything earlier builds wrote
  still loads, and earlier builds read every secret that fits.
- On Windows a longer secret is split. The account's credential holds a
  manifest written as bytes, through `set_secret`: `0xFF 0xDF`, then
  `subspace-keychain/chunks/v2/<generation>/<count>/<length>/<digest>`, 141
  bytes. `0xFF` never occurs in UTF-8, and as UTF-16LE the first code unit is
  `0xDFFF`, a low surrogate with nothing before it, so the bytes are text in
  neither encoding at any length, even cut short; the odd length alone makes
  Windows return them as bytes. The generation is 32 random hexadecimal
  digits, new for every save; the count (3 digits) and the secret's UTF-8
  length (9 digits) are zero-padded; the digest is the SHA-256 of the
  generation followed by the secret.
- Chunk `index` is the text `<generation>/<index>/` and its piece of the
  secret, under `subspace-keychain#<scope hash>/<generation>/<index>`. The
  scope hash is the first 32 hexadecimal digits of the SHA-256 of
  `<service length>:<service><account>`, with ASCII letters in lower case,
  so every service has its own namespace and derived names are at most 87
  bytes for any account. Pieces are split between characters and hold at
  most 960 UTF-16 code units; with its header of at most 37 units a chunk
  stays more than a fifth under the limit. A secret uses 2 to 256 chunks, at
  most 240 KiB of ASCII.
- On Windows, an account name that contains a NUL, or the marker
  `subspace-keychain#` in any case, or that is longer than 513 bytes, is
  refused with `KeychainError` before anything is read or written. A long
  secret under a service so long that a derived target name would exceed
  32,767 bytes is refused before anything is written. The callers' accounts
  (`session`, GitHub logins and `fleet-api:<fleet>`) are unaffected.
- A load fails with `KeychainError`, never returning anything but the saved
  secret, when the manifest cannot be read or describes a layout no save
  makes: fewer than 2 or more than 256 chunks, a secret that would have fitted
  in one credential, pieces other than the ones a save cuts (every piece but
  the last as full as the limit allows), or a count or length that disagrees
  with the pieces. It also fails when a chunk is missing, empty, not text,
  carries another save's header, or the chunks do not make a secret with the
  manifest's digest. A missing account credential is `None`.
- A save reads the account's credential, writes the new chunks, writes the
  account's credential, the commit, and then deletes the chunks of the one
  generation it read. It never searches the store. A save whose commit
  reports a failure reads the account's credential again and deletes its
  chunks only if that credential does not name them, since a write that
  reported a failure may have landed.
- A delete deletes the account's credential, the commit, then sweeps: it
  searches for the account's chunk names under this service and deletes
  every one that the account's credential, read again, does not name. The
  sweep is best effort. It skips names it cannot parse and items it cannot
  delete, and once the account's credential is gone nothing can make the
  delete return an error.
- Errors stay opaque: no file fallback, no provider details, and nothing logs
  a secret or a chunk. Copies of a secret or of its pieces made while
  splitting and joining, the bytes of a manifest read back and the SHA-256
  state are wiped when dropped (`zeroize`, and `sha2`'s `zeroize` feature);
  the caller's own `Secret` is not.

### Why the manifest is bytes in the account's own credential

Two designs keep every plain secret verbatim. One moves the manifest to a
separate derived credential and leaves the account's credential absent while
a secret is split. The other, chosen here, keeps the manifest in the
account's credential in a form the password path cannot produce.

Keeping it there leaves one commit point that every build, earlier ones
included, reads and writes. Whatever an earlier build does to that
credential supersedes the manifest: if it saves, its text wins; if it signs
out, the manifest goes with the credential, and no chunk can be read
without it. With a separate manifest, an earlier build that signed out would
delete only the account's credential, and a later upgrade would find the
old manifest and sign the user back in with a stale session.

### Why a save never searches and a delete always does

A save removes only the generation it read before writing. Of two saves
from different processes at once, the one that commits last is always
readable; the other's chunks are left over. If saves swept the store too,
one process's sweep could delete the chunks of another's save before that
save committed, and the secret committed last would fail to load. A
cross-process lock would avoid that, but it could not be tested from macOS,
so it is not used.

Sign-out is where removing everything for the account is the intent, so
the sweep runs there. It deletes the account's credential first, so that
whatever happens after, no secret is left to load. The sweep then never
fails the delete. If sign-out reported an error after the secret was gone,
a caller could keep its session in memory and write it back at the next
refresh. Within a process a lock serialises split loads, saves and deletes.

### Each step of a save and a delete

| Operation | Before the commit | After the commit |
| --- | --- | --- |
| Short to short | The previous secret. | The new secret. |
| Short to long | The previous secret; the new chunks are left over. | The new secret. |
| Long to short | The previous secret, whose chunks are untouched. | The new secret; the previous chunks remain until the save deletes them. |
| Long to long | The previous secret; the new chunks are left over. | The new secret; the previous chunks remain until the save deletes them. |
| Delete | The secret, and the delete reports the failure. | No secret; chunks remain until the sweep. |

The commit is one credential write or delete, so a save or delete that stops
at any step leaves either the previous state or the new one.

### What can be left over, and when it goes

Chunks that nothing names hold pieces of a secret, so they matter. They are
left by a save that stops before its commit or fails to delete the
generation it replaced, by the losing one of two concurrent saves, by an
earlier build that signs out or overwrites a split secret, and by a sweep
that cannot search or delete. None of them is ever read. The next sign-out
on this build removes them all: its sweep deletes every chunk of the account
under the service that the account's credential does not name. Until then
they stay in the user's credential store, under the same protection as the
live secret.

### Two processes at once

- Two saves: the last to commit stays readable, as above; a load that reads
  a manifest just before the other save deletes its chunks fails closed.
- A save and a delete: depending on the order, the save's secret is
  readable, there is no secret, or, when the delete's sweep removes chunks
  of the save before it commits, the load fails closed with
  `KeychainError`, which the app reports as the store being unavailable;
  the next sign-in replaces it. A load never returns a mix of two secrets or
  an older one: a manifest names only its own generation's chunks, every
  chunk carries that generation in its header, and the digest covers the
  whole secret.

### Names the sweep may touch

The search pattern matches only `<namespace><32 hexadecimal digits>/<digits>`
followed by `.` and the exact service, ignoring case as Windows does. Every
name found is then parsed, and deleted only if the target name re-derived
from its parts equals, case included, the name the store returned. Names of
another service, another account, another case of the generation, an index
with a leading zero or any other credential are left alone, and a found
credential whose name cannot be read is skipped.

Only ASCII letters are folded into the namespace. Windows treats accounts
that differ only in the case of another letter (`É` and `é`) as one
credential but they hash to different namespaces, so a split secret saved
under one cannot be loaded under the other and fails closed. No caller uses
such accounts.

### Earlier builds

- Every secret that fits, and every credential an earlier build wrote, is
  text and reads the same in both directions.
- A split secret's credential holds bytes, so an earlier build's load
  fails with its keychain error. It can still sign in: its save overwrites
  the credential with text, which then wins here too. Its sign-out deletes
  the manifest and leaves the chunks, which nothing can read and the next
  sign-out here sweeps.
- An earlier build cannot save a split secret at all, as before.

The drafts of this change, a text manifest with an escape for lookalike
secrets and then a one-byte tag, were never released, and a credential
holding the text draft now reads as the plain secret it looks like.

### Windows only

The split applies only where the limit exists. On macOS and Linux every
secret stays one item holding the secret itself, so nothing changes there.
The format code is the same on every platform. Only the limits differ,
chosen with `cfg!(windows)`, so the Windows path compiles everywhere and its
tests run on macOS and Linux against an in-memory store that holds every
credential as a blob keyed by target name, reads it back with keyring's
Windows rules, and refuses what Windows refuses: more than 2,560 bytes in a
credential, account names over 513 bytes and target names over 32,767.

## Delivery

`keychain.rs` keeps the public `Keychain`, the process lock and the private
`Store` trait: its service, get (text, or bytes the store cannot read as
text), set text, set bytes, delete, and find target names by pattern. Its
keyring implementation finds through `keyring_core::Entry::search`, which
`keyring::v1` does not re-export, and reads each found credential's target
name. `keychain/split.rs` holds the save and delete steps,
`keychain/split/read.rs` the load and its checks, and
`keychain/split/format.rs` the stored format. `subspace-core` now depends on
`keyring-core`, `sha2` (with its `zeroize` feature) and `zeroize`, all
already in the dependency tree; no lockfile gains a package.

The tests use that in-memory store. It can fail any step, crash there, or
let a write land while reporting a failure; run another operation before any
step; share its credentials between services; and return stray names from a
search. They cover the boundary at 1,280 units; round trips of ASCII and
two-, three- and four-byte characters around each boundary and at 64 KiB;
the exact largest chunk; the on-disk layout; literal earlier entries,
including text that looks like this format or its drafts, loading verbatim,
and what earlier builds read back; refused and long account and service
names and the ASCII-only folding; damaged, substituted, empty and missing
chunks; a manifest cut short at every length; layouts no save makes; a save
from each kind of secret to each other kind and a delete stopped at every
step in each of the three ways; a commit that lands but reports a failure;
interleaved saves, where the last committed stays readable; a save racing a
delete, which ends with the save's secret, no secret or a closed failure;
and a sweep that leaves another service's and another account's chunks and
foreign lookalike credentials alone, survives a failed search, skips stray
names and carries on past a failed delete.

A Windows-only test saves, overwrites and deletes a 5,000-character secret
through the real Credential Manager under a random service name, checks the
chunks through the real search, and checks how Windows reads back a
manifest cut short. It is ignored by default: no CI job runs on Windows,
and tests do not touch the real credential store. Run it on Windows with
`cargo test -p subspace-core -- --ignored keychain`.

The change was built and tested on macOS. `cargo check --target
x86_64-pc-windows-msvc` of the Framework stops at a C dependency of rustls
that needs the Windows SDK, so the keychain sources were type-checked and
linted for `x86_64-pc-windows-msvc` and `x86_64-pc-windows-gnu`, tests
included, in a scratch crate that compiles the same files against the same
crate versions. Nothing here has run on Windows: not the split, not the
search, not the ignored test, not a real sign-in. All four must run on
Windows before a release ships this.

## Review findings

The first review round raised these; each is resolved.

- A plain secret that looked like the manifest or its escape was misread,
  and earlier builds saw escaped secrets: the manifest is now bytes, and
  text is always verbatim.
- Derived names could alias an account through case or a NUL, and grew with
  the account: accounts with a NUL or the marker are refused, and derived
  names use a fixed-length hash.
- A same-length substituted chunk and a one-chunk, zero-length manifest were
  accepted: the manifest carries a digest, and impossible layouts fail.
- A delete removed chunks before the manifest, and a failed read could
  strand chunks: the manifest goes first, then a sweep.
- Orphaned chunks held whole secrets with nothing to remove them: the sweep
  removes them at sign-out.
- Copies of the secret were not wiped: they are, with `zeroize`.
- Missing tests: failing steps, a crash between the commit and the clean-up,
  interleaved operations, and the exact limits.
- `rustqual` reported no IOSP finding in the code named; its `match`
  dispatch rule accepts these functions, and the baseline is unchanged.

The second round found that the sweep added in the first could harm what
it was meant to protect, and raised smaller points; each is resolved.

- A save's sweep could delete the chunks of another process's save before
  it committed, so the secret committed last failed to load: saves no
  longer search (decision above).
- A failed sweep could fail a sign-out after the secret was gone: the sweep
  runs only in delete, after the commit, and never fails it.
- The search pattern's `.*` let one service's sweep reach another's names,
  the namespace did not include the service, and a found name was cut by
  the service's byte length after a case-insensitive match: the namespace
  hashes the service, the pattern is exact, and every name is re-derived and
  compared before it is deleted. The derived target name is bounded before
  any write, and only ASCII case is folded.
- A manifest cut short by one byte could read back as UTF-16 text: the tag
  is now text in neither encoding at any length, and layouts no save makes
  fail.
- A commit that landed but reported a failure lost its chunks: the save
  reads the account's credential again first. The hash state is wiped, the
  bytes of a read are wrapped at once, the sweep's selection is a pure
  function, and `zeroize` sits in its sorted place in the workspace list.

## Tasks

- [x] Split secrets over one credential across several on Windows.
- [x] Keep every secret that fits as one plain credential, verbatim.
- [x] Remove the replaced generation on save and every leftover at sign-out.
- [x] Test the format on every platform with an in-memory store.
- [x] Add the ignored Credential Manager test.
- [ ] Run the split, the search, the ignored test and a real sign-in on
      Windows before release.
