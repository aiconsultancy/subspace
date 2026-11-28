//! Keep the native menu empty during the press so it cannot cover the link
//! before TextView dispatches its auxiliary click on release.

use std::{cell::Cell, rc::Rc};

use gpui_kit::component::menu::{ContextMenu, PopupMenu};
use gpui_kit::{App, InteractiveElement, MouseButton, ParentElement, Styled, WeakEntity, Window};

use crate::anchored::AnchoredContextMenuExt as _;

use super::{Options, RichTextContext, Source, build};

#[derive(Default)]
enum Press {
    #[default]
    Idle,
    RightDown,
    WaitingForLink(WeakEntity<PopupMenu>),
}

#[derive(Clone, Default)]
pub(in super::super) struct LinkMenuPress(Rc<Cell<Press>>);

impl LinkMenuPress {
    pub(in super::super) fn wrap<E>(
        &self,
        element: E,
        context: RichTextContext,
        options: Options,
    ) -> ContextMenu<E>
    where
        E: InteractiveElement + ParentElement + Styled,
    {
        let down = self.clone();
        let inside = self.clone();
        let outside = self.clone();
        let source = Source::Rich(context);
        let inside_source = source.clone();
        let outside_source = source.clone();
        let press = self.clone();
        let inside_options = options.clone();
        let outside_options = options.clone();
        element
            .capture_any_mouse_down(move |event, _, _| {
                if event.button == MouseButton::Right {
                    down.0.set(Press::RightDown);
                }
            })
            .capture_any_mouse_up(move |event, window, cx| {
                if event.button == MouseButton::Right {
                    inside.finish(&inside_source, &inside_options, window, cx);
                }
            })
            .on_mouse_up_out(MouseButton::Right, move |_, window, cx| {
                outside.finish(&outside_source, &outside_options, window, cx);
            })
            .anchored_context_menu(move |menu, window, cx| match press.0.take() {
                Press::RightDown => {
                    press.0.set(Press::WaitingForLink(cx.weak_entity()));
                    menu
                }
                _ => build(menu, &source, &options, window, cx),
            })
    }

    fn finish(&self, source: &Source, options: &Options, window: &mut Window, cx: &mut App) {
        let Press::WaitingForLink(menu) = self.0.take() else {
            return;
        };
        // Link handlers run in the bubble phase and may stop propagation.
        // Read the destination after dispatch has completed.
        let source = source.clone();
        let options = options.clone();
        window.defer(cx, move |window, cx| {
            _ = menu.update(cx, |menu, cx| {
                menu.rebuild(window, cx, |menu, window, cx| {
                    build(menu, &source, &options, window, cx)
                });
            });
        });
    }
}
