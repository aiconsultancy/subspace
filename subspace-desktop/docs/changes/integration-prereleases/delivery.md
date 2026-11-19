# Delivery

Script fixtures use local Git repositories and fake GitHub and packaging
commands. No release, tag or upload is published by these tests. Stamp tests
cover branch slugs, reserved names and unchanged source on rejection. Update
tests cover stable, nightly, alpha and beta rejecting integration candidates,
integration builds rejecting all channel updates (including stable downgrade),
and refusing to construct an updater. Product tests verify the stable name,
bundle id and icons with no update source.

Real signing, notarization and first publication require a Mac after merge.
Integration builds use the existing no-update-source path and show no update
controls. No popover wording, GUI changes or screenshots are needed.

The follow-up merges the updated `feat/subspace-desktop` at `e2f1013` once,
including the guide from `db8831f`, because this branch was already published
before the guide's rebase rule arrived. It keeps the original published
commits and records the exception for the PR. The only merge conflict was the
quality baseline's function total; regenerating it retained zero findings.
Its integration base follows app-scoped conventional commits since the last
published stable tag; nightly, alpha and beta retain the patch-bump base.
The example integration version is now
`0.3.0-feat-subspace-desktop.202611151345.gabc1234`.

Synced with `feat/subspace-desktop` at `ca8d700` after #25; regenerated the baseline and retained both Issues registration and integration update handling.

Verification on Linux:

- `python3 scripts/test-prereleases.py`: 28 passing tests. New cases cover
  feature/minor, fix-perf/patch, both breaking markers below 1.0, breaking
  precedence from 1.0, app-path filtering, no-change refusal and missing tags.
  Nightly, alpha and beta are explicitly checked to retain the patch base.
- `bash scripts/test-release-macos.sh`, `test-release-assets.sh`,
  `test-prerelease-products.sh` and `test-install-releases.sh`: passed.
- `mise exec -- cargo test -p subspace-product -p subspace-update --locked`:
  passed, including all four channel-rejection tests, updater refusal and
  stable-downgrade prevention.
- `mise exec -- make check`: passed.
- `mise run quality`: passed; no findings in changed code. Baseline regenerated
  after fixing the new parser findings; only the function total changed.
- `mise run audit`: passed for all three lockfiles with existing advisory policy.

PowerShell installer fixtures and native Mac bundle fixtures are unavailable
on this Linux host. Mac release orchestration is covered with fake commands.

## Cross-model review fixes

1. Stable-latest coverage now puts an integration pre-release before the
   stable release in the stable-list fixture. The fake CLI honours exclusion
   flags, and the test asserts the stable download tag. It failed first with
   the old fake CLI selecting the integration archive.
2. The breaking-change test now includes `BREAKING-CHANGE:`. This coverage
   passed against the existing implementation; no behavior fix was needed.
3. Empty breaking footers cannot consume a later paragraph. The new
   `test_empty_breaking_footer_cannot_consume_the_next_paragraph` failed first
   for both footer forms; restricting whitespace to spaces/tabs fixes it.
4. The planner reports a missing published stable release. The new
   `test_integration_without_published_stable_reports_the_problem` failed
   first for empty, draft-only, pre-release-only and other-product lists.
5. The Makefile rejects empty signing identity/profile values before mise.
   `test_integration_make_target_rejects_empty_signing_arguments` failed
   first for each argument, with a fake mise that never runs a release.
6. Integration-version acceptance now uses explicit `return 0`. This is
   defensive clarity: the existing explicit-install fixture already passed;
   the preceding successful negated grep gave the bare return status zero.
7. No ancestry guard was added. Locally available stable tags `0.1.0`,
   `0.2.0` and `0.2.1` are all ancestors of this integration checkout;
   `0.2.1` points to `1127853` and entered through release PR #4. The guide's
   `stable..HEAD` range is a set difference and remains correct for diverged
   histories: count app commits reachable from HEAD that are absent from
   stable. The other planner's ancestry check concerns the previous channel
   release and whether to skip an unchanged build, not the stable base.
   `test_diverged_stable_tag_counts_only_commits_unique_to_the_integration_branch`
   already passed: a stable-only feature does not inflate an integration-only
   fix from patch to minor. Rejecting or warning would add a restriction to
   the documented range without correcting its calculation.
8. Both guides now say PowerShell rejects integration versions, and explicit
   macOS integration installs replace the stable app and its CLI symlink.
   This documents the existing installer behavior; no new install mode was added.

## Final review fixes

1. Stable latest now filters tags to strict `X.Y.Z`, independently of release
   flags. The installer fixture failed first by downloading a `nightly-fix`
   integration release marked `prerelease: false` ahead of the stable release;
   explicit integration installation remains supported.
2. Both parsed Git history commands disable `log.showSignature` locally.
   `test_signature_display_does_not_change_the_integration_plan` uses a real
   SSH-signed fixture commit and enables signature display. Its two checks
   failed first: signature text hid the feature subject and corrupted the
   committer timestamp. No user Git configuration is changed by the planner.
3. Breaking footer detection is limited to the final message paragraph.
   `test_breaking_change_examples_in_the_body_do_not_bump_minor` failed first
   for both footer spellings quoted mid-body; each now keeps the patch bump.
   Existing real-footer, empty-footer and subject-`!` cases remain covered.
4. Integration mode rejects metadata whose version differs from the plan
   before packaging, and directly requests `--prerelease --latest=false`.
   `test_integration_release_rejects_metadata_that_differs_from_the_plan`
   failed first for stable and different-branch metadata. Both now fail with
   a clear diagnostic and restore the stamps. Existing stable, channel and
   already-stamped integration flag behavior is unchanged.
5. Product positive cases, all four channel-rejection tests, update refusal
   and explicit shell installation now cover `nightly-fix` and `beta-2`.
   These are coverage additions for existing behavior, with no expected red.
6. The installation guide now cautions that returning to stable is a downgrade
   over the same data locations. This is documentation only.

The full workspace gate passed with `RUST_TEST_THREADS=2`. Two default-thread
attempts hit existing timing assertions: IRC's pathological-link test took
1.061 seconds against its one-second limit (its isolated rerun passed in
0.18 seconds), then Board Lab's source-edit launch-race check exceeded its
two-second wait. Both pass in the reduced-concurrency full run; neither test
nor its timeout was changed.
