# Spec: board-to-board contracts

Back to the [change records](../README.md). The developer reference is
[Board-to-board contracts](../../board-integration.md#board-to-board-contracts);
this record holds the decisions and their reasons.

## 1. Two kinds of traffic

- **Board and Framework**: tools write data in; `DashboardEvent`,
  notifications, the full-text index, help and chat requests send data out.
  Unchanged.
- **Board and board**: only through contracts and the broker. A board never
  names another board, depends on its crate, holds its entities or reads its
  globals or stored choices.

## 2. Contracts

`framework/services/contracts` (`subspace-contracts`), a services crate with
no GPUI and one dependency (`url`, for issue-link recognition), so boards,
clients and hubs can build messages and the
architecture rules hold (services depend only on services; boards and
board-kit may depend on services).

```rust
pub trait Contract: 'static { const NAME: &'static str; const VERSION: u32; }
pub trait Event: 'static { type Contract: Contract; const NAME: &'static str; }
pub trait Request: 'static {
    type Contract: Contract;
    const NAME: &'static str;
    type Response: 'static;
}
```

- A `MessageKey` (`contract`, `version`, `message`, `kind`) identifies a
  message, shown as `issues@1/resolve`. Names are lower-case ASCII letters,
  digits and hyphens, starting with a letter; versions start at 1.
- A contract belongs to no board, so its name is the interface's name
  (`issues`), not a publisher prefix. This replaces the proposal's
  `irc.reference-mentioned` naming: several boards may publish one event.
- Payloads are plain Rust records, `#[non_exhaustive]` with constructors or
  builders. There is no serde: messages never leave the process. Records that
  carry content (titles, names) do not derive `Debug`.
- Events and requests. Requests are justified by the Issues case: IRC needs
  the current scope and summaries now, from whichever board holds the
  session, and must learn at once when there is none.

## 3. Declaration at registration

`Dashboard::contracts() -> BoardContracts`, a static function like
`connection_settings`, with a `'static` slice of `Declaration`s built by
`const fn`s: `publishes::<E>()`, `subscribes::<E>()`, `requests::<R>()`,
`responds::<R>()`. `BoardRegistration::new` captures it without creating the
view; `BoardRegistration::contracts` lists it. Configured boards declare
none.

`subspace_dashboard::contracts::validate`, run by `registration_sets_with`
for the included boards (through `registry/validation.rs`, which now also
holds the settings and feature checks moved out of `registry.rs`), rejects malformed keys, a role declared twice by one
board, and a request with more than one responder.

## 4. The broker

`subspace_dashboard::contracts`, in board-kit because it needs `App` and
`WeakEntity`. One private global, `Broker`.

- **Install.** `install(boards, cx)` sets the running boards' declarations in
  product order. The host calls it from `sync_backgrounds`, after stopping
  removed boards and before starting new ones, so stop hooks can still
  publish and start hooks already can. Receivers stay attached across
  installs; routing filters by the running set, so a retained-when-disabled
  view resumes when its board runs again.
- **Attach.** `subscribe::<E, T>(board, &Entity<T>, handler, cx)` and
  `respond::<R, T>(…)` store `AnyWeakEntity` plus a closure over
  `WeakEntity<T>`. Nothing holds a strong handle. Dead endpoints are pruned
  on attach and on delivery. The same entity subscribing again replaces its
  handler; a board's later responder replaces its earlier one.
- **Publish.** `publish::<E>(board, event, cx)` checks the declaration and
  depth, queues the event and, if no flush is pending, `cx.defer(flush)`.
  `flush` pops events in FIFO order (so each publisher's order holds) and
  calls the live receivers of running boards that declare `subscribes`, in
  board order then attach order, skipping the publisher's own board.
  Handlers run inside `WeakEntity::update`, never inside the publisher.
- **Depth.** An event carries the depth of the delivery it was published
  from; deeper than `MAX_DEPTH` (8) is refused as `TooDeep`, so a cycle ends.
  Work a handler defers or spawns starts a new chain.
- **Request.** `request::<R>(board, &request, cx)` checks the declaration,
  finds the first running board that declares `responds` and has a live
  responder, marks it answering, calls it synchronously and downcasts the
  answer. `Unavailable` when there is none, when the responder is gone, or
  when that board is already answering higher up (a request cycle).
  Responders answer from memory and must not block.
- **Refusals.** `ContractError::{Undeclared, Unavailable, TooDeep}`.
  `Undeclared` and `TooDeep` are logged at warn under `contracts` with the
  board ID and key, never payloads. `Unavailable` is normal and not logged.
- **Headless.** `subspace mcp` and the CLI install no broker; a publish there
  is refused as undeclared. Nothing headless publishes.

## 5. The `issues` contract (v1)

`subspace_contracts::issues`, defined in this change and adopted in phases 3
and 4 of the [plan](tasks.md).

| Message | Kind | Payload | Answer |
| --- | --- | --- | --- |
| `current-scope` | Request | none | `Option<IssueScope>`: profile ID and name, app origin, organisation ID, team keys (loaded, or fallback). `None` while signed out or without an organisation |
| `resolve` | Request | `Resolve`: profile ID and at most 50 distinct identifiers (`MAX_RESOLVE`, the batch resolver's limit) | `Vec<Resolution>`, one per identifier in order: `Available(IssueSummary)`, `Unavailable`, `Pending`, `SignedOut`, `Failed` |
| `changed` | Event | none | — |

- `IssueSummary`: identifier, title, URL, `IssueState` (name and `StateType`),
  tracker, archived, assignee display name. These are the tooltip's fields
  (spec section 4 of [NETT issue workspace](../nett-issue-workspace/spec.md))
  plus assignee and URL; no credentials and no description.
- The responder answers from its hub's memory. An identifier it has not
  resolved starts a read on the hub's reference priority and answers
  `Pending`; the hub publishes `changed` when the read lands, when the board
  starts or stops, and when sign-in, organisation or keys change. A profile
  other than the current one answers `Unavailable`, never another profile's
  issue.
- Subscribers keep nothing across `changed`: they ask for the scope and for
  the references they show again. Both requests are in-memory and cheap.
- With the Issues board disabled or absent, both requests are `Unavailable`
  and IRC shows references as plain text, with links as ordinary links and no
  tooltips. IRC never depends on `clients/nett` or the Issues board.
- This replaces `Hub::publish`, `hub::irc_snapshot()`, `hub::irc_version()`
  and `hub::irc_resolve_reference()` as IRC's way in: `changed` replaces the
  50 ms version poll, `current-scope` the snapshot's scope fields, and
  `resolve` both the snapshot's cards and the scalar resolve call.
- Opening an issue is navigation, a Framework path (`BoardNavigation` and
  `Dashboard::open_link`, slice 4). To keep IRC from naming the `issues`
  board, the navigation request names the contract and the shell sends it to
  that contract's responder. That is designed in phase 4.
- Avatars (the avatar support change) are a separate `people` contract, designed in phase 6:
  who an IRC nick is in NETT is not settled, so no types are defined yet.

## 6. Placement of `framework/services/issues`

| Piece | Goes to | Why |
| --- | --- | --- |
| `reference.rs`: `ReferenceRecognizer`, `IssueReference` | `subspace-contracts`, `issues::recognition` | Pure text matching that both sides of the contract need with the same grammar. IRC builds a recogniser from `IssueScope.team_keys` without depending on NETT |
| `issue_link.rs`: `IssueLinkContext`, `issue_link_identifier`, `relative_issue_addresses` | `subspace-contracts`, `issues::recognition` | IRC recognises issue links too, against `IssueScope.app_origin` and `organisation_id`. Brings the `url` dependency to the contracts crate |
| `reference.rs`: `IssueResolver` | `clients/nett` | The scalar read seam over the NETT client; only the hub uses it |
| `resolution_cache.rs` | `clients/nett` (the hub) | Only the responder caches; its scope generations are the hub's |
| `profile.rs`, `credential.rs` | `clients/nett` | The NETT session's profiles and credential store; only the client uses them |
| `nett_markdown` | `spaces/issues` (a module, or `spaces/issues/crates/` if separate compilation helps) | NETT's Markdown dialect, rendered only by the Issues board. It uses the recogniser from the contracts crate. Board-kit's combined tests (`rich/prepared_tests.rs`) move with it, and board-kit drops its `subspace-issues` dev-dependency, keeping `rich_text_prepared` tests on a literal prepared string |

After the moves `framework/services/issues` is removed. The move is phase 2's
follow-up and does not wait for phases 3 and 4. `nett_markdown` moves when
`spaces/issues` exists (the issue workspace implementation slice 2).

## 7. Tests

- Contracts crate: key format and validity, `Resolve`'s distinct cap,
  record builders.
- Board-kit (`contracts/tests.rs`, `tests/events.rs`, `tests/requests.rs`): validation; deferred, ordered delivery in
  board order; no delivery to the publisher's own board; a dropped receiver
  is skipped, forgotten and not kept alive; a board that is not running
  receives nothing and resumes when it runs; undeclared publish and
  subscribe are refused; an event cycle stops at the depth limit and the
  depth resets; requests are unavailable without a running responder, with a
  gone responder or after the board stops, and answered when it runs; a
  request cycle finds the first responder unavailable; the test-support
  recorder captures events.
- Host (`registry/background/contracts_tests.rs`): declarations listed without creating
  the board; configured boards declare none; two responders rejected at
  registration; background work receives and answers with no view, the
  responder's start and stop announcements reach the subscriber, and after
  it stops the request is unavailable.
- Product: `the_documented_contract_inventory_matches_the_registrations`
  keeps [Declared contracts](../../board-integration.md#declared-contracts)
  equal to the registrations.
