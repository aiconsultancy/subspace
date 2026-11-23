# Records a board hides from search

Back to the [documentation index](../../README.md).

## Intent

A board may keep records that must not be shown for a while without being
deleted. Channels keeps the lines of networks that come from a signed-in
account; another person, or nobody, signed in on the same computer must not
find them in search, and the first person must find them again on signing
back in. Deleting them would lose history; a scope per account would split
the board's other records too, as a board has one active scope.

## Change

- `Service::set_hidden(board, rule)` records a rule over a `Document` for the
  board, replacing any earlier one. `search` and `search_filtered` leave out
  every hit the rule hides, asking it at each search.
- `Service::shows(&hit)` asks the rule for one hit. The Search view drops the
  results it hides before it renders them, so results that arrived before a
  change, or were already on screen, are hidden too; the selection is
  cleared when any go.
- Nothing is deleted, and a board's own record reads (`records`, `groups`)
  are unchanged: the board applies its own rule there.

## Checks

`services/search/tests/cases/isolation.rs` hides a document from search,
shows it again when the rule allows, and asks the rule again for a result
kept from an earlier search. The Framework's `make check` and
`mise run quality` pass.
