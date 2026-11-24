# Default initiative selection

## Intent

The initiative selection lived in memory only and started empty. A person
who never opened the picker joined no initiative channels after every
launch: connected to IRC, but seeing nobody. With an API backend, people are
set up in the database, so the maintainer wants exactly one initiative
selected at a time, the same in every Space, remembered between runs, and a
plain message when the person belongs to none. Initiatives are tied to
fleets, so there is no choice of all of them or of none. A build with no API
backend keeps its local behaviour.

## Spec

What changes with an API backend that serves initiative channels:

- **A person with no memberships has Channels off entirely**, unrelated
  servers included, and Issues shows no projects. Both say *Not a member of
  any initiative yet*, with **Reload**.
- **Configured channels on the initiative's server are no longer joined as
  well:** the initiative's list replaces them, as it always did once an
  initiative was picked, and now one always is.
- **There is no Local channels and no All initiatives.**

In detail:

- **Default selection.** Once the memberships for the current account and
  organisation are known and nothing is selected, the catalogue selects the
  initiative the person last chose there while they still belong to it,
  else their first in the API's order. A selection whose initiative goes,
  through a removed membership or an account or organisation change, falls
  back the same way. Nothing is carried across accounts or organisations:
  `fence_scope` and `reset` retire the selection and where it is
  remembered.
- **Remembered choice.** `Catalog::pick` remembers the person's choice in
  the state file through the `prefs` global, under `initiatives.selected`,
  keyed by a digest of the API origin, the verified user and the NETT
  organisation, holding only the initiative's ID. The catalogue lives on the
  UI thread and writes through the one `prefs` writer, so it needs no file
  of its own: `github-accounts.json` and `nett-organisations.json` are
  separate because their writers run outside the `prefs` global, which
  rewrites `state.json` whole. The first initiative selected for someone is
  not remembered, so a person who never chooses follows the API's order.
- **One selection.** The shared menu lists the person's initiatives only.
  Issues shows the selected initiative's projects only.
- **No memberships.** When the latest settled read for the current account
  and organisation found none, Channels closes: the runtime runs no network,
  the hub sends no network command and refuses agent tools, the Space offers
  no commands, search, subject or agent context, and the pane shows only the
  message with **Reload**. A read that then fails in transit keeps it
  closed; while a new account's or organisation's memberships are read it
  stays as it was. The read that finds a membership reopens it on that
  initiative and sends each buffer's newest line so it catches up. Issues
  shows the message in place of the list, offers no projects and reads no
  list.
- **Start-up.** Where the API may serve initiative channels (for NETT, until
  the organisation's capability list says otherwise), a running hub holds
  its networks until the first read of the memberships settles, or 45
  seconds pass, the time Issues allows an API to wake, and the pane says
  *Checking initiatives…*. A member's configured channels are then not
  joined and left, and someone in no initiative never connects. A read
  refused for want of a session settles nothing, as a saved session may
  still be restoring. The longer wait was chosen over 10 seconds with a
  documented join-then-leave on a cold API: the wait ends as soon as the
  read settles, so only a slow API waits longer, and it says why.
- **Unchanged.** A read that fails before any settled (*Initiatives
  unavailable*), a refusal, an API without the endpoint, a NETT organisation
  that does not serve initiative channels and a build with no API backend
  keep the configured networks and their own autojoin.
- **An unreachable API.** When the memberships read, or the account's check
  or login request, cannot reach the API at all, Channels says so once in
  the status bar with `unreachable_message()`, the words Issues and
  Decisions use, which a build may extend. With no network to show, the pane
  says *Offline* with that message and **Reload** in place of *No IRC
  networks*. The account side keeps retrying meanwhile; only requests it
  made count. **Reload** asks again at once: the memberships, the account
  check while it has not settled, and each server's login. Refusals and
  other failures are not named so.
- **Focus and joins.** A selection made for the person, by default or as a
  fallback, joins its channels without focusing one; only their own pick
  shows the new initiative's first channel, once a connection carries it.
  While the same person's memberships in another organisation are read, the
  previous initiative's channels stay until the new one replaces them, so
  the configured channels are not joined in between. Closing Channels also
  closes an open Join dialog. An initiative whose fleet's server is
  unknown still joins nothing until the lookup succeeds. Captain notices
  follow the selected initiative's network.
- **Layout.** The Channels centre layout is saved while an initiative is
  selected, since the selection now comes back after a restart.

## Delivery

`cargo test` passed for `subspace-api-client` (unit tests, the NETT
organisation test with remembering, and a no-backend process test),
`subspace-irc-board`, `subspace-issues-board`, `subspace-client-fleet` and the
product's `nett_build` and `no_backend_build`; `make check` and
`mise run quality` pass. The capture fixture gains `CAPTURE_NO_MEMBERSHIPS=1`,
`CAPTURE_API_UNREACHABLE=1` and `CAPTURE_INITIATIVES_DELAY=<seconds>` for
these states.

Screenshots, from `make capture` in Twilight Two with fixture data:

- `evidence/channels-picker-before.png`: before, on `origin/main`, a member
  starts on **Local channels**, offered with their initiatives.
- `evidence/channels-picker.png`: after, the same member starts on their
  first initiative, and the menu offers their initiatives only.
- `evidence/channels-not-a-member.png`: Channels for someone in no
  initiative: off, with the reason and **Reload**, and the status bar saying
  it.
- `evidence/issues-not-a-member.png`: Issues for the same person: the reason
  and **Reload** in place of the list, and no projects or initiatives.
- `evidence/channels-unreachable.png`: Channels with no network to show
  while the API cannot be reached: *Offline*, the shared message and
  **Reload**.
- `evidence/channels-checking.png`: Channels at start while the first read
  of the memberships is held by the fixture: *Checking initiatives…*, and
  no network running yet.

Each was compared with the design guide's shell anatomy image: the
navigator keeps its title, the state sits in the pane, and the status bar
carries it.

## Tasks

- [x] Select the remembered or first initiative once memberships are known.
- [x] Remember the person's choice per account and organisation.
- [x] Drop **Local channels** and **All initiatives**.
- [x] Close Channels and empty Issues for someone in no initiative.
- [x] Hold the networks at start for the first read.
- [x] Join the default selection's channels without taking focus.
- [x] Tests and docs.
- [x] Name an unreachable API as the other Spaces do.
- [x] Screenshots for the pull request.
