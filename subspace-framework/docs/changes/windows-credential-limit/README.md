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
`<account>.<service>`, to 32,767.

## Spec

- `Keychain::load`, `save` and `delete` keep their signatures, and no caller
  changes. On Windows each works for secrets of at least 64 KiB.
- A secret that fits one credential, 1,280 UTF-16 code units, is stored
  exactly as before: one credential under the account's name holding the
  secret itself. Earlier builds read it, and a secret an earlier build saved
  loads unchanged.
- On Windows a longer secret is split. The account's credential holds a
  manifest, `subspace-keychain/chunks/v1/<generation>/<count>/<bytes>`, where
  the generation is 32 random hexadecimal digits, new for every save, and
  `bytes` is the secret's UTF-8 length. Chunk `index` is stored under
  `<account>#chunk/<generation>/<index>` as `<generation>/<index>/` and its
  piece of the secret. Pieces are split between characters and hold at most
  960 UTF-16 code units, so a chunk, with its header of at most 37 units,
  stays more than a fifth under the limit. A secret may use at most 256
  chunks, 240 KiB of ASCII.
- A secret that itself starts with `subspace-keychain/` is stored as
  `subspace-keychain/plain/` and the secret, so it cannot be read as a
  manifest. Any other value with that prefix is a format this build cannot
  read.
- A save writes the new chunks, then the manifest, and only then deletes the
  previous generation's chunks. A save that stops part way leaves the
  previous secret readable, and one that fails deletes the chunks it wrote.
- A load fails with `KeychainError`, never part of a secret, when a chunk is
  missing, belongs to another save, has the wrong length, or the manifest
  cannot be read. A missing account credential is still `None`.
- A delete removes the chunks, then the manifest, so a delete that stops part
  way can be repeated. Missing pieces are not an error.
- Errors stay opaque: no file fallback, no provider details, and nothing logs
  a secret or a chunk.

### Windows only

The split applies only where the limit exists. On macOS and Linux every
secret, including one that starts with the manifest prefix, stays one item
holding the secret itself, so nothing changes for existing users there or
for older builds reading what newer builds write. Splitting everywhere above
a threshold would change the stored format on platforms that do not need it
and give older builds there secrets they cannot read.

The format code is the same on every platform. Only the limits differ, chosen
with `cfg!(windows)`, so the Windows path compiles everywhere and its tests
run on macOS and Linux against an in-memory store that refuses items over
the Windows limit.

## Delivery

`keychain.rs` keeps the public `Keychain` and puts the system store behind a
private `Store` trait (get, set and delete by account name). The keyring
implementation is `System`; the tests use an in-memory store that enforces
the Windows limit and can fail any write, either once or as a crash after
which nothing more is written. The `keychain/split` module holds the format.
A save or delete reads the previous account credential on a best-effort
basis: one that cannot be read is replaced or deleted, as earlier builds did,
at the cost of any chunks it named. Split operations take a process-wide
lock, so a load never reads a manifest whose chunks a save in the same
process is deleting. Saves from two processes at once can leave unreferenced
chunks or a transient load failure, never a mix of two secrets.

The unit tests cover a secret that fits staying one plain item at the limit;
round trips of ASCII, two-, three- and four-byte characters around each
boundary and at 64 KiB; the on-disk layout; overwrites from long to short and
back with no orphan chunks; delete with and without missing chunks; a single
item from an earlier build; missing, truncated and foreign chunks; unreadable
manifests; a secret that looks like a manifest; a save stopped at each write,
as an error and as a crash; the chunk limit; and the unchanged single-item
behaviour where there is no limit.

A Windows-only test saves, overwrites and deletes a 5,000-character secret
through the real Credential Manager under a random service name and checks
that every chunk is removed. It is ignored by default: no CI job runs on
Windows, and tests do not touch the real credential store. Run it on Windows
with `cargo test -p subspace-core -- --ignored keychain`.

The change was built and tested on macOS. `cargo check --target
x86_64-pc-windows-msvc` of the Framework stops at a C dependency of rustls
that needs the Windows SDK, so the keychain sources were type-checked and
linted for `x86_64-pc-windows-msvc` and `x86_64-pc-windows-gnu`, tests
included, in a scratch crate that compiles the same files against the same
`keyring` and `windows-native-keyring-store` versions. They have not yet run
on Windows.

## Tasks

- [x] Split secrets over one credential across several on Windows.
- [x] Keep short and earlier secrets as one plain credential.
- [x] Test the format on every platform with an in-memory store.
- [x] Add the ignored Credential Manager test.
- [ ] Run the ignored test and a real sign-in on Windows.
