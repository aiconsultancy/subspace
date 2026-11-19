//! Persistent navigation presentation choices and their safe fallbacks.

mod measure;
mod resize;
mod sizing;

#[cfg(test)]
mod interaction_tests;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::ContextMenu;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement as _, ParentElement,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, px, svg,
};
use subspace_dashboard::icon_button::{IconButton, Metrics};
use subspace_dashboard::{Tokens, glass, prefs};
use subspace_lucide::IconName;

use super::SubspaceApp;
use super::space_menu::with_space_menu;
use super::toolbar::style::tab_badge;
use sizing::{BUTTON, ICON};

#[cfg(test)]
use resize::widths_match;

pub(crate) const NAVIGATION_KEY: &str = "app.board-navigation";
pub(crate) const COLLAPSED_KEY: &str = "app.board-navigation-collapsed";
pub(crate) const WIDTH_KEY: &str = "app.board-navigation-width";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigationMode {
    Horizontal,
    Vertical,
}

impl NavigationMode {
    pub(crate) fn read(value: Option<&str>) -> Self {
        match value {
            Some("vertical") => Self::Vertical,
            _ => Self::Horizontal,
        }
    }

    pub(crate) const fn stored(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }

    pub(super) fn toggle(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }
}

pub(crate) fn read_mode(cx: &App) -> NavigationMode {
    NavigationMode::read(prefs::get(cx, NAVIGATION_KEY).as_deref())
}

pub(crate) fn read_collapsed(value: Option<&str>) -> bool {
    value == Some("1")
}

pub(crate) fn read_collapsed_preference(cx: &App) -> bool {
    read_collapsed(prefs::get(cx, COLLAPSED_KEY).as_deref())
}

pub(crate) fn read_width_preference(cx: &App) -> Option<f32> {
    sizing::read_width(prefs::get(cx, WIDTH_KEY).as_deref())
}

pub(crate) fn save_mode(cx: &mut App, mode: NavigationMode) {
    prefs::set(cx, NAVIGATION_KEY, mode.stored());
}

pub(crate) fn save_collapsed(cx: &mut App, collapsed: bool) {
    prefs::set(cx, COLLAPSED_KEY, if collapsed { "1" } else { "0" });
}

pub(crate) fn save_width(cx: &mut App, width: f32) {
    prefs::set(cx, WIDTH_KEY, width.to_string());
}

pub(super) fn collapsed_rail_width() -> f32 {
    sizing::collapsed_width()
}

pub(super) fn effective_collapsed_rail_width(cx: &App) -> f32 {
    let target = Metrics::current(cx)
        .target()
        .to_pixels(cx.theme().font_size);
    collapsed_rail_width().max(sizing::INSET + f32::from(target) + sizing::INSET)
}

impl SubspaceApp {
    pub(super) fn vertical_navigation(
        &self,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let collapsed = !self.rail_shows_labels(cx);
        let tabs = self.vertical_tabs(collapsed, tokens, cx);
        let collapse_label = if collapsed {
            "Expand Space navigation"
        } else {
            "Collapse Space navigation"
        };
        v_flex()
            .id("board-navigation-rail")
            .test_support()
            .h_full()
            .w_full()
            .gap(px(sizing::INSET))
            .pt(px(sizing::INSET))
            .pb(px(sizing::INSET))
            .pl(px(sizing::INSET))
            .pr(px(sizing::inner_end_inset()))
            .bg(tokens.sidebar)
            .border_r_1()
            .border_color(tokens.sidebar_line)
            .child(
                v_flex()
                    .id("board-navigation-tabs")
                    .flex_1()
                    .min_h_0()
                    .gap(px(sizing::INSET))
                    .overflow_y_scroll()
                    .children(tabs),
            )
            .child(
                gpui_kit::div()
                    .id("board-navigation-footer")
                    .test_support()
                    .child(
                        IconButton::new("toggle-board-navigation")
                            .ghost()
                            .icon(if collapsed {
                                IconName::PanelLeftOpen
                            } else {
                                IconName::PanelLeftClose
                            })
                            .accessibility_label(collapse_label)
                            .tooltip(collapse_label)
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.toggle_navigation_collapsed(window, cx)
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn rail_content_max(&self, window: &Window, cx: &App) -> f32 {
        let font = Tokens::current(cx).row_font;
        let family = cx.theme().font_family.clone();
        sizing::content_max(self.dashboards.iter().map(|dashboard| {
            let label = measure::shaped_width(window, &dashboard.title, font, family.clone());
            let badge = dashboard.badge(cx).map(|badge| {
                sizing::badge_box(measure::shaped_width(
                    window,
                    &badge,
                    px(sizing::BADGE_FONT),
                    family.clone(),
                ))
            });
            sizing::entry_width(dashboard.icon.is_some(), label, badge)
        }))
    }

    pub(super) fn shown_rail_width(&self, window: &Window, cx: &App) -> f32 {
        sizing::displayed_width(
            self.navigation_collapsed,
            self.preferred_rail_width,
            self.rail_content_max(window, cx),
            effective_collapsed_rail_width(cx),
        )
    }

    fn vertical_tabs(
        &self,
        collapsed: bool,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Vec<ContextMenu<Button>> {
        let family = cx.theme().font_family.clone();
        let closable = self.can_close_space();
        let app = cx.weak_entity();
        self.dashboards
            .iter()
            .enumerate()
            .map(|(index, dashboard)| {
                let active = self.shows_current(index);
                let color = glass::label_color(active, tokens);
                let button = rail_button(
                    ("vertical-dashboard-tab", index),
                    active,
                    collapsed,
                    tokens,
                    cx,
                )
                .accessibility_label(dashboard.description.clone())
                .tooltip(dashboard.description.clone())
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .when(collapsed, |row| row.justify_center())
                        .when(!collapsed, |row| row.justify_start())
                        .gap(px(sizing::GAP))
                        .when_some(dashboard.icon, |row, path| {
                            row.child(
                                svg()
                                    .path(path)
                                    .size(px(ICON))
                                    .flex_shrink_0()
                                    .text_color(color),
                            )
                        })
                        .when(!collapsed, |row| {
                            row.child(
                                gpui_kit::div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .font_family(family.clone())
                                    .text_size(tokens.row_font)
                                    .child(dashboard.title.clone()),
                            )
                        })
                        .when_some(dashboard.badge(cx).filter(|_| !collapsed), |row, badge| {
                            row.child(tab_badge(badge, tokens, cx))
                        }),
                )
                .on_click(cx.listener(move |app, _, window, cx| app.switch_to(index, window, cx)));
                with_space_menu(button, dashboard.id, closable, app.clone())
            })
            .collect()
    }
}

/// One rail destination: the same capsule as a horizontal board tab.
fn rail_button(
    id: impl Into<gpui_kit::ElementId>,
    active: bool,
    collapsed: bool,
    tokens: &Tokens,
    cx: &App,
) -> Button {
    glass::nav_button(id, active, px(BUTTON), tokens, cx)
        .font_family(cx.theme().font_family.clone())
        .text_size(tokens.row_font)
        .w_full()
        .when(collapsed, |button| button.px_0())
        .when(!collapsed, |button| button.px(px(sizing::icon_pad())))
        .py_0()
}

#[cfg(test)]
mod tests {
    use gpui_kit::{
        App, AppContext as _, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
        IntoElement, Render, SharedString, TestAppContext, Window, WindowHandle, div, px,
    };
    use subspace_dashboard::prefs;
    use subspace_dashboard::{Dashboard, DashboardEvent};

    use super::{
        COLLAPSED_KEY, NavigationMode, WIDTH_KEY, collapsed_rail_width, read_collapsed,
        read_collapsed_preference, read_width_preference, save_collapsed, save_width,
    };
    use crate::app::SubspaceApp;

    #[test]
    fn invalid_navigation_preferences_fall_back_to_horizontal_and_expanded() {
        assert_eq!(
            NavigationMode::read(Some("diagonal")),
            NavigationMode::Horizontal
        );
        assert!(!read_collapsed(Some("true")));
        assert!(!read_collapsed(None));
    }

    #[test]
    fn rail_widths_within_half_a_pixel_match() {
        assert!(super::widths_match(90., 90.4));
        assert!(!super::widths_match(90., 91.));
    }

    #[test]
    fn navigation_mode_round_trips_and_toggles() {
        assert_eq!(NavigationMode::read(Some("vertical")).stored(), "vertical");
        assert_eq!(
            NavigationMode::Horizontal.toggle(),
            NavigationMode::Vertical
        );
        assert_eq!(
            NavigationMode::Vertical.toggle(),
            NavigationMode::Horizontal
        );
    }

    #[gpui_kit::test]
    fn a_user_width_is_stored_and_a_clamp_does_not_rewrite_it(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            prefs::init_file(Some(root.path().join("state.json")), cx);
            save_width(cx, 180.);
            save_collapsed(cx, false);
            assert_eq!(read_width_preference(cx), Some(180.));
            let shown = super::sizing::displayed_width(
                false,
                Some(180.),
                90.,
                super::sizing::collapsed_width(),
            );
            assert_eq!(shown, 90.);
            assert_eq!(read_width_preference(cx), Some(180.));
            save_collapsed(cx, true);
            assert!(read_collapsed_preference(cx));
            assert_eq!(read_width_preference(cx), Some(180.));
            assert_eq!(prefs::get(cx, WIDTH_KEY).as_deref(), Some("180"));
        });
    }

    #[gpui_kit::test]
    fn first_collapsed_steady_frame_is_not_the_resizable_default(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let window = open_vertical(cx, root.path(), true, Some("180"));
        draw(&window, cx);
        draw(&window, cx);
        let width = live_width(&window, cx);
        assert!(
            (width - collapsed_rail_width()).abs() < 1.,
            "collapsed rail stuck at {width}, expected {}",
            collapsed_rail_width()
        );
        assert!(
            (width - 100.).abs() > 1.,
            "first laid-out collapsed width must not remain the 100px panel default"
        );
        window
            .update(cx, |app, _, cx| {
                assert_eq!(app.preferred_rail_width, Some(180.));
                assert!(!app.rail_shows_labels(cx));
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn a_temporary_cap_clamps_panel_width_then_restores_preferred(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let window = open_vertical(cx, root.path(), false, Some("180"));
        draw(&window, cx);
        draw(&window, cx);
        window
            .update(cx, |app, window, cx| {
                let panels = app.rail_state.read(cx).sizes().len();
                assert!(
                    panels >= 2,
                    "resizable group must have rail and workspace panels, got {panels}"
                );
                app.reconcile_rail(90., window, cx);
                let clamped = app.live_rail_width(cx).unwrap_or(0.);
                assert!(
                    (clamped - 90.).abs() < 1.,
                    "clamped live width {clamped}, expected 90"
                );
                assert_eq!(app.preferred_rail_width, Some(180.));
                app.reconcile_rail(200., window, cx);
                let restored = app.live_rail_width(cx).unwrap_or(0.);
                assert!(
                    (restored - 180.).abs() < 1.,
                    "restored live width {restored}, expected 180"
                );
                assert_eq!(app.preferred_rail_width, Some(180.));
                assert_eq!(prefs::get(cx, WIDTH_KEY).as_deref(), Some("180"));
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn drag_live_width_hides_labels_without_writing_prefs(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let window = open_vertical(cx, root.path(), false, Some("180"));
        draw(&window, cx);
        draw(&window, cx);
        window
            .update(cx, |app, window, cx| {
                let collapsed = collapsed_rail_width();
                app.rail_state.update(cx, |state, cx| {
                    state.resize_panel(0, px(collapsed), window, cx);
                });
            })
            .unwrap();
        draw(&window, cx);
        window
            .update(cx, |app, _, cx| {
                assert!(!app.rail_shows_labels(cx));
                assert!(!app.navigation_collapsed);
                assert_eq!(app.preferred_rail_width, Some(180.));
                assert_eq!(prefs::get(cx, COLLAPSED_KEY).as_deref(), Some("0"));
                assert_eq!(prefs::get(cx, WIDTH_KEY).as_deref(), Some("180"));
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn collapse_button_and_settings_save_match_after_panels_exist(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let window = open_vertical(cx, root.path(), false, Some("180"));
        draw(&window, cx);
        draw(&window, cx);
        window
            .update(cx, |app, window, cx| {
                app.toggle_navigation_collapsed(window, cx);
            })
            .unwrap();
        draw(&window, cx);
        window
            .update(cx, |app, _, cx| {
                assert!(app.navigation_collapsed);
                assert!(!app.rail_shows_labels(cx));
                assert_eq!(prefs::get(cx, COLLAPSED_KEY).as_deref(), Some("1"));
                assert_eq!(app.preferred_rail_width, Some(180.));
            })
            .unwrap();
        assert!(
            (live_width(&window, cx) - collapsed_rail_width()).abs() < 1.,
            "collapse button live width {}",
            live_width(&window, cx)
        );
        window
            .update(cx, |app, window, cx| {
                save_collapsed(cx, false);
                app.sync_navigation_preferences(window, cx);
            })
            .unwrap();
        draw(&window, cx);
        window
            .update(cx, |app, _, cx| {
                assert!(!app.navigation_collapsed);
                assert!(app.rail_shows_labels(cx));
                assert_eq!(prefs::get(cx, COLLAPSED_KEY).as_deref(), Some("0"));
                assert_eq!(app.preferred_rail_width, Some(180.));
            })
            .unwrap();
        assert!(
            (live_width(&window, cx) - 180.).abs() < 1.,
            "settings save restored live width {}",
            live_width(&window, cx)
        );
    }

    struct WideBoard {
        focus: FocusHandle,
    }
    impl EventEmitter<DashboardEvent> for WideBoard {}
    impl Focusable for WideBoard {
        fn focus_handle(&self, _: &App) -> FocusHandle {
            self.focus.clone()
        }
    }
    impl Render for WideBoard {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().id(Self::ID)
        }
    }
    impl Dashboard for WideBoard {
        const ID: &'static str = "wide-fixture";
        const TITLE: &'static str = "ExtraWideNavigationLabelFixture";
        fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
            Self {
                focus: cx.focus_handle(),
            }
        }
        fn badge(&self, _: &App) -> Option<SharedString> {
            Some("12".into())
        }
        fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    }

    fn open_vertical(
        cx: &mut TestAppContext,
        root: &std::path::Path,
        collapsed: bool,
        width: Option<&str>,
    ) -> WindowHandle<SubspaceApp> {
        cx.update(|cx| {
            crate::theme::preview_tests::init_app(cx, root, root.join("state.json"));
            crate::theme::initialize(cx);
            prefs::set(cx, super::NAVIGATION_KEY, "vertical");
            prefs::set(cx, COLLAPSED_KEY, if collapsed { "1" } else { "0" });
            if let Some(width) = width {
                prefs::set(cx, WIDTH_KEY, width);
            }
            cx.set_global(crate::registry::BoardFactories(vec![
                crate::registry::BoardRegistration::new::<WideBoard>(),
            ]));
        });
        cx.add_window(|window, cx| SubspaceApp::new(Vec::new(), window, cx))
    }

    fn draw(window: &WindowHandle<SubspaceApp>, cx: &mut TestAppContext) {
        cx.update_window((*window).into(), |_, window, cx| {
            window.draw(cx).clear(cx);
        })
        .expect("the window should draw");
    }

    fn live_width(window: &WindowHandle<SubspaceApp>, cx: &mut TestAppContext) -> f32 {
        window
            .update(cx, |app, _, cx| app.live_rail_width(cx).unwrap_or(0.))
            .unwrap()
    }
}
