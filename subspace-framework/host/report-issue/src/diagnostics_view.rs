use gpui_kit::TestSupportExt as _;
use gpui_kit::component::{Disableable as _, collapsible::Collapsible};
use gpui_kit::{
    Context, Div, InteractiveElement as _, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, div,
};
use subspace_dashboard::{Tokens, icon_button::IconButton, rich, toggle};
use subspace_lucide::IconName;

use crate::{ReportIssue, form::diagnostic_fields};

// qual:allow(coupling, oi) reason: "The diagnostics region shares the retained ReportIssue owner in report.rs."
impl ReportIssue {
    pub(super) fn render_diagnostics(&self, cx: &mut Context<Self>) -> Div {
        let tokens = Tokens::current(cx);
        let diagnostics = self.effective_diagnostics();
        let fields = diagnostic_fields(&diagnostics);
        let open = self.state.diagnostics_expanded;
        let label = format!("Automatic diagnostics, {} fields", fields.len());
        div().flex().flex_col().gap_2()
            .child(toggle::toggle_row(
                "Include paths",
                toggle::switch("report-include-paths", cx)
                    .checked(self.state.include_paths)
                    .disabled(self.state.busy() || self.available_paths.is_none())
                    .on_click(cx.listener(|this, checked: &bool, _, cx| this.set_include_paths(*checked, cx))),
            ))
            .child(div().text_color(tokens.muted).child("Paths can show folder and config names. Check the preview before sending."))
            .child(Collapsible::new().open(open)
                .child(IconButton::new("report-diagnostics-toggle")
                    .icon(if open { IconName::ChevronDown } else { IconName::ChevronRight })
                    .label(label).aria_expanded(open).focus_on_click()
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_diagnostics(cx)))
                    // The dialog binds Enter to Confirm before native button key-up.
                    // Handle that existing action at the disclosure, not the dialog.
                    .on_action(cx.listener(|this, _: &gpui_kit::component::dialog::Confirm, _, cx| this.toggle_diagnostics(cx))))
                .content(diagnostics_grid(&fields, tokens)))
            .child(div().text_color(tokens.muted).child("Only the diagnostics shown above are included. Configuration and logs are not collected."))
    }

    fn toggle_diagnostics(&mut self, cx: &mut Context<Self>) {
        self.state.diagnostics_expanded = !self.state.diagnostics_expanded;
        cx.notify();
    }
}

fn diagnostics_grid(fields: &[(&str, &str)], tokens: Tokens) -> impl gpui_kit::IntoElement {
    div()
        .id("report-diagnostics-grid")
        .test_support()
        .role(Role::Table)
        .aria_label("Automatic diagnostics")
        .aria_column_count(2)
        .aria_row_count(fields.len())
        .flex()
        .flex_col()
        .gap_1()
        .text_size(tokens.secondary_size())
        .children(fields.iter().map(|(label, value)| {
            div()
                .id(SharedString::from(format!("report-diagnostic-row-{label}")))
                .role(Role::Row)
                .flex()
                .items_start()
                .gap_3()
                .min_w_0()
                .child(
                    div()
                        .id(SharedString::from(format!("report-diagnostic-key-{label}")))
                        .test_support()
                        .role(Role::RowHeader)
                        .aria_label((*label).to_owned())
                        .w_32()
                        .flex_shrink_0()
                        .text_color(tokens.muted)
                        .child((*label).to_owned()),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "report-diagnostic-value-{label}"
                        )))
                        .test_support()
                        .role(Role::Cell)
                        .aria_label((*value).to_owned())
                        .flex_1()
                        .min_w_0()
                        .child(rich::plain_text(
                            SharedString::from(format!("report-diagnostic-{label}")),
                            value,
                        )),
                )
        }))
}
