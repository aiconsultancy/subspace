// qual:allow(srp, file_length=530) reason: "The panel facade owns one terminal panel lifecycle and delegates reload, tab, header and close policy logic to focused modules."

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
};
use subspace_framework::dashboard::action::{self, Intent};
use subspace_framework::dashboard::dialog::{self, DialogWidth};
use subspace_framework::dashboard::dock_panel::{
    DockPanel, DockPanelEvent, DockPanelHandle, DockPanelRegistration, PanelLayout,
};
use subspace_framework::dashboard::tabs::{self as dashboard_tabs, TabSpec};
use subspace_framework::dashboard::{HelpImage, MODAL_CONTEXT, Services, Tokens};
use subspace_framework::lucide::IconName;
use subspace_ghostty_config::{ConfirmCloseSurface, GhosttyConfig};
use subspace_terminal::{
    TerminalStyle, TerminalView,
    style::{FontChoice, ProfileFont},
    tabs as tab_model,
};

use crate::font_settings;

use crate::keys::{
    CancelClose, CloseTab, ConfirmClose, KEYS, NewTab, NextTab, PreviousTab, Restart, Retry,
};

/// The panel's key context.
pub(crate) const PANEL_CONTEXT: &str = "TerminalPanel";

const HELP_IMAGES: &[HelpImage] = &[HelpImage::new(
    "tab",
    "Local Terminal tab in an isolated shell with no service credentials",
    "image/webp",
    include_bytes!("../help-assets/tab.webp"),
)];

/// What the product passes the shell to dock the terminal.
pub fn registration() -> DockPanelRegistration {
    DockPanelRegistration {
        create,
        bind_keys,
        help_markdown: include_str!("../help.md"),
        help_images: HELP_IMAGES,
        keys: KEYS,
        settings_editor: Some(font_settings::editor::factory),
    }
}

fn create(_window: &mut Window, cx: &mut App) -> DockPanelHandle {
    DockPanelHandle::create(cx, TerminalPanel::new)
}

fn bind_keys(cx: &mut App) {
    let context = format!("{PANEL_CONTEXT} && !{MODAL_CONTEXT}");
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        gpui_kit::KeyBinding::new("cmd-t", NewTab, Some(context.as_str())),
        gpui_kit::KeyBinding::new("cmd-w", CloseTab, Some(context.as_str())),
        gpui_kit::KeyBinding::new("cmd->", NextTab, Some(context.as_str())),
        gpui_kit::KeyBinding::new("cmd-<", PreviousTab, Some(context.as_str())),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        gpui_kit::KeyBinding::new("ctrl-shift-t", NewTab, Some(context.as_str())),
        gpui_kit::KeyBinding::new("ctrl-shift-w", CloseTab, Some(context.as_str())),
        gpui_kit::KeyBinding::new("ctrl-pagedown", NextTab, Some(context.as_str())),
        gpui_kit::KeyBinding::new("ctrl-pageup", PreviousTab, Some(context.as_str())),
    ]);
    subspace_terminal::bind_keys(cx);
}
use crate::reload;
use crate::status::should_confirm;
use subspace_terminal::tabs::TabOrder;

const MAX_TAB_TITLE: usize = 28;
pub(crate) struct SessionTab {
    pub(crate) id: u64,
    pub(crate) view: Option<Entity<TerminalView>>,
    pub(crate) error: Option<String>,
}

pub struct TerminalPanel {
    focus: FocusHandle,
    pub(crate) tabs: Vec<SessionTab>,
    pub(crate) order: TabOrder,
    pub(crate) next_id: u64,
    pub(crate) style: Option<TerminalStyle>,
    pub(crate) config: GhosttyConfig,
    pub(crate) warnings: Vec<String>,
    pub(crate) close_policy: Option<ConfirmCloseSurface>,
    pub(crate) pending_close: Option<u64>,
    pub(crate) themes: [subspace_ghostty_config::ThemeValues; 2],
    pub(crate) config_warnings: Vec<String>,
    pub(crate) theme_warnings: [Vec<String>; 2],
    pub(crate) tokens: Option<Tokens>,
    code_family: SharedString,
    font: FontChoice,
    profile_font: ProfileFont,
    pub(crate) layout: PanelLayout,
    _settings: Subscription,
}

impl EventEmitter<DockPanelEvent> for TerminalPanel {}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_appearance(cx);
        let tokens = Tokens::current(cx);
        let content = self.render_content(&tokens, window, cx);
        let tabs = self.render_header(cx);
        // A key context is a name; predicates such as `!SubspaceModal` belong
        // in the bindings (see `bind_keys`).
        let root = v_flex()
            .id("terminal-panel")
            .relative()
            .size_full()
            .key_context(PANEL_CONTEXT)
            .track_focus(&self.focus)
            .child(tabs)
            .child(content);
        let root = root
            .on_action(cx.listener(|this, _: &NewTab, window, cx| this.open_tab(window, cx)))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| this.request_close(window, cx)))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.order.next();
                this.focus_selected(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| {
                this.order.previous();
                this.focus_selected(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &Restart, window, cx| this.restart_selected(window, cx)),
            )
            .on_action(cx.listener(|this, _: &Retry, window, cx| this.retry_selected(window, cx)))
            .on_action(cx.listener(|this, _: &ConfirmClose, window, cx| {
                if let Some(id) = this.pending_close {
                    this.close(id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &CancelClose, _, cx| {
                this.pending_close = None;
                cx.notify();
            }));
        match self.pending_close {
            Some(id) => root.child(close_dialog(&tokens, id, cx)),
            None => root,
        }
    }
}

impl DockPanel for TerminalPanel {
    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            self.load_config_and_style(cx);
            self.open_tab(window, cx);
        } else {
            self.focus_selected(window, cx);
        }
    }

    fn set_layout(&mut self, layout: PanelLayout, cx: &mut Context<Self>) {
        self.layout = layout;
        cx.notify();
    }

    fn status(&self, _cx: &App) -> Option<SharedString> {
        (!self.warnings.is_empty()).then(|| {
            self.warnings
                .iter()
                .map(|warning| format!("⚠ {warning}"))
                .collect::<Vec<_>>()
                .join(" · ")
                .into()
        })
    }

    fn quit_warnings(&self, cx: &App) -> Vec<SharedString> {
        self.tabs
            .iter()
            .filter_map(|tab| tab.view.as_ref())
            .filter_map(|view| {
                view.read_with(cx, |view: &TerminalView, _| {
                    view.session()
                        .busy_program()
                        .map(|program| program.to_string().into())
                })
            })
            .collect()
    }

    fn reload(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reload_config(cx);
    }
}

impl TerminalPanel {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            tabs: Vec::new(),
            order: TabOrder::new(),
            next_id: 1,
            style: None,
            config: GhosttyConfig::default(),
            warnings: Vec::new(),
            close_policy: None,
            pending_close: None,
            themes: Default::default(),
            config_warnings: Vec::new(),
            theme_warnings: Default::default(),
            tokens: None,
            code_family: SharedString::default(),
            font: font_settings::saved(cx),
            profile_font: ProfileFont::default(),
            layout: PanelLayout::default(),
            _settings: cx.observe_global::<Services>(Self::follow_settings),
        }
    }

    /// Load configuration and apply the resolved values to this board.
    pub(crate) fn load_config_and_style(&mut self, cx: &mut Context<Self>) {
        self.reload_config(cx);
    }

    fn reload_config(&mut self, cx: &mut Context<Self>) {
        let loaded = reload::load_config();
        self.close_policy = loaded.config.confirm_close_surface;
        self.config = loaded.config;
        self.profile_font = loaded.profile_font;
        self.themes = loaded.themes;
        self.config_warnings = loaded.config_warnings;
        self.theme_warnings = loaded.theme_warnings;
        self.resolve_style(cx);
    }

    fn follow_settings(&mut self, cx: &mut Context<Self>) {
        let font = font_settings::saved(cx);
        if self.font != font {
            self.font = font;
            if self.style.is_some() {
                self.resolve_style(cx);
            }
        }
    }

    fn refresh_appearance(&mut self, cx: &mut Context<Self>) {
        let tokens = Tokens::current(cx);
        if self.style.is_some()
            && (self.code_family != cx.theme().mono_font_family
                || reload::tokens_changed(self.tokens.as_ref(), &tokens))
        {
            self.resolve_style(cx);
        }
    }

    fn resolve_style(&mut self, cx: &mut Context<Self>) {
        let tokens = Tokens::current(cx);
        let appearance = reload::current_appearance(&tokens);
        let theme = reload::appearance_theme(&self.themes, appearance);
        let (style, warnings) = TerminalStyle::resolve_with_fonts(
            &self.config,
            theme,
            &self.font,
            &self.profile_font,
            cx,
        );
        self.apply_style(&style, cx);
        self.style = Some(style);
        self.tokens = Some(tokens);
        self.code_family = cx.theme().mono_font_family.clone();
        self.warnings = reload::replace_warnings(
            &self.warnings,
            reload::warnings_for(
                &self.config_warnings,
                &self.theme_warnings[reload::appearance_index(appearance)],
                warnings,
            ),
        );
        cx.notify();
    }

    fn apply_style(&self, style: &TerminalStyle, cx: &mut Context<Self>) {
        for tab in &self.tabs {
            if let Some(view) = &tab.view {
                let style = style.clone();
                view.update(cx, |view, cx| view.apply_config(style, cx));
            }
        }
    }

    fn open_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        let (view, error) = match self.start_view(cx) {
            Ok(view) => (Some(view), None),
            Err(error) => (None, Some(error)),
        };
        self.tabs.push(SessionTab { id, view, error });
        self.order.insert(id);
        self.focus_selected(window, cx);
    }

    fn selected_tab(&self) -> Option<&SessionTab> {
        let id = self.order.selected()?;
        self.tabs.iter().find(|tab| tab.id == id)
    }
    fn selected_id(&self) -> Option<u64> {
        self.order.selected()
    }

    pub(crate) fn render_tabs(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let items = self.tabs.iter().map(|tab| {
            let (label, icon) = tab.view.as_ref().map_or_else(
                || ("Failed".to_owned(), IconName::TriangleAlert),
                |view| {
                    view.read_with(cx, |view: &TerminalView, _| {
                        (
                            tab_model::title(view.title().as_deref(), view.session().program()),
                            IconName::Terminal,
                        )
                    })
                },
            );
            dashboard_tabs::tab(TabSpec::new(
                tab_model::truncate(&label, MAX_TAB_TITLE),
                icon,
            ))
            .aria_label(format!("Select terminal tab {label}"))
        });
        let actions = h_flex()
            .gap_1()
            .child(
                action::action_button(
                    "terminal-close-tab-pointer",
                    Intent::Quiet,
                    Some(IconName::X),
                    "Close",
                )
                .tooltip(close_tab_tooltip())
                .accessibility_label("Close the selected terminal tab")
                .on_click(cx.listener(|this, _, window, cx| this.request_close(window, cx))),
            )
            .child(
                action::action_button(
                    "terminal-new-tab",
                    Intent::Quiet,
                    Some(IconName::Plus),
                    "New",
                )
                .tooltip(new_tab_tooltip())
                .accessibility_label("Open a new terminal tab")
                .on_click(cx.listener(|this, _, window, cx| this.open_tab(window, cx))),
            );
        dashboard_tabs::tab_bar("terminal-tabs")
            .selected_index(
                self.order
                    .ids()
                    .iter()
                    .position(|id| Some(*id) == self.selected_id())
                    .unwrap_or(0),
            )
            .children(items)
            .suffix(actions.child(self.layout_buttons(cx)))
            .on_click(cx.listener(|this, index: &usize, window, cx| {
                this.order.select(*index);
                this.focus_selected(window, cx);
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_content(
        &self,
        tokens: &Tokens,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        match self.selected_tab() {
            Some(SessionTab {
                view: Some(view), ..
            }) => crate::view::running(
                view,
                view.read_with(cx, |view: &TerminalView, _| view.exit().is_some()),
            ),
            Some(SessionTab {
                view: None,
                error: Some(error),
                ..
            }) => crate::view::startup_error(error, tokens),
            Some(_) => div().size_full(),
            None => crate::view::no_session(tokens),
        }
        .bg(tokens.bg)
        .into_any_element()
    }

    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_id() else {
            return;
        };
        let confirm = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .and_then(|tab| tab.view.as_ref())
            .is_some_and(|view| {
                view.read_with(cx, |view: &TerminalView, _| {
                    should_confirm(
                        self.close_policy,
                        view.session()
                            .busy_program()
                            .as_ref()
                            .map(|program| program.as_str()),
                        view.session().program().as_str(),
                    )
                })
            });
        if confirm {
            self.pending_close = Some(id);
            cx.notify();
        } else {
            self.close(id, window, cx);
        }
    }

    fn close(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.retain(|tab| tab.id != id);
        self.order.remove(id);
        self.pending_close = None;
        self.focus_selected(window, cx);
        cx.notify();
    }
    fn restart_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_id() {
            self.replace_tab(id, window, cx);
        }
    }
    fn retry_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_id() {
            self.replace_tab(id, window, cx);
        }
    }
    fn replace_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.start_view(cx);
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            match result {
                Ok(view) => {
                    tab.view = Some(view);
                    tab.error = None;
                }
                Err(error) => {
                    tab.view = None;
                    tab.error = Some(error);
                }
            }
            self.focus_selected(window, cx);
            cx.notify();
        }
    }

    fn start_view(&self, cx: &mut Context<Self>) -> Result<Entity<TerminalView>, String> {
        let style = self
            .style
            .clone()
            .ok_or_else(|| "Terminal configuration is not loaded".to_owned())?;
        TerminalView::open(&reload::session_options(&self.config), style, cx)
            .map_err(|error| error.to_string())
    }

    fn focus_selected(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.selected_tab().and_then(|tab| tab.view.as_ref()) else {
            self.focus.focus(window, cx);
            return;
        };
        view.update(cx, |view, cx| view.focus_handle(cx).focus(window, cx));
    }
}

fn close_dialog(
    tokens: &Tokens,
    _id: u64,
    cx: &mut Context<TerminalPanel>,
) -> gpui_kit::AnyElement {
    let frame = dialog::frame(tokens, DialogWidth::D540, "Close terminal tab")
        .child("A foreground program is still running.")
        .child(
            dialog::footer()
                .child(
                    action::action_button(
                        "terminal-cancel-close",
                        Intent::Neutral,
                        Some(IconName::X),
                        "Cancel",
                    )
                    .accessibility_label("Keep the terminal tab open")
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.pending_close = None;
                        cx.notify();
                    })),
                )
                .child(
                    action::action_button(
                        "terminal-confirm-close",
                        Intent::Reject,
                        Some(IconName::Trash),
                        "Close",
                    )
                    .accessibility_label("Close the running terminal tab")
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(id) = this.pending_close {
                            this.close(id, window, cx);
                        }
                    })),
                ),
        );
    dialog::backdrop(tokens, frame)
}

#[cfg(target_os = "macos")]
fn new_tab_tooltip() -> &'static str {
    "New terminal tab (Cmd+T)"
}

#[cfg(not(target_os = "macos"))]
fn new_tab_tooltip() -> &'static str {
    "New terminal tab (Ctrl+Shift+T)"
}

#[cfg(target_os = "macos")]
fn close_tab_tooltip() -> &'static str {
    "Close terminal tab (Cmd+W)"
}

#[cfg(not(target_os = "macos"))]
fn close_tab_tooltip() -> &'static str {
    "Close terminal tab (Ctrl+Shift+W)"
}

#[cfg(test)]
mod registration_tests {
    use super::{HELP_IMAGES, registration};
    use crate::keys::KEYS;

    #[test]
    fn the_registration_carries_the_guide_keys_and_font_settings() {
        let registration = registration();
        assert!(registration.help_markdown.starts_with("# Terminal guide"));
        assert_eq!(registration.help_images.len(), HELP_IMAGES.len());
        assert_eq!(registration.help_images[0].id, "tab");
        assert_eq!(registration.help_images[0].media_type, "image/webp");
        assert_eq!(registration.keys.len(), KEYS.len());
        assert!(registration.settings_editor.is_some());
    }
}
