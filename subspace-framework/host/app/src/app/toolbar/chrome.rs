use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::Button;
use gpui_kit::{AnyElement, Context, IntoElement as _};
use subspace_lucide::IconName;

#[cfg(feature = "report-issue")]
use super::super::OpenReportIssue;
use super::super::{OpenAbout, OpenSettings, Reload, SubspaceApp, ToggleHelp, toggle_wrap};
use super::style::{chrome, shell_button, sized_button, with_label_menu};
use crate::toolbar_labels::ToolbarControl;

fn chrome_item(
    button: Button,
    control: ToolbarControl,
    icon: IconName,
    cx: &mut Context<SubspaceApp>,
) -> AnyElement {
    shell_button(button, control, icon, cx).into_any_element()
}

pub(super) const RELOAD_BUSY_LABEL: &str = "Reload (Space busy)";

pub(super) fn reload_chrome(busy: bool, cx: &mut Context<SubspaceApp>) -> AnyElement {
    let control = ToolbarControl::Reload;
    let show_spinner = busy && !subspace_dashboard::activity_mark::in_product_toolbar(cx);
    let tooltip = if busy {
        RELOAD_BUSY_LABEL
    } else {
        "Reload (Cmd/Ctrl+R)"
    };
    let button = chrome("reload", tooltip, tooltip, busy)
        .disabled(busy)
        .on_click(cx.listener(|app, _, window, cx| app.reload(&Reload, window, cx)));
    let label = control.shows_label(cx).then(|| control.label().into());
    let button = sized_button(button, IconName::RotateCw, label, show_spinner);
    with_label_menu(button, control, cx).into_any_element()
}

pub(super) fn wrap_chrome(wrap: bool, cx: &mut Context<SubspaceApp>) -> AnyElement {
    let control = ToolbarControl::Wrap;
    let button = chrome("wrap", "Wrap long lines (W)", "Wrap long lines (W)", false)
        .on_click(cx.listener(|_, _, _, cx| toggle_wrap(cx)));
    let label = control.shows_label(cx).then(|| control.label().into());
    let button = sized_button(button, IconName::TextWrap, label, false).latched(wrap);
    with_label_menu(button, control, cx).into_any_element()
}

pub(super) fn jig_chrome(open: bool, cx: &mut Context<SubspaceApp>) -> AnyElement {
    chrome_item(
        chrome("jig", "Theme designer", "Theme designer (`)", open).on_click(cx.listener(
            |app, _, window, cx| {
                let open = !app.jig_open.get();
                app.set_jig(open, window, cx);
            },
        )),
        ToolbarControl::Jig,
        IconName::Eye,
        cx,
    )
}

type ChromeClick = fn(&mut SubspaceApp, &mut gpui_kit::Window, &mut Context<SubspaceApp>);

/// A toolbar button that only runs one app method when clicked.
struct SimpleChrome {
    id: &'static str,
    icon: IconName,
    label: &'static str,
    tooltip: &'static str,
    control: ToolbarControl,
    click: ChromeClick,
}

impl SimpleChrome {
    fn render(self, selected: bool, cx: &mut Context<SubspaceApp>) -> AnyElement {
        let click = self.click;
        chrome_item(
            chrome(self.id, self.label, self.tooltip, selected)
                .on_click(cx.listener(move |app, _, window, cx| click(app, window, cx))),
            self.control,
            self.icon,
            cx,
        )
    }
}

pub(super) fn settings_chrome(cx: &mut Context<SubspaceApp>) -> AnyElement {
    SimpleChrome {
        id: "settings",
        icon: IconName::Settings,
        label: "Settings",
        tooltip: "Settings (Cmd/Ctrl+,)",
        control: ToolbarControl::Settings,
        click: |app, window, cx| app.open_settings(&OpenSettings, window, cx),
    }
    .render(false, cx)
}

pub(super) fn about_chrome(cx: &mut Context<SubspaceApp>) -> AnyElement {
    let control = ToolbarControl::About;
    let (icon, label) = about_presentation(crate::app::available_update_version(cx).is_some());
    let button = chrome("about", label, label, false)
        .on_click(cx.listener(|app, _, window, cx| app.open_about(&OpenAbout, window, cx)));
    let label = control.shows_label(cx).then(|| label.into());
    let button = sized_button(button, icon, label, false);
    with_label_menu(button, control, cx).into_any_element()
}

/// A read-only projection shared by inline chrome and the overflow menu.
pub(super) fn about_presentation(available: bool) -> (IconName, &'static str) {
    if available {
        (IconName::Download, "About · Update available")
    } else {
        (IconName::Info, "About")
    }
}

#[cfg(feature = "report-issue")]
pub(super) fn report_issue_chrome(cx: &mut Context<SubspaceApp>) -> AnyElement {
    SimpleChrome {
        id: "report-issue",
        icon: IconName::FaceSlightlyFrowning,
        label: "Report issue",
        tooltip: "Report issue",
        control: ToolbarControl::ReportIssue,
        click: |app, window, cx| app.open_report_issue(&OpenReportIssue, window, cx),
    }
    .render(false, cx)
}

pub(super) fn help_chrome(open: bool, cx: &mut Context<SubspaceApp>) -> AnyElement {
    SimpleChrome {
        id: "help",
        icon: IconName::CircleQuestionMark,
        label: "Help (?)",
        tooltip: "Help (?)",
        control: ToolbarControl::Help,
        click: |app, window, cx| app.toggle_help(&ToggleHelp, window, cx),
    }
    .render(open, cx)
}

#[cfg(test)]
mod tests {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, TestAppContext};
    use subspace_dashboard::{prefs, wrap};

    #[gpui_kit::test]
    fn shell_wrap_reports_pressed_and_preserves_its_owner_and_preference(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let (window, _) =
            super::super::tests::status::rooted_fixture(cx, root.path(), 2200., false);
        cx.update_window(window, |_, window, cx| {
            let before = wrap(cx);
            assert_eq!(window.find("wrap").checked(), Some(before));
            assert_eq!(window.find("wrap").label(), Some("Wrap long lines (W)"));
            assert_eq!(window.find("reload").checked(), None);
            window.click("wrap", cx);
            assert_eq!(wrap(cx), !before);
            assert_eq!(window.find("wrap").checked(), Some(!before));
            assert_eq!(
                prefs::get(cx, crate::app::WRAP_KEY).as_deref(),
                Some(if before { "0" } else { "1" })
            );
            window.click("wrap", cx);
            assert_eq!(wrap(cx), before);
            assert_eq!(window.find("wrap").checked(), Some(before));
        })
        .unwrap();
    }
}
