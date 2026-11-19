# Issue workspace design approval (Defiant's captain, 15 November 2026)

Approved under the owner's delegation of 15 November ("You have approval for designs and specs etc from your crew"). The Admirals review it afterwards and may change anything.

Approved package: `index.md` (290 lines, copied into this record as [design.md](design.md)), 32 states × 2 widths × 2 themes in `shots/`, six NETT comparisons, the mocks in `mocks/`. The full capture package stays with the design hand-off.

## Decisions on the open questions (design.md §10)

1. **Placement:** the Issues board, opened from IRC references. The IRC split is rejected: at about 580 px it drops NETT's two-column layout.
2. **TST-827's state:** Subspace shows the state NETT returns and never calculates it. The mock's Backlog stands. Whether NETT dev recalculates mirrored trackers is NETT's question, not Subspace's.
3. **Profiles:** one current profile serves both the board and IRC references. Per-network profiles wait until someone needs them.
4. **Subspace API profile:** the Spec confirms its address, contract and sign-in with Kirk. Until it's confirmed, the Build ships NETT dev only. The profile model can hold more than one profile.
5. **List scope:** Cycles, Views and the Board display stay out of the first release.
6. **Decisions:** title, status, date and current identifier only. No decision reader.
7. **Images:** no Save image. Image bytes stay in memory.
8. **Progress panel:** show NETT's current header progress until the NETT progress and nested-issues rollout's fourth PR ships the progress read on dev. The panel follows then.
9. **External links:** links to other sites open in the system browser. NETT and Linear issue links stay in the app.
10. **IRC keyboard:** the §6 focus model is accepted as a proposal. The Spec checks it against the IRC board's code.
11. **Sign-in effect:** sign-in and sign-out take effect at once, and Settings says so.

**Also:**
- Pull request rows open in the PRs board only if that board can already open a PR by reference. Otherwise they open in the system browser.
- The board heading *Issues* stays (departure 12).
- The image viewer's backdrop needs a theme token, not literal colours (departure 9).

## Where it goes next

- The specification records the design and this approval, then settles §11. Done in this record: [spec.md](spec.md).
- The Build's UI crew (Claude Opus 5.5) builds the board from these screens once Build slice 1 (the NETT client) lands.
