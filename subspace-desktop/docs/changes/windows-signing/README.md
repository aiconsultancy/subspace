# Sign Windows releases with Azure Artifact Signing

Back to the [change records](../README.md).

## Intent

Releases shipped only the signed macOS app. The owner asked for the Windows
build to be signed with Azure Artifact Signing, with a make target and a
workflow, keeping every Azure resource name out of this public repository.
They asked whether signing needs a Windows VM, and wanted the builds to run on
the platform's self-hosted runners rather than GitHub-hosted ones.

## Spec

- No Windows machine is needed to sign. Artifact Signing keeps the key in
  Microsoft's HSMs and signs a digest the client sends over HTTPS. The client,
  jsign, runs on any OS. There is no certificate to store. Key Vault holds
  only the endpoint, account and certificate profile names
  (`Subspace--WindowsSigning*`).
- The build does need Windows. A GPUI release build for Windows cannot be
  cross-compiled from Linux, because the renderer compiles its shaders with the
  Windows SDK's `fxc.exe` in a build script. The jobs run on two self-hosted
  ARC scale sets in runner group `subspace-release`:
  - `subspace-windows`, a Windows Server 2022 runner on a node pool that scales
    from zero to one, builds and verifies;
  - `subspace-signing`, a Linux runner that reaches the private Key Vault, signs.
- `make sign-windows FILES=...` (`scripts/sign-windows.sh`) signs `.exe`,
  `.dll`, `.msi` and `.msix` files in place. Given a release `.zip`, it signs
  every `.exe` and `.dll` inside, keeps the entries and their order, and
  rewrites the `.sha256`.
  - Configuration comes from the environment, then from the Key Vault named by
    `SUBSPACE_SIGNING_KEY_VAULT`, which has no default.
  - It refuses an endpoint that is not Artifact Signing's.
  - It passes the access token to jsign through the environment only.
- `.github/workflows/windows-release.yml` runs when `release.yml` creates an
  app release, or by hand on `main` with a tag and a mode: `publish`,
  `sign-only` or `build-only`.
  - A first job on the signing runner refuses anything but a stable release
    tag on a commit on `main` (the release's own commit, when `release.yml`
    starts the run), and fixes that commit and the archive's name for the
    run. It also refuses a release from before this workflow, whose
    packaging would run the app and fail. `scripts/windows-release-resolve.sh`
    makes these checks. The repository's two writers are trusted, so these
    checks catch mistakes; a tag ruleset stops release tags being moved or
    deleted.
  - It builds that commit on `subspace-windows` with no secrets.
  - `package-release.sh` normally runs the built app to read its version
    back. The app cannot start in a Windows Server container, so the job sets
    `SUBSPACE_PACKAGE_SKIP_RUN_CHECK=1` to skip that.
  - It signs on `subspace-signing` in the `windows-signing` environment, using
    GitHub OIDC. The environment's deployment rule must allow only `main`,
    never a tag pattern: a workflow file committed on an admitted tag could
    request the signing identity without this workflow's checks. The
    build's scripts could rewrite the build job's outputs, so the sign job
    uses nothing from it but the archive. The script refuses an archive with
    links, unsafe paths or duplicate entries, so a tampered archive can't
    copy the signer's credentials into the release. The sign job refuses an
    archive with any Windows binary but `subspace.exe`, and records the
    signed archive's SHA-256 as a job output.
  - It verifies that hash and the signature with `Get-AuthenticodeSignature`
    on Windows, with read access only.
  - Back on the signing runner, it checks the hash again and that the tag
    still points at the resolved commit, then attaches the archive and its
    `.sha256` to the draft release. It never replaces a published release's
    files: it adds ones it lacks and keeps ones that match. The owner waits for it before
    `make release-macos` publishes the release.
  - A compromised build dependency in a reviewed commit could still swap
    `subspace.exe` before it is signed. Only commits on `main` and the
    reviewed `Cargo.lock` limit that.
- The Azure identity, the vault name and the runner configuration live outside
  this repository: the identity's IDs and the vault name are in the
  environment's secrets.

## Delivery

- `bash scripts/test-sign-windows.sh` runs the script against a fake `az` and
  jsign. It is also part of `scripts/test-scripts.sh`. It covers:
  - configuration from Key Vault and from the environment;
  - the exact jsign arguments, with the token never on the command line;
  - a wrong endpoint and a missing secret;
  - archive rewriting with a valid checksum;
  - a jsign failure leaving the archive untouched;
  - an archive with nothing to sign;
  - an archive with a symbolic link, an unsafe path or a duplicate entry,
    including names that differ only in case, end in a dot or space, use a
    character Windows forbids, or name a Windows device;
  - signing an executable whatever the case of its extension;
  - masking the Key Vault values in GitHub Actions.

  It passes under macOS's bash 3.2 and under bash 5.
- `bash scripts/test-windows-release-resolve.sh`, part of the root
  `make release-check`, runs the resolve job's checks against a local Git
  history. It covers:
  - a release through an annotated tag, with and without the release's
    commit given, and a wrong commit given;
  - malformed, pre-release, padded and missing tags;
  - a tag on a commit off `main`;
  - a branch named like a tag, pointing elsewhere, either way round;
  - a release from before Windows builds.

  It passes under macOS's bash 3.2 and under bash 5.
- `actionlint` passes on the workflows.
- Nothing in `windows-release.yml` has run yet: not the resolve job, not the
  build on the Windows runner, not the signing. The resolve checks are
  tested only by the fixtures above.
- A trial build of the runner image compiled the release. The built app then
  failed to start in Windows Server 2022 containers: a missing DLL in Server
  Core, a missing entry point in the full Server image. Containers lack parts
  of the desktop, so this doesn't settle which Windows versions the app runs
  on. That needs a test on a real Windows 10 and Windows 11 desktop.
- Not yet verified end to end. The certificate profile cannot exist until the
  organisation's identity validation completes in the Azure portal. The first
  `build-only` run on the Windows runner will show whether the build compiles
  there.

## Tasks

- [x] `scripts/sign-windows.sh`, its fixture test and `make sign-windows`.
- [x] `windows-release.yml` and its start from `release.yml`.
- [x] Installation guide, ways of working and agent instructions.
- [x] Resolve checks in `scripts/windows-release-resolve.sh`, with fixtures.
- [ ] Owner: identity validation in the portal, then the Public Trust
      certificate profile.
- [ ] Owner, in the repository settings, which no change to this repository
      can set: the `windows-signing` environment's **Deployment branches and
      tags** must be **Selected branches and tags** with the single branch
      rule `main` and no tag rule. A read-only check on 2026-11-21 found
      exactly that; confirm it before the first `sign-only` run with
      `gh api repos/aiconsultancy/subspace/environments/windows-signing/deployment-branch-policies`:
      one policy, `"name": "main"`, `"type": "branch"`.
- [ ] Owner, optional second layer: a tag ruleset, enforcement **Active**,
      targeting `refs/tags/subspace-desktop-v*` and excluding
      `refs/tags/subspace-desktop-v*-*` (pre-release tags, which pre-release
      retention deletes), with **Restrict updates** and **Restrict
      deletions**. Add **Restrict creations** only with a bypass that lets
      `release.yml`'s `GITHUB_TOKEN` still create the release tag, and check
      the next release is tagged. The resolve job does not depend on it.
- [ ] A `build-only` run, then a `sign-only` run, of `windows-release`.
