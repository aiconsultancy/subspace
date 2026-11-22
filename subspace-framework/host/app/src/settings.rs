//! Service credentials and appearance choices. Saved tokens are never read
//! into an input; an empty input keeps its token unless Clear is selected.

#[cfg(test)]
mod agent_icon_tests;
mod agent_icons;
#[cfg(test)]
mod background_update_tests;
mod background_updates;
pub(crate) mod board_choice;
#[cfg(test)]
pub(crate) mod board_editor_tests;
mod board_editors;
#[cfg(test)]
mod board_field_tests;
mod board_fields;
mod board_navigation;
pub(crate) mod board_order;
#[cfg(test)]
mod board_page_tests;
mod chat_engine;
mod configured;
#[cfg(test)]
mod connection_tests;
mod copy;
pub(crate) mod extensions;
mod form;
#[cfg(test)]
mod icon_button_tests;
mod icon_buttons;
mod mcp_draft;
#[cfg(test)]
mod mcp_page_tests;
#[cfg(test)]
mod navigation_layout_tests;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod notification_tests;
#[cfg(test)]
mod organization_tests;
mod pickers;
#[cfg(test)]
mod preview_tests;
mod repositories;
#[cfg(test)]
mod repository_tests;
#[cfg(test)]
mod size_tests;
mod sizes;
mod snapshot;
#[cfg(test)]
mod snapshot_tests;
mod sources;
mod terminal;
#[cfg(test)]
mod terminal_tests;
mod toolbar;
#[cfg(test)]
mod toolbar_tests;
#[cfg(test)]
mod update_tests;
mod view;

use std::sync::Arc;

use chat_engine::{
    acp_agent_choice, acp_agent_from_label, chat_engine_choice, chat_engine_from_label,
    validate_acp_feature, validate_chat_feature,
};
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{SearchableVec, SelectDelegate, SelectEvent, SelectItem};
use gpui_kit::component::tree::TreeEvent;
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Pixels, Point, ScrollHandle, SharedString,
    Subscription, Window,
};
use subspace_core::config::{self, ChatEngine};
use subspace_core::credentials::{Credentials, GITHUB_ACCOUNT_KEY, GitHubAccounts};
use subspace_core::features::{ACP_CHAT, FeatureSpec, PI_CHAT};
use subspace_core::settings::SettingsFile;
use subspace_dashboard::density::DensityPreview;
use subspace_dashboard::github_sign_in::{self, GitHubSignIn, SignInEvent};
use subspace_dashboard::prefs;

use crate::notifications::{NotificationChoice, NotificationPreferences};
use crate::theme::{self, AUTOMATIC_FONT};
use board_navigation::NavigationDraft;
use form::{Choice, Picker, ServiceForm, SettingsPage, choice, picker};
use pickers::{FontItem, ThemeList};
use view::navigation::Navigation;

const APP_FEATURES: &[FeatureSpec] = &[PI_CHAT, ACP_CHAT];
const CHAT_OFF_AFTER_RESTART: &str = "Chat will be off after saving and restarting Subspace. Choose another engine to keep using Chat.";
pub(crate) const CONFIRM_QUIT_KEY: &str = "app.confirm-quit";

/// Startup update checks (REQ-07), default on.
pub(crate) const CHECK_UPDATES_KEY: &str = "app.check-updates";
const APPROVE_ALL_DEFAULT_KEY: &str = "chat.approve-all-default";
/// Developer mode shows the Developer menu and the inspector in debug builds.
pub(crate) const DEVELOPER_MODE_KEY: &str = "app.developer-mode";

/// Developer mode defaults to on; only an explicit "0" turns it off.
fn developer_mode_enabled(value: Option<&str>) -> bool {
    value != Some("0")
}

pub(crate) fn developer_mode(cx: &App) -> bool {
    developer_mode_enabled(prefs::get(cx, DEVELOPER_MODE_KEY).as_deref())
}

/// Startup update checks default to on; only an explicit "0" opts out.
pub(crate) fn check_updates_enabled(value: Option<&str>) -> bool {
    value != Some("0")
}

fn check_updates_preference(enabled: bool) -> &'static str {
    if enabled { "1" } else { "0" }
}

fn approve_all_default(value: Option<&str>) -> bool {
    value == Some("1")
}

fn approval_default(cx: &App) -> bool {
    approve_all_default(prefs::get(cx, APPROVE_ALL_DEFAULT_KEY).as_deref())
}

/// Whether this app's product has an update source at all (REQ-06, REQ-07).
pub(super) fn has_update_source(cx: &App) -> bool {
    cx.global::<subspace_dashboard::CurrentProduct>()
        .0
        .product
        .update
        .is_some()
}

// qual:allow(srp, god_struct) reason: "One retained editor owns input entities, subscriptions, drafts and the preview snapshot through Save/Cancel; partial-save errors retain that same Settings entity."
pub struct Settings {
    file: SettingsFile,
    form: ServiceForm,
    mcp: mcp_draft::McpDraft,
    page: SettingsPage,
    boards: Vec<crate::registry::BoardRegistration>,
    extensions: Vec<(&'static str, gpui_kit::AnyView)>,
    initial_board_order: Vec<String>,
    navigation: Navigation,
    themes: ThemePicker,
    chat_engine: Choice,
    acp_agent: Choice,
    notifications: NotificationPreferences,
    sources: Vec<sources::Source>,
    background_updates: Vec<background_updates::Draft>,
    board_fields: Vec<board_fields::Draft>,
    board_editors: Vec<board_editors::Draft>,
    confirm_quit: bool,
    initial_confirm_quit: bool,
    fonts: [FontPicker; 3],
    font_sizes: Vec<Entity<InputState>>,
    icon_buttons: icon_buttons::Draft<subspace_dashboard::icon_button::IconButtonSize>,
    agent_icons: icon_buttons::Draft<subspace_dashboard::avatar::AgentIcons>,
    account: Choice,
    initial_account: Option<String>,
    /// The accounts under GitHub accounts, and the shared sign-in.
    github: AccountList,
    error: Option<SharedString>,
    notice: Option<SharedString>,
    services_saved: bool,
    initial_appearance: theme::AppearanceSnapshot,
    scroll: ScrollHandle,
    /// The saved-token field whose locked input was last clicked.
    secret_hint: Option<gpui_kit::ElementId>,
    focus: FocusHandle,
    check_updates: bool,
    auto_refresh: bool,
    developer_mode: bool,
    /// The docked terminal's drafts, for a product with a terminal.
    terminal: Option<terminal::TerminalDraft>,
    approve_all_default: bool,
    toolbar: toolbar::ToolbarDraft,
    initial_toolbar: toolbar::ToolbarDraft,
    board_navigation: NavigationDraft,
    initial_draft: Option<snapshot::DraftSnapshot>,
    partial_draft: Option<snapshot::DraftSnapshot>,
    _subscriptions: Vec<Subscription>,
}

/// App-level values that Save writes beside the service form's values.
struct AppValues {
    board_edits: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
    board_order: Vec<String>,
    /// The terminal font editor's changed values, set in the config file's
    /// `terminal` object.
    terminal: Option<serde_json::Map<String, serde_json::Value>>,
    mcp: subspace_core::config::McpSettings,
}

type ThemePicker = Picker<ThemeList>;
type FontPicker = Picker<SearchableVec<FontItem>>;

struct SubscriptionInputs<'a> {
    themes: &'a ThemePicker,
    fonts: &'a [FontPicker; 3],
    font_sizes: &'a [Entity<InputState>],
    icon_buttons: &'a Choice,
    agent_icons: &'a Choice,
    chat_engine: &'a Choice,
    navigation: &'a Navigation,
}

/// The scroll offset a freshly shown section starts from.
fn top_of_page() -> Point<Pixels> {
    Point::default()
}

impl Settings {
    pub fn open(window: &mut Window, cx: &mut App) -> Result<Entity<Self>, String> {
        let path = crate::product::gui_config_file(cx);
        let file = SettingsFile::open(&path).map_err(|error| error.to_string())?;
        // Reads the signed-in list and gh's hosts.yml for logins only; no
        // token is read.
        let accounts = Credentials::from_system(&config::load(&path).0)
            .github_accounts()
            .unwrap_or_default();
        Ok(cx.new(|cx| Self::new(file, accounts, window, cx)))
    }

    fn new(
        file: SettingsFile,
        accounts: GitHubAccounts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut settings = Self::draft(file, accounts, window, cx);
        settings._subscriptions = Self::subscriptions(
            SubscriptionInputs {
                themes: &settings.themes,
                fonts: &settings.fonts,
                font_sizes: &settings.font_sizes,
                icon_buttons: &settings.icon_buttons.choice,
                agent_icons: &settings.agent_icons.choice,
                chat_engine: &settings.chat_engine,
                navigation: &settings.navigation,
            },
            window,
            cx,
        );
        // Picks in the repository search change the draft list.
        let repos = settings.form.repos.clone();
        settings
            ._subscriptions
            .push(cx.observe(&repos, |_, _, cx| cx.notify()));
        settings._subscriptions.extend(Self::sign_in_subscriptions(
            settings.github.sign_in.as_ref(),
            window,
            cx,
        ));
        settings
            ._subscriptions
            .extend(Self::account_subscription(cx));
        settings.initial_draft = Some(settings.draft_snapshot(cx));
        settings
    }

    fn account_subscription(cx: &mut Context<Self>) -> Option<Subscription> {
        crate::account::shared(cx).map(|account| cx.observe(&account, |_, _, cx| cx.notify()))
    }

    /// Redraws as the shared sign-in moves on, and lists accounts again when
    /// one signs in or out.
    fn sign_in_subscriptions(
        sign_in: Option<&Entity<GitHubSignIn>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let Some(sign_in) = sign_in else {
            return Vec::new();
        };
        vec![
            cx.observe(sign_in, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                sign_in,
                window,
                |settings, _, _: &SignInEvent, window, cx| {
                    settings.refresh_accounts(window, cx);
                },
            ),
        ]
    }

    /// Reads the accounts again after a sign-in or sign-out. The chosen
    /// account stays chosen while it is still listed.
    fn refresh_accounts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let accounts = current_accounts(cx);
        let logins = SearchableVec::new(accounts.logins.clone());
        self.account
            .update(cx, |account, cx| account.set_items(logins, window, cx));
        self.github.accounts = accounts;
        cx.notify();
    }

    /// Starts Sign in with GitHub.
    fn start_sign_in(&self, cx: &mut Context<Self>) {
        if let Some(sign_in) = &self.github.sign_in {
            sign_in.update(cx, |sign_in, cx| sign_in.start(cx));
        }
    }

    fn cancel_sign_in(&self, cx: &mut Context<Self>) {
        if let Some(sign_in) = &self.github.sign_in {
            sign_in.update(cx, |sign_in, cx| sign_in.cancel(cx));
        }
    }

    fn sign_out(&self, login: String, cx: &mut Context<Self>) {
        if let Some(sign_in) = &self.github.sign_in {
            sign_in.update(cx, |sign_in, cx| sign_in.sign_out(login, cx));
        }
    }

    fn draft(
        mut file: SettingsFile,
        accounts: GitHubAccounts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let file_values = file.values().clone();
        let (boards, initial_board_order) = load_board_order(&file_values.board_order, cx);
        let navigation = Navigation::new(&boards, cx);
        let form = board_form(&file_values, &boards, window, cx);
        let (editors, errors) = board_editors::load(&boards, &mut file, &form, window, cx);
        let (account, initial_account) = account_draft(&accounts, window, cx);
        let current_theme = theme::selected_theme(cx);
        let confirm_quit = confirm_quit_enabled(prefs::get(cx, CONFIRM_QUIT_KEY).as_deref());
        let themes = Self::theme_choice(&current_theme, window, cx);
        let (fonts, font_sizes, error) = font_controls(&current_theme, window, cx);
        let (initial_appearance, check_updates) = begin_preview(cx);
        let mcp_token = crate::mcp::token_status(&crate::mcp::token_file(cx));
        let terminal = terminal::TerminalDraft::load(&file, window, cx);
        Self {
            board_fields: board_fields::load(&boards, &file, window, cx),
            board_editors: editors,
            file,
            form,
            mcp: mcp_draft::McpDraft::new(&file_values.mcp, mcp_token, window, cx),
            page: SettingsPage::General,
            sources: sources::load(&boards, window, cx),
            background_updates: background_updates::load(&boards, window, cx),
            boards,
            extensions: extensions::create_views(window, cx),
            initial_board_order,
            navigation,
            themes,
            chat_engine: chat_engine_choice(file_values.chat_engine, window, cx),
            acp_agent: acp_agent_choice(file_values.acp_agent, window, cx),
            notifications: NotificationPreferences::load(cx),
            confirm_quit,
            initial_confirm_quit: confirm_quit,
            fonts,
            font_sizes,
            icon_buttons: icon_buttons::Draft::new(window, cx),
            agent_icons: icon_buttons::Draft::new(window, cx),
            account,
            initial_account,
            github: AccountList::new(accounts, cx),
            error: error.or_else(|| warning_message(errors)),
            notice: None,
            services_saved: false,
            initial_appearance,
            scroll: ScrollHandle::default(),
            secret_hint: None,
            focus: cx.focus_handle(),
            check_updates,
            auto_refresh: file_values.auto_refresh,
            developer_mode: developer_mode(cx),
            terminal,
            approve_all_default: approval_default(cx),
            toolbar: toolbar::ToolbarDraft::load(cx),
            initial_toolbar: toolbar::ToolbarDraft::load(cx),
            board_navigation: NavigationDraft::new(window, cx),
            initial_draft: None,
            partial_draft: None,
            _subscriptions: Vec::new(),
        }
    }

    fn theme_choice(current: &str, window: &mut Window, cx: &mut Context<Self>) -> ThemePicker {
        let items = ThemeList::new(pickers::theme_items(cx));
        picker(items, Some(current), true, window, cx)
    }

    fn subscriptions(
        inputs: SubscriptionInputs<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let mut subscriptions = vec![Self::preview_on_confirm(inputs.themes, window, cx)];
        subscriptions.extend(
            inputs
                .fonts
                .iter()
                .map(|select| Self::preview_on_confirm(select, window, cx)),
        );
        subscriptions.extend(sizes::subscribe(inputs.font_sizes, window, cx));
        subscriptions.push(cx.observe(inputs.icon_buttons, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(inputs.agent_icons, |_, _, cx| cx.notify()));
        // The ACP agent choice shows only while ACP is the selected engine.
        subscriptions.push(cx.observe(inputs.chat_engine, |_, _, cx| cx.notify()));
        subscriptions.extend(inputs.navigation.states().map(|state| {
            cx.observe(state, |settings, selected, cx| {
                if let Some(page) = settings.navigation.activate(&selected, cx)
                    && settings.page != page
                {
                    settings.page = page;
                    settings.scroll.set_offset(top_of_page());
                    cx.notify();
                }
            })
        }));
        subscriptions.push(
            cx.subscribe(&inputs.navigation.board_state, |_, _, _: &TreeEvent, cx| {
                cx.notify()
            }),
        );
        // The MCP page shows the app server's state as it changes.
        subscriptions.push(cx.observe_global::<crate::mcp::McpService>(|_, cx| cx.notify()));
        subscriptions
    }

    /// Previewing the appearance as soon as a choice is confirmed.
    fn preview_on_confirm<D>(
        select: &Picker<D>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription
    where
        D: SelectDelegate + 'static,
        D::Item: SelectItem<Value = String>,
    {
        cx.subscribe_in(
            select,
            window,
            |settings: &mut Self, _, event, window, cx| {
                if matches!(event, SelectEvent::Confirm(Some(_))) {
                    settings.preview(window, cx);
                }
            },
        )
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    /// Clear a saved token, or undo that choice, and empty its input.
    fn toggle_clear(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let token = &mut self.form.tokens[index];
        token.clear = !token.clear;
        token
            .input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.secret_hint = None;
        cx.notify();
    }

    /// Whether the saved-token field `field` was clicked last.
    fn is_secret_hinted(&self, field: &gpui_kit::ElementId) -> bool {
        self.secret_hint.as_ref() == Some(field)
    }

    /// Explain why a saved token's field does not accept text.
    fn show_secret_hint(&mut self, field: gpui_kit::ElementId, cx: &mut Context<Self>) {
        self.secret_hint = Some(field);
        cx.notify();
    }

    fn toggle_feature(
        &mut self,
        feature: &'static FeatureSpec,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.form.set_feature_enabled(feature, enabled);
        let engine = match feature.id {
            id if id == PI_CHAT.id => Some(ChatEngine::Pi),
            id if id == ACP_CHAT.id => Some(ChatEngine::Acp),
            _ => None,
        };
        if let Some(engine) = engine {
            let turning_chat_off = !enabled && self.selected_chat_engine(cx) == engine;
            if turning_chat_off {
                let off = ChatEngine::Off.label().to_owned();
                self.chat_engine.update(cx, |engine, cx| {
                    engine.set_selected_value(&off, window, cx);
                });
            }
            self.notice = turning_chat_off.then(|| CHAT_OFF_AFTER_RESTART.into());
        }
        cx.notify();
    }

    fn appearance_selection(&self, cx: &App) -> Result<(String, theme::FontChoices), String> {
        let theme = self
            .themes
            .read(cx)
            .selected_value()
            .cloned()
            .ok_or("Select a theme.")?;
        let fonts = theme::FontChoices {
            heading: font_selection(&self.fonts[0], cx)?,
            body: font_selection(&self.fonts[1], cx)?,
            mono: font_selection(&self.fonts[2], cx)?,
        };
        Ok((theme, fonts))
    }

    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self
            .appearance_selection(cx)
            .and_then(|(theme, font)| theme::preview_selection(&theme, &font, cx))
        {
            Ok(warnings) => self.error = warning_message(warnings),
            Err(error) => self.error = Some(error.into()),
        }
        sizes::apply(&self.font_sizes, cx);
        window.refresh();
        cx.notify();
    }

    pub fn restore_preview(&self, window: &mut Window, cx: &mut App) -> Result<(), String> {
        self.initial_appearance.restore(cx);
        window.refresh();
        Ok(())
    }

    /// Returns whether saved service changes need a restart. The two files
    /// have separate save boundaries; report any partial save explicitly.
    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<bool, String> {
        let board_edits = board_editors::edits(&self.board_editors, cx)?;
        validate_chat_feature(
            self.selected_chat_engine(cx),
            self.form.feature_enabled(&PI_CHAT),
            &PI_CHAT,
        )?;
        validate_acp_feature(
            self.selected_chat_engine(cx),
            self.form.feature_enabled(&ACP_CHAT),
            &ACP_CHAT,
        )?;
        let mcp = self.validated_mcp(cx)?;
        let (selected_theme, selected_fonts) = self.appearance_selection(cx)?;
        sizes::commit_all(&self.font_sizes, window, cx);
        let board_order: Vec<String> = self
            .boards
            .iter()
            .map(|board| board.id.to_owned())
            .collect();
        let restart_required = self.restart_required(cx);
        // Board choices and favourite repositories rebuild the boards' tools.
        let boards_changed = self.boards_changed(cx) || self.repos_changed(cx);
        // MCP changes apply on Save without a restart (REQ-06).
        let settings_changed = restart_required
            || boards_changed
            || self.form.values(cx).features != self.file.values().features
            || board_order != self.initial_board_order
            || self.runtime_settings_changed(cx)
            || mcp != self.file.values().mcp;
        if settings_changed {
            let app_values = AppValues {
                board_edits,
                board_order,
                terminal: self.terminal_font_edits(cx)?,
                mcp,
            };
            self.save_services(window, cx, app_values, restart_required)?;
        }
        if boards_changed {
            self.apply_board_choice(window, cx);
        }
        // Runs even without changes, so an Error state is retried (REQ-14).
        crate::mcp::reconcile(&self.file.values().mcp, cx);
        self.mcp.accept(&self.file.values().mcp);
        self.save_account(cx);
        if let Err(error) = theme::save_selection(&selected_theme, &selected_fonts, cx) {
            return Err(partial_save_message(&error, self.services_saved));
        }
        self.save_preferences(cx);
        cx.set_global(DensityPreview(false));
        background_updates::save(&mut self.background_updates, cx)?;
        self.icon_buttons.save(cx)?;
        self.agent_icons.save(cx)?;
        self.initial_appearance = theme::AppearanceSnapshot::capture(cx);
        self.initial_draft = Some(self.draft_snapshot(cx));
        self.partial_draft = None;
        window.refresh();
        Ok(self.services_saved)
    }

    fn save_preferences(&mut self, cx: &mut Context<Self>) {
        self.notifications.save(cx);
        for source in &mut self.sources {
            let chosen = source.preferences(cx);
            if chosen != source.loaded {
                chosen.save(source.id, cx);
                source.loaded = chosen;
            }
        }
        if self.confirm_quit != self.initial_confirm_quit {
            prefs::set(cx, CONFIRM_QUIT_KEY, self.confirm_quit.to_string());
            self.initial_confirm_quit = self.confirm_quit;
        }
        prefs::set(
            cx,
            CHECK_UPDATES_KEY,
            check_updates_preference(self.check_updates),
        );
        prefs::set(
            cx,
            "chat.approve-all-default",
            if self.approve_all_default { "1" } else { "0" },
        );
        self.toolbar.save(&self.initial_toolbar, cx);
        self.initial_toolbar = self.toolbar;
        self.board_navigation.save(cx);
        if let Some(terminal) = &mut self.terminal {
            terminal.save(cx);
        }
        self.save_developer_mode(cx);
    }

    /// Writes a changed Developer mode and shows or hides the Developer menu.
    fn save_developer_mode(&self, cx: &mut Context<Self>) {
        if self.developer_mode == developer_mode(cx) {
            return;
        }
        let value = if self.developer_mode { "1" } else { "0" };
        prefs::set(cx, DEVELOPER_MODE_KEY, value);
        crate::app::install_menus(cx);
    }

    /// The MCP draft as saved policy. An invalid field is marked on the page
    /// and blocks the whole Save before anything is written (REQ-04, REQ-05).
    fn validated_mcp(&mut self, cx: &App) -> Result<subspace_core::config::McpSettings, String> {
        let result = mcp_draft::draft_settings(&self.file.values().mcp, &self.mcp.fields(cx));
        self.mcp.error = result.as_ref().err().copied();
        result.map_err(|error| mcp_draft::save_error(&error))
    }

    /// Whether the saved service values need a restart. Board choices apply
    /// on Save (`apply_board_choice`), so a board-only change does not count.
    fn restart_required(&self, cx: &App) -> bool {
        let mut original = self.file.values().clone();
        let draft = self.form.values(cx);
        original.boards = draft.boards;
        original.repos = draft.repos;
        for id in self.live_feature_ids() {
            match draft.features.get(id) {
                Some(value) => {
                    original.features.insert(id.to_owned(), *value);
                }
                None => {
                    original.features.remove(id);
                }
            }
        }
        self.form.changed(&original, cx)
            || self.selected_chat_engine(cx) != self.file.values().chat_engine
            || self.selected_acp_agent(cx) != self.file.values().acp_agent
    }

    fn live_feature_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.boards.iter().flat_map(|board| {
            board.settings.live_features.iter().copied().filter(|id| {
                board
                    .settings
                    .features
                    .iter()
                    .any(|feature| feature.id == *id)
            })
        })
    }

    /// Sets a board's draft choice as its Enabled switch does.
    #[cfg(test)]
    pub(crate) fn draft_board_enabled(&mut self, id: &str, enabled: bool) {
        self.form.set_board_enabled(id, enabled);
    }

    fn boards_changed(&self, cx: &App) -> bool {
        self.form.values(cx).boards != self.file.values().boards
    }

    /// Favourite repositories apply on Save without a restart.
    fn repos_changed(&self, cx: &App) -> bool {
        self.form.repos.read(cx).favourites() != self.file.values().repos
    }

    /// Shows and hides the saved board choice without a restart. When the
    /// board set cannot be rebuilt, the choice applies after a restart.
    fn apply_board_choice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.services_saved |= !board_choice::apply_saved_boards(window, cx);
    }

    fn runtime_settings_changed(&self, cx: &App) -> bool {
        board_fields::changed(&self.board_fields, cx)
            || board_editors::changed(&self.board_editors, cx)
            || self.auto_refresh != self.file.values().auto_refresh
            || self
                .terminal
                .as_ref()
                .is_some_and(|terminal| terminal.fonts_changed(cx))
    }

    fn terminal_font_edits(
        &self,
        cx: &App,
    ) -> Result<Option<serde_json::Map<String, serde_json::Value>>, String> {
        self.terminal
            .as_ref()
            .map_or(Ok(None), |terminal| terminal.font_edits(cx))
    }

    fn save_services(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        app_values: AppValues,
        restart_required: bool,
    ) -> Result<(), String> {
        let mut values = self.form.values(cx);
        values.chat_engine = self.selected_chat_engine(cx);
        values.acp_agent = self.selected_acp_agent(cx);
        values.board_order = app_values.board_order;
        values.auto_refresh = self.auto_refresh;
        values.terminal = app_values.terminal.unwrap_or_default();
        values.mcp = app_values.mcp;
        values.board_fields = board_fields::edits(&self.board_fields, cx)?;
        values.board_files = app_values.board_edits;
        self.file.save(&values).map_err(|error| error.to_string())?;
        values.terminal.clone_from(&self.file.values().terminal);
        self.partial_draft = Some(self.draft_snapshot(cx));
        self.initial_board_order.clone_from(&values.board_order);
        let saved_boards = board_editors::saved(&values.board_files, &self.file);
        mirror_services(&values, saved_boards, self.live_feature_ids(), cx);
        board_fields::accept(&mut self.board_fields, &self.file, window, cx);
        board_editors::accept(&mut self.board_editors, cx);
        if let Some(terminal) = &mut self.terminal {
            terminal.accept_fonts(cx);
        }
        self.services_saved |= restart_required;
        for token in &mut self.form.tokens {
            token.clear = false;
            token
                .input
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        Ok(())
    }

    fn selected_acp_agent(&self, cx: &App) -> config::AcpAgent {
        acp_agent_from_label(self.acp_agent.read(cx).selected_value().map(String::as_str))
    }

    fn selected_chat_engine(&self, cx: &App) -> ChatEngine {
        chat_engine_from_label(
            self.chat_engine
                .read(cx)
                .selected_value()
                .map(String::as_str),
        )
    }

    fn save_account(&mut self, cx: &mut Context<Self>) {
        if let Some(login) = self.account.read(cx).selected_value().cloned()
            && self.initial_account.as_deref() != Some(login.as_str())
        {
            self.partial_draft = Some(self.draft_snapshot(cx));
            prefs::set(cx, GITHUB_ACCOUNT_KEY, login.clone());
            self.initial_account = Some(login);
            self.services_saved = true;
        }
    }

    fn set_notification(
        &mut self,
        choice: NotificationChoice,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        self.notifications.set(choice, enabled);
        cx.notify();
    }

    fn set_confirm_quit(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.confirm_quit = enabled;
        cx.notify();
    }

    pub(super) fn check_updates(&self) -> bool {
        self.check_updates
    }

    fn set_check_updates(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.check_updates = enabled;
        cx.notify();
    }

    #[cfg(debug_assertions)]
    fn set_developer_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.developer_mode = enabled;
        cx.notify();
    }

    pub(super) fn set_auto_refresh(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.auto_refresh = enabled;
        cx.notify();
    }

    /// Changes one toolbar control's draft label choice; Save applies it.
    pub(super) fn set_toolbar_label(
        &mut self,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(slot) = self.toolbar.labels.get_mut(index) {
            *slot = enabled;
            cx.notify();
        }
    }

    /// Changes the draft terminal button text choice; Save applies it.
    pub(super) fn set_terminal_labels(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(terminal) = &mut self.terminal {
            terminal.labels = enabled;
            cx.notify();
        }
    }

    /// Changes the draft toolbar collapse choice; Save applies it.
    pub(super) fn set_toolbar_menu(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.toolbar.menu = enabled;
        cx.notify();
    }

    pub(super) fn move_board(&mut self, id: &str, delta: isize, cx: &mut Context<Self>) {
        let Some(index) = self.boards.iter().position(|board| board.id == id) else {
            return;
        };
        let target = index as isize + delta;
        if target.is_negative() || target as usize >= self.boards.len() {
            return;
        }
        self.boards.swap(index, target as usize);
        cx.notify();
    }

    pub fn show_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error.into());
        cx.notify();
    }
}

fn load_board_order(
    saved: &[String],
    cx: &App,
) -> (Vec<crate::registry::BoardRegistration>, Vec<String>) {
    let mut boards = crate::registry::included_registrations(cx);
    let order = effective_board_order(&boards, saved);
    boards.sort_by_key(|board| order.iter().position(|id| id == board.id));
    (boards, order)
}

fn effective_board_order(
    boards: &[crate::registry::BoardRegistration],
    saved: &[String],
) -> Vec<String> {
    crate::registry::normalize_order(boards, saved)
}

fn included_board_defaults(
    boards: &[crate::registry::BoardRegistration],
    cx: &App,
) -> Vec<(String, bool)> {
    let product = cx.try_global::<subspace_dashboard::CurrentProduct>();
    if !boards.is_empty() {
        return boards
            .iter()
            .map(|board| {
                let default_enabled = product
                    .and_then(|product| {
                        product
                            .0
                            .product
                            .boards
                            .iter()
                            .find(|descriptor| descriptor.id.as_str() == board.id)
                    })
                    .is_none_or(|descriptor| descriptor.default_enabled);
                (board.id.to_owned(), default_enabled)
            })
            .collect();
    }
    product
        .map(|product| {
            product
                .0
                .product
                .boards
                .iter()
                .map(|board| (board.id.as_str().to_owned(), board.default_enabled))
                .collect()
        })
        .unwrap_or_default()
}

fn board_form(
    values: &subspace_core::settings::ServiceSettings,
    boards: &[crate::registry::BoardRegistration],
    window: &mut Window,
    cx: &mut App,
) -> ServiceForm {
    ServiceForm::new(values, &included_board_defaults(boards, cx), window, cx)
}

fn begin_preview(cx: &mut App) -> (theme::AppearanceSnapshot, bool) {
    let appearance = theme::AppearanceSnapshot::capture(cx);
    cx.set_global(DensityPreview(true));
    let updates = check_updates_enabled(prefs::get(cx, CHECK_UPDATES_KEY).as_deref());
    (appearance, updates)
}

fn font_controls(
    current_theme: &str,
    window: &mut Window,
    cx: &mut App,
) -> (
    [FontPicker; 3],
    Vec<Entity<InputState>>,
    Option<SharedString>,
) {
    let fonts = font_selects(window, cx);
    let sizes = sizes::inputs(window, cx);
    let warnings = theme::font_warnings(current_theme, &theme::selected_fonts(cx), cx)
        .unwrap_or_else(|error| vec![error]);
    (fonts, sizes, warning_message(warnings))
}

/// The accounts Settings lists, and the app's shared Sign in with GitHub
/// when the shell installed it.
struct AccountList {
    accounts: GitHubAccounts,
    sign_in: Option<Entity<GitHubSignIn>>,
}

impl AccountList {
    fn new(accounts: GitHubAccounts, cx: &App) -> Self {
        Self {
            accounts,
            sign_in: github_sign_in::shared(cx),
        }
    }

    fn found(&self) -> bool {
        !self.accounts.logins.is_empty()
    }
}

/// The accounts the shared credentials list now, else none.
fn current_accounts(cx: &App) -> GitHubAccounts {
    cx.try_global::<subspace_dashboard::Services>()
        .and_then(|services| services.credentials.github_accounts().ok())
        .unwrap_or_default()
}

fn account_draft(
    accounts: &GitHubAccounts,
    window: &mut Window,
    cx: &mut App,
) -> (Choice, Option<String>) {
    let initial_account = accounts.pick(prefs::get(cx, GITHUB_ACCOUNT_KEY).as_deref());
    let account = choice(
        accounts.logins.clone(),
        initial_account.as_deref(),
        false,
        window,
        cx,
    );
    (account, initial_account)
}

fn font_selects(window: &mut Window, cx: &mut App) -> [FontPicker; 3] {
    let fonts = theme::selected_fonts(cx);
    let items: Vec<_> = theme::font_choices(cx)
        .into_iter()
        .map(FontItem::new)
        .collect();
    [fonts.heading, fonts.body, fonts.mono].map(|font| {
        let list = SearchableVec::new(items.clone());
        picker(list, Some(shown_font(&font)), true, window, cx)
    })
}

fn font_selection(select: &FontPicker, cx: &App) -> Result<String, String> {
    select
        .read(cx)
        .selected_value()
        .map(|font| saved_font(font))
        .ok_or_else(|| "Select a font for each role.".to_owned())
}

fn warning_message(warnings: Vec<String>) -> Option<SharedString> {
    (!warnings.is_empty()).then(|| warnings.join("\n").into())
}

/// The list entry for a saved font; an empty saved font means automatic.
fn shown_font(saved: &str) -> &str {
    if saved.is_empty() {
        AUTOMATIC_FONT
    } else {
        saved
    }
}

/// The saved value for a chosen font; the automatic entry saves as empty.
fn saved_font(chosen: &str) -> String {
    if chosen == AUTOMATIC_FONT {
        String::new()
    } else {
        chosen.to_owned()
    }
}

/// Apply saved settings that take effect without a restart, including the
/// board choices. Board-owned settings, including `saved_boards` from the
/// boards' own files, are mirrored into the saved config snapshot.
fn mirror_services(
    values: &subspace_core::settings::ServiceSettings,
    saved_boards: Vec<(String, serde_json::Map<String, serde_json::Value>)>,
    live_features: impl Iterator<Item = &'static str>,
    cx: &mut App,
) {
    if !cx.has_global::<subspace_dashboard::Services>() {
        return;
    }
    let file = crate::product::gui_config_file(cx);
    let services = cx.global_mut::<subspace_dashboard::Services>();
    let mut config = (*services.config).clone();
    config.auto_refresh = values.auto_refresh;
    config.board_order.clone_from(&values.board_order);
    config.terminal.clone_from(&values.terminal);
    config.mcp.clone_from(&values.mcp);
    config.repos.clone_from(&values.repos);
    for id in live_features {
        match values.features.get(id) {
            Some(value) => {
                config.features.insert(id.to_owned(), *value);
            }
            None => {
                config.features.remove(id);
            }
        }
    }
    if !values.board_fields.is_empty() || values.boards != config.boards {
        load_saved_boards(&mut config, &file);
    }
    config.board_settings.extend(saved_boards);
    services.config = Arc::new(config);
}

/// Reads the saved board choices and web Spaces into `config`.
fn load_saved_boards(config: &mut config::Config, file: &std::path::Path) {
    let saved = config::load(file).0;
    config.web_boards = saved.web_boards;
    config.boards = saved.boards;
}

/// What to report when the services saved but the appearance did not.
fn partial_save_message(error: &str, services_saved: bool) -> String {
    if services_saved {
        format!("Service settings were saved and need a restart. Appearance was not saved: {error}")
    } else {
        error.to_owned()
    }
}

fn confirm_quit_enabled(value: Option<&str>) -> bool {
    value == Some("true")
}

#[cfg(test)]
mod tests {
    use super::{
        AUTOMATIC_FONT, confirm_quit_enabled, partial_save_message, saved_font, shown_font,
        top_of_page,
    };
    use gpui_kit::px;

    #[test]
    fn the_automatic_font_is_stored_as_an_empty_choice() {
        assert_eq!(shown_font(""), AUTOMATIC_FONT);
        assert_eq!(shown_font("Menlo"), "Menlo");
        assert_eq!(saved_font(AUTOMATIC_FONT), "");
        assert_eq!(saved_font("Menlo"), "Menlo");
    }

    #[test]
    fn each_font_role_keeps_its_own_choice() {
        let choices = ["Inter", AUTOMATIC_FONT, "Menlo"].map(saved_font);
        assert_eq!(choices, ["Inter", "", "Menlo"]);
        assert_eq!(shown_font(&choices[1]), AUTOMATIC_FONT);
    }

    #[test]
    fn a_new_section_starts_at_the_top() {
        assert_eq!(top_of_page().x, px(0.));
        assert_eq!(top_of_page().y, px(0.));
    }

    #[test]
    fn a_partial_save_says_what_was_written() {
        assert_eq!(partial_save_message("disk full", false), "disk full");
        let partial = partial_save_message("disk full", true);
        assert!(partial.contains("Service settings were saved"));
        assert!(partial.ends_with("disk full"));
    }

    #[test]
    fn confirm_quit_defaults_to_off() {
        assert!(!confirm_quit_enabled(None));
        assert!(confirm_quit_enabled(Some("true")));
    }

    #[test]
    fn developer_mode_defaults_to_on() {
        assert!(super::developer_mode_enabled(None));
        assert!(super::developer_mode_enabled(Some("1")));
        assert!(!super::developer_mode_enabled(Some("0")));
    }
}
