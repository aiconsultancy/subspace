//! Drawing the log viewer: the shared surface header with Close, a flush
//! toolbar of shared controls and a monospace list of lines.
mod toolbar;

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, Global, Hsla, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, actions, div, px,
};
use subspace_core::log::{Level, Record};
use subspace_lucide::IconName;

use super::{LogView, MenuKind};
use crate::Tokens;
use crate::action::{Intent, action_button};
use crate::measure::text_width;
use crate::surface_header::SurfaceHeader;
use crate::tabs::{self, TabSpec};

actions!(subspace_log_view, [Close, ToggleFollow]);

struct Keys;
impl Global for Keys {}

const BAR_PADDING_PX: f32 = 6.;
const LINE_PADDING_PX: f32 = 2.;
/// One icon per level, in [`Level::ALL`] order.
const LEVEL_ICONS: [IconName; 5] = [
    IconName::CircleX,
    IconName::TriangleAlert,
    IconName::Info,
    IconName::Bug,
    IconName::Footprints,
];

pub(super) fn bind_keys(cx: &mut App) {
    if cx.has_global::<Keys>() {
        return;
    }
    cx.set_global(Keys);
    cx.bind_keys([
        KeyBinding::new("escape", Close, Some("LogView")),
        KeyBinding::new("escape", Close, Some("LogView > Input")),
        KeyBinding::new("cmd-shift-l", ToggleFollow, Some("LogView")),
    ]);
}

/// A menu's title and filter placeholder.
pub(super) fn menu_frame(kind: MenuKind) -> (&'static str, &'static str) {
    match kind {
        MenuKind::Target => ("Target", "Show only\u{2026}"),
        MenuKind::Capture => ("Capture level", "Keep records at\u{2026}"),
    }
}

/// A menu button's text: what it shows now, and that it opens a menu.
fn opens_menu(label: &str) -> String {
    format!("{label} \u{25be}")
}

/// The colour of a level's label.
pub(super) fn level_color(level: Level, tokens: &Tokens) -> Hsla {
    match level {
        Level::Error => tokens.red,
        Level::Warn => tokens.yellow,
        Level::Info => tokens.fg_strong,
        Level::Debug | Level::Trace => tokens.muted,
    }
}

/// A level's label in the line list.
fn level_label(level: Level) -> String {
    level.as_str().to_ascii_uppercase()
}

/// The level lane fits the widest label at the current monospace size, so
/// no label wraps and every message starts on the same spine.
fn level_lane(window: &Window, family: SharedString, size: Pixels) -> Pixels {
    Level::ALL
        .into_iter()
        .map(|level| text_width(window, &level_label(level), family.clone(), size))
        .max()
        .unwrap_or(Pixels::ZERO)
        .ceil()
}

impl Render for LogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let menu = self.menu_anchors.view();
        let mono = cx.theme().mono_font_family.clone();
        let lane = level_lane(window, mono.clone(), tokens.mono_font);
        v_flex()
            .id("log-view")
            .test_support()
            .key_context("LogView")
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .bg(tokens.bg)
            .text_color(tokens.fg)
            .text_size(tokens.row_font)
            .on_action(cx.listener(|this, _: &Close, window, cx| this.escape(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleFollow, _, cx| {
                this.set_follow(!this.follow, cx);
            }))
            .child(Self::header(cx))
            .child(self.toolbar(&tokens, cx))
            .child(self.lines(&tokens, mono, lane))
            .children(menu)
    }
}

impl LogView {
    fn header(cx: &mut Context<Self>) -> SurfaceHeader {
        SurfaceHeader::new(
            IconName::FileText,
            "Logs",
            "Close logs",
            cx.listener(|this, _, window, cx| this.close(window, cx)),
        )
    }

    fn toolbar(&self, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        toolbar::render(self, tokens, cx)
    }

    /// A toolbar button that opens one of the two popover menus.
    fn menu_button(
        &self,
        icon: IconName,
        label: String,
        kind: MenuKind,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let button = action_button(kind.id(), Intent::Neutral, Some(icon), label).on_click(
            cx.listener(move |this, _, window, cx| {
                this.open_menu(kind, window, cx);
            }),
        );
        self.menu_anchors.trigger(kind.id(), button, cx)
    }

    fn follow_control(&self, cx: &mut Context<Self>) -> impl IntoElement {
        action_button("log-follow", Intent::Quiet, Some(IconName::Eye), "Follow")
            .tooltip("Follow")
            .latched(self.follow)
            .on_click(cx.listener(|this, _, _, cx| this.set_follow(!this.follow, cx)))
    }

    fn level_tabs(&self, cx: &mut Context<Self>) -> crate::tabs::TabBar {
        tabs::tab_bar("log-levels")
            .selected_index(self.filter.level.index())
            .children(
                Level::ALL
                    .into_iter()
                    .map(|level| tabs::tab(TabSpec::new(level.as_str(), level_icon(level)))),
            )
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                if let Some(level) = Level::ALL.get(*index) {
                    this.set_level(*level, cx);
                }
            }))
    }

    fn lines(&self, tokens: &Tokens, mono: SharedString, lane: Pixels) -> impl IntoElement {
        let shown = self.shown();
        let count = format!("{} of {} records", shown.len(), self.records.len());
        v_flex()
            .id("log-lines")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .font_family(mono)
            .text_size(tokens.mono_font)
            .child(
                div()
                    .px(tokens.pad_x)
                    .py(px(LINE_PADDING_PX))
                    .text_color(tokens.muted)
                    .child(count),
            )
            .children(shown.into_iter().map(|record| line(record, tokens, lane)))
    }
}

fn level_icon(level: Level) -> IconName {
    LEVEL_ICONS[level.index()]
}

/// One record: its time and level in colour, then the rest of the line.
fn line(record: &Record, tokens: &Tokens, lane: Pixels) -> Div {
    h_flex()
        .gap_2()
        .items_start()
        .px(tokens.pad_x)
        .py(px(LINE_PADDING_PX))
        .border_b_1()
        .border_color(tokens.hair)
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.muted)
                .child(record.at.format("%H:%M:%S%.3f").to_string()),
        )
        .child(
            div()
                .id(("log-level", record.seq))
                .test_support()
                .flex_shrink_0()
                .w(lane)
                .whitespace_nowrap()
                .text_color(level_color(record.level, tokens))
                .child(level_label(record.level)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(tokens.accent)
                .child(record.target.clone()),
        )
        .child(record_body(record, tokens))
}

fn record_body(record: &Record, tokens: &Tokens) -> Div {
    let fields = record
        .fields
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("  ");
    v_flex()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .id(("log-message", record.seq))
                .test_support()
                .w_full()
                .child(record.message.clone()),
        )
        .child(
            div()
                .id(("log-fields", record.seq))
                .test_support()
                .w_full()
                .min_w_0()
                .text_color(tokens.muted)
                .child(fields),
        )
}
