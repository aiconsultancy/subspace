//! The navigator: the leading dock where a Space lists what there is to look
//! at, such as views, projects, networks or repositories.

use std::rc::Rc;

use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, Role, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled, TestSupportExt as _, Window, div,
};
use subspace_lucide::SvgIcon;

use super::row_parts::{ClickHandler, MenuBuilder, count_mark, fill, with_menu};
use super::{header, text_edge};
use crate::row_menu::GroupedMenu;
use crate::tokens::Tokens;
use crate::truncated;
use crate::typography::heading_font;

/// The leading dock: a header naming the Space, a scrolling body of sections
/// and rows, and an optional footer. Add the body with `child` or `children`.
#[derive(IntoElement)]
pub struct Navigator {
    title: SharedString,
    badge: Option<SharedString>,
    actions: Vec<AnyElement>,
    children: Vec<AnyElement>,
    footer: Option<AnyElement>,
}

impl Navigator {
    /// `title` is the Space's name.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            badge: None,
            actions: Vec::new(),
            children: Vec::new(),
            footer: None,
        }
    }

    /// A quiet word after the title, such as "Alpha".
    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// A header control after the title, in call order.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    /// A line pinned under the body, such as the signed-in account.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }
}

impl ParentElement for Navigator {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Navigator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        // The Space's name takes the accent, so it reads as where you are.
        let title = header::title(
            "workspace-navigator-title",
            self.title,
            FontWeight::BOLD,
            heading_font(cx),
            &tokens,
        )
        .text_color(tokens.accent)
        .test_support();
        let head = header::strip("workspace-navigator-header", tokens.sidebar_line, &tokens)
            .child(title)
            .children(self.badge.map(|badge| badge_mark(badge, &tokens)))
            .children(self.actions);
        v_flex()
            .id("workspace-navigator")
            .test_support()
            .size_full()
            .min_w_0()
            .bg(tokens.sidebar)
            .border_r_1()
            .border_color(tokens.sidebar_line)
            .child(head)
            .child(
                v_flex()
                    .id("workspace-navigator-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .py_1()
                    .children(self.children),
            )
            .children(self.footer.map(|footer| foot(footer, &tokens)))
    }
}

fn badge_mark(badge: SharedString, tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .child(badge)
}

fn foot(footer: AnyElement, tokens: &Tokens) -> Stateful<Div> {
    h_flex()
        .id("workspace-navigator-footer")
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .gap_2()
        .px(text_edge(tokens))
        .py_2()
        .border_t_1()
        .border_color(tokens.sidebar_line)
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .child(footer)
}

/// A section label above a run of navigator rows, such as "Views". It shows
/// in capitals; its accessible name and id keep the text as written.
pub fn nav_section(label: impl Into<SharedString>, cx: &App) -> Stateful<Div> {
    let tokens = Tokens::current(cx);
    let label = label.into();
    div()
        .id(SharedString::from(format!("nav-section:{label}")))
        .role(Role::Heading)
        .aria_label(label.clone())
        .w_full()
        .flex_shrink_0()
        .px(text_edge(&tokens))
        .pt_3()
        .pb_1()
        .text_size(tokens.caption_size())
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tokens.muted)
        .child(header::capitals(&label))
}

/// One navigator destination: a glyph, a label, and a count or a mark at the
/// trailing edge. The selected row shows through its own fill.
#[derive(IntoElement)]
pub struct NavRow {
    key: SharedString,
    label: SharedString,
    icon: Option<SvgIcon>,
    leading: Option<AnyElement>,
    count: Option<SharedString>,
    trailing: Option<AnyElement>,
    selected: bool,
    quiet: bool,
    strong: bool,
    indent: u8,
    on_click: Option<ClickHandler>,
    menu: Option<MenuBuilder>,
}

// qual:allow(dry, boilerplate) reason: "A controlled row's facts are independent options; a builder keeps call sites readable."
impl NavRow {
    /// `key` is the row's element id and must be unique in the navigator.
    pub fn new(key: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            icon: None,
            leading: None,
            count: None,
            trailing: None,
            selected: false,
            quiet: false,
            strong: false,
            indent: 0,
            on_click: None,
            menu: None,
        }
    }

    /// A decorative Lucide glyph before the label.
    pub fn icon(mut self, icon: SvgIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// An element before the label instead of a glyph, such as a status dot.
    pub fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    /// Quiet text at the trailing edge, such as "12" or "Connected".
    pub fn count(mut self, count: impl Into<SharedString>) -> Self {
        self.count = Some(count.into());
        self
    }

    /// An element at the trailing edge, after any count, such as an unread
    /// mark.
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Muted text, for a destination with nothing in it.
    pub fn quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Stronger text, for a parent row such as a network.
    pub fn strong(mut self, strong: bool) -> Self {
        self.strong = strong;
        self
    }

    /// Levels of indentation under a parent row.
    pub fn indent(mut self, indent: u8) -> Self {
        self.indent = indent;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// A right-click menu built from the standard groups, which `commands`
    /// fills as the menu opens. A right-click first does what a click does,
    /// so the highlighted row is the one the menu acts on. Without it, the
    /// row has no menu.
    pub fn menu(
        mut self,
        commands: impl Fn(&mut Window, &mut App) -> GroupedMenu + 'static,
    ) -> Self {
        self.menu = Some(Rc::new(commands));
        self
    }
}

impl RenderOnce for NavRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let group = SharedString::from(format!("nav-row:{}", self.key));
        let title_id = SharedString::from(format!("nav-row-title:{}", self.key));
        let colour = if self.selected || self.strong {
            tokens.fg_strong
        } else if self.quiet {
            tokens.muted
        } else {
            tokens.fg
        };
        let edge = text_edge(&tokens);
        let mono = cx.theme().mono_font_family.clone();
        let leading = self.leading.or_else(|| {
            self.icon.map(|icon| {
                Icon::from(icon)
                    .xsmall()
                    .flex_shrink_0()
                    .text_color(tokens.muted)
                    .into_any_element()
            })
        });
        let row = h_flex()
            .id(ElementId::Name(self.key))
            .test_support()
            .group(group.clone())
            .role(Role::Button)
            .aria_label(self.label.clone())
            .aria_selected(self.selected)
            .relative()
            .w_full()
            .flex_shrink_0()
            .h(tokens.row_height)
            .gap_2()
            .pl(edge + tokens.indent * f32::from(self.indent))
            .pr(edge)
            .text_size(tokens.row_font)
            .text_color(colour)
            .when(self.strong, |row| row.font_weight(FontWeight::SEMIBOLD))
            .child(fill(group, self.selected, &tokens))
            .children(leading)
            .child(truncated::text(title_id, self.label).flex_1())
            .children(self.count.map(|count| count_mark(count, mono, &tokens)))
            .children(self.trailing)
            .when_some(self.on_click.clone(), |row, on_click| {
                row.on_click(move |event, window, cx| on_click(event, window, cx))
            });
        with_menu(row, self.menu, self.on_click)
    }
}
