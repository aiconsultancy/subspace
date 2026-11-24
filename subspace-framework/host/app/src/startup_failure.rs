//! A start-up failure a person can see. An app that cannot start, because a
//! setting it reads before the shell is wrong, opens one small window
//! saying what is wrong and where, with Quit, instead of only writing to a
//! terminal that someone who opened it from the Finder, the Dock or the
//! Start menu never sees.

use std::process::ExitCode;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AppContext as _, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, TestSupportExt as _, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, px, size,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::action::{Intent, dialog_button};
use subspace_dashboard::dialog;
use subspace_lucide::IconName;

const WIDTH: f32 = 560.;
const HEIGHT: f32 = 260.;

/// Writes `title` and `detail` to standard error and shows them in a window
/// titled `app_name` until the person quits or closes it. Returns the
/// failure exit code where the event loop returns (Linux and Windows); on
/// macOS quitting ends the process from inside the loop, with exit code 0.
// qual:api - a product calls it when its own start-up checks fail.
pub fn report_startup_failure(app_name: &str, title: &str, detail: &str) -> ExitCode {
    eprintln!("{title}\n{detail}");
    let (name, title, detail) = (
        SharedString::from(app_name.to_owned()),
        SharedString::from(title.to_owned()),
        SharedString::from(detail.to_owned()),
    );
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(WIDTH), px(HEIGHT)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some(name.clone()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let opened = gpui_kit::open_window(options, cx, |_, cx| {
            cx.new(|_| StartupFailure::new(title.clone(), detail.clone()))
        });
        if opened.is_err() {
            cx.quit();
        }
        // Closing the only window quits too, rather than leaving a process
        // with no window in the Dock.
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
    ExitCode::FAILURE
}

/// The window's one view: what failed, the detail, and Quit.
pub(crate) struct StartupFailure {
    title: SharedString,
    detail: SharedString,
}

impl StartupFailure {
    pub(crate) fn new(title: SharedString, detail: SharedString) -> Self {
        Self { title, detail }
    }
}

impl Render for StartupFailure {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        v_flex()
            .id("startup-failure")
            .test_support()
            .size_full()
            .p_6()
            .gap_3()
            .bg(tokens.bg)
            .text_color(tokens.fg)
            .child(
                div()
                    .text_size(tokens.heading_font)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(tokens.fg_strong)
                    .child(self.title.clone()),
            )
            .child(div().flex_1().child(self.detail.clone()))
            .child(
                dialog::footer().child(
                    dialog_button(
                        "startup-failure-quit",
                        Intent::Primary,
                        Some(IconName::LogOut),
                        "Quit",
                    )
                    .on_click(|_, _, cx| cx.quit()),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, TestAppContext};

    use super::StartupFailure;

    #[gpui_kit::test]
    fn the_failure_says_what_and_where_and_offers_quit(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let window = cx.add_window(|_, _| {
            StartupFailure::new(
                "Subspace cannot start".into(),
                "connection.json: api_url must be HTTPS.".into(),
            )
        });
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("startup-failure").is_some());
            assert_eq!(window.find("startup-failure-quit").label(), Some("Quit"));
        })
        .unwrap();
    }
}
