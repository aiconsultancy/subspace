//! The separate keyboard shortcut reference.

use gpui_kit::component::input::InputState;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Bounds, Context, Entity, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, WeakEntity, Window, div, point,
};
use subspace_dashboard::anchored::Anchor;
use subspace_dashboard::fields::{self, InputRole};
use subspace_dashboard::{
    Tokens,
    action::{Intent, icon_button},
};

use super::{SubspaceApp, ToggleShortcuts};

// Preserve the pre-#289 panel and key-column widths at the default 14px base
// font, while letting both follow interface zoom and fit a narrower window.
const PANEL_WIDTH_REMS: f32 = 560. / 14.;
const KEY_COLUMN_WIDTH_REMS: f32 = 120. / 14.;
const MAX_WIDTH_SHARE: f32 = 0.95;
const MAX_HEIGHT_SHARE: f32 = 0.85;
const TOP_SHARE: f32 = 0.075;
const CENTER_SHARE: f32 = 0.5;

struct Shortcuts {
    input: Entity<InputState>,
    anchor: WeakEntity<Anchor>,
    keys: Vec<(SharedString, SharedString)>,
    foot: Option<SharedString>,
    _subscription: gpui_kit::Subscription,
}

#[cfg(test)]
#[path = "shortcuts_tests.rs"]
mod tests;

impl Focusable for Shortcuts {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Render for Shortcuts {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let query = self.input.read(cx).value();
        let rows = matching_keys(&self.keys, &query);
        let width = panel_width(window);
        v_flex()
            .id("keyboard-shortcuts")
            .test_support()
            .role(gpui_kit::Role::Dialog)
            .aria_label("Keyboard Shortcuts")
            .w(width)
            .max_h(window.viewport_size().height * MAX_HEIGHT_SHARE)
            .overflow_y_scroll()
            .p_4()
            .bg(tokens.surface)
            .border_1()
            .border_color(tokens.line)
            .rounded(tokens.surface_radius)
            .text_size(tokens.row_font)
            .child(header(self.anchor.clone(), &tokens))
            .child(div().mb_2().child(fields::input(
                &self.input,
                InputRole::Search,
                "Filter shortcuts",
            )))
            .children(
                rows.iter()
                    .map(|(key, meaning)| row(key.clone(), meaning.clone(), &tokens)),
            )
            .when(rows.is_empty(), |view| {
                view.child(
                    div()
                        .id("shortcuts-empty")
                        .test_support()
                        .py_1()
                        .text_color(tokens.muted)
                        .child("No keys match."),
                )
            })
            .when_some(self.foot.clone(), |view, foot| {
                view.child(div().mt_2().text_color(tokens.muted).child(foot))
            })
    }
}

fn header(anchor: WeakEntity<Anchor>, tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .mb_2()
        .child(
            div()
                .flex_1()
                .text_color(tokens.fg_strong)
                .child("Keyboard Shortcuts"),
        )
        .child(
            icon_button(
                "shortcuts-close",
                Intent::Quiet,
                subspace_lucide::IconName::X,
                "Close Keyboard Shortcuts",
            )
            .on_click(move |_, window, cx| {
                let _ = anchor.update(cx, |anchor, cx| anchor.dismiss(window, cx));
            }),
        )
}

/// Match every word against the key and description, without case sensitivity.
pub(in crate::app) fn matching_keys(
    keys: &[(SharedString, SharedString)],
    query: &str,
) -> Vec<(SharedString, SharedString)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    keys.iter()
        .filter(|(key, meaning)| {
            let text = format!("{key} {meaning}").to_lowercase();
            words.iter().all(|word| text.contains(word.as_str()))
        })
        .cloned()
        .collect()
}

fn row(key: SharedString, meaning: SharedString, tokens: &Tokens) -> impl IntoElement {
    h_flex()
        .id(SharedString::from(format!("shortcuts-key-{key}-{meaning}")))
        .test_support()
        .aria_label(meaning.clone())
        .items_start()
        .py_1()
        .border_b_1()
        .border_color(tokens.hair)
        .child(
            div()
                .w(gpui_kit::rems(KEY_COLUMN_WIDTH_REMS))
                .flex_shrink_0()
                .text_color(tokens.accent)
                .child(key),
        )
        .child(div().flex_1().min_w_0().whitespace_normal().child(meaning))
}

fn panel_width(window: &Window) -> gpui_kit::Pixels {
    (window.rem_size() * PANEL_WIDTH_REMS).min(window.viewport_size().width * MAX_WIDTH_SHARE)
}

impl SubspaceApp {
    pub(super) fn dismiss_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shortcuts_anchor
            .update(cx, |anchor, cx| anchor.dismiss(window, cx));
    }

    pub(super) fn toggle_shortcuts(
        &mut self,
        _: &ToggleShortcuts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shortcuts_anchor.read(cx).is_open() {
            self.shortcuts_anchor
                .update(cx, |anchor, cx| anchor.dismiss(window, cx));
            return;
        }
        if self.settings.is_some() {
            return;
        }
        if self.session.is_some() {
            self.close_palette(window, cx);
        }
        self.shortcuts_origin = window.focused(cx);
        let keys = self.help_keys(cx);
        let foot = self.dashboard().and_then(|board| board.help_foot(cx));
        let anchor = self.shortcuts_anchor.downgrade();
        let body = cx.new(|cx| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter shortcuts"));
            let subscription = cx.observe(&input, |_, _, cx| cx.notify());
            Shortcuts {
                input,
                anchor,
                keys,
                foot,
                _subscription: subscription,
            }
        });
        self.shortcuts_anchor
            .update(cx, |anchor, cx| anchor.show(body, window, cx));
        cx.notify();
    }

    pub(super) fn position_shortcuts(&self, window: &Window, cx: &mut Context<Self>) {
        if !self.shortcuts_anchor.read(cx).is_open() {
            return;
        }
        let viewport = window.viewport_size();
        let width = panel_width(window);
        // Keyboard and palette invocations have no visible trigger. Give the
        // shared host a centered virtual anchor; it still owns placement and
        // dismissal rather than adding another popup or positioning system.
        let origin = point(
            (viewport.width - width) * CENTER_SHARE,
            viewport.height * TOP_SHARE,
        );
        self.shortcuts_anchor.update(cx, |anchor, cx| {
            anchor.set_bounds(Bounds::new(origin, Default::default()), cx)
        });
    }
}
