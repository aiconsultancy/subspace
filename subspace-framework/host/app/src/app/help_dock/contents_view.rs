//! The Help panel's contents list, document sections and Back to contents.
//!
//! The scroll container's first child is the header with the contents. Each
//! document section is the next child, so a jump asks layout to put child
//! `section + 1` at the top. The request is an index, not a pixel offset, and
//! every render repeats it until the user scrolls, so images or wrapping that
//! change the height above the section cannot move it.

use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Point, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, div,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{Intent, action_button};
use subspace_dashboard::icon_button::IconButton;
use subspace_dashboard::rich::{Format, rich_text};
use subspace_lucide::IconName;

use super::HelpPanel;
use crate::app::help_dock::contents::{Heading, HeadingLevel, HelpSections};

/// The header is scroll item 0; section `n` is item `n + 1`.
const FIRST_SECTION_ITEM: usize = 1;

impl HelpPanel {
    /// Rebuilds the sections and contents for the current board and returns
    /// to the top of the document.
    pub(super) fn load_document(&mut self, cx: &mut Context<Self>) {
        self.document = HelpSections::parse(&self.content.document());
        self.section_focus = self
            .document
            .sections()
            .iter()
            .map(|_| cx.focus_handle())
            .collect();
        self.contents_open = self.document.shows_contents();
        self.jump = None;
        self.scroll.set_offset(Point::default());
    }

    /// The help below the filter: a fixed Back to contents row whenever the
    /// document has contents, so the scroll area never changes height while
    /// reading, then the scrolling header and contents, one item per
    /// document section, and the shortcuts. The row is outside the scroll
    /// region, so it never covers help text.
    pub(super) fn scroll_frame(&self, tokens: &Tokens, query: &str, cx: &mut Context<Self>) -> Div {
        let show_document = query.trim().is_empty();
        self.reapply_jump(show_document);
        v_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .when(show_document, |frame| {
                frame.children(self.back_to_contents(cx))
            })
            .child(
                v_flex()
                    .id("help-dock-scroll")
                    .test_support()
                    .track_scroll(&self.scroll)
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .p_4()
                    .gap_4()
                    .on_scroll(cx.listener(|panel, _, _, cx| panel.user_scrolled(cx)))
                    .child(self.header(tokens, show_document, cx))
                    .when(show_document, |body| body.children(self.sections(tokens)))
                    .child(self.footer(tokens, query)),
            )
    }

    /// Repeats the last jump for this layout while it is pinned.
    fn reapply_jump(&self, show_document: bool) {
        if let Some(section) = self.jump.filter(|_| show_document) {
            self.scroll
                .scroll_to_top_of_item(section + FIRST_SECTION_ITEM);
        }
    }

    /// A wheel or trackpad scroll releases a pinned jump.
    pub(super) fn user_scrolled(&mut self, cx: &mut Context<Self>) {
        if self.jump.take().is_some() {
            cx.notify();
        }
    }

    /// Scrolls `section` to the top and moves focus to it, so the next Tab
    /// continues from the section the reader chose.
    pub(super) fn jump_to(&mut self, section: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.jump = Some(section);
        self.scroll
            .scroll_to_top_of_item(section + FIRST_SECTION_ITEM);
        if let Some(focus) = self.section_focus.get(section) {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    /// Returns to the top, opens the contents and focuses their toggle.
    fn show_contents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.jump = None;
        self.contents_open = true;
        self.scroll.set_offset(Point::default());
        self.contents_focus.focus(window, cx);
        cx.notify();
    }

    fn toggle_contents(&mut self, cx: &mut Context<Self>) {
        self.contents_open = !self.contents_open;
        cx.notify();
    }

    /// The collapsible contents, or nothing for a short document.
    pub(super) fn contents(&self, tokens: &Tokens, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.document.shows_contents().then(|| {
            let entries: Vec<_> = self
                .document
                .headings()
                .iter()
                .map(|heading| self.contents_entry(heading, tokens, cx))
                .collect();
            Collapsible::new()
                .open(self.contents_open)
                .child(self.contents_toggle(tokens, cx))
                .content(v_flex().id("help-contents-list").gap_1().children(entries))
                .into_any_element()
        })
    }

    fn contents_toggle(&self, tokens: &Tokens, cx: &mut Context<Self>) -> IconButton {
        let open = self.contents_open;
        IconButton::new("help-contents-toggle")
            .track_focus(&self.contents_focus)
            .focus_on_click()
            .justify_start()
            .text_color(tokens.muted)
            .rounded(tokens.control_radius)
            .icon(if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .label("Contents")
            .accessibility_label("Contents")
            .aria_expanded(open)
            .on_click(cx.listener(|panel, _, _, cx| panel.toggle_contents(cx)))
    }

    fn contents_entry(
        &self,
        heading: &Heading,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> gpui_kit::base::Button {
        let section = heading.section;
        let board = self.content.board_id();
        let top_level = heading.level == HeadingLevel::One;
        gpui_kit::base::Button::new(SharedString::from(format!(
            "help-contents-{board}-{section}"
        )))
        .w_full()
        .justify_start()
        .p_1()
        .when(!top_level, |entry| entry.pl(tokens.indent))
        .when(top_level, |entry| entry.font_weight(FontWeight::MEDIUM))
        .rounded(tokens.control_radius)
        .hover(|style| style.bg(tokens.select))
        .focus_visible(|style| style.bg(tokens.select))
        .accessibility_label(format!("Go to {}", heading.title))
        .child(
            div()
                .min_w_0()
                .whitespace_normal()
                .child(heading.title.clone()),
        )
        .on_click(cx.listener(move |panel, _, window, cx| panel.jump_to(section, window, cx)))
    }

    /// One scroll item per document section, in document order.
    pub(super) fn sections(&self, tokens: &Tokens) -> Vec<AnyElement> {
        self.document
            .sections()
            .iter()
            .zip(&self.section_focus)
            .enumerate()
            .map(|(index, (markdown, focus))| self.section(index, markdown, focus, tokens))
            .collect()
    }

    fn section(
        &self,
        index: usize,
        markdown: &str,
        focus: &FocusHandle,
        tokens: &Tokens,
    ) -> AnyElement {
        let board = self.content.board_id();
        let title = self
            .document
            .heading_for(index)
            .map(|heading| heading.title.clone());
        div()
            .id(SharedString::from(format!(
                "help-dock-section-{board}-{index}"
            )))
            .test_support()
            .track_focus(focus)
            .rounded(tokens.control_radius)
            .focus_visible(|style| style.bg(tokens.select))
            .when_some(title, |section, title| {
                section.role(Role::Group).aria_label(title)
            })
            .child(rich_text(
                SharedString::from(format!("help-dock-document-{board}-{index}")),
                Format::Markdown,
                markdown,
            ))
            .into_any_element()
    }

    /// A slim fixed row with Back to contents for a document that has
    /// contents. At the top it still reopens and focuses the contents.
    fn back_to_contents(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.document.shows_contents().then(|| {
            h_flex()
                .id("help-dock-back-row")
                .test_support()
                .px_3()
                .pb_2()
                .child(
                    action_button(
                        "help-dock-back-to-contents",
                        Intent::Neutral,
                        Some(IconName::ArrowUp),
                        "Contents",
                    )
                    .accessibility_label("Back to contents")
                    .on_click(cx.listener(|panel, _, window, cx| panel.show_contents(window, cx))),
                )
                .into_any_element()
        })
    }
}
