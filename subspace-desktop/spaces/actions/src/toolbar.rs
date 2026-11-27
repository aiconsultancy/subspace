//! The pane's toolbar. Filters lead, Trigger (the Space's one primary
//! action) trails, and leading controls wrap in a narrow pane. The dock
//! switches are the status bar's (`Dashboard::docks`).

use gpui_kit::component::Disableable as _;
use gpui_kit::{AnyElement, ClickEvent, Context, WeakEntity};
use subspace_framework::dashboard::action::{
    Intent, action_button, icon_button, main_action, menu_button,
};
use subspace_framework::dashboard::icon_button::IconButton;
use subspace_framework::dashboard::workspace;
use subspace_framework::lucide::IconName;

use crate::{Actions, filter_popup, menus::Picker, model::View, writes::Operation};

pub(crate) fn render(this: &Actions, cx: &mut Context<Actions>) -> workspace::Toolbar {
    let toolbar = workspace::Toolbar::new("actions-list-toolbar");
    // The navigator holds the views; hidden, they stay one menu away.
    let toolbar = if this.docks.navigator {
        toolbar
    } else {
        toolbar.leading(picker(this, Picker::View, cx))
    };
    toolbar
        .leading(filter_popup::button(this, cx))
        .leading(picker(this, Picker::Account, cx))
        .leading(picker(this, Picker::Log, cx))
        .trailing(
            icon_button(
                "actions-refresh",
                Intent::Quiet,
                IconName::RefreshCw,
                "Refresh",
            )
            .on_click(cx.listener(|this, _, _, cx| this.refresh_all(cx))),
        )
        .trailing(trigger(this))
}

/// A menu button that opens its picker from its own anchor.
fn picker(this: &Actions, kind: Picker, cx: &mut Context<Actions>) -> AnyElement {
    let button = menu_button(
        format!("actions-picker-{}", kind.title()),
        picker_icon(kind, this.view),
        picker_label(kind, this),
        false,
    )
    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
        crate::menus::open_picker(this, kind, None, window, cx)
    }));
    this.menu_anchors.trigger(kind.id(), button, cx)
}

/// The view menu shows the current view's glyph; the others filter.
fn picker_icon(kind: Picker, view: View) -> IconName {
    if kind == Picker::View {
        view.icon()
    } else {
        IconName::Funnel
    }
}

/// The view menu and the account picker name the current choice.
fn picker_label(kind: Picker, this: &Actions) -> String {
    match kind {
        Picker::View => this.view.title().into(),
        Picker::Account => this.login.clone().unwrap_or_else(|| kind.title().into()),
        _ => kind.title().into(),
    }
}

/// Trigger starts a new run of the selected run's workflow. It is the
/// Space's main action: the toolbar's one primary button.
fn trigger(this: &Actions) -> IconButton {
    let operation = Operation::Dispatch;
    let button = main_action(
        write_id(operation),
        operation.icon(),
        write_label(operation),
    );
    let reason = crate::writes::unavailable(operation, this.write_target());
    wire(button, this.weak.clone(), operation, reason)
}

/// A button that opens `operation`'s confirmation. `reason` disables it and
/// is its tooltip.
pub(crate) fn write_button(
    weak: WeakEntity<Actions>,
    operation: Operation,
    reason: Option<&'static str>,
    intent: Intent,
) -> IconButton {
    let button = action_button(
        write_id(operation),
        intent,
        Some(operation.icon()),
        write_label(operation),
    );
    wire(button, weak, operation, reason)
}

/// A write button's element id.
fn write_id(operation: Operation) -> String {
    format!("actions-intent-{}", operation.title())
}

/// Opens `operation`'s confirmation from `button`; `reason` disables it and
/// is its tooltip.
fn wire(
    button: IconButton,
    weak: WeakEntity<Actions>,
    operation: Operation,
    reason: Option<&'static str>,
) -> IconButton {
    button
        .disabled(reason.is_some())
        .tooltip(reason.unwrap_or(operation.title()))
        .on_click(move |_, window, cx| {
            _ = weak.update(cx, |this, cx| this.begin_write(operation, window, cx));
        })
}

/// Trigger asks for a branch and inputs first, so its label says more
/// follows; the others keep their titles.
fn write_label(operation: Operation) -> String {
    if operation == Operation::Dispatch {
        operation.prompt()
    } else {
        operation.title().into()
    }
}

#[cfg(test)]
mod tests {
    use subspace_framework::lucide::IconName;

    use super::{picker_icon, write_label};
    use crate::{menus::Picker, model::View, writes::Operation};

    #[test]
    fn only_trigger_is_labelled_as_asking_for_more() {
        assert_eq!(write_label(Operation::Dispatch), "Trigger…");
        let labels = Operation::FOOTER.map(write_label);
        assert_eq!(labels[0], "Approve");
        assert_eq!(labels[2], "Re-run");
        assert!(labels.iter().all(|label| !label.ends_with('…')));
    }

    #[test]
    fn the_view_menu_wears_the_current_views_glyph() {
        assert_eq!(picker_icon(Picker::View, View::Flow), View::Flow.icon());
        assert_eq!(picker_icon(Picker::Account, View::Flow), IconName::Funnel);
        assert_eq!(picker_icon(Picker::Log, View::Runs), IconName::Funnel);
    }
}
