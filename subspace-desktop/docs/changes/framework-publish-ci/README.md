# Publish the Framework from CI

Back to the [change records](../README.md). Follows
[The Framework as its own crate](../framework-crate/spec.md).

**Superseded in part** by
[Release Please](../release-please/README.md): the tag-push trigger is gone.
`release.yml` now starts `publish-framework.yml` on the tag Release Please
creates, with a `publish` input; verification moved to Linux without the
test run; and `make bump-version` was removed, since the release pull
request moves every version.

## Intent

The owner supplied a crates.io token and asked for a manual first publish,
then CI publishing from then on.

## Spec

- **First publish, by hand.** `make publish` published all 24 crates at
  0.1.0 from `main` (`bfe047c`) on 17 November 2026. The token went from a
  private prompt to the publish process's environment only.
- **Trusted Publishing.** Each of the 24 crates on crates.io names
  `publish-framework.yml` in `aiconsultancy/subspace` as a trusted
  publisher, so the workflow needs no stored token.
- **`.github/workflows/publish-framework.yml`.** On a tag
  `subspace-framework-v<version>`: a macOS job checks the tag is on `main`,
  packages and builds every crate from its packaged copy and runs the
  tests; a Linux job exchanges the workflow's identity for a 30-minute
  crates.io token and runs `scripts/publish.sh --ci`. Run by hand it
  uploads nothing (verify, token exchange, dry run).
- **`scripts/publish.sh --ci`.** The workflow's own jobs are the gate, so CI
  mode replaces the remote-branch and local-stamp conditions with: GitHub
  Actions, and a run on the tag that names the workspace version.
- Docs: the Framework's publishing guide (status, the release steps, CI,
  new crates) and both AGENTS.md files.

## Delivery

- All 24 crates are on crates.io at 0.1.0 under Apache-2.0, and each has
  the trusted publisher.
- `scripts/test-publish.py` covers CI mode: it publishes from the release
  tag without local stamps and refuses outside Actions or off the tag.
- The workflow passes `actionlint`. Its first manual run is recorded below.

## Tasks

- [x] Manual publish of 0.1.0.
- [x] Trusted publisher on each crate.
- [x] Workflow and the script's CI mode, with tests.
- [x] Docs.
- [ ] A manual run of the workflow on `main` passes.
