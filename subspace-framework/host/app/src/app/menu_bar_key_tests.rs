//! The menu bar's keys, and the window and app commands its items run.

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn the_menu_bar_keys_reach_their_commands(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        super::super::bind_keys(cx);
        for (keys, action) in MAC_KEYS {
            let input = [gpui_kit::Keystroke::parse(keys).unwrap()];
            let bound = cx
                .all_bindings_for_input(&input)
                .iter()
                .any(|binding| binding.action().name() == action);
            assert!(bound, "{keys} runs {action}");
        }
    });
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn chats_tab_keys_win_while_chat_has_focus(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::KeyContext;
    use subspace_dashboard::chat_input::CHAT_PANEL_CONTEXT;
    cx.update(|cx| {
        subspace_dashboard::chat_input::bind_keys(cx);
        super::super::bind_keys(cx);
        let shell = KeyContext::parse("Workspace").unwrap();
        let chat = KeyContext::parse(CHAT_PANEL_CONTEXT).unwrap();
        // As GPUI reports the keys from a US keyboard, and as written.
        for (keys, space, tab) in [
            ("cmd-}", NEXT_SPACE, "ChatNextTab"),
            ("cmd-{", PREVIOUS_SPACE, "ChatPreviousTab"),
            ("cmd-shift-]", NEXT_SPACE, "ChatNextTab"),
            ("cmd-shift-[", PREVIOUS_SPACE, "ChatPreviousTab"),
        ] {
            assert_eq!(
                first_binding(cx, keys, std::slice::from_ref(&shell)),
                Some(space)
            );
            let in_chat =
                first_binding(cx, keys, &[shell.clone(), chat.clone()]).unwrap_or_default();
            assert!(in_chat.ends_with(tab), "{keys} in Chat runs {in_chat}");
        }
    });
}

/// The action of the binding that wins for `keys` in `stack`.
#[cfg(target_os = "macos")]
fn first_binding(
    cx: &gpui_kit::App,
    keys: &'static str,
    stack: &[gpui_kit::KeyContext],
) -> Option<&'static str> {
    let input = [gpui_kit::Keystroke::parse(keys).unwrap()];
    let (bindings, _) = cx.key_bindings().borrow().bindings_for_input(&input, stack);
    bindings.first().map(|binding| binding.action().name())
}

#[cfg(target_os = "macos")]
const NEXT_SPACE: &str = "subspace_app::NextDashboard";
#[cfg(target_os = "macos")]
const PREVIOUS_SPACE: &str = "subspace_app::PreviousDashboard";

#[cfg(target_os = "macos")]
const MAC_KEYS: [(&str, &str); 8] = [
    ("cmd-h", "subspace_app::HideApp"),
    ("cmd-alt-h", "subspace_app::HideOtherApps"),
    ("cmd-m", "subspace_app::MinimizeWindow"),
    ("cmd-ctrl-f", "subspace_app::ToggleFullScreen"),
    ("cmd-}", NEXT_SPACE),
    ("cmd-{", PREVIOUS_SPACE),
    ("cmd-shift-]", "subspace_app::NextDashboard"),
    ("cmd-shift-[", "subspace_app::PreviousDashboard"),
];

#[gpui_kit::test]
fn the_window_and_app_commands_are_available_without_a_focused_window(
    cx: &mut gpui_kit::TestAppContext,
) {
    cx.update(|cx| {
        // The app's own key setup, so the test also covers its wiring.
        super::super::bind_keys(cx);
        assert!(cx.is_action_available(&super::ToggleFullScreen));
        let mac = [
            cx.is_action_available(&super::HideApp),
            cx.is_action_available(&super::HideOtherApps),
            cx.is_action_available(&super::ShowAllApps),
            cx.is_action_available(&super::MinimizeWindow),
            cx.is_action_available(&super::ZoomWindow),
        ];
        // Only macOS handles them: GPUI panics on Windows when asked to hide
        // other apps.
        assert_eq!(mac, [cfg!(target_os = "macos"); 5]);
    });
}

/// A focused window, as the shell's is when a key or menu item runs.
struct Focused(gpui_kit::FocusHandle);

impl gpui_kit::Render for Focused {
    fn render(
        &mut self,
        _: &mut gpui_kit::Window,
        _: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::InteractiveElement as _;
        gpui_kit::div().track_focus(&self.0)
    }
}

#[gpui_kit::test]
fn full_screen_from_a_focused_window_changes_that_window(cx: &mut gpui_kit::TestAppContext) {
    cx.update(super::register_actions);
    let (view, cx) = cx.add_window_view(|_, cx| Focused(cx.focus_handle()));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| {
        let focus = view.read(cx).0.clone();
        window.focus(&focus, cx);
        window.dispatch_action(Box::new(super::ToggleFullScreen), cx);
    });
    cx.run_until_parked();
    cx.update(|window, _| assert!(window.is_fullscreen(), "the window went full screen"));
}
