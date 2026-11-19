# Change: explicit state machines

## Intent

Several components kept lifecycle state in independent booleans and options,
so impossible combinations were representable and transitions were copied
across files. A survey confirmed twelve such state machines; this change makes
the most harmful ones explicit enums and fixes the bugs they caused.

## Spec

| Area | Before | After | Bugs fixed |
| --- | --- | --- | --- |
| App shell (`framework/host/app/src/app/surface.rs`) | `search_view`, `log_view`, their subscriptions, `jig_open`, `board_help_open` | `MainSurface { Board, Search, Logs }` with one `set_main`; the theme designer flag is settable only through `set_jig` | Search could open over Logs and closing it woke the board underneath; three paths closed the designer without restoring the previewed theme; opening a palette item from Search or Logs left the board inactive |
| Chat tab (`framework/host/chat/src/agent_chat/tab_run.rs`) | `ready`, `busy`, `notify_completion`, `turn_started`, `phrase`, `failure_details` | `TabRun { Starting, Idle, Running(RunningTurn), Disconnected }`, `ShownFailure` | A non-fatal error mid-turn showed the tab idle while the agent streamed (frozen timer, lost alert, next message not steered); some turn ends left stale approval cards |
| Runtimes (`framework/runtimes/agent/src/runtime/refusal.rs`, `pi/chat_state.rs`) | Error used both as notice and as turn end; Pi `busy`, `steering`, `cancelled`, `has_models`, `models_known`, `startup_responses`, `ready_published` | Error is always a notice; a refused send while idle also sends `TurnDone`; Pi `Turn`, `Models`, `Startup` enums | Pi ended the turn on a rejected steer, reset cancellation after Stop then steer, and never cleared `steering` |
| Actions board (`spaces/actions/src/fetch.rs`) | `loading`, `task` | `Fetch { Idle, Discovering, Loading { follow_up }, Refreshing }`; dispatch dialog `Definition` | A write during a full load cancelled the load; a missing login left loading stuck |
| IRC (`spaces/irc`, `clients/irc`) | `History { loading, complete }` | `HistoryState { Idle, Loading, Complete }`; dropped connections and `FAIL CHATHISTORY` answer pending requests | History stayed loading forever after a disconnect or a refused request |
| Web board manager, Teams | Ordering bugs | Checks before setting busy; no forced loading | A missing settings global left the manager busy; Teams forced its spinner on when nothing was fetching |

## Delivery

Regression tests cover each bug. `make check` and `mise run quality` pass.
These changes alter no visual design, so there are no screenshots.

## Tasks

- [x] App shell main surface and guarded theme designer close.
- [x] Chat tab run state, notice-only errors, refusal `TurnDone`.
- [x] Pi turn, models and startup enums.
- [x] Actions fetch and dispatch definition enums.
- [x] IRC history state and abandoned-request answers.
- [x] Web manager and Teams loading fixes.
- [ ] Teams entry mode (`restore_pending`, `retained_view`,
      `notification_state`) as one enum.
- [ ] Copilot runtime chat: held settings after a watchdog-ended turn,
      `read_only` implying fixed flags.
- [ ] Search view: one shared `error` for query, source and install errors.
