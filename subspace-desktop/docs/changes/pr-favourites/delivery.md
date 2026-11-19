# Delivery: favourite repositories and All open

## Tests

- `clients/github/src/scope.rs`: account-wide and repository queries,
  de-duplication, no search for no repositories, chunking within 256
  characters in order, refused names, merging.
- `clients/github/src/tests/repositories.rs`: All open sends exactly the
  scoped query and nothing without repositories; a personal section in
  chunks; the repository search uses `/search/repositories` or `/user/repos`.
- `framework/services/core/src/settings/repos.rs`: favourites save alone and
  keep every other value and token; owner-only files; invalid names and
  stale files write nothing.
- `framework/board-kit/dashboard/src/repositories/tests.rs`: toggling,
  option order and sections, owner-only write, `apply` notifies once, and a
  keyboard search → star → favourite flow that keeps the picker open.
- `framework/host/app/src/settings/repository_tests.rs`: Remove edits the
  draft only until Save; Save writes and updates the running config without a
  restart; Add repository… searches and stars into the draft; Escape closes
  the search.
- `framework/host/app/src/mcp/decision.rs`: a repository change swaps the
  served catalog.
- `spaces/github/src/favourites_tests.rs` (offline GitHub): search, star,
  saved file, running config, `ConfigChanged` and All open reloading with
  the new `repo:` qualifiers; Favourites only scoping and its preference; the
  empty state's Add repositories…; `f` on a repository header.
- `spaces/github/src/model.rs`, `favourites.rs`: section scopes, empty
  states, reload sets, labels.

## Evidence

Isolated debug capture builds, Cupertino Dark (default), fictional data from
the fixture GitHub server. **Before** is HEAD (release 0.1.0); **after** is
this change.

![Before: All open with the configured repositories and no Repositories control](evidence/before-all-open.png)

![After: All open with favourite repositories, starred headers and the Repositories control](evidence/after-all-open.png)

![After: the Repositories search with Favourites only, filled stars for favourites and outlined stars for your other repositories](evidence/after-repositories.png)

![After: typing searches GitHub; results show outlined stars](evidence/after-repositories-search.png)

![After: a pick stars acme/docs at once and the search stays open](evidence/after-repositories-starred.png)

![Before: Mine lists every repository the account can see](evidence/before-mine.png)

![After: Favourites only limits Mine to the favourite repositories; the control looks pressed](evidence/after-favourites-only.png)

![Before: Settings › PRs with the repository textarea that needs a restart](evidence/before-settings-prs.png)

![After: Settings › PRs lists the favourite repositories with Remove and Add repository…](evidence/after-settings-prs.png)

![After: Add repository… opens the same search in Settings](evidence/after-settings-add-repository.png)

![After: a star adds acme/mobile to the Settings draft until Save](evidence/after-settings-draft.png)
