//! The smallest Space: one view and no data. `ID`, `TITLE`, `new` and
//! `reload` are all `Dashboard` requires; `agent_tools` is the one opt-in
//! this Space takes, in `tools.rs`.

mod tools;

use std::sync::Arc;

use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div,
};
use subspace_framework::dashboard::{
    AgentTool, Dashboard, DashboardEvent, KeyHelp, Tokens, ToolEnvironment, feedback,
};

pub struct Hello {
    focus: FocusHandle,
}

impl EventEmitter<DashboardEvent> for Hello {}

impl Focusable for Hello {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Hello {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        div()
            .key_context(Self::ID)
            .track_focus(&self.focus)
            .size_full()
            .bg(tokens.bg)
            .child(feedback::empty(
                &tokens,
                "hello",
                "Hello from a Space",
                "This view is all the Space draws. The shell supplies the rest.",
            ))
    }
}

impl Dashboard for Hello {
    const ID: &'static str = "hello";
    const TITLE: &'static str = "Hello";

    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }

    fn agent_tools(_environment: &ToolEnvironment) -> Vec<Arc<dyn AgentTool>> {
        tools::tools()
    }

    /// Not a list, so it lists no list keys.
    fn keys(&self) -> &'static [KeyHelp] {
        &[]
    }

    fn reload(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}
}
