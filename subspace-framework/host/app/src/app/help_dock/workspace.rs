//! A retained Dock center with a weak shell owner. Only its render callback
//! borrows the shell; Dock metadata must never do so during reconciliation.

use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, WeakEntity, Window, div,
};

use super::super::SubspaceApp;

pub(super) type WorkspaceRenderer =
    fn(&mut SubspaceApp, &mut Window, &mut Context<SubspaceApp>) -> AnyElement;

pub(super) struct WorkspacePanel {
    owner: WeakEntity<SubspaceApp>,
    renderer: WorkspaceRenderer,
    focus: FocusHandle,
    _observation: Subscription,
}

impl WorkspacePanel {
    pub(super) fn create(
        owner: &Entity<SubspaceApp>,
        renderer: WorkspaceRenderer,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self {
            owner: owner.downgrade(),
            renderer,
            focus: cx.focus_handle(),
            _observation: cx.observe(owner, |_, _, cx| cx.notify()),
        })
    }
}

impl BasePanel for WorkspacePanel {
    fn panel_name(&self) -> &'static str {
        "app.workspace"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn zoomable(&self, _: &App) -> bool {
        false
    }
}

impl Panel for WorkspacePanel {
    fn title_bar(&self, _: &App) -> bool {
        false
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl EventEmitter<PanelEvent> for WorkspacePanel {}

impl Focusable for WorkspacePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WorkspacePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let renderer = self.renderer;
        self.owner
            .update(cx, |app, cx| {
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .child(renderer(app, window, cx))
                    .into_any_element()
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}
