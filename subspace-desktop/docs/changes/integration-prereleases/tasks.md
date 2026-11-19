# Tasks

- [x] Add failing script, product and updater regression tests first.
- [x] Share version planning and support branch stamps.
- [x] Add a Mac integration pre-release command and restore temporary stamps.
- [x] Disable integration updates and retain stable identity.
- [x] Support explicit installation and document the workflow.
- [x] Complete required script, product, update, workspace, quality and audit gates.
- [x] Prepare the tested branch for captain review.
- [x] Merge the updated integration base once, preserving the published history.
- [x] Test first: features bump minor; fix/perf bump patch; breaking markers and
  footers bump minor below 1.0; no app-changing commits refuse a build.
- [x] Keep the nightly, alpha and beta patch-bump path unchanged.
- [x] Link the record from the documentation index through its README.
- [x] Retain the existing no-update-source path: integration builds show no
  update controls. No popover wording, GUI changes or screenshots are required.
- [x] Address the cross-model P2s with regression fixtures and diagnostics;
  keep the canonical commit range, with the ancestry decision in delivery.
- [x] Address final-review P2s: strict latest tags, signature-independent Git
  parsing, final-paragraph footers, planned-version checks and channel-prefix
  slug coverage; document the shared-data downgrade when returning to stable.
