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
case.

## Spec

- `Keychain::load`, `save` and `delete` keep their signatures, and no caller
  changes. On Windows each works for secrets of at least 64 KiB.
- Every secret that fits one credential, 1,280 UTF-16 code units, is stored
  exactly as before: one credential under the account's name holding the
  secret itself, as text, verbatim. Text in that credential is always read
  back verbatim, whatever it looks like, so everything earlier builds wrote
  still loads, and earlier builds read every secret that fits.
- On Windows a longer secret is split. The account's credential holds a
  manifest written as bytes, through `set_secret`, that no password write
  can produce: the byte `0xFF`, which never occurs in UTF-8, then
  `subspace-keychain/chunks/v2/<generation>/<count>/<length>/<digest>`, 141
  bytes. A password is UTF-16 on Windows, always an even number of bytes, so
  Windows hands the odd-length manifest back as `BadEncoding` bytes rather
  than text, as the other stores would for bytes that are not UTF-8. The
  generation is 32 random hexadecimal digits, new for every save; the
  count (3 digits) and the secret's UTF-8 length (10 digits) are
  zero-padded; the digest is the SHA-256 of the generation followed by the
  secret.
- Chunk `index` is the text `<generation>/<index>/` and its piece of the
  secret, under `subspace-keychain#<account hash>/<generation>/<index>`. The
  account hash is the first 32 hexadecimal digits of the SHA-256 of the
  account in lower case, so derived names are at most 87 bytes for any
  account and accounts that Windows treats as one share one set of chunks.
  Pieces are split between characters and hold at most 960 UTF-16 code
  units; with its header of at most 37 units a chunk stays more than a fifth
  under the limit. A secret uses 2 to 256 chunks, at most 240 KiB of ASCII.
- On Windows, an account name that contains a NUL, or the marker
  `subspace-keychain#` in any case, or that is longer than 513 bytes, is
  refused with `KeychainError` before anything is read or written. The callers' accounts
  (`session`, GitHub logins and `fleet-api:<fleet>`) are unaffected.
- A load fails with `KeychainError`, never returning anything but the saved
  secret, when the manifest cannot be read or describes an impossible
  layout (fewer than 2 or more than 256 chunks, a length shorter than the
  chunk count, malformed fields), or when a chunk is missing, empty, not
  text, carries another save's header, or the chunks do not make a secret
  of the manifest's length and digest. A missing account credential is
  `None`.
- A save writes the new chunks, then the account's credential, the commit,
  then sweeps: it lists the account's derived names and deletes every one
  that the account's credential, read afresh, does not name. A save that
  fails before its commit deletes the chunks it wrote. The sweep after a
  commit is best effort, since the secret is saved and the next save or
  delete sweeps again.
- A delete deletes the account's credential, the commit, then sweeps every
  derived name of the account, returning an error if the sweep fails.
- Errors stay opaque: no file fallback, no provider details, and nothing logs
  a secret or a chunk. Copies of a secret or of its pieces made while
  splitting and joining are wiped when dropped (`zeroize`); the SHA-256
  state and the caller's own `Secret` are not.

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

### Each step of a save and a delete

| Operation | Before the commit | After the commit |
| --- | --- | --- |
| Short to short | The previous secret. | The new secret. |
| Short to long | The previous secret; unnamed new chunks remain until a sweep. | The new secret. |
| Long to short | The previous secret, whose chunks are untouched. | The new secret; previous chunks remain until the sweep. |
| Long to long | The previous secret; unnamed new chunks remain until a sweep. | The new secret; previous chunks remain until the sweep. |
| Delete | The secret. | No secret; chunks remain until the sweep. |

The commit is one credential write or delete, so a save or delete that stops
at any step leaves either the previous state or the new one, and a later
save or delete removes whatever it left behind.

### Two processes at once

Within a process a lock serialises split loads, saves and deletes. Across
processes, one process's sweep can delete the chunks of another process's
save that is not yet committed, or of one committed after the sweep read
the account's credential. That save then commits a manifest whose chunks are
partly gone, and a load fails closed with `KeychainError`: the app reports
the store as unavailable and the next sign-in replaces the secret. A load
never returns a mix of two secrets: a manifest names only its own
generation's chunks, every chunk carries that generation in its header, and
the digest covers the whole secret.

### Earlier builds

- Every secret that fits, and every credential an earlier build wrote, is
  text and reads the same in both directions.
- A split secret's credential holds bytes, so an earlier build's load
  fails with its keychain error. It can still sign in: its save overwrites
  the credential with text, which then wins here too. Its sign-out deletes
  the manifest and leaves the chunks, which nothing can read and the next
  save or delete here sweeps.
- An earlier build cannot save a split secret at all, as before.

The first draft of this change stored the manifest as text with an escape
for lookalike secrets. It was never released, and a credential holding that
text now reads as the plain secret it looks like.

### Windows only

The split applies only where the limit exists. On macOS and Linux every
secret stays one item holding the secret itself, so nothing changes there.
The format code is the same on every platform. Only the limits differ,
chosen with `cfg!(windows)`, so the Windows path compiles everywhere and its
tests run on macOS and Linux against an in-memory store that refuses exactly
what Windows refuses: more than 2,560 bytes in a credential and account
names over 513 bytes.

## Delivery

`keychain.rs` keeps the public `Keychain`, the process lock and the private
`Store` trait: get (text, or bytes the store cannot read as text), set text,
set bytes, delete, and list account names by prefix. Its keyring
implementation lists through `keyring_core::Entry::search`, which
`keyring::v1` does not re-export, with a case-insensitive pattern over the
target names `<account>.<service>`, and reads each found credential's target
name. `keychain/split.rs` holds the save, load, delete and sweep steps, and
`keychain/split/format.rs` the stored format. `subspace-core` now depends on
`keyring-core`, `sha2` and `zeroize`, all already in the dependency tree; no
lockfile gains a package.

The tests use an in-memory store that enforces the Windows limits, can fail
any step once or crash there, and can run another operation before any
step. They cover the boundary at 1,280 units; round trips of ASCII and two-,
three- and four-byte characters around each boundary and at 64 KiB; the
exact largest chunk; the on-disk layout; literal earlier entries, including
text that looks like either draft of this format, loading verbatim, and
what earlier builds read back; refused and long account names; damaged,
substituted, empty and missing chunks; impossible manifests; a save from
each kind of secret to each other kind and a delete stopped at every step,
as an error and as a crash; and saves and deletes interleaved before every
step of another save, which end in one whole secret, no secret or a closed
failure, never a mix.

A Windows-only test saves, overwrites and deletes a 5,000-character secret
through the real Credential Manager under a random service name and checks
the chunks through the real search. It is ignored by default: no CI job
runs on Windows, and tests do not touch the real credential store. Run it on
Windows with `cargo test -p subspace-core -- --ignored keychain`.

The change was built and tested on macOS. `cargo check --target
x86_64-pc-windows-msvc` of the Framework stops at a C dependency of rustls
that needs the Windows SDK, so the keychain sources were type-checked and
linted for `x86_64-pc-windows-msvc` and `x86_64-pc-windows-gnu`, tests
included, in a scratch crate that compiles the same files against the same
crate versions. Nothing here has run on Windows: not the split, not the
search, not a real sign-in.

## Review findings

The first review round raised these; each is resolved in this change.

- A plain secret that looked like the manifest or its escape was misread,
  and earlier builds saw escaped secrets: the manifest is now bytes, and
  text is always verbatim.
- Derived names could alias an account through case or a NUL, and grew with
  the account: accounts with a NUL or the marker are refused, and derived
  names use a fixed-length hash.
- A same-length substituted chunk and a one-chunk, zero-length manifest were
  accepted: the manifest carries a digest, and impossible layouts fail.
- A delete removed chunks before the manifest, and a failed read could
  strand chunks: the manifest goes first, then a sweep of every derived
  name.
- Orphaned chunks held whole secrets with nothing to remove them: every save
  and delete sweeps the account's derived names.
- Copies of the secret were not wiped: they are, with `zeroize`, already in
  the dependency tree.
- Missing tests: failing get and delete steps, a crash between the manifest
  and the sweep, interleaved operations, and the exact limits.
- `rustqual` reported no IOSP finding in the code named; its `match`
  dispatch rule accepts these functions, and the baseline is unchanged.

## Tasks

- [x] Split secrets over one credential across several on Windows.
- [x] Keep every secret that fits as one plain credential, verbatim.
- [x] Sweep chunks the account's credential does not name.
- [x] Test the format on every platform with an in-memory store.
- [x] Add the ignored Credential Manager test.
- [ ] Run the ignored test and a real sign-in on Windows.
