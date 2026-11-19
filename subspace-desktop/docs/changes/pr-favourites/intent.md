# Intent: PRs board favourites and All open

The owner's feedback on the PRs board, verbatim:

> Actually this doesn't respect the list of repos added in the settings, which
> may be fine, but it doesn't respect it. It would actually be better to be
> able to just search and favourite, I guess. It would also be good to be able
> to see all pull requests, not just mine or via requested, and all open, I
> guess, would be a good one

Before this change the sections (**Mine**, **Review requested**,
**Involved**) were GitHub searches across everything the account can see, and
the typed `repos` list (a textarea in Settings, applied after a restart) only
scoped the agent tools, Copilot Agents and Actions.

Delivered in two steps:

1. **All open** (release 0.1.0): every open pull request in the `repos` list.
2. **Favourites** (this change): search and star repositories in the board,
   with the stars *being* `repos`, saved at once and applied without a
   restart; the Settings list replaces the textarea; a star on repository
   headers; **Favourites only** for the personal sections, which otherwise
   stay account-wide ("which may be fine").
