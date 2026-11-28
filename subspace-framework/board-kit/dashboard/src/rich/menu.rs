//! One menu policy for literal values, rich blocks and legacy selectable hosts.

use std::rc::Rc;

use gpui_kit::base::TextSelection;
use gpui_kit::component::menu::{ContextMenu, PopupMenu, PopupMenuItem};
use gpui_kit::{
    AnyElement, App, Context, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled, Window,
};

use crate::anchored::AnchoredContextMenuExt as _;
use crate::row_menu::MenuChat;
use crate::{AddSelectionToChat, links, set_selection};

use super::plain::PlainState;
use super::view::RichTextContext;

#[derive(Clone)]
pub(super) struct Options {
    pub(super) chat: MenuChat,
    pub(super) leading: Option<Rc<dyn Fn() -> PopupMenuItem>>,
}

impl Options {
    pub(super) fn new(chat: MenuChat) -> Self {
        Self {
            chat,
            leading: None,
        }
    }
}

#[cfg(feature = "gpui-kit-text-patch")]
#[path = "menu_link.rs"]
mod link;
#[cfg(feature = "gpui-kit-text-patch")]
use link::LinkMenuPress;

#[derive(Default)]
pub(super) struct RichMenu {
    #[cfg(feature = "gpui-kit-text-patch")]
    press: LinkMenuPress,
}

impl RichMenu {
    pub(super) fn wrap<E>(
        &self,
        root: E,
        context: RichTextContext,
        options: Options,
        show_menu: bool,
    ) -> AnyElement
    where
        E: InteractiveElement + IntoElement + ParentElement + Styled + 'static,
    {
        let clear = context.clone();
        let root = root.capture_any_mouse_down(move |event, _, _| {
            if event.button == MouseButton::Right {
                clear.clear_link();
            }
        });
        if !show_menu {
            return root.into_any_element();
        }
        #[cfg(feature = "gpui-kit-text-patch")]
        let menu = self.press.wrap(root, context, options);
        #[cfg(not(feature = "gpui-kit-text-patch"))]
        let menu = root.anchored_context_menu(move |menu, window, cx| {
            build(menu, &Source::Rich(context.clone()), &options, window, cx)
        });
        menu.into_any_element()
    }
}

#[derive(Clone)]
pub(super) enum Source {
    Rich(RichTextContext),
    Plain(Entity<PlainState>),
    Fallback(String),
}

impl Source {
    fn payload(&self, window: &mut Window, cx: &mut App) -> String {
        let selected = TextSelection::selected_text(window, cx);
        match self {
            Self::Rich(context) => rich_payload(context, &selected, cx),
            Self::Plain(state) => plain_payload(state, &selected, cx),
            Self::Fallback(fallback) => super::selected_or_fallback(&selected, fallback),
        }
    }

    fn take_link(&self) -> Option<String> {
        match self {
            Self::Rich(context) => context.take_link(),
            _ => None,
        }
    }

    fn select_all(&self, window: &mut Window, cx: &mut App) {
        match self {
            Self::Rich(context) => {
                TextSelection::clear(window, cx);
                _ = context.state.update(cx, |state, cx| state.select_all(cx));
            }
            Self::Plain(state) => state.update(cx, |state, cx| state.select_all(window, cx)),
            Self::Fallback(_) => {}
        }
    }
}

pub(super) fn with_source<E>(element: E, source: Source, chat: MenuChat) -> ContextMenu<E>
where
    E: InteractiveElement + ParentElement + Styled,
{
    let options = Options::new(chat);
    element
        .anchored_context_menu(move |menu, window, cx| build(menu, &source, &options, window, cx))
}

fn build(
    menu: PopupMenu,
    source: &Source,
    options: &Options,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let payload = source.payload(window, cx);
    let copy = payload.clone();
    let menu = match &options.leading {
        Some(item) => menu.item(item()).separator(),
        None => menu,
    };
    let mut menu = menu.item(
        PopupMenuItem::new("Copy")
            .icon(subspace_lucide::IconName::Copy)
            .on_click(move |_, _, cx| links::copy(copy.clone(), cx)),
    );
    if let Some(url) = source.take_link() {
        menu = menu.item(PopupMenuItem::new("Copy link").on_click(move |_, _, cx| {
            links::copy(url.clone(), cx);
        }));
    }
    if !matches!(source, Source::Fallback(_)) {
        let source = source.clone();
        menu = menu.item(
            PopupMenuItem::new("Select all").on_click(move |_, window, cx| {
                source.select_all(window, cx);
            }),
        );
    }
    if options.chat == MenuChat::Offered && super::local_chat_enabled(cx) {
        menu = menu.item(PopupMenuItem::new("Add to local model chat").on_click(
            move |_, window, cx| {
                let payload = payload.clone();
                window.defer(cx, move |window, cx| {
                    set_selection(cx, payload);
                    window.dispatch_action(Box::new(AddSelectionToChat), cx);
                });
            },
        ));
    }
    menu
}

fn plain_payload(state: &Entity<PlainState>, selected: &str, cx: &App) -> String {
    let state = state.read(cx);
    if !selected.is_empty()
        && (state.selection.has_local_selection(cx) || state.selection.snapshot(cx).is_some())
    {
        selected.to_owned()
    } else {
        state.text.to_string()
    }
}

fn rich_payload(context: &RichTextContext, selected: &str, cx: &App) -> String {
    let Some(state) = context.state.upgrade() else {
        return String::new();
    };
    let state = state.read(cx);
    let own = state.selected_text();
    if !selected.is_empty() && !own.is_empty() {
        if own == selected {
            let text = state.rendered_text();
            if let Some(range) = state
                .selected_source_range()
                .and_then(|range| text.range_for_source(range))
                && let Some(exact) = text.as_str().get(range)
            {
                return exact.to_owned();
            }
        }
        return selected.to_owned();
    }
    context.fallback.clone().unwrap_or_else(|| {
        state
            .rendered_text()
            .as_str()
            .trim_end_matches('\n')
            .to_owned()
    })
}
