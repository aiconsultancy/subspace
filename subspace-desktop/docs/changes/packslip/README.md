# packslip for releases

Back to the [change records](../README.md).

## Intent

The owner asked to add [packslip](https://packslip.dev) to releases: a
signed manifest beside the release files that installers and people can
verify, instead of trusting a checksum file published next to the download.

## Spec

- `.github/workflows/packslip.yml` runs when a release is published (tags
  `subspace-desktop-v*`) and by hand with a tag. Releases are still built,
  notarized and published from a desktop session; the workflow only signs.
- One job (`contents: read`, `id-token: write`) downloads
  `Subspace_*_darwin_arm64.zip` from the release and runs the
  `jdx/packslip` action pinned to v1.11.4's commit: project
  `github.com/aiconsultancy/subspace/subspace-desktop` (a monorepo subpath
  matching the tag prefix), the version from the tag, command `subspace` at
  `Subspace.app/Contents/MacOS/subspace`, and an `app` resource for
  `Subspace.app`. packslip reads `darwin` and `aarch64` from the file name.
  A second job (`contents: write`) attaches
  `packslip.subspace-desktop.sigstore.json` to the release, so the signing
  action never holds release write access.
- Keyless signing with the workflow's identity: no key to store. Sigstore's
  public transparency log records each signature with the repository and
  workflow names.
- `attest: false`: GitHub attestations need Enterprise Cloud in a private
  repository (the organisation is on Team).
- Docs: the installation guide (packslip, verify a download), AGENTS.md
  release notes and ways of working.

## Delivery

A local trial with a throwaway key (`packslip create --key --no-log`) on the
0.8.0 zip recorded `darwin`/`aarch64`/`zip`, the `subspace` command and the
`app` resource, and `packslip verify --artifact` passed. `actionlint` passes
on the workflow. The workflow then signed 0.8.0 by hand
([run 43992562746](https://github.com/aiconsultancy/subspace/actions/runs/43992562746)),
and `packslip verify` passed against the published zip: signed by
`.github/workflows/packslip.yml@refs/heads/main`, logged in Rekor, signer
fingerprint `ps1_n7ptlnytm5dm6y5s2safxx3lbu`.

## Tasks

- [x] Workflow, pinned action, two-job split.
- [x] Docs.
- [x] First signed release and the signer fingerprint (`packslip pin`)
      published in the installation guide.
