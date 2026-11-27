# Publish macOS releases from a release runner

The owner asked for a dedicated Mac to build, sign and notarize releases, so
a release no longer has to wait for a signer's desktop session. The release
flow and its checks stay those of `make release-macos`; only where it runs
and where the credentials come from change.

## Behaviour

- `.github/workflows/release-macos.yml` (**Release macOS**) runs by hand on
  `main` with a stable draft's tag, on the organisation's self-hosted runner
  group `subspace-macos-signing` (label `macos-signing`). The group admits
  only this workflow on `main`. The `subspace-macos-signing` environment
  holds the signing secrets, allows only `main` and needs a maintainer's
  approval.
- The job's first step, `main`'s code, checks through GitHub's API that the
  tag names a draft release at a commit already on `main`; the job then
  checks out that commit, not the tag name, so only reviewed code runs with
  the secrets. It installs the pinned tools without rustqual and runs
  `scripts/package-macos-ci.sh --run make release-macos`. The wrapper
  builds the temporary Keychain from the environment's secrets, as before,
  then runs the command in it, from the caller's directory and with the
  caller's umask, with `SUBSPACE_MACOS_KEYCHAIN`, `SIGNING_IDENTITY` and
  `KEYCHAIN_PROFILE` set and the raw credentials unset, and deletes the
  Keychain on success or failure. Without `--run` it packages one app, as
  before, now also with the caller's umask rather than the credential
  files' private one.
- `release-macos.sh` no longer moves into the desktop session when it has a
  Keychain of its own (`SUBSPACE_MACOS_KEYCHAIN`), as `package-macos.sh`
  already did. A signer's Mac is unchanged.
- A release published with the workflow's token starts no other workflow, so
  the job starts packslip on the tag itself.
- Pre-releases and integration builds are still made on a signer's Mac.
- `--run` also lets another build of the app, such as an organisation's own
  build from a pinned commit, sign in the same temporary Keychain without
  copying the wrapper.

## Checks

- `scripts/test-package-macos-ci.sh`: `--run` gives the command the
  temporary Keychain, the identity and the notary profile without the raw
  credentials, in the caller's directory and umask, and packages nothing of
  its own; a failing command keeps its exit status and still deletes the
  Keychain and restores the search list; `--run` without a command stops
  before a Keychain exists.
- `scripts/test-release-macos.sh`: with its own Keychain, the release stays
  in its shell instead of the desktop session.
- `make check` in `subspace-desktop`, root `make release-check` and
  `actionlint` on the workflows (`.github/actionlint.yaml` names the
  runner's label).
