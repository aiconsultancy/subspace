# Tasks: leave Board Lab and the Component gallery out of releases

See the [spec](spec.md) and [delivery](delivery.md).

- [x] App features `board-lab`, `component-gallery` and `unreleased-spaces`,
      none in `default`; Board Lab's dependency optional, the Framework's
      `component-gallery` requested only through the app's feature.
- [x] Descriptor features `board-lab` and `component-gallery`, turned on
      only by the app's; both Spaces listed only with them and off by
      default in every build profile.
- [x] Registrations gated by the same features.
- [x] Product and descriptor tests for both configurations, including a
      test that the descriptor lists exactly the registered boards.
- [x] `mise run lint` and `mise run test` cover the workspace with both
      Spaces and the app alone in its release configuration.
- [x] `make run-unreleased`.
- [x] `scripts/test-product-interfaces.sh` expects the default board list,
      and builds with `board-lab` for its optional Board Lab checks.
- [x] Release and packaging scripts and workflows read: none passes
      `--all-features` or names these features.
- [ ] Release archives and signed apps refuse an inherited
      `SUBSPACE_BUILD_FEATURES` that names an unreleased Space, tested in
      `scripts/test-package-products.sh`. Not yet run.
- [x] Docs: the Spaces audit, the boards page, the README, `AGENTS.md`, the
      developer checklist, the installation page, Board Lab's page, guide
      and design evidence, and the gallery's guide.
- [x] Build, lint and test on the final tree (21 November 2026): `make check`
      in both projects, `mise run quality` in the desktop, `make interfaces`
      and `make capture`, as recorded in [delivery](delivery.md).
- [ ] The checks in [delivery](delivery.md) not yet run: Board Lab's optional
      interfaces run, `MISE_ENV=terminal make check-terminal` and
      `make run-unreleased` checked by hand.
- [ ] `mise run audit`. Run on 21 November 2026 and fails on nine advisories
      against `heapless` 0.9.3 (RUSTSEC-2026-0574 to RUSTSEC-2026-0582, all
      "unsound"); `main` depends on the same version and this branch adds no
      third-party crate, so `main` fails it too.
- [ ] Both model reviews clear the final head. Not run.
