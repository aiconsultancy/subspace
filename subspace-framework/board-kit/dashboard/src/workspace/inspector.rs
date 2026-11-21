//! The inspector: the trailing dock that shows the detail of what is
//! selected or open, with the actions for it in a footer.

use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, RenderOnce, Role, ScrollHandle, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};
use subspace_lucide::IconName;

use super::{header, text_edge};
use crate::action::{Intent, icon_button};
use crate::tokens::Tokens;
use crate::typography::heading_font;

/// The label column of a fact row, wide enough for "Created by".
const FACT_LABEL_WIDTH_PX: f32 = 84.;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The trailing dock: a header naming what it shows, a scrolling body added
/// with `child` or `children`, and an optional footer for the actions that
/// act on it.
#[derive(IntoElement)]
pub struct Inspector {
    id: ElementId,
    title: SharedString,
    actions: Vec<AnyElement>,
    close: Option<(SharedString, ClickHandler)>,
    children: Vec<AnyElement>,
    footer: Option<AnyElement>,
    scroll: Option<ScrollHandle>,
}

// qual:allow(dry, boilerplate) reason: "The dock's header controls, footer and scroll handle are independent options on a GPUI element; a fluent builder keeps call sites readable."
impl Inspector {
    /// `title` names what the dock shows, such as "Properties" or `SUB-12`.
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            actions: Vec::new(),
            close: None,
            children: Vec::new(),
            footer: None,
            scroll: None,
        }
    }

    /// A header control after the title, in call order, such as Open.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    /// Adds Close as the last header control. `label` names it, such as
    /// "Close inspector".
    pub fn on_close(
        mut self,
        label: impl Into<SharedString>,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.close = Some((label.into(), Rc::new(on_close)));
        self
    }

    /// The actions for what the dock shows, pinned under the body.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Lets the owner keep and restore the body's scroll position.
    pub fn track_scroll(mut self, scroll: &ScrollHandle) -> Self {
        self.scroll = Some(scroll.clone());
        self
    }
}

impl ParentElement for Inspector {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Inspector {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let title = header::title(
            "workspace-inspector-title",
            self.title,
            FontWeight::SEMIBOLD,
            heading_font(cx),
            &tokens,
        )
        .test_support();
        let close = self.close.map(|(label, on_close)| {
            icon_button(
                "workspace-inspector-close",
                Intent::Quiet,
                IconName::X,
                label,
            )
            .on_click(move |event, window, cx| on_close(event, window, cx))
        });
        let head = header::strip("workspace-inspector-header", tokens.line, &tokens)
            .child(title)
            .children(self.actions)
            .children(close);
        let body = v_flex()
            .id("workspace-inspector-body")
            .test_support()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .when_some(self.scroll.as_ref(), |body, scroll| {
                body.track_scroll(scroll)
            })
            .gap_6()
            .px(text_edge(&tokens))
            .py_4()
            .children(self.children);
        v_flex()
            .id(self.id)
            .test_support()
            .size_full()
            .min_w_0()
            .bg(tokens.surface)
            .border_l_1()
            .border_color(tokens.line)
            .child(head)
            .child(body)
            .children(self.footer.map(|footer| foot(footer, &tokens)))
    }
}

/// The bar under the body that holds the actions for what the dock shows.
/// Its buttons wrap in a narrow dock.
fn foot(footer: AnyElement, tokens: &Tokens) -> Stateful<Div> {
    h_flex()
        .id("workspace-inspector-footer")
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .flex_wrap()
        .gap_2()
        .px(text_edge(tokens))
        .py_3()
        .border_t_1()
        .border_color(tokens.line)
        .child(footer)
}

/// A titled group in the inspector's body, such as "Properties". Add its
/// rows with `child` or `children`. The title shows in capitals; its
/// accessible name and ids keep the text as written.
pub fn section(title: impl Into<SharedString>, cx: &App) -> Stateful<Div> {
    let tokens = Tokens::current(cx);
    let title = title.into();
    v_flex()
        .id(SharedString::from(format!("inspector-section:{title}")))
        .w_full()
        .min_w_0()
        .gap_2()
        .child(
            div()
                .id(SharedString::from(format!(
                    "inspector-section-title:{title}"
                )))
                .role(Role::Heading)
                .aria_label(title.clone())
                .text_size(tokens.caption_size())
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.muted)
                .child(header::capitals(&title)),
        )
}

/// One labelled fact in an inspector section: a quiet label in a fixed
/// column, then its value.
pub fn fact(label: impl Into<SharedString>, value: impl IntoElement, cx: &App) -> Stateful<Div> {
    let tokens = Tokens::current(cx);
    let label = label.into();
    h_flex()
        .id(SharedString::from(format!("inspector-fact:{label}")))
        .w_full()
        .min_w_0()
        .items_start()
        .gap_3()
        .text_size(tokens.secondary_size())
        .child(
            div()
                .flex_shrink_0()
                .w(px(FACT_LABEL_WIDTH_PX))
                .text_color(tokens.muted)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(tokens.fg_strong)
                .child(value),
        )
}
