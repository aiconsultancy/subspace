//! Type-erased native Space find host construction.
use crate::{Dashboard, find_in_page::FindInPage};
use gpui_kit::{AnyView, App, AppContext as _, Entity, Window};
pub(super) fn create<D: Dashboard>(
    owner: &Entity<D>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<FindInPage> {
    let read = owner.downgrade();
    let reveal = read.clone();
    let mode = read.clone();
    let provider = crate::find_in_page::Provider {
        virtual_content: std::rc::Rc::new(move |cx| {
            mode.read_with(cx, |board, cx| board.find_virtual_content(cx))
                .unwrap_or(false)
        }),
        snapshot: std::rc::Rc::new(move |cx| {
            read.read_with(cx, |board, cx| board.find_documents(cx))
                .unwrap_or_default()
        }),
        reveal: std::rc::Rc::new(move |id, window, cx| {
            _ = reveal.update(cx, |board, cx| board.reveal_find_document(id, window, cx));
        }),
    };
    cx.new(|cx| {
        let mut pane =
            crate::find_in_page::FindInPage::new(owner.clone().into(), provider, window, cx);
        pane.observe_owner(owner, cx);
        pane
    })
}

impl super::DashboardHandle {
    pub fn find_commands(&self, cx: &App) -> Vec<crate::PaletteCommand> {
        crate::find_in_page::commands(
            self.find
                .get()
                .is_some_and(|find| find.read(cx).is_open(cx)),
        )
    }
    pub fn find_view(&self, window: &mut Window, cx: &mut App) -> AnyView {
        self.find
            .get_or_init(|| self.inner.find_view(window, cx))
            .clone()
            .into()
    }
}
