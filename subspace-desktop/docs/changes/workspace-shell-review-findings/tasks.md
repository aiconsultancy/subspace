# Workspace shell review findings: checklist

Back to the [change record](intent.md).

- [x] Check every finding #29 left open against the current code.
- [x] Decisions: follow the signed-in account and drop everything read for
      the previous one (S12, F1, O1), with the lease, the account watch and
      the account epoch on return keys.
- [x] Decisions: stop superseded reads (S16), clear rows on a new search
      (S14) and give a linked issue an explicit Back origin (S15).
- [x] Decisions: state the 1,000-match limit in the guide, the spec and the
      [developer page](../../decisions.md) (S36; paging left for the owner).
- [x] Issues: the organisation menu with the navigator hidden (S13), and the
      Back origin dropped when the hub attaches again (N1).
- [x] Framework shared kit (S1 to S4) and host app (S6 to S9, S11); S5 and
      S10 refuted.
- [x] Pull requests (S17, S18, S21, S22) and Channels (S19, S20).
- [x] Actions (S23 to S29, O2) and Copilot Agents (S30, S31, O3, S35).
- [x] Release packaging refuses unreleased features (S34); the interfaces
      script checks the target first (F2).
- [x] Guidance: hooks on every commit (S33), screenshot pairs in one theme
      (S38), Decisions in the Space directory (S39); S37 refuted.
- [x] A headless test for each behavioural fix.
- [x] S32: fix the Board Lab guide on the Space Lab rename branch (its
      settings path and default, in the
      [Space Lab guide](../../../spaces/space-lab/help.md#board-settings)).
- [x] Build and test both projects, `make check`, `mise run quality` and
      `make interfaces` (see [delivery](delivery.md#checks)).
- [x] Captures of the changes visible in the UI
      (see [delivery](delivery.md#screenshots)).
- [ ] Model reviews of this pull request.
