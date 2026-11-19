//! Bundled board documentation and shortcuts, supplied as a shell snapshot.

use gpui_kit::component::dock::{BasePanel, Panel, PanelControl, PanelEvent};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Div, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, div,
};
use subspace_dashboard::action::{Intent, action_button};
use subspace_dashboard::{HelpImage, Tokens};
use subspace_lucide::IconName;

use super::super::SubspaceApp;
use super::super::{CloseHelp, keys::HELP_CONTEXT};
use super::contents::HelpSections;
use super::shortcuts::{FocusShortcutFilter, matching_keys};
use super::state::Side;

/// Read while the shell is already borrowed, never from Dock metadata hooks.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct HelpContent {
    board: Option<BoardHelp>,
    keys: Vec<(SharedString, SharedString)>,
    foot: Option<SharedString>,
}

#[derive(Clone, PartialEq)]
struct BoardHelp {
    id: &'static str,
    title: SharedString,
    markdown: &'static str,
    images: &'static [HelpImage],
}

impl HelpContent {
    pub(crate) fn from_app(app: &SubspaceApp, cx: &App) -> Self {
        // The terminal's guide while Help shows it, else the Space's.
        let terminal = app
            .terminal_help(cx)
            .map(|(markdown, images)| ("terminal", "Terminal".into(), markdown, images));
        let foot = terminal.is_none().then(|| app.dashboard()).flatten();
        let board = terminal.or_else(|| {
            app.dashboard()
                .map(|board| (board.id, board.title.clone(), board.help, board.help_images))
        });
        Self {
            board: board.map(|(id, title, markdown, images)| BoardHelp {
                id,
                title,
                markdown,
                images,
            }),
            keys: app.help_keys(cx),
            foot: foot.and_then(|board| board.help_foot(cx)),
        }
    }

    fn document(&self) -> String {
        match &self.board {
            Some(board) if !board.markdown.trim().is_empty() => {
                subspace_dashboard::help::prepare_markdown(board.markdown, board.images)
            }
            Some(board) => super::super::board_help::missing_help(Some(&board.title)),
            None => super::super::board_help::missing_help(None),
        }
    }

    fn board_id(&self) -> &'static str {
        self.board.as_ref().map_or("none", |board| board.id)
    }
}

pub(super) enum HelpPanelEvent {
    Close,
    Move(Side),
}

pub(super) struct HelpPanel {
    focus: FocusHandle,
    content: HelpContent,
    document: HelpSections,
    /// One focus target per document section, for keyboard jumps.
    section_focus: Vec<FocusHandle>,
    contents_focus: FocusHandle,
    contents_open: bool,
    scroll: ScrollHandle,
    /// The section a contents entry scrolled to the top. Layout re-applies it
    /// until the user scrolls, so images that load above it cannot move it.
    jump: Option<usize>,
    side: Side,
    protected: bool,
    query: Entity<InputState>,
    _filter_subscription: Subscription,
}

impl HelpPanel {
    pub(super) fn new(
        side: Side,
        protected: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Filter shortcuts"));
        let subscription = cx.subscribe(&query, |panel, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                panel.jump = None;
                cx.notify();
            }
        });
        let mut panel = Self {
            focus: cx.focus_handle().tab_stop(true),
            content: HelpContent::default(),
            document: HelpSections::default(),
            section_focus: Vec::new(),
            contents_focus: cx.focus_handle().tab_stop(true),
            contents_open: false,
            scroll: ScrollHandle::new(),
            jump: None,
            side,
            protected,
            query,
            _filter_subscription: subscription,
        };
        panel.load_document(cx);
        panel
    }

    /// A new board document rebuilds the contents and scrolls to the top. A
    /// change to only the shortcuts or footer keeps the reading position.
    pub(super) fn set_content(&mut self, content: HelpContent, cx: &mut Context<Self>) {
        if self.content == content {
            return;
        }
        let new_document = self.content.board != content.board;
        self.content = content;
        if new_document {
            self.load_document(cx);
        }
        cx.notify();
    }

    pub(super) fn set_side(&mut self, side: Side, cx: &mut Context<Self>) {
        self.side = side;
        cx.notify();
    }

    fn shortcut_rows(&self, tokens: &Tokens, query: &str) -> Vec<gpui_kit::AnyElement> {
        matching_keys(&self.content.keys, query)
            .into_iter()
            .map(|(key, meaning)| {
                v_flex()
                    .id(SharedString::from(format!("help-key-{key}-{meaning}")))
                    .test_support()
                    .aria_label(meaning.clone())
                    .gap_1()
                    .py_2()
                    .border_b_1()
                    .border_color(tokens.hair)
                    .child(
                        div()
                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                            .child(key.clone()),
                    )
                    .child(div().whitespace_normal().child(meaning.clone()))
                    .into_any_element()
            })
            .collect()
    }

    /// The first scroll item: board title, layout warning and contents.
    fn header(&self, tokens: &Tokens, show_document: bool, cx: &mut Context<Self>) -> Div {
        let title = self
            .content
            .board
            .as_ref()
            .map_or("Help", |board| board.title.as_ref());
        v_flex()
            .gap_4()
            .child(div().id("help-dock-board-title").test_support().aria_label(title.to_owned()).font_weight(gpui_kit::FontWeight::MEDIUM).child(title.to_owned()))
            .when(self.protected, |body| {
                body.child(div().text_color(tokens.muted).whitespace_normal().child(
                    "Saved Help layout is invalid or unsupported. It has been kept unchanged; layout changes apply only to this session.",
                ))
            })
            .when(show_document, |body| body.children(self.contents(tokens, cx)))
    }

    /// The last scroll item: shortcuts, the board footer and app-wide About help.
    fn footer(&self, tokens: &Tokens, query: &str) -> Div {
        let rows = self.shortcut_rows(tokens, query);
        let empty = rows.is_empty();
        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                            .child("Keyboard shortcuts"),
                    )
                    .children(rows)
                    .when(empty, |rows| {
                        rows.child(
                            div()
                                .id("help-shortcuts-empty")
                                .test_support()
                                .child("No shortcuts match."),
                        )
                    }),
            )
            .when_some(self.content.foot.clone(), |body, foot| {
                body.child(
                    div()
                        .text_color(tokens.muted)
                        .whitespace_normal()
                        .child(foot),
                )
            })
            .when(query.trim().is_empty(), |body| {
                body.child(crate::app::about_dialog::help())
            })
    }
}

impl BasePanel for HelpPanel {
    fn panel_name(&self) -> &'static str {
        "app.help"
    }

    // Kit refuses to remove a side dock's last panel. Close collapses it instead.
    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Panel for HelpPanel {
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some("Help".into())
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_flex().gap_1().child(IconName::BookOpen).child("Help")
    }

    fn title_suffix(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        Some(
            // Kit's single-panel dock title is fixed at 30px; this suffix must fit it.
            action_button("help-dock-close", Intent::Quiet, None, "Close")
                .icon(IconName::X)
                .compact()
                .on_click(cx.listener(|_, _, _, cx| cx.emit(HelpPanelEvent::Close))),
        )
    }

    fn dropdown_menu(
        &mut self,
        menu: PopupMenu,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> PopupMenu {
        let owner = cx.weak_entity();
        [Side::Left, Side::Right]
            .into_iter()
            .fold(menu, |menu, side| {
                let owner = owner.clone();
                let (label, icon) = match side {
                    Side::Left => ("Move Help left", IconName::ArrowLeft),
                    Side::Right => ("Move Help right", IconName::ArrowRight),
                };
                menu.item(
                    PopupMenuItem::new(label)
                        .icon(icon)
                        .disabled(side == self.side)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |_, cx| cx.emit(HelpPanelEvent::Move(side)));
                        }),
                )
            })
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl EventEmitter<PanelEvent> for HelpPanel {}
impl EventEmitter<HelpPanelEvent> for HelpPanel {}

impl Focusable for HelpPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for HelpPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let query = self.query.read(cx).value();
        let content = v_flex()
            .id("help-dock")
            .test_support()
            .track_focus(&self.focus)
            .key_context(HELP_CONTEXT)
            .on_action(cx.listener(|_, _: &CloseHelp, _, cx| cx.emit(HelpPanelEvent::Close)))
            .on_action(cx.listener(|panel, _: &FocusShortcutFilter, window, cx| {
                panel.query.update(cx, |query, cx| query.focus(window, cx));
            }))
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(tokens.surface)
            .text_color(tokens.fg)
            .child(
                div().p_3().child(
                    Input::new(&self.query)
                        .id("help-shortcuts-filter")
                        .aria_label("Filter shortcuts"),
                ),
            )
            .child(subspace_dashboard::find_in_page::scroll(
                self.scroll.clone(),
                self.scroll_frame(&tokens, &query, cx),
            ));
        subspace_dashboard::find_in_page::surface("help-find", content, window, cx)
    }
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod image_tests;

#[cfg(any(test, feature = "test-support"))]
#[path = "test_support.rs"]
pub(crate) mod test_support;

#[path = "contents_view.rs"]
mod contents_view;

#[cfg(test)]
#[path = "contents_runtime_tests.rs"]
mod contents_runtime_tests;

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;

    use super::{BoardHelp, HelpContent, HelpPanel};
    use crate::app::help_dock::state::Side;

    #[gpui_kit::test]
    fn changing_boards_replaces_the_document_shortcuts_and_footer(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (panel, _) =
            cx.add_window_view(|window, cx| HelpPanel::new(Side::Right, false, window, cx));
        let first = HelpContent {
            board: Some(BoardHelp {
                id: "first",
                title: "First board".into(),
                markdown: "# First guide",
                images: &[],
            }),
            keys: vec![("a".into(), "First action".into())],
            foot: Some("First footer".into()),
        };
        panel.update(cx, |panel, cx| panel.set_content(first, cx));
        assert!(panel.read_with(cx, |panel, _| {
            panel.document.sections().concat().contains("First guide")
        }));
        let second = HelpContent {
            board: Some(BoardHelp {
                id: "second",
                title: "Second board".into(),
                markdown: "# Second guide",
                images: &[],
            }),
            keys: vec![("b".into(), "Second action".into())],
            foot: None,
        };
        panel.update(cx, |panel, cx| panel.set_content(second, cx));
        panel.read_with(cx, |panel, _| {
            let document = panel.document.sections().concat();
            assert!(document.contains("Second guide"));
            assert!(!document.contains("First guide"));
            assert_eq!(panel.content.board_id(), "second");
            assert_eq!(panel.content.keys[0].0.as_ref(), "b");
            assert!(panel.content.foot.is_none());
        });
    }
}
