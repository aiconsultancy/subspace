use super::{IssuesSettings, connection_note};
use gpui_kit::component::{Sizable as _, spinner::Spinner, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_framework::dashboard::{Tokens, fields, row_menu::MenuChat};
impl Render for IssuesSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let status = self.connection_state();
        let connecting = self.access == crate::scope::Access::Starting;
        v_flex()
            .id("issues-settings")
            .test_support()
            .gap_2()
            .child(fields::heading(subspace_api_client::service_name(), cx))
            .child(fields::note_with(
                &tokens,
                self.address.clone(),
                MenuChat::Off,
            ))
            .child(fields::note_with(&tokens, connection_note(), MenuChat::Off))
            .child(
                div()
                    .id("issues-connection-state")
                    .role(Role::Status)
                    .test_support()
                    .aria_label(status.clone())
                    .text_color(tokens.muted)
                    .when(connecting, |row| {
                        row.child(Spinner::new().small().color(tokens.muted))
                    })
                    .when(!connecting, |row| row.child(status)),
            )
    }
}
