# Workspace shell review findings: verification

Back to the [change record](intent.md).

## Findings

Every finding pull request #29 left open, and one found while fixing them.
IDs: **O** for Claude Opus 5.5, **S** for GPT-6.2 Sol in the order of #29's
list of open findings, **F** for the fleet review, **N** for the one found
here. Locations are as #29's reviewers gave them, against #29's head
`225c4b9`; paths under `spaces/`, `docs/` and `scripts/` are in
`subspace-desktop/`. Tests named without a path are in the crate that owns
the file.

| ID | Reviewer and severity | Location in #29 | Outcome | Covering test or reason |
| --- | --- | --- | --- | --- |
| S12 | GPT-6.2 Sol, high | `spaces/decisions/src/board.rs:128` | Fixed | `tests/account.rs`: `switching_account_drops_everything_read_for_the_previous_one`, `an_answer_read_for_the_previous_account_is_dropped`, `signing_out_drops_everything_read_for_the_account`, `a_hidden_space_drops_the_previous_account_and_reads_when_shown`; `page.rs`: `a_return_key_names_its_account_epoch` |
| F1 | Fleet review, P3 | `spaces/decisions/src/board.rs:80` | Fixed, with S12 | As S12 |
| O1 | Claude Opus 5.5, low | `spaces/decisions/src/board.rs:699` | Fixed, with S12 | `switching_account_drops_everything_read_for_the_previous_one`: the directory is dropped and read again |
| S16 | GPT-6.2 Sol, low | `spaces/decisions/src/board.rs:89` | Fixed | `load.rs`: `a_stopped_read_asks_for_no_further_page`; `tests/account.rs`: `a_newer_read_stops_the_one_it_supersedes` |
| S14 | GPT-6.2 Sol, medium | `spaces/decisions/src/search.rs:51` | Fixed | `tests/list.rs`: `a_failed_search_shows_no_rows_found_for_the_previous_text` |
| S15 | GPT-6.2 Sol, medium | `spaces/decisions/src/page.rs:44` | Fixed | `tests/issues.rs`: `back_from_an_issue_chosen_in_the_list_returns_to_its_decision` |
| S36 | GPT-6.2 Sol, medium | `docs/changes/decisions-space/spec.md:67` | Left | Paging past 1,000 matches (a "Load more" or a documented cap) needs the owner's design decision; the guide, the spec and the developer page now state the limit, the **+** count and that narrowing reaches the rest |
| S13 | GPT-6.2 Sol, medium | `spaces/issues/src/view/shell.rs:88` | Fixed | `view/shell.rs`: `tests::a_screen_keeps_the_organisation_menu_while_the_navigator_is_hidden`, a test of the toolbar rule; a full render needs a NETT backend |
| N1 | Found while fixing S15 | `spaces/issues/src/seam.rs` (`attach`) | Fixed | `tests/detail/back.rs`: `a_new_hub_forgets_the_origin_from_the_last_account`. The stale origin was latent: opening an issue after the page was dropped already reset it, but the Back state no longer outlives the page |
| S1 | GPT-6.2 Sol, medium | `subspace-framework/board-kit/dashboard/src/list/menu.rs:27` | Fixed | `list::menu::tests`: `a_menu_selects_its_line_and_offers_the_boards_commands_for_it`, `a_blocked_board_keeps_its_selection_and_the_menu_never_acts_on_it`, `a_blocked_header_menu_shows_fold_disabled` |
| S2 | GPT-6.2 Sol, medium | `subspace-framework/board-kit/dashboard/src/workspace.rs:101` | Fixed | `workspace::layout_tests`: `the_docks_keep_their_widths_while_there_is_room`, `the_docks_yield_so_the_pane_keeps_its_least_width`, `a_workspace_narrower_than_the_pane_gives_the_pane_all_of_it` |
| S3 | GPT-6.2 Sol, medium | `subspace-framework/board-kit/dashboard/src/workspace/navigator.rs:287` | Fixed | `workspace::interaction_tests`: `rows_are_tree_items_that_say_which_is_selected`, `the_list_keys_move_between_rows_and_enter_or_space_chooses_one`, `a_click_chooses_a_row_without_taking_the_keyboard_focus` |
| S4 | GPT-6.2 Sol, low | `subspace-framework/board-kit/dashboard/src/workspace/row_parts.rs:40` | Fixed, narrowed | Choosing an unselected row first is the documented contract; a right-click on the selected row no longer runs its click again. `workspace::interaction_tests`: `the_menu_of_the_selected_row_does_not_choose_it_again`, `the_menu_of_another_row_chooses_it_first` |
| S5 | GPT-6.2 Sol, low | `subspace-framework/board-kit/dashboard/src/list/menu.rs:62` | Not a defect | GPUI Kit 0.9.1's `PopupMenu` skips a separator when the menu is empty or already ends with one and drops a trailing one, so the drawn menu never doubles them |
| S6 | GPT-6.2 Sol, medium | `subspace-framework/host/app/src/app/add_space/menu.rs:101` | Fixed | `space_menu_tests/add_space.rs`: `each_space_in_the_plus_menu_is_named_by_its_title`, and `choosing_a_space_in_the_plus_menu_adds_it` updated |
| S7 | GPT-6.2 Sol, medium | `subspace-framework/host/app/src/app/add_space.rs:192` | Fixed | `space_menu_tests/add_space.rs`: `a_space_added_for_after_a_restart_is_current_after_the_restart` |
| S8 | GPT-6.2 Sol, medium | `subspace-framework/host/app/src/app/shell.rs:193` | Fixed | `toolbar/tests/status.rs`: `zoomed_help_hides_the_dock_switches_and_keeps_the_hidden_spaces_docks` |
| S9 | GPT-6.2 Sol, medium | `subspace-framework/host/app/themes/twilight-two.json:174` | Fixed | `theme.rs`: `twilight_two_syntax_colours_meet_normal_text_contrast` |
| S10 | GPT-6.2 Sol, low | `subspace-framework/docs/design-guide.md:360` | Not a defect | The checklist is the project's own screenshot checklist for people, linked from `AGENTS.md`; it gives no instruction to an automated reviewer |
| S11 | GPT-6.2 Sol, low | `subspace-framework/host/app/src/app/space_menu.rs:83` | Fixed | `space_menu.rs`: `the_closed_toast_says_where_to_reopen_the_space`; `space_menu_tests.rs`: `the_closed_notice_points_to_the_rail_when_navigation_is_vertical` |
| S17 | GPT-6.2 Sol, medium | `spaces/github/src/lib.rs:569` | Fixed | `navigation/tests/drafts.rs`: `an_account_switch_drops_the_open_review_draft`, `the_same_account_keeps_the_open_review_draft` |
| S18 | GPT-6.2 Sol, medium | `spaces/github/src/page.rs:112` | Fixed | `navigation/tests/drafts.rs`: `a_review_action_on_the_same_pull_request_keeps_the_draft`, `a_review_action_on_another_pull_request_keeps_the_draft` |
| S19 | GPT-6.2 Sol, medium | `spaces/irc/src/toolbar.rs:38` | Fixed | `tests/hidden_navigator.rs`: `with_the_navigator_hidden_connection_acts_on_the_named_conversation`, `with_the_navigator_shown_the_toolbar_acts_on_the_selection` |
| S20 | GPT-6.2 Sol, medium | `spaces/irc/src/view/frame.rs:25` | Fixed | `tests/hidden_navigator.rs`: `with_the_navigator_hidden_the_board_keeps_its_focus_target` |
| S21 | GPT-6.2 Sol, low | `spaces/github/src/inspector/status_rows.rs:85` | Fixed | `inspector::status_rows::tests::a_check_link_names_the_site_it_opens` |
| S22 | GPT-6.2 Sol, low | `spaces/github/src/lib.rs:1866` | Fixed | `repo_filter::tests::the_status_counts_stay_the_views_while_a_repository_is_filtered` |
| S23 | GPT-6.2 Sol, medium | `spaces/actions/src/open.rs:92` | Fixed | `panel/fetch/runtime_tests.rs`: `the_writes_act_on_the_run_the_opened_tab_shows` |
| S24 | GPT-6.2 Sol, medium | `spaces/actions/src/panel/fetch.rs:23` | Fixed | `panel/fetch/runtime_tests.rs`: `the_failed_job_log_picker_offers_and_reads_only_the_selected_runs_jobs` |
| S25 | GPT-6.2 Sol, medium | `spaces/actions/src/panel/fetch.rs:87` | Fixed | `panel/fetch/runtime_tests.rs`: `a_new_attempt_starts_the_opened_tab_afresh_and_drops_the_old_attempts_answer`, `a_rerun_seen_while_another_run_is_selected_reads_the_opened_run_again`; `open.rs`: `a_run_page_shows_jobs_only_from_the_attempt_it_names` |
| S26 | GPT-6.2 Sol, medium | `spaces/actions/src/panel/fetch.rs:50` | Fixed | `panel/fetch/runtime_tests.rs`: `only_the_newest_log_read_lands_and_a_failed_log_keeps_the_jobs` |
| S27 | GPT-6.2 Sol, medium | `spaces/actions/src/lanes.rs:67` | Fixed | `view.rs`: `a_narrow_list_keeps_room_for_the_run_title` |
| S28 | GPT-6.2 Sol, medium | `spaces/actions/src/panel.rs:130` | Fixed | `panel/fetch/runtime_tests.rs`: `quick_selection_reads_one_run_at_a_time_and_skips_the_runs_passed` |
| S29 | GPT-6.2 Sol, low | `spaces/actions/src/panel/fetch.rs:55` | Fixed | `panel/fetch/runtime_tests.rs`: `only_the_newest_log_read_lands_and_a_failed_log_keeps_the_jobs` |
| O2 | Claude Opus 5.5, low | `spaces/actions/src/panel/fetch.rs` (line given as 10761, beyond the file) | Fixed, with S29 | As S29 |
| S30 | GPT-6.2 Sol, low | `spaces/github-copilot-agents/src/shell.rs:82` | Fixed | `shell/tests/opened.rs`: `an_opened_task_that_leaves_the_list_keeps_a_tab_that_closes` |
| S31 | GPT-6.2 Sol, low | `spaces/github-copilot-agents/src/shell.rs:56` | Fixed | `shell/tests/opened.rs`: `a_filter_key_on_an_opened_task_shows_the_list_and_opens_the_menu_there` |
| O3 | Claude Opus 5.5, low | `spaces/github-copilot-agents/src/load.rs` (line given as 17220, beyond the file) | Fixed | `load/detail_tests.rs`: `each_loaded_list_renames_the_account_and_a_failed_one_keeps_it` |
| S32 | GPT-6.2 Sol, low | `spaces/board-lab/help.md:523` | Left | Board Lab is being renamed to Space Lab on a parallel branch that rewrites this guide; fixing it there avoids a conflict |
| S33 | GPT-6.2 Sol, medium | `AGENTS.md:78` (repository root) | Fixed | Checked by hand with prek 0.5.10 in a scratch repository: a ranged run, and a per-commit `--from-ref c^ --to-ref c` run, both miss a key added and later removed, because prek checks the files in the checkout. The root `AGENTS.md` now checks out each commit in turn, merge commits included, and runs `prek run --last-commit` there. The first version used `git rebase --exec`, which flattens merge commits and misses a key added in a merge resolution (R14) |
| S34 | GPT-6.2 Sol, medium | `docs/changes/unreleased-spaces/spec.md:69` | Fixed | A new block in `scripts/test-package-products.sh` |
| S35 | GPT-6.2 Sol, medium | `docs/github-copilot-agents.md:159` | Fixed, in code and docs | `logs/load.rs`: `tests::a_prefetch_reads_no_log_the_search_index_would_not_keep` |
| S37 | GPT-6.2 Sol, low | `subspace-desktop/AGENTS.md:291` | Not a defect | The project's own guidance to compare screenshots with the design guide's anatomy image, which reviewers are given |
| S38 | GPT-6.2 Sol, low | `subspace-desktop/AGENTS.md:335` | Fixed | Guidance only: before and after in the same theme, with the fallback stated |
| S39 | GPT-6.2 Sol, low | `docs/subspace/boards.md:12` | Fixed | `scripts/check-board-docs.py` and `scripts/check-docs-index.py` |
| F2 | Fleet review, nit | `scripts/test-product-interfaces.sh:89` | Fixed | No automated test: nothing drives the script with a stub binary. The first version of the fix overwrote the board metadata with the `--about` output before the board list was read (R12); fixed |

Of #29's 44 findings, 39 are fixed (three of them with another finding),
3 are not a defect and 2 are left. N1, found here, is fixed too.

## Review of this pull request

The first review round covered head `7c62e5f` against `main`, merge base
`40b9ebd`. Claude Opus 5.5 at high effort cleared it with four low findings
(R8 to R11). GPT-6.2 Sol at high effort did not: it raised two high findings
(R1, R2), seven medium (R3, R5, R6, R10, R12, R13, R14) and two low (R4,
R7). Both reviewers raised R10. The R IDs are this record's; locations are
against `7c62e5f`, with paths as in the table above.

| ID | Reviewer and severity | Location in #32 | Outcome | Covering test or reason |
| --- | --- | --- | --- | --- |
| R1 | GPT-6.2 Sol, high | `spaces/decisions/src/load.rs:113` | Fixed | `load::tests::a_read_is_leased_on_the_account_of_its_own_client`, `load::tests::a_read_that_can_make_no_client_reads_nothing_and_has_no_lease`, `tests::account::a_read_that_can_make_no_client_keeps_watching_the_shown_account` |
| R2 | GPT-6.2 Sol, high | `spaces/github-copilot-agents/src/logs/load.rs:168` | Fixed, one gap left | `logs::load::tests::a_log_is_indexed_under_the_account_it_was_read_as`, `logs::load::tests::a_log_read_while_the_work_account_changed_is_not_indexed`, `logs::tests::a_cached_log_is_kept_apart_for_each_work_account`. The index scope had come from the default credential, not the work login. Left: a switch from one account to another and back again entirely within one request still passes the check before and after the read; closing it needs the Framework GitHub client to report the login it used, which is the owner's decision |
| R3 | GPT-6.2 Sol, medium | `spaces/github-copilot-agents/src/load.rs:267` | Fixed | `load::tests::an_answer_is_named_with_the_account_it_was_read_as`, `load::tests::an_answer_read_while_the_work_account_changed_is_refused`, `load::tests::nothing_is_read_without_a_work_account` |
| R4 | GPT-6.2 Sol, low | `spaces/decisions/src/render.rs:23` | Fixed | `tests::account::the_frame_that_finds_the_account_changed_draws_no_previous_search_or_menu` |
| R5 | GPT-6.2 Sol, medium | `spaces/actions/src/panel/fetch.rs:257` | Fixed, with a Framework addition | `subspace-framework/clients/github`: `an_attempts_jobs_come_from_that_attempt_and_attempt_zero_reads_the_latest`. The new default method `ActionsApi::actions_attempt_jobs` is non-breaking |
| R6 | GPT-6.2 Sol, medium | `spaces/actions/src/panel/fetch.rs:35` | Fixed | `panel/fetch/runtime_tests.rs`: `the_failed_job_log_picker_offers_no_job_of_an_earlier_attempt` |
| R7 | GPT-6.2 Sol, low | `spaces/github/src/inspector/status_rows.rs:106` | Fixed | `a_check_link_on_an_ipv6_host_names_the_whole_address` |
| R8 | Claude Opus 5.5, low | `spaces/github/src/lib.rs:983` | Fixed | `another_write_in_flight_leaves_the_draft_free_to_change_mode`, `a_draft_being_sent_keeps_its_mode_until_github_answers` |
| R9 | Claude Opus 5.5, low | `spaces/irc/src/commands.rs:60` | Fixed | `with_the_navigator_hidden_the_palette_offers_what_the_toolbar_acts_on` |
| R10 | GPT-6.2 Sol, medium; Claude Opus 5.5, low | `subspace-framework/board-kit/dashboard/src/workspace/row_parts.rs:94` | Fixed | `workspace::interaction_tests::the_list_keys_stop_at_the_first_and_the_last_row`; the design guide is updated |
| R11 | Claude Opus 5.5, low | `subspace-framework/board-kit/dashboard/src/workspace/resize.rs:103` | Fixed for the docks | `workspace::layout_tests::a_yielding_dock_clips_content_that_cannot_shrink`. Left: the detail panel is not clipped, because its resize handle sits outside its wrapper, and as the trailing panel what it cannot shrink spills off the window rather than over another zone |
| R12 | GPT-6.2 Sol, medium | `scripts/test-product-interfaces.sh:92` | Fixed | No automated test, as F2; checked by hand with a stub `cargo` and a fake binary, with the Board Lab target check on and off and an x86_64 refusal |
| R13 | GPT-6.2 Sol, medium | `scripts/product.sh:159` | Fixed | `scripts/test-package-products.sh`: new refusal cases for a carriage return, tab, vertical tab, form feed and no-break space, which fail without the fix |
| R14 | GPT-6.2 Sol, medium | `AGENTS.md:84` (repository root) | Fixed | Guidance only. Reproduced by hand in a scratch repository: `git rebase --exec` flattened a merge, missed a key added in its resolution and rewrote the hashes |

All 14 are fixed; R2 leaves the narrow gap above and R11 covers the docks
only.

### Second review round (#35)

#32 was merged at `7c62e5f` before the fixes above were pushed, so they went
to `main` through #35. Its first review covered head `e6968d3` against
`main`, merge base `7c62e5f`. Claude Opus 5.5 (high) cleared it with three
low findings; GPT-6.2 Sol (high) did not, with two high findings on the
Copilot Agents account binding. IDs Q1 to Q10.

| ID | Reviewer and severity | Location in #35 | Outcome | Covering test or reason |
| --- | --- | --- | --- | --- |
| Q1 | GPT-6.2 Sol, high | `spaces/github-copilot-agents/src/load.rs:288` | Fixed | The before-and-after login check did not bind the request to the credential that ran it. The Framework's new `Client::pin_work` resolves the work credential once, verifies it with a `viewer` read sent with that credential, and every request of the operation uses it; results, cache and search scope are keyed by the verified node ID. Framework `tests/work_pin.rs`: `a_pinned_operation_sends_every_request_with_the_credential_it_verified`, `a_rejected_pinned_credential_is_not_retried_with_another`, `a_credential_of_another_account_is_not_pinned`. The task-start write path is not pinned (an open question) |
| Q2 | GPT-6.2 Sol, high | `spaces/github-copilot-agents/src/logs/load.rs:169` | Fixed | A read takes its search activation ticket at dispatch, and an overtaken read neither indexes nor activates its scope; a log read as another account than the Space shows is refused. `logs::load::tests`: `an_overtaken_read_neither_indexes_nor_activates_its_account_scope`, `no_ticket_is_taken_while_the_index_takes_no_logs`, `a_read_as_another_account_than_the_space_shows_is_refused`; `load::detail_tests`: `a_log_read_as_another_account_than_the_space_shows_is_not_shown`, `a_list_read_as_another_account_closes_the_open_log` |
| Q3 | GPT-6.2 Sol, medium; Claude Opus 5.5, low | `spaces/github-copilot-agents/src/logs/load.rs:275` | Fixed | One pin per operation; a cache hit needs no further lookup, and a token verified once this run is pinned again without a request, offline included. `a_credential_verified_once_is_pinned_again_without_a_lookup_even_offline` (Framework), `a_cached_log_is_shown_without_a_request`, `a_log_another_account_cached_is_read_again` |
| Q4 | GPT-6.2 Sol, medium | `spaces/github-copilot-agents/src/logs.rs:96` | Fixed | Search scopes and cache keys now use the account's node ID, so entries an earlier build wrote are never activated or matched. `logs::tests::a_search_scope_is_keyed_by_node_id_apart_from_earlier_builds` |
| Q5 | GPT-6.2 Sol, medium | `subspace-framework/board-kit/dashboard/src/workspace/row_order.rs:33` | Fixed | Each navigator keeps its own row order. `workspace::interaction_tests::two_navigators_in_one_window_each_keep_their_own_row_order` |
| Q6 | GPT-6.2 Sol, low | `subspace-framework/board-kit/dashboard/src/workspace/row_order.rs:43` | Fixed | A set beside the ordered rows makes registration linear; covered by the navigator interaction tests |
| Q7 | GPT-6.2 Sol, low | `subspace-framework/clients/github/src/actions.rs:55` | Fixed | The default attempt read refuses a nonzero attempt (status 501). `actions::tests::default_attempt::the_default_attempt_read_gives_the_latest_jobs_only_for_attempt_zero` |
| Q8 | Claude Opus 5.5, low | `subspace-framework/clients/github/src/actions/request.rs:22` | Not changed | GitHub documents the endpoint only as listing the jobs of one attempt; nothing shows it leaving out jobs a partial re-run carried over. Settling it needs a live partial re-run |
| Q9 | Claude Opus 5.5, low | `AGENTS.md` (hook recipe) | Fixed | The recipe stops with a message from a detached HEAD instead of failing after the loop; checked by hand in a scratch repository |
| Q10 | GPT-6.2 Sol, low | `AGENTS.md:85` | Fixed | The recipe now says it is for the pull request's author, and that reviewers do not run it |

## Checks

All on the final working tree, one build at a time, on macOS (aarch64).

| Check | Result |
| --- | --- |
| Framework `make check` (architecture, board docs, docs index, format, clippy with `-D warnings`, tests, packaging, publish script) | Passed. Tests: 3,460 passed, 0 failed, 6 ignored |
| Framework `mise run quality` | Passed: 0 findings against the baseline |
| Desktop `make check` (architecture, board docs, docs index, format, licences, clippy with `-D warnings`, tests, script fixtures, web bridge and web Space tests) | Passed. Tests: 2,394 passed, 0 failed, 13 ignored |
| Desktop `mise run quality` | Passed: 0 findings. `main` itself had one, a stale suppression in `products/subspace/build.rs`, removed here |
| Desktop `make interfaces` | Passed |
| `scripts/test-package-products.sh`, with the new refusal block | Passed (also part of `make check`) |
| `bash -n` and `shellcheck -x` on the changed scripts | Clean |

The build needed three fixes the editing pass could not see, none of which
changes an outcome above: a type in the navigator row's handler helper, a
missing trait import in the S22 test, and a redundant closure in the S19 test.

Rerun after the review fixes: see the pull request.

## Screenshots

Captured with `make capture` in Twilight Two from fixture data: before from
a build of `main` (`40b9ebd`), after from this branch, with the same steps.
The window is 1440 x 900, except the narrow Actions pair at 960 x 900.

**S3: a navigator row reached from the keyboard.** The same eleven
Shift+Tab presses from the Space. Before, the rows are not in the tab order
and the focus ends on Report issue in the status bar; after, it lands on a
project row and shows its accent ring.

![Before: Issues after Shift+Tab, with the focus on Report issue and no navigator row focused](evidence/navigator-focus-before.png)
![After: the SUB project row in the navigator shows the keyboard focus ring](evidence/navigator-focus-after.png)

**S8: zoomed Help.** Before, the status bar still has the navigator and
inspector switches at its two ends, acting on the hidden Space; after, it
has none while Help is zoomed.

![Before: zoomed Help with the dock switches still in the status bar](evidence/help-zoomed-before.png)
![After: zoomed Help with no dock switches in the status bar](evidence/help-zoomed-after.png)

**S11: closing a Space from the navigation rail.** Before, the notice
points to a plus in the title bar, which vertical navigation does not have;
after, it points to the plus at the end of the rail.

![Before: the close notice names the title bar while navigation is vertical](evidence/close-notice-rail-before.png)
![After: the close notice names the plus at the end of the navigation rail](evidence/close-notice-rail-after.png)

**S2 and S27: Actions in a narrow window.** Before, the branch, actor and
status columns take the whole row and the run titles disappear; after, they
give way and each title keeps its place.

![Before: narrow Actions list with the run titles crowded out](evidence/actions-narrow-before.png)
![After: narrow Actions list with run titles shown beside shortened columns](evidence/actions-narrow-after.png)

**S9, Twilight Two syntax colours: no capture.** The four colours changed
(`link_uri`, `link_text`, `title` and `type`) are Markdown-source and type
highlights, and no fixture view draws them: the issue editor is plain text,
and the fictional pull requests' TypeScript diffs use none of them. The
contrast test checks each against every Twilight Two background; the worst
case, the editor's current line, rises from 2.77, 3.36 and 3.34 to 4.55,
4.53 and 4.51. The other visible changes (S13, S14, S18, S22, S29, S30) need
fixture states the capture servers do not provide, such as a failing search
or a NETT organisation choice; their headless tests cover them.
