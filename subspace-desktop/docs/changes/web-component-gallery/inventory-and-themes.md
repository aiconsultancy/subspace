# Component gallery inventory and theme mapping

18 November 2026. Planning appendix for phase 1 of the web component gallery.
Source baseline: `42b07e3377fd2a3ed892d7413bf15ca0199b8f24`.
Companion: [Phase 1 plan](plan.md). No implementation is included.

## Evidence and conventions

This inventory follows the 16 entries in
`subspace-framework/spaces/component-gallery/src/catalog.rs::SECTIONS`
and each entry's actual `ITEMS` array, in dispatch order.
It records all 180 distinct cards, their implementation reference,
their sample builder and the demonstrated examples or states.
It does not substitute a generic upstream component list.

All source paths are repository-relative. A suffix `:N::function` identifies
the source line and builder. Shell builders expand through `surface_samples!`;
their line points to that invocation. Dependency source references beginning
`gpui-component 0.9.1:` are not repository files. The gallery's `kit_source!`
macro in `samples/kit.rs` supplies those versioned references.
The workspace manifest and lockfile pin the component release to 0.9.1.

Kit usage means Framework production source outside the gallery, not every
desktop Space. `kit_usage.rs` excludes tests, examples and the gallery itself.
Its source-validation tests compare those paths against the declared flags.
This appendix checks the declared counts without running Rust tests.

States below are what the source demonstrates, not a promise that every
possible hover, focus, error or disabled combination has a separate example.
Some samples use fixed callbacks. Stateful entities live in `state.rs`;
controlled flags and numbers live in `samples.rs::Values`.

## Section and card counts

| Library | Section ID | Heading | Cards | Catalogue source |
| --- | --- | --- | --- | --- |
| Subspace | `buttons` | Buttons | 12 | `subspace-framework/spaces/component-gallery/src/samples/buttons.rs` |
| Subspace | `choices` | Choices | 5 | `subspace-framework/spaces/component-gallery/src/samples/choices.rs` |
| Subspace | `inputs` | Inputs | 11 | `subspace-framework/spaces/component-gallery/src/samples/inputs.rs` |
| Subspace | `menus` | Menus | 7 | `subspace-framework/spaces/component-gallery/src/samples/menus.rs` |
| Subspace | `dialogs` | Dialogs | 3 | `subspace-framework/spaces/component-gallery/src/samples/dialogs.rs` |
| Subspace | `feedback` | Feedback | 13 | `subspace-framework/spaces/component-gallery/src/samples/feedback.rs` |
| Subspace | `content` | Content | 10 | `subspace-framework/spaces/component-gallery/src/samples/content.rs` |
| Subspace | `messaging` | Messaging | 7 | `subspace-framework/spaces/component-gallery/src/samples/messaging.rs` |
| Subspace | `navigation` | Navigation and chrome | 5 | `subspace-framework/spaces/component-gallery/src/samples/navigation.rs` |
| Subspace | `lists-and-tables` | Lists and tables | 3 | `subspace-framework/spaces/component-gallery/src/samples/lists.rs` |
| Subspace | `inconsistencies` | Inconsistencies (one-offs) | 5 | `subspace-framework/spaces/component-gallery/src/samples/inconsistencies.rs` |
| Shell | `shell` | Shell surfaces | 15 | `subspace-framework/spaces/component-gallery/src/samples/shell.rs` |
| Kit | `kit-basic` | Basic components | 37 | `subspace-framework/spaces/component-gallery/src/samples/kit/basic.rs` |
| Kit | `kit-form` | Form components | 13 | `subspace-framework/spaces/component-gallery/src/samples/kit/form.rs` |
| Kit | `kit-layout` | Layout components | 17 | `subspace-framework/spaces/component-gallery/src/samples/kit/layout.rs` |
| Kit | `kit-advanced` | Advanced components | 17 | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced.rs` |

Totals: **81 Subspace cards, 15 shell cards and 84 GPUI Kit cards**.
Kit flags: **39 used, 45 unused**. Total: **180 unique cards**.

`browse.rs` caps filtered results at 40 cards but counts every match.
The filter requires every query word, without case sensitivity, in the card
name, source or registered Kit API paths. Hide-unused affects Kit cards only.
Navigation groups are Subspace components, Shell and GPUI Kit.
Section and hide-unused preferences persist through the gallery owner.

## Every catalogue card
### Buttons / buttons
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/buttons.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `icon_button::IconButton` | `subspace-framework/board-kit/dashboard/src/icon_button.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons.rs:126::shared_icon_button` | Current global size. Settings selects Small, Medium, Large or Extra large. | Not a Kit card |
| `icon_button::latched` | `subspace-framework/board-kit/dashboard/src/icon_button.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons/toolbar.rs:17::latched_icon_buttons` | Off; On, each labelled with current global icon size. | Not a Kit card |
| `action::action_button` | `subspace-framework/board-kit/dashboard/src/action.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons.rs:141::action_buttons` | Accept; Reject; Caution; Primary; Neutral; Quiet; No icon; Disabled; Selected; Selected and toggled. | Not a Kit card |
| `action::ActionState` | `subspace-framework/board-kit/dashboard/src/action.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons.rs:197::action_states` | Idle/Save; Busy/Saving…; Success/Saved; Failure/Save failed. | Not a Kit card |
| `action::menu_button` | `subspace-framework/board-kit/dashboard/src/action.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons/toolbar.rs:43::menu_buttons` | Inactive; Active (non-default); Disabled. | Not a Kit card |
| `action::header_button` | `subspace-framework/board-kit/dashboard/src/action.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons.rs:218::header_buttons` | Accept; Reject; Caution; Primary; Neutral; Quiet; Disabled. | Not a Kit card |
| `action::icon_button` | `subspace-framework/board-kit/dashboard/src/action.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons.rs:247::icon_buttons` | Accept; Reject; Caution; Primary; Neutral; Quiet; Disabled. | Not a Kit card |
| `action::toolbar_button` | `subspace-framework/board-kit/dashboard/src/action.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons/toolbar.rs:72::toolbar_buttons` | Six intents × icon only/with label; Disabled; Loading. | Not a Kit card |
| `feedback::retry_action` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons/recovery.rs:12::retry_action` | Default; Disabled | Not a Kit card |
| `feedback::recovery_action` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons/recovery.rs:31::recovery_action` | Caution; Disabled | Not a Kit card |
| `feedback::configure_action` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons/recovery.rs:55::configure_action` | Default; Disabled | Not a Kit card |
| `panel::service_link` | `subspace-framework/board-kit/dashboard/src/panel.rs` | `subspace-framework/spaces/component-gallery/src/samples/buttons/recovery.rs:74::service_link` | Default | Not a Kit card |

### Choices / choices
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/choices.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `segmented::control` | `subspace-framework/board-kit/dashboard/src/segmented.rs` | `subspace-framework/spaces/component-gallery/src/samples/choices.rs:76::segmented_controls` | No choice yet; Selected; With icons | Not a Kit card |
| `tabs::tab_bar` | `subspace-framework/board-kit/dashboard/src/tabs.rs` | `subspace-framework/spaces/component-gallery/src/samples/choices.rs:124::tab_bars` | Icon, selected; Plain; Count; Busy; Count and busy; Disabled | Not a Kit card |
| `toggle::switch` | `subspace-framework/board-kit/dashboard/src/toggle.rs` | `subspace-framework/spaces/component-gallery/src/samples/choices/switches.rs:54::switches` | Seven backgrounds × six states. Backgrounds: Window, Panel, Secondary panel, Sidebar, Popover, Selected row, Active list row. States: off, on, disabled off, disabled on, small off, small on. Fixed fixture callbacks. | Not a Kit card |
| `toggle::toggle_row` | `subspace-framework/board-kit/dashboard/src/toggle.rs` | `subspace-framework/spaces/component-gallery/src/samples/choices.rs:165::toggle_rows` | On; Off; Disabled; Long label wraps | Not a Kit card |
| `slider::control` | `subspace-framework/board-kit/dashboard/src/slider.rs` | `subspace-framework/spaces/component-gallery/src/samples/choices.rs:214::sliders` | Value; Disabled | Not a Kit card |

### Inputs / inputs
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/inputs.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `fields::input` | `subspace-framework/board-kit/dashboard/src/fields.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs.rs:100::role_inputs` | Search; Configuration; Compact; Disabled; Masked | Not a Kit card |
| `fields::labelled_input` | `subspace-framework/board-kit/dashboard/src/fields.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs.rs:156::labelled_inputs` | Inline; Stacked; Error | Not a Kit card |
| `fields::code_editor` | `subspace-framework/board-kit/dashboard/src/fields.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs.rs:201::code_editor` | Default | Not a Kit card |
| `fields::form_field` | `subspace-framework/board-kit/dashboard/src/fields.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs/forms.rs:60::form_fields` | Label above its control | Not a Kit card |
| `fields::heading` | `subspace-framework/board-kit/dashboard/src/fields.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs/forms.rs:77::headings` | Settings page group | Not a Kit card |
| `fields::note` | `subspace-framework/board-kit/dashboard/src/fields.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs/forms.rs:89::notes` | Muted help under a control | Not a Kit card |
| `fields::field_error` | `subspace-framework/board-kit/dashboard/src/fields.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs/forms.rs:104::field_errors` | Validation under its control | Not a Kit card |
| `fields::path_input` | `subspace-framework/board-kit/dashboard/src/fields.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs/forms.rs:123::path_inputs` | Choose… opens the file picker; Disabled | Not a Kit card |
| `fields::textarea` | `subspace-framework/board-kit/dashboard/src/fields.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs/forms.rs:155::textareas` | Multiline value | Not a Kit card |
| `secret::secret_field` | `subspace-framework/board-kit/dashboard/src/secret.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs/forms.rs:190::secrets` | Saved; click Clear; Not saved | Not a Kit card |
| `reveal::button` | `subspace-framework/board-kit/dashboard/src/reveal.rs` | `subspace-framework/spaces/component-gallery/src/samples/inputs/forms.rs:238::reveal_buttons` | Platform label; disabled so the gallery opens no folder | Not a Kit card |

### Menus / menus
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/menus.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `anchored::dropdown_menu` | `subspace-framework/board-kit/dashboard/src/anchored.rs` | `subspace-framework/spaces/component-gallery/src/samples/menus.rs:101::dropdown_menu` | Closed trigger; open popup with checked option, plain option, separator and disabled option. | Not a Kit card |
| `anchored::select` | `subspace-framework/board-kit/dashboard/src/anchored.rs` | `subspace-framework/spaces/component-gallery/src/samples/menus.rs:135::selects` | Value; Placeholder; Small; Disabled | Not a Kit card |
| `anchored::color_select` | `subspace-framework/board-kit/dashboard/src/anchored.rs` | `subspace-framework/spaces/component-gallery/src/samples/menus.rs:119::color_selects` | Swatch and hex value; click for the palette, HSLA and hex | Not a Kit card |
| `menu::MenuAnchors` | `subspace-framework/board-kit/dashboard/src/menu/anchors.rs` | `subspace-framework/spaces/component-gallery/src/samples/menus.rs:180::menu_anchors` | Closed trigger; filterable popup with checked option, glyph option and plain option. | Not a Kit card |
| `anchored::Anchor` | `subspace-framework/board-kit/dashboard/src/anchored.rs` | `subspace-framework/spaces/component-gallery/src/samples/menus/owned.rs:91::anchors` | Retained popover body; Escape closes it | Not a Kit card |
| `row_menu::RowMenu` | `subspace-framework/board-kit/dashboard/src/row_menu.rs` | `subspace-framework/spaces/component-gallery/src/samples/menus/owned.rs:109::row_menus` | A row's standard context menu | Not a Kit card |
| `repositories::RepositoryPicker` | `subspace-framework/board-kit/dashboard/src/repositories.rs` | `subspace-framework/spaces/component-gallery/src/samples/menus/owned.rs:133::repository_pickers` | Fixture search; favourite stars; filter toggle. Five fixed example repositories. | Not a Kit card |

### Dialogs / dialogs
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/dialogs.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `dialog::frame` | `subspace-framework/board-kit/dashboard/src/dialog.rs` | `subspace-framework/spaces/component-gallery/src/samples/dialogs.rs:83::frames` | D540 with footer | Not a Kit card |
| `dialog::footer` | `subspace-framework/board-kit/dashboard/src/dialog.rs` | `subspace-framework/spaces/component-gallery/src/samples/dialogs.rs:95::footers` | Cancel → Save; Cancel → Delete; Keep editing → Discard. | Not a Kit card |
| `dialog::backdrop` | `subspace-framework/board-kit/dashboard/src/dialog.rs` | `subspace-framework/spaces/component-gallery/src/samples/dialogs.rs:115::backdrop` | Bounded | Not a Kit card |

### Feedback / feedback
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/feedback.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `copy_details::CopyDetails` | `subspace-framework/board-kit/dashboard/src/copy_details.rs` | `subspace-framework/spaces/component-gallery/src/samples/copy_details.rs:13::samples` | Error, activate to copy and confirm; About, without an error | Not a Kit card |
| `github_sign_in::status` | `subspace-framework/board-kit/dashboard/src/github_sign_in/view.rs` | `subspace-framework/spaces/component-gallery/src/samples/github_sign_in.rs:11::samples` | Ready sign-in button; busy sign-in button; Starting; Continue in the browser; Device code with Copy code/Open GitHub/Cancel; Failed. Fixture code OCTO-1234; starts no sign-in. | Not a Kit card |
| `feedback::skeleton` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback/lines.rs:10::skeleton` | Loading | Not a Kit card |
| `feedback::loading_line` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback/lines.rs:22::loading_line` | Loading | Not a Kit card |
| `feedback::empty_line` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback/lines.rs:38::empty_line` | Empty | Not a Kit card |
| `feedback::empty` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback.rs:117::empty` | Empty | Not a Kit card |
| `feedback::configuration` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback.rs:132::configuration` | Needs configuration | Not a Kit card |
| `feedback::error_box` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback.rs:150::error_box` | Error with Retry | Not a Kit card |
| `feedback::error_box_with_settings` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback.rs:169::error_box_with_settings` | Error with Retry and Configure | Not a Kit card |
| `feedback::error_box_with_actions` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback.rs:188::error_box_with_actions` | Error with actions | Not a Kit card |
| `feedback::error_banner` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback.rs:217::error_banner` | Error with Retry | Not a Kit card |
| `feedback::error_banner_with_actions` | `subspace-framework/board-kit/dashboard/src/feedback.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback.rs:236::error_banner_with_actions` | Error with actions | Not a Kit card |
| `Toast` | `subspace-framework/board-kit/dashboard/src/lib.rs` | `subspace-framework/spaces/component-gallery/src/samples/feedback.rs:262::toasts` | Info; Error | Not a Kit card |

### Content / content
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/content.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `badge::badge` | `subspace-framework/board-kit/dashboard/src/badge.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:95::badges` | Count; Accent; Key hint | Not a Kit card |
| `links::external` | `subspace-framework/board-kit/dashboard/src/links.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:111::external` | Default | Not a Kit card |
| `links::external_marked` | `subspace-framework/board-kit/dashboard/src/links.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:123::external_marked` | Marked | Not a Kit card |
| `truncated::text` | `subspace-framework/board-kit/dashboard/src/truncated.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:140::truncated_text` | Shortened; Wrapped | Not a Kit card |
| `rich::plain_text` | `subspace-framework/board-kit/dashboard/src/rich.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:164::plain_text` | Selectable | Not a Kit card |
| `rich::rich_text` | `subspace-framework/board-kit/dashboard/src/rich.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:176::rich_text` | Markdown; HTML | Not a Kit card |
| `typography::group_heading` | `subspace-framework/board-kit/dashboard/src/typography.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:200::group_heading` | Default | Not a Kit card |
| `toolbar_label::label` | `subspace-framework/board-kit/dashboard/src/toolbar_label.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:240::toolbar_labels` | One account | Not a Kit card |
| `avatar::person` | `subspace-framework/board-kit/dashboard/src/avatar.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:212::person` | Initials | Not a Kit card |
| `avatar::group` | `subspace-framework/board-kit/dashboard/src/avatar.rs` | `subspace-framework/spaces/component-gallery/src/samples/content.rs:221::group` | One person; Two people; Three people (limit 2) | Not a Kit card |

### Messaging / messaging
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/messaging.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `chat_input::ChatInput` | `subspace-framework/board-kit/dashboard/src/chat_input.rs` | `subspace-framework/spaces/component-gallery/src/samples/messaging.rs:117::chat_inputs` | Agent: Markdown with Write and Preview; Message: plain text, Return sends | Not a Kit card |
| `chat_input::completion::CompletionList` | `subspace-framework/board-kit/dashboard/src/chat_input/completion.rs` | `subspace-framework/spaces/component-gallery/src/samples/messaging.rs:136::completion_lists` | Command, people and a channel | Not a Kit card |
| `composer::Composer` | `subspace-framework/board-kit/dashboard/src/composer.rs` | `subspace-framework/spaces/component-gallery/src/samples/messaging.rs:148::composers` | Write and Preview with a submit action | Not a Kit card |
| `chat_bubble::bubble` | `subspace-framework/board-kit/dashboard/src/chat_bubble.rs` | `subspace-framework/spaces/component-gallery/src/samples/messaging.rs:164::bubbles` | Transcript bubble | Not a Kit card |
| `chat_bubble::message` | `subspace-framework/board-kit/dashboard/src/chat_bubble.rs` | `subspace-framework/spaces/component-gallery/src/samples/messaging.rs:179::messages` | Sender, header and text | Not a Kit card |
| `avatar::sender` | `subspace-framework/board-kit/dashboard/src/avatar.rs` | `subspace-framework/spaces/component-gallery/src/samples/messaging.rs:214::senders` | Initials in a rounded square | Not a Kit card |
| `avatar::member` | `subspace-framework/board-kit/dashboard/src/avatar.rs` | `subspace-framework/spaces/component-gallery/src/samples/messaging.rs:226::members` | Extra small, control radius | Not a Kit card |

### Navigation and chrome / navigation
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/navigation.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `glass::nav_button` | `subspace-framework/board-kit/dashboard/src/glass.rs` | `subspace-framework/spaces/component-gallery/src/samples/navigation.rs:114::nav_buttons` | Current destination and two others | Not a Kit card |
| `glass::control` | `subspace-framework/board-kit/dashboard/src/glass.rs` | `subspace-framework/spaces/component-gallery/src/samples/navigation.rs:134::glass_controls` | Rest; Latched | Not a Kit card |
| `overflow_toolbar::OverflowToolbar` | `subspace-framework/board-kit/dashboard/src/overflow_toolbar.rs` | `subspace-framework/spaces/component-gallery/src/samples/navigation.rs:169::overflow_toolbars` | Too narrow for five actions; open the chevron | Not a Kit card |
| `surface_header::SurfaceHeader` | `subspace-framework/board-kit/dashboard/src/surface_header.rs` | `subspace-framework/spaces/component-gallery/src/samples/navigation.rs:181::surface_headers` | Title, description, an action and Close | Not a Kit card |
| `tokens::Tokens` | `subspace-framework/board-kit/dashboard/src/tokens.rs` | `subspace-framework/spaces/component-gallery/src/samples/navigation.rs:219::token_swatches` | Surfaces and lines; Text and tones | Not a Kit card |

### Lists and tables / lists-and-tables
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/lists.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `list::render` | `subspace-framework/board-kit/dashboard/src/list/view.rs` | `subspace-framework/spaces/component-gallery/src/samples/lists.rs:84::list_render` | Two groups of fixture rows, one muted; Group notes: loading and empty | Not a Kit card |
| `native_grid::NativeGrid` | `subspace-framework/board-kit/dashboard/src/native_grid.rs` | `subspace-framework/spaces/component-gallery/src/samples/lists.rs:48::native_grid` | 3 × 3 fixture | Not a Kit card |
| `chart::Chart` | `subspace-framework/board-kit/dashboard/src/chart.rs` | `subspace-framework/spaces/component-gallery/src/samples/lists.rs:74::chart` | Bar | Not a Kit card |

### Inconsistencies (one-offs) / inconsistencies
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/inconsistencies.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `github::mode_switch` | `subspace-desktop/spaces/github/src/detail.rs` | `subspace-framework/spaces/component-gallery/src/samples/inconsistencies/controls.rs:23::mode_switch` | Shared segmented::control: Approve selected; Request changes; Comment. Not a copy of the one-off review mode buttons. | Not a Kit card |
| `space-lab::row_actions` | `subspace-desktop/spaces/space-lab/src/project_settings/view.rs` | `subspace-framework/spaces/component-gallery/src/samples/inconsistencies/controls.rs:38::selected_button_row` | Shared segmented::control: Keep selected; Replace; Clear. Not copied selected action buttons. | Not a Kit card |
| `github::check_run_link` | `subspace-desktop/spaces/github/src/detail/panel.rs` | `subspace-framework/spaces/component-gallery/src/samples/inconsistencies/controls.rs:53::check_run_link` | Shared: links::external | Not a Kit card |
| `inline-confirmations` | `subspace-desktop/spaces/web/src/manager/view.rs` | `subspace-framework/spaces/component-gallery/src/samples/inconsistencies/surfaces.rs:16::inline_confirmation` | Shared: dialog::frame with dialog::footer | Not a Kit card |
| `inconsistencies::size_and_radius_strip` | `subspace-framework/spaces/component-gallery/src/samples/inconsistencies/strip.rs` | `subspace-framework/spaces/component-gallery/src/samples/inconsistencies/strip.rs:38::size_and_radius` | Measured replica boxes, not live one-offs: badge::badge height/radius/padding-x 16/7/5 px; github::mode_switch 20/4/8 px. | Not a Kit card |

### Shell surfaces / shell
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/shell.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `shell::window` | `subspace-framework/host/app/src/app/shell.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::window` | Built from links: gpui_kit::component::resizable::ResizablePanelGroup, gpui_kit::component::Root. No Open action. | Not a Kit card |
| `shell::toolbar` | `subspace-framework/host/app/src/app/toolbar.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::toolbar` | Built from links: glass::nav_button, overflow_toolbar::OverflowToolbar, icon_button::IconButton, badge::badge. No Open action. | Not a Kit card |
| `shell::navigation_rail` | `subspace-framework/host/app/src/app/navigation.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::navigation_rail` | Built from links: glass::nav_button, badge::badge. Open dispatches subspace_app::ToggleNavigationMode; disabled if action is unavailable. | Not a Kit card |
| `shell::status_bar` | `subspace-framework/host/app/src/status_bar.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::status_bar` | Built from links: icon_button::IconButton, gpui_kit::component::spinner::Spinner, gpui_kit::component::status_bar::StatusBar. No Open action. | Not a Kit card |
| `shell::command_palette` | `subspace-framework/host/app/src/app/palette_view.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::command_palette` | Built from links: gpui_kit::component::command::Command, badge::badge. Open dispatches subspace_app::TogglePalette; disabled if action is unavailable. | Not a Kit card |
| `shell::help` | `subspace-framework/host/app/src/app/help.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::help` | Built from links: rich::rich_text, gpui_kit::component::collapsible::Collapsible, gpui_kit::component::dock::DockArea. Open dispatches subspace_app::ToggleHelp; disabled if action is unavailable. | Not a Kit card |
| `shell::settings_dialog` | `subspace-framework/host/app/src/app/settings_dialog.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::settings_dialog` | Built from links: fields::form_field, fields::heading, fields::note, toggle::toggle_row, secret::secret_field, anchored::select, gpui_kit::component::tree::Tree, gpui_kit::component::dialog::AlertDialog. Open dispatches subspace_app::OpenSettings; disabled if action is unavailable. | Not a Kit card |
| `shell::about_dialog` | `subspace-framework/host/app/src/app/about_dialog.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::about_dialog` | Built from links: copy_details::CopyDetails, links::external, gpui_kit::component::group_box::GroupBox, gpui_kit::component::progress::Progress, gpui_kit::component::dialog::Dialog. Open dispatches subspace_app::OpenAbout; disabled if action is unavailable. | Not a Kit card |
| `shell::report_issue` | `subspace-framework/host/report-issue/src/lib.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::report_issue` | Built from links: toggle::toggle_row, gpui_kit::component::input::Textarea, action::action_button, gpui_kit::component::collapsible::Collapsible. Open dispatches subspace_app::OpenReportIssue; disabled if action is unavailable. | Not a Kit card |
| `shell::theme_designer` | `subspace-framework/host/app/src/jig/view.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::theme_designer` | Built from links: gpui_kit::component::accordion::Accordion, segmented::control, toggle::toggle_row, gpui_kit::component::slider::Slider, tokens::Tokens. Open dispatches subspace_app::ToggleJig; disabled if action is unavailable. | Not a Kit card |
| `shell::toasts` | `subspace-framework/host/app/src/app/toasts.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::toasts` | Built from links: Toast, action::icon_button, links::external. No Open action. | Not a Kit card |
| `shell::chat_panel` | `subspace-framework/host/chat/src/lib.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::chat_panel` | Built from links: chat_input::ChatInput, chat_input::completion::CompletionList, chat_bubble::bubble, tabs::tab_bar, gpui_kit::component::attachment::Attachment. Open dispatches subspace_app::ToggleChat; disabled if action is unavailable. | Not a Kit card |
| `shell::chat_canvas` | `subspace-framework/host/chat/src/agent_chat/canvas.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::chat_canvas` | Built from links: chart::Chart, action::action_button. No Open action. | Not a Kit card |
| `shell::search_view` | `subspace-framework/host/search-ui/src/search_view.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::search_view` | Built from links: surface_header::SurfaceHeader, fields::input, icon_button::latched, toggle::switch. Open dispatches subspace_app::OpenSearchView; disabled if action is unavailable. | Not a Kit card |
| `shell::log_view` | `subspace-framework/board-kit/dashboard/src/log_view.rs` | `subspace-framework/spaces/component-gallery/src/samples/shell.rs:198::log_view` | Built from links: surface_header::SurfaceHeader, tabs::tab_bar, overflow_toolbar::OverflowToolbar, icon_button::latched. Open dispatches subspace_app::OpenLogView; disabled if action is unavailable. | Not a Kit card |

### Basic components / kit-basic
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/kit/basic.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `gpui_kit::component::accordion::Accordion` | `gpui-component 0.9.1: src/accordion.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/status.rs:137::accordions` | Interactive, one open at a time | Used |
| `gpui_kit::component::alert::Alert` | `gpui-component 0.9.1: src/alert.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/display.rs:31::alerts` | Info with title; Success; Warning; Error with title; Banner. | Unused |
| `gpui_kit::component::attachment::Attachment` | `gpui-component 0.9.1: src/attachment.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/messages.rs:99::attachments` | Complete; Uploading; Failed | Used |
| `gpui_kit::component::avatar::Avatar` | `gpui-component 0.9.1: src/avatar/avatar.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/messages.rs:145::avatars` | Initials; Placeholder; Small; Group with a limit | Used; prefer `avatar::person` |
| `gpui_kit::component::badge::Badge` | `gpui-component 0.9.1: src/badge.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/display.rs:67::badges` | Count; Over the maximum; Dot; Icon | Unused; prefer `badge::badge` |
| `gpui_kit::component::breadcrumb::Breadcrumb` | `gpui-component 0.9.1: src/breadcrumb.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/display.rs:93::breadcrumbs` | Path with a disabled step | Unused |
| `gpui_kit::component::bubble::Bubble` | `gpui-component 0.9.1: src/bubble.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/messages.rs:165::bubbles` | Primary; Secondary | Used; prefer `chat_bubble::bubble` |
| `gpui_kit::component::button::Button` | `gpui-component 0.9.1: src/button/button.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/controls.rs:27::buttons` | Primary; Default; Outline; Ghost; Danger. Icon; loading; disabled; selected. Outline button group with Week selected. | Used; prefer `action::action_button` |
| `gpui_kit::component::checkbox::Checkbox` | `gpui-component 0.9.1: src/checkbox.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/controls.rs:72::checkboxes` | Interactive; Disabled | Unused |
| `gpui_kit::component::clipboard::Clipboard` | `gpui-component 0.9.1: src/clipboard.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/controls.rs:98::clipboards` | Copies a fixture value | Unused |
| `gpui_kit::component::collapsible::Collapsible` | `gpui-component 0.9.1: src/collapsible.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/status.rs:98::collapsibles` | Interactive; click Details | Used |
| `gpui_kit::component::button::DropdownButton` | `gpui-component 0.9.1: src/button/dropdown_button.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/controls.rs:114::dropdown_buttons` | Main action and menu trigger | Unused; prefer `anchored::dropdown_menu` |
| `gpui_kit::component::empty::Empty` | `gpui-component 0.9.1: src/empty.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/display.rs:108::empties` | Media, title, description and action | Unused; prefer `feedback::empty` |
| `gpui_kit::component::Icon` | `gpui-component 0.9.1: src/icon.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/display.rs:137::icons` | Extra-small; small; default; large. Semantic success/warning/error colours. | Used |
| `gpui_kit::img` | `GPUI: the img element, re-exported by gpui_kit` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/display.rs:164::images` | Embedded SVG | Used |
| `gpui_kit::component::kbd::Kbd` | `gpui-component 0.9.1: src/kbd.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/display.rs:174::keys` | Shortcuts; Outline | Unused |
| `gpui_kit::component::label::Label` | `gpui-component 0.9.1: src/label.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/text.rs:32::labels` | Plain; With secondary text; Highlighted match; Masked | Unused; prefer `fields::form_field` |
| `gpui_kit::component::link::Link` | `gpui-component 0.9.1: src/link.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/text.rs:52::links` | External; Disabled | Used; prefer `links::external` |
| `gpui_kit::component::marker::Marker` | `gpui-component 0.9.1: src/marker.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/messages.rs:182::markers` | Plain; Separator; Border; Loading | Unused |
| `gpui_kit::component::message::Message` | `gpui-component 0.9.1: src/message.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/messages.rs:230::messages` | Received and sent | Used; prefer `chat_bubble::message` |
| `gpui_kit::component::message_scroller::MessageScroller` | `gpui-component 0.9.1: src/message_scroller.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/messages.rs:267::message_scrollers` | Follows the end; scroll up for the jump button | Unused |
| `gpui_kit::component::pagination::Pagination` | `gpui-component 0.9.1: src/pagination.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/controls.rs:134::paginations` | Interactive; Compact | Unused |
| `gpui_kit::component::progress::Progress` | `gpui-component 0.9.1: src/progress/progress.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/status.rs:54::progress` | Bar, accent; Bar, red; Circle; Circle, loading | Used |
| `gpui_kit::component::questionnaire::Questionnaire` | `gpui-component 0.9.1: src/questionnaire/mod.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/messages.rs:287::questionnaires` | Two questions; choose and go next | Unused |
| `gpui_kit::component::radio::Radio` | `gpui-component 0.9.1: src/radio.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/choices.rs:50::radios` | Group; Disabled | Unused; prefer `segmented::control` |
| `gpui_kit::component::rating::Rating` | `gpui-component 0.9.1: src/rating.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/choices.rs:78::ratings` | Interactive; Disabled | Unused |
| `gpui_kit::component::separator::Separator` | `gpui-component 0.9.1: src/separator.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/text.rs:75::separators` | Horizontal; Labelled; Dashed; Vertical | Unused |
| `gpui_kit::component::shimmer::ShimmerText` | `gpui-component 0.9.1: src/shimmer.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/text.rs:100::shimmers` | Streaming text | Unused |
| `gpui_kit::component::skeleton::Skeleton` | `gpui-component 0.9.1: src/skeleton.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/text.rs:112::skeletons` | Row placeholder | Unused; prefer `feedback::skeleton` |
| `gpui_kit::component::slider::Slider` | `gpui-component 0.9.1: src/slider.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/choices.rs:104::sliders` | Horizontal; Disabled | Used; prefer `slider::control` |
| `gpui_kit::component::spinner::Spinner` | `gpui-component 0.9.1: src/spinner.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/status.rs:41::spinners` | xsmall; small; medium | Used |
| `gpui_kit::component::stepper::Stepper` | `gpui-component 0.9.1: src/stepper/stepper.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/choices.rs:133::steppers` | Interactive; click a step | Unused |
| `gpui_kit::component::switch::Switch` | `gpui-component 0.9.1: src/switch.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/choices.rs:153::switches` | Interactive, through toggle::switch; Disabled | Used; prefer `toggle::switch` |
| `gpui_kit::component::tag::Tag` | `gpui-component 0.9.1: src/tag.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/text.rs:137::tags` | Default; Primary; Secondary; Success; Warning; Danger; Info. Outline; Small; rounded-full Pill. | Unused |
| `gpui_kit::component::text::TextView` | `gpui-component 0.9.1: src/text/compat.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/text.rs:167::text_views` | Markdown; HTML | Used; prefer `rich::rich_text` |
| `gpui_kit::component::button::Toggle` | `gpui-component 0.9.1: src/button/toggle.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/choices.rs:177::toggles` | Interactive; Segmented group | Unused; prefer `icon_button::latched` |
| `gpui_kit::component::tooltip::Tooltip` | `gpui-component 0.9.1: src/tooltip.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/basic/text.rs:188::tooltips` | Text; hover to show; Custom element; hover to show | Used |

### Form components / kit-form
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/kit/form.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `gpui_kit::component::input::Input` | `gpui-component 0.9.1: src/input/input.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/fields.rs:23::inputs` | Placeholder, clean button; Prefix icon; Masked; Disabled | Used; prefer `fields::input` |
| `gpui_kit::component::input::Textarea` | `gpui-component 0.9.1: src/input/textarea.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/fields.rs:53::textareas` | Fixed rows | Used; prefer `fields::textarea` |
| `gpui_kit::component::input::Editor` | `gpui-component 0.9.1: src/input/editor.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/fields.rs:65::editors` | Rust, with line numbers | Used; prefer `fields::code_editor` |
| `gpui_kit::component::select::Select` | `gpui-component 0.9.1: src/select.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/fields.rs:81::selects` | Through anchored::select | Used; prefer `anchored::select` |
| `gpui_kit::component::combobox::Combobox` | `gpui-component 0.9.1: src/combobox.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/fields.rs:98::comboboxes` | Searchable; Multiple | Unused |
| `gpui_kit::component::input::NumberInput` | `gpui-component 0.9.1: src/input/number_input.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/fields.rs:125::number_inputs` | Step buttons | Used |
| `gpui_kit::component::date_picker::DatePicker` | `gpui-component 0.9.1: src/time/date_picker.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/pickers.rs:16::date_pickers` | Click to open the calendar | Unused |
| `gpui_kit::component::time_field::TimeField` | `gpui-component 0.9.1: src/time/time_field.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/pickers.rs:33::time_fields` | Hours and minutes; type or use arrow keys | Unused |
| `gpui_kit::component::input::OtpInput` | `gpui-component 0.9.1: src/input/otp_input.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/fields.rs:137::otp_inputs` | Six digits, masked | Unused |
| `gpui_kit::component::speech::SpeechButton` | `gpui-component 0.9.1: src/speech/button.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/pickers.rs:57::speech_buttons` | Unsupported in this build. Speech feature is not enabled; no microphone opens. | Unused |
| `gpui_kit::component::color_picker::ColorPicker` | `gpui-component 0.9.1: src/color_picker.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/pickers.rs:45::color_pickers` | Click the swatch | Used; prefer `anchored::color_select` |
| `gpui_kit::component::form::v_form` | `gpui-component 0.9.1: src/form/form.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/fields.rs:169::forms` | Vertical; Horizontal | Unused; prefer `fields::form_field` |
| `gpui_kit::component::input::InputGroup` | `gpui-component 0.9.1: src/input/group.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/form/fields.rs:149::input_groups` | Text addons | Used |

### Layout components / kit-layout
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/kit/layout.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `gpui_kit::component::description_list::DescriptionList` | `gpui-component 0.9.1: src/description_list.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/regions.rs:41::description_lists` | Bordered, two columns | Unused |
| `gpui_kit::component::group_box::GroupBox` | `gpui-component 0.9.1: src/group_box.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/regions.rs:65::group_boxes` | Titled; Outline | Used |
| `gpui_kit::component::Root` | `gpui-component 0.9.1: src/root.rs (re-exported from gpui-base)` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/overlays.rs:36::roots` | This window's Root shows the notification | Used |
| `gpui_kit::component::Theme` | `gpui-component 0.9.1: src/theme/mod.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/overlays.rs:74::themes` | Swatches: background; foreground; primary; secondary; muted; accent; border; popover; sidebar; info; success; warning; danger; ring. | Used; prefer `tokens::Tokens` |
| `gpui_kit::component::dialog::Dialog` | `gpui-component 0.9.1: src/dialog/dialog.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/overlays.rs:107::dialogs` | Opens a dialog with Cancel and OK | Used; prefer `dialog::frame` |
| `gpui_kit::component::dialog::AlertDialog` | `gpui-component 0.9.1: src/dialog/alert_dialog.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/overlays.rs:143::alert_dialogs` | Asks before a consequential action | Used |
| `gpui_kit::component::notification::Notification` | `gpui-component 0.9.1: src/notification.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/overlays.rs:167::notifications` | Info; Success; Warning; Error, opened by separate buttons in the window Root. | Used; prefer `Toast` |
| `gpui_kit::component::popover::Popover` | `gpui-component 0.9.1: src/popover.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/overlays.rs:197::popovers` | Through anchored::Anchor; click to open | Unused; prefer `anchored::Anchor` |
| `gpui_kit::component::hover_card::HoverCard` | `gpui-component 0.9.1: src/hover_card.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/overlays.rs:215::hover_cards` | Hover the name | Unused |
| `gpui_kit::component::resizable::ResizablePanelGroup` | `gpui-component 0.9.1: src/resizable.rs (gpui-base)` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/regions.rs:95::resizables` | Two panes | Used |
| `gpui_kit::component::scroll::Scrollable` | `gpui-component 0.9.1: src/scroll/scrollable.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/regions.rs:113::scrollables` | Vertical scrollbar | Used |
| `gpui_kit::component::sheet::Sheet` | `gpui-component 0.9.1: src/sheet.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/overlays.rs:240::sheets` | Slides in from the edge | Unused |
| `gpui_kit::component::sidebar::Sidebar` | `gpui-component 0.9.1: src/sidebar/mod.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/regions.rs:134::sidebars` | Click a destination | Unused |
| `gpui_kit::component::status_bar::StatusBar` | `gpui-component 0.9.1: src/status_bar.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/regions.rs:159::status_bars` | Leading and trailing regions | Used |
| `gpui_kit::component::toolbar::Toolbar` | `gpui-component 0.9.1: src/toolbar.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/regions.rs:184::toolbars` | Grouped commands | Unused; prefer `overflow_toolbar::OverflowToolbar` |
| `gpui_kit::component::TitleBar` | `gpui-component 0.9.1: src/title_bar.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/regions.rs:213::title_bars` | Shown only: dragging would move the window | Unused |
| `gpui_kit::component::FocusTrapElement` | `gpui-component 0.9.1: src/focus_trap.rs (gpui-base)` | `subspace-framework/spaces/component-gallery/src/samples/kit/layout/regions.rs:236::focus_traps` | Click a field, then Tab: focus stays inside | Unused |

### Advanced components / kit-advanced
Catalogue: `subspace-framework/spaces/component-gallery/src/samples/kit/advanced.rs`.

| Card ID | Implementation reference | Sample builder | Demonstrated examples and states | Kit usage / shared treatment |
| --- | --- | --- | --- | --- |
| `gpui_kit::component::diff::Diff` | `gpui-component 0.9.1: src/diff/mod.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/data.rs:32::diffs` | One file, one hunk | Unused |
| `gpui_kit::component::calendar::Calendar` | `gpui-component 0.9.1: src/time/calendar.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/data.rs:44::calendars` | Click a day | Unused |
| `gpui_kit::component::carousel::Carousel` | `gpui-component 0.9.1: src/carousel/carousel.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/data.rs:68::carousels` | Previous and next | Unused |
| `gpui_kit::component::command::Command` | `gpui-component 0.9.1: src/command/command.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/data.rs:95::commands` | Filter as you type | Used |
| `gpui_kit::component::chart::BarChart` | `gpui-component 0.9.1: src/chart/mod.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/charts.rs:58::charts` | BarChart; LineChart; AreaChart, two series; PieChart and donut; RadarChart; CandlestickChart; SankeyChart | Unused; prefer `chart::Chart` |
| `gpui_kit::component::plot::Plot` | `gpui-component 0.9.1: src/plot/mod.rs (gpui-base)` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/charts.rs:203::plots` | A custom Plot that paints one line | Used; prefer `chart::Chart` |
| `gpui_kit::component::list::List` | `gpui-component 0.9.1: src/list/list.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/views.rs:114::lists` | Searchable; click or use the arrow keys | Used |
| `gpui_kit::component::menu::PopupMenu` | `gpui-component 0.9.1: src/menu/popup_menu.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/menus.rs:50::menus` | Dropdown, through anchored::dropdown_menu; Context menu, through anchored | Used; prefer `anchored::dropdown_menu` |
| `gpui_kit::component::native_menu::NativeMenu` | `gpui-component 0.9.1: src/native_menu/mod.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/menus.rs:85::native_menus` | The operating system's own menu | Unused |
| `gpui_kit::component::setting::Settings` | `gpui-component 0.9.1: src/setting/mod.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/views.rs:139::settings` | One page, one group | Unused; prefer `fields::form_field` |
| `gpui_kit::component::table::DataTable` | `gpui-component 0.9.1: src/table/data_table.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/tables.rs:114::data_tables` | Click a header to sort | Unused; prefer `native_grid::NativeGrid` |
| `gpui_kit::component::table::Table` | `gpui-component 0.9.1: src/table/table.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/tables.rs:130::tables` | Header and three rows | Unused |
| `gpui_kit::component::dock::DockArea` | `gpui-component 0.9.1: src/dock/mod.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/dock.rs:114::docks` | Tabs and a split; drag a tab | Used |
| `gpui_kit::component::grid_layout::GridLayoutState` | `gpui-component 0.9.1: src/grid_layout/mod.rs (gpui-base)` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/views.rs:229::grid_layouts` | Drag or resize a tile | Unused |
| `gpui_kit::component::tab::TabBar` | `gpui-component 0.9.1: src/tab/tab_bar.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/menus.rs:105::tabs` | Default; Underline; Pill; Outline; Segmented; Disabled tab | Used; prefer `tabs::tab_bar` |
| `gpui_kit::component::tree::Tree` | `gpui-component 0.9.1: src/tree.rs` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/views.rs:170::trees` | Click a folder to expand it | Used |
| `gpui_kit::component::v_virtual_list` | `gpui-component 0.9.1: src/virtual_list.rs (gpui-base)` | `subspace-framework/spaces/component-gallery/src/samples/kit/advanced/views.rs:201::virtual_lists` | A thousand rows; only visible ones render | Unused |

## Interpretation limits

- Shell cards link to their constituent component cards. A registered shell
  action can open the real UI. They are not fifteen inline fixture screens.
  `samples/shell/surfaces.rs` owns the action and composition lists.
- Inconsistency cards show the closest shared control, not copied one-offs.
  The size/radius strip draws measured replica boxes only.
- The shared switch card shows 42 fixed instances: seven backgrounds times
  six states. The separate Kit switch sample is interactive.
- GitHub sign-in shows fixture phases and starts no sign-in.
- Speech is unsupported because its feature is excluded from the build.
- The TitleBar example is covered to prevent dragging the real window.
- The repository picker searches a fixed fixture list off the UI thread and
  keeps favourites in the gallery owner, not service configuration.
- `panel::frame` has no list-section sample because its resize handle saves
  application-wide panel width. Do not add a fictitious card for it.
- The catalogue test exempts infrastructure modules `global_state`,
  `highlighter`, `history` and `searchable_list` from independent cards.
  Highlighter is represented through Editor/Diff; searchable-list models
  underpin Select/Combobox. See `tests/catalogue.rs::KIT_INFRASTRUCTURE`.

## Canonical themes and legacy reference palette

There is no registered theme named **Subspace Dark**. These are distinct:

- **Subspace One Dark**, `host/app/themes/one-dark.json`, dark variant.
- **Subspace Channel Dark**, `host/app/themes/subspace-channel.json`,
  dark variant.
- **Cupertino Dark**, `host/app/themes/cupertino.json`, second variant.

All theme paths in this section are relative to `subspace-framework/`.
`host/app/src/theme.rs::SUBSPACE_THEMES` embeds these canonical documents.
`host/app/src/theme/loader.rs` selects the product catalogue.
Cupertino Dark is the current product default. Saved theme choices still win.
The requested Subspace preset needs an explicit name before baseline capture.

`board-kit/dashboard/src/tokens.rs::hex` and `Tokens::one_dark()` retain the
One Dark reference/fallback palette. This is the legacy/default-code palette,
not the source of truth for the current Cupertino or Channel appearance.
One Dark remains a supported selectable theme; it is not retired.
The example copies under `board-kit/dashboard/examples/themes/` and desktop
`spaces/actions/examples/themes/` are not the host's registration boundary.
Do not use an example file or old screenshot instead of the embedded theme.

Actual precedence:

1. Parse the selected GPUI theme and Subspace metadata.
2. Apply the GPUI configuration and switch-contrast correction.
3. Resolve installed heading/body/mono families.
4. Load density defaults plus selected theme metadata.
5. Map the active GPUI Theme into dashboard Tokens.
6. Overlay known `subspace.tones` values.
7. Saved appearance/density overrides can change the resolved result.

Source functions: `host/app/src/theme.rs::apply_document`, `config_fonts`,
`sync_font_sizes`; `board-kit/dashboard/src/tokens.rs::current`,
`with_theme`, `with_tones`; `host/app/src/theme/document/metadata.rs`.

The tables use OD = One Dark, CD = Cupertino Dark, SC = Channel Dark.
An omitted JSON field is recorded as absent, not guessed from dependency
defaults. Hex case has no semantic effect and is retained from source.

## Colour mapping to shadcn

Mappings below are proposals, not an existing CSS adapter.
Use the GPUI semantic fields for standard shadcn variables. Add explicitly
named extensions when the standard variables cannot preserve a distinction.

| GPUI colour key | OD | CD | SC | Proposed mapping |
| --- | --- | --- | --- | --- |
| `background` | `#101115` | `#20201f` | `#090D13` | `--background` |
| `foreground` | `#abb2bf` | `#f0efed` | `#f0efed` | `--foreground` |
| `border` | `#313640` | `#353431` | `#090D13` | `--border` |
| `window.border` | `#313640` | `#353431` | `#353431` | Extension required; no separate standard variable |
| `ring` | `#7dd3fc` | `#4c8dff` | `#4c8dff` | `--ring` |
| `caret` | `#d1d5dc` | `#f0efed` | `#f0efed` | Extension required; no separate standard variable |
| `accent.background` | `#2c313a` | `#303b4c` | `#303b4c` | `--accent` |
| `accent.foreground` | `#d1d5dc` | `#f4f3f0` | `#f4f3f0` | `--accent-foreground` |
| `muted.background` | `#15171b` | `#323230` | `#101419` | `--muted` |
| `muted.foreground` | `#8b919c` | `#a9a8a2` | `#a9a8a2` | `--muted-foreground` |
| `primary.background` | `#7dd3fc` | `#4c8dff` | `#217dFc` | `--primary` |
| `primary.hover.background` | `#a5e1fd` | `#5e99ff` | `#5e99ff` | Extension required; no separate standard variable |
| `primary.active.background` | `#5cc4f5` | `#3d7ef0` | `#3d7ef0` | Extension required; no separate standard variable |
| `primary.foreground` | `#101115` | `#ffffff` | `#ffffff` | `--primary-foreground` |
| `secondary.background` | `#15171b` | `#30302e` | `#30302e` | `--secondary` |
| `secondary.hover.background` | `#2c313a` | `#3a3a37` | `#3a3a37` | Extension required; no separate standard variable |
| `secondary.active.background` | `#313640` | `#41413d` | `#41413d` | Extension required; no separate standard variable |
| `switch.background` | `#757575` | `#7c7b77` | `#7c7b77` | Extension required; no separate standard variable |
| `switch.thumb.background` | `#101115` | `#ffffff` | `#ffffff` | Extension required; no separate standard variable |
| `secondary.foreground` | `#abb2bf` | `#ecebe8` | `#ecebe8` | `--secondary-foreground` |
| `danger.background` | `#e06c75` | `#ff6b60` | `#E65954` | `--destructive` |
| `danger.foreground` | `#101115` | `#1b1b1a` | `#1b1b1a` | `--destructive-foreground if target defines it` |
| `warning.background` | `#e5c07b` | `#d9a741` | `#d9a741` | Extension required; no separate standard variable |
| `warning.foreground` | `#101115` | `#2a2105` | `#2a2105` | Extension required; no separate standard variable |
| `success.background` | `#98c379` | `#56c789` | `#56c789` | Extension required; no separate standard variable |
| `success.foreground` | `#101115` | `#10271b` | `#10271b` | Extension required; no separate standard variable |
| `info.background` | `#56b6c2` | `#4c8dff` | `#4c8dff` | Extension required; no separate standard variable |
| `info.foreground` | `#101115` | `#ffffff` | `#ffffff` | Extension required; no separate standard variable |
| `input.border` | `#313640` | `#44423e` | `#44423e` | `--input` |
| `list.background` | `#101115` | `#20201f` | `#20201f` | Extension required; no separate standard variable |
| `list.active.background` | `#2c313a` | `#303b4c` | `#303b4c` | Extension required; no separate standard variable |
| `list.active.border` | `#7dd3fc` | `#303b4c` | `#303b4c` | Extension required; no separate standard variable |
| `list.even.background` | `#101115` | `#20201f` | `#20201f` | Extension required; no separate standard variable |
| `list.head.background` | `#15171b` | `#20201f` | `#20201f` | Extension required; no separate standard variable |
| `list.hover.background` | `#1d2026` | `#2a2a28` | `#1E2023` | Extension required; no separate standard variable |
| `popover.background` | `#15171b` | `#2c2c2a` | `#0D1117` | `--popover` |
| `popover.foreground` | `#abb2bf` | `#f0efed` | `#f0efed` | `--popover-foreground` |
| `selection.background` | `#313640` | `#4c8dff52` | `#4c8dff52` | Extension required; no separate standard variable |
| `scrollbar.background` | `#00000000` | `#00000000` | `#00000000` | Extension required; no separate standard variable |
| `scrollbar.thumb.background` | `#313640` | `#3e3e3b` | `#3e3e3b` | Extension required; no separate standard variable |
| `scrollbar.thumb.hover.background` | `#8b919c` | `#4a4a47` | `#4a4a47` | Extension required; no separate standard variable |
| `title_bar.background` | `#101115` | `#1b1b1a` | `#0D1117` | Extension required; no separate standard variable |
| `title_bar.border` | `#313640` | `#1b1b1a` | `#090D13` | Extension required; no separate standard variable |
| `status_bar.background` | `#15171b` | `#1b1b1a` | `#1b1b1a` | Extension required; no separate standard variable |
| `status_bar.border` | `#313640` | `#292927` | `#292927` | Extension required; no separate standard variable |
| `sidebar.background` | `#101115` | `#1b1b1a` | `#0D1117` | `--sidebar` |
| `sidebar.foreground` | `#abb2bf` | `#d3d2ce` | `#d3d2ce` | `--sidebar-foreground` |
| `sidebar.border` | `#313640` | `#292927` | `#090D13` | `--sidebar-border` |
| `sidebar.accent.background` | `#7dd3fc` | `#343432` | `#343432` | `--sidebar-accent` |
| `sidebar.accent.foreground` | `#101115` | `#f5f4f1` | `#f5f4f1` | `--sidebar-accent-foreground` |
| `overlay` | `#0000008c` | `#00000073` | `#00000073` | Extension required; no separate standard variable |
| `base.red` | `#e06c75` | `#ff6b60` | `#ff6b60` | Extension required; no separate standard variable |
| `base.green` | `#98c379` | `#56c789` | `#56c789` | Extension required; no separate standard variable |
| `base.yellow` | `#e5c07b` | `#e3b341` | `#e3b341` | Extension required; no separate standard variable |
| `base.blue` | `#61afef` | `#6ea3ff` | `#6ea3ff` | Extension required; no separate standard variable |
| `base.magenta` | `#c678dd` | `#c38cf0` | `#c38cf0` | Extension required; no separate standard variable |
| `base.cyan` | `#56b6c2` | `#5ac8e0` | `#5ac8e0` | Extension required; no separate standard variable |
| `danger.hover.background` | Absent | `#ff7b72` | `#ff7b72` | Extension required; no separate standard variable |
| `danger.active.background` | Absent | `#ee5d54` | `#ee5d54` | Extension required; no separate standard variable |
| `link` | Absent | `#6ea3ff` | `#6ea3ff` | Extension required; no separate standard variable |
| `link.hover` | Absent | `#8ab5ff` | `#8ab5ff` | Extension required; no separate standard variable |
| `link.active` | Absent | `#4c8dff` | `#4c8dff` | Extension required; no separate standard variable |
| `group_box.background` | Absent | `#2c2c2a` | `#2c2c2a` | Extension required; no separate standard variable |
| `group_box.foreground` | Absent | `#f0efed` | `#f0efed` | Extension required; no separate standard variable |
| `sidebar.primary.background` | Absent | `#4c8dff` | `#4c8dff` | `--sidebar-primary` |
| `sidebar.primary.foreground` | Absent | `#ffffff` | `#ffffff` | `--sidebar-primary-foreground` |
| `tab_bar.background` | Absent | `#20201f` | `#20201f` | Extension required; no separate standard variable |
| `tab_bar.segmented.background` | Absent | `#30302e` | `#30302e` | Extension required; no separate standard variable |
| `tab.background` | Absent | `#20201f` | `#20201f` | Extension required; no separate standard variable |
| `tab.foreground` | Absent | `#a9a8a2` | `#a9a8a2` | Extension required; no separate standard variable |
| `tab.active.background` | Absent | `#2c2c2a` | `#2c2c2a` | Extension required; no separate standard variable |
| `tab.active.foreground` | Absent | `#f0efed` | `#f0efed` | Extension required; no separate standard variable |
| `skeleton.background` | Absent | `#323230` | `#323230` | Extension required; no separate standard variable |
| `progress.bar.background` | Absent | `#4c8dff` | `#4c8dff` | Extension required; no separate standard variable |

Missing or non-exact standard mappings:

- `--card` and `--card-foreground`: there is no GPUI `card.*` pair.
  Cupertino/Channel define `group_box.*`; using it for every web card is a
  design choice. One Dark omits that pair.
- `--sidebar-ring`: no separate source value. Alias the normal ring explicitly.
- One Dark omits `sidebar.primary.*`; its fallback must be explicit.
- `--chart-1` through `--chart-5`: no ordered five-colour source contract.
  Named base/status tones do not define an ordered chart palette.
- Standard opacity-based button hover classes do not preserve the explicit
  hover and active colours above.
- Keep alpha in selection, scrollbar, overlay and glass values.

### Dashboard aliases and semantic tone overrides

`Tokens` maps Theme fields as follows before metadata overrides:

| Dashboard token | GPUI JSON field before overrides |
| --- | --- |
| `bg` | `background` |
| `pane` | `muted.background` |
| `line` | `border` |
| `hair` | `list.hover.background` |
| `select` | `accent.background` |
| `fg` | `foreground` |
| `fg_strong` | `caret` |
| `muted` | `muted.foreground` |
| `accent` | `primary.background` |
| `red` | `danger.background` |
| `yellow` | `warning.background` |
| `green` | `success.background` |
| `cyan` | `info.background` |
| `blue` | `base.blue` |
| `magenta` | `base.magenta` |
| `chrome` | `title_bar.background` |
| `chrome_line` | `title_bar.border` |
| `sidebar` | `sidebar.background` |
| `sidebar_line` | `sidebar.border` |
| `surface` | `popover.background` |


| Resolved dashboard tone | OD | CD | SC |
| --- | --- | --- | --- |
| `accent` | `#7dd3fc` | `#4c8dff` | `#4c8dff` |
| `red` | `#e06c75` | `#ff6b60` | `#ff6b60` |
| `orange` | `#d19a66` | `#ffa94d` | `#ffa94d` |
| `yellow` | `#e5c07b` | `#e3b341` | `#e3b341` |
| `green` | `#98c379` | `#56c789` | `#56c789` |
| `cyan` | `#56b6c2` | `#5ac8e0` | `#5ac8e0` |
| `blue` | `#61afef` | `#6ea3ff` | `#6ea3ff` |
| `magenta` | `#c678dd` | `#c38cf0` | `#c38cf0` |
| `glass` | `#ffffff1a` | `#ffffff1f` | `#ffffff1f` |
| `glass-rim` | `#ffffff40` | `#ffffff4d` | `#ffffff4d` |
| `glass-shadow` | `#00000080` | `#00000099` | `#00000099` |
| `media-stage` | `#101115` | `#111110` | `#111110` |

Dark-mode derived values from `tokens/tones.rs`:

- `button_hover`: `#ffffff17` unless metadata overrides `button-hover`.
- `button_on`: active GPUI primary colour at alpha `0.24`, unless metadata
  overrides `button-on`.
- `media_stage` initially uses an opaque background, then metadata replaces it.

Dashboard `accent` means primary action tone, not shadcn `--accent`.
Channel primary is `#217dFc` but dashboard accent is `#4c8dff`.
Channel danger is `#E65954` but dashboard red is `#ff6b60`.
Cupertino/Channel warning is `#d9a741` but dashboard yellow is `#e3b341`.
Their info is blue but dashboard cyan is `#5ac8e0`.
Do not collapse these pairs. Preserve `--tone-*`, `--fg-strong`, `--hair`,
`--glass-*`, `--button-hover`, `--button-on` and `--media-stage` extensions.

`board-kit/dashboard/src/toggle/track.rs` corrects switch off/on/thumb colours
against seven backgrounds to target 3:1 non-text contrast. The raw JSON switch
colours are inputs, not guaranteed final rendered colours. Shared switches
also apply the corrected on-track colour. This requires behaviour parity,
not only a CSS variable copy.

### Editor and syntax palette

Each theme's `highlight` block is independent of ordinary semantic UI colours.
No standard shadcn variables cover it. Preserve the editor theme separately.

| Editor key | OD | CD | SC |
| --- | --- | --- | --- |
| `editor.background` | `#15171b` | `#323230` | `#323230` |
| `editor.foreground` | `#abb2bf` | `#f0efed` | `#f0efed` |
| `editor.active_line.background` | `#1d2026` | `#3a3a37` | `#3a3a37` |
| `editor.line_number` | `#8b919c` | `#8a8984` | `#8a8984` |
| `editor.active_line_number` | `#d1d5dc` | `#f0efed` | `#f0efed` |
| `editor.invisible` | `#313640` | `#44423e` | `#44423e` |


| Syntax key | OD | CD | SC |
| --- | --- | --- | --- |
| `comment` | `color=#7f848e; font_style=italic` | `color=#7f8c98; font_style=italic` | `color=#7f8c98; font_style=italic` |
| `comment_doc` | `color=#7f848e; font_style=italic` | `color=#7f8c98; font_style=italic` | `color=#7f8c98; font_style=italic` |
| `keyword` | `color=#c678dd` | `color=#ff7ab2` | `color=#ff7ab2` |
| `preproc` | `color=#c678dd` | `color=#fd8f3f` | `color=#fd8f3f` |
| `string` | `color=#98c379` | `color=#ff8170` | `color=#ff8170` |
| `string.escape` | `color=#56b6c2` | `color=#d9c97c` | `color=#d9c97c` |
| `string.regex` | `color=#56b6c2` | `color=#d9c97c` | `color=#d9c97c` |
| `string.special` | `color=#56b6c2` | `color=#d9c97c` | `color=#d9c97c` |
| `string.special.symbol` | `color=#56b6c2` | `color=#d9c97c` | `color=#d9c97c` |
| `text.literal` | `color=#98c379` | `color=#ff8170` | `color=#ff8170` |
| `text.code.span` | `color=#98c379` | `color=#ff8170` | `color=#ff8170` |
| `number` | `color=#d19a66` | `color=#d9c97c` | `color=#d9c97c` |
| `boolean` | `color=#d19a66` | `color=#ff7ab2` | `color=#ff7ab2` |
| `constant` | `color=#d19a66` | `color=#d9c97c` | `color=#d9c97c` |
| `attribute` | `color=#d19a66` | `color=#cc9768` | `color=#cc9768` |
| `property` | `color=#e06c75` | `color=#67b7a4` | `color=#67b7a4` |
| `function` | `color=#61afef` | `color=#67b7a4` | `color=#67b7a4` |
| `constructor` | `color=#e5c07b` | `color=#6bdfff` | `color=#6bdfff` |
| `type` | `color=#e5c07b` | `color=#6bdfff` | `color=#6bdfff` |
| `enum` | `color=#e5c07b` | `color=#6bdfff` | `color=#6bdfff` |
| `variant` | `color=#e5c07b` | `color=#67b7a4` | `color=#67b7a4` |
| `variable.special` | `color=#e5c07b` | `color=#ff7ab2` | `color=#ff7ab2` |
| `variable` | `color=#e06c75` | `color=#f0efed` | `color=#f0efed` |
| `label` | `color=#e06c75` | `color=#cc9768` | `color=#cc9768` |
| `tag` | `color=#e06c75` | `color=#ff7ab2` | `color=#ff7ab2` |
| `title` | `color=#e06c75; font_weight=700` | `color=#f0efed; font_weight=600` | `color=#f0efed; font_weight=600` |
| `link_text` | `color=#61afef` | `color=#6ea3ff` | `color=#6ea3ff` |
| `link_uri` | `color=#56b6c2` | `color=#6ea3ff` | `color=#6ea3ff` |
| `operator` | `color=#56b6c2` | `color=#f0efed` | `color=#f0efed` |
| `punctuation` | `color=#abb2bf` | `color=#f0efed` | `color=#f0efed` |
| `punctuation.bracket` | `color=#abb2bf` | `color=#f0efed` | `color=#f0efed` |
| `punctuation.delimiter` | `color=#abb2bf` | `color=#f0efed` | `color=#f0efed` |
| `punctuation.special` | `color=#c678dd` | `color=#ff7ab2` | `color=#ff7ab2` |
| `embedded` | `color=#abb2bf` | `color=#f0efed` | `color=#f0efed` |

## Radius, type and density values

GPUI radii and dashboard density radii are separate systems.
One standard shadcn `--radius` with a derived scale cannot preserve them all.
Keep `--kit-radius`, `--kit-radius-lg`, `--control-radius`,
`--surface-radius` and the button-role radii separate.
`--nav-round` is a ratio of half the item height, not a pixel radius.

| Top-level GPUI property | OD | CD | SC |
| --- | --- | --- | --- |
| `radius` | `6` | `8` | `8` |
| `radius.lg` | `8` | `12` | `12` |
| `shadow` | `Absent` | `True` | `True` |
| `font.family` | `Absent` | `.SystemUIFont` | `.SystemUIFont` |
| `mono_font.family` | `Absent` | `Menlo` | `Menlo` |

### Font resolution and type roles

`subspace.fonts` overrides the top-level GPUI font fields. Channel's top-level
mono field is Menlo, but its effective requested mono family is
JetBrainsMono Nerd Font Mono. Missing families use installed fallbacks and
produce warnings. The application downloads no fonts.

| Subspace metadata property | OD | CD | SC |
| --- | --- | --- | --- |
| `fonts.heading` | `Automatic` | `.SystemUIFont` | `.SystemUIFont` |
| `fonts.body` | `Automatic` | `.SystemUIFont` | `.SystemUIFont` |
| `fonts.mono` | `Automatic` | `Menlo` | `JetBrainsMono Nerd Font Mono` |
| `groupHeading.font` | `Heading family` | `` | `` |
| `groupHeading.size` | `13` | `12` | `12.0` |
| `groupHeading.bold` | `True` | `False` | `False` |
| `groupHeading.italic` | `False` | `False` | `False` |

Automatic sans candidates start with `.SystemUIFont`, Segoe UI,
Helvetica Neue and Arial. Automatic mono candidates start with JetBrains Mono,
SF Mono, Menlo and Consolas. Resolution lives in
`host/app/src/theme/fonts.rs`; metadata parsing supplies defaults.
`.SystemUIFont` is a GPUI virtual family, not a portable CSS font family.

- Heading, body and mono families resolve independently.
- Tabs use the heading family but have an independent size token.
- Caption size is body size × 11/13; secondary size is body size × 12/13.
- Heading and mono reference sizes scale existing hierarchy proportionally.
- GPUI base font size is 14 px × body-size/13. Dashboard row text uses the
  explicit body token, which defaults to 13 px.
- Button metrics convert through a 14 px reference rem. Dividing source
  pixels by a browser's assumed 16 px rem changes the geometry.
- Tailwind font variables can bridge families, but shadcn has no standard
  heading/body/tab/mono/group-heading size and weight contract.

Sources: `board-kit/dashboard/src/density.rs`, `density/groups.rs`,
`tokens/controls.rs`, `tokens/sizes.rs`, `typography.rs` and
`host/app/src/theme.rs::sync_font_sizes`.

### All 41 density controls

Values below resolve theme metadata over declared density defaults, before
saved user overrides. The Source column distinguishes an explicit JSON value
from a fallback default. These tokens need CSS extensions or component rules;
standard shadcn semantic variables do not represent their dimensions.

| Token | OD | CD | SC | Source OD/CD/SC |
| --- | --- | --- | --- | --- |
| `--heading-font` | `15px` | `15px` | `15px` | default/JSON/default |
| `--row-font` | `13px` | `13px` | `13px` | default/JSON/default |
| `--mono-font` | `13px` | `12.5px` | `13px` | default/JSON/default |
| `--tab-font` | `15px` | `15px` | `15px` | default/default/default |
| `--row-h` | `28px` | `32px` | `32px` | default/JSON/JSON |
| `--pad-x` | `12px` | `12px` | `12px` | default/JSON/default |
| `--indent` | `24px` | `24px` | `24px` | default/JSON/default |
| `--row-inset` | `0px` | `8px` | `8px` | default/JSON/JSON |
| `--lane-w` | `360px` | `360px` | `270px` | default/default/JSON |
| `--done-w` | `0.6fr` | `0.6fr` | `0.6fr` | default/default/default |
| `--head-h` | `28px` | `32px` | `32px` | default/JSON/JSON |
| `--tree-w` | `360px` | `360px` | `360px` | default/default/default |
| `--chrome-h` | `30px` | `38px` | `38px` | default/JSON/JSON |
| `--toolbar-h` | `36px` | `44px` | `44px` | default/JSON/JSON |
| `--panel-w` | `600px` | `720px` | `720px` | default/JSON/JSON |
| `--motion` | `140ms` | `160ms` | `160ms` | default/JSON/JSON |
| `--radius` | `4px` | `6px` | `6px` | default/JSON/JSON |
| `--surface-radius` | `6px` | `12px` | `12px` | default/JSON/JSON |
| `--nav-round` | `1fr` | `1fr` | `1fr` | default/default/default |
| `--nav-h` | `28px` | `28px` | `28px` | default/default/default |
| `--dialog-pad` | `20px` | `20px` | `20px` | JSON/JSON/JSON |
| `--dialog-gap` | `12px` | `12px` | `12px` | JSON/JSON/JSON |
| `--liquid-glass` | `1` | `1` | `1` | JSON/JSON/JSON |
| `--toolbar-button-rest` | `flat` | `flat` | `flat` | default/default/default |
| `--toolbar-button-hover` | `flat` | `flat` | `flat` | default/default/default |
| `--toolbar-button-h` | `24px` | `24px` | `24px` | JSON/JSON/JSON |
| `--toolbar-button-pad-x` | `10px` | `10px` | `12px` | JSON/JSON/JSON |
| `--toolbar-button-font` | `13px` | `13px` | `12px` | JSON/JSON/JSON |
| `--toolbar-button-radius` | `12px` | `12px` | `11.5px` | JSON/JSON/JSON |
| `--action-button-h` | `20px` | `20px` | `24px` | JSON/JSON/JSON |
| `--action-button-pad-x` | `8px` | `8px` | `12px` | JSON/JSON/JSON |
| `--action-button-font` | `11px` | `11px` | `12px` | JSON/JSON/JSON |
| `--action-button-radius` | `6px` | `6px` | `12px` | JSON/JSON/JSON |
| `--dialog-button-h` | `24px` | `24px` | `24px` | JSON/JSON/JSON |
| `--dialog-button-pad-x` | `12px` | `12px` | `12px` | JSON/JSON/JSON |
| `--dialog-button-font` | `13px` | `13px` | `13px` | JSON/JSON/JSON |
| `--dialog-button-radius` | `7px` | `7px` | `7px` | JSON/JSON/JSON |
| `--compact-button-h` | `16px` | `16px` | `16px` | JSON/JSON/JSON |
| `--compact-button-pad-x` | `4px` | `4px` | `4px` | JSON/JSON/JSON |
| `--compact-button-font` | `10px` | `10px` | `10px` | JSON/JSON/JSON |
| `--compact-button-radius` | `5px` | `5px` | `5px` | JSON/JSON/JSON |

The legacy `Tokens::one_dark()` constructor uses 4 px control and
6 px raised-container radii, default geometry and the reference palette.
Applying canonical One Dark metadata changes its button geometry and enables
Liquid Glass. Reading only the constructor misses those applied values.

## Verification

This documentation-only inventory was checked against the current source:

- Parsed all 16 `catalog.rs::SECTIONS` dispatches in order.
- Followed every dispatch into its `ITEMS` array.
- Found 180 cards with 180 distinct names and no missing builder resolution.
- Resolved each sample builder to an existing source file and function,
  except shell builders, which use the verified `surface_samples!` invocation.
- Checked every repository-local implementation reference exists.
- Counted 81 Subspace, 15 shell and 84 Kit cards; Kit flags are 39 used and
  45 unused. Section counts are 12, 5, 11, 7, 3, 13, 10, 7, 5, 3, 5, 15,
  37, 13, 17 and 17 in dispatch order.
- Read dynamic state arrays for buttons, switches, GitHub sign-in, dialog
  footers and replica boxes rather than treating literal caption extraction
  as complete coverage.
- Parsed all three canonical theme variants and included every explicit
  `colors` key, editor property and syntax assignment in their union.
- Included all 41 density controls, with metadata/default provenance.
- Preserved `plan.md` byte-for-byte. No app launch, Rust test run, pixel
  comparison, commit or push is claimed by this appendix.

Rendered parity remains unverified. Installed fonts, contrast correction,
saved preferences, native materials and rem-based scaling affect the result.
