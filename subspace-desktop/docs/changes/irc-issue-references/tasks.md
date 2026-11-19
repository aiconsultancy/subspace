# Tasks: IRC issue references

Back to the [change records](../README.md). Phase 4 of the
[board-contracts plan](../board-contracts/tasks.md); [spec](spec.md).

- [x] Declarations and the hub's subscription and scope (spec section 1).
- [x] Recognition in plain, mIRC-formatted and Markdown lines, issue links
  with the organisation check, code plain (section 2).
- [x] Visible references resolved in batches of at most 50, deduplicated,
  coalesced per frame, asked again after `changed` (section 3, slice 12's IRC
  side). The responder's batch read (`POST org/references/resolve`) is the
  Issues board's, in phase 3.
- [x] Normal, unavailable and focused looks; tooltip on hover and on keyboard
  focus with the summary and the section 5 wording (section 4, slice 8).
- [x] Transcript keyboard focus (section 5, slice 9) for links and every
  reference kind in plain and mIRC-formatted lines.
- [x] With no answering board: no recognition, ordinary links, no tooltips,
  nothing logged.
- [x] Opening in the browser (section 6).
- [ ] Opening in the answering board through `BoardNavigation` with a return
  target: needs the Framework design (board-contracts phase 4, *Opening*).
- [ ] Board-kit's shared reference plugin for Markdown lines (card, muted
  look, keyboard focus for links in formatted lines).
- [ ] **Open issue reference…** in the palette and More menu: not built; the
  transcript's keyboard focus reaches every reference.
- [ ] Signed-out and error tooltip evidence: the stand-in answers available,
  unavailable and pending only; the wording is tested.
- [x] Docs: [IRC](../../irc.md#issue-references), the IRC guide,
  [Declared contracts](../../board-integration.md#declared-contracts),
  audit and gaps, `docs/issues.md`, `docs/ui-components.md`.

### Board checklist: irc

Identity and registration
- [x] ID, title, description and icon are final: unchanged.
- [x] Product descriptor and `BoardRegistration` added; artwork declared with `ARTWORK`: unchanged.
- [x] `RETAIN_WHEN_DISABLED`: not applicable because the runtime is process-owned and stops with the board.

Layout and UI
- [x] List board hooks: unchanged; the dock layout stays.
- [x] Toolbar, status, badge and busy: unchanged.
- [x] Palette commands for every toolbar and menu action: unchanged; no new toolbar or menu action.
- [x] Own keys: the transcript's keys are bound in `conversation::bind_keys` under `IrcConversation > Input` and `IrcTranscript`, and `keys` lists them.
- [x] Menus and dialogs reported with `modal` and closed by `dismiss`: unchanged; the tooltip card is not a menu.
- [x] Empty, loading and error states: a reference's card says *Loading…*, unavailable, signed out or couldn't reach.
- [x] `quit_warnings`: not applicable because nothing new runs.

Settings and configuration
- [x] `settings()`: unchanged.
- [x] Service connections or own config file: unchanged; the issue tracker's profile is the answering board's.
- [x] Feature flags: not applicable because references only appear when a board answers the contract.

Data caching and refresh
- [x] Stored choices: none added.
- [x] Framework store: unchanged; issue answers are deliberately kept out of it.
- [x] Loaded data shown from the store first: unchanged.
- [x] Refresh: no polling; `changed` drives re-asking; requests are in-memory and synchronous.

Search
- [x] Item search and `Opened`: unchanged.
- [x] Full-text index: unchanged; issue titles are not indexed.
- [x] Exact cross-board references: unchanged; issue references are not recorded (the contract's identity is enough).

Agent tools, MCP and CLI
- [x] No tool changes: issue summaries are the answering board's.
- [x] App-only tools: unchanged.
- [x] docs/mcp.md: unchanged.

Chat context
- [x] `agent_context`: unchanged; the tooltip offers no chat.

Notifications and attention
- [x] Unchanged.
- [x] Unchanged.

Help and docs
- [x] `help.md`: *Issue references*, *Move through messages with the keyboard*, keys and limits.
- [x] Guide and developer page already linked.

Communicating with other boards
- [x] Contracts: `subscribes` `issues@1/changed`, `requests` `issues@1/current-scope` and `issues@1/resolve`, declared in `contracts()`, the hub subscribed from `start_background`, listed in Declared contracts.
- [x] No dependency on `clients/nett`, `framework/services/issues` or the Issues board.

Links, logging and diagnostics
- [x] Issue pages and links open through `links::open`.
- [x] Nothing new logged; refused requests are the broker's (identifiers and payloads never logged).

Testing and evidence
- [x] Pure logic and the keyboard path tested headless; resolution through the real broker with a fixture responder.
- [x] Screenshots from the capture build with fixture data: [delivery](delivery.md).
