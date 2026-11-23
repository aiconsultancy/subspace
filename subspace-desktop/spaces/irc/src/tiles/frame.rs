//! A tile's fixed box under a message, shared by issues and pull requests:
//! a lead (state mark and key), two lines (the title, then the facts) and
//! an optional trailing meter. Every state of a tile, loaded, loading or
//! unresolved, is the same width and the same height, so an answer never
//! changes how the tiles wrap or how tall the message is.

use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, h_flex, progress::Progress, v_flex,
};
use gpui_kit::{
    AnyElement, App, Div, FontWeight, IntoElement, ParentElement as _, SharedString, Styled as _,
    div, rems,
};
use subspace_framework::dashboard::density::REFERENCE_REM;
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::{Tokens, Tone};

/// Every tile's width at the reference text size; a narrow transcript caps
/// it at the row's width.
const WIDTH_REM: f32 = 340. / REFERENCE_REM;
/// The title's line.
const TITLE_LINE_REM: f32 = 17. / REFERENCE_REM;
/// The facts' line: tall enough for a Large agent icon (16 px).
const META_LINE_REM: f32 = 16. / REFERENCE_REM;
const LINES_GAP_REM: f32 = 2. / REFERENCE_REM;
/// The gap between the loading tile's two skeleton bars.
const SKELETON_GAP_REM: f32 = 6. / REFERENCE_REM;
/// The content's fixed height: both lines and the gap between them.
const CONTENT_REM: f32 = TITLE_LINE_REM + LINES_GAP_REM + META_LINE_REM;
/// A meter's bar.
const METER_REM: f32 = 44. / REFERENCE_REM;
/// The loading tile's skeleton bars, as the mock-up draws them.
const TITLE_SKELETON_REM: f32 = 190. / REFERENCE_REM;
const META_SKELETON_REM: f32 = 110. / REFERENCE_REM;
const SKELETON_BAR_REM: f32 = 6. / REFERENCE_REM;
/// The shared list skeleton's bar opacity (`dashboard::feedback::skeleton`).
const SKELETON_OPACITY: f32 = 0.6;
const PERCENT: f32 = 100.;

/// What leads a tile: its state mark, then its key in monospace.
pub(crate) struct Lead {
    pub mark: Option<AnyElement>,
    pub key: String,
}

/// A loaded tile's parts.
pub(crate) struct Slot {
    pub lead: Lead,
    pub title: SharedString,
    /// The second line: state, people, dates, sizes.
    pub meta: AnyElement,
    /// A meter at the trailing end, such as progress or checks.
    pub trailing: Option<AnyElement>,
}

/// A loaded tile's content.
pub(crate) fn tile(slot: Slot, tokens: &Tokens, cx: &App) -> Div {
    body()
        .child(lead(slot.lead, tokens, cx))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(rems(LINES_GAP_REM))
                .child(title_line(slot.title, tokens))
                .child(meta_line(tokens).child(slot.meta)),
        )
        .children(slot.trailing)
}

/// A tile whose answer has not come: its known key and static skeleton
/// bars where the title and facts go. Nothing moves, so reduced motion
/// needs no case of its own.
pub(crate) fn loading(key: String, tokens: &Tokens, cx: &App) -> Div {
    let mark = skeleton(tokens)
        .size(tokens.caption_size())
        .into_any_element();
    let leading = Lead {
        mark: Some(mark),
        key,
    };
    body().child(lead(leading, tokens, cx)).child(
        v_flex()
            .flex_1()
            .min_w_0()
            .gap(rems(SKELETON_GAP_REM))
            .child(
                skeleton(tokens)
                    .h(rems(SKELETON_BAR_REM))
                    .w(rems(TITLE_SKELETON_REM))
                    .max_w_full(),
            )
            .child(
                skeleton(tokens)
                    .h(rems(SKELETON_BAR_REM))
                    .w(rems(META_SKELETON_REM))
                    .max_w_full(),
            ),
    )
}

/// A tile that will not load: a muted glyph, its key and the short reason,
/// on one line in the same box.
pub(crate) fn unresolved(key: String, reason: &'static str, tokens: &Tokens, cx: &App) -> Div {
    let mark = Icon::from(IconName::CircleSlash)
        .size(tokens.secondary_size())
        .into_any_element();
    let leading = Lead {
        mark: Some(mark),
        key,
    };
    body()
        .text_color(tokens.muted)
        .child(lead(leading, tokens, cx))
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(tokens.secondary_size())
                .child(reason),
        )
}

/// The box one tile takes in the row, whatever its state.
pub(crate) fn slot(content: impl IntoElement) -> Div {
    div()
        .flex_shrink_0()
        .min_w_0()
        .w(rems(WIDTH_REM))
        .max_w_full()
        .child(content)
}

/// A meter: a small bar of `done` out of `total` in `tone`, then `words`,
/// named `spoken`.
pub(crate) fn meter(
    id: SharedString,
    (done, total): (u32, u32),
    (words, spoken): (String, String),
    tone: Tone,
    tokens: &Tokens,
) -> AnyElement {
    h_flex()
        .flex_shrink_0()
        .gap_1p5()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
        .child(
            Progress::new(id)
                .xsmall()
                .w(rems(METER_REM))
                .bg(tokens.line)
                .color(tokens.tone(tone))
                .value(share(done, total))
                .accessibility_label(spoken),
        )
        .child(words)
        .into_any_element()
}

/// `done` of `total` as a percentage; nothing of nothing is empty.
pub(crate) fn share(done: u32, total: u32) -> f32 {
    done.min(total) as f32 / total.max(1) as f32 * PERCENT
}

/// A key in monospace, as the inline reference writes it.
fn key_text(key: String, tokens: &Tokens, cx: &App) -> Div {
    div()
        .flex_shrink_0()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(tokens.caption_size())
        .child(key)
}

/// The mark and the key, in the muted colour the mock-up gives keys.
fn lead(lead: Lead, tokens: &Tokens, cx: &App) -> Div {
    h_flex()
        .flex_shrink_0()
        .gap_1p5()
        .text_color(tokens.muted)
        .children(lead.mark)
        .child(key_text(lead.key, tokens, cx))
}

fn body() -> Div {
    h_flex().w_full().min_w_0().h(rems(CONTENT_REM)).gap_2p5()
}

fn title_line(title: SharedString, tokens: &Tokens) -> Div {
    div()
        .h(rems(TITLE_LINE_REM))
        .min_w_0()
        .flex()
        .items_center()
        .text_size(tokens.secondary_size())
        .font_weight(FontWeight::MEDIUM)
        .text_color(tokens.fg_strong)
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(title),
        )
}

fn meta_line(tokens: &Tokens) -> Div {
    h_flex()
        .h(rems(META_LINE_REM))
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_size(tokens.caption_size())
        .text_color(tokens.muted)
}

fn skeleton(tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .rounded_full()
        .bg(tokens.line.opacity(SKELETON_OPACITY))
}
